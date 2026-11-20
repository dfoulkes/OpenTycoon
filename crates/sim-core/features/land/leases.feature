# Spec §5.2. Tags name the rulings each scenario proves.
Feature: Leases and land rent
  The winner of a land auction holds the lease for 30 in-game years and collects the
  land rent from every building on the block. Unleased land pays rent to the city.
  When a lease runs out the block simply goes back to auction, and leases change hands
  only by auction.

  Background:
    # The arithmetic below is the 3×3 map's.
    Given the shipped config on the v1 map
    # Enough cash to stay solvent through the midnights these scenarios run past (§2.3).
    And the shipped config with "game.yaml" key "/players/starting_cash" set to "20000"
    And a game with 3 players

  @R9
  Scenario: A lease runs 30 years, then the block goes straight back to auction
    When player 1 wins the lease on "Old Kent Road" for £2,000
    Then player 1 holds the lease on "Old Kent Road" with 30 years left
    When 5 game days go by
    Then the year is 1955 in the game
    And player 1 holds the lease on "Old Kent Road" with 5 years left
    When the game runs to the next midnight
    Then the year is 1960 in the game
    And "Old Kent Road" has no leaseholder
    And a re-auction of "Old Kent Road" is open with the city as the seller
    And player 1 has paid £2,000

  @R9
  Scenario: The re-auction is a plain auction and the winner gets a fresh 30-year lease
    When player 1 wins the lease on "Old Kent Road" for £2,000
    And the game runs to 1960
    And 120 real seconds pass
    Then it is player 0's turn
    When player 0 raises by £50
    And player 1 passes
    And player 2 passes
    Then player 0 holds the lease on "Old Kent Road" with 30 years left
    And the city has received £4,050 from auctions

  @R9
  Scenario: Buildings stay through a re-auction and pay rent to the new holder
    Then the city owns the apartment block on "Whitechapel"
    And the land rent from the apartment block on "Whitechapel" goes to the city
    When player 1 wins the lease on "Whitechapel" for £2,000
    Then the land rent from the apartment block on "Whitechapel" goes to player 1
    When the game runs to 1960
    Then "Whitechapel" has no leaseholder
    And the city owns the apartment block on "Whitechapel"
    And the land rent from the apartment block on "Whitechapel" goes to the city
    When 120 real seconds pass
    And player 0 raises by £100
    And player 1 passes
    And player 2 passes
    Then the city owns the apartment block on "Whitechapel"
    And the land rent from the apartment block on "Whitechapel" goes to player 0

  @R33
  Scenario: A leaseholder can auction their lease and the buyer gets the years left
    When player 1 wins the lease on "Old Kent Road" for £2,000
    And 2 game days go by
    And player 1 offers their lease on "Old Kent Road" for auction
    And 120 real seconds pass
    Then it is player 2's turn
    When player 2 raises by £500
    And player 0 passes
    Then no auction is on
    And player 2 holds the lease on "Old Kent Road" with 20 years left
    And player 2 has paid £2,500
    And player 1 has gained £500

  @R33
  Scenario: A lease cannot be bought from its holder except at an auction they start
    When player 1 wins the lease on "Old Kent Road" for £2,000
    And player 2 opens an auction on "Old Kent Road" with £5,000
    Then the command is refused: not for sale
    When player 2 offers their lease on "Old Kent Road" for auction
    Then the command is refused: not yours
    And player 1 holds the lease on "Old Kent Road" with 30 years left
