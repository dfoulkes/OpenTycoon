# R79: "best" means most fun, not most efficient. A model must pass R65; passing models are then
# ranked by a fun score built from the upset rate (50–75%), at least 2 lead changes per game,
# the comeback rate (20–40%), a median winning margin under about 25%, distinct playstyles per
# personality and no improbable winning streaks. `sim-cli study` measures them; it implements no
# game rule (docs/study.md "Fun (R79)").
Feature: The fun score
  Each game is measured from its midnight boards while it plays (or from a recording's
  checkpoints); each model's games give six metrics, each scored against a target band, and
  the weighted mean ranks the models that pass R65.

  # ---- One game ------------------------------------------------------------------------------

  # Net worths per midnight, seats separated by "/", midnights by ","; "x" marks a bankrupt seat.
  @R79
  Scenario Outline: Lead changes are counted across the midnights
    Given a game whose midnight net worths are "<boards>"
    And seat <winner> wins outright
    When I measure the game's fun
    Then it has <changes> lead changes

    Examples:
      | boards                                                   | winner | changes |
      | 100/90/80, 100/120/80, 130/130/80, 150/140/80, 150/140/160, 150/140/170 | 2 | 3 |
      | 100/90, 110/90, 120/90, 130/90                          | 0      | 0       |
      | 100/90, 90/100, 100/90, 90/100                          | 1      | 3       |
      | 100/90, 100/100, 100/90                                 | 0      | 0       |
      | 900x/100/90, 900x/90/100, 900x/80/120                   | 2      | 1       |

  @R79
  Scenario Outline: A comeback is a winner who was not leading at the halfway midnight
    Given a game whose midnight net worths are "<boards>"
    And seat <winner> wins outright
    When I measure the game's fun
    Then the halfway midnight is day <day>
    And the game <verdict> a comeback

    Examples:
      | boards                          | winner | day | verdict |
      | 100/90, 80/120, 90/100, 150/140 | 0      | 2   | is      |
      | 100/90, 120/80, 90/100, 150/140 | 0      | 2   | is not  |
      | 100/100, 100/100, 1/2, 9/1      | 0      | 2   | is not  |
      | 100/90, 80/120, 90/100, 150/150 | none   | 2   | is undecided as |

  @R79
  Scenario Outline: The winning margin is (winner − runner-up) / winner
    Given a game whose midnight net worths are "<final>"
    And seat <winner> wins outright
    When I measure the game's fun
    Then the winning margin is <margin>

    Examples:
      | final         | winner | margin   |
      | 1000/750/200  | 0      | 0.250000 |
      | 400/1000x/300 | 0      | 0.250000 |
      | 1000/999      | 0      | 0.001000 |
      | 500/-200x     | 0      | 1.000000 |

  # ---- One model -----------------------------------------------------------------------------

  @R79
  Scenario: The upset rate counts the games the strongest AI did not win
    Given the roster "grocer, landlord, toff, idle"
    And the winners in seed order are "grocer, grocer, landlord, toff, grocer, tie, idle, none, landlord, grocer"
    When I compute the model's fun
    Then the strongest AI is "grocer"
    And the upset rate is 0.600000

  @R79
  Scenario: The streak statistic flags an improbable winning streak
    Given the roster "grocer, landlord, toff, idle"
    And the winners in seed order are "grocer*12, landlord*12, toff*12"
    When I compute the model's fun
    Then the streak statistic is above 2.0
    And the longest winning streak is 12
    And the streak scores 0.000000

  @R79
  Scenario: A random-looking winner sequence passes the streak test
    Given the roster "grocer, landlord, toff, idle"
    And the winners in seed order are "grocer, toff, toff, landlord, grocer, landlord, toff, grocer, grocer, landlord, toff, landlord, grocer, toff, landlord, landlord, grocer, toff, grocer, landlord, toff, toff, grocer, landlord, landlord, toff, grocer, toff, landlord, grocer, grocer, toff, landlord, grocer, toff, landlord"
    When I compute the model's fun
    Then the streak statistic is between -2.0 and 2.0
    And the streak scores 1.000000

  # Holdings per game, "flats/shops/land/hotels/other".
  @R79
  Scenario Outline: Distinctness is high for distinct holdings and low for identical ones
    Given the roster "grocer, landlord, speculator, idle"
    And over 6 games grocer holds "<grocer>", landlord holds "<landlord>" and speculator holds "<speculator>"
    When I compute the model's fun
    Then the playstyle distinctness is <distinctness>

    Examples:
      | grocer     | landlord   | speculator | distinctness |
      | 0/10/0/0/0 | 10/0/0/0/0 | 0/0/10/0/0 | 1.000000     |
      | 2/5/3/0/0  | 2/5/3/0/0  | 4/10/6/0/0 | 0.000000     |
      | 0/10/0/0/0 | 0/5/5/0/0  | 0/0/10/0/0 | 0.666667     |
      | 0/0/0/0/0  | 0/0/0/0/0  | 0/0/0/0/0  | 0.000000     |

  # ---- Scoring and ranking -------------------------------------------------------------------

  @R79
  Scenario Outline: A metric scores 1 inside its band and falls off linearly outside it
    Given the band from <min> to <max> with a falloff of <falloff>
    When I score <value> against it
    Then the score is <score>

    Examples:
      | min  | max  | falloff | value | score    |
      | 0.50 | 0.75 | 0.25    | 0.60  | 1.000000 |
      | 0.50 | 0.75 | 0.25    | 0.50  | 1.000000 |
      | 0.50 | 0.75 | 0.25    | 0.75  | 1.000000 |
      | 0.50 | 0.75 | 0.25    | 0.40  | 0.600000 |
      | 0.50 | 0.75 | 0.25    | 0.90  | 0.400000 |
      | 0.50 | 0.75 | 0.25    | 0.10  | 0.000000 |
      | 2    | -    | 2       | 1     | 0.500000 |
      | 2    | -    | 2       | 7     | 1.000000 |
      | -    | 0.25 | 0.25    | 0.30  | 0.800000 |

  @R65 @R79
  Scenario: Models that pass R65 come first, then the most fun, then the evenness tie-breaks
    Given these finished models
      | model | balanced | fun_score | score |
      | 1     | false    | 0.950000  | 0.99  |
      | 2     | true     | 0.400000  | 0.99  |
      | 3     | true     | 0.800000  | 0.50  |
      | 4     | true     | 0.800000  | 0.90  |
    When I rank the models
    Then the ranking is "4, 3, 2, 1"
    And the evenness-only ranking is "2, 4, 3, 1"

  # ---- Running -------------------------------------------------------------------------------

  @R79
  Scenario: Measuring fun does not change the game
    Given the shipped config
    When I play seed 5 of the roster "grocer, landlord, idle" at rotation 1 on preset "v1" measuring fun, and again plainly
    Then both games end with the same state hash
    And the measured game has fun data for 4 midnights and 3 seats

  @R64 @R79
  Scenario: The fun columns are the same on 1 thread and on 4 threads
    Given the shipped config
    And a study of 2 models of 6 games from seed 3 with the roster "grocer, landlord, idle" on preset "v1"
    When I run the study on 1 thread without wall time
    And I run the study on 4 threads without wall time
    Then the two runs wrote identical models.csv and games.csv
    And every ok model has a fun score

  @R79
  Scenario: rescore on a local recording equals the fun columns measured live
    When I run sim-cli "study config --def crates/sim-cli/tests/fixtures/study-fun.yaml --out {tmp}/out --threads 2 --no-wall-time --games-csv --record {tmp}/rec"
    Then sim-cli succeeds
    When I run sim-cli "study rescore --recordings {tmp}/rec --run {tmp}/out --out {tmp}/re1 --threads 1"
    Then sim-cli succeeds
    And every fun column but "fun_distinctness", and the ranks, of "re1/models.csv" equal those of "out/models.csv"
    When I run sim-cli "study rescore --recordings {tmp}/rec --run {tmp}/out --out {tmp}/re4 --threads 4"
    Then the files "re1/models.csv" and "re4/models.csv" are identical
    And the files "re1/summary.json" and "re4/summary.json" are identical

  @R79
  Scenario: rescore measures playstyle on leases and businesses, from a run id alone or before summary.json exists
    When I run sim-cli "study config --def crates/sim-cli/tests/fixtures/study-loose.yaml --out {tmp}/out --threads 2 --no-wall-time --record {tmp}/rec --run-id test-fun"
    Then sim-cli succeeds
    When I run sim-cli "study rescore --recordings {tmp}/rec --run {tmp}/out --out {tmp}/re"
    Then sim-cli succeeds
    And the raw fun columns of "re/models.csv" equal those of "out/models.csv" except "fun_distinctness"
    And "re/summary.json" says the playstyle was measured on "checkpoint"
    When I run sim-cli "study rescore --recordings {tmp}/rec --run test-fun --out {tmp}/bare"
    Then sim-cli succeeds
    And the raw fun columns of "bare/models.csv" equal those of "re/models.csv" except "fun_distinctness"
    # A study still in its first chunk has no summary.json yet: the definition comes from --def
    # and the run is the store's one study run.
    When I remove "out/summary.json"
    And I run sim-cli "study rescore --recordings {tmp}/rec --run {tmp}/out --def crates/sim-cli/tests/fixtures/study-loose.yaml --out {tmp}/early"
    Then sim-cli succeeds
    And the raw fun columns of "early/models.csv" equal those of "re/models.csv" except "fun_distinctness"

  @R79
  Scenario: confirm re-runs chosen models on fresh seeds next to the study's numbers
    When I run sim-cli "study config --def crates/sim-cli/tests/fixtures/study-fun.yaml --out {tmp}/out --threads 2 --no-wall-time"
    And I run sim-cli "study confirm config --from {tmp}/out --models 0,2 --games 6 --seed 1000 --out {tmp}/confirm --threads 2 --quiet"
    Then sim-cli succeeds
    And the file "confirm/confirm.csv" has 27 lines
    And "confirm/summary.json" played seeds "1000..1002"
    When I run sim-cli "study confirm config --from {tmp}/out --models 1 --seed 1 --out {tmp}/again --quiet"
    Then sim-cli fails
    And its error output contains "overlap"

  @R79
  Scenario: evolve scores each generation's champion line-up by fun
    When I run sim-cli "evolve config --def crates/sim-cli/tests/fixtures/evolve-fun.yaml --out {tmp}/evo --quiet --no-wall-time"
    Then sim-cli succeeds
    And "evo/lineup_fun.json" scores generations 0 and 1, each with a fun score and an R65 verdict
