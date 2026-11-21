# Spec §8.1 "Births" and §3 (no inflation). Population is a cohort model: counts per age band,
# sex and class. Only adults walk the map; children are a number until they turn 18.
Feature: The population cohort model
  Every midnight is a five-year step. Women aged 18–49 have children at the era's historical
  births per woman, every band ages, some die, children who reach 18 come in on the next
  05:30 train, and migrants top up the arrivals.

  Background:
    Given the shipped config

  @R60
  Scenario Outline: Adults fall into four age bands; children are not shoppers
    Then the age bands are "18-29, 30-49, 50-64, 65+"
    And a person aged <age> is <band>

    Examples:
      | age | band              |
      | 0   | a child           |
      | 17  | a child           |
      | 18  | in band "18-29"   |
      | 29  | in band "18-29"   |
      | 30  | in band "30-49"   |
      | 49  | in band "30-49"   |
      | 50  | in band "50-64"   |
      | 64  | in band "50-64"   |
      | 65  | in band "65+"     |
      | 101 | in band "65+"     |

  @R61
  Scenario Outline: Births follow the era's births per woman over the childbearing span
    # births = women 18–49 × births per woman ÷ 32-year span × 5 years per step
    Given migration is off and nobody dies
    And a city of 3200 working women aged "<band>"
    When the year steps from <from> to <to>
    Then <born> children were born

    Examples:
      | band  | from | to   | born |
      | 18-29 | 1930 | 1935 | 950  |
      | 30-49 | 1960 | 1965 | 1350 |
      | 30-49 | 1975 | 1980 | 900  |
      | 18-29 | 2025 | 2030 | 725  |

  @R61
  Scenario: Only women aged 18–49 have children
    Given migration is off and nobody dies
    And a city of 3200 working men aged "18-29"
    And a city of 3200 working women aged "50-64"
    And a city of 3200 upper women aged "65+"
    When the year steps from 1930 to 1935
    Then 0 children were born

  @R61
  Scenario: Children are counted, not walking, and come in on the 05:30 train at 18
    Given migration is off and nobody dies
    And 100 children born by 1935
    When the year steps to 1950
    Then 100 children remain
    And 0 people are waiting for the 05:30 train
    And 0 adults live in the city
    When the year steps to 1955
    Then 0 children remain
    And 100 newcomers aged "18-29" are waiting for the 05:30 train

  @R61 @R60
  Scenario: Each band ages up by the share of its span that five years covers
    Given migration is off and nobody dies
    And a city of 1200 working men aged "18-29"
    And a city of 1500 middle women aged "50-64"
    And a city of 400 upper men aged "65+"
    When the year steps from 1930 to 1935
    Then there are 700 working men aged "18-29"
    And there are 500 working men aged "30-49"
    And there are 1000 middle women aged "50-64"
    And there are 500 middle women aged "65+"
    And there are 400 upper men aged "65+"

  @R61
  Scenario: Deaths follow each band's mortality per step
    Given migration is off
    And a city of 1000 upper women aged "65+"
    And a city of 1000 working men aged "50-64"
    When the year steps from 1930 to 1935
    Then 290 adults died
    And there are 750 upper women aged "65+"

  @R61 @R17
  Scenario: Migrants top up the morning's arrivals
    Given the shipped config with "economy.yaml" key "/population/migration_per_day" set to "40"
    And nobody dies
    When the year steps from 1930 to 1935
    Then 40 people are waiting for the 05:30 train

  @R6
  Scenario: No inflation: allowances are the same in every era
    Then every allowance is the same in every year from 1930 to 2050

  @R6 @R53
  Scenario: The population grows every morning and the growth arrives by train
    Given a fresh city
    When the fresh city runs for 4 days
    Then the population grew every day
    And people arrived on every morning's train
