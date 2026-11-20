# Spec §9: fire, burglary and health inspection roll per building per day at 06:00, after the
# delivery. Insurance is optional: a daily premium of 0.05% of property value, paid to the city,
# and the city pays insured losses back. Chances are forced here.
#
# Each scenario: a 10-high 2×2 bakery on a Low block (cost £240) pays £3.20 rent and utilities
# at 06:00; an insured player also pays a 12p premium.
Feature: Random building events and insurance

  Background:
    Given the shipped config with "customers.yaml" key "/initial_population" set to "200"
    And the shipped config with "events.yaml" key "/random/0/daily_chance" set to "0.0"
    And the shipped config with "events.yaml" key "/random/1/daily_chance" set to "0.0"
    And the shipped config with "events.yaml" key "/random/2/daily_chance" set to "0.0"

  @R16
  Scenario: A fire destroys a building; insurance pays its value back
    Given the shipped config with "events.yaml" key "/random/0/daily_chance" set to "1.0"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 2 owns a 10-high 2x2 "bakery" on "Bethnal Green"
    And player 1 takes out insurance
    And player 1 has a balance of "£100.00"
    And player 2 has a balance of "£100.00"
    When the game runs to "06:00"
    Then the "bakery" on "Old Kent Road" is gone
    And the "bakery" on "Bethnal Green" is gone
    And player 1's balance is "£336.68"
    And player 2's balance is "£96.80"
    And no money was created or destroyed

  @R16
  Scenario: A burglary takes half the stock; insurance pays its wholesale value
    Given the shipped config with "events.yaml" key "/random/1/daily_chance" set to "1.0"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And the "bakery" on "Old Kent Road" keeps a standing order of 100 "bread"
    And player 1 takes out insurance
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    Then the "bakery" on "Old Kent Road" has 50 "bread" in stock
    # −£3.20 bills − £4.00 for 100 bread − 12p premium + 50 × 4p back.
    And player 1's balance is "£94.68"
    And no money was created or destroyed

  @R16
  Scenario: A health inspection fines the owner; fines are not insurable
    Given the shipped config with "events.yaml" key "/random/2/daily_chance" set to "1.0"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 takes out insurance
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    Then player 1's balance is "£71.68"
    And no money was created or destroyed
