# docs/replay.md: every game is recorded (header, command log, midnight checkpoints, outcome)
# so a learning AI can be trained from it later. The recorder is harness tooling (spec §11): it
# implements no game rule and must never change a game. Determinism (docs/engineering.md §3)
# is what makes a replay re-simulate bit for bit.
Feature: Game recorder
  Every game's inputs are recorded into an object store, indexed by Parquet manifests, and can
  be re-simulated to prove the recording is complete.

  Background:
    Given the shipped config
    And the preset "v1"
    And a build whose rules fingerprint is "00c0ffee00c0ffee"

  @spec-11
  Scenario: A recorded game re-verifies
    When I record a batch of 3 games from seed 5 with the "scripted" brain in 6 seats
    Then the manifest lists 3 games
    And every recorded game re-verifies at every midnight

  @spec-11
  Scenario: Recording does not change the game
    When I run a batch of 4 games from seed 20 with the "scripted" brain in 6 seats
    And I record a batch of 4 games from seed 20 with the "scripted" brain in 6 seats
    Then each recorded game ends with the same state hash as the unrecorded one

  @spec-11
  Scenario Outline: A tampered checkpoint fails verification naming that midnight
    When I record a batch of 1 games from seed 9 with the "scripted" brain
    And I tamper with the state hash of midnight <day> of the first game
    Then verifying the first game fails with "diverges at midnight <day>"

    Examples:
      | day |
      | 1   |
      | 3   |

  @spec-11
  Scenario: A tampered command fails verification at the first midnight after it
    When I record a batch of 1 games from seed 9 with the "scripted" brain in 6 seats
    And I raise the opening bid in the first game's first external command by £1
    Then verifying the first game fails with "diverges at midnight 1"

  @spec-11
  Scenario: A replay from other rules is refused, not re-simulated
    When I record a batch of 1 games from seed 9 with the "idle" brain
    And the build's rules fingerprint becomes "0123456789abcdef"
    Then verifying the first game fails with "this build's rules fingerprint is 0123456789abcdef"
    And the message names the replay's rules fingerprint "00c0ffee00c0ffee"

  @spec-11
  Scenario: The manifest has the documented columns and weighs human games more
    Given a human multiplier of 5
    When I record one game with a human in seat 0 and one game without
    Then the manifest has the columns of docs/replay.md
    And the human game has weight 5 and 1 human seat
    And the AI-only game has weight 1 and 0 human seats
    And the human seat carries the anonymous player id "anon-7f3a" and no name

  @spec-11
  Scenario: Config snapshots are stored once per distinct config
    When I record a batch of 2 games from seed 1 with the "idle" brain
    And I record a batch of 2 games from seed 3 with the "idle" brain into the same store
    Then the store holds 1 config snapshot
    When I record a batch of 2 games from seed 1 with the "idle" brain and "economy.money.overdraft.start_limit=20000" into the same store
    Then the store holds 2 config snapshots
    And every config snapshot is named by the hash of its content and loads

  @spec-11
  Scenario: Replays are packed into a few segment objects, each game one ranged read
    Given segments of 4 KiB
    When I record a batch of 24 games from seed 0 with the "scripted" brain
    Then the store holds fewer replay objects than games
    And every manifest row's byte range decodes to that game

  @spec-11
  Scenario: The extractor writes one row per command with lineage
    When I record a batch of 2 games from seed 4 with the "scripted" brain in 6 seats
    And I extract the run
    Then the extract has one row per recorded command
    And every extract row carries the extractor id, the game id and the rules fingerprint

  @spec-11
  Scenario Outline: Older rules weigh less at extract time
    Given rules history "aaaaaaaaaaaaaaaa,bbbbbbbbbbbbbbbb" with decay 0.5 and floor 0.1
    Then a game recorded under "<fingerprint>" weighs <weight>

    Examples:
      | fingerprint      | weight |
      | 00c0ffee00c0ffee | 1      |
      | aaaaaaaaaaaaaaaa | 0.5    |
      | bbbbbbbbbbbbbbbb | 0.25   |
      | cccccccccccccccc | 0.5    |

  @spec-11
  Scenario: Rules weight never drops below the floor
    Given rules history "a1,a2,a3,a4,a5,a6" with decay 0.5 and floor 0.1
    Then a game recorded under "a6" weighs 0.1

  # ---- Command line ------------------------------------------------------------------------

  @spec-11
  Scenario: sim-cli records a batch and verifies it from the manifest
    When I run sim-cli "batch config --games 2 --preset v1 --brain scripted --players 6 --no-wall-time --out {tmp}/a.csv --record {tmp}/rec --run-id test-bdd"
    Then sim-cli succeeds
    When I run sim-cli "replay --verify {tmp}/rec/manifests/test-bdd/part-00000.parquet"
    Then sim-cli succeeds
    And its output contains "2 games verified"

  @spec-11
  Scenario: sim-cli refuses a replay recorded under other rules
    When I run sim-cli "record config --games 1 --preset v1 --dest {tmp}/rec --run-id test-bdd"
    Then sim-cli succeeds
    When I rewrite the rules fingerprint of run "test-bdd" under "rec" to "0000000000000000"
    And I run sim-cli "replay --verify {tmp}/rec/manifests/test-bdd/part-00000.parquet"
    Then sim-cli fails
    And its error output contains "recorded under rules fingerprint 0000000000000000"
