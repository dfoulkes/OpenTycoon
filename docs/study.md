# The balance study

How OpenTycoon looks for a balanced economy (R64, R65) that is fun to play (R79): `sim-cli study`
samples ~100 economic models (parameter sets), plays each over ~500 games of the full 1930–2050
span with the seats rotated, and ranks the balanced ones by how much fun their games are. Like the rest of the harness it implements no game
rule: a model is a config overlay validated exactly like `--set`, and every game is an ordinary
`World`. The code is `crates/sim-cli/src/study/`; the scenarios are
`crates/sim-cli/features/study.feature`.

```sh
cargo run --release -p sim-cli -- study config                       # study/study.yaml → study-out/
cargo run --release -p sim-cli -- study config --models 10 --games 50 --preset v1 \
    --roster grocer,idle --out study-out/smoke                        # a quick look
cargo run --release -p sim-cli -- study config --set economy.money.starting_cash=9000 # on a base change
cargo run --release -p sim-cli -- study rescore --recordings s3 --run study-out --out study-out/fun
cargo run --release -p sim-cli -- study confirm config --from study-out --models 12,40 \
    --games 2004 --seed 1000000 --out study-out/confirm         # top models on fresh seeds
```

Interrupt it at any time: rerun the same command and it resumes (below). `--stop-after N` runs
N more models and stops, for chunked runs.

## The definition file

`study/study.yaml` (or `--def FILE`). The shipped `study.yaml` varies the economy only and holds AI behaviour at its shipped values, so a balanced model is balanced by the economy; `study/study-with-ai.yaml` also varies the AI tunables, to measure how sensitive balance is to AI strength. Every economy parameter varies a key in `config/economy.yaml` (R71), so a model is
one `economy.yaml` (a scenario checks that the shipped definitions vary nothing else, bar
`ai.yaml` in the AI variant). The definition cannot live in `config/`: that directory holds
exactly the nine game files and the loader rejects any other YAML there. Parsing is strict: an unknown
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
| `fun` | R79 bands | the fun score's bands, falloffs, weights and playstyle basis ("Fun (R79)" below) |

`--models`, `--games`, `--preset`, `--seed`, `--roster a,b` and `--top` override the file.
`--set file.key=value` changes the **base** config every model starts from (model 0 included).

### Parameters

```yaml
parameters:
  - { name: lease_term,   key: economy.land.lease_term_years, min: 10, max: 60, kind: integer }
  - { name: overdraft,    key: economy.money.overdraft.start_limit, min: 2500, max: 25000,
      distribution: log_uniform, kind: integer }
  - { name: allowance,    key: economy.population.allowance_by_class, min: 0.6, max: 1.6, mode: scale }
  - { name: demand_1950s, key: economy.demand_multipliers.1950s, min: 0.6, max: 1.4 }
  - { name: land_value,   key: economy.land.value_method, values: [book_decay, market, market_no_decay] }
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
| `mode` | `set` | `set` replaces the value; `scale` multiplies **every number under the key** by the sampled factor (a scalar, or a whole map such as every class of `economy.population.allowance_by_class`). A whole-number leaf stays whole |
| `optional` | false | if the key does not exist in the base config, skip the parameter with a warning instead of failing the study: for keys that only exist once some content lands |
| `expect` | none | guards on the base config, `{ key, equals }` or a list of them: the node at `key` must equal `equals` (mappings in any key order), or the study refuses to start, naming the parameter, the key, what it found and what it expected. For keys addressed by position (a list index such as `customers.desires.6.goods`), so a reordered list is never varied in its place. The shipped definitions need none: demand by decade is named in `economy.yaml` (`economy.demand_multipliers.1950s`), not addressed by rule index. Skipped for an `optional` parameter whose key is missing |

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

**Score and ranking (R65, R79).** `score` = `evenness_entropy` ("how even the win shares are").
Models are ranked: balanced first; then the higher `fun_score` (below; compared exactly as
fixed point, a row without one counts as 0); then the evenness tie-breaks: higher score, lower
`max_win_share`, lower `ai_bankruptcy_rate`, lower id. Invalid models are not ranked. `rank` is
1-based. `evenness_rank` is the ranking without the fun score (balanced, then the tie-breaks),
the order the study used until R79, kept for comparison. Until R79 `rank` was that order.

## Fun (R79)

"Best" means most fun, not most efficient. Each game is measured while it plays, from the
**midnight boards** (each seat's net worth and bankruptcy at every midnight, exactly what a
recording's checkpoints hold) and each seat's holdings at the whistle. The measurement only
reads the world, so a study game is the same game with or without it (same state hash;
`features/fun.feature`). Code: `crates/sim-cli/src/study/fun/`.

Everything that feeds the ranking is **integer**: rates, means, margins, distances, z-scores
and scores are fixed point in parts per million, computed from counts and pence with integer
rounding (half away from zero) and an integer square root. Band edges and weights are read from
the definition as decimals and converted once, exactly (`round(v × 10⁶)`). So every machine
ranks identically. The CSVs show them with six decimals and read them back exactly.

### Per game

| Measure | Definition |
|---|---|
| leader | at one midnight, the solvent seat(s) with the highest net worth (several on a tie; none if nobody is solvent) |
| lead changes | walking the midnights in order, the times a single leader differs from the last single leader seen. A tie at the top, or no solvent seat, keeps the previous leader; the first leader is not a change |
| halfway midnight | day `⌊D / 2⌋` of the `D` midnights played: day 12 of 24 in the full game, the 1990 midnight. None if `D < 2` |
| comeback | a game with an outright winner whose seat was **not** among the leaders at the halfway midnight (a shared lead counts as leading). Undecided with no outright winner or no halfway midnight |
| winning margin | `(winner − runner-up) / winner` in final net worth; the runner-up is the best other seat (solvent ahead of bankrupt, then net worth). Clamped to `[0, 1]`; undefined without an outright winner or when the winner's net worth is not above 0 |
| holdings | per seat at the whistle: **flats** (apartment buildings it owns), **shops** (walk-in businesses trading by day or night), **land** (buildable blocks it leases with none of its buildings on them), **hotels**, **other** (online and infrastructure businesses, leased utility blocks); plus **leases** (every block it leases) and **businesses** (every business it owns), the two counts a checkpoint holds |

### Per model

Over the model's games in plan order (seed by seed, each seed's rotations in turn):

| Metric | Definition | Band (default) |
|---|---|---|
| `fun_strongest` | the AI personality with the most outright wins (the earlier roster entry on a tie; empty if no AI won) | |
| `fun_upset_rate` | games whose outright winner is not the strongest AI (a tie, nobody solvent and an idle win included) / games | 0.50–0.75 |
| `fun_lead_changes_mean` | lead changes / games | at least 2 |
| `fun_decided` | games with an outright winner and a halfway midnight | |
| `fun_comeback_rate` | comebacks / decided games | 0.20–0.40 |
| `fun_margin_median` | the lower median (nearest rank) of the defined winning margins | at most 0.25 |
| `fun_distinctness` | **playstyle distinctness** on the holdings mix (flats, shops, land, hotels, other): each AI personality's holdings are pooled over all its games into one mix (the share of each category; a personality that never held anything has a mix of its own, "nothing"). The distance between two personalities is the **total variation distance** of their mixes, `½ Σᵢ |aᵢ − bᵢ|`, which is already in `[0, 1]`: 0 for the same mix, 1 for mixes with no category in common. Distinctness is the mean over every pair of personalities: 0 when they all own the same mix, 1 when no two share a category; 1 with fewer than two personalities | at least 0.30 |
| `fun_distinctness_checkpoint` | the same on what a checkpoint holds: leases and businesses | (with `playstyle: checkpoint`) |
| `fun_streak_z` | the **streak statistic**, a runs test on the winner sequence in plan order (below) | at most 2.0 |
| `fun_streak_longest` | the longest run of games won by one AI (reported only) | |

**The streak statistic.** `A` counts adjacent games won by the same AI personality (a tie,
nobody or the idle seat never matches). Under the hypothesis that the order is random (every
arrangement of the model's winners equally likely, which is what "independent of the seed
order" means given the win counts), the mean and variance of `A` are exact functions of the
AI win counts `nₖ` and the games `N`. With `(x)ᵣ` the falling factorial `x (x − 1) … (x − r + 1)`,
`S₂ = Σ (nₖ)₂`, `S₃ = Σ (nₖ)₃`, `S₄ = Σ (nₖ)₄`, `S₂₂ = S₂² − Σ (nₖ)₂²` and
`D = (N − 2)(N − 3) / 2`:

- `E[A] = (N − 1) S₂ / (N)₂`
- `E[A²] = E[A] + 2 (N − 2) S₃ / (N)₃ + 2 D (S₄ + S₂₂) / (N)₄`
- `Var[A] = E[A²] − E[A]²`, and `fun_streak_z = (A − E[A]) / √Var[A]` (0 if `Var[A]` is 0).

About 0 for a random-looking sequence, negative when winners alternate more than chance, and
large when one AI wins runs of games more often than chance predicts (z = 2 is the one-sided
2.3% level). The moments are evaluated in fixed point (10⁻¹² of a game) in 128-bit integers;
games of a model up to tens of thousands are safe (a value that would overflow leaves the column
empty, which scores 0). Because the games of one seed are adjacent, a seed that one AI wins in
every seating shows up as a streak: the map, not the seat, decided those games.

### The fun score

Each metric is scored against its band: **1 inside** the band (edges included), falling off
**linearly** outside it to 0 at `falloff` beyond the edge, `max(0, 1 − distance / falloff)`. A
metric with no value (no decided game, no defined margin) scores 0. `fun_score` is the
**weighted mean** of the six scores, `Σ wᵢ sᵢ / Σ wᵢ`. Scores are in `fun_<metric>_score`.

The bands live in the definition's `fun:` block; every key is optional and these are the
defaults (the shipped definitions spell them out):

```yaml
fun:
  playstyle: holdings          # or checkpoint: what fun_distinctness_score reads
  upset:        { min: 0.50, max: 0.75, falloff: 0.25, weight: 1 }   # R79: 50–75%
  lead_changes: { min: 2.0,             falloff: 2.0,  weight: 1 }   # R79: at least 2 per game
  comeback:     { min: 0.20, max: 0.40, falloff: 0.20, weight: 1 }   # R79: 20–40%
  margin:       {            max: 0.25, falloff: 0.25, weight: 1 }   # R79: under about 25%
  distinctness: { min: 0.30,            falloff: 0.30, weight: 1 }   # "differs clearly"
  streak:       {            max: 2.0,  falloff: 2.0,  weight: 1 }   # no improbable streaks
```

`min`/`max` are inclusive edges (leave one out for an open side), `falloff > 0` is in the
metric's unit, `weight ≥ 0` (at least one above 0). Distinctness is held to 0.30: on average
at least 30% of what two personalities own is in different categories. The streak band passes
anything up to z = 2 and scores 0 from z = 4.

**The bands only score games.** The raw metrics are written when a model finishes; the scores
are recomputed from them whenever the file is finalised. The resume fingerprint and the
recording's run id leave `fun:` out, so editing the bands and rerunning the same command
re-ranks a finished study without playing a game.

### Rescoring a recorded study: `study rescore`

```sh
sim-cli study rescore --recordings <s3|dir> --run <study-out dir | run id> --out <dir> \
    [--run-id <id>] [--def FILE] [--threads N]
```

Reads the run's manifests (`manifests/<run>/part-m<id>.parquet`) and replays through the
recorder's store (S3 or a local directory, docs/replay.md), rebuilds each game from its header
(the seats' brains give the seating), its checkpoints (the midnight boards) and its outcome,
and computes the fun metrics exactly as the live study does. Parts are read in parallel and
results collected in model order: the output is identical on any thread count, and memory holds
one model's games per thread.

- **With the study's output directory** (`--run study-out`): the definition, roster and bands
  come from its `summary.json` (`--def FILE` replaces the bands with that file's `fun:`); its
  `models.csv` rows are kept as they are and the fun columns replaced or appended; the models
  are re-ranked. The run id is `summary.json`'s `recording.run_id`; a study recorded before it
  was written there needs `--run-id`, or the store must hold exactly one study run. Models of
  `models.csv` with no part yet (a study still running) keep empty fun columns and are listed.
  A directory with no `summary.json` yet (a study still in its first chunk) works too, given
  the study's definition with `--def`; the roster then comes from the recordings.
- **From a run id alone** (`--run study-…`): every metric, R65 included, is computed from the
  outcomes (criteria and bands from `--def`, default R65 and the default bands); there are no
  parameter columns.

It writes `models.csv` and `summary.json` (with `rescore`: run id, models, games, missing) to
`--out`; it does not rewrite overlays. **A checkpoint counts only leases and businesses**, so a
rescore measures playstyle on those whatever the definition asks (`summary.json`
`fun.playstyle_asked` / `playstyle_used`) and leaves `fun_distinctness` empty. Every other fun
column equals the live study's; with `playstyle: checkpoint` in the definition the scores and
the ranking are equal too (`features/fun.feature`).

### Guarding against the winner's curse: `study confirm`

The best of 300 noisy models is flattered by its own noise. `study confirm` re-runs chosen
models on **fresh seeds** and puts their R65 and fun metrics next to the study's:

```sh
sim-cli study confirm config --from study-out --models 12,40,7 --games 2004 --seed 1000000 \
    --out study-out/confirm [--threads N] [--set ...]
```

The models are regenerated from the study's definition (its `summary.json`), so they are the
same models; only the game seeds change (`--seed`, which must not overlap the study's seeds;
`--games` defaults to the study's). Use the study's base config (directory and `--set`). It
writes the re-run's `models.csv` and `summary.json` and `confirm.csv`, one row per model and
metric (`model_id, metric, original, confirm`: `balanced`, the R65 shares and rates, the fun
metrics, `fun_score`, `rank`), and prints them. Existing flags cannot do this: `--seed` also
drives the hypercube, so a study with another seed samples other models.

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
   `idle_last_rate`, `balanced` (`true`/`false`), `score`, `rank` (R65 then fun, above), then
   `games_per_sec` (left out with `--no-wall-time`), then `idle_below_solvent_rate`;
4. per roster entry, in roster order: `<entry>_wins`, `<entry>_win_share`, `<entry>_win_low`,
   `<entry>_win_high`, `<entry>_bankrupt_rate`, `<entry>_nw_p10`, `<entry>_nw_median`,
   `<entry>_nw_p90` (net worth in pence);
5. the fun columns (R79), appended after every column above: `fun_strongest`,
   `fun_upset_rate`, `fun_lead_changes_mean`, `fun_decided`, `fun_comeback_rate`,
   `fun_margin_median`, `fun_distinctness`, `fun_distinctness_checkpoint`, `fun_streak_z`,
   `fun_streak_longest`, then the scores `fun_upset_score`, `fun_lead_changes_score`,
   `fun_comeback_score`, `fun_margin_score`, `fun_distinctness_score`, `fun_streak_score`,
   `fun_score`, then `evenness_rank`.

Shares and rates are fractions with six decimals, as are the fun metrics and scores (exact
fixed point). Invalid rows leave the metric columns empty; so does a missing idle seat for the
`idle_` columns, and an undefined fun metric (no decided game, say).

### `games.csv` (`--games-csv`)

One row per game: `model_id, game, seed, rotation, winner (entry | tie | none), days, ticks,
state_hash`, then per seat `seat<i>_entry, seat<i>_net_worth_pence, seat<i>_bankrupt`, then
`wall_ms` (left out with `--no-wall-time`), then the per-game fun columns (R79):
`lead_changes, halfway_day, halfway_leader` (the leading entry, entries joined by `+` on a
tie, empty if none) `, comeback` (`true`/`false`, empty if undecided) `, margin` (six
decimals, empty if undefined), then per seat `seat<i>_flats, seat<i>_shops, seat<i>_land,
seat<i>_hotels, seat<i>_other, seat<i>_leases, seat<i>_businesses`.

### `summary.json`

- `study`: the resolved definition, the `preset` actually run, `roster`, `seats`,
  `games_per_model` (after rounding), `seeds` (`first..end`);
- `models`: `total`, `finished`, `complete`, `ok`, `invalid`, `balanced`;
- `ranking`: every ok model in rank order: `rank`, `model_id`, `balanced`, `score`,
  `max_win_share`, `ai_bankruptcy_rate`, `idle_last_rate`, `idle_below_solvent_rate`,
  `fun_score`, `evenness_rank`;
- `top`: the ids that got an overlay (the best `top_n` balanced models);
- `invalid`: `model_id` and `error` per invalid model;
- `baseline`: model 0's row as an object (column → value);
- `columns`: the `models.csv` and `games.csv` headers of this study;
- `fun`: `definition` (the bands used) and `playstyle_used`;
- `recording` (only when recorded): `run_id`, which `study rescore` reads;
- `timing` (only with wall time): `wall_s`, `games_this_run`, `games_per_sec`, `threads`.

### Overlays: `models/<id>/`

Written when the study is complete, for the top `top_n` balanced models (older overlays are
removed):

- `economy.yaml`: the model's whole economy (R71). **To adopt a model, copy that one file over
  `config/`** and run `cargo validate`:

  ```sh
  cp study-out/models/<id>/economy.yaml config/
  ```

  The copied file reproduces the model exactly: a study of the same definition on the adopted
  config plays its model 0 with the same per-game state hashes as the model had
  (`features/study.feature`, "copying it over config/ reproduces its games"). A model that
  changes nothing (model 0) still gets the base `economy.yaml`, with its comments; any other
  model's is re-serialised, so the shipped comments are lost: keep them by applying
  `overrides.txt` by hand instead if you prefer;
- any other config file the model changes, the whole file: only `ai.yaml`, for a study that
  varies the AI tunables (`study-with-ai.yaml`). That is AI behaviour, not an economic model;
  copy it too to reproduce the run;
- `overrides.txt`: the same change as `file.key=value` lines (one `--set` each);
- `model.json`: the overrides and the model's `models.csv` row.

An overlay is relative to the study's base: the config directory plus any global `--set`. A
`--set` on `economy.yaml` is in every overlay's `economy.yaml`; one on another file is not, so
apply it too when you adopt such a model.

## Resume

`study.json` in the output directory holds a fingerprint of everything that decides the results
and the file layout: the definition, the base config files (all nine, `economy.yaml`
included), the preset, the roster and the
`--games-csv` / wall-time options. The definition's `fun:` block is left out (it only scores
games), so a definition without one fingerprints exactly as it did before R79. A restart with
the same fingerprint keeps every model already in `models.csv`, drops a line cut off mid-write
and any `games.csv` rows of unfinished models, and runs the rest. A different study in the same
directory is refused; use `--fresh` to discard it or choose another `--out`.

A `models.csv` written before the fun columns existed (the same header without them) still
resumes: its finished models keep empty fun columns (counted as a fun score of 0 in the
ranking, with a note on stderr), and `study rescore` fills them from the recordings. Rows of
an older `games.csv` have the old layout and are dropped on resume.

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
the extra compute goes on more models for its 26 parameters. At the rate above that is about
**2 hours**, before the cost of the full roster. Per-game cost scales with what the AI and the population do: expect it to
grow as more personalities, the full map, hotels and online land, and re-measure from
`summary.json` `timing` after a few models (the progress line prints an ETA).
