Feature: Game Time
  Scenario: A new game has started
    Given the game has started
    When in the year nineteenThirty
    Then clock time should be midnight
    And game state is running

  Scenario: Time processing
    Given the game has started
    And end of day one of nineteenThirty
    When clock time is eleven fifty nine
    Then the start of the new day should be the same year as before

  Scenario: The year iterates by five years after two game days
    Given end of day two of nineteenThirty
    When clock time is eleven fifty nine
    Then the start of the new day should be Nineteen Thirty Five

  Scenario: the end should not exceed the year 2000 when playing
    Given the game has started
    When clock time is midnight in the year two thousand on day one
    Then game should end

  Scenario: The year iterates by five years after three game days
    Given the game has started
    When the game has lapsed three days
    Then game date should be "02-01-1935"
