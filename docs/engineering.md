# Engineering standards

The contract every change to OpenTycoon follows, human or agent. The spec
([`spec.md`](spec.md)) says *what*; this says *how*. Module boundaries and the tick order are in
[`architecture.md`](architecture.md).

## 1. Behaviour is driven by the spec: strict BDD + TDD

**Every rule in the sim exists because a ruling (R1–R61) or a spec section says so, and every
rule is proven by a test written before the code.**

### Gherkin features

- Behaviour lives in `.feature` files under `crates/sim-core/features/<module>/`, e.g.
  `features/clock/clock.feature`, `features/auctions/passing.feature`.
- They run with the [`cucumber`](https://docs.rs/cucumber) crate through one harness,
  `crates/sim-core/tests/bdd.rs` (`harness = false`, driven by `futures::executor::block_on`; no
  async runtime). Run them with `cargo bdd`.
- Step definitions live in `crates/sim-core/tests/steps/<module>.rs`. Scenario state is a field
  per module on `SimWorld` in `tests/bdd.rs`. Undefined or skipped steps **fail** the run.
- **Every scenario is tagged with the ruling(s) it proves**: `@R7 @R32`. Tag only what the
  scenario actually proves. A scenario about a spec section with no ruling cites it in a comment
  (`# §8.3`) and is tagged with the nearest ruling if one applies.
- Write scenarios in the spec's language (players, blocks, midnight, settlement), not the code's.
  Use `Scenario Outline` + `Examples` for boundary tables (07:00 open, 17:00 closed).

### Red → green → refactor

1. **Red.** Write the failing scenario (and/or unit test) first. Run it and watch it fail for the
   right reason. Commit it, or at least keep it in the same PR ahead of the code; the PR
   description says which scenarios went red → green.
2. **Green.** Write the least code that passes.
3. **Refactor** with the tests green.

Example in history: commit `e3bb0f7` added `clock.feature` against a stub clock (14 steps red);
`73728b2` implemented the clock (31/31 green).

### Unit and property tests

- Unit tests sit next to the code (`#[cfg(test)] mod tests`) and cover each pure function's edge
  cases.
- **`proptest` property tests for every invariant**, at minimum:
  - money conservation: transfers and settlement never create or destroy money (`economy`);
  - determinism: same seed + config ⇒ same state (`tests/determinism.rs`);
  - no negative stock, ever (`businesses`, when stock lands);
  - lease terms: remaining years stay within `0..=term` (`land`);
  - auction invariants: one auction at a time, bids never exceed the ceiling (`auctions`).
- Integration tests in `crates/sim-core/tests/` use the shipped `config/` via
  `tests/common/mod.rs` (`shipped_config()`, `set_key()` to override one value by JSON pointer).

### Traceability

`crates/sim-core/tests/traceability.rs` (`cargo trace`) parses the "Ruled" table in §12 of
`docs/spec.md` and the `@Rn` tags in every feature, then reports:

- fully covered, partial, pending, and out-of-scope-but-tagged rulings;
- **fails** if an in-scope ruling is neither tagged nor listed in
  `docs/traceability-pending.txt`, if a tag cites a ruling that doesn't exist, or if an id is in
  both allowlists. Stale pending entries are reported (set `TRACEABILITY_STRICT=1` to fail).

Allowlists:

- `docs/traceability-out-of-scope.txt`: rulings outside the v1 sim (post-v1 features, client,
  architecture), each with a reason.
- `docs/traceability-pending.txt`: in-scope rulings not yet proven. **Shrink it**: when your
  scenario fully proves a ruling, delete its line in the same PR. Mark half-proven ones
  `# partial: <what's missing>`.

When the spec snapshot is refreshed from the vault, re-run `cargo trace`: new rulings show up as
failures until they are tagged, pending or out of scope.

## 2. Readability and testability

- **Small pure functions.** Formulas from the spec are standalone functions named after the spec
  term (`land_rent_per_day`, `price_factor`, `net_worth`) with the formula in the doc comment.
- **No I/O in sim-core.** No files, network, env vars, stdout, wall clock or OS randomness. The
  one exception is `ConfigSources::read_dir`, behind the `fs` feature, used only by binaries.
- **Inject everything**: config (typed struct), RNG (`&mut SimRng`), time (`Clock` or plain
  values). No globals, no `static mut`, no lazy singletons.
- **No `unwrap()`/`expect()` outside tests** (`clippy::unwrap_used`, `clippy::expect_used` are
  denied; `clippy.toml` allows them in `#[test]` code; integration test files opt in at the top).
  Return a typed error or handle the case.
- **Docs on every public item** (`missing_docs` is denied in sim-core and warned elsewhere;
  CI runs with `-D warnings`). Module docs state the module's owner boundary and the rulings it
  implements.
- **One module per domain** (see architecture.md). A module that grows past ~600 lines becomes
  a directory with submodules.
- `rustfmt` (`rustfmt.toml`, width 100) and clippy are law: `cargo fmtc`, `cargo lint`.

### Lints

Set once in `[workspace.lints]` in the root `Cargo.toml`, so every crate shares them:

| Lint | Level | Why |
|---|---|---|
| `clippy::all` | deny | correctness and style |
| `clippy::pedantic` | warn (fails CI via `-D warnings`) | readability |
| `clippy::unwrap_used`, `expect_used` | deny | no panics in sim logic |
| `clippy::disallowed_types` | deny | `HashMap`/`HashSet` banned (iteration order) |
| `clippy::disallowed_methods` | deny | `f64::exp`/`ln`/`powf`/trig and wall-clock reads banned |
| `clippy::float_arithmetic` | deny in sim-core | floats only where a module opts out with a reason |
| `unsafe_code` | deny | only `client-ext` allows it, for the gdext entry point |
| `missing_docs` | warn (deny in sim-core) | every public item documented |

Allowed pedantic lints, with reasons (in `Cargo.toml`): `module_name_repetitions`,
`must_use_candidate`, `missing_errors_doc`. Any other `#[allow]` must carry
`reason = "..."` and be as narrow as possible (one item, not a module).

## 3. Determinism and OS/arch agnosticism

The same seed, config and command stream must produce the **same state, bit for bit, on Linux
x86_64 and macOS aarch64**. Balance runs, replays, saves and multiplayer all rely on it.

- **Money is integer pence**: `money::Money(i64)`. Never a float. Ratios are `money::Rate`
  (basis points) and `Rate::apply` rounds half away from zero. Config money and ratios are
  converted exactly at load and rejected if they would lose precision.
- **Seeded RNG only**: `rand_chacha::ChaCha8Rng` (`rng::SimRng`), created from the game seed and
  owned by `World`. Never `thread_rng`, `rand::rng()`, `OsRng`, or time-seeded anything. Use
  `rng::unit_f64` and `rng::below` (portable, unbiased) rather than ad-hoc conversions.
- **Floats only in customer scoring** (`customers`; `#![allow(clippy::float_arithmetic)]` with a
  reason). Transcendentals go through the **`libm`** crate (`libm::exp`), never `f64::exp`, which
  calls the platform libm and can differ between x86_64 and aarch64 (clippy enforces this). Basic
  `+ − × ÷` and `sqrt` are IEEE-exact and fine. No `mul_add`/FMA. A pinned-bits test
  (`customers::price_factor_bits_are_pinned`) runs on both CI targets.
- **No iteration over hash order**: `BTreeMap`/`BTreeSet` (or `IndexMap` if insertion order is the
  point) in sim logic. `HashMap`/`HashSet` are banned by clippy.
- **No platform-specific code**: no `cfg(target_os)`, no `usize` in hashed or saved state (use
  `u32`/`u64`), no pointer addresses, no parallel iteration whose order affects results.
- **State hash**: `World::state_hash()` uses `hash::StableHasher` (FNV-1a 64, little-endian,
  `usize` widened). `std`'s `DefaultHasher` is randomly keyed and must not be used for this.
- **Tests**: `tests/determinism.rs` checks same seed ⇒ same hash (including a proptest over
  seeds). CI runs `sim-cli run --seed 42 --ticks 17280` on both OSes and compares the hashes.

## 4. Performance

- **criterion** benches live in `crates/sim-core/benches/` and are never part of `cargo test`
  (`cargo bench-build` compiles them; CI does too). Add a bench with every phase that does real
  work.
- `sim-cli bench` (a fixed scenario printing ticks/sec) lands with M1.

### Budget

Real time per tick at 1× is 1200 s / 8640 ≈ **139 ms**. The sim must use a small fraction of that
so the server can run 4× speed votes, serve clients and run 1,000-game balance sweeps.

| Measure | Budget | Baseline (M0, Apple M1 Pro, `cargo bench`) |
|---|---|---|
| one tick, 3×3 map, 2 players, no agents | < 1 µs | 15.5 ns |
| one game day (8,640 ticks), 3×3 map | < 50 ms | 150 µs |
| one tick, 3×3 map, 2,000 customers | < 1 ms | *M1: fill in* |
| one tick, 9×9 map, 20,000 customers | < 10 ms | *post-v1: fill in* |
| 1,000 games 1930–1950 headless (`sim-cli`) | < 10 min on one machine | *M1: fill in* |
| config load + validate | < 20 ms | 1.37 ms |
| 06:00 settlement, 6 players × 1,000 buildings (`benches/economy.rs`) | < 1 ms | 110 µs (M1) |
| midnight net worth, 6 players × 1,000 buildings | < 1 ms | 74 µs (M1) |

Update the baseline column when a feature lands, and flag any regression over 20% in the PR.

## 5. Config

- **Every constant is configurable** (R17, R45). A number in sim code is either a unit
  conversion (seconds per day) or a bug. Spec defaults go in `config/*.yaml`; placeholders are
  marked `# placeholder`.
- Adding a key: add the field (with a doc comment, and `#[schemars(range(...))]` where it has
  bounds) to the typed struct in `src/config/<file>.rs`; add cross-field rules to
  `config/validate.rs`; set the default in `config/<file>.yaml`; run `cargo schema`; add a test
  that the bad value fails and names the key.
- **Fail loudly**: unknown keys, wrong types, out-of-range values, dangling references, unknown
  files. Every error names the file and the JSON pointer of the key.

## 6. Workflow

- Branch per ticket off `v2`; small commits that each build. Commit messages start with the
  ticket key (`OTY-12: ...`).
- Agents commit, push and open PRs as their GitHub App
  (`~/source/github_runner/scripts/github-agents/as-agent.sh <app> git ...`), never as Dan. Agents
  never merge, approve, or change branch protection. `.github/workflows/` is off-limits to apps:
  propose workflow changes as files under `docs/ci/` and say so in the PR.
- **Definition of done** for a change:
  - `cargo fmtc`, `cargo lint`, `cargo t` pass; `cargo bench-build` compiles;
  - scenarios tagged with the rulings proven, red → green shown in the PR;
  - `docs/traceability-pending.txt` shrunk accordingly;
  - config changes validated (`cargo validate`) and schemas regenerated (`cargo schema`);
  - perf baseline updated if a phase gained real work;
  - Godot tests pass if `client/` or `client-ext` changed.

## 7. Commands

| Alias (`.cargo/config.toml`) | Runs |
|---|---|
| `cargo fmtc` | `cargo fmt --all -- --check` |
| `cargo lint` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `cargo t` | `cargo test --workspace` (unit, property, BDD, traceability, schema sync) |
| `cargo bdd` | BDD scenarios only |
| `cargo trace` | traceability report |
| `cargo validate` | `sim-cli validate-config config` |
| `cargo schema` | regenerate `config/schema/*.schema.json` |
| `cargo bench-build` | compile benches without running |

Godot tests: `cargo build -p client-ext && godot --headless --path client --import && godot
--headless --path client -s addons/gut/gut_cmdln.gd`.
