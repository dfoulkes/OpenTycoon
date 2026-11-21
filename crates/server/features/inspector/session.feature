# Spec §10d (R88): the inspector server runs the real sim and streams it to the client as one
# snapshot, then deltas (docs/inspector-protocol.md). The server is authoritative and the viewer
# only watches (R40).
Feature: The inspector session
  A new game seats an AI opponent in every seat, and the session turns the world into what the
  client draws: the board, the lots and their values, the players, live auctions and bids, and
  the walking shoppers.

  Background:
    Given the shipped config and server settings

  @R88 @R40
  Scenario: Every seat of a new game is an AI and the viewer is a spectator
    When I start an inspector game with seed 42 and 4 players
    Then there are 4 players
    And every seat is played by an AI opponent named after its personality

  @R88
  Scenario Outline: A game needs a seat count the config allows
    When I try to start an inspector game with seed 1 and <players> players
    Then the game is refused naming "players must be between"

    Examples:
      | players |
      | 1       |
      | 6       |

  @R88
  Scenario: The snapshot shows the whole board
    When I start an inspector game with seed 42 and 4 players
    And I take a snapshot
    Then the board has 9 columns, 9 rows and 81 blocks
    And the snapshot has one lot per block, in board order
    And the board has river water and 5 bridges
    And no street rectangle overlaps the river

  @R88
  Scenario: A lot in the snapshot shows the world's values
    When I start an inspector game with seed 42 and 4 players
    And I play 3 game days through the session
    And I take a snapshot
    Then every lot's owner, lease, land value and building match the world
    And every lot's profit per day is its takings plus its rent less its costs
    And every business's goods table matches the world

  @R88
  Scenario: City-run buildings are marked
    When I start an inspector game with seed 42 and 4 players
    And I take a snapshot
    Then some lot's business is run by the city

  @R88
  Scenario: A delta carries only what changed
    When I start an inspector game with seed 42 and 4 players
    And I take a snapshot
    And I take a delta
    Then the delta has no lots, no players and no bids
    When I play 1 game day through the session
    And I take a delta
    Then the delta has some lots or players

  @R88
  Scenario: Live auctions show on their lot, with the bids in the feed
    When I start an inspector game with seed 42 and 4 players
    And I play through the session until an auction opens
    Then the live auction names a block of the board
    And a bid event "open" names that block
    And that lot is open for bids
    When I play through the session until bidding starts
    Then the auction room has one seat per player and only the seat whose turn it is is to act
    When I play through the session until that auction closes
    Then a bid event "won" or "unsold" names that block
    And the snapshot's recent bids include that result

  @R88
  Scenario: Shoppers walk the streets, at most max_walkers of them
    Given server settings with max_walkers 10
    When I start an inspector game with seed 42 and 4 players
    And I play through the session until shoppers are out
    And I take a delta
    Then the delta has between 1 and 10 walkers
    And every walker stands on the board

  @R88 @R40
  Scenario: Watching a game does not change it
    When I start an inspector game with seed 42 and 4 players
    And I play 2 game days through the session, taking a delta every 7 ticks
    Then the state hash equals a headless game of seed 42 and 4 AI seats run for as many ticks

  @R88
  Scenario: The final whistle sends the result once
    Given the config with the game ending after 1 game day
    When I start an inspector game with seed 42 and 4 players
    And I play 2 game days through the session
    Then the game is over with a winner
    And the result is sent only once
