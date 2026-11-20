# Spec §6 (utilities), §5.1 (auctions); R12, R24. Shipped placeholders: electricity 2p, gas,
# water and telecoms 1p per unit of volume per day; reserve prices Electricity £3,000, Gas,
# Water and the Telephone Exchange £2,000. Elections are switched off here (elections.feature).
Feature: Utilities are owned like land
  Electricity, Gas, Water and the Telephone Exchange are auctioned like land: same auction
  house, same queue, same rules. Whoever holds a utility collects that utility's bills from
  every building on the map at 06:00; their own buildings' bills for it net to zero. Revenue
  from a utility nobody holds goes to the city.

  Background:
    Given the shipped config with "events.yaml" key "/elections/enabled" set to "false"
    # The v1 placeholder bakery and bread this arithmetic is written against (the shipped
    # stats are era content: docs/economy.md).
    And the shipped config with "businesses.yaml" key "/goods/bread" set to "{name: Bread, ref_price: 0.10, purchase_price: 0.04}"
    And the shipped config with "businesses.yaml" key "/types/bakery/stock_density" set to "0.5"
    And the shipped config with "businesses.yaml" key "/types/bakery/utility_factor" set to "{electricity: 1.0, gas: 1.5, water: 1.0, telecoms: 0.5}"
    And the shipped config with "customers.yaml" key "/initial_population" set to "200"
    And a new game with 2 players
    And the game has run to "07:00"

  @R12
  Scenario: A utility is auctioned like land, with a minimum opening bid
    Given player 1 has a balance of "£7,000.00"
    When player 1 opens an auction on the "Electricity" utility with a bid of "£2,999.00"
    Then the auction house refused player 1 as below the minimum
    When player 1 opens an auction on the "Electricity" utility with a bid of "£3,000.00"
    And everyone else passes until the auction closes
    Then player 1 holds the "Electricity" utility
    And player 1's balance is "£4,000.00"
    And no money was created or destroyed

  @R12
  Scenario: Only one auction at a time, utilities included
    When player 1 opens an auction on the "Electricity" utility with a bid of "£3,000.00"
    And player 2 opens an auction on the "Gas" utility with a bid of "£2,000.00"
    Then the auction house refused player 2 as busy

  @R12
  Scenario: A held utility is sold only when its holder offers it
    Given player 1 holds the "Gas" utility bought for "£2,000.00"
    When player 2 opens an auction on the "Gas" utility with a bid of "£5,000.00"
    Then the auction house refused player 2 as not for sale
    When player 1 offers the "Gas" utility for auction
    Then the "Gas" utility is up for auction, sold by player 1

  # Two 10-high 2×2 bakeries (volume 40) on Low blocks, one each. Each pays rent £1.20 and
  # utilities £2.00: electricity 80p, gas 60p, water 40p, telecoms 20p. The city's apartment
  # block on Whitechapel (3×3, 10 high: volume 90, electricity factor 0.8) uses £1.44 of
  # electricity.
  @R12 @R24
  Scenario: The holder collects the utility's bills map-wide and pays itself nothing
    Given player 1 holds the "Electricity" utility bought for "£3,000.00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 2 owns a 10-high 2x2 "bakery" on "Bethnal Green"
    And player 1 has a balance of "£100.00"
    And player 2 has a balance of "£100.00"
    When the game runs to "06:00"
    # Player 1: −£1.20 rent − £1.20 gas, water and telecoms + 80p from player 2 + £1.44 from
    # the city's apartment block. Its own 80p electricity bill nets to zero.
    Then player 1's balance is "£99.84"
    And player 2's balance is "£96.80"
    And no money was created or destroyed

  @R12
  Scenario: Revenue from a utility nobody holds goes to the city
    Given player 2 owns a 10-high 2x2 "bakery" on "Bethnal Green"
    And player 2 has a balance of "£100.00"
    And the city has a balance of "£0.00"
    When the game runs to "06:00"
    Then player 2's balance is "£96.80"
    And the city's balance is "£3.20" plus its residents' rent
