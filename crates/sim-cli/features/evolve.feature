# `sim-cli evolve` (docs/evolve.md): the AIs get better the more they play, step one. Each
# personality's ai.yaml settings evolve over generations of games: a candidate genome is seated
# against the other personalities' champions and an idle seat, seats rotated, and the fittest
# are kept and bred. It is harness tooling (spec §11): it implements no game rule, and changes
# settings, never a brain's code, so the brains still act only through public commands.
Feature: Evolving the AI personalities
  A seeded, deterministic genetic algorithm over each personality's tunables, resumable and
  recordable like the balance study.

  Background:
    Given the shipped config
    And the evolution definition:
      """
      population: 3
      generations: 2
      games_per_eval: 3
      elitism: 1
      preset: v1
      seed: 11
      roster: [grocer, toff, idle]
      genes:
        - { name: grocer_bid, key: ai.grocer.max_bid_share, min: 0.1, max: 0.6 }
        - { name: grocer_buffer, key: ai.grocer.cash_buffer, min: 100, max: 1500, distribution: log_uniform, kind: integer }
        - { name: toff_aggression, key: ai.toff.aggression, min: 0.3, max: 1.0 }
        - { name: toff_premium, key: ai.toff.price_premium, min: 0.0, max: 0.6 }
      """

  # ---- Determinism ---------------------------------------------------------------------------

  @spec-11
  Scenario: The same seed gives the same evolution, a different seed a different one
    When I evolve
    And I evolve again into a new directory
    Then both evolutions wrote identical generations.csv, champions and summary.json
    When I evolve from seed 12
    Then its generations.csv differs from the first

  @spec-11
  Scenario: One thread and several threads evolve identically
    Given only "toff" evolves
    When I evolve on 1 thread
    And I evolve on 4 threads
    Then both evolutions wrote identical generations.csv, champions and summary.json

  @spec-11
  Scenario: A resumed evolution equals one run in one go
    When I evolve
    And I evolve into a new directory, stopping after 1 generation
    And generation 1 is cut off mid-write in that directory
    And I evolve that directory again
    Then both evolutions wrote identical generations.csv, champions and summary.json

  # ---- The algorithm -------------------------------------------------------------------------

  @spec-11
  Scenario: Generation 0 starts from the base config's settings
    When I evolve
    Then candidate 0 of generation 0 of "grocer" has the shipped genome "grocer_bid=0.30;grocer_buffer=300"
    And candidate 0 of generation 0 of "toff" has the shipped genome "toff_aggression=0.80;toff_premium=0.30"
    And every candidate plays 3 games

  @spec-11
  Scenario: Elitism keeps the best genome unchanged
    Given the elitism is 2
    When I evolve
    Then each personality's champion of generation 0 is in generation 1 unchanged, as an elite
    And generation 1's elites are generation 0's top 2 in rank order

  @spec-11
  Scenario Outline: Mutation keeps every gene within its range
    Given a <distribution> <kind> gene from <min> to <max>
    And mutation at rate 1 with sigma 1
    When I breed 500 children from parents at the edges of the range
    Then every child's gene is between <min> and <max>
    And every child's gene is a value the gene can take

    Examples:
      | min | max   | distribution | kind    |
      | 0.1 | 0.9   | uniform      | decimal |
      | 1   | 16    | uniform      | integer |
      | 100 | 25000 | log-uniform  | integer |
      | 0.5 | 4.0   | log-uniform  | decimal |

  @spec-11
  Scenario: Fitness ranks a dominating genome above a dominated one
    Given synthetic games where genome "A" wins 6 of 12 and genome "B" wins 3 of 12
    And "A" finishes further above the idle seat than "B" in every game
    Then "A" ranks above "B"
    And with equal wins, the genome further above idle ranks higher
    And with equal wins and net worth, the genome that went bankrupt ranks lower

  # ---- Fitness when one personality dominates (R78) -----------------------------------------
  # Below the low-win floor (shipped 0.10) a genome's fitness blends its win share with its
  # median net worth over the idle seat (clamped at ±£7,000), the net-worth weight rising from
  # 0 at the floor to 0.8 at a win share of 0. At or above the floor nothing changes.

  @spec-11 @R78
  Scenario: Two genomes that never win are ranked by the net worth they gain over idle
    Given the shipped fitness weights
    And these candidates' results over 24 games:
      | genome | wins | bankruptcies | nw_vs_idle |
      | poorer | 0    | 0            | 100000     |
      | richer | 0    | 0            | 300000     |
    Then they rank "richer, poorer"
    And "richer" has a higher fitness than "poorer", and both are above 0

  @spec-11 @R78
  Scenario: A genome that never wins but builds wealth beats one lucky win that loses it
    Given the shipped fitness weights
    And these candidates' results over 24 games:
      | genome  | wins | bankruptcies | nw_vs_idle |
      | lucky   | 1    | 0            | -300000    |
      | builder | 0    | 0            | 400000     |
    Then they rank "builder, lucky"

  @spec-11 @R78
  Scenario: At or above the floor the ranking is unchanged from the old fitness
    Given the shipped fitness weights
    And these candidates' results over 24 games:
      | genome | wins | bankruptcies | nw_vs_idle |
      | A      | 12   | 0            | -500000    |
      | B      | 12   | 2            | 900000     |
      | C      | 6    | 0            | 900000     |
      | D      | 3    | 0            | 0          |
      | E      | 3    | 0            | 100        |
      | F      | 13   | 4            | 0          |
    Then they rank "A, B, F, C, E, D"
    And they rank as the old fitness does: win share − 0.5 × bankruptcy rate, then net worth over idle
    And each fitness is exactly win share − 0.5 × bankruptcy rate

  @spec-11 @R78
  Scenario Outline: The net-worth weight grows as win share falls below the floor
    Given the shipped fitness weights
    Then a genome winning <wins> of 40 games has a net-worth weight of <weight>

    Examples:
      | wins | weight |
      | 40   | 0      |
      | 5    | 0      |
      | 4    | 0      |
      | 3    | 0.2    |
      | 2    | 0.4    |
      | 1    | 0.6    |
      | 0    | 0.8    |

  @spec-11 @R78
  Scenario: Bankruptcy is still penalised below the floor
    Given the shipped fitness weights
    And these candidates' results over 24 games:
      | genome   | wins | bankruptcies | nw_vs_idle |
      | reckless | 0    | 3            | 300000     |
      | always   | 0    | 24           | 700000     |
      | careful  | 0    | 0            | 300000     |
    Then they rank "careful, reckless, always"

  @spec-11 @R78
  Scenario: A blended evolution is identical on any thread count
    Given the roster is "grocer, landlord, sir_penny_sugar, idle"
    And only "grocer" evolves
    And the fitness weights "{ low_win_floor: 1, net_worth_scale: 50000, max_net_worth_weight: 1 }"
    When I evolve on 1 thread
    And I evolve on 4 threads
    Then both evolutions wrote identical generations.csv, champions and summary.json
    And some candidate's fitness is not win share − 0.5 × bankruptcy rate

  @spec-11 @R78
  Scenario Outline: Bad fitness weights are refused, naming the key
    Given the fitness weights "<weights>"
    Then the evolution refuses to start, naming "<error>"

    Examples:
      | weights                          | error                                                  |
      | { low_win_floor: 1.2 }           | fitness.low_win_floor: must be between 0 and 1         |
      | { low_win_floor: -0.1 }          | fitness.low_win_floor: must be between 0 and 1         |
      | { net_worth_scale: 0 }           | fitness.net_worth_scale: must be a positive            |
      | { max_net_worth_weight: 1.5 }    | fitness.max_net_worth_weight: must be between 0 and 1  |
      | { bankruptcy_penalty: 0.1234567 }| fitness.bankruptcy_penalty: must have at most six      |

  @spec-11
  Scenario: An invalid genome is reported, not fatal
    Given the gene "toff_floor" on "ai.toff.opportunism.min_rent" from 0.5 to 8.0
    And the population is 6
    When I evolve
    Then the evolution finishes 2 generations
    And some candidates are invalid, naming "min_rent"
    And no invalid candidate is ranked, a champion or an elite

  # ---- Outputs -------------------------------------------------------------------------------

  @spec-11
  Scenario: A champion ai.yaml loads and reproduces its evaluation games
    Given only "toff" evolves
    And the evolution runs 1 generation
    When I evolve
    And I copy champions/0/ai.yaml over a copy of the config
    Then the copied config loads
    And playing generation 0's games with it gives the lineup's state hashes
    And they are the games the "toff" champion was evaluated in

  @spec-11
  Scenario: summary.json holds the fitness curve per personality
    When I evolve
    Then summary.json has a best and mean fitness curve of 2 points for "grocer" and "toff"
    And summary.json names each generation's champion genome

  @spec-11
  Scenario: Recorded games carry the genome id
    When I evolve, recording every game
    Then every candidate's games are recorded, 3 each, plus 3 lineup games per generation
    And each recorded seat of an evolving personality carries that candidate's or champion's genome id
    And the recorded games re-verify at every midnight

  @spec-11
  Scenario: The command line evolves, writes its outputs and resumes
    Given the definition is saved in the scratch directory as "evolve.yaml"
    When I run sim-cli "evolve config --def {tmp}/evolve.yaml --out {tmp}/out --quiet --no-wall-time"
    Then sim-cli succeeds
    And its output contains "evolve: 2/2 generations (2 run now, 0 resumed)"
    And the evolution in "{tmp}/out" has generations.csv, champions/0/ai.yaml, champions/1/ai.yaml and summary.json
    When I run sim-cli "evolve config --def {tmp}/evolve.yaml --out {tmp}/out --quiet --no-wall-time"
    Then its output contains "(0 run now, 2 resumed)"

  # ---- Guard rails ---------------------------------------------------------------------------

  @spec-10 @R66
  Scenario Outline: The Grocer's opportunism cannot be evolved on
    Given the gene "grocer_opportunism" on "<key>" <range>
    Then the evolution refuses to start, naming "R66" and "<key>"

    Examples:
      | key                               | range                       |
      | ai.grocer.opportunism.enabled     | with values "true, false"   |
      | ai.grocer.opportunism.threshold   | from 0.05 to 0.6            |
      | ai.grocer.opportunism.max_rent    | from 1.0 to 4.0             |

  @spec-10 @R66
  Scenario: An evolution refuses a base config whose Grocer is opportunistic
    Given the override "ai.grocer.opportunism.enabled=true"
    Then the evolution refuses to start, naming "R66" and "grocer.opportunism.enabled"

  @spec-10 @R66
  Scenario: Every champion keeps the Grocer pure retail
    When I evolve
    Then every champions ai.yaml loads with the Grocer's opportunism off

  @spec-11
  Scenario Outline: Genes are one personality's settings
    Given the gene "bad" on "<key>" from 0 to 1
    Then the evolution refuses to start, naming "<error>"

    Examples:
      | key                       | error           |
      | economy.land.base_rate    | genes[4].key    |
      | ai.opponents.x            | genes[4].key    |
      | ai.toff.no_such_key       | no key          |
