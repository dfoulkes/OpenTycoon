# Spec §10, §7.2: "Sir Penny Sugar deals mostly in residential." He plays seat 1 through public
# commands only (R40); seat 0 is a scripted rival. Tunables are ai.yaml `sir_penny_sugar`
# (shipped: Low and Medium tier, station lots first, 3×3×15 apartment blocks starting at the
# reference rent of £3.00, moved 45% of reference: up when 95% let, down when 60% or less let).
Feature: Sir Penny Sugar
  Sir Penny Sugar builds and runs apartment blocks near the tube, and sets each block's rent by
  how full it is: up when it is full, down when it stands empty.

  Background:
    Given the shipped config
    And Sir Penny Sugar plays seat 1 against a scripted rival in seat 0

  @spec-10 @R25 @R23
  Scenario: He goes for a lot with a tube station first
    When Sir Penny Sugar plays for 1 game hour
    Then Sir Penny Sugar opened an auction on "Whitechapel" at £2,000

  @spec-10 @R8
  Scenario: He never bids on High tier
    Given the rival holds the lease on every block except "Mayfair"
    When the rival bids £14,000 to open an auction on "Mayfair"
    And the auction is played out with the rival passing
    Then Sir Penny Sugar never bid on "Mayfair"

  @spec-10 @R51 @R52
  Scenario Outline: He raises the rent when a block is full and cuts it when it stands empty
    Given <people> people live in the city
    And Sir Penny Sugar holds the lease on "Old Kent Road"
    When Sir Penny Sugar plays until 07:00 on day 2
    Then Sir Penny Sugar owns at least 1 apartment block on "Old Kent Road"
    And the rent on his first block is <direction> £3.00

    Examples:
      | people | direction  |
      | 2000   | above      |
      | 0      | below      |

  @spec-10 @R13 @R20
  Scenario: Over a whole game he deals only in residential
    Given the "v1" game length
    When the full roster plays the whole game
    Then every building Sir Penny Sugar put up is the residential type
    And Sir Penny Sugar put up at least 1 apartment block
