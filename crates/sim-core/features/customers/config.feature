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
      | /residential/stretch_share              | 0.1                    | /residential/stretch_share        |
      | /residential/comfortable_share          | 1.5                    | /residential/comfortable_share    |
      | /allowance/day_share                    | 1.5                    | /allowance/day_share              |
      | /desires/0/goods                        | {caviar: 0.5}          | /desires/0/goods/caviar           |
      | /desires/0/goods                        | {bread: 1.5}           | /desires/0/goods/bread            |
      | /desires/0/match                        | {window: hotel}        | /desires/0/match/window           |
      | /desires/0/match                        | {age: '12-17'}         | /desires/0/match/age              |
      | /movement/cells_per_tick                | 0                      | /movement/cells_per_tick          |
      | /class_affinity/upper/high              | -1                     | /class_affinity/upper/high        |
      | /reputation/sale_recovery               | 2                      | /reputation/sale_recovery         |
      | /residential/stretch_min_desirability   | -0.5                   | /residential/stretch_min_desirability |
      | /residential/rent_tax                   | 1.5                    | /residential/rent_tax             |
      | /residential/commute_preference         | 1.5                    | /residential/commute_preference   |
      | /residential/max_rent_per_flat          | -1.00                  | /residential/max_rent_per_flat    |

  @R45 @R56
  Scenario: The apartment type must not trade
    Given the shipped config with "customers.yaml" key "/residential/building_type" set to "bakery"
    When the config is loaded
    Then loading fails
    And an error names "customers.yaml" and "hours: none"
