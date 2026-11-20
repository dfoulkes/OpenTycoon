# R66: AI personalities are opportunistic. Each keeps its preferences but moves into residential
# (or any line) when that line is earning more than its own; the Grocer stays pure retail.
#
# Once a day, after night trade closes, each AI reads what every line is earning from public
# state only: each apartment block's flats let × rent, each shop's takings, their running costs
# (land rent, utilities, stock) and what they cost to build, and the land rent leaseholders
# collect on what they paid. A line's yield is its daily net earnings per pound tied up. Tunables
# are ai.yaml `<personality>.opportunism` (shipped: the Toff and the Speculator move when another
# line earns 25% more than their own; the Toff keeps at most 40% of its buildings outside retail;
# the Grocer's is off).
Feature: Opportunism
  The Toff puts luxury flats on its prime lots and the Speculator trades apartment blocks when
  flats pay better than their own line, while the Grocer sticks to its shops.

  Background:
    # These scenarios name the 3×3 map's districts, sets and prices.
    Given the shipped config on the v1 map

  @spec-10 @spec-7.2 @R66
  Scenario Outline: The Toff builds flats on its High lot only when flats out-earn its shops
    # The city's starter apartment blocks are let at the reference rent: at £3.00 they pay well,
    # at £0.05 they barely cover their bills.
    Given the Toff plays seat 1 against a scripted rival in seat 0
    And every player starts with £60,000
    And 2000 people live in the city
    And the reference rent is £<rent>
    And the Toff holds the lease on "Mayfair"
    When the Toff plays for 3 game days
    Then the Toff put up at least 1 shop
    And the Toff <did> put up an apartment block on "Mayfair"

    Examples:
      | rent | did    |
      | 3.00 | did    |
      | 0.05 | didn't |

  @spec-10 @spec-7.2 @R66 @R51
  Scenario: The Toff's flats are luxury flats: high fit-out, let over the reference rent
    Given the Toff plays seat 1 against a scripted rival in seat 0
    And every player starts with £60,000
    And 2000 people live in the city
    And the Toff holds the lease on "Mayfair"
    When the Toff plays for 3 game days
    Then every Toff apartment block has high fit-out
    And the rent on every Toff apartment block is above £3.00

  @spec-10 @R66 @R28
  Scenario: The Speculator buys an apartment block and re-lists it when residential pays
    Given the Speculator plays seat 1 against a scripted rival in seat 0
    And 2000 people live in the city
    And the rival holds every lease
    When the Speculator plays for 4 game days
    Then the Speculator bought an apartment block
    And the Speculator offered an apartment block it bought for auction

  @spec-10 @R66 @R28
  Scenario: The Speculator leaves apartment blocks alone when they don't pay
    Given the Speculator plays seat 1 against a scripted rival in seat 0
    And 2000 people live in the city
    And the reference rent is £0.05
    And the rival holds every lease
    When the Speculator plays for 4 game days
    Then the Speculator never bid on an apartment block

  @spec-10 @R66
  Scenario: The Grocer never builds apartments, whatever the yields
    Given the Grocer plays seat 1 against a scripted rival in seat 0
    And every player starts with £60,000
    And 2000 people live in the city
    And the reference rent is £9.00
    And the Grocer holds the lease on "Old Kent Road"
    When seat 1 plays for 3 game days
    Then the Grocer put up at least 1 shop
    And the Grocer never put up an apartment block

  @spec-10 @R66
  Scenario: Over a whole game against the full roster the Grocer never builds apartments
    Given the "v1" game length
    When the full roster plays the whole game
    Then the Grocer never put up an apartment block

  @spec-10 @R66
  Scenario: Opportunism respects the holdings cap
    # Rich enough to fill its lot, it still keeps at least 60% of its buildings in retail.
    Given the Toff plays seat 1 against a scripted rival in seat 0
    And every player starts with £200,000
    And 2000 people live in the city
    And the Toff holds the lease on "Mayfair"
    When the Toff plays for 4 game days
    Then the Toff put up at least 1 apartment block
    And after every building it put up, at most 40% of the Toff's buildings by build cost were apartment blocks
