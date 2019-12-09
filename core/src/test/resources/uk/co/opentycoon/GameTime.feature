Feature: Game Time
  Scenario: Core-GameTime
    Given the game has started
    When in the year 1930
    Then clock time should be 00:00am
    And year should be 1930
