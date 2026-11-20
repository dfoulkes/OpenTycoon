# Spec §2.2 (06:00 settlement), §5.2 (who collects rent), §5.3 (formulas), §6 (utility bills).
# Tags name the rulings each scenario proves. Shipped placeholders: land_base_rate £0.05,
# Low tier ×0.6; utilities electricity 2p, gas/water/telecoms 1p; bread wholesale 4p.
Feature: The 06:00 settlement
  At 06:00 every building pays its land rent, its utility bills and tomorrow's stock order.
  Rent goes to whoever holds the lease, or to the city if nobody does. Utility revenue goes to
  the city while it holds the utilities. Stock is bought from the city's wholesalers.

  Background:
    Given the shipped config
    And a new game with 2 players
    And the game has run to "01:00"

  # A 10-high 2×2 bakery on Old Kent Road (Low): volume 40.
  #   rent      = 40 × £0.05 × 0.6                              = £1.20
  #   utilities = 40 × (2p × 1.0 + 1p × 1.5 + 1p × 1.0 + 1p × 0.5) = £2.00
  #   stock     = 100 bread × £0.04                               = £4.00
  @R10 @R32
  Scenario: Rent goes to the leaseholder; utilities and stock go to the city
    Given player 2 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And the "bakery" on "Old Kent Road" has ordered 100 "bread" for tomorrow
    And player 1 has a balance of "£100.00"
    And player 2 has a balance of "£0.00"
    And the city has a balance of "£0.00"
    When the game runs to "05:50"
    Then player 1's balance is "£100.00"
    When the game runs to "06:00"
    Then player 1's balance is "£92.80"
    And player 2's balance is "£1.20"
    And the city's balance is "£6.00"
    And no money was created or destroyed

  @R10
  Scenario: Unleased land pays its rent to the city
    Given player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    And the city has a balance of "£0.00"
    When the game runs to "06:00"
    Then player 1's balance is "£96.80"
    And the city's balance is "£3.20"

  @R10
  Scenario: Rent on the leaseholder's own building nets to zero
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    Then player 1's balance is "£98.00"

  @R10
  Scenario: Rent and bills scale with the building's volume, not the plot
    Given player 1 owns a 20-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    Then player 1's balance is "£93.60"

  @R32
  Scenario: Settlement happens once a day
    Given player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    And the game runs to "05:50"
    Then player 1's balance is "£96.80"
    When the game runs to "06:00"
    Then player 1's balance is "£93.60"
