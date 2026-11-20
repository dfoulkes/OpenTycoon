# The balance study

How OpenTycoon looks for a balanced economy (R64, R65): `sim-cli study` samples ~100
economic models (parameter sets), plays each over ~500 games of the full 1930–2050 span with the
seats rotated, and ranks the balanced ones. Like the rest of the harness it implements no game
rule: a model is a config overlay validated exactly like `--set`, and every game is an ordinary
`World`. The code is `crates/sim-cli/src/study/`; the scenarios are
`crates/sim-cli/features/study.feature`.

```sh
cargo run --release -p sim-cli -- study config                       # study/study.yaml → study-out/
cargo run --release -p sim-cli -- study config --models 10 --games 50 --preset v1 \
    --roster grocer,idle --out study-out/smoke                        # a quick look
cargo run --release -p sim-cli -- study config --set game.players.starting_cash=9000 # on a base change
```

Interrupt it at any time: rerun the same command and it resumes (below). `--stop-after N` runs
N more models and stops, for chunked runs.

## The definition file

`study/study.yaml` (or `--def FILE`). It cannot live in `config/`: that directory holds exactly
the eight game files and the loader rejects any other YAML there. Parsing is strict: an unknown
key, a wrong type or a bad range fails before any game, naming the key path
(`parameters[3].min`). Every top-level key is optional:

| Key | Default | Meaning |
|---|---|---|
| `models` | 100 | models, including model 0 |
| `games_per_model` | 500 | rounded **up** to a whole number of seat rotations (504 for six seats) |
| `preset` | `full` | preset from `game.yaml`; if missing, the longest preset is used (with a warning) |
| `seed` | 0 | drives the hypercube and the game seeds |
| `roster` | every personality in `ai.yaml` (id order), then `idle` | the seats: personality ids and at most one `idle`; 2–6 seats (`game.players`) |
| `top_n` | 10 | balanced models that get an overlay |
| `criteria` | R65 | `min_win_share` 0.10, `max_win_share` 0.40, `max_bankruptcy_rate` 0.15, `min_idle_below_solvent_rate` 1.0 |
| `parameters` | none | the varied values (below) |

`--models`, `--games`, `--preset`, `--seed`, `--roster a,b` and `--top` override the file.
`--set file.key=value` changes the **base** config every model starts from (model 0 included).

### Parameters

```yaml
parameters:
  - { name: lease_term, key: game.leases.term_years, min: 10, max: 60, kind: integer }
  - { name: overdraft,  key: game.overdraft.start_limit, min: 2500, max: 25000,
      distribution: log_uniform, kind: integer }
  - { name: births,     key: customers.births_per_woman, min: 0.7, max: 1.3, mode: scale }
  - { name: land_value, key: game.net_worth.land_value, values: [book_decay, market, market_no_decay] }
```

| Field | Default | Meaning |
|---|---|---|
| `name` | required | column `param_<name>` in `models.csv`; letters, digits, `_` |
| `key` / `keys` | one required | config key(s) in `--set` form, `file.key.path` (year keys and list indices work); every key takes the same value |
| `min`, `max` | required (numeric) | inclusive range; leave out for a categorical parameter |
| `values` | none | makes the parameter **categorical**: each model sets the key to one of these words (letters, digits, `_`, `-`; no repeats). Takes no `min`/`max`, `distribution`, `decimals` or `mode: scale`. `models.csv` shows the word |
| `distribution` | `uniform` | `uniform`, or `log_uniform` (even in log space; needs `min > 0`) |
| `kind` | `decimal` | `integer` (every whole number in range equally likely) or `decimal` |
| `decimals` | 2 | places a decimal is rounded to. Money in the config is pence and ratios are basis points, so a value with more places than the key allows makes that model invalid |
| `mode` | `set` | `set` replaces the value; `scale` multiplies **every number under the key** by the sampled factor (a scalar, or a whole map such as every decade of `customers.births_per_woman`). A whole-number leaf stays whole |
| `optional` | false | if the key does not exist in the base config, skip the parameter with a warning instead of failing the study: for keys that only exist once some content lands |
| `expect` | none | guards on the base config, `{ key, equals }` or a list of them: the node at `key` must equal `equals` (mappings in any key order), or the study refuses to start, naming the parameter, the key, what it found and what it expected. For keys addressed by position, such as `customers.desires.6.goods`, so a reordered rule is never varied in its place. Skipped for an `optional` parameter whose key is missing |

A non-optional key that does not exist fails the study before any game. A sampled value the
config rejects (out of the key's range, or breaking a cross-file rule) makes that model
**invalid**: it is reported in `models.csv` and the summary and the study carries on.

## How models are made

- **Model 0 is always the base config** (the shipped defaults, plus any `--set`): the baseline
  every other model is compared with. Its `param_` columns show the base values (1 for a scale
  factor; the base word for a categorical parameter, listed among its `values` or not).
- **Models 1..N−1 are a Latin hypercube** over the parameters: each parameter's range is cut
  into N−1 equal strata, a seeded Fisher–Yates shuffle assigns strata to models, and a seeded
  jitter places each sample inside its stratum. Every stratum of every parameter is sampled
  exactly once. The draws are sim-core's portable ChaCha8 helpers seeded from `seed`, and
  log-uniform maps use `libm`, so the same definition gives the same models on any machine.
  Adding a parameter at the end leaves the earlier parameters' samples unchanged.
- **A categorical parameter** with `k` values is stratified the same way: the model in stratum
  `s` of `N−1` takes value `⌊s · k / (N−1)⌋`, so each value gets an equal share of the models
  (`(N−1)/k`, or one more or fewer when `k` does not divide `N−1`), whatever the jitter.
- A model's config is: base sources → the preset's start/end years → the model's overrides,
  through the normal loader (schema + cross-file rules, R45), the same order as
  `sim-cli batch --preset P --set ...`.

## How games are played

A roster of `n` entries fills `n` seats. Games run in blocks of `n`: block `b` uses seed
`seed + b` and plays it once under each rotation `r = 0..n−1`, where seat `s` holds roster entry
`(s + r) mod n`. So every entry sits in every seat equally often and every seed is played by
every seating: seat order and seed luck cannot bias the win shares. **Every model uses the same
seeds** (common random numbers), so models differ only by their parameters.

AI personalities play their seats inside `World::tick` (`World::seated` puts each one in its
seat); the `idle` seat is the harness's idle brain. A personality with no brain yet sits inert,
which is why the default roster cannot be balanced until wave B: inert personalities never win.

With a rotation that puts the AI in the last seat, a study game is exactly a `sim-cli batch`
game of the same seed (same `state_hash`).

Games of a model run in parallel on a `rayon` pool (`--threads`, default every core); results
are collected in plan order, so every output except the wall-time columns is identical on any
thread count (`--no-wall-time` drops those).

## Metrics

Per model, from integer counts (so exact and platform-independent up to float formatting):

| Metric | Definition |
|---|---|
| win share | games an entry won outright / games. A tie at the top, or nobody solvent, is nobody's win (`no_winner`) |
| Wilson interval | 95% Wilson score interval of the win share, z = 1.959964: centre `(p̂ + z²/2n)/(1 + z²/n)`, half-width `z/(1 + z²/n) · √(p̂(1−p̂)/n + z²/4n²)` |
| `max_win_share`, `min_win_share` | over the AI personalities |
| `evenness_entropy` | normalised Shannon entropy of the AI win counts, `−Σ pᵢ ln pᵢ / ln k`: 1 = perfectly even, 0 = one personality wins everything. 0 if no AI ever wins; 1 with a single AI (nothing to compare) |
| `evenness_gini` | Gini index of the AI win counts, `Σᵢ Σⱼ |xᵢ − xⱼ| / (2k² x̄)`: 0 = even, `(k−1)/k` = one wins everything |
| `bankruptcy_rate` | bankrupt player-games / player-games, every seat (as in `sim-cli batch`) |
| `ai_bankruptcy_rate` | bankrupt AI player-games / AI player-games: the R65 criterion |
| rank in a game | 1 + the number of players strictly ahead; solvent players are ahead of bankrupt ones, then higher net worth (the final midnight board, as in `batch`) |
| `idle_mean_rank` | the idle seat's mean rank (1 = first) |
| `idle_last_rate` | share of games the idle seat finished **strictly** last, bankrupt AIs included (a tie at the bottom does not count). Reported only; it does not decide `balanced` |
| `idle_below_solvent_rate` | share of games the idle seat finished **strictly** below every AI that is still solvent: the R65 criterion. Bankrupt AIs are ignored (the bankruptcy limit covers them); a game where no AI is solvent counts as a miss, since the idle seat beat everyone |
| net worth | per entry, p10 / median / p90 of final net worth in pence (nearest rank, as `batch`) |
| `games_per_sec` | games / wall time of that model (only with wall time) |

**`balanced`** (R65) is true when, with at least one game played:

1. every AI personality's win share is within `[min_win_share, max_win_share]` (10–40%,
   inclusive);
2. `ai_bankruptcy_rate` is **below** `max_bankruptcy_rate` (15%); the idle seat is not counted,
   since it never trades and would dilute the rate;
3. the roster has an idle seat and `idle_below_solvent_rate ≥ min_idle_below_solvent_rate`
   (1.0: below every solvent AI in every game). A roster without an idle seat can never be
   balanced. An AI bankruptcy only counts against criterion 2, so one bankrupt AI no longer
   fails a model on its own (until 2026-11-20 the rule was "idle strictly last", bankrupt AIs
   included, reported as `idle_last_rate`).

**Score and ranking.** `score` = `evenness_entropy` ("ranked by how even the win shares are").
Models are ranked: balanced first; then higher score; then lower `max_win_share`; then lower
`ai_bankruptcy_rate`; then lower id. Invalid models are not ranked. `rank` is 1-based.

## Outputs

All under `--out` (default `study-out/`, ignored by git). The columns below are a contract the
charts rely on: new columns are added at the end of their group, never renamed or reordered.

### `models.csv`

One row per model, sorted by `model_id` once the run ends (rows are appended as models finish,
so a crash loses at most the model in progress). Columns, in order:

1. `model_id`, `status` (`ok` | `invalid`), `error` (invalid only: the loader's message, naming
   the file and key; quoted, newlines as `; `);
2. `param_<name>` per parameter, in definition order: the value set, or the scale factor;
3. `games`, `no_winner`, `max_win_share`, `min_win_share`, `evenness_entropy`,
   `evenness_gini`, `bankruptcy_rate`, `ai_bankruptcy_rate`, `idle_mean_rank`,
   `idle_last_rate`, `balanced` (`true`/`false`), `score`, `rank`, then `games_per_sec` (left
   out with `--no-wall-time`), then `idle_below_solvent_rate`;
4. per roster entry, in roster order: `<entry>_wins`, `<entry>_win_share`, `<entry>_win_low`,
   `<entry>_win_high`, `<entry>_bankrupt_rate`, `<entry>_nw_p10`, `<entry>_nw_median`,
   `<entry>_nw_p90` (net worth in pence).

Shares and rates are fractions with six decimals. Invalid rows leave the metric columns empty;
so does a missing idle seat for the `idle_` columns.

### `games.csv` (`--games-csv`)

One row per game: `model_id, game, seed, rotation, winner (entry | tie | none), days, ticks,
state_hash`, then per seat `seat<i>_entry, seat<i>_net_worth_pence, seat<i>_bankrupt`, then
`wall_ms` (left out with `--no-wall-time`).

### `summary.json`

- `study`: the resolved definition, the `preset` actually run, `roster`, `seats`,
  `games_per_model` (after rounding), `seeds` (`first..end`);
- `models`: `total`, `finished`, `complete`, `ok`, `invalid`, `balanced`;
- `ranking`: every ok model in rank order: `rank`, `model_id`, `balanced`, `score`,
  `max_win_share`, `ai_bankruptcy_rate`, `idle_last_rate`, `idle_below_solvent_rate`;
- `top`: the ids that got an overlay (the best `top_n` balanced models);
- `invalid`: `model_id` and `error` per invalid model;
- `baseline`: model 0's row as an object (column → value);
- `columns`: the `models.csv` and `games.csv` headers of this study;
- `timing` (only with wall time): `wall_s`, `games_this_run`, `games_per_sec`, `threads`.

### Overlays: `models/<id>/`

Written when the study is complete, for the top `top_n` balanced models (older overlays are
removed):

- every config file the model changes (e.g. `game.yaml`), the whole file with the model's
  values. **To adopt a model, copy these over `config/`** and run `cargo validate`. They are
  re-serialised, so the shipped comments are lost: keep the comments by applying
  `overrides.txt` by hand instead if you prefer;
- `overrides.txt`: the same change as `file.key=value` lines (one `--set` each);
- `model.json`: the overrides and the model's `models.csv` row.

## Resume

`study.json` in the output directory holds a fingerprint of everything that decides the results
and the file layout: the definition, the base config files, the preset, the roster and the
`--games-csv` / wall-time options. A restart with the same fingerprint keeps every model already
in `models.csv`, drops a line cut off mid-write and any `games.csv` rows of unfinished models,
and runs the rest. A different study in the same directory is refused; use `--fresh` to discard
it or choose another `--out`.

## Cost

Measured on a 10-core arm64 laptop with other builds running (load average 10–13), release
build, 2026-11-20:

| Run | Wall time |
|---|---|
| 10 models × 50 games, `v1`, Grocer + idle | 1.7 s (500 games) |
| 3 models × 120 games, `full`, default roster (6 seats) | 4.2 s (360 games, ~86 games/s; ~80 core-ms per game) |

So the R64 study, 100 models × 504 games of `full` (50,400 games), is about **10 minutes** on
that machine today. The shipped `study/study.yaml` runs 300 models × 2,004 games (2,000 rounded up to
six-seat rotations; 601,200 games): 2,000 games puts a win share within about ±2 points, and
the extra compute goes on more models for its ~25 parameters. At the rate above that is about
**2 hours**, before the cost of the full roster. Per-game cost scales with what the AI and the population do: expect it to
grow as more personalities, the full map, hotels and online land, and re-measure from
`summary.json` `timing` after a few models (the progress line prints an ETA).
