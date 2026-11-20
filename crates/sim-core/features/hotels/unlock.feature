# Spec §4.1 and §7.3: holding the lease on every block of a colour set unlocks that set's
# Hotel, and a set has one Hotel at most (R14, R29). On the shipped 3×3 map each row is a
# three-block set: Old Kent Road, Whitechapel and Bethnal Green are the East End set.
Feature: A full set unlocks its one Hotel
  A Hotel is the reward for holding a whole colour set, and each set gets one.

  Background:
    Given the shipped config
    And a new game with 2 players

  @R14
  Scenario: Holding every block of a set unlocks the Hotel
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Whitechapel" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Bethnal Green" bought for "£2,200.00" in 1930
    When player 1 builds a 10-high 3x3 hotel on "Old Kent Road"
    Then player 1 owns a hotel on "Old Kent Road"

  @R14
  Scenario: Two blocks of a three-block set are not enough
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Whitechapel" bought for "£2,000.00" in 1930
    When player 1 builds a 10-high 3x3 hotel on "Old Kent Road"
    Then there is no hotel on "Old Kent Road"
    And a hotel on "Old Kent Road" for player 1 is refused because the set is not held in full

  @R14
  Scenario: A rival holding one block of the set keeps the Hotel locked
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Whitechapel" bought for "£2,000.00" in 1930
    And player 2 holds the lease on "Bethnal Green" bought for "£2,200.00" in 1930
    When player 1 builds a 10-high 3x3 hotel on "Old Kent Road"
    And player 2 builds a 10-high 3x3 hotel on "Bethnal Green"
    Then there is no hotel on "Old Kent Road"
    And there is no hotel on "Bethnal Green"

  @R14
  Scenario: Other buildings need only the block's own lease
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    When player 1 builds a 4-high 3x3 bakery on "Old Kent Road"
    Then player 1 owns a bakery on "Old Kent Road"

  @R29 @R14
  Scenario: One Hotel per set
    Given player 1 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Whitechapel" bought for "£2,000.00" in 1930
    And player 1 holds the lease on "Bethnal Green" bought for "£2,200.00" in 1930
    When player 1 builds a 10-high 3x3 hotel on "Old Kent Road"
    And player 1 builds a 10-high 3x3 hotel on "Bethnal Green"
    Then player 1 owns a hotel on "Old Kent Road"
    And there is no hotel on "Bethnal Green"
    And a hotel on "Bethnal Green" for player 1 is refused because the set already has its Hotel

  @R29
  Scenario: The set keeps its one Hotel when the leases change hands
    Given the city owns a 10-high 3x3 "hotel" on "Whitechapel"
    And player 2 holds the lease on "Old Kent Road" bought for "£2,000.00" in 1930
    And player 2 holds the lease on "Whitechapel" bought for "£2,000.00" in 1930
    And player 2 holds the lease on "Bethnal Green" bought for "£2,200.00" in 1930
    When player 2 builds a 10-high 3x3 hotel on "Old Kent Road"
    Then there is no hotel on "Old Kent Road"
    And a hotel on "Old Kent Road" for player 2 is refused because the set already has its Hotel

  @R29 @R30
  Scenario: Room revenue is booked at check-in, in time for that midnight
    Given player 1 has a 10-high 3x3 hotel on "Mayfair"
    When the game runs to "23:29"
    And the game runs to "23:59" watching player 1's balance
    Then some guests have checked in at player 1's hotel
    And player 1 was paid the room price for each guest as they checked in
