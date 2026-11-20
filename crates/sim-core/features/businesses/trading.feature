# Spec §2.2, §7.1, §8.3. Tags name the rulings each scenario proves.
Feature: Shops trade inside their hours and sell from stock
  A shop sells only while its trading window is open and its owner has it open. Every sale
  comes out of stock at the shop's price; a shopper who finds the shelf empty is a lost sale.
  Whatever is left on the shelves at midnight is written off as waste.

  Background:
    Given the shipped config
    And player 1 owns a bakery 3×3 and 10 high on Old Kent Road
    And player 1 owns a bar 3×3 and 10 high on Soho

  @R1
  Scenario Outline: A day shop and a night shop sell only inside their trading windows
    Given the bakery has 50 bread in stock
    And the bar has 50 drinks in stock
    When the day runs to "<time>"
    Then selling 1 bread at the bakery <bakery>
    And selling 1 drinks at the bar <bar>

    Examples:
      | time     | bakery                     | bar                        |
      | 06:59:50 | is lost because it is closed | is lost because it is closed |
      | 07:00:00 | earns "£0.60"              | is lost because it is closed |
      | 16:59:50 | earns "£0.60"              | is lost because it is closed |
      | 17:00:00 | is lost because it is closed | is lost because it is closed |
      | 18:59:50 | is lost because it is closed | is lost because it is closed |
      | 19:00:00 | is lost because it is closed | earns "£1.50"              |
      | 23:49:50 | is lost because it is closed | earns "£1.50"              |
      | 23:50:00 | is lost because it is closed | is lost because it is closed |

  @R1
  Scenario: An owner can shut the shop during trading hours
    Given the bakery has 50 bread in stock
    When the day runs to "10:00"
    And player 1 closes the bakery
    Then selling 1 bread at the bakery is lost because it is closed
    When player 1 opens the bakery
    Then selling 1 bread at the bakery earns "£0.60"

  @R1
  Scenario: A sale comes out of stock at the shop's price
    Given the bakery has 50 bread in stock
    And player 1 prices bread at the bakery at 120% of the reference price
    When the day runs to "10:00"
    Then selling 5 bread at the bakery earns "£3.60"
    And the bakery has 45 bread in stock
    And the bakery has taken "£3.60" today

  @R1
  Scenario: A shopper who wants more than is on the shelf is a lost sale
    Given the bakery has 3 bread in stock
    When the day runs to "10:00"
    Then selling 5 bread at the bakery is lost because it is out of stock
    And the bakery has 3 bread in stock
    And the bakery has lost 1 sale today
    And selling 3 bread at the bakery earns "£1.80"
    And selling 1 bread at the bakery is lost because it is out of stock

  @R13
  Scenario: A shop sells only the goods of its one business type
    Given the bakery has 50 bread in stock
    When the day runs to "10:00"
    Then selling 1 drinks at the bakery is lost because it does not sell it

  @R1 @R2
  Scenario: Unsold stock is written off at midnight
    When the day runs to "10:00"
    And player 1 orders 40 bread for tomorrow at the bakery
    And the day runs to "06:00"
    Then the bakery has 40 bread in stock
    When the day runs to "12:00"
    And 15 bread are sold at the bakery
    And the day runs to "23:59:50"
    Then the bakery has 25 bread in stock
    When the day runs to "00:00"
    Then 25 bread worth "£9.00" were written off at the bakery
    And the bakery has 0 bread in stock

  @R1 @R2
  Scenario: A night shop gets its stock at 06:00, sits on it until 19:00, and bins the rest at midnight
    When the day runs to "10:00"
    And player 1 orders 30 drinks for tomorrow at the bar
    And the day runs to "06:00"
    Then the bar has 30 drinks in stock
    When the day runs to "12:00"
    Then selling 1 drinks at the bar is lost because it is closed
    When the day runs to "20:00"
    And 10 drinks are sold at the bar
    And the day runs to "00:00"
    Then 20 drinks worth "£9.00" were written off at the bar
    And the bar has 0 drinks in stock
