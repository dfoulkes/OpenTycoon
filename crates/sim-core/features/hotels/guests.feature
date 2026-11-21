# Spec §2.2, §7.3 and §8.1: Hotels trade 23:30–03:00 with rooms set by volume (R30). A bed is a
# tourist's last need of the day: a tourist who has finished shopping wanders until the hotels
# open, then takes a room or the train home (R38, R42). Commuters always take the train (R59).
# The tourist pool is everyone not housed (R27), and 30% of it are tourists (R59).
Feature: Tourists sleep in Hotels
  Hotels sell rooms to tourists late at night, and only to tourists.

  Background:
    Given the shipped config
    And the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: night}, goods: {drinks: 1.0}}]"

  @R30
  Scenario: A Hotel's rooms come from its volume
    Given a hotel "Ritz" on "mayfair" 3×3×8
    Then "Ritz" has 18 rooms
    And "Ritz" holds no stock

  @R30 @R38
  Scenario Outline: Hotels trade from 23:30 to 03:00
    Given a hotel "Ritz" on "mayfair" 3×3×8
    When the city runs to "<time>"
    Then "Ritz" is <state>

    Examples:
      | time     | state   |
      | 03:00:00 | shut    |
      | 12:00:00 | shut    |
      | 19:00:00 | shut    |
      | 23:29:50 | shut    |
      | 23:30:00 | trading |
      | 23:50:00 | trading |
      | 00:00:00 | trading |
      | 02:59:50 | trading |

  @R30
  Scenario: Once night trade closes, Hotels are the only business open
    Given a hotel "Ritz" on "mayfair" 3×3×8
    And a bar "Pub" on "mayfair" with 100 drinks
    And a bakery "Bun" on "holborn" with 100 bread
    When the city runs to "23:45:00"
    Then "Ritz" is trading
    And "Pub" is trading
    When the city runs to "00:30:00"
    Then "Ritz" is the only business trading

  @R29 @R59
  Scenario: Guests are only tourists
    Given a hotel "Ritz" on "mayfair" 3×3×40
    And a bar "Pub" on "mayfair" with 5000 drinks
    And a city of 1000 working men aged "30-49"
    When the city runs through the night to "03:00:00"
    Then "Ritz" checked in 90 guests
    And every guest was a tourist
    And nobody is on the map

  @R59 @R38
  Scenario: Commuters never take a room, even with rooms to spare
    Given a hotel "Ritz" on "mayfair" 3×3×100
    And a bar "Pub" on "mayfair" with 5000 drinks
    And an apartment block "Flats" on "holborn" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a city of 100 working men aged "30-49"
    When the city runs through the night to "03:00:00"
    Then "Ritz" checked in 30 guests
    And every guest was a tourist
    And no commuter or resident took a room

  @R42 @R38
  Scenario: A tourist who has finished shopping wanders until the hotels open
    Given a hotel "Ritz" on "mayfair" 3×3×40
    And a bar "Pub" on "mayfair" with 5000 drinks
    And a city of 100 working men aged "30-49"
    When the city runs to "23:29:50"
    Then 30 tourists are wandering the streets
    And no commuter is on the map
    And nobody has checked in
    When the city runs to "23:30:00"
    Then nobody is wandering
    When the city runs through the night to "03:00:00"
    Then "Ritz" checked in 30 guests

  @R38 @R59
  Scenario: With no room to be had, tourists take the train home
    Given a hotel "Inn" on "mayfair" 3×3×4
    And a bar "Pub" on "mayfair" with 5000 drinks
    And a city of 100 working men aged "30-49"
    When the city runs to "23:50:00"
    Then "Inn" checked in 9 guests
    And nobody is wandering or looking for a bed
    And every tourist still about is heading for a tube station
    When the city runs to "01:00:00"
    Then nobody is on the map

  @R38 @R59
  Scenario: With no Hotel in the city, every visitor takes the train home
    Given a bar "Pub" on "mayfair" with 5000 drinks
    And an apartment block "Flats" on "holborn" 3×3×12
    And a working woman aged "30-49" has lived in "Flats" since 1930
    And a city of 100 working men aged "30-49"
    When the city runs to "23:50:00"
    Then every night shopper still about is heading home or to a tube station
    When the city runs to "01:00:00"
    Then nobody is on the map

  @R29
  Scenario: Hotels compete for tourists on price
    Given a hotel "Cheap" on "old_kent_road" 3×3×100
    And a hotel "Dear" on "bethnal_green" 3×3×100
    And "Cheap" prices room_night at 60% of the reference price
    And "Dear" prices room_night at 160% of the reference price
    And a bar "Pub" on "whitechapel" with 5000 drinks
    And a city of 1000 working men aged "30-49"
    When the city runs through the night to "03:00:00"
    Then "Cheap" checked in more guests than "Dear"

  @R29
  Scenario: Tourists who can't afford a room take the train
    Given a hotel "Ritz" on "mayfair" 3×3×40
    And "Ritz" prices room_night at 1000% of the reference price
    And a bar "Pub" on "mayfair" with 5000 drinks
    And a city of 100 working men aged "30-49"
    When the city runs through the night to "03:00:00"
    Then "Ritz" checked in 0 guests
    And nobody is on the map

  @R29
  Scenario: Tourists favour the Souvenir Shop
    Given the shipped config with "customers.yaml" key "/desires" set to "[{match: {window: day}, goods: {souvenirs: 0.1}}]"
    And shoppers shop at any hour of trade
    And the shipped config with "customers.yaml" key "/tourists/desire_boost" set to "{souvenirs: 5.0}"
    And a souvenir shop "Gifts" on "holborn" with 5000 souvenirs
    And a city of 2000 working men aged "30-49"
    When the city runs to "12:00:00"
    Then between 40% and 60% of the day's tourists bought souvenirs
    And between 6% and 14% of the day's commuters bought souvenirs

  @R27 @R59
  Scenario: Every flat let shrinks the tourist pool
    Given an apartment block "Flats" on "holborn" 3×3×12
    And a city of 100 working men aged "30-49"
    Then the city has 100 visitors and 30 tourists
    When 10 of the visitors move into "Flats"
    Then the city has 90 visitors and 27 tourists
