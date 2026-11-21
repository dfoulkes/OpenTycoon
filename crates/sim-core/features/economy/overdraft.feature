# Spec §3 Money (overdraft) and §5.1 (bid ceiling). Tags name the rulings each scenario proves.
Feature: The overdraft and the bid ceiling
  Every player can go overdrawn down to their credit limit: £10,000 at the start, rising by a
  flat £2,500 at every 5-year step. Nothing charges interest. A player can spend or bid up to
  their balance plus their credit limit.

  Background:
    Given the shipped config
    # The v1 placeholder bakery and bread this arithmetic is written against (the shipped
    # stats are era content: docs/economy.md).
    And the shipped config with "businesses.yaml" key "/goods/bread" set to "{name: Bread, ref_price: 0.10, purchase_price: 0.04}"
    And the shipped config with "businesses.yaml" key "/types/bakery/stock_density" set to "0.5"
    And the shipped config with "businesses.yaml" key "/types/bakery/utility_factor" set to "{electricity: 1.0, gas: 1.5, water: 1.0, telecoms: 0.5}"
    And a new game with 2 players

  @R7
  Scenario Outline: The credit limit rises a flat £2,500 every 5-year step
    When the game runs for <days> days
    Then the game year is <year>
    And the credit limit is "<limit>"

    Examples:
      | days | year | limit      |
      | 0    | 1930 | £10,000.00 |
      | 1    | 1935 | £12,500.00 |
      | 4    | 1950 | £20,000.00 |
      | 24   | 2050 | £70,000.00 |

  @R7 @R17
  Scenario: The overdraft is configurable
    Given the shipped config with "economy.yaml" key "/money/overdraft/start_limit" set to "20000"
    And the shipped config with "economy.yaml" key "/money/overdraft/increase_per_step" set to "1000"
    And a new game with 2 players
    When the game runs for 2 days
    Then the credit limit is "£22,000.00"

  @R7
  Scenario Outline: The bid ceiling is balance plus credit limit
    Given player 1 has a balance of "<balance>"
    Then player 1's bid ceiling is "<ceiling>"

    Examples:
      | balance     | ceiling    |
      | £3,000.00   | £13,000.00 |
      | £0.00       | £10,000.00 |
      | -£4,000.00  | £6,000.00  |
      | -£10,000.00 | £0.00      |
      | -£12,000.00 | -£2,000.00 |

  @R7
  Scenario: Players can spend down to minus their credit limit and no further
    Given player 1 has a balance of "-£4,000.00"
    Then player 1 can spend "£6,000.00"
    And player 1 cannot spend "£6,000.01"

  @R32
  Scenario: The 06:00 bills can push a player past the limit
    Given the game has run to "01:00"
    And player 1 owns a 10-high 2x2 "bakery" on "Old Kent Road"
    And the "bakery" on "Old Kent Road" has ordered 100 "bread" for tomorrow
    And player 1 has a balance of "-£9,999.00"
    When the game runs to "06:00"
    Then player 1's balance is "-£10,006.20"
    And player 1's bid ceiling is "-£6.20"
    And no money was created or destroyed

  @R32
  Scenario: No interest is charged on an overdraft
    Given player 1 has a balance of "-£5,000.00"
    When the game runs for 3 days
    Then player 1's balance is "-£5,000.00"

  @R17
  Scenario: An interest rate, if configured, is charged to the city at 06:00
    Given the shipped config with "economy.yaml" key "/money/overdraft/interest_rate" set to "0.01"
    And a new game with 2 players
    And the game has run to "01:00"
    And player 1 has a balance of "-£5,000.00"
    And player 2 has a balance of "£5,000.00"
    When the game runs to "06:00"
    Then player 1's balance is "-£5,050.00"
    And player 2's balance is "£5,000.00"
    And no money was created or destroyed
