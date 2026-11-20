# Spec §3 Money (net worth, the win metric). Tags name the rulings each scenario proves.
#   net_worth = cash + stock
#             + Σ (owned properties) property_value
#             + Σ (held lots) [ land_value + 0.2 × Σ property_value on that lot ]
#   land_value = last auction price × remaining lease years / 30; property value = build cost.
# Scenarios start at 12:00 so the day's 06:00 bills are already paid.
Feature: Net worth
  Net worth is cash plus stock plus property plus land. It is recalculated every midnight,
  before the unsold stock is written off, and the last one is the final score.

  Background:
    Given the shipped config
    And a new game with 2 players
    And the game has run to "12:00"

  @R19
  Scenario: Net worth is cash plus stock plus property
    # 10-high 3×3 clothes store on Soho (Medium ×1.0): build cost 90 × £10 = £900.
    Given player 1 has a balance of "£1,000.00"
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    And the "clothes_store" on "Soho" has 10 "clothes" in stock
    When the game runs to midnight
    # £1,000 + 10 × £1.40 + £900
    Then player 1's net worth is "£1,914.00"

  @R19
  Scenario: A negative balance counts against net worth
    Given player 1 has a balance of "-£2,500.00"
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    When the game runs to midnight
    Then player 1's net worth is "-£1,600.00"

  @R39
  Scenario: The leaseholder gets the land value plus 20% of every property on the lot
    Given player 1 holds the lease on "Soho" bought for "£5,000.00" in 1930
    And player 2 owns a 10-high 3x3 "clothes_store" on "Soho"
    And player 1 has a balance of "£0.00"
    And player 2 has a balance of "£0.00"
    When the game runs to midnight
    # player 1: £5,000 land + 20% × £900; player 2: £900 property
    Then player 1's net worth is "£5,180.00"
    And player 2's net worth is "£900.00"

  @R39
  Scenario: Holding the land and owning the building earns both
    Given player 1 holds the lease on "Soho" bought for "£5,000.00" in 1930
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    And player 1 owns a 10-high 2x3 "grocery" on "Soho"
    And player 2 owns a 10-high 2x2 "bakery" on "Soho"
    And player 1 has a balance of "£0.00"
    When the game runs to midnight
    # £5,000 + (£900 + £600) + 20% × (£900 + £600 + £400)
    Then player 1's net worth is "£6,880.00"

  @R19 @R39
  Scenario: Land value falls with the years left on the lease
    Given player 1 holds the lease on "Bloomsbury" bought for "£5,500.00" in 1930
    And player 1 has a balance of "£0.00"
    When the game runs to midnight
    # scored for 1930: 30 of 30 years left
    Then player 1's net worth is "£5,500.00"
    When the game runs to midnight
    # scored for 1935: £5,500 × 25 / 30 = £4,583.33
    Then player 1's net worth is "£4,583.33"

  @R31
  Scenario: Net worth counts the stock before it is written off
    Given player 1 has a balance of "£0.00"
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    And the "clothes_store" on "Soho" has 10 "clothes" in stock
    When the game runs to midnight
    Then player 1's net worth is "£914.00"
    And the "clothes_store" on "Soho" has 0 "clothes" in stock

  @R31
  Scenario: Net worth is recalculated every midnight
    Given player 1 has a balance of "£0.00"
    When the game runs to midnight
    Then player 1's net worth is "£0.00"
    When the game runs to "12:00"
    And player 1's balance changes to "£750.00"
    Then player 1's net worth is "£0.00"
    When the game runs to midnight
    Then player 1's net worth is "£750.00"

  @R19 @R31
  Scenario: The final midnight's net worth is the final score, stock included
    Given the shipped config with "game.yaml" key "/clock/end_year" set to "1935"
    And a new game with 2 players
    And the game has run to "12:00"
    And player 1 has a balance of "£100.00"
    And player 2 has a balance of "£200.00"
    And player 1 owns a 10-high 3x3 "clothes_store" on "Soho"
    And the "clothes_store" on "Soho" has 10 "clothes" in stock
    When the game runs to midnight
    Then the game is finished
    And player 1's net worth is "£1,014.00"
    And the leader is player 1
