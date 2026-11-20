# Spec §4.1, §4.2, §5.1. Tags name the rulings each scenario proves.
Feature: The full 9×9 board
  The full game is played on a 9×9 board laid out like a traditional board: 26 colour sets in
  eight price bands, four utility blocks and City Hall in the centre. A set's blocks are
  adjacent, value falls from the centre outwards, and the Thames cuts the board with bridges.

  Background:
    Given the shipped config with "map.yaml" key "/map" set to "full"

  @R26 @R14
  Scenario: The full board validates and has the traditional shape
    When the config is loaded
    Then loading succeeds
    And the board is 9 blocks wide and 9 blocks deep
    And the board has 26 colour sets
    And the board has these sets per band:
      | band       | sets | blocks per set |
      | brown      | 1    | 2              |
      | light_blue | 4    | 3              |
      | pink       | 4    | 3              |
      | orange     | 4    | 3              |
      | red        | 4    | 3              |
      | yellow     | 4    | 3              |
      | green      | 4    | 3              |
      | dark_blue  | 1    | 2              |
    And the board has one utility block each for electricity, gas, water and telecoms
    And "City Hall" stands at the centre and cannot be built on
    And every block is a real district, a utility or City Hall

  @R26 @R14
  Scenario: Every set's blocks are adjacent by road
    Given a game with 2 players
    Then every colour set's blocks are connected by road without leaving the set

  @R26
  Scenario: Every block reaches every other block by road, over the Thames bridges
    Given a game with 2 players
    Then every block reaches every other block by road
    And "Whitechapel" and "Old Kent Road" are joined by a bridge
    And "Tower Hill" and "Bermondsey" are not joined: no bridge there

  @R26 @R23
  Scenario: Value falls from the centre outwards
    Given a game with 2 players
    Then the mean land base price falls with each ring out from City Hall
    And every block in a dearer band has a higher land base price than any block in a cheaper band
    And "Mayfair" has the highest land base price and "Old Kent Road" the lowest

  @R26 @R23 @R14
  Scenario: Each band's tier follows the spec table at the start of the game
    Given a game with 2 players
    Then brown, light blue and pink blocks are Low tier
    And orange, red and yellow blocks are Medium tier
    And green and dark blue blocks are High tier

  @R26 @R17
  Scenario Outline: A broken board fails to load and names the problem
    Given the full board with <change>
    When the config is loaded
    Then loading fails
    And an error names "map.yaml" and "<message>"

    Examples:
      | change                                              | message                      |
      | "Mayfair" moved to the "the_city" set               | not adjacent                 |
      | the gas works holding water instead                 | no gas utility block         |
      | "Old Kent Road" made a second City Hall             | City Hall                    |
      | "Hackney" moved to the "mayfair" set                | 3 blocks                     |
      | no bridges over the Thames                          | cannot reach                 |
      | a station on the "Overground" line                  | unknown line                 |
      | a desirability shift for "Atlantis"                 | unknown block                |
      | a utility named on "Soho"                           | only on a utility block      |
