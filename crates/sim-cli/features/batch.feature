# Spec §11: "Run 1,000 AI-vs-AI games per preset, then chart median net worth and bankruptcy
# rate. Tune constants from that, not by feel." §10c: sim-cli plays 1,000 games 1930–1950 and
# writes CSV.
Feature: Balance batch
  sim-cli plays many independent games of a preset and writes one CSV row per game, with a
  summary of how players fared.

  @spec-11
  Scenario: A batch plays one full game per seed, sorted by seed
    Given the shipped config
    And the preset "v1"
    When I run a batch of 6 games from seed 100 with the "idle" brain
    Then the CSV has 6 games with seeds 100 to 105 in order
    And every game played 4 days
    And every row has a net worth and a bankrupt flag for 2 players

  @spec-11
  Scenario: The summary gives the net worth spread, bankruptcy rate and throughput
    Given the shipped config
    And the preset "v1"
    When I run a batch of 6 games from seed 0 with the "scripted" brain
    Then the summary has p10, median and p90 net worth for each of 2 seats
    And the summary's bankruptcy rate is between 0 and 1
    And the summary reports games per second

  @spec-11
  Scenario: The idle brain never acts
    Given a new game of the "v1" preset
    When the "idle" brain plays seat 0 for one day
    Then it issued 0 commands

  @spec-11 @R23
  Scenario: The scripted brain opens with one bid on the cheapest lot at its base price
    Given a new game of the "v1" preset with no AI opponents
    When the "scripted" brain plays seat 0 for one day
    Then its first command opens an auction on "Old Kent Road" at £2000
    And it bid during day trade

  # The harness exercises building: after winning its lease the scripted brain builds one
  # shop and stocks it.
  @spec-11 @R2 @R22
  Scenario: The scripted brain builds one shop on the lot it wins and stocks it
    Given a new game of the "v1" preset with no AI opponents
    When the "scripted" brain plays seat 0 for one day
    Then it issued 3 commands
    And its second command builds a "grocery" on "Old Kent Road"
    And player 0 owns 1 shop on "Old Kent Road", touching the lot edge
    And player 0's shop has a standing order for its full capacity

  @spec-11
  Scenario: sim-cli batch writes the CSV and prints the summary
    When I run sim-cli "batch config --games 3 --threads 2 --out {tmp}/batch.csv"
    Then sim-cli succeeds
    And the file "batch.csv" has 4 lines
    And its output contains "games/sec"
    And its output contains "median"

  # §3, R19, R39: net worth counts land, not just cash. Guards against the harness scoring
  # cash only, which made every game a £0 tie.
  @spec-11 @R19
  Scenario: Players with different assets get different net worths in the CSV
    Given a new game of the "v1" preset
    And player 0 holds the lease on "Mayfair", bought in 1930 for £14000
    When the harness plays it to the whistle and writes the CSV
    Then player 0's net worth in the CSV is higher than player 1's
    And the CSV names player 0 the winner
