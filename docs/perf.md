# Performance and the balance harness

How fast the sim is, how we know, and what fails when it gets slower. The budgets themselves are
in [`perf/budgets.yaml`](../perf/budgets.yaml); the standards are in
[`engineering.md`](engineering.md) §4. Everything here runs through `sim-cli`
(`crates/sim-cli/`), which drives `sim_core::World` and measures whatever it does: it implements
no game rules.

## The tools

| Command | What it does | Output |
|---|---|---|
| `sim-cli batch config` | 1,000 games (default) of the `v1` preset (1930–1950) in parallel | CSV (one row per game) + summary |
| `sim-cli bench config` | headless throughput of `World::tick`; `--phases` adds a per-phase breakdown | JSON |
| `sim-cli perf-budget config` (`cargo perf`) | measures the budgeted metrics, compares with `perf/budgets.yaml` | table or JSON; exit 1 if over budget |
| `cargo bench -p sim-core` | criterion micro-benches (`benches/*.rs`) | criterion reports |
| `sim-cli study config` | the balance study (R64, R65): sampled models × rotated games | `models.csv`, `summary.json`, overlays ([`study.md`](study.md)) |

Always measure with a release build (`cargo run --release -p sim-cli -- ...`, or the `cargo perf`
/ `cargo simbench` aliases). Debug builds are 10–50× slower.

## `sim-cli batch`

```sh
cargo run --release -p sim-cli -- batch config --games 1000 --out runs/v1.csv
cargo run --release -p sim-cli -- batch config --brain scripted --set game.overdraft.start_limit=20000 --out runs/od20k.csv
```

- **Seeds** `seed..seed+games` (`--seed`, default 0). Each game depends only on the config, its
  seed and the brains, and results are sorted by seed before writing, so the CSV is identical on
  any number of threads (`--threads N`, default every core). `--no-wall-time` drops the only
  non-deterministic column so two runs diff byte for byte.
- **Preset** `--preset v1` (default) sets `game.clock.start_year`/`end_year` from
  `game.presets.v1`. Any preset in `game.yaml` works.
- **Overrides** `--set file.key.path=value` (repeatable): the first segment names the YAML file
  (`game` → `game.yaml`), the rest is the key path (list indices and year keys work:
  `customers.births_per_woman.1930=2.5`), the value is YAML. The edited config goes through the
  normal schema + cross-file validation (R45), so a bad sweep value fails before any game runs,
  naming the file and key. `--set` is applied after `--preset`, so it wins.
- **Brains** for the seats `ai.yaml` does not fill (`--brain`): `idle` does nothing (the control
  group); `scripted` opens an auction on the cheapest free lot at its base price once day trade
  opens, then builds one grocery on the lot it wins and orders its full capacity. The AI
  opponents from `ai.yaml` (the full roster: the Grocer, the Landlord, the Toff, the Speculator and
  Sir Penny Sugar, from the last seat down) play their own seats inside `World::tick`. Harness brains get their own RNG
  stream (derived from the game seed and seat), so they never shift the world's draws.
- **CSV columns**: `seed, winner (p<i> | tie | none), days_played, ticks, state_hash`, then
  `p<i>_net_worth_pence, p<i>_bankrupt` per seat, then `wall_ms`. `state_hash` equals
  `sim-cli run --seed S --ticks <ticks>` for that seed when the free seats are idle.
- **Summary** (stdout with `--out`, else stderr; `--summary-json FILE` for JSON): games/sec,
  days played, bankruptcy rate (bankrupt player-games / player-games), ties, and per seat the
  p10/median/p90 net worth, bankruptcy rate and wins. Percentiles are nearest-rank on integer
  pence, so summaries are exact and platform-independent.

**Net worth** is the economy's nightly board (`World::net_worth()`, R19/R31/R39): cash, stock,
owned property and held lots, scored at each midnight before the write-off, so after the final
whistle it is the final score. A bankrupt player keeps their last scored value. The scenario
"Players with different assets get different net worths in the CSV" (`batch.feature`) guards
against the harness scoring cash only again.

## `sim-cli bench`

```sh
cargo run --release -p sim-cli -- bench config --days 4            # the v1 game
cargo run --release -p sim-cli -- bench config --days 1 --agents 10000
```

One untimed warm-up, then `--repeats` (default 5) timed runs of `--days` game days; the median
gives `ticks_per_second`, `ms_per_game_day`, `tick_mean_us` (the day per tick) and
`real_time_factor` (`real_ms_per_game_day / ms_per_game_day`: how many times faster than 1×
play). Then `--repeats` more runs time every tick for `tick_latency` (p50/p90/p99/p99.9/max, ns);
each percentile is the median over those runs, so one tick preempted by a busy machine does not
become the reported max. `--agents N` is the synthetic large scenario: it sets
`customers.initial_population = N`. `state_hash` in the output proves the repeats were the same
game.

`--phases` adds one more run through `World::tick_observed`, which calls the harness back after
each tick phase; the harness reads the clock there (sim-core never does), giving `ms_per_day`,
`share` and `runs_per_day` per phase. The clock reads cost ~20–40 ns each, so phases that run
every tick but do nothing (clock, commands, auctions, AI, businesses) show ~0.2–0.3 ms per day of
pure timer overhead, and the phase total is a little above the throughput number.

Timer resolution on the arm64 reference laptop is ~42 ns (a 24 MHz counter): most ticks are
idle and read 42–84 ns at p50/p90. The mean comes from the throughput runs, so it is accurate
below the timer floor.

## Budgets and `perf-budget`

```sh
cargo perf                                                         # all budgets, table
cargo run --release -p sim-cli -- perf-budget config --json
cargo run --release -p sim-cli -- perf-budget config --only game_day_ms --budget game_day_ms=0.1
```

A metric passes while `measured ≤ limit × (1 + tolerance)` (tolerance 20%). Each budget also
records the `baseline` measured when it was set; drifting more than the tolerance above it is
printed as a warning to flag in the PR, but only the limit fails the run.

| Metric | Limit | Baseline (10-core arm64 laptop, 2026-11-20) | Measures |
|---|---|---|---|
| `game_day_ms` | 20 ms | 1.8–1.9 ms | one game day (8,640 ticks), shipped config (2,000 adults), v1, 2 players |
| `tick_mean_us` | 2.315 µs | 0.21–0.22 µs | the same day per tick |
| `tick_p99_us` | 46 µs | 3.2–3.4 µs | p99 tick, same scenario |
| `game_day_ms_10k_agents` | 100 ms | 8.6–9.2 ms | one game day with `initial_population = 10000` |
| `tick_p99_us_10k_agents` | 231 µs | 18–20 µs | p99 tick, same |
| `tick_max_us_20k_agents` | 8,680 µs | 1,390–1,480 µs | slowest tick with `initial_population = 20000` (median of 5 runs' max) |
| `batch_1000_games_s` | 10 s | 0.9–1.2 s | 1,000 v1 games on every core, extrapolated from 200 |
| `config_load_ms` | 20 ms | 1.6 ms | load + validate the shipped config |

### Deriving the budgets

The inputs are the game's real-time needs:

- **1× play**: a game day lasts 20 real minutes = 8,640 ticks, so 7.2 ticks/s and 138.9 ms of
  real time per tick.
- **4× speed votes** (multiplayer): 28.8 ticks/s, **34.7 ms per tick**. The server also serves
  clients in that time, so one tick may use at most a quarter of it: **8.68 ms**, at any
  population we support (20,000 adults is the largest).
- **Balance sweeps**: 1,000 v1 games (1930–1950, 4 game days each, so 4,000 game days) headless.
  The spec allows 10 minutes; a sweep someone reruns while tuning should take seconds.

Those needs are all loose against what the sim costs (a 2 ms day is 0.0002% of 1× real time),
so a limit taken straight from them would never catch a regression. Instead there is **one
root**, the shipped-scenario day budget, and every other tick limit is derived from it:

1. **D = 20 ms** per game day at the shipped 2,000 adults: about 10× the measured 1.9 ms, room for
   the businesses, AI and auctions work still to land.
2. **Mean tick** = D / 8,640 = **2.315 µs**. This is the same budget per tick: it passes exactly
   when `game_day_ms` does (the two come from the same runs).
3. **p99 tick** = 20 × mean = **46 µs**. Ticks are bursty: most are idle (~40 ns) and the work
   lands on the ~720 ticks a day when shoppers are out. The measured p99/mean ratio is 14–17 at
   2k–20k adults, so the factor must be above 17 or a uniform slowdown would fail the p99 before
   the day; 20 keeps both limits at the same ~10× headroom. Consistency bound: at the limit, the
   slowest 1% of ticks (86 ticks) use 86.4 × 46 µs = 4 ms, 20% of D. A p99 limit above
   D / 86.4 = 231 µs would be unreachable while the day passes: that is the contradiction the old
   1 ms limit had (86 ms of slow ticks against a 50 ms day).
4. **Scaling**: customer work is linear in adults, so the 10k day is D × 5 = **100 ms** and its
   p99 is 20 × (100 ms / 8,640) = **231 µs**. Superlinear growth (an O(n²) scan) fails here first.
5. **Worst tick**: the 20k scenario's slowest tick (the 07:00 spawn wave) must fit in 25% of a 4×
   tick: **8.68 ms**. This is the only limit that comes straight from real time; it is below the
   20k day budget (D × 10 = 200 ms), so it cannot pass while that would fail.
6. **Batch**: 4,000 days × D = 80 core-seconds; on the reference 10 cores at 80% parallel
   efficiency (measured: ~95%) that is **10 s**, 60× inside the spec's 10 minutes. On a machine
   with a different core count, scale by cores.
7. **Real time is implied**: D at 20k adults (200 ms) is 0.07% of a 4× day (300 s); the mean tick
   at 20k (23 µs) is 0.07% of a 4× tick. A server running many games at 4× still fits.

`config_load_ms` (20 ms) is start-up cost, independent of D.

`perf-budget` checks rules 2, 3 (both scenarios) and 5 before it measures anything, and refuses a
`budgets.yaml` that breaks them (`budget::consistency`; the old file fails it). `--budget`
overrides on the command line are experiments and are not held to it.

**Why it is not in `cargo test`.** Shared CI runners are slower and noisier than the reference
machine, so a timing check there flakes. The perf-budget BDD scenarios in
`crates/sim-cli/features/perf_budget.feature` only use limits far from any real measurement (a
pass at 100,000 ms, a fail at 0.000001 ms), so they test the mechanism, not the machine. Run the
real check with `cargo perf` on a quiet machine before and after perf-sensitive work.

**Updating budgets.** When a phase gains real work: run `cargo perf` (and `cargo bench -p
sim-core`) on the reference machine, update `baseline` in `perf/budgets.yaml` and the table
above, and state the change in the PR. Change a `limit` only by re-deriving it from D (or by
changing D, with the reason in its `why`); `perf-budget` rejects limits that disagree.

## Measured numbers (2026-11-20, a 10-core arm64 laptop, release)

M1 wave 3: customers and population real; AI, businesses phase and auctions mostly idle in the
benchmark game (both seats idle, so there are no shops and shoppers leave after one look). Two
other builds were running on the machine (load average 15–100), so every number is a range over
at least three runs; throughput spread was within ±5%.

`sim-cli bench --repeats 5` (after the optimisations below):

| Adults | ms per game day | mean tick | p99 tick | p99.9 tick | slowest tick |
|---|---|---|---|---|---|
| 2,000 (shipped) | 1.77–1.92 | 0.21–0.22 µs | 3.2–3.4 µs | 6.5 µs | 165 µs |
| 10,000 | 8.6–9.3 | 1.0–1.1 µs | 18–20 µs | 24 µs | 0.76 ms |
| 20,000 | 17.9 | 2.1 µs | 37–40 µs | 47 µs | 1.48 ms |

`sim-cli batch --games 1000`: 0.9–1.26 s on 10 threads (~800–1,100 games/sec).

Where a day goes (`bench --phases`, 20,000 adults, before the optimisations): customers 20.1 ms
(91%), arrivals 1.0 ms (once a day), every other phase ~0.2–0.3 ms of timer overhead. Inside
customers, by tick:

| Ticks per day | Kind | 2,000 adults | 20,000 adults |
|---|---|---|---|
| ~720 | shoppers on the map (07:00–08:00, 19:00–20:00 spawn spread) | 1.3–1.75 ms | 13.7 ms |
| 2 | wave opens (07:00, 19:00): every shopper spawned and given a list | 0.59 ms → 0.30 ms | 5.5 ms → 2.6 ms |
| 1 | 05:30 arrivals: house-hunting, the unhoused pool reshuffled | 0.12 ms | 1.07 ms |
| ~7,900 | idle | 0.32–0.38 ms (~40 ns each) | same |

The on-map ticks cost ~3.2 ns per agent per tick, where a bare scan of the agents costs 0.84 ns:
the rest is `Vec::retain_mut` compacting the vector every tick a shopper scattered through it
leaves, moving every later 96-byte `Customer`. The criterion `customers_day` bench (with a high
street of shops) is 28.6 ms per day at 10k adults.

## Determinism checks

- `sim-cli run config --seed 42 --ticks 17280` prints the state hash; CI compares it between
  Linux x86_64 and macOS aarch64 (`.github/workflows/ci.yml`). The output line is a contract.
- `crates/sim-cli/tests/determinism.rs`: a 32-game batch on 1 thread and on 8 threads gives
  byte-identical CSVs (both brains).
- `crates/sim-cli/features/determinism.feature`: the same at 1 vs 4 threads in-process, 1 vs 3
  threads through the binary, and every batch game's `state_hash` equals a plain `World` run of
  that seed for the same ticks.
