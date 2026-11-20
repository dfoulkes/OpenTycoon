# docs/engineering.md §4: performance budgets, and a regression check that fails loudly.
# Not part of plain `cargo test` runs on CI's shared runners: these scenarios use budgets
# far from the measured value, so they cannot flake.
Feature: Performance budget check
  sim-cli perf-budget measures the sim, compares each metric with its budget plus a tolerance,
  and exits non-zero naming every metric over budget.

  @spec-11
  Scenario: A run within budget passes
    When I run sim-cli "perf-budget config --only config_load_ms --budget config_load_ms=100000"
    Then sim-cli succeeds
    And its output contains "config_load_ms"
    And its output contains "ok"

  @spec-11
  Scenario: A budget set below the measured value fails and names the metric
    When I run sim-cli "perf-budget config --only game_day_ms --budget game_day_ms=0.000001 --repeats 1"
    Then sim-cli fails
    And its output contains "OVER BUDGET"
    And its error output contains "game_day_ms is over budget"

  @spec-11
  Scenario: An unknown metric is refused
    When I run sim-cli "perf-budget config --budget gaem_day_ms=1"
    Then sim-cli fails
    And its error output contains "game_day_ms"

  @spec-11
  Scenario: A budgets file whose limits contradict each other is refused before measuring
    When I run sim-cli "perf-budget config --budgets crates/sim-cli/tests/fixtures/inconsistent-budgets.yaml"
    Then sim-cli fails
    And its error output contains "inconsistent budgets"
    And its error output contains "tick_p99_us"
