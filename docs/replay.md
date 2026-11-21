# Replays: the game recorder

Every game is recorded, AI-only and human alike, so a learning AI can be trained from them later.
Nothing is sampled: all games are kept. Human games carry more weight. A replay holds a game's
*inputs* plus enough checkpoints to prove a re-simulation matches. It does not hold the game's
state. The sim is deterministic (docs/engineering.md §3), so the inputs are the game.

**Recordings, manifests, config snapshots and training sets never go in this repository.** Only
the format and the tools live here. `.gitignore` refuses `*.otr`, `recordings/` and `*.parquet`
outside test fixtures. Trained models may be published later; training data may not.

## What a replay holds

A replay is a **header**, a **command log**, a **checkpoint at every midnight** and an
**outcome**.

| Part | Contents |
|---|---|
| Header | game id, run id, sim build git commit, **rules fingerprint**, config hash, seed, preset, start and end year, seats, AI phase order |
| Seat | `human` or `ai`; brain id (`core:<personality>`, `harness:<brain>`, `client`); brain version (the personality's settings hash, or the harness brain version); an anonymous player id for humans, never a name |
| Command log | every command the world applied: `(tick, seat, origin, accepted, command)` |
| Checkpoint | day, tick, `World::state_hash()`, and per seat: net worth, cash, leases held, businesses owned, bankrupt |
| Outcome | winner, per seat rank / final net worth / bankrupt, days played, ticks, final state hash |

**Commands** are recorded at the single place they enter the game: phase 2 of the tick
(`World::tick_with`, which reports each applied command and whether the world accepted it).
The log records what was *submitted*, accepted or refused (a refused command changed nothing;
a bankrupt player's commands are refused, R4). `origin` says where a command came from:

- `external`: submitted to the world by a client or a harness brain. A replay re-submits these.
- `core`: queued by sim-core's own AI phase. A replay lets the AI regenerate these and checks
  they match the log, so the log is complete training data *and* a check on the AI.

The command type is `sim_core::world::Command`. The `protocol` crate is a placeholder until M2;
when it gets real wire commands, they should map one-to-one onto it.

### Rules fingerprint

The **rules fingerprint** is the final state hash (16 hex digits) of a fixed **reference game**
as this build plays it: seed 42, the `full` preset, 2 seats, the *shipped* config compiled into
the binary. It is exactly what `sim-cli run config --seed 42 --ticks 207360` prints as `hash=`.
Any change to the rules moves it. Builds with the same fingerprint re-simulate each other's
replays; `replay --verify` refuses a replay from another fingerprint and names the commit that
recorded it. It is computed once per process (about 0.3 s in a release build) and printed by
`sim-cli rules-fingerprint`.

### Config snapshots

The config a game ran with is the shipped (or given) config directory with the preset and any
overrides applied. Its **snapshot** is one YAML document mapping each file name to that file's
text. The **config hash** is the stable hash (FNV-1a 64, `sim_core::hash::stable_hash`) of the
sorted `(file name, text)` pairs, as 16 hex digits. Each distinct config is stored once, at
`configs/<config_hash>.yaml`, never inside a replay. A reader re-hashes the snapshot and
refuses one whose content does not match its name.

## Byte layout

A replay is one **envelope**: a 16-byte header followed by the payload. All integers in the
envelope header are little-endian.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `OTRP` (`4f 54 52 50`) |
| 4 | 2 | format version, `u16` (currently `1`) |
| 6 | 2 | flags, `u16`: bit 0 = payload is a zstd frame (always set in version 1); other bits 0 |
| 8 | 4 | payload length in bytes, `u32` |
| 12 | 4 | CRC-32 (IEEE) of the payload, `u32` |
| 16 | n | payload |

The payload is a **zstd** frame (level 9) of the **postcard 1.x** encoding of:

```text
Wire {
  header:      Header { game_id: String, run_id: String, git_commit: String,
                        rules_fingerprint: String, config_hash: String, seed: u64,
                        preset: String, start_year: i32, end_year: i32,
                        seats: Vec<Seat { kind: SeatKind{Human=0, Ai=1}, brain: String,
                                          brain_version: String, player_id: Option<String> }>,
                        ai_order: Vec<u8> },
  commands:    Vec<{ dtick: u64, seat: u8, origin: Origin{External=0, Core=1},
                     accepted: bool, command: sim_core::world::Command }>,
  checkpoints: Vec<Checkpoint { day: u32, tick: u64, state_hash: u64,
                                seats: Vec<{ net_worth: i64, cash: i64, leases: u32,
                                             businesses: u32, bankrupt: bool }> }>,
  outcome:     Outcome { winner: Winner{Seat(u8)=0, Tie=1, Nobody=2},
                         seats: Vec<{ rank: u8, net_worth: i64, bankrupt: bool }>,
                         days_played: u32, ticks: u64, final_state_hash: u64 },
}
```

postcard writes fields in declaration order with no names. Integers are varints (signed ones
zig-zag); strings and vectors are length-prefixed; `Option` is a 0/1 tag; enums are a varint
variant index followed by the variant's fields. Money is integer pence (`i64`), rates basis
points. `dtick` is the tick minus the previous command's tick (the first is from 0), so a busy
log costs about a byte per tick. `ai_order` lists the seats sim-core's AI plays, in the order
its phase runs them, which `World::seated` needs to rebuild the game.

Envelopes are self-delimiting, so a **segment** file is just envelopes back to back. A single
game is a segment of one.

### Versioning

- The envelope's format version covers the payload layout, *including* the order and fields of
  `Command`, which postcard encodes by variant index.
- **Bump `FORMAT_VERSION`** for any change that alters existing bytes: reordering, removing or
  retyping a field or a `Command` variant, changing compression or the delta encoding. A reader
  refuses any version it does not know by name; it never guesses.
- Appending a new `Command` variant at the end does not change existing bytes. Old replays
  still decode. A replay holding the new variant fails cleanly on an older reader. It is still
  a format change, so bump the version when it lands.
- The format version and the rules fingerprint are independent. The version says whether a
  build can *read* a replay; the fingerprint says whether it can *re-simulate* it. A replay
  recorded under old rules stays readable for training (the extractor reads it) after the
  rules move on.

### Size

Measured on the shipped config, scripted brain in the free seat, mean of 20 games:

| Game | Commands per game | Midnights | Bytes per game |
|---|---|---|---|
| `v1` preset (1930-1950), 2 seats (AI only) | 151 | 4 | 682 |
| `v1`, 6 seats (5 AI + scripted) | 535 | 4 | 1,199 |
| `full` preset (1930-2050), 2 seats | 719 | 24 | 2,537 |
| `full`, 6 seats | 3,471 | 24 | 5,547 |

A config snapshot is about 100 KB, stored once per distinct config (once per study model).
zstd shrinks postcard's output 3-6x. That puts a 600,000-game study at a few GB.

## Storage layout

One layout, in every backend:

```text
replays/<rules_fingerprint>/<run_id>/<part>-s<NNN>.otr   replay segments
configs/<config_hash>.yaml                              config snapshots
manifests/<run_id>/<part>.parquet                       manifests
```

A **part** is the unit a run writes at once: `part-00000`, `part-00001`, ... for a batch
(4,096 games each), `part-m<model id>` for a study (one per model, so a resumed study adds
parts without touching finished ones). Run ids default to a hash of the run's inputs, so
rerunning the same batch or study overwrites its own objects with identical bytes. `--run-id`
names a run explicitly; use `test-*` for throwaway runs.

**Why segments and not one object per game.** A replay is about 1-5 KB, so a 600,000-game study
written as one object per game would be 600,000 tiny PUTs. Request overhead would dominate, and
listing would be slow. Instead, each part's games are packed in game order into segments of
about 8 MiB (`--segment-mib`). The manifest records each game's segment, byte offset and
length, so reading one game is one ranged GET and reading a run is a few large ones. Packing
happens after a part's games finish, in game order, so the objects are byte-identical whatever
the thread count.

Backends:

- **S3** (MinIO or any S3-compatible store), `--record s3` / `--store s3`. Configured only by
  the environment; there are no defaults and no credentials in the repository:
  `OPENTYCOON_S3_ENDPOINT` (`http(s)://host[:port]`), `OPENTYCOON_S3_BUCKET`,
  `OPENTYCOON_S3_ACCESS_KEY`, `OPENTYCOON_S3_SECRET_KEY`, and optionally `OPENTYCOON_S3_REGION`
  (default `us-east-1`). Requests are SigV4-presigned, path-style, sent with a blocking HTTP
  client.
- **Local directory**, `--record <dir>` / `--store <dir>`: the same layout under a directory.
  Tests use it, and it serves as a local cache.

## Manifest schema

One Parquet file per part, one row per game, zstd-compressed. Unsigned 64-bit values (seed,
ticks, offsets) are stored in `INT64` columns bit for bit: read them back as `u64`. Seat
columns repeat for `<N>` = 0 to 7 and are null for seats a game does not have. A game with more
than 8 seats is refused.

| Column | Type | Meaning |
|---|---|---|
| `game_id` | string | unique within the run: `seed<seed>` (batch), `m<model>-g<index>` (study) |
| `run_id` | string | the run |
| `rules_fingerprint` | string | rules the game was recorded under |
| `git_commit` | string | sim build commit (`unknown` outside git) |
| `config_hash` | string | config snapshot: `configs/<config_hash>.yaml` |
| `seed` | int64 (u64) | game seed |
| `preset` | string | length preset |
| `start_year` | int64 | first year |
| `end_year` | int64 | last year |
| `format_version` | int64 | `.otr` format version |
| `source` | string | what recorded it: `batch`, `study`, `record` |
| `model_id` | int64, nullable | study model id |
| `n_seats` | int64 | seats in the game |
| `human_seats` | int64 | seats played by humans |
| `weight` | double | training weight: 1.0, or the human multiplier (`--human-weight`, default 5.0) if any seat is human |
| `winner` | string | `p<seat>`, `tie` or `none` |
| `days_played` | int64 | midnights passed |
| `ticks` | int64 (u64) | ticks run |
| `final_state_hash` | string | final `World::state_hash()`, 16 hex digits |
| `bankruptcies` | int64 | seats that went bankrupt |
| `commands` | int64 (u64) | commands in the log |
| `object_key` | string | the segment holding the replay |
| `offset` | int64 (u64) | byte offset of its envelope in the segment |
| `size_bytes` | int64 (u64) | envelope length |
| `seat<N>_kind` | string | `human` or `ai` |
| `seat<N>_brain` | string | brain id |
| `seat<N>_brain_version` | string | brain version or settings hash |
| `seat<N>_player_id` | string, nullable | anonymous player id (humans only) |
| `seat<N>_rank` | int64 | final rank (1 = best; bankrupt seats after solvent ones) |
| `seat<N>_net_worth` | int64 | final net worth, pence |
| `seat<N>_bankrupt` | boolean | went bankrupt |
| `seat<N>_weight` | double, nullable | reserved for per-player weights; null for now |

## Training weights

- **Game weight** (in the manifest): AI-only games weigh 1.0. A game with any human seat weighs
  the human multiplier (`--human-weight`, default 5.0). Per-player weights are reserved
  (`seat<N>_weight`).
- **Rules weight** (applied at extract time, never baked into a manifest): 1.0 for the current
  rules fingerprint, `decay^(i+1)` for the `i`-th fingerprint in `--rules-history` (newest
  first), `decay` for any other fingerprint, and never below `--rules-floor`. Defaults: decay
  0.5, floor 0. The extract's `sample_weight` is the game weight times the rules weight.

## Commands

```sh
# Record (any of these; dest is `s3` or a directory)
sim-cli record config --dest recordings --games 1000 --preset v1
sim-cli batch config --games 1000 --record recordings
sim-cli study config --models 100 --games 500 --record s3

# Re-simulate and compare every midnight. Exit 1 on any mismatch (naming the first diverging
# midnight), 2 if the games were recorded under another rules fingerprint.
sim-cli replay --verify recordings/manifests/<run_id>/part-00000.parquet
sim-cli replay --verify manifests/<run_id>/part-m00000.parquet --store s3 [--game <id>]

# Stub extractor: one row per command, with lineage and weights
sim-cli extract recordings/manifests/<run_id>/part-00000.parquet --out commands.parquet

sim-cli rules-fingerprint
```

`extract` writes one row per recorded command: `extractor_id` (`cmdlog-stub`),
`extractor_version`, lineage (`game_id`, `run_id`, `rules_fingerprint`, `git_commit`,
`config_hash`, `format_version`, `source_object`, `seed`), the decision (`decision_tick`: the
tick the command was decided on, one before `tick`, the tick it applied; `seat`, `seat_kind`,
`brain`, `origin`, `accepted`, `command_kind`, `command` as JSON), the weights (`game_weight`,
`rules_weight`, `sample_weight`) and `index` in the log. The real feature extractor replaces it.

## Guarantees and their tests

- **Recording never changes a game.** The recorder only reads the world around
  `World::tick_with`. Recorded and unrecorded batches give the same state hashes
  (`features/replay.feature`; sim-core's `tick_with_runs_exactly_the_same_tick_as_tick`).
- **A recorded game re-verifies** at every midnight, and a tampered checkpoint or command fails
  naming the first diverging midnight.
- **A replay from other rules is refused** with both fingerprints and the recording commit.
- Encoding round-trips, segments decode with their offsets, and checkpoints are monotonic in
  time (property tests in `replay::format`, `replay::manifest`, `replay::record`).
- Every store backend passes the same conformance test. The S3 backend runs it against a local
  S3 mock. `OPENTYCOON_S3_IT=1` with the `OPENTYCOON_S3_*` variables also runs it against a real
  bucket, under `runs/test-it-<pid>/`, and cleans up.
