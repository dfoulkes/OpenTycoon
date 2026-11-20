# R68: a competently run shop must be profitable in every era; retail is fixed in the economy,
# not by loosening the balance rule (R65). docs/economy.md "Retail through the eras".
# §7.1: daily running cost is rent + utilities + stock; §2.2, R1, R2: stock arrives at 06:00 and
# what is unsold is binned at midnight.
Feature: A competently run shop pays its way in every era
  A shop of reference size on a Medium-tier block by the tube, priced at the reference price,
  that orders each evening what it was asked for that day plus a margin, makes a profit in
  every decade of the full game in the shipped city. Its takings cover the stock it bought
  (binned waste included), its land rent and its utility bills, decade after decade, from the
  1930s to the 2040s.

  Background:
    Given the shipped config

  # A competent shopkeeper orders from demand, not just sales: what it sold plus the shoppers it
  # turned away once it sold out, plus 10%, within what the shop holds (R2, R10). A day with no
  # delivery says nothing about demand, so the order stands. Rent is counted as a cost even
  # though the shopkeeper holds the lease and pays it to itself.
  @R68 @R1 @R2 @R10
  Scenario Outline: A lone shop of reference size is profitable in every decade, 1930–2040
    Given a city with one shopkeeper and no rivals
    And the shopkeeper holds the lease on "Soho" and runs a <type> of reference size there at reference prices
    When the full game is played with the shopkeeper ordering what it sold plus the shoppers it turned away, plus 10%
    Then the <type> made a net profit in every decade from 1930 to 2040
    And the <type> sold at least its reference footfall on most days it had stock

    Examples:
      | type          |
      | grocery       |
      | bakery        |
      | clothes_store |
