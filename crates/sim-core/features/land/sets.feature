# Spec §4.1, §7.3. Tags name the rulings each scenario proves.
Feature: Holding a full colour set
  Holding the leases on every block of a set unlocks the Hotel. The map answers whether a
  player holds a set in full; the hotel itself belongs to the businesses.

  Background:
    Given the shipped config with "map.yaml" key "/map" set to "full"
    And a game with 3 players

  @R14
  Scenario: Leases on every block of a set make a full set
    Given player 0 holds the leases on "Mayfair" and "Belgravia"
    Then player 0 holds the "mayfair" set in full
    And player 0 holds exactly these sets in full: "mayfair"
    And player 1 does not hold the "mayfair" set in full

  @R14
  Scenario: Two blocks of a three-block set are not a full set
    Given player 0 holds the leases on "Soho" and "Fitzrovia"
    Then player 0 does not hold the "soho" set in full

  @R14
  Scenario: A set split between two players is nobody's
    Given player 0 holds the leases on "Soho" and "Fitzrovia"
    And player 1 holds the leases on "Bloomsbury"
    Then player 0 does not hold the "soho" set in full
    And player 1 does not hold the "soho" set in full

  @R14 @R9
  Scenario: A full set is lost when one of its leases runs out
    Given player 0 holds the leases on "Mayfair" and "Belgravia"
    And the lease on "Belgravia" started 30 years ago
    When the game runs to the next midnight
    Then player 0 does not hold the "mayfair" set in full

  @R14
  Scenario: An unknown set is never held
    Then player 0 does not hold the "atlantis" set in full
