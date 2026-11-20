# Spec §2.3 Bankruptcy. Tags name the rulings each scenario proves.
Feature: The midnight solvency check and bankruptcy
  At midnight every player's cash balance is checked. A balance of £0 or more passes.
  Two failed midnights in a row and the player is bankrupt: their leases, buildings and
  stock go to the city, which runs the businesses at default settings and can never go bust.

  Background:
    Given the shipped config on the v1 map
    # The v1 placeholder bakery and bread this arithmetic is written against (the shipped
    # stats are era content: docs/economy.md).
    And the shipped config with "businesses.yaml" key "/goods/bread" set to "{name: Bread, ref_price: 0.10, purchase_price: 0.04}"
    And the shipped config with "businesses.yaml" key "/types/bakery/stock_density" set to "0.5"
    And the shipped config with "businesses.yaml" key "/types/bakery/utility_factor" set to "{electricity: 1.0, gas: 1.5, water: 1.0, telecoms: 0.5}"
    And a new game with 2 players
    And the game has run to "12:00"

  @R3
  Scenario Outline: £0 at midnight is solvent; a penny under is a strike
    Given player 1 has a balance of "<balance>"
    When the game runs to midnight
    Then player 1 has <strikes> strikes
    And player 1 is still in the game

    Examples:
      | balance    | strikes |
      | £0.00      | 0       |
      | £0.01      | 0       |
      | £5,000.00  | 0       |
      | -£0.01     | 1       |
      | -£9,000.00 | 1       |

  @R3 @R17
  Scenario: The solvency threshold is configurable
    Given the shipped config with "game.yaml" key "/bankruptcy/solvent_at_or_above" set to "100"
    And a new game with 2 players
    And the game has run to "12:00"
    And player 1 has a balance of "£99.99"
    When the game runs to midnight
    Then player 1 has 1 strikes

  @R4
  Scenario: Two failed midnights in a row is bankruptcy
    Given player 1 has a balance of "-£1.00"
    When the game runs to midnight
    Then player 1 has 1 strikes
    And player 1 is still in the game
    When the game runs to midnight
    Then player 1 is bankrupt
    And player 2 is still in the game

  @R4
  Scenario: Only a passed midnight resets the count
    Given player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And player 1's balance changes to "£0.00"
    And the game runs to midnight
    Then player 1 has 0 strikes
    When player 1's balance changes to "-£1.00"
    And the game runs to midnight
    Then player 1 has 1 strikes
    And player 1 is still in the game

  @R4
  Scenario: Going positive during the day and spending it again does not reset the count
    Given player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And the game runs to "12:00"
    And player 1's balance changes to "£500.00"
    And the game runs to "18:00"
    And player 1's balance changes to "-£1.00"
    And the game runs to midnight
    Then player 1 is bankrupt

  @R4
  Scenario: A bankrupt player's leases, buildings and stock go to the city
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    And the "clothes_store" on "Soho" is priced at 150% for "clothes"
    And the "clothes_store" on "Soho" has 20 "clothes" in stock
    And the "clothes_store" on "Soho" has ordered 5 "clothes" for tomorrow
    And player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And the game runs to midnight
    Then player 1 is bankrupt
    And "Old Kent Road" is unleased
    And the city owns the "bakery" on "Old Kent Road"
    And the city owns the "clothes_store" on "Soho"
    And the "clothes_store" on "Soho" charges 100% for "clothes"
    And the "clothes_store" on "Soho" has ordered 27 "clothes" for tomorrow
    And player 1 owns nothing

  @R4
  Scenario: A bankrupt player is out of the game and their commands are ignored
    Given player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And the game runs to midnight
    And player 1's balance changes to "-£1.00"
    And player 1 sells the "bakery" on "Old Kent Road" to the city
    Then player 1's balance is "-£1.00"
    And player 1 has no bid ceiling

  @R4
  Scenario: The city can never go bust
    Given player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And the game runs to midnight
    And the city's balance changes to "-£1,000,000.00"
    And the game runs for 3 days
    Then the city owns the "bakery" on "Old Kent Road"
    And no money was created or destroyed

  # A 10-high 3×3 bakery on Holborn holds 90 × 0.5 = 45 bread. Holborn has a tube station, so
  # the morning's shoppers walk past it.
  @R4
  Scenario: The city runs a bankrupt player's shop: restocked at 06:00 and selling to shoppers
    Given player 1 owns a 10-high 3x3 "bakery" on "Holborn"
    And the "bakery" on "Holborn" is priced at 150% for "bread"
    And player 1 has a balance of "-£1.00"
    When the game runs to midnight
    And the game runs to midnight
    Then player 1 is bankrupt
    And the city owns the "bakery" on "Holborn"
    And the "bakery" on "Holborn" charges 100% for "bread"
    And the "bakery" on "Holborn" has ordered 45 "bread" for tomorrow
    When the city's balance changes to "-£1,000,000.00"
    And the game runs to "06:00"
    Then the "bakery" on "Holborn" has 45 "bread" in stock
    When the game runs to "17:00"
    Then the city has sold "bread" at the "bakery" on "Holborn" today
    When the game runs to "06:00"
    Then the "bakery" on "Holborn" has 45 "bread" in stock
    When the game runs to "17:00"
    Then the city has sold "bread" at the "bakery" on "Holborn" today
    And the city's balance is below "£0.00"
    And the city owns the "bakery" on "Holborn"
    And no money was created or destroyed
