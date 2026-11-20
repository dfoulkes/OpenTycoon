# Spec §6, R11. Shipped placeholders: telecoms 1p and the internet 2p per unit of volume per day;
# a bakery's telecoms factor is 0.5, an apartment block's 0.5.
Feature: The telephone exchange becomes the internet in 1995
  The Telephone Exchange is there from 1930 and becomes the Internet in 1995. From then every
  building pays an internet bill, to whoever holds it.

  @R11
  Scenario: Every building pays the internet bill from 1995
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1990"
    And the shipped config with "events.yaml" key "/elections/enabled" set to "false"
    And the shipped config with "customers.yaml" key "/initial_population" set to "200"
    And a new game with 2 players
    And the game has run to "01:00"
    And player 2 holds the "Telephone Exchange" utility bought for "£2,000.00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 2 has a balance of "£0.00"
    Then the "Telecoms" utility is called "Telephone Exchange"
    # 1990: bakery 40 × 1p × 0.5 = 20p; the city's apartments 90 × 1p × 0.5 = 45p.
    When the game runs to "06:00"
    Then player 2's balance is "£0.65"
    # 1995: bakery 40 × 2p × 0.5 = 40p; apartments 90 × 2p × 0.5 = 90p.
    When the game runs to "05:50"
    And the game runs to "06:00"
    Then the "Telecoms" utility is called "Internet"
    And player 2's balance is "£1.95"
