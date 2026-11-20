# Spec §9. Each historical event fires on the year step covering its year (1939 on the 1935
# step, the crash of 2008 on 2005). Effects are typed modifiers from events.yaml; lasting ones
# sit on one modifier stack that the rest of the game queries.
Feature: History makes the front page and moves the market

  Background:
    Given the shipped config with "customers.yaml" key "/initial_population" set to "200"

  @R16
  Scenario Outline: Each event fires on the step covering its year
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "<start>"
    And a new game with 2 players
    When the game runs to midnight
    Then the front page for <step> reads "<headline>"

    Examples:
      | start | step | headline                             |
      | 1930  | 1935 | War Declared: Rationing Begins       |
      | 1935  | 1940 | The Blitz: East End Ablaze           |
      | 1945  | 1950 | Long Live the Queen                  |
      | 1960  | 1965 | England Win the World Cup            |
      | 1965  | 1970 | Decimal Day: Goodbye to the Shilling |
      | 1965  | 1970 | Three-Day Week as Miners Strike      |
      | 1980  | 1985 | Big Bang in the City                 |
      | 1995  | 2000 | Dot-com Fever                        |
      | 2000  | 2005 | Banks Collapse                       |
      | 2005  | 2010 | London Olympics Open                 |
      | 2015  | 2020 | Lockdown                             |

  # WWII (1939, the 1935 step): luxury demand −60%, staples capped at half a shop's capacity,
  # for two days.
  @R16
  Scenario: Wartime rationing cuts luxury demand and caps staples
    Given a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 3x3 "grocery" on "Soho"
    And the "grocery" on "Soho" has room for 100 of each good
    And the "grocery" on "Soho" keeps a standing order of 100 "groceries"
    When the game runs to midnight
    Then the "grocery" on "Soho" has ordered 50 "groceries" for tomorrow
    And demand for "clothes" by working shoppers is at 40%
    And demand for "bread" by working shoppers is at 100%
    When the game runs for 2 days
    Then demand for "clothes" by working shoppers is at 100%
    And the "grocery" on "Soho" has ordered 100 "groceries" for tomorrow

  # A 10-high 2×2 bakery on Whitechapel (Low, ×0.6) cost 40 × £10 × 0.6 = £240.
  @R16
  Scenario: The Blitz destroys East End buildings and the city pays half their value
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1935"
    And the shipped config with "events.yaml" key "/historical/1/effects/0/chance" set to "1.0"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Whitechapel"
    And player 1 owns a 10-high 2x2 "bakery" on "Mayfair"
    And player 1 has a balance of "£100.00"
    When the game runs to midnight
    Then the "bakery" on "Whitechapel" is gone
    And the "bakery" on "Mayfair" still stands
    And player 1's balance is "£220.00"
    And no money was created or destroyed

  @R16
  Scenario: Decimal Day flips the currency and confuses prices for a day
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1965"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And player 1 owns a 10-high 2x2 "bakery" on "Soho"
    And player 1 owns a 10-high 2x2 "bakery" on "Mayfair"
    And the "bakery" on "Old Kent Road" is priced at 100% for "bread"
    And the "bakery" on "Soho" is priced at 100% for "bread"
    And the "bakery" on "Mayfair" is priced at 100% for "bread"
    Then the currency shows pounds, shillings and pence
    When the game runs to midnight
    Then the currency shows decimal
    And every shelf price is within 10% of usual and some are off
    When the game runs to midnight
    Then every shelf price is back to usual
    And the currency shows decimal

  @R16
  Scenario: The three-day week lets each shop open only 3 days in 7
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1965"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 has a bakery selling bread on each of "Old Kent Road, Whitechapel, Bethnal Green, Soho, Holborn, Bloomsbury, Knightsbridge"
    When the game runs to midnight
    Then 3 of the 7 bakeries may open today
    When the game runs to "17:00"
    Then only the bakeries allowed to open sold bread today

  @R16
  Scenario: The Big Bang lifts the City's desirability
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1980"
    And a new game with 2 players
    When the game runs to midnight
    Then "Holborn" is a high-tier block

  # The dot-com swing applies to the business types events.yaml lists; bakeries stand in for
  # online businesses here. A 10-high 2×2 bakery on Soho (Medium) cost £400: ×2.0, then ×0.4.
  @R16
  Scenario: The dot-com boom and bust swing business values
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "1995"
    And the shipped config with "events.yaml" key "/historical/8/effects/0/types" set to "[bakery]"
    And the shipped config with "events.yaml" key "/historical/9/effects/0/types" set to "[bakery]"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Soho"
    When the game runs to midnight
    Then the "bakery" on "Soho" is worth "£320.00"

  @R16
  Scenario: The crash cuts land values and upper-class spending
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "2000"
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 holds the lease on "Soho" bought for "£5,000.00" in 2000
    When the game runs to midnight
    Then the lease on "Soho" is valued at "£3,500.00"
    And demand for "bread" by upper shoppers is at 80%
    And demand for "bread" by working shoppers is at 100%

  @R16
  Scenario: The Olympics bring footfall to the East End
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "2005"
    And a new game with 2 players
    When the game runs to midnight
    Then shops on "Whitechapel" draw 150% of their usual footfall
    And shops on "Mayfair" draw 100% of their usual footfall

  @R16
  Scenario: The pandemic empties the high street
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "2015"
    And a new game with 2 players
    When the game runs to midnight
    Then demand for "bread" by middle shoppers is at 30%

  @R16
  Scenario: From 2030 the paper can carry a random event
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "2025"
    And the shipped config with "events.yaml" key "/random_era/chance_per_step" set to "1.0"
    And a new game with 2 players
    When the game runs to midnight
    Then the front page for 2030 carries one of the random 2030+ headlines
