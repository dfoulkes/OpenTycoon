# Spec §2.2 (06:00 settlement), §7.2 (apartments). Study levers, off by default
# (docs/economy.md "Residential levers"): a cap on any flat's rent and a tax on residents' rent.
# They are not rulings, so each scenario is tagged with the ruling it adjusts (R51: the owner
# sets rent; R63: rent comes out of minted allowances, so no money is created or destroyed).
# Same block as resident_rent.feature: 10-high 3×3 on Old Kent Road (Low), volume 90, 22 flats,
# land rent £2.70 and utilities £3.51 a day.
Feature: Study levers on residents' rent
  The designer can cap rents or tax them, so the balance study can rein residential income in.
  Both are off in the shipped config.

  @R51 @R63
  Scenario: A rent tax sends a share of every resident's rent to the city
    # 22 × £3.00 = £66.00 of rent; 20% (£13.20) to the city, £52.80 to the owner.
    #   £100.00 + £52.80 − £2.70 − £3.51 = £146.59
    Given the shipped config with "customers.yaml" key "/residential/rent_tax" set to "0.2"
    And a new game with 2 players
    And the game has run to "01:00"
    And player 1 owns a 10-high 3x3 "apartments" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    Then 22 people live in the "apartments" on "Old Kent Road"
    And player 1's balance is "£146.59"
    And the shoppers' purse paid every resident's rent
    And no money was created or destroyed

  @R51 @R63
  Scenario: A rent cap holds what residents pay below what the owner asks
    # The owner asks £3.50 a flat; the cap is £2.50: 22 × £2.50 = £55.00.
    #   £100.00 + £55.00 − £2.70 − £3.51 = £148.79
    Given the shipped config with "customers.yaml" key "/residential/max_rent_per_flat" set to "2.50"
    And a new game with 2 players
    And the game has run to "01:00"
    And player 1 owns a 10-high 3x3 "apartments" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "05:50"
    And player 1 sets the rent at the "apartments" on "Old Kent Road" to "£3.50"
    And the game runs to "06:00"
    Then player 1's balance is "£148.79"
    And the shoppers' purse paid every resident's rent
    And no money was created or destroyed
