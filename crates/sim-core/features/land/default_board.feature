# Spec §4.1, §4.2, §9. Tags name the rulings each scenario proves.
Feature: The full board is the real game
  The shipped game is played on the full 9×9 board. History happens to its real districts: the
  Blitz falls on the East End and the City, the Olympics bring crowds to Stratford, and Big Bang
  lifts the City's fringe. The 3×3 grey-box map stays available as `map: v1`.

  @R26
  Scenario: The default game is played on the 9×9 board
    Given the shipped config
    And a game with 2 players
    Then the game's board is 9 blocks wide and 9 blocks deep
    And "City Hall" is on the game's board

  @R16 @R26
  Scenario: The Blitz damages East End and City blocks of the full board
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1935"
    And a game with 2 players
    When the game runs to the next midnight
    Then the year is 1940 in the game
    And these blocks were damaged in 1940:
      | district      |
      | Whitechapel   |
      | Bethnal Green |
      | Stepney       |
      | Docklands     |
      | The City      |
      | St Paul's     |
    And "Mayfair" was not damaged

  @R16 @R26
  Scenario Outline: Historical events name real blocks of the full board
    Given the shipped config
    Then the "<event>" event acts on "<district>"

    Examples:
      | event        | district     |
      | blitz        | Whitechapel  |
      | blitz        | The City     |
      | olympics     | Stratford    |
      | big_bang     | Spitalfields |
      | canary_wharf | Docklands    |

  @R26 @R17
  Scenario: A v1 game still loads the shipped events, whose blocks are on the full board
    Given the shipped config with "map.yaml" key "/map" set to "v1"
    When the config is loaded
    Then loading succeeds

  @R26 @R17
  Scenario: An event naming a block on no board still fails
    Given the shipped config with "events.yaml" key "/historical/1/effects/0/blocks" set to "[atlantis]"
    When the config is loaded
    Then loading fails
    And an error names "events.yaml" and "atlantis"
