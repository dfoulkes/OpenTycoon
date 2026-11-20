# Spec §7.1 (unlock_year, obsolete_year), §7.4 (unlock tables 1930–2030+). The stats of every
# type come from one formula (docs/economy.md "Business stats"), so the full 1930–2050 game is
# playable headless (R64).
Feature: Every era brings its own businesses
  A business type can be built from its unlock year and never before. Its footprint, height,
  stock density and utility factors come from one formula, so a competently run shop of
  reference size covers its rent, utilities and stock at reference footfall, on every tier and
  in every era it trades.

  Background:
    Given the shipped config

  @R64
  Scenario Outline: A business can't be built before its unlock year
    Given it is <before>
    When player 1 builds a <type> <w>×<d> and 4 high on Soho
    Then the building is refused as not yet unlocked
    Given it is <unlock>
    When player 1 builds a <type> <w>×<d> and 4 high on Holborn
    Then the building goes up

    Examples:
      | type              | w | d | before | unlock |
      | ice_cream_parlour | 2 | 2 | 1935   | 1940   |
      | delicatessen      | 2 | 3 | 1945   | 1950   |
      | travel_agency     | 2 | 3 | 1955   | 1960   |
      | department_store  | 4 | 6 | 1965   | 1970   |
      | computer_shop     | 2 | 3 | 1975   | 1980   |
      | coffee_chain      | 2 | 2 | 1985   | 1990   |
      | internet_cafe     | 2 | 3 | 1990   | 1995   |
      | gym               | 3 | 3 | 1995   | 2000   |
      | craft_taproom     | 3 | 3 | 2000   | 2005   |
      | vape_shop         | 2 | 2 | 2005   | 2010   |
      | escape_room       | 2 | 3 | 2010   | 2015   |
      | data_centre       | 4 | 4 | 2015   | 2020   |
      | ev_charging_hub   | 3 | 4 | 2020   | 2025   |
      | vr_arcade         | 3 | 3 | 2025   | 2030   |

  # §7.4: the 1930 set, then each era's additions (apartments and the Hotel count from 1930).
  @R64
  Scenario Outline: Each era adds to what can be built
    Then <count> business types can be built in <year>

    Examples:
      | year | count |
      | 1930 | 24    |
      | 1945 | 28    |
      | 1950 | 32    |
      | 1960 | 36    |
      | 1970 | 40    |
      | 1980 | 44    |
      | 1990 | 50    |
      | 1995 | 52    |
      | 2000 | 55    |
      | 2005 | 57    |
      | 2010 | 60    |
      | 2015 | 63    |
      | 2020 | 66    |
      | 2025 | 68    |
      | 2030 | 72    |
      | 2050 | 72    |

  # §7.5: online sellers are declared here; their demand model is the e-commerce module's.
  @R64
  Scenario: Online businesses arrive with the internet
    Then every online business type unlocks in or after the internet upgrade year

  # docs/economy.md: reference size is min_footprint × reference.height; reference footfall is
  # reference.sell_through of its stock, sold at reference prices.
  @R10 @R35 @R64
  Scenario Outline: A shop of reference size pays its way at reference footfall
    Given it is <year>
    And player 1 owns a <type> of reference size on <block>
    And player 1 orders the <type>'s full capacity for tomorrow
    When the <type> sells its reference footfall at reference prices
    Then the <type>'s takings cover its rent, utilities and stock

    Examples:
      | type             | year | block         |
      | bakery           | 1930 | Old Kent Road |
      | bakery           | 1930 | Mayfair       |
      | grocery          | 1930 | Soho          |
      | restaurant       | 1930 | Mayfair       |
      | jeweller         | 1930 | Belgravia     |
      | department_store | 1970 | Holborn       |
      | supermarket      | 1980 | Whitechapel   |
      | coffee_chain     | 1990 | Soho          |
      | gym              | 2000 | Mayfair       |
      | vr_arcade        | 2030 | Bloomsbury    |

  @R10 @R35 @R64
  Scenario: Every shop type pays its way at reference footfall on every tier, in every era
    Then every shop type clears its rent, utilities and stock at reference footfall on every tier from its unlock year to 2050

  @R64
  Scenario: The starting city can support a reference shop of every type
    Then the starting city wants at least one reference shop's footfall of every good, in each type's unlock year
