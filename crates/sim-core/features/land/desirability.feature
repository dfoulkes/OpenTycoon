# Spec §4.2, §5.1. Tags name the rulings each scenario proves.
Feature: Desirability shifts over the years
  Districts change: Shoreditch, Hackney and Brixton gentrify between 1990 and 2010, Docklands
  goes upmarket after 1987 and Notting Hill climbs during the 1980s. Each shift lands at the
  first midnight year step that reaches its year, moves the district's land base price with it,
  and is announced with a newspaper headline.

  Background:
    Given the shipped config with "map.yaml" key "/map" set to "full"

  @R16 @R23
  Scenario Outline: Each shift lands in the right year
    Given the full board brought to <before>
    Then "<district>" is <tier before> tier
    When the board's year steps to <after>
    Then "<district>" is <tier after> tier

    Examples:
      | district     | before | tier before | after | tier after |
      | Notting Hill | 1980   | Medium      | 1985  | High       |
      | Docklands    | 1985   | Low         | 1990  | High       |
      | Shoreditch   | 1990   | Low         | 1995  | Medium     |
      | Shoreditch   | 2000   | Medium      | 2005  | High       |
      | Hackney      | 1995   | Low         | 2000  | Medium     |
      | Hackney      | 2005   | Medium      | 2010  | High       |
      | Brixton      | 1995   | Low         | 2000  | Medium     |
      | Whitechapel  | 1930   | Low         | 2050  | Low        |

  @R16 @R23
  Scenario: A shift moves the district's land base price
    Given the full board brought to 1985
    Then the land base price of "Docklands" is £1,500
    When the board's year steps to 1990
    Then the land base price of "Docklands" is £12,000

  @R16
  Scenario: The midnight year step applies the shift and puts it on the front page
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1985"
    And a game with 2 players
    Then "Docklands" is Low tier in the game
    When the game runs to the next midnight
    Then the year is 1990 in the game
    And "Docklands" is High tier in the game
    And the front page for 1990 reads "Canary Wharf Rises: Docklands Goes Upmarket"

  @R16 @R23
  Scenario: A game that starts late starts with the shifts already made
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1990"
    And a game with 2 players
    Then "Docklands" is High tier in the game
    And "Notting Hill" is High tier in the game
    And "Shoreditch" is Low tier in the game

  @R16
  Scenario: Land rent follows the new tier
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1985"
    And a game with 2 players
    Then a 3x3 building 10 high on "Docklands" would pay £2.70 land rent a day
    When the game runs to the next midnight
    Then a 3x3 building 10 high on "Docklands" would pay £8.10 land rent a day
