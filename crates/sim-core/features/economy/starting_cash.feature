# Spec §3 Money (starting cash). Tags name the rulings each scenario proves.
Feature: Starting cash
  Every player starts the game with £7,000 in the bank, on top of their overdraft.
  The amount is configurable.

  @R62
  Scenario: Players start with £7,000
    Given the shipped config
    And a new game with 2 players
    Then player 1's balance is "£7,000.00"
    And player 2's balance is "£7,000.00"
    And player 1's bid ceiling is "£17,000.00"

  @R62 @R17
  Scenario: Starting cash is configurable
    Given the shipped config with "game.yaml" key "/players/starting_cash" set to "2500"
    And a new game with 2 players
    Then player 1's balance is "£2,500.00"
    And player 2's balance is "£2,500.00"
