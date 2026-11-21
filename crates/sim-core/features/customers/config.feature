# Spec §10b: a bad customers.yaml never loads, and every problem names its key (R45).
Feature: Population and customer settings fail loudly

  Background:
    Given the shipped config

  @R45
  Scenario Outline: A bad population or customer setting fails and names the key
    Given the shipped config with "customers.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "<named>"

    Examples:
      | key                                     | value                  | named                             |
      | /residential/building_type              | bungalows              | /residential/building_type        |
      | /allowance/day_share                    | 1.5                    | /allowance/day_share              |
      | /desires/0/goods                        | {caviar: 0.5}          | /desires/0/goods/caviar           |
      | /desires/0/goods                        | {bread: 1.5}           | /desires/0/goods/bread            |
      | /desires/0/match                        | {window: hotel}        | /desires/0/match/window           |
      | /desires/0/match                        | {age: '12-17'}         | /desires/0/match/age              |
      | /movement/cells_per_tick                | 0                      | /movement/cells_per_tick          |
      | /class_affinity/upper/high              | -1                     | /class_affinity/upper/high        |
      | /reputation/sale_recovery               | 2                      | /reputation/sale_recovery         |

  # The residential market's numbers are in economy.yaml (R71).
  @R45 @R71
  Scenario Outline: A bad residential market setting fails and names the key
    Given the shipped config with "economy.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "economy.yaml" and "<key>"

    Examples:
      | key                                   | value |
      | /residential/stretch_share            | 0.1   |
      | /residential/comfortable_share        | 1.5   |
      | /residential/stretch_min_desirability | -0.5  |
      | /residential/rent_tax                 | 1.5   |
      | /residential/commute_preference       | 1.5   |
      | /residential/max_rent_per_flat        | -1.00 |

  @R45 @R56
  Scenario: The apartment type must not trade
    Given the shipped config with "customers.yaml" key "/residential/building_type" set to "bakery"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "hours: none"

  # R70: shopping windows (customers.yaml `shopping_windows`). Rule 0 of the shipped table is
  # day trade's catch-all; rule 1 is night trade's.
  @R70 @R17
  Scenario Outline: A bad shopping window fails and names the key
    Given the shipped config with "customers.yaml" key "<key>" set to "<value>"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "<named>"
    And an error names "customers.yaml" and "<says>"

    Examples:
      | key                                   | value                                  | named                                       | says                        |
      | /shopping_windows/rules/0/windows     | {caviar: {from: '09:00', to: '10:00'}} | /shopping_windows/rules/0/windows/caviar    | unknown good or group       |
      | /shopping_windows/rules/0/windows     | {bread: {from: '10:00', to: '09:00'}}  | /shopping_windows/rules/0/windows/bread     | must start before it ends   |
      | /shopping_windows/rules/0/windows     | {bread: {from: '06:00', to: '09:00'}}  | /shopping_windows/rules/0/windows/bread     | inside day trade            |
      | /shopping_windows/rules/0/windows     | {bread: {from: '16:00', to: '18:00'}}  | /shopping_windows/rules/0/windows/bread     | inside day trade            |
      | /shopping_windows/rules/1/windows     | {drinks: {from: '12:00', to: '13:00'}} | /shopping_windows/rules/1/windows/drinks    | inside night trade          |
      | /shopping_windows/rules/0/match       | {trade: hotel}                         | /shopping_windows/rules/0/match/trade       | day or night                |
      | /shopping_windows/rules/0/match       | {trade: day, age: '12-17'}             | /shopping_windows/rules/0/match/age         | unknown age band            |
      | /shopping_windows/groups/food         | [bread, caviar]                        | /shopping_windows/groups/food/1             | unknown good                |
      | /shopping_windows/groups/bread        | [groceries]                            | /shopping_windows/groups/bread              | is a good                   |
      | /shopping_windows/rules/0/windows/bread | {from: '25:00', to: '26:00'}         | /shopping_windows/rules/0/windows/bread/from | does not match             |

  # Coverage: every good a segment can want, in every era it can want it, must have a window,
  # from a rule naming the good, its group or "*".
  @R70 @R17
  Scenario: Every segment needs a window for every good it can want
    Given the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {bread: {from: '07:00', to: '17:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "/shopping_windows"
    And an error names "customers.yaml" and "no day window for groceries"

  @R70 @R17
  Scenario: A window can cover a good for one segment only, if the rest are covered too
    Given the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day, class: upper}, windows: {'*': {from: '10:00', to: '11:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "working"
