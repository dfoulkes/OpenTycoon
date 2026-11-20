# Spec §2.3 (selling up) and §5.1. Tags name the rulings each scenario proves.
Feature: Business auctions
  Businesses are auctioned on their own, separately from land, under the same rules as
  land: the same one-at-a-time queue, the same 120-second countdown, and no shortcuts
  for distressed sellers. A land auction sells the land only.

  # The distressed-seller scenario needs player 0 overdrawn after buying, so starting cash (R62) is pinned to £0.
  Background:
    Given the shipped config
    And the shipped config with "game.yaml" key "/players/starting_cash" set to "0"
    And a game with 3 players
    And player 0 has won the apartment block on "Whitechapel" at auction

  @R5
  Scenario: A player auctions their business on its own; the land stays with its holder
    When player 2 wins the lease on "Whitechapel" for £2,000
    And player 0 offers the apartment block on "Whitechapel" for auction
    And 120 real seconds pass
    Then it is player 1's turn
    When player 1 raises by £500
    And player 2 passes
    Then player 1 owns the apartment block on "Whitechapel"
    And the sale paid player 0 £500
    And player 2 holds the lease on "Whitechapel" with 30 years left
    And the land rent from the apartment block on "Whitechapel" goes to player 2

  @R5
  Scenario: A land auction sells the land only
    When player 1 wins the lease on "Whitechapel" for £2,000
    Then player 0 owns the apartment block on "Whitechapel"
    And the land rent from the apartment block on "Whitechapel" goes to player 1

  @R5
  Scenario: No shortcuts for a distressed seller: same countdown, same queue
    Then player 0 is overdrawn
    When player 0 offers the apartment block on "Whitechapel" for auction
    Then the command is accepted
    When player 1 opens an auction on "Soho" with £5,000
    Then the command is refused: another auction is on
    When 119 real seconds pass
    Then bidding has not started
    When 1 real second passes
    Then it is player 1's turn

  @R5
  Scenario: Only the owner can put a business up for auction
    When player 1 offers the apartment block on "Whitechapel" for auction
    Then the command is refused: not yours
    When player 1 opens an auction on the apartment block on "Whitechapel" with £5,000
    Then the command is refused: not for sale

  @R5
  Scenario: A business nobody bids on stays with its owner
    When player 0 offers the apartment block on "Whitechapel" for auction
    And 120 real seconds pass
    And player 1 passes
    And player 2 passes
    Then no auction is on
    And player 0 owns the apartment block on "Whitechapel"
