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
        ("map.yaml", "/boards/v1/grid/depth"),
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
        load_with("map.yaml", "/boards/v1/blocks/0/set", "purple"),
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
        ("/net_worth/land_value", "freehold"),
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

#[test]
fn hotel_and_online_settings_are_checked_by_name() {
    // §7.3, §7.5: the Hotel type, tourists' desire boost and the online shares.
    for (file, pointer, value, text) in [
        (
            "businesses.yaml",
            "/hotel/room_good",
            "spa",
            "/hotel/room_good",
        ),
        (
            "businesses.yaml",
            "/types/hotel/goods",
            "[bread]",
            "/types/hotel/goods",
        ),
        (
            "businesses.yaml",
            "/hotel/room_size",
            "0",
            "/hotel/room_size",
        ),
        (
            "businesses.yaml",
            "/types/hotel/stock_density",
            "0.5",
            "/types/hotel/stock_density",
        ),
        (
            "customers.yaml",
            "/tourists/desire_boost",
            "{fudge: 2.0}",
            "/tourists/desire_boost/fudge",
        ),
        (
            "customers.yaml",
            "/tourists/desire_boost",
            "{souvenirs: -1.0}",
            "/tourists/desire_boost/souvenirs",
        ),
        (
            "customers.yaml",
            "/online/share_by_year",
            "{1995: 1.5}",
            "/online/share_by_year/1995",
        ),
        (
            "customers.yaml",
            "/online/share_by_year",
            "{soon: 0.1}",
            "four-digit year",
        ),
        (
            "customers.yaml",
            "/online/good_multiplier",
            "{fudge: 1.0}",
            "/online/good_multiplier/fudge",
        ),
        (
            "customers.yaml",
            "/online/brand/max",
            "0.1",
            "/online/brand/initial",
        ),
        (
            "customers.yaml",
            "/online/brand/max",
            "1.5",
            "/online/brand/max",
        ),
        (
            "businesses.yaml",
            "/types/bakery/channel",
            "telepathy",
            "/types/bakery/channel",
        ),
    ] {
        assert_names(load_with(file, pointer, value), file, text);
    }
}

/// `ai.yaml` `landlord`, `toff`, `speculator`, `sir_penny_sugar` (§10): each bad value fails
/// the load and names its key.
#[test]
fn roster_tunables_out_of_range_are_rejected() {
    let cases = [
        ("/landlord/aggression", "0", "/landlord/aggression"),
        ("/landlord/aggression", "1.5", "/landlord/aggression"),
        ("/landlord/cash_buffer", "-1", "/landlord/cash_buffer"),
        (
            "/landlord/min_tier_weight",
            "2",
            "/landlord/min_tier_weight",
        ),
        ("/landlord/rent", "-0.5", "/landlord/rent"),
        ("/landlord/max_leases", "0", "/landlord/max_leases"),
        (
            "/landlord/apartment_height",
            "101",
            "/landlord/apartment_height",
        ),
        ("/landlord/apartment_footprint", "[1, 1]", "min_footprint"),
        (
            "/landlord/apartment_footprint",
            "[10, 3]",
            "/landlord/apartment_footprint/0",
        ),
        (
            "/landlord/rescue_after",
            "\"09:00\"",
            "/landlord/rescue_after",
        ),
        ("/toff/aggression", "0", "/toff/aggression"),
        ("/toff/bid_jitter", "1.5", "/toff/bid_jitter"),
        ("/landlord/bid_jitter", "-0.1", "/landlord/bid_jitter"),
        ("/toff/cash_buffer", "-5", "/toff/cash_buffer"),
        ("/toff/price_premium", "5", "/toff/price_premium"),
        (
            "/toff/initial_order_share",
            "0",
            "/toff/initial_order_share",
        ),
        ("/toff/order_margin", "1.5", "/toff/order_margin"),
        ("/toff/shop_types", "[corner_shop]", "/toff/shop_types/0"),
        ("/toff/shop_types", "[apartments]", "/toff/shop_types/0"),
        ("/toff/shop_height", "21", "/toff/shop_height"),
        ("/toff/fit_out", "gold", "/toff/fit_out"),
        ("/toff/shops_per_lot", "0", "/toff/shops_per_lot"),
        ("/speculator/aggression", "1.01", "/speculator/aggression"),
        (
            "/toff/opportunism/threshold",
            "11",
            "/toff/opportunism/threshold",
        ),
        (
            "/toff/opportunism/max_outside_share",
            "1.5",
            "/toff/opportunism/max_outside_share",
        ),
        (
            "/speculator/opportunism/cut_at",
            "0.99",
            "/speculator/opportunism/cut_at",
        ),
        (
            "/speculator/opportunism/start_rent",
            "9",
            "/speculator/opportunism/start_rent",
        ),
        (
            "/grocer/opportunism/apartment_height",
            "101",
            "/grocer/opportunism/apartment_height",
        ),
        (
            "/grocer/opportunism/apartment_footprint",
            "[1, 1]",
            "min_footprint",
        ),
        (
            "/grocer/opportunism/enabled",
            "1",
            "/grocer/opportunism/enabled",
        ),
        (
            "/speculator/apartment_hold_hours",
            "-1",
            "/speculator/apartment_hold_hours",
        ),
        (
            "/speculator/bargain_share",
            "0",
            "/speculator/bargain_share",
        ),
        ("/speculator/max_holdings", "0", "/speculator/max_holdings"),
        (
            "/speculator/rescue_after",
            "\"06:00\"",
            "/speculator/rescue_after",
        ),
        (
            "/sir_penny_sugar/aggression",
            "0",
            "/sir_penny_sugar/aggression",
        ),
        (
            "/sir_penny_sugar/station_premium",
            "1.2",
            "/sir_penny_sugar/station_premium",
        ),
        (
            "/sir_penny_sugar/rent_step",
            "0",
            "/sir_penny_sugar/rent_step",
        ),
        (
            "/sir_penny_sugar/raise_at",
            "1.1",
            "/sir_penny_sugar/raise_at",
        ),
        ("/sir_penny_sugar/cut_at", "0.95", "/sir_penny_sugar/cut_at"),
        (
            "/sir_penny_sugar/min_rent",
            "4",
            "/sir_penny_sugar/min_rent",
        ),
        (
            "/sir_penny_sugar/start_rent",
            "0.1",
            "/sir_penny_sugar/start_rent",
        ),
        (
            "/sir_penny_sugar/apartments_per_lot",
            "0",
            "/sir_penny_sugar/apartments_per_lot",
        ),
        ("/sir_penny_sugar/unknown_knob", "1", "unknown_knob"),
    ];
    for (pointer, value, names) in cases {
        assert_names(load_with("ai.yaml", pointer, value), "ai.yaml", names);
    }
}

#[test]
fn roster_at_its_limits_is_accepted() {
    let mut sources = common::shipped_sources();
    for (pointer, value) in [
        ("/landlord/aggression", "1"),
        ("/landlord/apartments_per_lot", "0"),
        ("/landlord/sell_within_years", "0"),
        ("/toff/shop_types", "[]"),
        ("/toff/price_premium", "0"),
        ("/speculator/cash_buffer", "0"),
        ("/speculator/bargain_share", "1"),
        ("/sir_penny_sugar/cut_at", "0"),
        ("/sir_penny_sugar/apartment_height", "100"),
    ] {
        common::set_key(&mut sources, "ai.yaml", pointer, value);
    }
    Config::load(&sources).unwrap();
}

#[test]
fn selecting_a_board_the_file_does_not_have_is_rejected() {
    let mut sources = common::shipped_sources();
    let text = sources.get("map.yaml").unwrap().to_owned();
    let mut doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    doc["boards"].as_mapping_mut().unwrap().remove("full");
    doc["map"] = "full".into();
    sources.insert("map.yaml", serde_yaml_ng::to_string(&doc).unwrap());
    assert_names(
        Config::load(&sources),
        "map.yaml",
        "/boards/full is missing",
    );
}
