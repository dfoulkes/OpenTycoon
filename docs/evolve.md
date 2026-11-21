# Evolving the AI personalities

Step one of making the AIs smarter the more they play: `sim-cli evolve` evolves each AI
personality's **settings** (its `ai.yaml` tunables: aggression, buffers, opportunism thresholds,
rent rules) over generations of games. A neural "Apprentice" trained from the recorded games may
come later; it is not part of this.

Like the balance study ([`study.md`](study.md)) it implements no game rule. A genome is a config
overlay validated exactly like `--set`; every game is an ordinary `World`; the brains still act
only through public commands. Evolution changes settings, never code. The code is
`crates/sim-cli/src/evolve/`; the scenarios are `crates/sim-cli/features/evolve.feature`.

```sh
cargo run --release -p sim-cli -- evolve config                      # study/evolve.yaml → evolve-out/
cargo run --release -p sim-cli -- evolve config --generations 3 --population 6 --games 24 \
    --preset v1 --out evolve-out/smoke                                # a quick look
cargo run --release -p sim-cli -- evolve config --economy study-out/models/12/economy.yaml
cargo run --release -p sim-cli -- evolve config --record recordings   # record every game
```

Interrupt it at any time: rerun the same command and it resumes (below). `--stop-after N` runs N
more generations and stops, for chunked runs.

## The definition file

`study/evolve.yaml` (or `--def FILE`). Parsing is strict, like the study's: an unknown key, a
wrong type or a bad value fails before any game, naming the key path (`genes[3].min`). Every key
but `genes` is optional:

| Key | Default | Meaning |
|---|---|---|
| `evolve` | every personality in the roster | the personalities that evolve; the others play their base settings |
| `genes` | required | each personality's genes (below) |
| `population` | 16 | genomes per personality per generation (at least 2) |
| `generations` | 20 | generations |
| `games_per_eval` | 120 | games per candidate, rounded **up** to whole seat rotations |
| `elitism` | 2 | best genomes per personality carried over unchanged; below `population` |
| `selection` | `{ method: tournament, tournament_size: 3 }` | `tournament` or `rank` |
| `mutation` | `{ rate: 0.25, sigma: 0.1 }` | per-gene chance, and the step as a share of the gene's range |
| `crossover` | `{ rate: 0.5 }` | chance a child has two parents; 0 turns crossover off |
| `fitness` | `{ bankruptcy_penalty: 0.5, low_win_floor: 0.10, net_worth_scale: 700000, max_net_worth_weight: 0.8 }` | the fitness weights (below) |
| `economy` | none: `config/economy.yaml` | path to an adopted `economy.yaml` (e.g. a study overlay), relative to the working directory |
| `preset` | `full` | preset from `game.yaml`; if missing, the longest preset is used (with a warning) |
| `seed` | 0 | drives the first population, the breeding and the game seeds |
| `roster` | every personality in `ai.yaml` (id order), then `idle` | the seats, as in the study |
| `record` | none | record every game here (`s3` or a local directory, docs/replay.md) |

`--generations`, `--population`, `--games`, `--elitism`, `--preset`, `--seed`, `--evolve a,b`,
`--roster a,b`, `--economy` and `--record` override the file. `--set file.key=value` changes the
base config (after the adopted economy).

### Genes

Genes use the study's parameter format ([`study.md`](study.md) "Parameters": `name`, `key` or
`keys`, `min`/`max` or `values`, `distribution`, `kind`, `decimals`, `mode`, `optional`,
`expect`), with three extra rules:

- every key is one personality's setting, `ai.<personality>.<key>` (e.g. `ai.toff.aggression`,
  `ai.speculator.opportunism.threshold`), and all keys of one gene belong to the same
  personality; `ai.personalities`, `ai.opponents` and `ai.decision_interval_ticks` are not
  genes;
- **the Grocer's opportunism is never a gene** (R66: the Grocer stays pure retail): any key
  under `ai.grocer.opportunism` is refused, and an evolution refuses to start on a base config
  whose Grocer has `opportunism.enabled: true`. Every candidate config is checked again before
  it plays;
- a key must exist in the base config (unless `optional`), and a gene whose personality does
  not evolve in this run is skipped with a note.

A genome is one value per gene of its personality, in definition order. Values are always
**canonical**: clamped to `[min, max]` and rounded as the gene asks (whole numbers for
`integer`, `decimals` places otherwise, a category index for `values`). So **genes never leave
their ranges**, and a genome's text in `generations.csv` reads back bit for bit.

A genome the config loader rejects (out of a key's range, or breaking a cross-field rule such as
`min_rent ≤ max_rent`) is **invalid**: it is reported in `generations.csv` with the loader's
message (naming the key), never ranked, never a champion, an elite or a parent, and the run
carries on.

## The algorithm

Per evolving personality, a population of `population` genomes, generation by generation:

1. **Generation 0.** Candidate 0 is the **base genome** (the base config's settings, clamped to
   the ranges: a gene the base holds no single number for starts mid-range); candidates
   `1..population` are a Latin hypercube over the genes (the study's sampler, seeded per
   personality), so every stratum of every gene is tried once.
2. **Evaluation.** Each candidate is seated against the **current champions of the other
   evolving personalities**, the base settings of any personality that does not evolve, and
   the idle seat: its genome and the champions' genomes are applied as `--set` overrides on the
   base config, with the preset. It plays `games_per_eval` games (rounded up to whole rotations
   of the `n` seats) in blocks of `n`: block `b` of generation `g` uses seed
   `seed + g · blocks + b` and plays it once per rotation, seat `s` holding roster entry
   `(s + r) mod n`, exactly as the study does. Every candidate of a generation plays the same
   seeds (common random numbers), so candidates differ only by their genomes; each generation
   plays fresh seeds, so a genome cannot overfit a fixed set of games.
3. **Ranking and champions.** Valid candidates are ranked by fitness (below). Rank 1 is the
   personality's **champion** of the generation. If every candidate is invalid, the previous
   champion stays.
4. **Lineup.** The new champions of every personality play the generation's seeds together:
   the **lineup** games. They show how the champions do against each other
   (`champions/<gen>/lineup.json`) and are the games the champions' `ai.yaml` reproduces.
5. **Breeding** generation `g + 1`, per personality, from generation `g`'s valid candidates in
   rank order:
   - **elitism**: the top `elitism` are copied unchanged (origin `elite`);
   - each **child** picks a parent by **tournament** (best of `tournament_size` uniform draws
     with replacement) or **rank** selection (the candidate ranked `r` of `k` drawn with weight
     `k − r + 1`); with chance `crossover.rate` it picks a second parent and takes each gene
     from either with even odds (**uniform crossover**);
   - then **Gaussian mutation**: each gene with chance `mutation.rate` takes a step of
     `z · sigma · (max − min)`, `z` standard normal (Box–Muller through `libm`); a log-uniform
     gene steps in log space, `exp(ln v + z · sigma · (ln max − ln min))`; an integer gene
     moves at least one whole step in the direction of `z`; a categorical gene takes a
     different value, uniformly. The result is clamped and rounded (canonical). A child none of
     whose genes came up mutates one gene picked uniformly, so no child is a plain copy.
   - With no valid candidate at all, children are bred from the base genome.

Every draw comes from sim-core's portable ChaCha8 helpers, one stream per purpose, personality
and generation (the run seed mixed with a stable hash of those), so the same seed breeds the
same genomes on any machine, in any thread count, and a resumed run breeds what an uninterrupted
one would.

### Coevolution: simultaneous, against frozen champions

The personalities evolve **together, each against the previous generation's frozen champions**
(generation 0 against the base settings), not in alternating rounds. Why:

- every candidate of a generation faces the same opponents, so their fitnesses are comparable,
  and all of a generation's games (personalities × population × games) are independent: one
  parallel batch, deterministic on any thread count, with a generation as the natural unit to
  resume;
- alternating rounds would take one sequential round per personality for each improvement of
  all five, and would favour whoever evolves first in each cycle;
- the cost is the classic risk of coevolution, cycling (A adapts to B, B to A, and back). Elites
  are re-evaluated each generation against the new champions on fresh seeds, and the lineup
  games give an absolute check: watch `lineup_win_share` in `summary.json` alongside fitness.

## Fitness

For a candidate of personality `P` over its `N` evaluation games:

| Term | Definition |
|---|---|
| `wins` | games `P`'s seat won **outright**: highest net worth among solvent players at the whistle (a tie at the top, or nobody solvent, is nobody's win), as in the study |
| `win_share` (`s`) | `wins / N` |
| `bankrupt_rate` (`b`) | games `P`'s seat ended bankrupt / `N` |
| `nw_vs_idle_median` (`m`) | the median over the games of (`P`'s final net worth − the idle seat's), in pence (nearest rank: the lower middle for an even `N`); `P`'s own median net worth if the roster has no idle seat |
| `c` | the net-worth term: `clamp(m / net_worth_scale, −1, 1)` |
| `ω` | the net-worth weight: 0 at or above the floor `f`; below it `max_net_worth_weight × (f − s) / f`, rising linearly from 0 at the floor to `max_net_worth_weight` at a win share of 0 |
| **`fitness`** | at or above the floor: `s − p·b`; below it: `(1 − ω)·s + ω·f·c − p·b` |

Net worth is the economy's final midnight board, as `sim-cli batch` and the study score it.

The weights, under `fitness:` in the definition:

| Key | Default | Rule | Meaning |
|---|---|---|---|
| `bankruptcy_penalty` (`p`) | 0.5 | 0 to 1,000 | cost of one unit of bankruptcy rate |
| `low_win_floor` (`f`) | 0.10 | 0 to 1 | below this win share, net worth is blended in; 0 turns the blend off |
| `net_worth_scale` | 700000 | positive, whole pence | the median margin over idle that counts as a full net-worth term |
| `max_net_worth_weight` | 0.8 | 0 to 1 | the net-worth weight at a win share of 0 |

The shares and weights take at most six decimal places; a value out of range, with more
decimals, or a scale of 0 or less fails before any game, naming the key
(`fitness.low_win_floor: must be between 0 and 1`).

**Why the blend (R78).** When one personality wins every game, every other personality's win
share is 0 and the old fitness (`s − p·b`) gives all their candidates the same score; selection
then ran on the tie-break alone and the fitness curves were flat at 0. Below the floor the
blend moves the score onto net worth gained over idle, so those candidates have a real,
graded fitness: a genome that builds wealth beats one that loses it, and a lucky win in 24
games no longer outranks a genome that finishes far further ahead of idle. The net-worth term
is multiplied by the floor, so `f·c` never exceeds `f`: a candidate below the floor never
scores above one at the floor with the same bankruptcies, and fitness is continuous at the
floor (`ω = 0` there). Winning stays what the game is about.

**Defaults.** At or above the floor `ω = 0` and fitness is exactly `s − p·b`, so any personality
winning at least 10% of its games ranks exactly as before. 0.10 is the bottom of the balance
target (every AI wins 10–40% of games, R65): a personality below it is the one that cannot win
as things stand. The scale is the starting cash (£7,000, R62): a candidate that finishes a full
starting stake ahead of idle earns the whole term, and the v1 medians observed (within about
±£4,000) stay inside the clamp, so it rarely saturates. The weight 0.8 leaves a fifth of a
zero-or-near-zero win share's say to the wins themselves.

**Ranking**, best first: a candidate with no games last; then higher `fitness`; then (the
tie-break) higher `nw_vs_idle_median`; then the lower candidate index. It is a total order on
integers. The weights are converted to whole parts per million when the definition loads, and
the fitness is computed in fixed point (units of 10⁻¹²) from the integer counts and pence with
`i128` intermediates and division rounded toward −∞; no float enters the ranking. At or above
the floor the fitness is one division (`(wins·10¹² − p_ppm·bankruptcies·10⁶) / N`), so
candidates with the same number of games (all of a run's do) compare exactly as their rational
fitnesses do for up to 10⁶ games per evaluation. Below the floor it takes three such steps, so
it may sit up to three units (3 × 10⁻¹²) below the exact value; that only matters between
candidates whose exact fitnesses agree to 12 decimals, which then fall to the tie-break. So
the ranking is identical on every platform and thread count, and a resumed run (which reads
the counts back from `generations.csv`) ranks exactly as an uninterrupted one. Floats appear
only in the printed `fitness` column and the curves in `summary.json`.

Why this shape: winning is what the game is about, so win share leads. A bankrupt player cannot
win, so bankruptcy already costs wins; the penalty also discourages reckless strategies that
win now and then and go bust often, and it applies below the floor too. With few games many
candidates tie on win share, and the median margin over the idle seat (a player who never
acts) separates a strategy that builds wealth from one that loses it, without letting one
freak game decide.

## Outputs

All under `--out` (default `evolve-out/`, ignored by git). Columns are a contract: new ones are
added at the end, never renamed or reordered.

### `generations.csv`

One row per candidate, appended one generation at a time, sorted by generation, personality
(roster order) and candidate:

`gen, personality, candidate, origin (base | sample | elite | child), parents (the previous
generation's candidate indices, + between two), genome_id, status (ok | invalid), error,
genome (name=value;… in gene order), games, wins, win_share, bankruptcies, bankrupt_rate,
nw_vs_idle_median, fitness, rank, champion (true | false), games_digest`

- `genome_id` is the **brain version** the recorder writes for that seat
  (`replay::record::brain_version`): the stable hash, 16 hex digits, of the personality's
  `personalities.<id>` weights and its whole `<id>:` block of tunables with the genome in place.
  Training data can therefore say which variant played: a replay seat's `brain_version` is the
  `genome_id` of the candidate (or champion) that sat there.
- `games_digest` is a stable hash of the candidate's games' final state hashes, in game order:
  two runs agree on it exactly when they played the same games.
- Shares and the fitness have six decimals; `rank` is 1-based among the valid candidates.
  Invalid rows leave the metric columns empty.

No column holds wall time, so two runs of the same definition diff byte for byte.

### `champions/<gen>/`

- `ai.yaml`: the base config's whole `ai.yaml` with every evolving personality's champion of that
  generation in place. **To adopt it, copy that one file over `config/`** and run
  `cargo validate`:

  ```sh
  cp evolve-out/champions/<gen>/ai.yaml config/
  ```

  It is re-serialised, so the shipped comments are lost. With an adopted `economy` or a
  `--set`, apply those too.
- `lineup.json`: the champions (`candidate`, `genome_id`, `genome`, `fitness`), the lineup
  games' results per roster entry (`wins`, `win_share`, `bankruptcies`, `bankrupt_rate`,
  `nw_vs_idle_median`), `no_winner`, and every lineup game (`game`, `seed`, `rotation`,
  `winner`, `state_hash`) with their `games_digest`. A copied `ai.yaml` reproduces these games:
  a study of the same seeds and roster on the adopted config plays them with the same state
  hashes (`features/evolve.feature`, "A champion ai.yaml loads and reproduces its evaluation
  games"). When only one personality evolves, the lineup is exactly its champion's evaluation,
  so the digests match too. `lineup.json` is written last: it marks the generation finished.

### `lineup_fun.json`

Choosing the shipped line-up by fun (R79): the strongest evolved settings make a harder
difficulty tier, while the line-up players meet by default should be the one whose games are
most fun. `lineup_fun.json` scores each finished generation's champion line-up with the study's
fun metrics ([`study.md`](study.md) "Fun (R79)") over its lineup games: one entry per
generation, in order, with `generation`, `games`, the lineup's R65 verdict and shares (`r65`:
`balanced`, `max_win_share`, `min_win_share`, `ai_bankruptcy_rate`,
`idle_below_solvent_rate`), the raw metrics (`fun`: `strongest`, `upset_rate`,
`lead_changes_mean`, `decided`, `comeback_rate`, `margin_median`, `distinctness`,
`distinctness_checkpoint`, `streak_z`, `streak_longest`), each metric's band score (`scores`)
and `fun_score`, with the default bands (`bands`). Pick the generation whose champions'
`ai.yaml` to ship by `fun_score` among those whose lineup is `balanced`.

It is output only: it reads the lineup games the generation already played and changes nothing
about fitness, selection or breeding. It is written just before the generation's
`lineup.json`, replacing any entry of that generation or later, so a resumed run ends with the
same file as a run in one go. With 2 to 3 lineup games per generation in a smoke run the numbers
are noise; judge them on a real run's `games_per_eval`.

### `summary.json`

- `evolution`: the resolved definition, the preset run, the roster, the evolving personalities,
  `games_per_eval` (after rounding) and the coevolution scheme;
- `generations`: `total`, `finished`, `complete`;
- `personalities.<id>`: the **fitness curve**, one entry per finished generation:
  `best_fitness`, `mean_fitness` (over valid candidates), `champion` (genome id),
  `champion_win_share`, `invalid` (count), and `lineup_win_share` (the champion's win share in
  the lineup games);
- `columns`: the `generations.csv` header;
- `timing` (unless `--no-wall-time`): `wall_s`, `games_this_run`, `games_per_sec`, `threads`.

Progress and an ETA go to stderr after each generation (and every tenth of a large one).

## Recording

With `--record <dir>` (or `record:` in the definition) every game is recorded through the
existing recorder ([`replay.md`](replay.md)): one manifest part per candidate,
`part-g<gen>-<personality>-c<candidate>`, game ids `g<gen>-<personality>-c<candidate>-n<game>`,
and one per generation for the lineup, `part-g<gen>-lineup`; manifest `source` is `evolve`. Each
candidate's config snapshot is stored once by hash. A seat's `brain_version` is the genome id
(above). The run id defaults to a hash of the definition and base config, so a resumed run
overwrites its own parts with identical bytes; a resumed run records only the generations it
runs. Recording never changes a game.

## Resume

`evolve.json` holds a fingerprint of everything that decides the results: the definition (bar
`record`), the base config files (all nine, after the adopted economy and any `--set`), the
preset, the roster, the evolving personalities and the wall-time option. A restart with the same
fingerprint keeps every finished generation (its `champions/<gen>/lineup.json` exists and all
its rows are in `generations.csv`), drops the rows and champion files of an unfinished one,
and breeds on from the last finished generation's rows. The result is identical to a run in one
go (`features/evolve.feature`, "A resumed evolution equals one run in one go"). A different
evolution in the same directory is refused; use `--fresh` to discard it or choose another
`--out`.

## Cost

A generation plays `personalities × population × games_per_eval` evaluation games plus
`games_per_eval` lineup games. With five evolving personalities:

`games = generations × games_per_eval × (5 × population + 1)`

Measured on a 10-thread development machine, release build, all threads (CPU time per game,
which is what scales with cores):

| Preset | Run | Games | Wall | CPU time per game |
|---|---|---|---|---|
| `v1` (1930–1950) | 3 generations × population 6 × 24 games | 2,232 | 17.1 s | ≈ 0.05 s |
| `full` (1930–2050) | 1 generation × population 4 × 12 games | 252 | 16.7 s | ≈ 0.54 s |

On a 32-core machine at that cost per game, a full-preset game costs about 0.54 / 32 ≈ 17 ms
of wall time, ideally; allow about 30% for slower cores and scaling losses (≈ 24 ms).

| Definition (full preset) | Games | 32 cores, ideal | 32 cores, with 30% losses |
|---|---|---|---|
| shipped `study/evolve.yaml`: 30 × 120 × population 16 | 291,600 | ≈ 1.4 h | ≈ 2 h |
| **overnight, about 8 h**: 40 × 240 × population 24 | 1,161,600 | ≈ 5.5 h | ≈ 7.8 h |

For an overnight run, spend the extra budget on **games per evaluation** first (at 120 games
a win share near 0.5 has a standard error of about 4.5 points, at 240 about 3.2, so fewer
selection mistakes), then on population (more of each gene's range tried per generation):

```sh
cargo run --release -p sim-cli -- evolve config --generations 40 --population 24 --games 240 \
    --out evolve-out/overnight
```

It resumes after an interruption, so a run cut short by the morning is not lost: rerun the same
command to finish it, or adopt the last finished generation's champion.

What the small `v1` run above showed (seed 0): Sir Penny Sugar's champion won 20 of 24 games
in generation 0 and every game from generation 1, so the other personalities' fitness fell to
0 and their selection ran on the tie-break alone. That is coevolution doing its job (one
personality found a strategy the others' current settings cannot answer) but it also means a
short preset can be dominated by one personality; judge the overnight run on the full preset
and watch `lineup_win_share`.

The same run with the R78 blend (shipped weights) played the same games and crowned the same
champions; only the `fitness` column changed. The other personalities' curves are no longer
flat at 0 (in generations 1–2: Grocer best 0.021/0.022, Landlord 0.014/0.016, Speculator
−0.005/−0.003; the Toff stays at 0, its median margin over idle being exactly 0 in every v1
game), but their champions' net worth over idle did not improve across generations (Grocer
£1,922 → £1,853 → £1,921, Landlord £1,946 → £1,268 → £1,428, Speculator £638 → −£396 → −£227),
exactly as before. The reason: when every candidate of a personality has 0 wins, the blend
orders them by `c`, which is the old tie-break's order while no median reaches the clamp, so
the ranking cannot change. The blend changes selection only where win shares below the floor
differ (a lucky win or two against more wealth), which in this run did not alter a rank.
