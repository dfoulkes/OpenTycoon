# Spec §2.2 ("restock can be adjusted during the day" means tomorrow's order), §7.1.
Feature: Tomorrow's stock order
  During the day a player sets tomorrow's order. It locks at midnight, arrives at the 06:00
  settlement and is paid for then. Nothing arrives mid-day: there is no emergency restock.

  Background:
    Given the shipped config
    And player 1 owns a bakery 3×3 and 10 high on Old Kent Road

  @R2
  Scenario: The order arrives at 06:00 and is paid for then
    When the day runs to "10:00"
    And player 1 orders 40 bread for tomorrow at the bakery
    And the day runs to "23:59:50"
    Then the bakery has 0 bread in stock
    When the day runs to "05:59:50"
    Then the bakery has 0 bread in stock
    When the day runs to "06:00"
    Then the bakery has 40 bread in stock
    And the 06:00 bills charge player 1 "£1.60" for stock

  @R2
  Scenario: A shop that sells out mid-day stays sold out until the next morning
    When the day runs to "10:00"
    And player 1 orders 10 bread for tomorrow at the bakery
    And the day runs to "06:00"
    And the day runs to "12:00"
    And 10 bread are sold at the bakery
    And player 1 orders 45 bread for tomorrow at the bakery
    And the day runs to "16:00"
    Then the bakery has 0 bread in stock
    And selling 1 bread at the bakery is lost because it is out of stock
    When the day runs to "06:00"
    Then the bakery has 45 bread in stock

  @R2
  Scenario: The order locks at midnight; a change after midnight is for the day after
    When the day runs to "10:00"
    And player 1 orders 40 bread for tomorrow at the bakery
    And the day runs to "02:00"
    And player 1 orders 10 bread for tomorrow at the bakery
    And the day runs to "06:00"
    Then the bakery has 40 bread in stock
    When the day runs to "06:00"
    Then the bakery has 10 bread in stock

  @R2
  Scenario: An order stands until it is changed
    When the day runs to "10:00"
    And player 1 orders 20 bread for tomorrow at the bakery
    And the day runs to "06:00"
    Then the bakery has 20 bread in stock
    When the day runs to "06:00"
    Then the bakery has 20 bread in stock

  @R2 @R10
  Scenario: An order is capped at what the building can hold
    # 3×3×10 = 90 units of volume × 0.5 stock density = 45 units of bread.
    When the day runs to "10:00"
    And player 1 orders 100 bread for tomorrow at the bakery
    Then tomorrow's order at the bakery is 45 bread
    When the day runs to "06:00"
    Then the bakery has 45 bread in stock

  @R2 @R10
  Scenario: What is billed at 06:00 is exactly what is delivered
    # A 100-unit order is capped at the bakery's 45; the bill is 45 × 4p, not 100 × 4p.
    When the day runs to "10:00"
    And player 1 orders 100 bread for tomorrow at the bakery
    And the day runs to "06:00"
    Then 45 bread were delivered and player 1 was billed "£1.80" for them

  @R2
  Scenario: A change after midnight is neither billed nor delivered until the day after
    When the day runs to "10:00"
    And player 1 orders 40 bread for tomorrow at the bakery
    And the day runs to "02:00"
    And player 1 orders 10 bread for tomorrow at the bakery
    And the day runs to "06:00"
    Then 40 bread were delivered and player 1 was billed "£1.60" for them
    When the day runs to "06:00"
    Then 10 bread were delivered and player 1 was billed "£0.40" for them
