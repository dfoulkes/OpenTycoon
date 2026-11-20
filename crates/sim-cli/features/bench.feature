# Spec §10a/§11 and docs/engineering.md §4: the sim must use a small fraction of real time so
# the server can run 4× speed and the harness can run 1,000-game sweeps. `sim-cli bench`
# measures it.
Feature: Headless benchmark
  sim-cli bench measures how fast the sim runs and prints machine-readable JSON.

  @spec-11
  Scenario: sim-cli bench prints throughput and the real-time factor as JSON
    When I run sim-cli "bench config --days 1 --repeats 1"
    Then sim-cli succeeds
    And its output is JSON with "ticks_per_second", "ms_per_game_day", "real_time_factor" and "tick_latency"
    And the JSON says scenario "shipped" with 8640 ticks

  @spec-11
  Scenario: The synthetic large scenario sets the population
    When I run sim-cli "bench config --days 1 --repeats 1 --agents 10000"
    Then sim-cli succeeds
    And the JSON says scenario "synthetic-10000-agents" with 10000 agents
