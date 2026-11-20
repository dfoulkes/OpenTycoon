# Spec §9.1, R12. Every year step is a general election: a seeded roll on the weights in
# events.yaml. Weights here are forced to make the result certain.
Feature: Elections decide who owns the utilities
  A Labour win nationalises the utilities: the city buys each holder out at book value (what
  they paid at auction) and takes the revenue. A Conservative win privatises them: every
  nationalised utility goes back to auction.

  Background:
    Given the shipped config with "customers.yaml" key "/initial_population" set to "200"

  @R12
  Scenario: A Labour win nationalises the utilities and buys the holders out at book value
    Given the shipped config with "events.yaml" key "/elections/weights" set to "{1930: {labour: 1.0, conservative: 0.0}}"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 holds the "Electricity" utility bought for "£3,000.00"
    And player 2 holds the "Water" utility bought for "£2,000.00"
    And player 1 has a balance of "£100.00"
    And player 2 has a balance of "£100.00"
    When the game runs to midnight
    Then Labour won the 1935 election
    And the front page for 1935 reads "Labour Landslide: Utilities Nationalised"
    And player 1's balance is "£3,100.00"
    And player 2's balance is "£2,100.00"
    And no money was created or destroyed
    And the "Electricity" utility is nationalised
    And nobody holds the "Water" utility
    When player 1 opens an auction on the "Gas" utility with a bid of "£5,000.00"
    Then the auction house refused player 1 as not for sale

  @R12
  Scenario: Nationalised revenue goes to the city
    Given the shipped config with "events.yaml" key "/elections/weights" set to "{1930: {labour: 1.0, conservative: 0.0}}"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 holds the "Electricity" utility bought for "£3,000.00"
    And player 2 owns a 10-high 2x2 "bakery" on "Bethnal Green"
    When the game runs to midnight
    And player 1's balance changes to "£0.00"
    And the game runs to "06:00"
    Then player 1's balance is "£0.00"

  @R12
  Scenario: A Conservative win puts the nationalised utilities back up for auction
    Given the shipped config with "events.yaml" key "/elections/weights" set to "{1930: {labour: 1.0, conservative: 0.0}, 1940: {labour: 0.0, conservative: 1.0}}"
    And a new game with 2 players
    When the game runs to midnight
    Then Labour won the 1935 election
    And the "Gas" utility is nationalised
    When the game runs to midnight
    Then Conservative won the 1940 election
    And the front page for 1940 reads "Conservatives Back in Number Ten: Utilities for Sale"
    And the "Electricity" utility is up for auction, sold by the city
    And the "Gas" utility is waiting for auction
    When player 1 raises the utility bid by "£50.00"
    And everyone else passes until the auction closes
    Then player 1 holds the "Electricity" utility
    And player 1's balance is "£3,950.00"

  @R12
  Scenario: The election is a seeded roll
    When two games with seed 42 are played to 1960
    Then both games held the same elections, events and incidents
