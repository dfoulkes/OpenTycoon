# Spec §8.3 Shop choice: class affinity × price factor × distance decay × reputation, picked
# by weighted random. Reputation, fit-out and class split are kept by R16; the price
# sensitivities are configurable (R17).
Feature: Choosing a shop
  Shoppers weigh every open shop that has the good in stock. Working-class shoppers watch the
  price; upper-class shoppers barely notice it. The best shop doesn't win every customer.

  Background:
    Given the shipped config

  @R17
  Scenario Outline: The price factor is exp(-k × (price/ref − 1)) with k by class
    Then the price factor for a <class> shopper at <pct>% of the reference price is <factor>

    Examples:
      | class   | pct | factor |
      | working | 100 | 1.0000 |
      | working | 120 | 0.5488 |
      | middle  | 120 | 0.7408 |
      | upper   | 120 | 0.9048 |
      | working | 80  | 1.8221 |

  @R16
  Scenario: The pick is weighted random, not the top score
    Given a bakery "Good" on "holborn" with 1000 bread
    And a bakery "Fair" on "holborn" with 1000 bread
    And "Fair" prices bread at 120% of the reference price
    When 1000 working shoppers at "holborn" choose where to buy bread
    Then both "Good" and "Fair" were chosen
    And "Good" was chosen more often than "Fair"

  @R16
  Scenario: Working-class shoppers chase the cheap shop; upper-class shoppers hardly care
    Given a bakery "Cheap" on "holborn" with 1000 bread
    And a bakery "Dear" on "holborn" with 1000 bread
    And "Dear" prices bread at 150% of the reference price
    When 1000 working shoppers at "holborn" choose where to buy bread
    Then "Cheap" was chosen by at least 75% of them
    When 1000 upper shoppers at "holborn" choose where to buy bread
    Then "Cheap" was chosen by at most 65% of them

  @R16
  Scenario: Each class is drawn to its own kind of district
    Given a bakery "East" on "whitechapel" with 1000 bread
    And a bakery "West" on "belgravia" with 1000 bread
    When 1000 working shoppers at "holborn" choose where to buy bread
    Then "East" was chosen more often than "West"
    When 1000 upper shoppers at "holborn" choose where to buy bread
    Then "West" was chosen more often than "East"

  @R16
  Scenario: Nearer shops pull harder
    Given a bakery "Near" on "holborn" with 1000 bread
    And a bakery "Far" on "bloomsbury" with 1000 bread
    When 1000 middle shoppers at "holborn" choose where to buy bread
    Then "Near" was chosen more often than "Far"

  @R16
  Scenario: A better reputation pulls harder; a shop with no stock isn't scored
    Given a bakery "Loved" on "holborn" with 1000 bread
    And a bakery "Shunned" on "holborn" with 1000 bread
    And a bakery "Empty" on "holborn" with 0 bread
    And "Loved" has a reputation of 0.9
    And "Shunned" has a reputation of 0.1
    When 1000 middle shoppers at "holborn" choose where to buy bread
    Then "Loved" was chosen more often than "Shunned"
    And "Empty" was never chosen

  @R16 @R25
  Scenario: Sold out on arrival is a lost sale: reputation dents and the shopper re-scores
    Given the shipped config with "customers.yaml" key "/desires/0/goods" set to "{bread: 1.0}"
    And the shipped config with "customers.yaml" key "/movement/spawn_spread_minutes" set to "0"
    And an apartment block "Flats" on "soho" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a bakery "Near" on "holborn" with 1 bread
    And a bakery "Far" on "mayfair" with 0 bread
    When the city runs until a shopper is walking to "Near"
    And "Near" sells out of bread
    And "Far" gets 10 bread
    And the city runs until "Near" has lost a sale
    Then "Near" lost 1 sale
    And "Near" has a reputation of 0.48
    When the city runs for 1 tick
    Then a shopper is walking to "Far"
    When the city runs to "12:00:00"
    Then "Far" sold 1 bread
    And "Far" has a reputation of 0.5025
