# Spec §7.5 and §8.2 (R15, R36): from 1995 a growing share of each good's demand goes online,
# with no travel, out of the shopper's allowance. Online sellers need the Internet (R11) and
# compete on price and a brand that builds over time. Physical retail loses what goes online.
Feature: Online shopping from 1995
  Shoppers can buy part of their list online instead of walking to a shop.

  Background:
    Given the shipped config
    And the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0}}]"
    And the shipped config with "customers.yaml" key "/online/share_by_year" set to "{1995: 0.05, 2020: 0.5}"
    And the shipped config with "customers.yaml" key "/online/good_multiplier" set to "{}"
    And the shipped config with "businesses.yaml" key "/types/web_bakery" set to "{name: Web Bakery, min_footprint: [2, 2], max_height: 4, stock_density: 0.5, hours: day, goods: [bread], utility_factor: {electricity: 1.0, gas: 0.0, water: 0.0, telecoms: 2.0}, unlock_year: 1995, channel: online}"

  @R15 @R36
  Scenario Outline: Online spend starts in 1995 and grows
    Given the game starts in <year>
    And a city of 1000 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 100000 bread
    And an online bakery "WebBun" on "old_kent_road" with 100000 bread
    When the city runs to "12:00:00"
    Then between <lo>% and <hi>% of the bread sold was sold online

    Examples:
      | year | lo | hi |
      | 1990 | 0  | 0  |
      | 1995 | 3  | 7  |
      | 2005 | 18 | 28 |
      | 2020 | 45 | 55 |
      | 2040 | 45 | 55 |

  @R15
  Scenario: Physical demand drops by the online share
    Given the game starts in 2020
    And a city of 1000 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 100000 bread
    And an online bakery "WebBun" on "old_kent_road" with 100000 bread
    When the city runs to "12:00:00"
    Then "Bun" sold and "WebBun" sold online 1000 bread between them
    And "Bun" sold between 45% and 55% fewer bread than the 1000 wanted
    And "WebBun" sold 0 bread

  @R15
  Scenario: With no online seller, everyone walks
    Given the game starts in 2020
    And a city of 1000 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 100000 bread
    When the city runs to "12:00:00"
    Then "Bun" sold 1000 bread

  @R15
  Scenario: Online spend follows the Internet's arrival, not the calendar
    Given the shipped config with "utilities.yaml" key "/internet/upgrade_year" set to "1985"
    And the shipped config with "businesses.yaml" key "/types/web_bakery/unlock_year" set to "1985"
    And the shipped config with "customers.yaml" key "/online/share_by_year" set to "{1985: 0.5}"
    And the game starts in 1990
    And a city of 1000 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 100000 bread
    And an online bakery "WebBun" on "old_kent_road" with 100000 bread
    When the city runs to "12:00:00"
    Then between 45% and 55% of the bread sold was sold online

  @R15 @R63
  Scenario: Online spending comes out of the shopper's allowance
    Given the game starts in 2020
    And the shipped config with "customers.yaml" key "/allowance/base_by_class/working" set to "1.00"
    And a city of 100 working men aged "30-49"
    And an online bakery "WebBun" on "old_kent_road" with 100000 bread
    When the city runs to "12:00:00"
    Then some bread was sold online
    And no shopper spent more than their day budget

  @R15
  Scenario: Online sellers compete on price
    Given the game starts in 2020
    And the shipped config with "customers.yaml" key "/online/share_by_year" set to "{1995: 1.0}"
    And a city of 1000 working men aged "30-49"
    And an online bakery "WebCheap" on "old_kent_road" with 100000 bread
    And an online bakery "WebDear" on "whitechapel" with 100000 bread
    And "WebCheap" prices bread at 60% of the reference price
    And "WebDear" prices bread at 150% of the reference price
    When the city runs to "12:00:00"
    Then "WebCheap" sold more bread online than "WebDear"

  @R15
  Scenario: Online sellers compete on brand
    Given the game starts in 2020
    And the shipped config with "customers.yaml" key "/online/share_by_year" set to "{1995: 1.0}"
    And a city of 1000 working men aged "30-49"
    And an online bakery "WebNew" on "old_kent_road" with 100000 bread
    And an online bakery "WebKnown" on "whitechapel" with 100000 bread
    And "WebNew" has a brand of 0.1
    And "WebKnown" has a brand of 0.9
    When the city runs to "12:00:00"
    Then "WebKnown" sold more bread online than "WebNew"

  @R15
  Scenario: A brand builds over time
    Given the game starts in 2020
    And the shipped config with "customers.yaml" key "/online/brand" set to "{initial: 0.2, growth_per_day: 0.1, max: 0.35}"
    And an online bakery "WebBun" on "old_kent_road" with 100 bread
    Then "WebBun" has a brand of 0.2
    When the city runs to "07:00:00"
    Then "WebBun" has a brand of 0.3
    When the city runs to "07:00:00"
    Then "WebBun" has a brand of 0.35

  @R15
  Scenario: A good only online sellers sell is always bought online
    Given the game starts in 2010
    And the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {music_streams: 1.0}}]"
    And a city of 100 working men aged "30-49"
    And an online "music_streaming" seller "Tunes" on "old_kent_road" with 100000 music_streams
    When the city runs to "12:00:00"
    Then "Tunes" sold 100 music_streams online
