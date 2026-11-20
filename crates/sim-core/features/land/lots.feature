# Spec §4.1. Tags name the rulings each scenario proves.
Feature: Lots and street frontage
  Every block is a 9×9 lot that holds several businesses. Each business takes a
  w×d footprint inside the lot and must touch the lot edge, so it has street frontage.

  Background:
    Given the shipped config
    And an empty lot

  @R22
  Scenario: Every block is a 9×9 lot
    Then the lot is 9 units wide and 9 units deep

  @R22
  Scenario: Old Kent Road as eight 3×3 businesses round the rim, with the middle left empty
    When 3x3 businesses are built at:
      | x | y |
      | 0 | 0 |
      | 3 | 0 |
      | 6 | 0 |
      | 0 | 3 |
      | 6 | 3 |
      | 0 | 6 |
      | 3 | 6 |
      | 6 | 6 |
    Then the lot holds 8 businesses
    And a 3x3 business at 3,3 is refused: no street frontage

  @R22
  Scenario: One business can take the whole lot
    Then a 9x9 business at 0,0 can be built

  @R22
  Scenario Outline: A business must touch the lot edge
    Then a <size> business at <x>,<y> <result>

    Examples:
      | size | x | y | result                          |
      | 2x2  | 0 | 4 | can be built                    |
      | 2x2  | 7 | 4 | can be built                    |
      | 2x2  | 4 | 0 | can be built                    |
      | 2x2  | 4 | 7 | can be built                    |
      | 1x9  | 4 | 0 | can be built                    |
      | 2x2  | 1 | 1 | is refused: no street frontage  |
      | 7x7  | 1 | 1 | is refused: no street frontage  |
      | 3x3  | 7 | 0 | is refused: outside the lot     |

  @R22
  Scenario: Businesses on one lot cannot overlap
    Given a 3x3 business already stands at 0,0
    Then a 2x2 business at 2,0 is refused: overlaps another business
    And a 2x2 business at 3,0 can be built

  @R22 @R17
  Scenario: The lot size is configurable
    Given the shipped config with "game.yaml" key "/world/lot_size" set to "[12, 12]"
    And an empty lot
    Then the lot is 12 units wide and 12 units deep
    And a 3x3 business at 9,4 can be built
    And a 3x3 business at 6,4 is refused: no street frontage
