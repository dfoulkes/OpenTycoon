# Spec §2.2 (06:00 settlement), §7.2 (apartments), §8.2 (rent comes out of the allowance
# first). Residents have no bank accounts (R63): their rent is paid out of the shoppers' purse,
# where each morning's allowances are minted, to whoever owns the apartment block.
# Shipped placeholders: default rent £3.00 a flat, flat size 4, land_base_rate £0.05, Low ×0.6.
Feature: Residents pay their rent at the 06:00 settlement
  At 06:00 every resident's rent leaves the shoppers' purse for the owner of their apartment
  block, before anyone goes shopping. The block itself pays land rent and utilities by its
  volume, like every building, and orders no stock.

  Background:
    Given the shipped config
    And a new game with 2 players
    And the game has run to "01:00"

  # A 10-high 3×3 block on Old Kent Road (Low): volume 90, 22 flats, all let on the first
  # morning (2,000 people arrive at 05:30).
  #   residents' rent = 22 × £3.00                                       = £66.00
  #   land rent       = 90 × £0.05 × 0.6 (to the city: the land is unleased) = £2.70
  #   utilities       = 90 × (2p × 0.8 + 1p × 0.8 + 1p × 1.0 + 1p × 0.5)    = £3.51
  @R56 @R51
  Scenario: An apartment block collects its residents' rent and pays its own land rent and utilities
    Given player 1 owns a 10-high 3x3 "apartments" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "05:50"
    Then 22 people live in the "apartments" on "Old Kent Road"
    And player 1's balance is "£100.00"
    When the game runs to "06:00"
    Then player 1's balance is "£159.79"
    And the shoppers' purse paid every resident's rent
    And no money was created or destroyed

  @R56 @R51
  Scenario: Rent is collected every morning, once
    Given player 1 owns a 10-high 3x3 "apartments" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "06:00"
    And the game runs to "05:50"
    Then player 1's balance is "£159.79"
    When the game runs to "06:00"
    Then player 1's balance is "£219.58"
    And no money was created or destroyed

  @R51
  Scenario: The owner's rent is what residents pay
    Given player 1 owns a 10-high 3x3 "apartments" on "Old Kent Road"
    And player 1 has a balance of "£100.00"
    When the game runs to "05:50"
    And player 1 sets the rent at the "apartments" on "Old Kent Road" to "£3.50"
    And the game runs to "06:00"
    Then player 1's balance is "£170.79"
