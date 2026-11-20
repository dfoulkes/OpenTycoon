# Spec §10: "The Grocer opens many cheap Low-tier shops." §10c: the one v1 AI.
# The Grocer plays seat 1 through public commands only, exactly as a client would (R40);
# the rival in seat 0 is scripted by each scenario. Tunables are ai.yaml `grocer` (shipped values:
# bids at most 30% of its bid ceiling, keeps £300 in hand, builds 3×3 groceries 12 high).
Feature: The Grocer
  The Grocer bids for cheap Low-tier lots, fills them with small day-trade shops, prices a
  little under the reference price for working-class shoppers, orders tomorrow's stock from
  today's sales, and keeps cash in hand so it is never caught short at midnight.

  Background:
    Given the shipped config

  @spec-10 @R23
  Scenario: It opens with a bid at the base price on the cheapest Low-tier lot
    When the Grocer plays for 1 game hour
    Then the Grocer opened an auction on "Old Kent Road" at £2,000

  @spec-10 @R8
  Scenario: It bids on a Low-tier lot someone else puts up
    Given the rival holds the lease on every Low-tier block except "Bethnal Green"
    When the rival opens an auction on "Bethnal Green" at £2,200
    And the auction runs to the close with the rival passing
    Then the Grocer raised on "Bethnal Green"
    And the Grocer holds the lease on "Bethnal Green"

  @spec-10 @R8
  Scenario: It never bids on High tier
    Given the rival holds the lease on every Low-tier block
    When the rival opens an auction on "Mayfair" at £14,000
    And the auction runs to the close with the rival passing
    Then the Grocer never bid on "Mayfair"
    And the rival holds the lease on "Mayfair"
    When the Grocer plays for 1 game day
    Then the Grocer opened no auction

  @spec-10 @R7
  Scenario: It never bids over its share of its bid ceiling
    # Ceiling = £7,000 + £10,000 credit = £17,000; 30% of it is £5,100.
    Given the rival holds the lease on every Low-tier block except "Old Kent Road"
    When the rival opens an auction on "Old Kent Road" at £2,000
    And the auction runs to the close with the rival raising by £500 every turn
    Then the Grocer raised on "Old Kent Road"
    And every bid the Grocer made was at most 30% of its bid ceiling at the time
    And the Grocer passed on "Old Kent Road"
    And the rival holds the lease on "Old Kent Road"

  @spec-10 @R13 @R22 @R58
  Scenario: After winning a lot it builds legal shops on it, round the city's apartments
    Given the rival holds the lease on every Low-tier block except "Whitechapel"
    When the Grocer plays for 6 game hours
    Then the Grocer holds the lease on "Whitechapel"
    And the Grocer owns at least 2 shops on "Whitechapel"
    And every Grocer shop is a day-trade type from its shop list
    And every Grocer shop touches the lot edge and overlaps no other building
    And every Grocer shop is no taller than its type's max_height
    And the Grocer's balance is at least its cash buffer

  @spec-10 @R2 @R10
  Scenario Outline: Tomorrow's order follows today's sales plus a 10% margin, within capacity
    # A 3×3×12 grocery holds 108 × 2.21 = 238 units of groceries.
    Given nobody comes shopping
    And the rival holds the lease on every Low-tier block except "Old Kent Road"
    When the Grocer plays until 16:00 on day 1
    And the Grocer's first shop had <delivered> delivered this morning and has sold <sold>
    And the Grocer plays until 18:00 on day 1
    Then the Grocer's first shop's order for tomorrow is <order>

    Examples:
      | delivered | sold | order |
      | 40        | 20   | 22    |
      | 40        | 40   | 44    |
      | 238       | 238  | 238   |
      | 40        | 0    | 1     |

  @spec-10 @R2 @R10
  Scenario: Over a whole game it never orders more than a shop can hold
    Given the "v1" game length
    When the Grocer plays the whole game
    Then every order the Grocer set was within its shop's capacity
    And the Grocer owns at least 2 shops

  @spec-10 @R3
  Scenario: It keeps its cash buffer and is never caught short at midnight
    Given the "v1" game length
    When the Grocer plays the whole game
    Then the Grocer had at least its cash buffer after every lot and shop it bought
    And the Grocer never took a strike
    And the Grocer is not bankrupt

  @spec-10 @R3 @R5
  Scenario: Caught short late in the evening, it sells a shop back rather than take a strike
    Given nobody comes shopping
    And the rival holds the lease on every Low-tier block except "Old Kent Road"
    When the Grocer plays until 22:30 on day 1
    And the Grocer's balance drops to -£100
    And the Grocer plays until 00:30 on day 2
    Then the Grocer sold a shop back to the city
    And the Grocer never took a strike

  # §10a, docs/engineering.md §3: same seed and config, same game, bit for bit. The Grocer
  # draws from its own stream, so it never shifts the world's draws.
  @spec-10
  Scenario: The same seed gives an identical Grocer game
    Given the "v1" game length
    When two games with seed 7 are played to the whistle with ai.yaml's Grocer in seat 1
    Then both games end with the same state hash
    And the Grocer owns shops in both games
