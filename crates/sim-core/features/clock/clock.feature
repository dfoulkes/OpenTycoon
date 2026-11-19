# Spec §2.1 and §2.2. Tags name the rulings each scenario proves.
Feature: The game clock
  A game day lasts 20 real minutes and every midnight the year jumps five years,
  so a full 1930–2050 game is 24 days. Shops trade only inside their windows.

  Background:
    Given the shipped config

  @R34
  Scenario: A day lasts 20 real minutes
    Then a day is 8640 ticks
    And a day lasts 1200 real seconds

  @R34 @R17
  Scenario: Day length is configurable
    Given the shipped config with "game.yaml" key "/clock/real_minutes_per_day" set to "10"
    Then a day lasts 600 real seconds

  @R34
  Scenario: The year steps +5 at midnight and not before
    Then the year is 1930
    And the time is "00:00:00"
    When the clock runs to "23:59:50"
    Then the year is 1930
    And the midnight reckoning has not fired
    When the clock advances 1 tick
    Then the year is 1935
    And the time is "00:00:00"
    And the midnight reckoning has fired

  @R34
  Scenario: A full 1930–2050 game is 24 days
    Then a full game is 24 days
    When the clock runs for 23 days
    Then the year is 2045
    And the game is not over
    When the clock runs for 1 day
    Then the year is 2050
    And the game is over

  @R34
  Scenario: Arrivals and settlement fire once each morning
    When the clock runs to "05:30:00"
    Then the arrivals have fired
    And the settlement has not fired
    When the clock runs to "06:00:00"
    Then the settlement has fired

  @R1
  Scenario Outline: Day and night trade open only inside their hours
    When the clock runs to "<time>"
    Then day trade is <day>
    And night trade is <night>

    Examples:
      | time     | day    | night  |
      | 06:59:50 | closed | closed |
      | 07:00:00 | open   | closed |
      | 16:59:50 | open   | closed |
      | 17:00:00 | closed | closed |
      | 18:59:50 | closed | closed |
      | 19:00:00 | closed | open   |
      | 23:49:50 | closed | open   |
      | 23:50:00 | closed | closed |

  @R30 @R38
  Scenario Outline: Hotels trade 23:30–03:00, across midnight, overlapping night trade on purpose
    When the clock runs to "<time>"
    Then hotel rooms are <hotel>
    And night trade is <night>
    And day trade is closed

    Examples:
      | time     | hotel  | night  |
      | 23:29:50 | closed | open   |
      | 23:30:00 | open   | open   |
      | 23:45:00 | open   | open   |
      | 00:00:00 | open   | closed |
      | 02:59:50 | open   | closed |
      | 03:00:00 | closed | closed |
