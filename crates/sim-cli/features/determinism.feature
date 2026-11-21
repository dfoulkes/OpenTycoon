# docs/engineering.md §3: same seed, config and commands give the same state on every OS,
# CPU and thread count. Balance runs rely on it (spec §11).
Feature: Batch determinism
  A batch's results depend only on the config, the seeds and the brains, never on how many
  threads ran it or in what order games finished.

  @spec-11
  Scenario: A batch gives the same CSV on 1 thread and on 4 threads
    Given the shipped config
    And the preset "v1"
    When I run a batch of 12 games from seed 0 with the "scripted" brain on 1 threads
    And I run a batch of 12 games from seed 0 with the "scripted" brain on 4 threads
    Then the two CSVs are identical without the wall time column

  @spec-11
  Scenario: A batch game ends in the same state as sim-cli run
    Given the shipped config
    And the preset "v1"
    When I run a batch of 3 games from seed 40 with the "idle" brain
    Then each game's state hash equals a plain run of that seed for the same ticks

  @spec-11
  Scenario: sim-cli batch on 1 and 3 threads writes identical files
    When I run sim-cli "batch config --games 6 --brain scripted --threads 1 --no-wall-time --out {tmp}/one.csv"
    And I run sim-cli "batch config --games 6 --brain scripted --threads 3 --no-wall-time --out {tmp}/three.csv"
    Then the files "one.csv" and "three.csv" are identical

  # R71 moved every economy lever into economy.yaml as a pure refactor: the shipped defaults
  # moved verbatim and every multiplier is exactly neutral at 1.0, so the full shipped game
  # ends in the same state as before (hash taken on the commit before R71). A deliberate
  # balance change moves this hash; say so in its PR and update it here. Street routing (shoppers
  # keep to the streets, cross the Thames by a bridge and wait at home between windows) moved it
  # from fd5bd3a7bcc696e4 on purpose.
  @spec-11 @R71
  Scenario: The shipped config plays the full game to the same state as before R71
    When I run sim-cli "run config --seed 42 --ticks 207360"
    Then sim-cli succeeds
    And its output contains "year=2050 day=24 hash=c2b3aca13bc5ee35"
