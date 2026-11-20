# Spec §4.1, §7.2. Tags name the rulings each scenario proves.
Feature: The city's starting apartment blocks
  The city starts the game owning an apartment block in each Low-tier set. Players can
  win them at auction like any other city asset.

  Background:
    Given the shipped config

  @R28
  Scenario: The city starts with one apartment block in each Low-tier set
    Given a game with 2 players
    Then every Low-tier set has exactly one apartment block owned by the city
    And the city owns the apartment block on "Whitechapel"
    And no other set has a city apartment block

  @R28 @R5
  Scenario: A city apartment block can be won at auction, on its own
    Given a game with 3 players
    When player 0 opens an auction on the apartment block on "Whitechapel" with its value
    And 120 real seconds pass
    And player 1 passes
    And player 2 passes
    Then player 0 owns the apartment block on "Whitechapel"
    And "Whitechapel" has no leaseholder

  @R28
  Scenario: An opening bid below the apartment block's value is refused
    Given a game with 2 players
    When player 0 opens an auction on the apartment block on "Whitechapel" with £50 less than its value
    Then the command is refused: below the minimum

  @R28 @R17
  Scenario: A Low-tier set without a city apartment block fails to load
    Given the shipped config with "map.yaml" key "/city_apartments/sites" set to "[]"
    When the config is loaded
    Then loading fails
    And an error names "map.yaml" and "/city_apartments/sites"

  @R28 @R22
  Scenario: A city apartment block must have street frontage
    Given the shipped config with "map.yaml" key "/city_apartments/sites/0/position" set to "[3, 3]"
    When the config is loaded
    Then loading fails
    And an error names "map.yaml" and "touch its edge"
