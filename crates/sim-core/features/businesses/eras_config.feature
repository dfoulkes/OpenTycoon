# Spec §10b: a bad businesses.yaml or desire table never loads, and every problem names its
# key (R45). Era content (§7.1, §7.4, §8.2) adds unlock and obsolete years, sales channels, the
# stats formula's reference shop, scale rules and the obsolescence fade.
Feature: Era content fails loudly

  Background:
    Given the shipped config

  @R45
  Scenario Outline: A bad era setting in businesses.yaml fails and names the key
    Given the shipped config with "businesses.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "businesses.yaml" and "<named>"

    Examples:
      | key                           | value       | named                                               |
      | /types/bakery/obsolete_year   | 1920        | /types/bakery/obsolete_year: obsolete before it unlocks |
      | /goods/newspapers/unlock_year | 2020        | /goods/newspapers/obsolete_year: obsolete before it unlocks |
      | /types/bakery/goods           | [computers] | /types/bakery/goods/0: computers unlocks in 1980, after the type |
      | /types/ecommerce/unlock_year  | 1990        | /types/ecommerce/unlock_year: an online business needs the internet (1995) |
      | /types/bakery/channel         | telepathy   | /types/bakery/channel                               |
      | /reference/sell_through       | 0           | /reference/sell_through                             |
      | /reference/sell_through       | 1.5         | /reference/sell_through                             |
      | /reference/payback_days       | 0           | /reference/payback_days                             |
      | /reference/height             | 0           | /reference/height                                   |

  @R45 @R55
  Scenario Outline: A bad desire or fade setting in customers.yaml fails and names the key
    Given the shipped config with "customers.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "<named>"

    Examples:
      | key                       | value             | named                                                    |
      | /desires/0/goods          | {computers: 0.5}  | /desires/0/goods/computers: never wanted               |
      | /desires/12/goods/bread   | 11                | /desires/12/goods/bread: a scale must be between 0 and 10 |
      | /obsolescence/fade_years  | 0                 | /obsolescence/fade_years                                 |
      | /obsolescence/floor       | 1.5               | /obsolescence/floor                                      |
