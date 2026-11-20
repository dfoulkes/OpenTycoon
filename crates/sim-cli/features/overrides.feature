# Spec §10b: "The balance harness (sim-cli) takes config overrides, so it can sweep a value
# across a run." R45: all values configurable in YAML, validated, failing loudly by key.
Feature: Config overrides for parameter sweeps
  A sweep changes one value on the command line. The changed config goes through the same
  schema and cross-file checks as a hand-edited file, so a bad value never reaches a game.

  @R45 @spec-11
  Scenario: An override changes the value the games run with
    Given the shipped config
    And the override "game.overdraft.start_limit=20000"
    When the harness loads the config
    Then the config loads
    And the overdraft start limit is £20000

  @R45 @spec-11
  Scenario: Year keys and list items can be overridden too
    Given the shipped config
    And the override "customers.births_per_woman.1930=2.5"
    And the override "game.world.lot_size.0=8"
    When the harness loads the config
    Then the config loads

  @R45 @spec-11
  Scenario Outline: A bad override fails loudly and names the key
    Given the shipped config
    And the override "<override>"
    When the harness loads the config
    Then loading fails naming "<named>"

    Examples:
      | override                          | named                       |
      | game.clock.real_minutes_per_day=0 | /clock/real_minutes_per_day |
      | game.overdraft.start_limt=1       | start_limt                  |
      | game.overdraft.start_limit=lots   | /overdraft/start_limit      |
      | game.nonsense.deeper=1            | /nonsense                   |
      | gaem.clock.start_year=1930        | gaem.yaml                   |
      | game.overdraft.start_limit        | file.key=value              |

  @R45 @spec-11
  Scenario: The v1 preset runs 1930 to 1950
    Given the shipped config
    And the preset "v1"
    When the harness loads the config
    Then the game runs from 1930 to 1950

  @R45 @spec-11
  Scenario: An explicit override beats the preset
    Given the shipped config
    And the preset "v1"
    And the override "game.clock.end_year=1940"
    When the harness loads the config
    Then the game runs from 1930 to 1940

  @R45 @spec-11
  Scenario: An unknown preset fails and lists the known ones
    Given the shipped config
    And the preset "v9"
    When the harness loads the config
    Then loading fails naming "v1"

  @R45 @spec-11
  Scenario: sim-cli batch refuses an invalid override and exits non-zero
    When I run sim-cli "batch config --games 1 --set game.clock.real_minutes_per_day=0"
    Then sim-cli fails
    And its error output contains "game.yaml"
    And its error output contains "/clock/real_minutes_per_day"
