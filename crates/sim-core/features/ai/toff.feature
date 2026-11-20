# Spec §10: "The Toff only goes for High tier." It plays seat 1 through public commands only
# (R40); seat 0 is a scripted rival. Tunables are ai.yaml `toff` (shipped: pays up to 80% of its
# bid ceiling for a prime lot, keeps £1,500, builds at most 3 high fit-out 3×3×12 shops a lot,
# priced 30% over reference).
Feature: The Toff
  The Toff wants prime land and nothing else. It builds fewer, finer shops, prices for the
  upper class and keeps a large buffer.

  Background:
    Given the shipped config
    And the Toff plays seat 1 against a scripted rival in seat 0

  @spec-10 @R8
  Scenario: It never bids below High tier
    Given the rival holds the lease on every block except "Soho"
    When the rival bids £5,000 to open an auction on "Soho"
    And the auction is played out with the rival passing
    Then the Toff never bid on "Soho"
    And "Soho" stays with the rival

  @spec-10 @R8
  Scenario: Over a whole game against the full roster it never bids below High tier
    Given the "v1" game length
    When the full roster plays the whole game
    Then the Toff never bid on a lot below High tier

  @spec-10 @R13 @R58
  Scenario: On a prime lot it builds a few high fit-out shops, priced over reference
    Given the Toff holds the lease on "Mayfair"
    When the Toff plays for 6 game hours
    Then the Toff owns at least 1 shop on "Mayfair"
    And every Toff shop has high fit-out
    And the Toff owns at most 3 shops on "Mayfair"
    And every Toff price is 130% of reference
    And the Toff's balance is at least its cash buffer

  # On the shipped v1 economy a High lot's base price (£11,000+) is more than the £7,000 a player
  # starts with, so the Toff's buffer keeps it out; with more starting cash it buys in.
  @spec-10 @R3 @R62
  Scenario: Given the means, it wins a prime lot and keeps its buffer
    Given every player starts with £20,000
    And the "v1" game length
    When the full roster plays the whole game
    Then the Toff held at least 1 lease
    And the Toff had at least its cash buffer after every lease it won
    And the Toff put up at least 1 shop
    And the Toff had at least its cash buffer after every building it put up

  # §7.3: holding every block of a colour set unlocks one Hotel there. A full-set holder puts it
  # up before anything else; anyone else never tries (the world would refuse it, R14).
  @spec-7.3 @R14 @R29
  Scenario: Holding a whole set, it puts up a Hotel first
    Given the Toff holds every lease in the "Mayfair & Belgravia" set
    When the Toff plays for 1 game hour
    Then the first building the Toff put up is a Hotel in the "Mayfair & Belgravia" set

  @spec-7.3 @R14
  Scenario: Without the whole set it never tries to build a Hotel
    Given the Toff holds the lease on "Mayfair"
    When the Toff plays for 6 game hours
    Then the Toff never tried to build a Hotel
