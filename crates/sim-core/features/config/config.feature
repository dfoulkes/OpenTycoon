# Spec §10b: every tunable lives in YAML, validated by JSON Schema; the server refuses to
# start on an invalid config and names the bad key, so nothing fails silently.
Feature: Config fails loudly
  A bad config never loads. Every problem names its file and key.

  @R17 @R45
  Scenario: The shipped config loads
    Given the shipped config
    When the config is loaded
    Then loading succeeds

  @R45
  Scenario: An unknown key fails and names the key and the file
    Given the shipped config with "game.yaml" key "/clock/real_minutes_per_dya" set to "20"
    When the config is loaded
    Then loading fails
    And an error names "game.yaml" and "real_minutes_per_dya"

  @R45
  Scenario: A wrong type fails and names the key and the file
    Given the shipped config with "auctions.yaml" key "/turn_real_seconds" set to "ten"
    When the config is loaded
    Then loading fails
    And an error names "auctions.yaml" and "/turn_real_seconds"

  @R45
  Scenario: An out-of-range value fails and names the key and the file
    Given the shipped config with "game.yaml" key "/clock/real_minutes_per_day" set to "0"
    When the config is loaded
    Then loading fails
    And an error names "game.yaml" and "/clock/real_minutes_per_day"

  @R45
  Scenario: Money with fractional pence fails
    Given the shipped config with "businesses.yaml" key "/goods/bread/ref_price" set to "0.105"
    When the config is loaded
    Then loading fails
    And an error names "businesses.yaml" and "ref_price"

  @R45
  Scenario: Every problem is reported, not just the first
    Given the shipped config with "game.yaml" key "/clock/real_minutes_per_dya" set to "20"
    And the shipped config with "map.yaml" key "/boards/v1/grid/width" set to "-1"
    When the config is loaded
    Then loading fails
    And an error names "game.yaml" and "real_minutes_per_dya"
    And an error names "map.yaml" and "/boards/v1/grid/width"

  @R45
  Scenario: A missing file fails and names it
    Given the shipped config without "ai.yaml"
    When the config is loaded
    Then loading fails
    And an error names "ai.yaml" and "missing"

  @R45
  Scenario: An unexpected file fails and names it
    Given the shipped config plus a file "aution.yaml"
    When the config is loaded
    Then loading fails
    And an error names "aution.yaml" and "unknown config file"

  @R58
  Scenario: A business type may be as tall as the world allows
    Given the shipped config with "businesses.yaml" key "/types/bakery/max_height" set to "100"
    When the config is loaded
    Then loading succeeds

  @R58
  Scenario: A business type taller than the world cap fails
    Given the shipped config with "businesses.yaml" key "/types/bakery/max_height" set to "101"
    When the config is loaded
    Then loading fails
    And an error names "businesses.yaml" and "/types/bakery/max_height"

  @R58
  Scenario: Lowering the world cap below a business type's height fails
    Given the shipped config with "game.yaml" key "/world/max_height" set to "10"
    When the config is loaded
    Then loading fails
    And an error names "businesses.yaml" and "/types/bakery/max_height"

  @R58
  Scenario: A business type must be at least one unit tall
    Given the shipped config with "businesses.yaml" key "/types/bar/max_height" set to "0"
    When the config is loaded
    Then loading fails
    And an error names "businesses.yaml" and "/types/bar/max_height"
