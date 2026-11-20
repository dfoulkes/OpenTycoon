# R64: balance is found by study: ~100 economic models (parameter sets), each tested over 500
# games of the full span. R65: a balanced model has every AI personality winning 10–40% of games
# (seats rotated), bankruptcy under 15% of player-games, and an idle player finishing below every
# AI that is still solvent, in every game; models are ranked by how even the win shares are. `sim-cli study` runs the study and measures; it
# implements no game rule (docs/study.md).
Feature: Balance study
  A study samples parameter sets with a Latin hypercube, plays every set over many games with
  the seats rotated, and reports which sets are balanced.

  # ---- Model generation ----------------------------------------------------------------------

  @R64
  Scenario: The same seed gives the same models, a different seed different ones
    Given a study of 10 models from seed 7
    And the parameter "lease" on "game.leases.term_years" from 10 to 60, uniform, integer
    And the parameter "overdraft" on "game.overdraft.start_limit" from 5000 to 50000, log-uniform, decimal
    When I generate the models twice
    Then both generations are identical
    And generating from seed 8 gives different models

  @R64
  Scenario Outline: Every sampled parameter covers its range, one model per stratum
    Given a study of 11 models from seed 3
    And the parameter "x" on "game.leases.term_years" from <min> to <max>, <distribution>, <kind>
    When I generate the models
    Then every value of "x" is between <min> and <max>
    And the 10 sampled models of "x" fall one in each of 10 equal strata

    Examples:
      | min  | max    | distribution | kind    |
      | 1    | 10     | uniform      | integer |
      | 0.5  | 2.5    | uniform      | decimal |
      | 100  | 100000 | log-uniform  | decimal |
      | 1    | 1000   | log-uniform  | integer |

  @R64
  Scenario: Model 0 is the shipped defaults
    Given the shipped config
    And a study of 5 models from seed 1
    And the parameter "lease" on "game.leases.term_years" from 10 to 60, uniform, integer
    When I generate the models
    Then model 0 has no overrides
    And model 0's config equals the shipped config
    And model 0's "lease" value is the shipped 30
    And model 1 overrides "game.leases.term_years"

  @R64
  Scenario: A scale parameter multiplies every number under its key
    Given the shipped config
    And a study of 3 models from seed 1
    And the scale parameter "births" on "customers.births_per_woman" from 0.5 to 1.5
    When I generate the models
    Then model 1 overrides every year under "customers.births_per_woman" by its "births" factor

  @R64
  Scenario Outline: A categorical parameter gives each value an equal share of the models
    Given the shipped config
    And a study of <models> models from seed <seed>
    And the categorical parameter "land" on "game.net_worth.land_value" with values "book_decay, market, market_no_decay"
    When I generate the models
    Then the <sampled> sampled models of "land" take each value between <least> and <most> times
    And every sampled model of "land" sets "game.net_worth.land_value" to its value

    Examples:
      | models | seed | sampled | least | most |
      | 13     | 3    | 12      | 4     | 4    |
      | 31     | 9    | 30      | 10    | 10   |
      | 12     | 1    | 11      | 3     | 4    |

  @R64
  Scenario: Model 0 shows the shipped category, and models.csv shows value names
    Given the shipped config
    And a study of 4 models of 2 games from seed 0 with the roster "grocer, idle" on preset "v1"
    And the categorical parameter "land" on "game.net_worth.land_value" with values "market, market_no_decay"
    When I run the study
    Then the study finishes with 4 models
    And models.csv shows "land" as "book_decay" for model 0
    And models.csv shows "land" as one of "market, market_no_decay" for models 1 to 3

  @R64
  Scenario: A parameter whose guard no longer holds stops the study before any game
    # Demand parameters address desire rules by position; a reordered rule must not be scaled
    # silently in its place.
    Given the shipped config with the first two "customers.yaml" desire rules swapped
    And a study of 3 models of 2 games from seed 0 with the roster "grocer, idle" on preset "v1"
    And the scale parameter "demand_1930s" on "customers.desires.0.goods" from 0.6 to 1.4
    And "demand_1930s" expects "customers.desires.0.match" to equal "{ window: day, from_year: 1930, until_year: 1945 }"
    When I try to run the study
    Then the study refuses to start, naming "parameters[0].expect[0]" and "demand_1930s"
    And no games were played

  @R64
  Scenario: A guard that holds lets the study run, whatever the key order
    Given the shipped config
    And a study of 2 models of 2 games from seed 0 with the roster "grocer, idle" on preset "v1"
    And the scale parameter "demand_1930s" on "customers.desires.0.goods" from 0.6 to 1.4
    And "demand_1930s" expects "customers.desires.0.match" to equal "{ until_year: 1945, from_year: 1930, window: day }"
    When I try to run the study
    Then the study finishes with 2 models

  # ---- Seats ---------------------------------------------------------------------------------

  @R65
  Scenario: Every entry of the roster sits in every seat equally often
    Given the roster "grocer, landlord, toff, idle"
    When I plan 12 games
    Then each of the 4 entries sits in each of the 4 seats 3 times
    And every rotation of a seed is played once

  @R65
  Scenario: Games per model round up to a whole number of rotations
    Given the roster "grocer, landlord, idle"
    When I plan 10 games
    Then 12 games are planned

  @R65
  Scenario: The default roster is every personality plus one idle seat
    Given the shipped config
    And a study of 2 models from seed 0
    When I resolve the roster
    Then the roster is every personality in ai.yaml followed by "idle"

  # ---- Metrics -------------------------------------------------------------------------------

  @R65
  Scenario Outline: Wilson 95% intervals on known counts
    When I take the Wilson interval of <wins> wins in <games> games
    Then it is <low> to <high> to 4 places

    Examples:
      | wins | games | low    | high   |
      | 0    | 10    | 0.0000 | 0.2775 |
      | 5    | 10    | 0.2366 | 0.7634 |
      | 10   | 10    | 0.7225 | 1.0000 |
      | 30   | 100   | 0.2189 | 0.3958 |
      | 1    | 500   | 0.0004 | 0.0112 |

  @R65
  Scenario: Evenness is 1 for equal win shares and falls as one personality dominates
    When the win counts are "25, 25, 25, 25"
    Then the normalised entropy is 1.0000 and the Gini index is 0.0000
    When the win counts are "50, 25, 25"
    Then the normalised entropy is 0.9464 and the Gini index is 0.1667
    When the win counts are "100, 0, 0, 0"
    Then the normalised entropy is 0.0000 and the Gini index is 0.7500

  # R65's idle rule is about the solvent AIs only: a bankrupt AI is covered by the bankruptcy
  # limit, so the idle seat finishing above it does not count against the model.
  @R65
  Scenario Outline: The idle seat must finish below every AI that is still solvent
    Given the roster "grocer, landlord, toff, idle"
    And a game that ends with grocer <grocer>, landlord <landlord>, toff <toff>, idle <idle>
    When I tally that game
    Then the idle seat <verdict> the idle rule
    And the model's idle_below_solvent_rate is <rate>

    Examples:
      | grocer        | landlord      | toff          | idle | verdict   | rate     |
      | 900           | 700           | -200 bankrupt | 500  | satisfies | 1.000000 |
      | 900           | 700           | 600           | 500  | satisfies | 1.000000 |
      | 900           | 400           | 700           | 500  | fails     | 0.000000 |
      | 900           | 500           | 700           | 500  | fails     | 0.000000 |
      | -100 bankrupt | -50 bankrupt  | -300 bankrupt | 500  | fails     | 0.000000 |

  # Bankrupt AIs no longer break the idle rule, so idle_last (idle strictly last, bankrupt AIs
  # included) is reported but does not decide the verdict: with 44 bankrupt AI player-games the
  # idle seat is above a bankrupt AI in 44 games and the model is still balanced.
  @R65
  Scenario Outline: The balanced flag follows R65 on synthetic results
    Given the roster "grocer, landlord, toff, idle"
    And 100 synthetic games where grocer wins <grocer>, landlord <landlord>, toff <toff>, idle <idle>
    And <bankrupt> AI player-games end bankrupt
    And the idle seat finishes below every solvent AI in <idle_below_solvent> games
    And the idle seat finishes strictly last in <idle_last> games
    When I compute the model's metrics
    Then the model is <verdict>

    Examples:
      | grocer | landlord | toff | idle | bankrupt | idle_below_solvent | idle_last | verdict      |
      | 34     | 33       | 33   | 0    | 0        | 100                | 100       | balanced     |
      | 10     | 40       | 40   | 0    | 0        | 100                | 100       | balanced     |
      | 9      | 41       | 40   | 0    | 0        | 100                | 100       | not balanced |
      | 20     | 41       | 39   | 0    | 0        | 100                | 100       | not balanced |
      | 34     | 33       | 33   | 0    | 45       | 100                | 55        | not balanced |
      | 34     | 33       | 33   | 0    | 44       | 100                | 56        | balanced     |
      | 34     | 33       | 33   | 0    | 0        | 99                 | 99        | not balanced |
      | 30     | 30       | 30   | 10   | 0        | 90                 | 90        | not balanced |

  # ---- Running -------------------------------------------------------------------------------

  @R64 @R65
  Scenario: A model whose config is invalid is reported, not fatal
    Given the shipped config
    And a study of 3 models of 2 games from seed 0 with the roster "grocer, idle" on preset "v1"
    And the parameter "bad" on "game.leases.term_years" from -50 to -10, uniform, integer
    When I run the study
    Then the study finishes with 3 models
    And model 0 is "ok"
    And models 1 and 2 are "invalid" naming "/leases/term_years"

  @R64
  Scenario: A restarted study skips the models already in models.csv
    Given the shipped config
    And a study of 4 models of 2 games from seed 0 with the roster "grocer, idle" on preset "v1"
    And the parameter "od" on "game.overdraft.start_limit" from 5000 to 20000, uniform, integer
    When I run the study, stopping after 2 models
    Then models.csv has 2 models
    When I run the study again
    Then 2 models were skipped and 2 were run
    And models.csv has 4 models
    And models.csv equals that of the same study run in one go

  @R64
  Scenario: A study gives the same CSVs on 1 thread and on 4 threads
    Given the shipped config
    And a study of 3 models of 4 games from seed 5 with the roster "grocer, idle" on preset "v1"
    And the parameter "od" on "game.overdraft.start_limit" from 5000 to 20000, uniform, integer
    When I run the study on 1 thread without wall time
    And I run the study on 4 threads without wall time
    Then the two runs wrote identical models.csv and games.csv

  @R64 @R65
  Scenario: sim-cli study writes the outputs and a balanced model's overlay is ready to copy
    When I run sim-cli "study config --def crates/sim-cli/tests/fixtures/study-loose.yaml --out {tmp}/out --threads 2 --no-wall-time --games-csv"
    Then sim-cli succeeds
    And the file "out/models.csv" has 4 lines
    And the file "out/games.csv" has 7 lines
    And "out/summary.json" lists 3 models ranked
    And the overlay of the top model loads when copied over the shipped config
