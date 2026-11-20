# Spec §5.1. Tags name the rulings each scenario proves.
Feature: Opening an auction
  Any player can start an auction on an unowned buildable block by placing an opening
  bid of at least the district's land base price. Only one auction may be scheduled or
  running at a time, and bidding starts 120 real seconds after the opening bid.

  Background:
    Given the shipped config

  @R23
  Scenario Outline: The minimum opening bid is the district's land base price
    Given the shipped config with "game.yaml" key "/players/starting_cash" set to "20000"
    And a game with 2 players
    When player 0 opens an auction on "<district>" with £<bid>
    Then the command is <result>

    Examples:
      | district      | bid    | result                          |
      | Old Kent Road | 2,000  | accepted                        |
      | Old Kent Road | 1,950  | refused: below the minimum      |
      | Bloomsbury    | 5,500  | accepted                        |
      | Bloomsbury    | 5,450  | refused: below the minimum      |
      | Mayfair       | 14,000 | accepted                        |
      | Mayfair       | 13,950 | refused: below the minimum      |

  @R23
  Scenario: Affluent districts cost more to open than poor ones
    Given a game with 2 players
    Then the minimum opening bid on "Mayfair" is more than on "Soho"
    And the minimum opening bid on "Soho" is more than on "Old Kent Road"

  # §5.1: one auction scheduled or running at a time, across the whole game.
  @R8
  Scenario: Only one auction may be scheduled or running at a time
    Given a game with 3 players
    When player 0 opens an auction on "Old Kent Road" with £2,000
    Then the command is accepted
    When player 1 opens an auction on "Soho" with £5,000
    Then the command is refused: another auction is on
    When 120 real seconds pass
    And player 1 opens an auction on "Soho" with £5,000
    Then the command is refused: another auction is on
    When player 1 passes
    And player 2 passes
    Then no auction is on
    When player 1 opens an auction on "Soho" with £5,000
    Then the command is accepted

  # §5.1: bidding starts 120 real seconds after the opening bid; the game does not pause.
  @R8
  Scenario: Bidding starts 120 real seconds after the opening bid
    Given a game with 3 players
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 119 real seconds pass
    Then bidding has not started
    When player 1 raises by £50
    Then the command is refused: bidding has not started
    When 1 real second passes
    Then it is player 1's turn

  @R8 @R17
  Scenario: The countdown is configurable
    Given the shipped config with "auctions.yaml" key "/countdown_real_seconds" set to "60"
    And a game with 2 players
    When player 0 opens an auction on "Old Kent Road" with £2,000
    And 60 real seconds pass
    Then it is player 1's turn

  # §5.1 and R7: the opening bid, like every bid, is capped at balance + credit limit.
  @R8
  Scenario: An opening bid above the bidder's ceiling is refused
    Given the shipped config with "game.yaml" key "/players/starting_cash" set to "0"
    And a game with 2 players
    When player 0 opens an auction on "Mayfair" with £14,000
    Then the command is refused: over the ceiling
