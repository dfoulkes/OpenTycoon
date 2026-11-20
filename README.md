# OpenTycoon

A real-time economic tycoon set in London, **1930 to 2050**. Two to six players, human or AI,
lease land at auction, put up buildings, run shops and charge rent, while a simulated population
of shoppers walks the streets and spends money. Every midnight the year jumps five years: the
Blitz, Decimal Day, the Big Bang, the dot-com boom. **The winner has the highest net worth at
the 2050 whistle**, or is the last player still solvent.

It's an immersive 3D world you can walk around at street level or run from a bird's-eye RTS
view. Single-player is multiplayer against a server on your own machine.

- **Spec**: [`docs/spec.md`](docs/spec.md) (rulings cited as R1–R61)
- **Architecture, module owners, tick order**: [`docs/architecture.md`](docs/architecture.md)
- **Engineering standards (BDD/TDD, determinism, lints, perf)**: [`docs/engineering.md`](docs/engineering.md)
- **CI**: [`.github/workflows/ci.yml`](.github/workflows/ci.yml)

Status: **M0 skeleton** (v1 plan in spec §10c). The workspace, config system, clock, BDD harness
and Godot project are real; the rest of the simulation lands in M1.

## Architecture

```
 sim-core (Rust library: the whole game, deterministic, headless, no I/O)
   ├── server   authoritative game server (multiplayer; single-player on 127.0.0.1)
   └── sim-cli  headless harness: config validation, balance runs
 client (Godot 4 + GDScript) + client-ext (Rust via godot-rust) ──protocol──► server
```

```
Cargo.toml            Rust workspace
crates/
  sim-core/           the game: config, money, clock, economy, land, auctions, businesses,
                      utilities, population, customers, ai, events, world
    features/         Gherkin scenarios, tagged with the rulings they prove
    tests/            BDD harness, determinism, config failure paths, traceability
    benches/          criterion benchmarks
  protocol/           wire messages (placeholder until M2)
  server/             game server (placeholder: validates config, prints ok)
  sim-cli/            harness CLI
  client-ext/         godot-rust extension
client/               Godot 4.7 project (+ GUT tests)
config/               YAML defaults (§10b) + generated JSON Schemas
docs/                 spec, architecture, engineering, CI proposal
```

Money is integer pence, randomness is a seeded ChaCha8 stream, and floats appear only in
customer scoring (through `libm`), so the same seed gives the same game on Linux x86_64 and
macOS aarch64.

## Build, test, bench

Needs Rust stable (see `rust-toolchain.toml`) and, for the client, Godot 4.7.

```sh
cargo build --workspace          # everything, including the Godot extension
cargo t                          # unit, property, BDD, schema-sync and traceability tests
cargo bdd                        # BDD scenarios only
cargo trace                      # which spec rulings have scenarios
cargo lint                       # clippy, warnings are errors
cargo fmtc                       # format check
cargo validate                   # validate config/
cargo schema                     # regenerate config/schema/*.schema.json after changing config types
cargo bench -p sim-core          # criterion benches (cargo bench-build to compile only)
cargo perf                       # perf budgets, release build (docs/perf.md)
```

The aliases live in `.cargo/config.toml`.

Run things:

```sh
cargo run -p sim-cli -- validate-config config            # names every bad key and file
cargo run -p sim-cli -- run config --seed 42 --ticks 8640  # one headless day, prints the state hash
cargo run -p server -- --config config                     # placeholder server: "ok"
```

## Running the harness

`sim-cli` is the headless balance harness (spec §11). Use release builds for anything timed.
Details, CSV columns and budgets: [`docs/perf.md`](docs/perf.md).

```sh
# 1,000 games of the v1 preset (1930–1950), every core, CSV + summary (median/p10/p90 net
# worth, bankruptcy rate, games/sec). Same CSV on any thread count.
cargo run --release -p sim-cli -- batch config --games 1000 --out runs/v1.csv

# Parameter sweep: override any config value; it is validated like a hand-edited file (R45).
cargo run --release -p sim-cli -- batch config --brain scripted \
    --set game.overdraft.start_limit=20000 --out runs/od20k.csv

# Throughput as JSON: ticks/sec, ms per game day, real-time factor, tick p99.
cargo simbench --days 4
cargo simbench --days 1 --agents 10000     # synthetic large scenario

# Perf budgets (perf/budgets.yaml): exits non-zero if over budget. Not part of `cargo t`.
cargo perf

# Balance study (R64, R65): ~100 sampled models × 500 rotated games of 1930–2050, ranked by
# how even the AI win shares are. Resumable; definition in study/study.yaml (docs/study.md).
cargo run --release -p sim-cli -- study config --out study-out
```

Brains for the seats `ai.yaml` does not fill: `idle` (does nothing) and `scripted` (bids on the
cheapest lot at day-trade open, then builds one grocery on it and stocks it). The real AI
opponents (in v1, the Grocer: `ai.yaml` `grocer`) play their own seats inside the sim.

Godot client and its tests:

```sh
cargo build -p client-ext                                  # the Rust extension the client loads
godot --headless --path client --import                    # first run only: build the import cache
godot --headless --path client -s addons/gut/gut_cmdln.gd  # GUT tests
godot --path client                                        # open the grey-box scene
```

## Contributing

Read [`docs/engineering.md`](docs/engineering.md) first: every behaviour starts as a failing
Gherkin scenario tagged with the spec ruling it proves. Licence: Apache 2.0 (see `LICENSE`).
Please follow the [code of conduct](CODE_OF_CONDUCT.md).
