# Spec §10, R64, R65: the five personalities play one game together, seats 1–5 (the Grocer, the
# Landlord, the Toff, the Speculator, Sir Penny Sugar), seat 0 idle. Each draws from its own RNG
# stream and acts only through public commands (R40).
Feature: The AI roster
  Every personality stays within its own ceiling and buffer, builds only what the era allows,
  and plays the same game from the same seed.

  Background:
    Given the shipped config
    And the "v1" game length

  @spec-10 @R7
  Scenario Outline: <who> never bids over its aggression share of its bid ceiling
    When the full roster plays the whole game
    Then every bid <who> made was within its aggression share of its bid ceiling

    Examples:
      | who             |
      | the Landlord    |
      | the Toff        |
      | the Speculator  |
      | Sir Penny Sugar |

  @spec-10 @R3
  Scenario Outline: <who> keeps its cash buffer after every building it puts up
    When the full roster plays the whole game
    Then <who> had at least its cash buffer after every building it put up

    Examples:
      | who             |
      | the Grocer      |
      | the Landlord    |
      | Sir Penny Sugar |

  @spec-10 @R3
  Scenario Outline: <who> keeps its cash buffer after every lease it wins
    When the full roster plays the whole game
    Then <who> had at least its cash buffer after every lease it won

    Examples:
      | who             |
      | the Landlord    |
      | Sir Penny Sugar |

  @spec-10 @R3 @R4
  Scenario: Nobody goes bankrupt in a typical game
    When the full roster plays the whole game
    Then no AI is bankrupt

  # §7.1, §7.4: every personality picks from what is unlocked and not obsolete this year.
  @spec-10
  Scenario: Nobody builds a type before it unlocks or after it is obsolete
    Given groceries unlock in 1940
    And apartments are obsolete from 1940
    When the full roster plays the whole game
    Then nobody built a grocery before 1940
    And nobody built apartments in or after 1940
    And the AIs put up at least 3 buildings
    And the Grocer put up a shop before 1940

  # §10a, docs/engineering.md §3: same seed and config, same game, bit for bit.
  @spec-10
  Scenario: The same seed gives an identical game with the full roster
    When two games with seed 11 are played to the whistle with the full roster seated by ai.yaml
    Then both roster games end with the same state hash
    And the AIs held leases and buildings at the end of both games
