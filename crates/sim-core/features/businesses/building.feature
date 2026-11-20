# Spec §4.1 (lots, frontage, height), §7.1 (business types), §3 (property value).
Feature: Putting up a building
  One building is one business of one type. It must fit its type's minimum footprint, touch
  the lot edge, not overlap another building, and be no taller than its type allows. The cap
  is per type, never per era. A building never wears: its value is what it cost to build.

  Background:
    Given the shipped config

  @R58
  Scenario Outline: Each business type has its own height cap
    When player 1 builds a <type> 3×3 and <height> high on Soho
    Then the building <outcome>

    Examples:
      | type          | height | outcome                     |
      | bakery        | 12     | goes up                     |
      | bakery        | 13     | is refused as too tall      |
      | grocery       | 16     | goes up                     |
      | grocery       | 17     | is refused as too tall      |
      | clothes_store | 20     | goes up                     |
      | clothes_store | 21     | is refused as too tall      |
      | bar           | 16     | goes up                     |
      | bar           | 17     | is refused as too tall      |
      | restaurant    | 20     | goes up                     |
      | restaurant    | 21     | is refused as too tall      |
      | bakery        | 0      | is refused as too short     |

  @R58
  Scenario: A type's cap comes from config, up to the world's 100
    Given the shipped config with "businesses.yaml" key "/types/bakery/max_height" set to "100"
    When player 1 builds a bakery 3×3 and 100 high on Soho
    Then the building goes up

  @R57
  Scenario Outline: The height cap is the same in every era
    Given the shipped config with "businesses.yaml" key "/types/bakery/max_height" set to "100"
    And it is <year>
    When player 1 builds a bakery 3×3 and 100 high on Soho
    Then the building goes up
    When player 1 builds a bakery 3×3 and 101 high on Holborn
    Then the building is refused as too tall

    Examples:
      | year |
      | 1930 |
      | 1950 |
      | 2045 |

  # §7.1 min_footprint, inside the 9×9 lot.
  @R22
  Scenario Outline: A building must meet its type's minimum footprint, either way round
    When player 1 builds a <type> <w>×<d> and 5 high on Soho
    Then the building <outcome>

    Examples:
      | type    | w | d | outcome                         |
      | bakery  | 2 | 2 | goes up                         |
      | bakery  | 1 | 2 | is refused as too small         |
      | grocery | 2 | 3 | goes up                         |
      | grocery | 3 | 2 | goes up                         |
      | grocery | 2 | 2 | is refused as too small         |

  # §4.1: every business must touch the lot edge (street frontage).
  @R22
  Scenario Outline: A building must have street frontage and stay inside the lot
    When player 1 builds a bakery <w>×<d> at <x>,<y> and 5 high on Soho
    Then the building <outcome>

    Examples:
      | x | y | w | d | outcome                                  |
      | 0 | 0 | 3 | 3 | goes up                                  |
      | 6 | 6 | 3 | 3 | goes up                                  |
      | 3 | 0 | 3 | 3 | goes up                                  |
      | 3 | 3 | 3 | 3 | is refused for having no street frontage |
      | 7 | 0 | 3 | 3 | is refused as outside the lot            |
      | 0 | 0 | 9 | 9 | goes up                                  |

  @R13
  Scenario: One building is one business; another type can't be stacked on it
    When player 1 builds a bakery 3×3 at 0,0 and 10 high on Soho
    Then the building goes up
    When player 1 builds a bar 3×3 at 0,0 and 10 high on Soho
    Then the building is refused for overlapping another building
    When player 1 builds a bar 3×3 at 3,0 and 10 high on Soho
    Then the building goes up
    And the bakery stocks only bread, up to 832 units

  @R41
  Scenario: Build cost scales with volume, tier and fit-out
    # 3×3×10 = 90 units × £10 = £900; Medium ×1.0, High ×1.8; fit-out High ×1.5.
    When player 1 builds a bakery 3×3 and 10 high on Soho
    Then the building cost "£900.00"
    When player 1 builds a bakery 3×3 and 10 high on Mayfair
    Then the building cost "£1,620.00"
    When player 1 builds a bakery 3×3 and 10 high on Holborn with a high fit-out
    Then the building cost "£1,350.00"

  @R41
  Scenario: A building never wears; its value stays its build cost
    Given player 1 owns a bakery 3×3 and 10 high on Soho
    And player 1 orders 40 bread for tomorrow at the bakery
    When the day runs to "12:00"
    And the clock runs on 3 days
    Then the bakery's property value is "£900.00"
