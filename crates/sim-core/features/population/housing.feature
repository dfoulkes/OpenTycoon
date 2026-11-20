# Spec §7.2 Residential. Apartment blocks are
# buildings of the type `residential.building_type`; their flats house the city's residents.
Feature: Apartments and who lives in them
  New arrivals look for a flat at 05:30, before the 06:00 settlement. Everyone wants to live
  in the city, but only if they can afford it: rent within 20% of their daily allowance, or up
  to 50% for somewhere really desirable.

  Background:
    Given the shipped config

  @R50
  Scenario Outline: Flats = building volume ÷ flat size
    Given the shipped config with "customers.yaml" key "/residential/flat_size" set to "<flat_size>"
    And an apartment block "Flats" on "old_kent_road" <w>×<d>×<h>
    Then "Flats" has <flats> flats

    Examples:
      | w | d | h  | flat_size | flats |
      | 3 | 3 | 12 | 4         | 27    |
      | 2 | 2 | 10 | 4         | 10    |
      | 3 | 3 | 1  | 4         | 2     |
      | 3 | 3 | 12 | 6         | 18    |
      | 9 | 9 | 100| 4         | 2025  |

  @R56
  Scenario: An apartment block pays land rent and utilities by volume, and holds no stock
    # 3×3×12 = 108 units. Land: 108 × 5p × 0.6 (Low) = £3.24. Utilities at the apartment
    # factors: electricity 216p × 0.8, gas 108p × 0.8, water 108p × 1.0, telecoms 108p × 0.5.
    Given an apartment block "Flats" on "old_kent_road" 3×3×12
    Then "Flats" pays £3.24 land rent a day
    And "Flats" pays £4.21 in utility bills a day
    And "Flats" can hold 0 stock

  @R53 @R51
  Scenario: Arrivals take a flat at 05:30 and owe rent from the 06:00 settlement
    Given an apartment block "Flats" on "old_kent_road" 3×3×12
    And the rent at "Flats" is £3.00
    And 1 working man aged "30-49" arrives on the morning train
    When the 05:30 arrivals house-hunt
    Then 1 person lives in "Flats"
    And 0 arrivals are commuting or visiting
    And 0 people are waiting for the 05:30 train
    And at the 06:00 settlement "Flats" is owed £3.00 by its residents

  @R63 @R51
  Scenario: A resident never pays more rent than their whole allowance
    # A working man aged 30–49 gets £22.00 a day. Residents have no savings (R63), so an owner
    # who puts the rent up past the allowance collects the allowance and no more.
    Given an apartment block "Flats" on "old_kent_road" 3×3×12 owned by player 0
    And a working man aged "30-49" has lived in "Flats" since 1930
    And the rent at "Flats" is £30.00
    Then at the 06:00 settlement "Flats" is owed £22.00 by its residents

  @R52 @R53
  Scenario Outline: Priced out when the cheapest flat is over 20% of the allowance
    # A working man aged 30–49 gets £20 × 1.1 = £22.00 a day; 20% is £4.40.
    Given an apartment block "Flats" on "old_kent_road" 3×3×12
    And the rent at "Flats" is £<rent>
    And 1 working man aged "30-49" arrives on the morning train
    When the 05:30 arrivals house-hunt
    Then <housed> person lives in "Flats"
    And <unhoused> arrivals are commuting or visiting

    Examples:
      | rent | housed | unhoused |
      | 4.40 | 1      | 0        |
      | 4.41 | 0      | 1        |

  @R52
  Scenario Outline: Stretching to 50% only for a desirable flat
    # An upper woman aged 30–49 gets £120 × 1.1 = £132.00: 20% is £26.40 and 50% is £66.00.
    # Mayfair (High) × medium fit-out is 1.2 desirable to her, Soho 0.8, Old Kent Road 0.3;
    # stretching needs 1.0.
    Given an apartment block "Cheap" on "old_kent_road" 3×3×12
    And an apartment block "Mid" on "soho" 3×3×12
    And an apartment block "Lux" on "mayfair" 3×3×12
    And the rent at "Cheap" is £20.00
    And the rent at "Mid" is £40.00
    And the rent at "Lux" is £<lux>
    Then an upper woman aged "30-49" would consider "<considered>"
    And a working man aged "30-49" would consider "nothing"

    Examples:
      | lux   | considered |
      | 26.40 | Cheap, Lux |
      | 60.00 | Cheap, Lux |
      | 66.00 | Cheap, Lux |
      | 66.01 | Cheap      |

  @R52
  Scenario: A desirable flat is no use if nothing comfortable is vacant
    Given an apartment block "Cheap" on "old_kent_road" 3×3×12
    And an apartment block "Lux" on "mayfair" 3×3×12
    And the rent at "Cheap" is £20.00
    And the rent at "Lux" is £60.00
    And "Cheap" is full
    Then an upper woman aged "30-49" would consider "nothing"

  @R52 @R17
  Scenario: The stretch threshold is configurable
    Given the shipped config with "customers.yaml" key "/residential/stretch_min_desirability" set to "2.0"
    And an apartment block "Cheap" on "old_kent_road" 3×3×12
    And an apartment block "Lux" on "mayfair" 3×3×12
    And the rent at "Cheap" is £20.00
    And the rent at "Lux" is £60.00
    Then an upper woman aged "30-49" would consider "Cheap"

  @R51
  Scenario: A nicer lot attracts more people at the same rent
    Given an apartment block "East" on "old_kent_road" 3×3×100
    And an apartment block "West" on "soho" 3×3×100
    And 200 middle women aged "30-49" arrive on the morning train
    When the 05:30 arrivals house-hunt
    Then 200 people live in "East" or "West"
    And more people live in "West" than in "East"

  @R51
  Scenario: A nicer building attracts more people at the same rent
    Given an apartment block "Plain" on "soho" 3×3×100 with low fit-out
    And an apartment block "Smart" on "soho" 3×3×100 with high fit-out
    And 200 middle women aged "30-49" arrive on the morning train
    When the 05:30 arrivals house-hunt
    Then more people live in "Smart" than in "Plain"

  @R51
  Scenario: The pull is balanced by price
    Given an apartment block "Dear" on "soho" 3×3×100
    And an apartment block "Cheap" on "soho" 3×3×100
    And the rent at "Dear" is £4.00
    And the rent at "Cheap" is £2.00
    And 200 middle women aged "30-49" arrive on the morning train
    When the 05:30 arrivals house-hunt
    Then more people live in "Cheap" than in "Dear"

  @R51
  Scenario: Only the owner sets the rent
    Given an apartment block "Flats" on "old_kent_road" 3×3×12 owned by player 0
    When player 1 sets the rent at "Flats" to £5.00
    Then the rent change is refused because "not the owner"
    And the rent at "Flats" is £3.00
    When player 0 sets the rent at "Flats" to £5.00
    Then the rent at "Flats" is £5.00

  @R54
  Scenario: Residents move at most once every 10 years, and only for something better
    # A working woman aged 30–49 gets £22.00; she can afford £4.40.
    Given an apartment block "Old" on "old_kent_road" 3×3×12
    And the rent at "Old" is £4.40
    And a working woman aged "30-49" has lived in "Old" since 1930
    And an apartment block "New" on "old_kent_road" 3×3×12
    And the rent at "New" is £2.00
    When the 05:30 arrivals house-hunt in 1935
    Then the resident lives in "Old"
    When the 05:30 arrivals house-hunt in 1940
    Then the resident lives in "New"
    And the resident moved in 1940
    Given an apartment block "Newer" on "old_kent_road" 3×3×12
    And the rent at "Newer" is £1.00
    When the 05:30 arrivals house-hunt in 1945
    Then the resident lives in "New"
    When the 05:30 arrivals house-hunt in 1950
    Then the resident lives in "Newer"

  @R54
  Scenario: A resident never moves somewhere worse
    Given an apartment block "Home" on "old_kent_road" 3×3×12 with high fit-out
    And the rent at "Home" is £3.00
    And a working woman aged "30-49" has lived in "Home" since 1930
    And an apartment block "Worse" on "whitechapel" 3×3×12 with low fit-out
    And the rent at "Worse" is £3.00
    When the 05:30 arrivals house-hunt in 1950
    Then the resident lives in "Home"
    And the resident moved in 1930

  @R54 @R52
  Scenario Outline: A move must pass the same affordability rule
    # An upper woman aged 30–49 gets £132.00: she may stretch to £66.00 for Mayfair.
    Given the shipped config with "customers.yaml" key "/residential/reference_rent" set to "50.00"
    And an apartment block "Home" on "old_kent_road" 3×3×12
    And the rent at "Home" is £3.00
    And an upper woman aged "30-49" has lived in "Home" since 1930
    And an apartment block "Spare" on "old_kent_road" 3×3×12
    And the rent at "Spare" is £20.00
    And an apartment block "Grand" on "mayfair" 3×3×12 with high fit-out
    And the rent at "Grand" is £<grand>
    When the 05:30 arrivals house-hunt in 1950
    Then the resident lives in "<lives>"

    Examples:
      | grand | lives |
      | 66.00 | Grand |
      | 66.01 | Home  |

  # Study levers (docs/economy.md "Residential levers"): configurable rules that are off by
  # default, for the balance study to search. Not rulings; tagged with the ruling they adjust.

  @R51 @R17
  Scenario: A rent cap limits what residents see and pay, whatever the owner sets
    # A working man aged 30–49 gets £22.00: £6.00 is over his 20% (£4.40), the £4.00 cap is not.
    Given the shipped config with "customers.yaml" key "/residential/max_rent_per_flat" set to "4.00"
    And an apartment block "Flats" on "old_kent_road" 3×3×12 owned by player 0
    And the rent at "Flats" is £6.00
    And 1 working man aged "30-49" arrives on the morning train
    When the 05:30 arrivals house-hunt
    Then 1 person lives in "Flats"
    And at the 06:00 settlement "Flats" is owed £4.00 by its residents
    And the rent at "Flats" is £6.00

  @R51
  Scenario: With no rent cap (the default) the owner's rent stands
    Given an apartment block "Flats" on "old_kent_road" 3×3×12 owned by player 0
    And a working man aged "30-49" has lived in "Flats" since 1930
    And the rent at "Flats" is £6.00
    Then at the 06:00 settlement "Flats" is owed £6.00 by its residents

  @R51 @R63 @R17
  Scenario Outline: A rent tax sends a share of residents' rent to the city
    Given the shipped config with "customers.yaml" key "/residential/rent_tax" set to "<tax>"
    And an apartment block "Flats" on "old_kent_road" 3×3×12 owned by player 0
    And a working man aged "30-49" has lived in "Flats" since 1930
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And the rent at "Flats" is £3.00
    Then at the 06:00 settlement "Flats" is owed £6.00 by its residents
    And £<city> of the rent at "Flats" is paid to the city as rent tax

    Examples:
      | tax   | city |
      | 0.0   | 0.00 |
      | 0.2   | 1.20 |
      | 0.125 | 0.75 |
      | 1.0   | 6.00 |

  @R52 @R53 @R17
  Scenario: Some adults would rather commute, even when they can afford a flat
    # 10 adults, 30% prefer to commute: at most 7 are housed, and the re-hunt the next
    # morning does not fill the empty flats with the other 3.
    Given the shipped config with "customers.yaml" key "/residential/commute_preference" set to "0.3"
    And an apartment block "Flats" on "old_kent_road" 3×3×12
    And 10 working men aged "30-49" arrive on the morning train
    When the 05:30 arrivals house-hunt
    Then 7 people live in "Flats"
    And 3 arrivals are commuting or visiting
    When the 05:30 arrivals house-hunt
    Then 7 people live in "Flats"
    And 3 people in the city are commuting or visiting

  @R52 @R53
  Scenario: By default everyone who can afford a flat takes one
    Given an apartment block "Flats" on "old_kent_road" 3×3×12
    And 10 working men aged "30-49" arrive on the morning train
    When the 05:30 arrivals house-hunt
    Then 10 people live in "Flats"
    And 0 arrivals are commuting or visiting
