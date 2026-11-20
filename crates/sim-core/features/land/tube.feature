# Spec §4.3, §8. Tags name the rulings each scenario proves.
Feature: Tube lines open over the years
  Tube stations sit on blocks as the points where visitors arrive and leave. New lines open
  over the years and each one raises footfall on the blocks it serves: a block's footfall
  weight is the sum of the weights of the lines open there.

  Background:
    Given the shipped config with "map.yaml" key "/map" set to "full"

  @R16 @R25
  Scenario Outline: A new line raises footfall weight on the blocks it serves
    Given the full board
    Then the footfall weight of "<district>" is <before> in <year before>
    And the footfall weight of "<district>" is <after> in <year after>

    Examples:
      | district    | year before | before | year after | after | line                      |
      | Brixton     | 1970        | 0      | 1975       | 1     | Victoria 1971             |
      | Mayfair     | 1965        | 2      | 1970       | 3     | Victoria 1968             |
      | Mayfair     | 1975        | 3      | 1980       | 4     | Jubilee 1979              |
      | Docklands   | 1985        | 0      | 1990       | 1     | DLR 1987                  |
      | Docklands   | 1995        | 1      | 2000       | 2     | Jubilee 1999              |
      | Southwark   | 1995        | 0      | 2000       | 1     | Jubilee 1999              |
      | Whitechapel | 2020        | 2      | 2025       | 4     | Elizabeth 2022, weight 2  |

  @R16 @R25
  Scenario: Only blocks with an open line are arrival points
    Given the full board
    Then in 1930 "Hackney" has no station
    And in 1930 "Docklands" has no station
    And in 1930 every station block has a weight of at least 1

  @R25
  Scenario: Visitors arrive at stations in proportion to footfall weight
    Given stations with footfall weights 1 and 3
    When 4000 visitors pick an arrival station
    Then about 1000 arrive at the first and 3000 at the second, within 5%
    And none arrive at a station of weight 0
