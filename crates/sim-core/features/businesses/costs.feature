# Spec §5.3 (volume formulas), §7.1 (daily running cost), §6 (utility bills at 06:00).
Feature: Running costs come from the building's volume
  Rent, utility bills and stock capacity are all set by the building's own volume (h × w × d),
  not the plot's. A shop's daily running cost is rent + utilities + stock: there are no staff.

  Background:
    Given the shipped config

  @R10
  Scenario Outline: Rent, utilities and capacity grow with the building's volume
    # Old Kent Road is Low tier (×0.6). Bakery utility factors: elec 1.0, gas 1.5, water 1.0,
    # telecoms 0.5 on base rates of 2p, 1p, 1p, 1p per unit of volume.
    Given player 1 owns a bakery <w>×<d> and <h> high on Old Kent Road
    Then the bakery's volume is <volume>
    And the bakery can hold <capacity> bread
    And the bakery's daily rent is "<rent>"
    And the bakery's daily utility bill is "<utilities>"

    Examples:
      | w | d | h  | volume | capacity | rent   | utilities |
      | 3 | 3 | 10 | 90     | 45       | £2.70  | £4.50     |
      | 3 | 3 | 4  | 36     | 18       | £1.08  | £1.80     |
      | 6 | 3 | 10 | 180    | 90       | £5.40  | £9.00     |
      | 2 | 2 | 10 | 40     | 20       | £1.20  | £2.00     |

  @R10
  Scenario: A small building on a big lot pays for its own volume, not the lot's
    Given player 1 owns a bakery 3×3 and 10 high on Old Kent Road
    Then the lot is 9×9
    And the bakery's daily utility bill is "£4.50"

  @R10
  Scenario: Utility bills are paid at the 06:00 settlement, to the city while it holds them
    Given player 1 owns a bakery 3×3 and 10 high on Old Kent Road
    When the day runs to "05:59:50"
    Then no bills have been raised
    When the day runs to "06:00"
    Then the 06:00 bills charge player 1 "£4.50" for utilities, paid to the city

  @R35
  Scenario: The daily running cost is rent, utilities and stock, with nothing for staff
    Given player 1 owns a bakery 3×3 and 10 high on Old Kent Road
    And player 1 orders 40 bread for tomorrow at the bakery
    Then the bakery's daily running cost is "£8.80"
    And it is made of rent "£2.70", utilities "£4.50" and stock "£1.60" and nothing else
