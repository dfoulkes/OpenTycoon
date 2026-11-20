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
    check_game_economy(config, &mut errors);
    check_map(config, &mut errors);
    check_businesses(config, &mut errors);
    check_customers(config, &mut errors);
    check_events(config, &mut errors);
    check_ai(config, &mut errors);
    check_city_apartments(config, &mut errors);
    check_auctions(config, &mut errors);
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

/// `game.yaml` keys the economy owns (§2.3, §3): no negative limits, rates or uplifts, and
/// the city never pays more than true value for a sell-back (R5).
fn check_game_economy(config: &Config, errors: &mut Vec<ConfigError>) {
    let game = &config.game;
    let ratio = game.bankruptcy.sell_to_city_ratio.rate();
    if ratio < Rate::ZERO || ratio > Rate::ONE {
        err(
            errors,
            ConfigFile::Game,
            "/bankruptcy/sell_to_city_ratio".into(),
            "must be between 0 and 1 (a share of true value, R5)".into(),
        );
    }
    for (pointer, negative) in [
        (
            "/overdraft/start_limit",
            game.overdraft.start_limit.money().is_negative(),
        ),
        (
            "/overdraft/increase_per_step",
            game.overdraft.increase_per_step.money().is_negative(),
        ),
        (
            "/overdraft/interest_rate",
            game.overdraft.interest_rate.rate() < Rate::ZERO,
        ),
        (
            "/net_worth/lot_property_uplift",
            game.net_worth.lot_property_uplift.rate() < Rate::ZERO,
        ),
    ] {
        if negative {
            err(
                errors,
                ConfigFile::Game,
                pointer.into(),
                "must not be negative".into(),
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
    check_customers_m1(config, &bands, errors);
}

/// Population and customer rules added in M1 (§7.2, §8; R50–R55, R61).
fn check_customers_m1(config: &Config, bands: &BTreeSet<&str>, errors: &mut Vec<ConfigError>) {
    use crate::businesses::Hours;
    let c = &config.customers;
    let file = ConfigFile::Customers;
    for (pointer, share) in [
        ("/allowance/day_share", c.allowance.day_share),
        (
            "/residential/comfortable_share",
            c.residential.comfortable_share,
        ),
        ("/residential/stretch_share", c.residential.stretch_share),
    ] {
        if share.rate() > Rate::ONE {
            err(
                errors,
                file,
                pointer.into(),
                "must be between 0 and 1".into(),
            );
        }
    }
    if c.residential.stretch_share.rate() < c.residential.comfortable_share.rate() {
        err(
            errors,
            file,
            "/residential/stretch_share".into(),
            "must be at least comfortable_share (R52)".into(),
        );
    }
    match config.businesses.types.get(&c.residential.building_type) {
        Some(kind) if kind.hours == Hours::None => {}
        Some(_) => err(
            errors,
            file,
            "/residential/building_type".into(),
            "an apartment type must have hours: none (R56)".into(),
        ),
        None => err(
            errors,
            file,
            "/residential/building_type".into(),
            format!(
                "unknown business type {:?} (not in businesses.yaml)",
                c.residential.building_type
            ),
        ),
    }
    let start = config.game.clock.start_year;
    for (name, keys) in [
        (
            "births_per_woman",
            c.births_per_woman.keys().collect::<Vec<_>>(),
        ),
        ("demographics", c.demographics.keys().collect::<Vec<_>>()),
    ] {
        if !keys
            .iter()
            .any(|k| k.parse::<i32>().is_ok_and(|y| y <= start))
        {
            err(
                errors,
                file,
                format!("/{name}"),
                format!("needs an entry at or before the start year {start}"),
            );
        }
    }
    for (i, rule) in c.desires.iter().enumerate() {
        let at = format!("/desires/{i}");
        if let Some(age) = &rule.matcher.age {
            if !bands.contains(age.as_str()) {
                err(
                    errors,
                    file,
                    format!("{at}/match/age"),
                    format!("unknown age band {age:?}"),
                );
            }
        }
        if matches!(rule.matcher.window, Some(Hours::Hotel | Hours::None)) {
            err(
                errors,
                file,
                format!("{at}/match/window"),
                "shopping windows are day or night".into(),
            );
        }
        for (good, weight) in &rule.goods {
            if !config.businesses.goods.contains_key(good) {
                err(
                    errors,
                    file,
                    format!("{at}/goods/{good}"),
                    format!("unknown good {good:?} (not in businesses.yaml)"),
                );
            }
            if !(0.0..=1.0).contains(weight) {
                err(
                    errors,
                    file,
                    format!("{at}/goods/{good}"),
                    "P(good) must be between 0 and 1".into(),
                );
            }
        }
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

/// `map.yaml` `city_apartments` (R28): one block in each Low-tier set, on a known block, with
/// street frontage (R22), of a known business type and within its limits (R58).
fn check_city_apartments(config: &Config, errors: &mut Vec<ConfigError>) {
    use crate::land::Tier;
    let map = &config.map;
    let apartments = &map.city_apartments;
    let kind = config.businesses.types.get(&apartments.business_type);
    if kind.is_none() {
        err(
            errors,
            ConfigFile::Map,
            "/city_apartments/business_type".into(),
            format!(
                "unknown business type {:?} (not in businesses.yaml)",
                apartments.business_type
            ),
        );
    }
    // A Low-tier set is a set whose blocks are all Low tier.
    let mut low_sets: BTreeSet<&str> = map.sets.keys().map(String::as_str).collect();
    for block in &map.blocks {
        if let Some(set) = &block.set
            && block.tier != Tier::Low
        {
            low_sets.remove(set.as_str());
        }
    }
    let mut covered = BTreeSet::new();
    for (i, site) in apartments.sites.iter().enumerate() {
        let at = format!("/city_apartments/sites/{i}");
        let Some(block) = map.blocks.iter().find(|b| b.id == site.block) else {
            err(
                errors,
                ConfigFile::Map,
                format!("{at}/block"),
                format!("unknown block {:?}", site.block),
            );
            continue;
        };
        match block.set.as_deref() {
            Some(set) if low_sets.contains(set) => {
                if !covered.insert(set) {
                    err(
                        errors,
                        ConfigFile::Map,
                        format!("{at}/block"),
                        format!(
                            "set {set:?} already has a city apartment block (R28: one per Low-tier set)"
                        ),
                    );
                }
            }
            _ => err(
                errors,
                ConfigFile::Map,
                format!("{at}/block"),
                format!("block {:?} is not in a Low-tier set (R28)", site.block),
            ),
        }
        check_city_apartment_site(config, site, &at, errors);
    }
    for set in low_sets.difference(&covered) {
        err(
            errors,
            ConfigFile::Map,
            "/city_apartments/sites".into(),
            format!("Low-tier set {set:?} has no city apartment block (R28: one per Low-tier set)"),
        );
    }
}

/// One `city_apartments` site: street frontage (R22) and the business type's limits (R58).
fn check_city_apartment_site(
    config: &Config,
    site: &super::map::CityApartmentSite,
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    use crate::land::{Footprint, Position, Size};
    let [lot_w, lot_d] = config.game.world.lot_size;
    let kind = config
        .businesses
        .types
        .get(&config.map.city_apartments.business_type);
    let footprint = Footprint::at(
        Size {
            w: site.size[0],
            d: site.size[1],
        },
        Position {
            x: site.position[0],
            y: site.position[1],
        },
    );
    if !footprint.touches_edge(lot_w, lot_d) {
        err(
            errors,
            ConfigFile::Map,
            at.to_owned(),
            format!(
                "footprint {}×{} at {},{} must lie inside the {lot_w}×{lot_d} lot and touch its edge (R22)",
                site.size[0], site.size[1], site.position[0], site.position[1]
            ),
        );
    }
    if let Some(kind) = kind {
        if site.height > kind.max_height {
            err(
                errors,
                ConfigFile::Map,
                format!("{at}/height"),
                format!(
                    "{} exceeds the type's max_height {} (R58)",
                    site.height, kind.max_height
                ),
            );
        }
        if site.size[0] < kind.min_footprint[0] || site.size[1] < kind.min_footprint[1] {
            err(
                errors,
                ConfigFile::Map,
                format!("{at}/size"),
                format!(
                    "smaller than the type's min_footprint {}×{}",
                    kind.min_footprint[0], kind.min_footprint[1]
                ),
            );
        }
    }
}

/// `auctions.yaml`: increments are positive and strictly ascending (§5.1).
fn check_auctions(config: &Config, errors: &mut Vec<ConfigError>) {
    let increments = &config.auctions.increments;
    for (i, inc) in increments.iter().enumerate() {
        if inc.money() <= crate::money::Money::ZERO {
            err(
                errors,
                ConfigFile::Auctions,
                format!("/increments/{i}"),
                "an increment must be more than £0".into(),
            );
        } else if i > 0 && inc.money() <= increments[i - 1].money() {
            err(
                errors,
                ConfigFile::Auctions,
                format!("/increments/{i}"),
                "increments must be in ascending order with no repeats".into(),
            );
        }
    }
}
