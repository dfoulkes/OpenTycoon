Feature: Game Time
  Scenario: A new game has started
    Given the game has started
    When in the year nineteenThirty
    Then clock time should be midnight

  Scenario: Time processing
    Given end of day one of nineteenThirty
    When clock time is eleven fifty nine
    Then the start of the new day should be the same year as before
