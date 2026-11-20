# Spec §7.1 (obsolete_year: demand fades), §8.2 (P(good | age, class, sex, hour, year)), R55,
# R64 (demand changes by decade).
Feature: What shoppers want changes by decade
  The shopping-list tables run from 1930 to 2050. Staples dominate early; leisure, electronics
  and services rise later. A good is wanted only from its unlock year, and demand for it fades
  after its obsolete year: newspapers, video rental and recorded music go the way of history.

  Background:
    Given the shipped config

  @R55 @R64
  Scenario Outline: Demand shifts by decade
    Then by <window>, a middle woman aged "30-49" wants <good> <direction> in <later> than in <earlier>

    Examples:
      | good       | window | earlier | later | direction |
      | bread      | day    | 1930    | 2030  | less      |
      | newspapers | day    | 1960    | 2025  | less      |
      | coffee     | day    | 1960    | 2000  | more      |
      | fitness    | day    | 1960    | 2020  | more      |
      | computers  | day    | 1975    | 1985  | more      |
      | movies     | night  | 1930    | 1990  | less      |
      | meals      | night  | 1930    | 2010  | more      |
      | takeaway   | night  | 2005    | 2015  | more      |

  @R55 @R64
  Scenario Outline: Staples dominate early; streaming and coffee arrive late
    Then in <year> by <window> the most wanted good for a <class> <sex> aged "<band>" is <good>

    Examples:
      | year | window | class   | sex   | band  | good          |
      | 1930 | day    | working | man   | 30-49 | bread         |
      | 1960 | day    | middle  | woman | 50-64 | newspapers    |
      | 2010 | day    | middle  | woman | 30-49 | groceries     |
      | 1990 | night  | middle  | man   | 18-29 | drinks        |
      | 2030 | night  | middle  | woman | 30-49 | video_streams |

  @R55 @R64
  Scenario Outline: A good isn't wanted before its unlock year
    Then a middle woman aged "30-49" wants <good> by <window> with P 0 in <year>

    Examples:
      | good            | window | year |
      | computers       | day    | 1975 |
      | phones          | day    | 1975 |
      | internet_access | day    | 1990 |
      | craft_beer      | night  | 2000 |
      | ev_charging     | day    | 2020 |

  # customers.yaml obsolescence: linear fade over fade_years (15) down to floor (10%).
  @R55 @R64
  Scenario Outline: Demand for an obsolete good fades after its obsolete year
    Then the fade leaves <pct>% of the demand for <good> by <window> in <year>

    Examples:
      | good       | window | year | pct |
      | newspapers | day    | 2005 | 100 |
      | newspapers | day    | 2010 | 100 |
      | newspapers | day    | 2015 | 67  |
      | newspapers | day    | 2020 | 33  |
      | newspapers | day    | 2050 | 10  |
      | videos     | night  | 2005 | 100 |
      | videos     | night  | 2010 | 87  |
      | videos     | night  | 2025 | 10  |
      | music      | day    | 2010 | 100 |
      | music      | day    | 2025 | 10  |

  @R55 @R64
  Scenario: Every good on sale is wanted by somebody while it is on sale
    Then every good a walk-in shop type sells is wanted by some shopper in every step it trades

  # §7.1: a type past its obsolete year draws shoppers with the same fade as an obsolete good.
  @R55 @R64
  Scenario Outline: A shop of an obsolete type draws fewer shoppers than one still in fashion
    Given the city's year is <year>
    And a news_stand "Kiosk" on "holborn" with 1000 newspapers
    And a general_store "Corner" on "holborn" with 1000 newspapers
    When 1000 middle shoppers at "holborn" choose where to buy newspapers
    Then "Kiosk" was chosen by at <bound> <pct>% of them

    Examples:
      | year | bound | pct |
      | 2005 | least | 40  |
      | 2025 | most  | 15  |

  # §7.5: online sellers have no door; their demand model is e-commerce's.
  @R64
  Scenario: Nobody walks to an online seller
    Given the city's year is 2000
    And an ecommerce "Warehouse" on "holborn" with 1000 books
    And a bookshop "Books" on "holborn" with 1000 books
    When 1000 middle shoppers at "holborn" choose where to buy books
    Then "Warehouse" was never chosen
    And "Books" was chosen by at least 95% of them
