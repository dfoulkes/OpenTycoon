# Spec §8.2 and R63: shoppers' allowances are new money created each morning. There are no
# household accounts and no wages, so the economy grows with the population (R6).
Feature: Allowances are new money

  Background:
    Given the shipped config

  @R63
  Scenario: Each morning mints every adult's allowance into the shoppers' purse
    Given a fresh city
    When the fresh city runs until "05:50:00"
    Then the shoppers' purse holds every adult's daily allowance
    And the ledger grew by exactly the allowances minted

  @R63 @R51
  Scenario: Residents' rent comes out of the purse at 06:00, before anyone shops
    # §8.2: rent comes out of the allowance first.
    Given a fresh city
    When the fresh city runs until "06:00:00"
    Then the shoppers' purse holds every adult's daily allowance less the residents' rent
    And the ledger grew by exactly the allowances minted

  @R63
  Scenario: Over several days the ledger only grows by what was minted
    Given a fresh city
    When the fresh city runs for 3 days
    Then the ledger grew by exactly the allowances minted
    And more was minted than one morning's allowances

  @R53 @R28
  Scenario: The city's starter apartment blocks are let on the first morning
    Given a fresh city
    When the fresh city runs until "06:00:00"
    Then every city apartment block has residents
