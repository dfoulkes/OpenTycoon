# Spec §8.1 (spawn sources, visitor split), §8.2 (allowance and shopping list) and §8.4
# (day/night). Shoppers are walking agents; their numbers come from the population.
Feature: Shoppers come from home and from the tube
  Residents leave their flats for the day; everyone else arrives at a tube station. Each has
  a daily allowance and a shopping list that depend on who they are.

  Background:
    Given the shipped config
    And the shipped config with "customers.yaml" key "/desires/0/goods" set to "{bread: 1.0}"
    And the shipped config with "customers.yaml" key "/desires/1/goods" set to "{drinks: 1.0}"

  @R55
  Scenario Outline: The daily allowance depends on class, age and sex
    Then a <class> <sex> aged "<band>" has a daily allowance of £<allowance>

    Examples:
      | class   | sex   | band  | allowance |
      | working | man   | 18-29 | 18.00     |
      | working | woman | 50-64 | 20.00     |
      | middle  | woman | 30-49 | 49.50     |
      | upper   | man   | 65+   | 84.00     |

  @R55 @R17
  Scenario: The sex multiplier is configurable
    Given the shipped config with "customers.yaml" key "/allowance/sex_multiplier/female" set to "1.2"
    Then a working woman aged "30-49" has a daily allowance of £26.40
    And a working man aged "30-49" has a daily allowance of £22.00

  @R55
  Scenario Outline: What people want depends on class, age, sex, the hour and the year
    Given the shipped config
    And the shipped config with "customers.yaml" key "/desires/2/match" set to "{sex: female, age: '18-29', window: day, from_year: 1950}"
    Then P(<good> | <class> <sex> aged "<band>", <window>, <year>) is <p>

    Examples:
      | good    | class   | sex   | band  | window | year | p    |
      | clothes | working | man   | 30-49 | day    | 1930 | 0.05 |
      | clothes | working | woman | 18-29 | day    | 1945 | 0.05 |
      | clothes | working | woman | 18-29 | day    | 1950 | 0.2  |
      | clothes | upper   | woman | 18-29 | day    | 1950 | 0.2  |
      | clothes | working | man   | 18-29 | day    | 1950 | 0.05 |
      | meals   | upper   | woman | 65+   | night  | 1930 | 0.4  |
      | meals   | working | woman | 65+   | night  | 1930 | 0.1  |
      | bread   | working | man   | 30-49 | night  | 1930 | 0    |

  @R55
  Scenario: Each person rolls their own shopping list from those odds
    Given the shipped config with "customers.yaml" key "/desires/0/goods" set to "{bread: 1.0, groceries: 0.0, clothes: 1.0}"
    Then a working man aged "30-49" shopping by day wants "bread, clothes"
    And a working man aged "30-49" shopping by night wants "drinks"

  @R25 @R59
  Scenario: Residents leave home and visitors arrive by tube, 70% commuters and 30% tourists
    Given an apartment block "Flats" on "holborn" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a city of 100 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    When the city runs to "08:30:00"
    Then 101 shoppers came out by day
    And 1 of them came from home and 100 from the tube
    And 70 commuters and 30 tourists came out by day
    And every resident set out from home and every visitor from a tube station
    And "Bun" sold 101 bread

  @R25
  Scenario: Day shoppers still on the map at 17:00 despawn
    Given the shipped config with "customers.yaml" key "/movement/spawn_spread_minutes" set to "600"
    And the shipped config with "customers.yaml" key "/movement/cells_per_tick" set to "1"
    And a city of 2000 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 5000 bread
    When the city runs to "16:59:50"
    Then some day shoppers are still about
    When the city runs to "17:00:00"
    Then nobody is on the map

  @R25 @R59
  Scenario: After night trade residents walk home and visitors take the train
    Given the shipped config with "customers.yaml" key "/movement/spawn_spread_minutes" set to "600"
    And an apartment block "Flats" on "old_kent_road" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a working man aged "18-29" has lived in "Flats" since 1930
    And a city of 200 working men aged "30-49"
    And a bar "Pub" on "mayfair" with 1000 drinks
    When the city runs to "23:50:00"
    Then every night shopper still about is heading home or to a tube station
    When the city runs to "01:00:00"
    Then nobody is on the map

  @R25
  Scenario: Shoppers spend only what they have
    Given the shipped config with "customers.yaml" key "/allowance/base_by_class/working" set to "0.10"
    And a city of 10 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    When the city runs to "12:00:00"
    Then "Bun" sold 0 bread
