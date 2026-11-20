# Architecture

How OpenTycoon v2 is put together, and where each piece of work belongs. The spec is
[`spec.md`](spec.md) (§10a–§10c for architecture); rulings are cited as R1–R61.

## Crates

```
crates/
  sim-core/    the game: deterministic, headless, no I/O        (library)
  protocol/    wire messages shared by server and client         (library, placeholder until M2)
  server/      authoritative server; single-player on 127.0.0.1  (binary `opentycoon-server`, placeholder)
  sim-cli/     headless harness: validate config, run, batch, bench, study (binary `sim-cli`; docs/perf.md, docs/study.md)
  client-ext/  godot-rust (gdext) extension for the Godot client  (cdylib)
client/        Godot 4.7 project: GDScript scenes, cameras, UI, GUT tests
config/        the eight YAML files (§10b) + generated JSON Schemas
```

Dependency direction is one way: `server`, `sim-cli` → `sim-core`; `server`, `client-ext` →
`protocol` (from M2). Nothing depends on `server` or `sim-cli`. `sim-core` depends on no other
workspace crate and never on the client.

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
| `customers` | `src/customers/` | shoppers, shopping lists, shop scoring, movement, tourists' nights (wandering, looking for a bed), online demand (`online.rs`) | R15, R25, R38, R42, R55, R59, §8.3 | **real**: hotels and online from wave A |
| `hotels` | `src/hotels.rs` | the Hotel unlock (full set) and one per set, rooms by volume, room offers and check-in | R14, R29, R30 | **real** (wave A) |
| `ai` | `src/ai/` | AI personalities behind the `Brain` trait, each seat on its own RNG stream | §10 | **real**: the Grocer, the Landlord, the Toff, the Speculator and Sir Penny Sugar (`ai.yaml`, one block each; shared `ai::kit`) |
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
| `game.yaml` | clock (clock, presets), economy (bankruptcy, overdraft, net worth, players), land (world, leases) |
| `map.yaml` | land |
| `auctions.yaml` | auctions |
| `utilities.yaml` | utilities |
| `businesses.yaml` | businesses |
| `customers.yaml` | population (cohorts, births, demographics), customers (allowance, desires, scoring) |
| `events.yaml` | events |
| `ai.yaml` | ai |

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
- **Event modifiers.** Lasting event effects (demand, supply caps, open days, price noise,
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

## Config

Eight YAML files (§10b). Pipeline: parse YAML → validate against the JSON Schema generated from
the Rust types (`schemars`) → deserialise into typed structs → cross-file rules
(`config/validate.rs`). Every problem is collected and reported with file and JSON pointer; any
problem fails the load. The committed `config/schema/*.schema.json` must match the types
(`tests/schemas.rs`); regenerate with `cargo schema`. YAML files carry a
`yaml-language-server` header so editors validate as you type.

Money is written as decimal pounds and converted exactly to pence; ratios are decimals converted
to basis points; times are `"HH:MM"`. A value that would lose precision fails the load.

## Client

`client/` is a Godot 4.7 project (R47). GDScript owns scenes, cameras and UI; the Rust extension
(`crates/client-ext`, registered by `client/opentycoon.gdextension`) will own the network client
and walker interpolation. GUT is vendored under `client/addons/gut`; tests live in
`client/test/`.
