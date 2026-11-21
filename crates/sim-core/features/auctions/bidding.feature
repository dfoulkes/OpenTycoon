# Spec §5.1. Tags name the rulings each scenario proves.
Feature: Bidding round the table
  Bidding goes round the table, left to right, starting with the player after the opener.
  On their turn a player raises by £50, £100, £500 or £1,000, or passes. A pass counts
  only for that round. A full round with no new bids closes the auction and the highest
  bidder wins and pays at once. A turn that runs out its 10 seconds is a pass.

  # Ceilings here are measured from a zero balance, so starting cash (R62) is pinned to £0.
  Background:
    Given the shipped config
    And the shipped config with "economy.yaml" key "/money/starting_cash" set to "0"
    And a game with 4 players

  @R8
  Scenario: Turns go left to right from the player after the opener, skipping the high bidder
    When player 2 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    Then it is player 3's turn
    When player 3 passes
    Then it is player 0's turn
    When player 0 passes
    Then it is player 1's turn
    When player 1 raises by £100
    Then it is player 2's turn
    When player 2 passes
    Then it is player 3's turn

  @R8
  Scenario Outline: Raises are £50, £100, £500 or £1,000
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And player 1 raises by £<by>
    Then the command is <result>

    Examples:
      | by    | result                      |
      | 50    | accepted                    |
      | 100   | accepted                    |
      | 500   | accepted                    |
      | 1,000 | accepted                    |
      | 75    | refused: not an increment   |
      | 2,000 | refused: not an increment   |

  @R8
  Scenario: A pass counts for that round only
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And player 1 passes
    And player 2 raises by £100
    And player 3 passes
    And player 0 passes
    Then it is player 1's turn
    When player 1 raises by £50
    Then the command is accepted
    And the high bid is £2,150 by player 1

  @R8
  Scenario: A full round with no new bids closes the auction; the highest bidder wins and pays at once
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And player 1 passes
    And player 2 raises by £100
    And player 3 passes
    And player 0 passes
    Then an auction is on
    When player 1 passes
    Then no auction is on
    And player 2 holds the lease on "Old Kent Road" with 30 years left
    And player 2 has paid £2,100
    And the city has received £2,100 from auctions

  @R8
  Scenario: Only the player whose turn it is may bid
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And player 2 raises by £50
    Then the command is refused: not your turn
    And it is player 1's turn

  @R8
  Scenario: A turn that runs out its 10 seconds counts as a pass
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And 9 real seconds pass
    Then it is player 1's turn
    When 1 real second passes
    Then it is player 2's turn
    When 20 real seconds pass
    Then no auction is on
    And player 0 holds the lease on "Old Kent Road" with 30 years left

  @R8 @R17
  Scenario: The turn timer is configurable
    Given the shipped config with "auctions.yaml" key "/turn_real_seconds" set to "5"
    And a game with 4 players
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 120 real seconds pass
    And 5 real seconds pass
    Then it is player 2's turn

  # §5.1 / R7: a bid may not exceed balance + credit limit (£0 + £10,000 at the start).
  @R8
  Scenario: A raise above the bidder's ceiling is refused
    When player 0 opens an auction on "Old Kent Road" with £9,500
    And 120 real seconds pass
    And player 1 raises by £500
    Then the command is accepted
    When player 2 raises by £50
    Then the command is refused: over the ceiling

  # §5.1 and R7: the winner pays at once. One who can no longer pay (bankrupt, or past the
  # ceiling after the 06:00 bills) defaults, and nothing changes hands.
  @R8
  Scenario: A winner who can no longer pay at the close defaults
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And player 0 spends £9,000 elsewhere
    And the auction runs to its close with everyone passing
    Then the sale defaulted
    And "Old Kent Road" has no leaseholder
    And player 0 has paid £9,000
