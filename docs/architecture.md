# Architecture

How OpenTycoon v2 is put together, and where each piece of work belongs. The spec is
[`spec.md`](spec.md) (§10a–§10c for architecture); rulings are cited as R1–R61.

## Crates

```
crates/
  sim-core/    the game: deterministic, headless, no I/O        (library)
  protocol/    wire messages shared by server and client         (library; v1, docs/inspector-protocol.md)
  server/      authoritative server; single-player on 127.0.0.1  (library + binary `opentycoon-server`)
  sim-cli/     headless harness: validate config, run, batch, bench, study, evolve, record (binary `sim-cli`; docs/perf.md, docs/study.md, docs/evolve.md, docs/replay.md)
  client-ext/  godot-rust (gdext) extension for the Godot client  (cdylib)
client/        Godot 4.7 project: GDScript scenes, cameras, UI, GUT tests
config/        the nine YAML files (§10b), server.yaml + generated JSON Schemas
```

Dependency direction is one way: `server`, `sim-cli` → `sim-core`; `server`, `client-ext` →
`protocol`; `client-ext` → `server` (single player runs the server in-process, R44). Nothing else
depends on `server`, and nothing depends on `sim-cli`. `sim-core` depends on no other workspace
crate and never on the client.

Single-player is multiplayer against a server on 127.0.0.1 (R44). The server runs the
simulation; clients only send commands and display state (R40).

## sim-core modules and owners

One module per domain. Each module has **one owner at a time**, who may change its files freely;
changes to another module's public API go through that owner (or a separate PR). The table is
the split for M1 parallel work.

| Module | Files | Owns | Main rulings | Status at M0 |
|---|---|---|---|---|
| `config` | `src/config/` | YAML → schema → typed config; cross-file rules | R17, R45, R58 | **real** |
| `money` | `src/money.rs` | `Money` (i64 pence), `Rate` (basis points), rounding | §3 | **real** |
| `clock` | `src/clock.rs` | ticks, days, the +5-year step, trading windows | R1, R30, R34 | **real** |
| `economy` | `src/economy/` | ledger, settlement, overdraft, solvency, bankruptcy, net worth, sell-back | R3, R4, R5 (sell-back), R7, R19, R31, R32, R39 | **real** (M1): 06:00 settlement and midnight reckoning wired |
| `land` | `src/land/` | map, boards, roads and the river, blocks, sets, desirability shifts, tube stations, footprints, leases, land rent | R9, R10, R14, R16, R22, R23, R26, R33 | **real**: v1 and full boards |
| `auctions` | `src/auctions.rs` | the one-at-a-time auction state machine | R5, R8, R23 | types only |
| `businesses` | `src/businesses.rs` | buildings, types, prices, orders, stock, write-off | R2, R10, R13, R35, R41, R58 | types + cost formulas |
| `utilities` | `src/utilities.rs` | utility bills, holders, auctions, nationalisation and privatisation | R11, R12, R24 | **real**: auctioned like land; elections move ownership |
| `population` | `src/population.rs` | cohort model, births, ageing, arrivals | R6, R53, R60, R61 | types only |
| `customers` | `src/customers/` | shoppers, shopping lists, shopping windows (`windows.rs`, `errands.rs`), shop scoring, movement and street routing (`streets.rs`), tourists' nights (wandering, looking for a bed), online demand (`online.rs`) | R15, R25, R38, R42, R55, R59, R69, R70, §8.3 | **real**: hotels and online from wave A |
| `hotels` | `src/hotels.rs` | the Hotel unlock (full set) and one per set, rooms by volume, room offers and check-in | R14, R29, R30 | **real** (wave A) |
| `ai` | `src/ai/` | AI personalities behind the `Brain` trait, each seat on its own RNG stream | §10, R66, R67 | **real**: the Grocer, the Landlord, the Toff, the Speculator and Sir Penny Sugar (`ai.yaml`, one block each; shared `ai::kit`, including the line-yield reading behind opportunism, R66, and rent by vacancy, R67) |
| `events` | `src/events/` | history, random events, elections, insurance, front page, the modifier stack | R12, R16 | **real**: every §9 event as a typed modifier; world side in `src/world/events_phase.rs` |
| `world` | `src/world.rs` | owns all state; runs the tick order; `Command`s; state hash | R40 | **real** (phases are stubs) |
| support | `src/rng.rs`, `src/hash.rs` | seeded RNG helpers; portable state hash | — | **real** |

Shared domain enums live with their domain and are reused by config: `land::Tier`,
`land::BlockKind`, `land::ColourBand`, `population::{Class, Sex}`, `businesses::Hours`.

### What each owner touches

A module owner normally edits only:

- `crates/sim-core/src/<module>.rs` (or `src/<module>/`),
- `crates/sim-core/src/config/<file>.rs` for the config file(s) they own, plus their rules in
  `config/validate.rs` (one `check_<file>` function each),
- `config/<file>.yaml`, then `cargo schema` to regenerate `config/schema/<file>.schema.json`,
- `crates/sim-core/features/<module>/*.feature` and `crates/sim-core/tests/steps/<module>.rs`,
- one line in `tests/steps/mod.rs` and one field in `SimWorld` (`tests/bdd.rs`),
- their phase method in `world.rs` (see below),
- their lines in `docs/traceability-pending.txt`.

Expected merge points (small, append-only): `tests/steps/mod.rs`, `tests/bdd.rs` `SimWorld`,
`world.rs` (each phase is its own method), `docs/traceability-pending.txt`.

| Config file | Owner module |
|---|---|
| `game.yaml` | clock (clock, presets), economy (bankruptcy strikes and solvency, players), land (world) |
| `map.yaml` | land |
| `auctions.yaml` | auctions |
| `utilities.yaml` | utilities |
| `businesses.yaml` | businesses |
| `customers.yaml` | population (cohorts, births, demographics, which type holds flats), customers (allowance shape, desires, windows, scoring) |
| `events.yaml` | events |
| `ai.yaml` | ai |
| `economy.yaml` | every economy weight (R71), read by the module that applies it: land (land rate, lease term), economy (net worth, starting cash, overdraft, sell-back), businesses (build rate), utilities (bill rates), population (births multiplier, migration, allowance by class, the residential market), customers (price sensitivity, demand by decade) |

## The tick

One tick is 10 game-seconds (`clock.tick_game_seconds`), so a game day is 8,640 ticks. At the
default 20 real minutes per day the server runs 7.2 ticks per real second (the spec's "≈6" was
approximate). `World::tick()` runs these phases **in this order, always**:

| # | Phase | When | Owner | Does |
|---|---|---|---|---|
| 1 | Clock | every tick | clock | advance; report `ClockEvents { midnight, arrivals, settlement, game_over }` |
| 2 | Commands | every tick | world → each module | apply queued player and AI commands in submission order |
| 3 | Midnight reckoning | 00:00 | economy (+ land, businesses, events, population) | net worth (R31, before write-off) → stock write-off (R1) → solvency/bankruptcy (R3, R4) → lease expiry (R9) → credit-limit step (R7) → events and front page: election, history, rationing caps on the locked orders (§9, §9.1) → population year step (R61) |
| 4 | Arrivals | 05:30 | population | new population arrives and house-hunts (R53) |
| 5 | Settlement | 06:00 | economy (bills from land, utilities, businesses, population, events) | residents' rent from the shoppers' purse to each block's owner (R51, R63); land rent, utilities, insurance premiums, stock paid, may pass the limit (R32, R56); tomorrow's order delivered (R2); random building events rolled (§9); population joins the map |
| 6 | Auctions | every tick | auctions | countdowns, turn timers, closing |
| 7 | AI | every tick | ai | read the world, queue commands for the next tick |
| 8 | Customers | every tick | customers | spawn, buy online (from the Internet's arrival), choose shops, walk, buy; tourists wander, then take a Hotel room or the train (23:30–03:00); the world pays sales and room nights from the shoppers' purse |
| 9 | Businesses | every tick | businesses | record sales, reputation, lost sales |

`World::tick_observed` runs the same tick and calls an observer after each phase that ran; the
harness uses it for per-phase timing (`sim-cli bench --phases`, docs/perf.md). The observer
cannot touch the world, and `tick` passes a no-op.

Rules that keep the order meaningful:

- **The year has already stepped when phase 3 runs**: `clock.year()` is the *new* year. Anything
  that must score the old day (net worth) reads state, not the year.
- **Commands are applied once, at phase 2.** Phases never mutate state on behalf of a command
  later in the tick. AI commands queued at phase 7 apply at phase 2 of the next tick, exactly like
  a human's, so AI and human play share one path.
- **Money moves only through `economy::Ledger`.** Modules compute what is owed and hand
  `economy::Bill`s to settlement; nothing writes balances directly. The ledger's total is
  conserved (proptest).
- **Event modifiers.** Lasting event effects (demand, supply caps, power cuts, price noise,
  footfall) live on one stack, `events::Modifiers`; modules query it rather than special-case
  an event. Customers read it through `PhaseInputs::modifiers`.
- **One RNG.** `World` owns the only `SimRng`; phases draw from it in phase order. AI seats
  and the events module each draw from their own seeded stream instead, so they never shift
  the world's draws. Adding a draw changes downstream randomness, which is fine; drawing in a
  data-dependent order (e.g. iterating a `HashMap`) is not, and is banned by clippy.

## Module interfaces

The conventions every module follows, so phases compose without knowing each other's internals:

- **State** is a plain struct (`Businesses`, `AuctionHouse`, `Population`, ...) owned by
  `World`, deriving `Clone, Debug, Hash, Serialize, Deserialize`. `Hash` feeds the state hash;
  `Serialize` feeds saves (M2).
- **Behaviour** is free functions or methods taking explicit inputs:
  `fn phase(state: &mut X, other: &Y, rules: &XConfig, rng: &mut SimRng) -> Output`. No globals,
  no `World` back-references (the AI is the exception: it reads `&World`).
- **Config** arrives as the module's typed config struct, already validated. Modules never parse
  YAML or read files.
- **Time** arrives as the `Clock` or plain values (`year`, `second_of_day`). Never the wall clock.
- **Cross-module effects** are returned as data (`Bill`, a list of sales, a list of headlines) and
  applied by `world.rs`, not by reaching into another module's state.

## Streets and walking

Walkers move on a cell grid: each block is a `lot_w × lot_d` lot with a one-cell road ring
around it (`customers::movement`). `customers::streets::Streets` is built once per board, from
the map and `game.yaml` `world.lot_size`, and held by `World` (not saved or hashed: it is
derived from the map and config, which are). It is handed to the customers phase as
`PhaseInputs::streets`.

- **Where a walker may stand:** a road cell that is not water. The river takes the whole road
  row south of `map.yaml` `river.south_of_row`, except its bridges. The bridge for column `c`
  carries the north–south road on that column's west side (`x = c × pitch_x`) over the river.
- **Doors are on the street.** A building's door is its frontage: the street cell outside the
  middle of the first lot edge it touches (north, west, east, south) that is not water, or the
  nearest side street for a building that only touches a riverside edge. A tube station's
  entrance is the frontage of the whole lot. Walkers never step inside a lot.
- **Routes:** junction-to-junction shortest distances and first hops come from Floyd–Warshall
  over the `(W + 1) × (H + 1)` road junctions (100 on the full board; 40 KB of distances, 10 KB
  of hops), once per board. A route between two street cells is either straight along one road
  or out to an end of the start's segment, junction to junction, and in from an end of the
  target's segment, so a route length is at most four table reads and a tick's step a few more.
  Integers only; equal routes are broken north, west, east, south, and the near end of a
  segment before the far end.
- **Shop distance** for `distance_decay` (§8.3), shops and Hotels alike, is the route length
  in blocks (cells over the pitch), not the Manhattan distance, so the river and the blocks in
  the way count.
- **Between windows (R70)** a shopper walks to their away spot (`Activity::GoingAway`): home
  for a resident, the station they arrived at for a visitor. Only there do they leave the map
  (`Activity::Away`), and they step back out there when the window opens. On the map a
  shopper's position never changes by more than `cells_per_tick` cells from one tick to the
  next. Despawning at the 17:00 close (§8.4) is unchanged.

## Config

Nine YAML files (§10b). Pipeline: parse YAML → validate against the JSON Schema generated from
the Rust types (`schemars`) → deserialise into typed structs → cross-file rules
(`config/validate.rs`). Every problem is collected and reported with file and JSON pointer; any
problem fails the load. The committed `config/schema/*.schema.json` must match the types
(`tests/schemas.rs`); regenerate with `cargo schema`. YAML files carry a
`yaml-language-server` header so editors validate as you type.

Money is written as decimal pounds and converted exactly to pence; ratios are decimals converted
to basis points; times are `"HH:MM"`. A value that would lose precision fails the load.

`config/server.yaml` sits beside them but is not sim config: it is one of sim-core's
`COMPANION_FILES`, which `ConfigSources::read_dir` leaves alone, and the `server` crate loads it
through the same pipeline (YAML → schema from its Rust types → typed struct → cross-field rules,
errors naming file and pointer). Its schema, `config/schema/server.schema.json`, is regenerated
with `cargo run -p server -- --write-schema config/schema` and kept in sync by a server test.
It holds the speed steps (ticks per real second), the delta rate, `max_walkers`, the recent-bids
history and the token rule. Nothing in it can change a game's result.

## Server

The `server` crate is the game inspector's back end (§10d, R88); the wire contract is
[`inspector-protocol.md`](inspector-protocol.md) and the `protocol` crate.

| Module | Does |
|---|---|
| `config` | `server.yaml`: speeds, delta rate, `max_walkers`, recent bids, token rule |
| `view` | pure functions `&World` → protocol shapes: the board's rectangles, lots (owner, land value, lease, buildings, shop table, takings, costs, rent, profit), players, the live auction and its room, walkers |
| `session` | one game, every seat an AI (`World::seated` with the `ai.yaml` roster from the last seat down): steps a tick at a time, keeps what the world forgets at midnight (each business's takings and units sold) and what the auction house does not keep (bid events, passes, last bids, rounds), and makes one snapshot then deltas holding only changed lots and players |
| `net` | the WebSocket server: plain `tungstenite` on a std thread, one client, bound to `127.0.0.1:0`; answers `hello` (protocol version and one-time token), `new_game`, `set_speed`, `leave`; runs ticks at the speed's rate and sends at most `deltas_per_second` deltas a second |

The session only reads the world between ticks, so a watched game is the same game as a headless
one with the same config and seed (a scenario checks the state hash). The wall clock is read only
in `net`, to decide how many ticks to run. The binary `opentycoon-server` checks the config (and
with `--serve --token` serves one client); single player does not use it.

## Client

`client/` is a Godot 4.7 project (R47). GDScript owns scenes, cameras and UI; the Rust extension
(`crates/client-ext`, registered by `client/opentycoon.gdextension`) owns the network client and
will own walker interpolation. Its `SimLink` node starts the server in-process on a thread
(R44: single player is multiplayer on localhost, hidden from the player), connects over
WebSocket, polls the socket without blocking in `_process` and emits each message as a
`Dictionary` signal; scenes never touch the socket. GUT is vendored under `client/addons/gut`;
tests live in `client/test/` (`test_sim_link.gd` plays a game through the real server).
