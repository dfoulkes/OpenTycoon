//! Cross-field and cross-file rules that JSON Schema cannot express.
//!
//! Each check returns errors naming the file and the JSON pointer of the offending key. Module
//! owners add their rules here, one small function per file, called from [`check`].

use std::collections::BTreeSet;

use super::{Config, ConfigError, ConfigFile};
use crate::land::BlockKind;
use crate::money::{BASIS_POINTS_PER_UNIT, Rate};

/// Runs every cross-field and cross-file rule.
pub fn check(config: &Config) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    check_game(config, &mut errors);
    check_map(config, &mut errors);
    check_businesses(config, &mut errors);
    check_customers(config, &mut errors);
    check_events(config, &mut errors);
    check_ai(config, &mut errors);
    errors
}

fn err(errors: &mut Vec<ConfigError>, file: ConfigFile, pointer: String, message: String) {
    errors.push(ConfigError::new(file.file_name(), pointer, message));
}

fn check_years(
    errors: &mut Vec<ConfigError>,
    pointer: &str,
    start: i32,
    end: i32,
    years_per_day: u32,
) {
    if start >= end {
        err(
            errors,
            ConfigFile::Game,
            pointer.to_owned(),
            format!("start_year {start} must be before end_year {end}"),
        );
    } else if (end - start).unsigned_abs() % years_per_day != 0 {
        err(
            errors,
            ConfigFile::Game,
            pointer.to_owned(),
            format!("span {start}–{end} is not a whole number of {years_per_day}-year steps (R34)"),
        );
    }
}

fn check_game(config: &Config, errors: &mut Vec<ConfigError>) {
    let game = &config.game;
    let clock = &game.clock;
    check_years(
        errors,
        "/clock",
        clock.start_year,
        clock.end_year,
        clock.years_per_day,
    );
    for (name, preset) in &game.presets {
        check_years(
            errors,
            &format!("/presets/{name}"),
            preset.start_year,
            preset.end_year,
            clock.years_per_day,
        );
    }
    if 60 % clock.tick_game_seconds != 0 {
        err(
            errors,
            ConfigFile::Game,
            "/clock/tick_game_seconds".into(),
            format!(
                "{} must divide 60 so every HH:MM lands on a tick",
                clock.tick_game_seconds
            ),
        );
    }
    let hours = &clock.trading_hours;
    for (name, window) in [
        ("day", hours.day),
        ("night", hours.night),
        ("hotel", hours.hotel),
    ] {
        if window.open == window.close {
            err(
                errors,
                ConfigFile::Game,
                format!("/clock/trading_hours/{name}"),
                "open and close are the same time; the window would be empty".into(),
            );
        }
    }
    for (i, size) in game.world.lot_size.iter().enumerate() {
        if *size == 0 {
            err(
                errors,
                ConfigFile::Game,
                format!("/world/lot_size/{i}"),
                "lot dimensions must be at least 1".into(),
            );
        }
    }
}

fn check_map(config: &Config, errors: &mut Vec<ConfigError>) {
    let map = &config.map;
    let expected = u64::from(map.grid.width) * u64::from(map.grid.height);
    if map.blocks.len() as u64 != expected {
        err(
            errors,
            ConfigFile::Map,
            "/blocks".into(),
            format!(
                "grid is {}×{} = {expected} cells but {} blocks are defined; every cell needs exactly one",
                map.grid.width,
                map.grid.height,
                map.blocks.len()
            ),
        );
    }
    let mut ids = BTreeSet::new();
    let mut cells = BTreeSet::new();
    for (i, block) in map.blocks.iter().enumerate() {
        let at = format!("/blocks/{i}");
        if !ids.insert(block.id.as_str()) {
            err(
                errors,
                ConfigFile::Map,
                format!("{at}/id"),
                format!("duplicate block id {:?}", block.id),
            );
        }
        if block.row >= map.grid.height || block.col >= map.grid.width {
            err(
                errors,
                ConfigFile::Map,
                at.clone(),
                format!(
                    "block {:?} at row {}, col {} is outside the grid",
                    block.id, block.row, block.col
                ),
            );
        } else if !cells.insert((block.row, block.col)) {
            err(
                errors,
                ConfigFile::Map,
                at.clone(),
                format!(
                    "block {:?} shares row {}, col {} with another block",
                    block.id, block.row, block.col
                ),
            );
        }
        if let Some(set) = &block.set
            && !map.sets.contains_key(set)
        {
            err(
                errors,
                ConfigFile::Map,
                format!("{at}/set"),
                format!("unknown set {set:?}"),
            );
        }
        if block.kind == BlockKind::Buildable {
            if block.land_base_price.is_none() {
                err(
                    errors,
                    ConfigFile::Map,
                    format!("{at}/land_base_price"),
                    format!(
                        "buildable block {:?} needs a land_base_price (R23)",
                        block.id
                    ),
                );
            }
            if block.set.is_none() {
                err(
                    errors,
                    ConfigFile::Map,
                    format!("{at}/set"),
                    format!("buildable block {:?} needs a set (R14)", block.id),
                );
            }
        }
    }
    for (i, station) in map.tube_stations.iter().enumerate() {
        if !ids.contains(station.block.as_str()) {
            err(
                errors,
                ConfigFile::Map,
                format!("/tube_stations/{i}/block"),
                format!("unknown block {:?}", station.block),
            );
        }
    }
}

fn check_businesses(config: &Config, errors: &mut Vec<ConfigError>) {
    let world = &config.game.world;
    let businesses = &config.businesses;
    for (id, kind) in &businesses.types {
        let at = format!("/types/{id}");
        if kind.max_height > world.max_height {
            err(
                errors,
                ConfigFile::Businesses,
                format!("{at}/max_height"),
                format!(
                    "max_height {} exceeds world.max_height {} in game.yaml (R58)",
                    kind.max_height, world.max_height
                ),
            );
        }
        for (axis, (min, lot)) in kind
            .min_footprint
            .iter()
            .zip(world.lot_size.iter())
            .enumerate()
        {
            if *min == 0 || min > lot {
                err(
                    errors,
                    ConfigFile::Businesses,
                    format!("{at}/min_footprint/{axis}"),
                    format!("{min} must be between 1 and the lot size {lot}"),
                );
            }
        }
        for (i, good) in kind.goods.iter().enumerate() {
            if !businesses.goods.contains_key(good) {
                err(
                    errors,
                    ConfigFile::Businesses,
                    format!("{at}/goods/{i}"),
                    format!("unknown good {good:?}"),
                );
            }
        }
    }
}

fn sums_to_one(shares: &[super::Ratio]) -> bool {
    shares.iter().map(|r| r.rate().bp()).sum::<i64>() == BASIS_POINTS_PER_UNIT
}

fn check_year_key(errors: &mut Vec<ConfigError>, file: ConfigFile, pointer: String, key: &str) {
    if key.len() != 4 || key.parse::<i32>().is_err() {
        err(
            errors,
            file,
            pointer,
            format!("key {key:?} must be a four-digit year"),
        );
    }
}

fn check_customers(config: &Config, errors: &mut Vec<ConfigError>) {
    let c = &config.customers;
    let bands: BTreeSet<&str> = c.age_bands.iter().map(|b| b.id.as_str()).collect();
    for (year, demo) in &c.demographics {
        let at = format!("/demographics/{year}");
        check_year_key(errors, ConfigFile::Customers, at.clone(), year);
        for (name, shares) in [
            ("class", demo.class.as_slice()),
            ("age", demo.age.as_slice()),
            ("sex", demo.sex.as_slice()),
        ] {
            if !sums_to_one(shares) {
                err(
                    errors,
                    ConfigFile::Customers,
                    format!("{at}/{name}"),
                    "shares must sum to 1".into(),
                );
            }
        }
        if demo.age.len() != c.age_bands.len() {
            err(
                errors,
                ConfigFile::Customers,
                format!("{at}/age"),
                format!(
                    "{} shares for {} age bands",
                    demo.age.len(),
                    c.age_bands.len()
                ),
            );
        }
    }
    for year in c.births_per_woman.keys() {
        check_year_key(
            errors,
            ConfigFile::Customers,
            format!("/births_per_woman/{year}"),
            year,
        );
    }
    for (field, keys) in [
        (
            "allowance/age_multiplier",
            c.allowance.age_multiplier.keys().collect::<Vec<_>>(),
        ),
        (
            "mortality_per_step",
            c.mortality_per_step.keys().collect::<Vec<_>>(),
        ),
    ] {
        for key in keys {
            if !bands.contains(key.as_str()) {
                err(
                    errors,
                    ConfigFile::Customers,
                    format!("/{field}/{key}"),
                    format!("unknown age band {key:?}"),
                );
            }
        }
    }
    if c.visitors.commuter_share.rate() > Rate::ONE {
        err(
            errors,
            ConfigFile::Customers,
            "/visitors/commuter_share".into(),
            "must be between 0 and 1".into(),
        );
    }
}

fn check_events(config: &Config, errors: &mut Vec<ConfigError>) {
    use super::events::Effect;
    let blocks: BTreeSet<&str> = config.map.blocks.iter().map(|b| b.id.as_str()).collect();
    for (i, event) in config.events.historical.iter().enumerate() {
        for (j, effect) in event.effects.iter().enumerate() {
            let named = match effect {
                Effect::DesirabilityChange { blocks, .. }
                | Effect::DestroyBuildings { blocks, .. } => blocks.as_slice(),
                _ => &[],
            };
            for block in named {
                if !blocks.contains(block.as_str()) {
                    err(
                        errors,
                        ConfigFile::Events,
                        format!("/historical/{i}/effects/{j}/blocks"),
                        format!("unknown block {block:?} (not in map.yaml)"),
                    );
                }
            }
        }
    }
    for year in config.events.elections.weights.keys() {
        check_year_key(
            errors,
            ConfigFile::Events,
            format!("/elections/weights/{year}"),
            year,
        );
    }
}

fn check_ai(config: &Config, errors: &mut Vec<ConfigError>) {
    for (i, id) in config.ai.opponents.iter().enumerate() {
        if !config.ai.personalities.contains_key(id) {
            err(
                errors,
                ConfigFile::Ai,
                format!("/opponents/{i}"),
                format!("unknown personality {id:?}"),
            );
        }
    }
}
