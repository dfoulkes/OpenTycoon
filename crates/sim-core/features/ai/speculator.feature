# Spec §10: "The Speculator bids hard and flips." It plays seat 1 through public commands only
# (R40); seat 0 is a scripted rival. Tunables are ai.yaml `speculator` (shipped: bids up to 51%
# of its bid ceiling, credit included, and at most 120% of base on a city lease; re-lists after
# 6 game hours; on a player's offer bids at most 60% of book value).
Feature: The Speculator
  The Speculator buys leases cheaply, from the city at the base price or from a player at a
  discount to book value, and puts them straight back up for auction.

  Background:
    # These scenarios name the 3×3 map's districts, sets and prices.
    Given the shipped config on the v1 map
    And the Speculator plays seat 1 against a scripted rival in seat 0

  @spec-10 @R33
  Scenario: It re-lists what it buys
    Given the rival holds the lease on every block except "Old Kent Road"
    When the Speculator plays for 12 game hours
    Then the Speculator won "Old Kent Road" at auction
    And the Speculator offered "Old Kent Road" for auction

  @spec-10 @R33 @R39
  Scenario Outline: On a lease a player offers it bids at most its bargain share of book value
    # Book value = last price × years left ÷ term (§3); 60% of £30,000 is £18,000, while 60% of
    # £3,000 is under the £2,000 reserve (the land base price).
    Given the rival holds the lease on every block except "Soho"
    And the rival's lease on "Old Kent Road" was bought for £<paid>
    And the Speculator's balance is £40,000
    When the rival offers "Old Kent Road" for auction
    And the auction is played out with the rival passing
    Then the Speculator <outcome> "Old Kent Road"
    And every bid the Speculator made on "Old Kent Road" was at most 60% of its book value

    Examples:
      | paid   | outcome      |
      | 30,000 | won          |
      | 3,000  | never bid on |
