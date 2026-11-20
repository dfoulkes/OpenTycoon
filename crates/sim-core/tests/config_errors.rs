//! Config failure paths at unit level (the BDD scenarios in features/config cover the same
//! rules in prose). Each test breaks one thing and checks the error names the file and key.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs,
    reason = "tests"
)]

mod common;

use sim_core::{Config, ConfigErrors, ConfigSources};

fn load_with(file: &str, pointer: &str, value: &str) -> Result<Config, ConfigErrors> {
    let mut sources = common::shipped_sources();
    common::set_key(&mut sources, file, pointer, value);
    Config::load(&sources)
}

fn assert_names(result: Result<Config, ConfigErrors>, file: &str, text: &str) {
    let errors = result.expect_err("config should have been rejected");
    assert!(
        errors
            .0
            .iter()
            .any(|e| e.file == file && e.to_string().contains(text)),
        "no error names {file} and {text}:\n{errors}"
    );
}

#[test]
fn shipped_config_is_valid() {
    common::shipped_config();
}

#[test]
fn unknown_key_in_every_file_is_rejected_by_name() {
    for (file, pointer) in [
        ("game.yaml", "/clock/tick_secs"),
        ("map.yaml", "/grid/depth"),
        ("auctions.yaml", "/countdown"),
        ("utilities.yaml", "/base_rates/steam"),
        ("businesses.yaml", "/types/bakery/staff"),
        ("customers.yaml", "/visitors/tourist_share"),
        ("events.yaml", "/elections/turnout"),
        ("ai.yaml", "/personalities/grocer/mood"),
    ] {
        let key = pointer.rsplit('/').next().unwrap();
        assert_names(load_with(file, pointer, "1"), file, key);
    }
}

#[test]
fn wrong_type_is_rejected() {
    assert_names(
        load_with("game.yaml", "/clock/start_year", "nineteen-thirty"),
        "game.yaml",
        "/clock/start_year",
    );
}

#[test]
fn out_of_range_is_rejected() {
    assert_names(
        load_with("game.yaml", "/clock/years_per_day", "0"),
        "game.yaml",
        "/clock/years_per_day",
    );
}

#[test]
fn business_max_height_over_world_cap_is_rejected() {
    assert_names(
        load_with("businesses.yaml", "/types/restaurant/max_height", "101"),
        "businesses.yaml",
        "/types/restaurant/max_height",
    );
}

#[test]
fn business_max_height_at_world_cap_is_accepted() {
    let mut sources = common::shipped_sources();
    common::set_key(
        &mut sources,
        "businesses.yaml",
        "/types/restaurant/max_height",
        "100",
    );
    Config::load(&sources).unwrap();
}

#[test]
fn unknown_event_effect_kind_is_rejected() {
    assert_names(
        load_with("events.yaml", "/historical/0/effects/0/kind", "earthquake"),
        "events.yaml",
        "/historical/0/effects/0",
    );
}

#[test]
fn shares_that_do_not_sum_to_one_are_rejected() {
    assert_names(
        load_with("customers.yaml", "/demographics/1930/sex", "[0.5, 0.6]"),
        "customers.yaml",
        "/demographics/1930/sex",
    );
}

#[test]
fn span_not_a_whole_number_of_steps_is_rejected() {
    assert_names(
        load_with("game.yaml", "/clock/end_year", "2052"),
        "game.yaml",
        "/clock",
    );
}

#[test]
fn unknown_references_are_rejected() {
    assert_names(
        load_with("businesses.yaml", "/types/bakery/goods", "[croissants]"),
        "businesses.yaml",
        "croissants",
    );
    assert_names(
        load_with("ai.yaml", "/opponents", "[the_toff]"),
        "ai.yaml",
        "the_toff",
    );
    assert_names(
        load_with("map.yaml", "/blocks/0/set", "purple"),
        "map.yaml",
        "purple",
    );
}

#[test]
fn yaml_syntax_errors_name_the_file_and_line() {
    let sources = common::shipped_sources().with("auctions.yaml", "increments: [50, 100\n");
    let errors = Config::load(&sources).unwrap_err();
    assert!(
        errors
            .0
            .iter()
            .any(|e| e.file == "auctions.yaml" && e.location.starts_with("line")),
        "{errors}"
    );
}

#[test]
fn empty_sources_report_all_eight_missing_files() {
    let errors = Config::load(&ConfigSources::new()).unwrap_err();
    assert_eq!(errors.0.len(), 8, "{errors}");
}

#[test]
fn economy_keys_reject_negative_and_out_of_range_values() {
    for (pointer, value) in [
        ("/bankruptcy/sell_to_city_ratio", "1.5"),
        ("/bankruptcy/sell_to_city_ratio", "-0.1"),
        ("/overdraft/start_limit", "-1"),
        ("/overdraft/increase_per_step", "-2500"),
        ("/overdraft/interest_rate", "-0.01"),
        ("/net_worth/lot_property_uplift", "-0.2"),
    ] {
        assert_names(load_with("game.yaml", pointer, value), "game.yaml", pointer);
    }
}

/// `ai.yaml` `grocer` (§10): each bad value fails the load and names its key.
#[test]
fn grocer_tunables_out_of_range_are_rejected() {
    let cases = [
        ("/grocer/max_bid_share", "0", "/grocer/max_bid_share"),
        ("/grocer/max_bid_share", "1.1", "/grocer/max_bid_share"),
        ("/grocer/min_tier_weight", "1.5", "/grocer/min_tier_weight"),
        ("/grocer/cash_buffer", "-1", "/grocer/cash_buffer"),
        (
            "/grocer/working_class_discount",
            "1",
            "/grocer/working_class_discount",
        ),
        ("/grocer/price_jitter", "0.95", "/grocer/price_jitter"),
        (
            "/grocer/initial_order_share",
            "0",
            "/grocer/initial_order_share",
        ),
        ("/grocer/order_margin", "2", "/grocer/order_margin"),
        ("/grocer/rescue_after", "\"12:00\"", "/grocer/rescue_after"),
        (
            "/grocer/shop_footprint",
            "[0, 3]",
            "/grocer/shop_footprint/0",
        ),
        (
            "/grocer/shop_footprint",
            "[3, 10]",
            "/grocer/shop_footprint/1",
        ),
        ("/grocer/shop_types", "[]", "/grocer/shop_types"),
    ];
    for (pointer, value, names) in cases {
        assert_names(load_with("ai.yaml", pointer, value), "ai.yaml", names);
    }
}

#[test]
fn grocer_shops_must_be_known_day_trade_types_that_fit() {
    assert_names(
        load_with("ai.yaml", "/grocer/shop_types", "[corner_shop]"),
        "ai.yaml",
        "/grocer/shop_types/0",
    );
    assert_names(
        load_with("ai.yaml", "/grocer/shop_types", "[bar]"),
        "ai.yaml",
        "not a day-trade shop",
    );
    assert_names(
        load_with("ai.yaml", "/grocer/shop_types", "[apartments]"),
        "ai.yaml",
        "/grocer/shop_types/0",
    );
    // A grocery is at least 2×3 (businesses.yaml).
    assert_names(
        load_with("ai.yaml", "/grocer/shop_footprint", "[2, 2]"),
        "ai.yaml",
        "min_footprint",
    );
    // R58: a grocery is at most 16 high.
    assert_names(
        load_with("ai.yaml", "/grocer/shop_height", "17"),
        "ai.yaml",
        "/grocer/shop_height",
    );
    assert_names(
        load_with("ai.yaml", "/grocer/unknown_knob", "1"),
        "ai.yaml",
        "unknown_knob",
    );
}

#[test]
fn grocer_at_its_limits_is_accepted() {
    let mut sources = common::shipped_sources();
    for (pointer, value) in [
        ("/grocer/max_bid_share", "1"),
        ("/grocer/shop_height", "16"),
        ("/grocer/order_margin", "0"),
        ("/grocer/rescue_after", "\"17:00\""),
        ("/grocer/shop_footprint", "[9, 9]"),
    ] {
        common::set_key(&mut sources, "ai.yaml", pointer, value);
    }
    Config::load(&sources).unwrap();
}

#[test]
fn event_effects_must_name_known_targets_blocks_and_types() {
    for (pointer, value, names) in [
        (
            "/historical/0/effects/0/target",
            "caviar",
            "/historical/0/effects/0/target",
        ),
        (
            "/historical/1/effects/0/blocks",
            "[atlantis]",
            "/historical/1/effects/0/blocks",
        ),
        (
            "/historical/8/effects/0/types",
            "[airship_dock]",
            "/historical/8/effects/0/types/0",
        ),
        ("/groups/luxury", "[caviar]", "/groups/luxury/0"),
    ] {
        assert_names(
            load_with("events.yaml", pointer, value),
            "events.yaml",
            names,
        );
    }
}

#[test]
fn event_shares_and_open_days_out_of_range_are_rejected() {
    for (pointer, value, names) in [
        (
            "/historical/0/effects/1/share",
            "1.5",
            "/historical/0/effects/1/share",
        ),
        (
            "/historical/1/effects/0/chance",
            "-0.1",
            "/historical/1/effects/0/chance",
        ),
        (
            "/historical/5/effects/0/open",
            "8",
            "/historical/5/effects/0/open",
        ),
        ("/random/0/daily_chance", "2", "/random/0/daily_chance"),
        ("/insurance/premium_rate", "-1", "/insurance/premium_rate"),
        (
            "/historical/10/effects/0/factor",
            "-0.7",
            "/historical/10/effects/0/factor",
        ),
        (
            "/elections/weights/1945/labour",
            "-0.5",
            "/elections/weights/1945",
        ),
    ] {
        assert_names(
            load_with("events.yaml", pointer, value),
            "events.yaml",
            names,
        );
    }
}

#[test]
fn utility_reserve_prices_must_be_positive() {
    assert_names(
        load_with("utilities.yaml", "/reserve_prices/gas", "0"),
        "utilities.yaml",
        "/reserve_prices/gas",
    );
    assert_names(
        load_with("utilities.yaml", "/nationalisation/buyout_ratio", "-1"),
        "utilities.yaml",
        "/nationalisation/buyout_ratio",
    );
}
