# Spec §3 Money: a held lease's land value, one of the parts of net worth (R19, R39). The spec's
# formula is `last auction price × remaining lease years / term`; game.yaml
# `net_worth.land_value` picks how it is valued, so the balance study can compare (R64):
#   book_decay      = last auction price × years left / term               (default, §3)
#   market          = current land base price × years left / term
#   market_no_decay = current land base price while the lease runs
# The current land base price is the block's land base price today (desirability shifts move
# it) after every land-value event so far (the 2008 crash). Scenarios start at 12:00 so the
# day's 06:00 bills are already paid; nobody owns a building, so land is all there is.
Feature: Land value
  Net worth counts a held lease's land. By default the land is worth what was paid for it,
  shrinking as the lease runs down; the market methods value it at what the block is worth today.

  @R19
  Scenario: The shipped config values land as the spec does
    Given the shipped config
    Then land is valued by "book_decay"

  @R19 @R39
  Scenario Outline: A gentrifying block, valued over the life of one lease
    # Bloomsbury's land base price is £5,500. Player 1 paid £6,000 in 1930 for a 30-year lease.
    Given the shipped config with "game.yaml" key "/net_worth/land_value" set to "<method>"
    And a new game with 2 players
    And the game has run to "12:00"
    And player 1 holds the lease on "Bloomsbury" bought for "£6,000.00" in 1930
    And player 1 has a balance of "£0.00"
    When the game runs to midnight
    # scored for 1930: 30 of 30 years left
    Then player 1's net worth is "<1930>"
    When "Bloomsbury" gentrifies and its land base price rises to "£9,900.00"
    And the game runs to midnight
    # scored for 1935: 25 of 30 years left
    Then player 1's net worth is "<1935>"
    When the game runs for 4 days
    # scored for 1955: 5 of 30 years left
    Then player 1's net worth is "<1955>"
    When the game runs to midnight
    # the lease ran out in 1960: the block is back at auction
    Then player 1's net worth is "£0.00"
    And "Bloomsbury" is unleased

    Examples:
      | method          | 1930      | 1935      | 1955      |
      | book_decay      | £6,000.00 | £5,000.00 | £1,000.00 |
      | market          | £5,500.00 | £8,250.00 | £1,650.00 |
      | market_no_decay | £5,500.00 | £9,900.00 | £9,900.00 |

  @R19 @R39
  Scenario Outline: The 2008 crash takes 30% off land, whichever way it is valued
    # Mayfair's land base price is £14,000. Player 1 paid £15,000 in 2000 for a 30-year lease.
    # The 2008 crash (land values ×0.7) lands at the 2005 year step.
    Given the shipped config with "game.yaml" key "/clock/start_year" set to "2000"
    And the shipped config with "game.yaml" key "/net_worth/land_value" set to "<method>"
    And a new game with 2 players
    And the game has run to "12:00"
    And player 1 holds the lease on "Mayfair" bought for "£15,000.00" in 2000
    And player 1 has a balance of "£0.00"
    When the game runs to midnight
    # scored for 2000: 30 of 30 years left, before the crash
    Then player 1's net worth is "<2000>"
    When the game runs to midnight
    # scored for 2005: 25 of 30 years left, after the crash
    Then player 1's net worth is "<2005>"
    # Valuing land at market never changes what an auction asks for it (R23).
    And the land base price of "Mayfair" is "£14,000.00"

    Examples:
      | method          | 2000       | 2005      |
      | book_decay      | £15,000.00 | £8,750.00 |
      | market          | £14,000.00 | £8,166.67 |
      | market_no_decay | £14,000.00 | £9,800.00 |
