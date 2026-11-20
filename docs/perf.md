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
| `sim-cli bench config` | headless throughput of `World::tick` | JSON |
| `sim-cli perf-budget config` (`cargo perf`) | measures the budgeted metrics, compares with `perf/budgets.yaml` | table or JSON; exit 1 if over budget |
| `cargo bench -p sim-core` | criterion micro-benches (`benches/tick.rs`) | criterion reports |

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
  opens, then would build one shop (waiting on a build command in sim-core). The AI opponents
  from `ai.yaml` play their own seats inside `World::tick`. Harness brains get their own RNG
  stream (derived from the game seed and seat), so they never shift the world's draws.
- **CSV columns**: `seed, winner (p<i> | tie | none), days_played, ticks, state_hash`, then
  `p<i>_net_worth_pence, p<i>_bankrupt` per seat, then `wall_ms`. `state_hash` equals
  `sim-cli run --seed S --ticks <ticks>` for that seed when the free seats are idle.
- **Summary** (stdout with `--out`, else stderr; `--summary-json FILE` for JSON): games/sec,
  days played, bankruptcy rate (bankrupt player-games / player-games), ties, and per seat the
  p10/median/p90 net worth, bankruptcy rate and wins. Percentiles are nearest-rank on integer
  pence, so summaries are exact and platform-independent.

**Net worth today is cash only.** `game::player_net_worth` calls `economy::net_worth` with the
cash balance; stock, property and held lots join when economy exposes a whole-world valuation.
Change that one function when it lands.

## `sim-cli bench`

```sh
cargo run --release -p sim-cli -- bench config --days 4            # the v1 game
cargo run --release -p sim-cli -- bench config --days 1 --agents 10000
```

One untimed warm-up, then `--repeats` (default 5) timed runs of `--days` game days; the median
gives `ticks_per_second`, `ms_per_game_day` and `real_time_factor` (`real_ms_per_game_day /
ms_per_game_day`: how many times faster than 1× play). A separate pass times every tick for
`tick_latency` (p50/p90/p99/max, ns). `--agents N` is the synthetic large scenario: it sets
`customers.initial_population = N`, so it exercises whatever the population and customer phases
do with N people. `state_hash` in the output proves the repeats were the same game.

Timer resolution on the arm64 reference laptop is ~42 ns (a 24 MHz counter), so while ticks are cheaper than
that, `tick_latency` reads 42 ns; the throughput numbers are the accurate ones.

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
| `game_day_ms` | 50 ms | 0.147 ms | one game day (8,640 ticks), shipped config, v1, 2 players |
| `tick_p99_us` | 1,000 µs | 0.042 µs (timer floor) | p99 tick, same scenario (2,000 people) |
| `tick_p99_us_10k_agents` | 5,000 µs | 0.042 µs (timer floor) | p99 tick with `initial_population = 10000` |
| `batch_1000_games_s` | 600 s | 0.25–0.46 s | 1,000 v1 games on every core, extrapolated from 200 |
| `config_load_ms` | 20 ms | 1.6 ms | load + validate the shipped config |

**Why the limits are the design budgets, not "measured × 1.2".** At M1 wave 2 most phases
(settlement, auctions, customers, businesses) are stubs, so measured numbers are a floor. A tight
multiple would fail every PR that adds real work, which teaches people to bump the file. The
limits are the engineering.md §4 design budgets; the baseline column catches drift.

**The budgets are not mutually consistent yet.** 1 ms per tick (the 2,000-customer budget) ×
8,640 ticks is 8.6 s per day, far over the 50 ms day budget, and the 1,000-game budget
(600 s on 10 cores for 4,000 game days) allows about 1.5 s per day. When customers land, the
1,000-game budget is the one that binds: measure, then revisit the day and tick limits with
numbers, not guesses.

**Why it is not in `cargo test`.** Shared CI runners are slower and noisier than the reference
machine, so a timing check there flakes. The perf-budget BDD scenarios in
`crates/sim-cli/features/perf_budget.feature` only use limits far from any real measurement (a
pass at 100,000 ms, a fail at 0.000001 ms), so they test the mechanism, not the machine. Run the
real check with `cargo perf` on a quiet machine before and after perf-sensitive work.

**Updating budgets.** When a phase gains real work: run `cargo perf` (and `cargo bench -p
sim-core`) on the reference machine, update `baseline` in `perf/budgets.yaml` and the table
above, and state the change in the PR. Change a `limit` only with a reason in its `why`.

## Measured numbers (2026-11-20, a 10-core arm64 laptop, release)

At M1 wave 2 (clock real; economy, land, auctions, businesses, population, customers phases
stubbed):

- `batch --games 1000` (v1, 2 players, idle): **0.23 s wall, ~4,300 games/sec**; scripted brain
  0.19 s, ~5,300 games/sec.
- `bench --days 4`: 34,560 ticks in 0.57 ms, **~61 M ticks/sec, 0.14 ms per game day,
  real-time factor ~8.4 million**. `--agents 10000`: the same (no agent work yet).
- `config_load_ms`: 1.6 ms.

## Determinism checks

- `sim-cli run config --seed 42 --ticks 17280` prints the state hash; CI compares it between
  Linux x86_64 and macOS aarch64 (`docs/ci/ci.yml.proposed`). The output line is a contract.
- `crates/sim-cli/tests/determinism.rs`: a 32-game batch on 1 thread and on 8 threads gives
  byte-identical CSVs (both brains).
- `crates/sim-cli/features/determinism.feature`: the same at 1 vs 4 threads in-process, 1 vs 3
  threads through the binary, and every batch game's `state_hash` equals a plain `World` run of
  that seed for the same ticks.
