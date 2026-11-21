# Spec §10b, R45: `sim-cli validate-config` loads a config directory exactly as the server and
# the harness do and fails loudly, naming the file and key. R71: economy.yaml is one of the nine
# files, required like the rest.
Feature: Validating a config directory
  A config directory is checked before any game runs.

  @R45 @R71
  Scenario: The shipped config validates
    When I run sim-cli "validate-config config"
    Then sim-cli succeeds
    And its output contains "ok: config valid"

  @R45 @R71
  Scenario: A directory without economy.yaml fails and names it
    Given a copy of the shipped config without "economy.yaml" in "no-economy"
    When I run sim-cli "validate-config {tmp}/no-economy"
    Then sim-cli fails
    And its error output contains "economy.yaml"

  @R45 @R71
  Scenario: An economy lever in its old file fails and names the file and key
    Given a copy of the shipped config in "old-layout" with "game.yaml" key "players.starting_cash" set to "7000"
    When I run sim-cli "validate-config {tmp}/old-layout"
    Then sim-cli fails
    And its error output contains "game.yaml"
    And its error output contains "starting_cash"
