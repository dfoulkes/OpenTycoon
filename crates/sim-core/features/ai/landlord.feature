# Spec §10: "The Landlord hoards leases and never builds retail." It plays seat 1 through public
# commands only (R40); seat 0 is a scripted rival. Tunables are ai.yaml `landlord` (shipped:
# pays at most 31% of its bid ceiling for a lease, keeps £500, puts up at most two 3×3×25
# apartment blocks per lease at the reference rent, offers a lease with 5 or fewer years left).
Feature: The Landlord
  The Landlord collects leases across every tier its weights allow, earns land rent and, as a
  sideline, residents' rent from apartment blocks, and sells a lease at auction late in its term
  when the sale beats holding on.

  Background:
    Given the shipped config
    And the Landlord plays seat 1 against a scripted rival in seat 0

  @spec-10 @R23
  Scenario: It bids on a lot of any tier it wants, cheapest first
    Given the rival holds the lease on every block except "Soho"
    When the Landlord plays for 1 game hour
    Then the Landlord opened an auction on "Soho" at £5,000

  @spec-10 @R13 @R20
  Scenario: It never builds retail, only apartment blocks
    Given the "v1" game length
    When the full roster plays the whole game
    Then the Landlord held at least 1 lease
    And every building the Landlord put up is the residential type
    And the Landlord set the rent on every apartment block it built

  @spec-10 @R51 @R56
  Scenario: On a lot it holds it puts up an apartment block and sets its rent
    Given the Landlord holds the lease on "Old Kent Road"
    When the Landlord plays for 2 game hours
    Then the Landlord owns at least 1 apartment block on "Old Kent Road"
    And the Landlord set the rent on every apartment block it built

  @spec-10 @R33
  Scenario Outline: Late in a lease it offers it at auction, but only when selling beats holding
    Given the Landlord holds the lease on "Old Kent Road" with <left> years left, bought for £<paid>
    When the Landlord plays for 1 game hour
    Then the Landlord <did> offer "Old Kent Road" for auction

    Examples:
      | left | paid   | did    |
      | 5    | 2,000  | did    |
      | 20   | 2,000  | didn't |
      | 5    | 30,000 | didn't |

  # §7.3: holding every block of a colour set unlocks one Hotel there. A full-set holder puts it
  # up before anything else; anyone else never tries (the world would refuse it, R14).
  @spec-7.3 @R14 @R29
  Scenario: Holding a whole set, it puts up a Hotel first
    Given the Landlord holds every lease in the "East End" set
    When the Landlord plays for 1 game hour
    Then the first building the Landlord put up is a Hotel in the "East End" set

  @spec-7.3 @R14
  Scenario: Without the whole set it never tries to build a Hotel
    Given the Landlord holds the lease on "Old Kent Road"
    When the Landlord plays for 6 game hours
    Then the Landlord never tried to build a Hotel
