# R69: events act within a game day, never across whole days (a game day is a 5-year era). The
# 1970s three-day week is power cuts: on the 1970 step every walk-in shop is dark for a few hours
# of day trade, and the front page says when. The locked stock order still arrives at 06:00 and
# is billed (§2.2, R1). Shoppers who find the lights out wait for them to come back on, then
# choose again, as long as the window for what they are out for (R70) is still open; if it
# closes in the dark they move on to their next item (features/customers/windows.feature).
# Night trade is not cut.
#
# Shipped windows (events.yaml): dark 07:00–10:00 and 13:00–15:30, 5½ of the 10 hours of day
# trade (about 4/7, the three-day week's share of a week lost).
Feature: Power cuts darken the shops for a few hours of the day

  Background:
    Given the shipped config with "customers.yaml" key "/initial_population" set to "200"
    And the shipped config with "game.yaml" key "/clock/start_year" set to "1965"

  @R69 @R16
  Scenario: The front page names the hours the lights go out
    Given a new game with 2 players
    When the game runs to midnight
    Then the front page for 1970 reads "POWER CUTS: shops dark from 07:00 to 10:00 and 13:00 to 15:30 as the miners' strike bites"

  # The text is rendered from the windows, so the paper and the rule can't drift apart.
  @R69 @R16
  Scenario: The front page follows the configured windows
    Given the shipped config with "events.yaml" key "/historical/5/effects/0/windows" set to "[{from: '08:00', to: '09:30'}]"
    And a new game with 2 players
    When the game runs to midnight
    Then the front page for 1970 reads "POWER CUTS: shops dark from 08:00 to 09:30 as the miners' strike bites"

  @R69 @R16
  Scenario: Shops are dark during the cuts and trade outside them
    Given a new game with 2 players
    And the game has run to "07:00"
    And player 1 has a bakery selling bread on each of "Old Kent Road, Whitechapel, Bethnal Green, Soho, Holborn, Bloomsbury, Knightsbridge"
    When the game runs to midnight
    And the game runs to "17:00" keeping the bakeries' books
    Then no bakery sold bread or turned a shopper away between "07:00" and "10:00"
    And no bakery sold bread or turned a shopper away between "13:00" and "15:30"
    And the bakeries sold bread between "10:00" and "13:00"
    And every bakery sold bread today

  # A cut that starts while the morning's shoppers are still stepping out: nobody buys in the
  # dark, nobody counts as turned away (the shop isn't out of stock), and those who would have
  # set out during the cut wait for the lights. A shopper who reaches a shop's door as the lights
  # go out is customers/choice.feature.
  # With a window open all day (R70), the lights come back in time for everyone out for bread.
  @R69 @R70 @R16
  Scenario: Shoppers don't buy in a dark shop, and buy once the lights are back
    Given the shipped config with "events.yaml" key "/historical/5/effects/0/windows" set to "[{from: '07:20', to: '09:00'}]"
    And shoppers shop at any hour of trade
    And a new game with 2 players
    And the game has run to "07:00"
    And player 1 has a bakery selling bread on each of "Old Kent Road, Whitechapel, Bethnal Green, Soho, Holborn, Bloomsbury, Knightsbridge"
    When the game runs to midnight
    And the game runs to "17:00" keeping the bakeries' books
    Then the bakeries sold bread between "07:00" and "07:20"
    And no bakery sold bread or turned a shopper away between "07:20" and "09:00"
    And the bakeries sold bread between "09:00" and "09:30"

  @R69 @R16 @R1
  Scenario: The locked order arrives at 06:00 and is billed on a power-cut day
    Given a new game with 2 players
    And the game has run to "07:00"
    And player 1 has a bakery selling bread on each of "Old Kent Road, Whitechapel, Bethnal Green, Soho, Holborn, Bloomsbury, Knightsbridge"
    When the game runs to midnight
    And the game runs to "07:00"
    Then the power cuts are in force
    And every bakery had 300 bread delivered and billed at 06:00

  @R69 @R16
  Scenario: The year after the cuts, the shops trade all day
    Given a new game with 2 players
    And the game has run to "07:00"
    And player 1 has a bakery selling bread on each of "Old Kent Road, Whitechapel, Bethnal Green, Soho, Holborn, Bloomsbury, Knightsbridge"
    When the game runs to midnight
    And the game runs to midnight
    And the game runs to "17:00" keeping the bakeries' books
    Then no power cut is in force
    And the bakeries sold bread between "07:00" and "10:00"
