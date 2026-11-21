# R70: each good is shopped for in a time window that depends on the shopper's age band, class
# and sex (customers.yaml `shopping_windows`; docs/economy.md "Shopping windows"). A shopper works
# through their list in window order and sets out to fit their earliest window. If a window
# passes without a purchase (every shop dark in a power cut, sold out or shut, or they got there
# too late) the good is dropped for the day and they go on to their next item. A missed window
# is not a lost sale against any shop. R69: during a cut shoppers wait on the street, but only
# while their window is still open.
Feature: Shoppers buy each good in its own time window
  Young middle-class shoppers buy clothes late in the afternoon, upper-class pensioners shop at
  ten, working people buy their food at lunchtime. A power cut that covers someone's whole
  window costs the shops that sale; one that ends in time does not.

  Background:
    Given the shipped config
    And the shipped config with "customers.yaml" key "/movement/spawn_spread_minutes" set to "0"

  # The shipped windows (customers.yaml): young middle-class shoppers buy clothes 15:00–17:00.
  @R70 @R55
  Scenario: A young middle-class shopper buys clothes only between 15:00 and 17:00
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {clothes: 1.0}}]"
    And a city of 40 middle women aged "18-29"
    And a clothes_store "Togs" on "holborn" with 1000 clothes
    When the city runs to "17:00:00"
    Then "Togs" sold 40 clothes
    And "Togs" sold clothes only between "15:00" and "17:00"
    And nobody came out before "15:00"

  # The shipped windows: upper-class pensioners do their day's shopping at ten.
  @R70 @R55
  Scenario: An upper-class senior buys at 10:00
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0}}]"
    And a city of 40 upper men aged "65+"
    And a bakery "Bun" on "holborn" with 1000 bread
    When the city runs to "17:00:00"
    Then "Bun" sold 40 bread
    And "Bun" sold bread only between "10:00" and "11:00"
    And nobody came out before "10:00"

  # Window order, not list order: the paper at 07:00, then bread at noon.
  @R70
  Scenario: A shopper works through their list in window order and sets out for the first
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, newspapers: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '12:00', to: '13:00'}, newspapers: {from: '07:00', to: '08:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    And a news_stand "Paper" on "holborn" with 1000 newspapers
    When the city runs to "17:00:00"
    Then "Paper" sold 30 newspapers
    And "Paper" sold newspapers only between "07:00" and "08:00"
    And "Bun" sold 30 bread
    And "Bun" sold bread only between "12:00" and "13:00"

  @R70 @R69
  Scenario: A power cut over someone's whole window means they skip that good and buy the next
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '08:00', to: '09:00'}, clothes: {from: '10:00', to: '11:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    And a clothes_store "Togs" on "holborn" with 1000 clothes
    And the lights are out today from "07:30" to "09:30"
    When the city runs to "17:00:00"
    Then "Bun" sold 0 bread
    And "Bun" lost 0 sales
    And "Togs" sold 30 clothes
    And "Togs" sold clothes only between "10:00" and "11:00"

  @R70 @R69
  Scenario: A cut that ends before the window closes: they wait for the lights and still buy
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '08:00', to: '09:00'}, clothes: {from: '10:00', to: '11:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    And a clothes_store "Togs" on "holborn" with 1000 clothes
    And the lights are out today from "07:30" to "08:20"
    When the city runs to "17:00:00"
    Then "Bun" sold 30 bread
    And "Bun" sold bread only between "08:20" and "09:00"
    And "Togs" sold 30 clothes

  # A missed window is nobody's lost sale: the shop wasn't out of stock, the shopper just never
  # got to buy. Turning a shopper away at the door is still a lost sale (§8.3).
  @R70 @R16
  Scenario: A missed window doesn't dent the shop's reputation
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '08:00', to: '09:00'}, clothes: {from: '10:00', to: '11:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    And a clothes_store "Togs" on "holborn" with 1000 clothes
    And the lights are out today from "07:30" to "09:30"
    When the city runs to "17:00:00"
    Then "Bun" lost 0 sales
    And "Bun" has a reputation of 0.5

  @R70 @R16
  Scenario: A stock-out at the door is still a lost sale
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '08:00', to: '09:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And a city of 5 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1 bread
    When the city runs to "17:00:00"
    Then "Bun" sold 1 bread
    And "Bun" lost 4 sales
    # 0.5, up to 0.5025 for the sale, then down 2% for each of the four turned away.
    And "Bun" has a reputation of 0.4634

  # Too late is missed, too: a shopper who reaches the door after their window has closed
  # doesn't buy, and nobody is turned away.
  @R70
  Scenario: A shopper who gets there after the window closes doesn't buy
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '08:00', to: '08:01'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And the shipped config with "customers.yaml" key "/movement/cells_per_tick" set to "1"
    And a city of 5 working men aged "30-49"
    And a bakery "Far" on "mayfair" with 1000 bread
    When the city runs to "12:00:00"
    Then "Far" sold 0 bread
    And "Far" lost 0 sales
    And nobody is on the map

  # The shipped windows under the shipped 1970 cuts (dark 07:00–10:00 and 13:00–15:30): a working
  # man's morning paper (07:00–08:30) is wholly in the dark, so he goes without; his lunchtime
  # bread (12:00–14:00) has an hour of light, so he buys it before 13:00.
  @R70 @R69
  Scenario: Under the 1970 cuts working men miss the morning paper and buy bread before 13:00
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, newspapers: 1.0}}]"
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "holborn" with 1000 bread
    And a news_stand "Paper" on "holborn" with 1000 newspapers
    And the lights are out today from "07:00" to "10:00"
    And the lights are out today from "13:00" to "15:30"
    When the city runs to "17:00:00"
    Then "Paper" sold 0 newspapers
    And "Paper" lost 0 sales
    And "Bun" sold 30 bread
    And "Bun" sold bread only between "12:00" and "13:00"
