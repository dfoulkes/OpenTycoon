# Spec §2.3 Selling up: the instant sale back to the city. The business-auction half of R5 is
# the auctions module's. Tags name the rulings each scenario proves.
Feature: Selling a business back to the city
  Any time, in one click, a player can sell a business to the city for 50% of its true value
  (its build cost). The city runs it at default settings. The land is not part of the sale.

  Background:
    Given the shipped config
    And a new game with 2 players
    And the game has run to "12:00"

  @R5
  Scenario: Sell back at half the build cost, instantly
    # 10-high 2×2 bakery on Old Kent Road (Low ×0.6): build cost 40 × £10 × 0.6 = £240.
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And the "bakery" on "Old Kent Road" is priced at 80% for "bread"
    And player 1 has a balance of "£0.00"
    When player 1 sells the "bakery" on "Old Kent Road" to the city
    Then player 1's balance is "£120.00"
    And the city owns the "bakery" on "Old Kent Road"
    And the "bakery" on "Old Kent Road" charges 100% for "bread"
    And player 1 holds the lease on "Old Kent Road"
    And no money was created or destroyed

  @R5 @R17
  Scenario: The sell-back share is configurable
    Given the shipped config with "economy.yaml" key "/money/sell_to_city_ratio" set to "0.75"
    And a new game with 2 players
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£0.00"
    When player 1 sells the "bakery" on "Old Kent Road" to the city
    Then player 1's balance is "£180.00"

  @R5
  Scenario: A player cannot sell someone else's business
    Given player 2 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£0.00"
    When player 1 sells the "bakery" on "Old Kent Road" to the city
    Then player 1's balance is "£0.00"
    And player 2 owns the "bakery" on "Old Kent Road"

  @R5 @R39
  Scenario: After selling up, the leaseholder still collects the uplift and the rent
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£0.00"
    When player 1 sells the "bakery" on "Old Kent Road" to the city
    And the game runs to midnight
    # £120 cash + £2,000 land + 20% × £240
    Then player 1's net worth is "£2,168.00"
