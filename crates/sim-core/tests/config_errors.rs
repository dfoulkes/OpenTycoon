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
