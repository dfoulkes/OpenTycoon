# §4.1: road segments between blocks carry the pedestrian pathfinding, and the Thames runs
# between rows as a gap crossed only at bridges (map.yaml `boards.full.river`). R22: every block
# is a lot and every business touches its edge, so shoppers come and go at a building's frontage
# on the street. R25: residents leave from home, visitors arrive at tube stations. R70: between
# two shopping windows a shopper is off the map, at home or at their away spot.
#
# Shoppers walk only on the street: the one-cell road ring around every lot, never inside a lot
# and never on the river except a bridge. Between windows a resident walks home and a visitor
# walks back to the station they arrived at; only then do they leave the map, and they step back
# out from that same spot when their next window opens.
Feature: Shoppers follow the streets and never vanish
  The designer wants walkers who keep to predictable street paths: round the blocks, over the
  bridges, home between errands, and never popping in or out of the pavement.

  Background:
    Given the shipped config
    And the shipped config with "customers.yaml" key "/movement/spawn_spread_minutes" set to "0"
    And every walker is watched

  @R22 @R25
  Scenario: A walker never stands on the river except on a bridge, and never inside a lot
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And a city of 60 working men aged "30-49"
    And a bakery "Bun" on "bermondsey" with 1000 bread
    And a clothes_store "Togs" on "tower_hill" with 1000 clothes
    When the city runs to "17:00:00"
    Then "Bun" sold 60 bread
    And no walker ever stood on the river except on a bridge
    And no walker ever stood inside a lot

  @R22
  Scenario: A shopper bound for a shop across the river walks round by a bridge
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0}}]"
    And an apartment block "Flats" on "tower_hill" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a bakery "Bun" on "bermondsey" with 1000 bread
    When the city runs to "17:00:00"
    Then "Bun" sold 1 bread
    And the resident crossed the river on a bridge on the way to "Bun"
    And the resident walked further to "Bun" than the straight-line distance

  @R70 @R25
  Scenario: Between windows a shopper is at home or back at their station, not on the pavement
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '08:00', to: '09:00'}, clothes: {from: '12:00', to: '13:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And an apartment block "Flats" on "tower_hill" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "bermondsey" with 1000 bread
    And a clothes_store "Togs" on "soho" with 1000 clothes
    When the city runs to "11:00:00"
    Then "Bun" sold 31 bread
    And nobody is on the map
    And every shopper away between windows is at home or at the station they arrived at

  @R70 @R25
  Scenario: When their next window opens they step back out where they went away
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {bread: 1.0, clothes: 1.0}}]"
    And the shipped config with "customers.yaml" key "/shopping_windows/rules" set to "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}, bread: {from: '08:00', to: '09:00'}, clothes: {from: '12:00', to: '13:00'}}}, {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]"
    And an apartment block "Flats" on "tower_hill" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a city of 30 working men aged "30-49"
    And a bakery "Bun" on "bermondsey" with 1000 bread
    And a clothes_store "Togs" on "soho" with 1000 clothes
    When the city runs to "11:00:00"
    And the city runs to "12:00:00"
    Then every shopper back out stepped out where they went away
    And no walker ever moved more than 3 cells in a tick
    When the city runs to "17:00:00"
    Then "Togs" sold 31 clothes
    And no walker ever moved more than 3 cells in a tick
