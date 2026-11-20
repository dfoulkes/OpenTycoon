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
    check_utilities(config, &mut errors);
    check_eras(config, &mut errors);
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
    check_desires(config, bands, errors);
}

/// Shopping-list rules: known bands and goods, day or night, `0 ≤ P ≤ 1` (§8.2, R55).
fn check_desires(config: &Config, bands: &BTreeSet<&str>, errors: &mut Vec<ConfigError>) {
    use crate::businesses::Hours;
    let c = &config.customers;
    let file = ConfigFile::Customers;
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
            let scale = rule.mode == super::customers::DesireMode::Scale;
            if !scale && !(0.0..=1.0).contains(weight) {
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
    let rules = &config.events;
    for (name, goods) in &rules.groups {
        for (i, good) in goods.iter().enumerate() {
            if !config.businesses.goods.contains_key(good) {
                err(
                    errors,
                    ConfigFile::Events,
                    format!("/groups/{name}/{i}"),
                    format!("unknown good {good:?} (not in businesses.yaml goods)"),
                );
            }
        }
    }
    for (i, event) in rules.historical.iter().enumerate() {
        for (j, effect) in event.effects.iter().enumerate() {
            check_effect(
                config,
                effect,
                &format!("/historical/{i}/effects/{j}"),
                errors,
            );
        }
    }
    for (i, event) in rules.random_era.events.iter().enumerate() {
        for (j, effect) in event.effects.iter().enumerate() {
            check_effect(
                config,
                effect,
                &format!("/random_era/events/{i}/effects/{j}"),
                errors,
            );
        }
    }
    check_share(
        errors,
        "/random_era/chance_per_step",
        rules.random_era.chance_per_step.rate(),
    );
    for (i, event) in rules.random.iter().enumerate() {
        check_share(
            errors,
            &format!("/random/{i}/daily_chance"),
            event.daily_chance.rate(),
        );
        check_share(
            errors,
            &format!("/random/{i}/insured_payout"),
            event.insured_payout.rate(),
        );
    }
    check_share(
        errors,
        "/insurance/premium_rate",
        rules.insurance.premium_rate.rate(),
    );
    for (year, w) in &rules.elections.weights {
        check_year_key(
            errors,
            ConfigFile::Events,
            format!("/elections/weights/{year}"),
            year,
        );
        if w.labour.rate().bp() < 0 || w.conservative.rate().bp() < 0 {
            err(
                errors,
                ConfigFile::Events,
                format!("/elections/weights/{year}"),
                "election weights can't be negative".into(),
            );
        }
    }
}

/// A share (chance, payout, premium) must be between 0 and 1.
fn check_share(errors: &mut Vec<ConfigError>, pointer: &str, share: Rate) {
    if !(0..=BASIS_POINTS_PER_UNIT).contains(&share.bp()) {
        err(
            errors,
            ConfigFile::Events,
            pointer.to_owned(),
            "must be between 0 and 1".into(),
        );
    }
}

/// One event effect: blocks, goods, groups and business types it names must exist; shares
/// lie in 0..=1; factors are not negative; open days fit in the week.
fn check_effect(
    config: &Config,
    effect: &super::events::Effect,
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    use super::events::Effect;
    let blocks: BTreeSet<&str> = config.map.blocks.iter().map(|b| b.id.as_str()).collect();
    let target_ok =
        |t: &str| config.events.groups.contains_key(t) || config.businesses.goods.contains_key(t);
    let mut named_blocks: &[String] = &[];
    let mut target: Option<&str> = None;
    let mut factor = None;
    match effect {
        Effect::DesirabilityChange { blocks, .. } => named_blocks = blocks,
        Effect::DestroyBuildings {
            blocks,
            chance,
            compensation,
        } => {
            named_blocks = blocks;
            check_share(errors, &format!("{at}/chance"), chance.rate());
            factor = Some(("compensation", compensation.rate()));
        }
        Effect::Footfall {
            blocks, factor: f, ..
        } => {
            named_blocks = blocks;
            factor = Some(("factor", f.rate()));
        }
        Effect::DemandMultiplier {
            target: t,
            factor: f,
            ..
        } => {
            target = t.as_deref();
            factor = Some(("factor", f.rate()));
        }
        Effect::SupplyCap {
            target: t, share, ..
        } => {
            target = Some(t);
            check_share(errors, &format!("{at}/share"), share.rate());
        }
        Effect::OpenDays { open, of, .. } => {
            if *of == 0 || open > of {
                err(
                    errors,
                    ConfigFile::Events,
                    format!("{at}/open"),
                    format!("open days must be at most `of` ({open} of {of})"),
                );
            }
        }
        Effect::PriceNoise { spread, .. } => {
            check_share(errors, &format!("{at}/spread"), spread.rate());
        }
        Effect::LandValue { factor: f } => factor = Some(("factor", f.rate())),
        Effect::PropertyValue { types, factor: f } => {
            factor = Some(("factor", f.rate()));
            for (k, t) in types.iter().enumerate() {
                if !config.businesses.types.contains_key(t) {
                    err(
                        errors,
                        ConfigFile::Events,
                        format!("{at}/types/{k}"),
                        format!("unknown business type {t:?} (not in businesses.yaml)"),
                    );
                }
            }
        }
        Effect::ClassShift { share, .. } => {
            check_share(errors, &format!("{at}/share"), share.rate());
        }
        Effect::DecimalCurrency | Effect::Flavour => {}
    }
    for block in named_blocks {
        if !blocks.contains(block.as_str()) {
            err(
                errors,
                ConfigFile::Events,
                format!("{at}/blocks"),
                format!("unknown block {block:?} (not in map.yaml)"),
            );
        }
    }
    if let Some(t) = target.filter(|t| !target_ok(t)) {
        err(
            errors,
            ConfigFile::Events,
            format!("{at}/target"),
            format!("unknown target {t:?} (not a group in events.yaml or a good)"),
        );
    }
    if let Some((key, f)) = factor.filter(|(_, f)| f.bp() < 0) {
        err(
            errors,
            ConfigFile::Events,
            format!("{at}/{key}"),
            format!("can't be negative ({})", f.bp()),
        );
    }
}

/// `utilities.yaml`: reserve prices are positive; the buy-out ratio is not negative (R12).
fn check_utilities(config: &Config, errors: &mut Vec<ConfigError>) {
    let u = &config.utilities;
    let r = &u.reserve_prices;
    for (key, price) in [
        ("electricity", r.electricity),
        ("gas", r.gas),
        ("water", r.water),
        ("telecoms", r.telecoms),
    ] {
        if price.money() <= crate::money::Money::ZERO {
            err(
                errors,
                ConfigFile::Utilities,
                format!("/reserve_prices/{key}"),
                "a utility's reserve price must be above zero".into(),
            );
        }
    }
    if u.nationalisation.buyout_ratio.rate().bp() < 0 {
        err(
            errors,
            ConfigFile::Utilities,
            "/nationalisation/buyout_ratio".into(),
            "can't be negative".into(),
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
    check_grocer(config, errors);
}

/// `ai.yaml` `grocer` (§10): shares in range, and every shop it builds is a known day-trade
/// type that its footprint and height fit (R22, R58).
fn check_grocer(config: &Config, errors: &mut Vec<ConfigError>) {
    let g = &config.ai.grocer;
    let mut bad = |key: &str, message: String| {
        err(errors, ConfigFile::Ai, format!("/grocer/{key}"), message);
    };
    let one = BASIS_POINTS_PER_UNIT;
    let bp = |r: super::Ratio| r.rate().bp();
    if !(0..=one).contains(&bp(g.min_tier_weight)) {
        bad("min_tier_weight", "must be between 0 and 1".into());
    }
    if !(1..=one).contains(&bp(g.max_bid_share)) {
        bad(
            "max_bid_share",
            "must be above 0 and at most 1: a bid can never pass the ceiling (R7)".into(),
        );
    }
    if g.cash_buffer.money().is_negative() {
        bad("cash_buffer", "must not be negative".into());
    }
    if !(0..one).contains(&bp(g.working_class_discount)) {
        bad(
            "working_class_discount",
            "must be at least 0 and below 1".into(),
        );
    }
    if bp(g.price_jitter) < 0 || bp(g.price_jitter) >= one - bp(g.working_class_discount) {
        bad(
            "price_jitter",
            "must be at least 0 and below 1 − working_class_discount, so prices stay above zero"
                .into(),
        );
    }
    if !(1..=one).contains(&bp(g.initial_order_share)) {
        bad(
            "initial_order_share",
            "must be above 0 and at most 1".into(),
        );
    }
    if !(0..=one).contains(&bp(g.order_margin)) {
        bad("order_margin", "must be between 0 and 1".into());
    }
    let day_close = config.game.clock.trading_hours.day.close;
    if g.rescue_after.seconds_since_midnight() < day_close.seconds_since_midnight() {
        bad(
            "rescue_after",
            format!("must be at or after day trade closes ({day_close}) in game.yaml"),
        );
    }
    let lot = config.game.world.lot_size;
    for (axis, (size, lot)) in g.shop_footprint.iter().zip(lot.iter()).enumerate() {
        if *size == 0 || size > lot {
            bad(
                &format!("shop_footprint/{axis}"),
                format!("{size} must be between 1 and the lot size {lot} (R22)"),
            );
        }
    }
    if g.shop_types.is_empty() {
        bad("shop_types", "must name at least one shop type".into());
    }
    for (i, id) in g.shop_types.iter().enumerate() {
        let at = format!("shop_types/{i}");
        let Some(kind) = config.businesses.types.get(id) else {
            bad(
                &at,
                format!("unknown business type {id:?} in businesses.yaml"),
            );
            continue;
        };
        if kind.hours != crate::businesses::Hours::Day || kind.goods.is_empty() {
            bad(&at, format!("{id:?} is not a day-trade shop with goods"));
        }
        if !crate::businesses::placement::meets_min_footprint(
            crate::land::Footprint {
                x: 0,
                y: 0,
                w: g.shop_footprint[0],
                d: g.shop_footprint[1],
            },
            kind.min_footprint,
        ) {
            bad(
                "shop_footprint",
                format!(
                    "smaller than {id:?}'s min_footprint {:?}",
                    kind.min_footprint
                ),
            );
        }
        if g.shop_height > kind.max_height {
            bad(
                "shop_height",
                format!(
                    "{} is taller than {id:?}'s max_height {} (R58)",
                    g.shop_height, kind.max_height
                ),
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

/// Era content (§7.1, §7.4, §7.5, §8.2; R45, R64): unlock and obsolete years in order, no type
/// selling a good before anyone wants it, online sellers only once the internet is up, the
/// stats formula's reference shop in range, scale rules in range, and no desire rule naming a
/// good that unlocks only after the rule ends.
fn check_eras(config: &Config, errors: &mut Vec<ConfigError>) {
    use super::businesses::Channel;
    let b = &config.businesses;
    let file = ConfigFile::Businesses;
    let backwards = |unlock: Option<i32>, obsolete: Option<i32>| matches!((unlock, obsolete), (Some(u), Some(o)) if o < u);
    for (id, good) in &b.goods {
        if backwards(good.unlock_year, good.obsolete_year) {
            err(
                errors,
                file,
                format!("/goods/{id}/obsolete_year"),
                format!(
                    "obsolete before it unlocks ({:?} < {:?})",
                    good.obsolete_year, good.unlock_year
                ),
            );
        }
    }
    let internet = config.utilities.internet.upgrade_year;
    for (id, kind) in &b.types {
        let at = format!("/types/{id}");
        if backwards(Some(kind.unlock_year), kind.obsolete_year) {
            err(
                errors,
                file,
                format!("{at}/obsolete_year"),
                format!("obsolete before it unlocks in {}", kind.unlock_year),
            );
        }
        for (i, name) in kind.goods.iter().enumerate() {
            if let Some(unlock) = b.goods.get(name).and_then(|g| g.unlock_year)
                && unlock > kind.unlock_year
            {
                err(
                    errors,
                    file,
                    format!("{at}/goods/{i}"),
                    format!(
                        "{name} unlocks in {unlock}, after the type ({}): it would stock what nobody wants",
                        kind.unlock_year
                    ),
                );
            }
        }
        if kind.channel == Channel::Online && kind.unlock_year < internet {
            err(
                errors,
                file,
                format!("{at}/unlock_year"),
                format!(
                    "an online business needs the internet ({internet}): utilities.yaml internet.upgrade_year (R15)"
                ),
            );
        }
    }
    let s = b.reference.sell_through.rate();
    if s <= Rate::ZERO || s > Rate::ONE {
        err(
            errors,
            file,
            "/reference/sell_through".into(),
            "must be above 0 and at most 1".into(),
        );
    }
    for (i, rule) in config.customers.desires.iter().enumerate() {
        for (name, weight) in &rule.goods {
            let at = format!("/desires/{i}/goods/{name}");
            if rule.mode == super::customers::DesireMode::Scale && !(0.0..=10.0).contains(weight) {
                err(
                    errors,
                    ConfigFile::Customers,
                    at.clone(),
                    "a scale must be between 0 and 10".into(),
                );
            }
            if let (Some(until), Some(unlock)) = (
                rule.matcher.until_year,
                b.goods.get(name).and_then(|g| g.unlock_year),
            ) && unlock > until
            {
                err(
                    errors,
                    ConfigFile::Customers,
                    at,
                    format!(
                        "never wanted: {name} unlocks in {unlock}, after this rule ends ({until})"
                    ),
                );
            }
        }
    }
}
