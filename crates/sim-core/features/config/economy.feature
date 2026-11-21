# R71: every economy weight (the levers the balance study varies) lives in one economy.yaml;
# adopting an economic model means swapping that one file. The line between economy.yaml and
# the content files is in docs/economy.md "Economy weights (R71)".
Feature: One file holds the economy's weights
  economy.yaml is the ninth config file. It is required, validated like the other eight, and
  the only place an economy lever can be set: a lever left in its old file is an unknown key.

  Background:
    Given the shipped config

  # ---- Required and validated ---------------------------------------------------------------

  @R71 @R45
  Scenario: The shipped economy.yaml loads with the other eight files
    When the config is loaded
    Then loading succeeds

  @R71 @R45
  Scenario: economy.yaml is required
    Given the shipped config without "economy.yaml"
    When the config is loaded
    Then loading fails
    And an error names "economy.yaml" and "missing"

  @R71 @R45
  Scenario: An unknown key in economy.yaml fails and names the file and the key
    Given the shipped config with "economy.yaml" key "/money/starting_cahs" set to "7000"
    When the config is loaded
    Then loading fails
    And an error names "economy.yaml" and "starting_cahs"

  @R71 @R45
  Scenario Outline: An out-of-range economy value fails and names the file and the key
    Given the shipped config with "economy.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "economy.yaml" and "<key>"

    Examples:
      | key                            | value |
      | /land/lease_term_years         | 0     |
      | /land/lot_property_uplift      | -0.2  |
      | /money/sell_to_city_ratio      | 1.5   |
      | /money/overdraft/start_limit   | -1    |
      | /population/births_multiplier  | -0.5  |
      | /demand_multipliers/1950s      | -1    |
      | /demand_multipliers/1950s      | 11    |
      | /residential/rent_tax          | 1.5   |
      | /residential/stretch_share     | 0.1   |

  @R71 @R45
  Scenario Outline: Demand multipliers are named by decade and cover the start of the game
    Given the shipped config with "economy.yaml" key "/demand_multipliers" set to "<table>"
    When the config is loaded
    Then loading fails
    And an error names "economy.yaml" and "<named>"

    Examples:
      | table                       | named                     |
      | {1930s: 1.0, "1950": 1.0}   | /demand_multipliers/1950  |
      | {1930s: 1.0, 1955s: 1.0}    | /demand_multipliers/1955s |
      | {1950s: 1.0}                | start year 1930           |

  # ---- A lever in its old place is rejected -------------------------------------------------

  @R71 @R45
  Scenario Outline: A lever left in its old file is an unknown key there
    Given the shipped config with "<file>" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "<file>" and "<name>"

    Examples:
      | file            | key                            | value                                                            | name                |
      | map.yaml        | /land_base_rate                | 0.05                                                             | land_base_rate      |
      | game.yaml       | /leases                        | {term_years: 30}                                                 | leases              |
      | game.yaml       | /net_worth                     | {lot_property_uplift: 0.2, land_value: book_decay}               | net_worth           |
      | businesses.yaml | /build_base_rate               | 10                                                               | build_base_rate     |
      | utilities.yaml  | /base_rates                    | {electricity: 0.02, gas: 0.01, water: 0.01, telecoms: 0.01}      | base_rates          |
      | utilities.yaml  | /internet/base_rate            | 0.02                                                             | base_rate           |
      | game.yaml       | /players/starting_cash         | 7000                                                             | starting_cash       |
      | game.yaml       | /overdraft                     | {start_limit: 10000, increase_per_step: 2500, interest_rate: 0}  | overdraft           |
      | game.yaml       | /bankruptcy/sell_to_city_ratio | 0.5                                                              | sell_to_city_ratio  |
      | customers.yaml  | /migration_per_day             | 50                                                               | migration_per_day   |
      | customers.yaml  | /allowance/base_by_class       | {working: 20, middle: 45, upper: 120}                            | base_by_class       |
      | customers.yaml  | /price_sensitivity             | {working: 3.0, middle: 1.5, upper: 0.5}                          | price_sensitivity   |
      | customers.yaml  | /residential/flat_size         | 4                                                                | flat_size           |
      | customers.yaml  | /residential/rent_tax          | 0                                                                | rent_tax            |
      | customers.yaml  | /residential/commute_preference | 0                                                               | commute_preference  |

  # ---- Each lever, set in economy.yaml, takes effect ----------------------------------------

  # The reference bakery (docs/economy.md "The formula"): 2×2×10 = 40 units on a Medium block.
  # Shipped: rent 40 × £0.05 = £2.00; utilities 40 × (2p × 1.0 + 1p × 2.0 + 1p × 1.5 + 1p × 0.5)
  # = £2.40; build 40 × £10 = £400, paid back over 10 days.
  @R71 @R17
  Scenario Outline: Land, building and utility rates are read from economy.yaml
    Given the shipped config with "economy.yaml" key "<key>" set to "<value>"
    Then a reference "bakery" pays £<overheads> a day in rent and utilities in 1930
    And a reference "bakery" must make £<target> a day

    Examples:
      | key                               | value | overheads | target |
      | /land/base_rate                   | 0.05  | 4.40      | 44.40  |
      | /land/base_rate                   | 0.10  | 6.40      | 46.40  |
      | /building/base_rate               | 20    | 4.40      | 84.40  |
      | /utilities/base_rates/electricity | 0.04  | 5.20      | 45.20  |

  @R71 @R62 @R7
  Scenario: Starting cash and the overdraft are read from economy.yaml
    Given the shipped config with "economy.yaml" key "/money/starting_cash" set to "2500"
    And the shipped config with "economy.yaml" key "/money/overdraft/start_limit" set to "4000"
    And a new game with 2 players
    Then player 1's balance is "£2,500.00"
    And player 1's bid ceiling is "£6,500.00"

  @R71 @R61
  Scenario Outline: The births multiplier scales every era's births per woman
    # births = 3,200 women × 1.9 (1930) × multiplier × 5 years ÷ 32-year span
    Given the shipped config with "economy.yaml" key "/population/births_multiplier" set to "<multiplier>"
    And migration is off and nobody dies
    And a city of 3200 working women aged "18-29"
    When the year steps from 1930 to 1935
    Then <born> children were born

    Examples:
      | multiplier | born |
      | 1.0        | 950  |
      | 2.0        | 1900 |
      | 0.5        | 475  |
      | 0          | 0    |

  # The 1930s table asks a middle-class woman for bread with P 0.55 by day; the 1950s table
  # with 0.5. A decade's multiplier applies from its first year until the next decade named,
  # so 1930s covers 1930–1945, and the result is capped at 1.
  @R71 @R55 @R64
  Scenario Outline: A decade's demand multiplier scales what shoppers want in that decade
    Given the shipped config with "economy.yaml" key "/demand_multipliers/1930s" set to "<multiplier>"
    Then a middle woman aged "30-49" acts on P(bread) = <p1930> by day in 1930
    And a middle woman aged "30-49" acts on P(bread) = <p1945> by day in 1945
    And a middle woman aged "30-49" acts on P(bread) = 0.5 by day in 1950

    Examples:
      | multiplier | p1930 | p1945 |
      | 1.0        | 0.55  | 0.55  |
      | 1.2        | 0.66  | 0.66  |
      | 0.5        | 0.275 | 0.275 |
      | 2.0        | 1.0   | 1.0   |

  @R71 @R50
  Scenario: The flat size is read from economy.yaml
    Given the shipped config with "economy.yaml" key "/residential/flat_size" set to "6"
    And an apartment block "Flats" on "old_kent_road" 3×3×12
    Then "Flats" has 18 flats
