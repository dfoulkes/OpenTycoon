//! Cross-field and cross-file rules that JSON Schema cannot express.
//!
//! Each check returns errors naming the file and the JSON pointer of the offending key. Module
//! owners add their rules here, one small function per file, called from [`check`].

use std::collections::BTreeSet;

use super::{Config, ConfigError, ConfigFile};
use crate::money::{BASIS_POINTS_PER_UNIT, Rate};

pub use super::map_checks::check_board;

/// Runs every cross-field and cross-file rule.
pub fn check(config: &Config) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    check_game(config, &mut errors);
    check_game_economy(config, &mut errors);
    errors.extend(check_board(config, &config.map));
    check_businesses(config, &mut errors);
    check_customers(config, &mut errors);
    check_events(config, &mut errors);
    check_ai(config, &mut errors);
    check_auctions(config, &mut errors);
    check_utilities(config, &mut errors);
    check_eras(config, &mut errors);
    check_hotel(config, &mut errors);
    check_tourists_and_online(config, &mut errors);
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
        ("/residential/rent_tax", c.residential.rent_tax),
        (
            "/residential/commute_preference",
            c.residential.commute_preference,
        ),
    ] {
        if share.rate() > Rate::ONE || share.rate() < Rate::ZERO {
            err(
                errors,
                file,
                pointer.into(),
                "must be between 0 and 1".into(),
            );
        }
    }
    if c.residential
        .max_rent_per_flat
        .is_some_and(|cap| cap.money().is_negative())
    {
        err(
            errors,
            file,
            "/residential/max_rent_per_flat".into(),
            "must be at least 0, or null for no cap".into(),
        );
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
        check_cut_headline(
            &event.headline,
            &event.effects,
            &format!("/historical/{i}"),
            errors,
        );
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
        check_cut_headline(
            &event.headline,
            &event.effects,
            &format!("/random_era/events/{i}"),
            errors,
        );
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

/// R69: a power cut's windows each start before they end (inside one day, no wrapping), don't
/// overlap, and leave some of day trade lit: an event never shuts shops for a whole day.
fn check_cut_windows(
    config: &Config,
    windows: &[super::events::CutWindow],
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    let at = format!("{at}/windows");
    if windows.is_empty() {
        err(
            errors,
            ConfigFile::Events,
            at.clone(),
            "a power cut needs at least one window".into(),
        );
    }
    for (i, w) in windows.iter().enumerate() {
        if w.from >= w.to {
            err(
                errors,
                ConfigFile::Events,
                format!("{at}/{i}"),
                format!(
                    "a window must start before it ends, inside one day ({} to {})",
                    w.from, w.to
                ),
            );
        }
        if let Some((j, other)) = windows[..i]
            .iter()
            .enumerate()
            .find(|(_, o)| o.from < w.to && w.from < o.to)
        {
            err(
                errors,
                ConfigFile::Events,
                format!("{at}/{i}"),
                format!(
                    "{} to {} overlaps window {j} ({} to {})",
                    w.from, w.to, other.from, other.to
                ),
            );
        }
    }
    let day = config.game.clock.trading_hours.day;
    let (open, close) = (
        day.open.seconds_since_midnight(),
        day.close.seconds_since_midnight(),
    );
    if open < close && dark_through(windows, open, close) {
        err(
            errors,
            ConfigFile::Events,
            at,
            format!(
                "the cuts black out the whole of day trade ({} to {}); an event can't shut \
                 shops for a whole day (R69)",
                day.open, day.close
            ),
        );
    }
}

/// True if `windows` together cover every second of `open..close`.
fn dark_through(windows: &[super::events::CutWindow], open: u32, close: u32) -> bool {
    let mut lit_from = open;
    loop {
        let Some(w) = windows.iter().find(|w| {
            w.from.seconds_since_midnight() <= lit_from && lit_from < w.to.seconds_since_midnight()
        }) else {
            return false;
        };
        lit_from = w.to.seconds_since_midnight();
        if lit_from >= close {
            return true;
        }
    }
}

/// An event's headline names its power cut's hours with `{cut_windows}` exactly when it has a
/// power cut (R69: the front page says when the lights go out; the text is rendered from the
/// windows, so the two can't drift apart).
fn check_cut_headline(
    headline: &str,
    effects: &[super::events::Effect],
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    use super::events::{CUT_WINDOWS, Effect};
    let cut = effects.iter().any(|e| matches!(e, Effect::PowerCut { .. }));
    let named = headline.contains(CUT_WINDOWS);
    if cut != named {
        let message = if cut {
            format!("a power cut's headline must name its hours with {CUT_WINDOWS}")
        } else {
            format!("{CUT_WINDOWS} names a power cut's hours; this event has no power cut")
        };
        err(
            errors,
            ConfigFile::Events,
            format!("{at}/headline"),
            message,
        );
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
/// lie in 0..=1; factors are not negative; power cuts act within the day (R69).
fn check_effect(
    config: &Config,
    effect: &super::events::Effect,
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    use super::events::Effect;
    let blocks = config.map.block_ids_on_any_board();
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
        Effect::PowerCut { windows } => check_cut_windows(config, windows, at, errors),
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
                format!("unknown block {block:?} (on no board in map.yaml)"),
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
    Block {
        errors,
        name: "grocer",
    }
    .opportunism(config, &config.ai.grocer.opportunism);
    check_roster(config, errors);
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

/// `businesses.yaml` `hotel`: the room good exists, and every Hotel type (`hours: hotel`) holds
/// no stock and sells rooms, not goods (§7.3; R29, R30).
fn check_hotel(config: &Config, errors: &mut Vec<ConfigError>) {
    use crate::businesses::Hours;
    let file = ConfigFile::Businesses;
    let room = &config.businesses.hotel.room_good;
    if !config.businesses.goods.contains_key(room) {
        err(
            errors,
            file,
            "/hotel/room_good".into(),
            format!("unknown good {room:?}"),
        );
    }
    for (id, kind) in &config.businesses.types {
        if kind.hours != Hours::Hotel {
            continue;
        }
        let at = format!("/types/{id}");
        if kind.stock_density.rate() != Rate::ZERO {
            err(
                errors,
                file,
                format!("{at}/stock_density"),
                "a Hotel type must have stock_density: 0 (R30)".into(),
            );
        }
        if !kind.goods.is_empty() {
            err(
                errors,
                file,
                format!("{at}/goods"),
                "a Hotel type sells rooms, not goods: goods: []".into(),
            );
        }
    }
}

/// `customers.yaml` `tourists` and `online`: known goods, shares between 0 and 1, year keys,
/// non-negative multipliers, and a brand that starts at or below its cap (§7.3, §7.5).
fn check_tourists_and_online(config: &Config, errors: &mut Vec<ConfigError>) {
    let file = ConfigFile::Customers;
    let goods = &config.businesses.goods;
    let c = &config.customers;
    for (section, map) in [
        ("/tourists/desire_boost", &c.tourists.desire_boost),
        ("/online/good_multiplier", &c.online.good_multiplier),
    ] {
        for (good, x) in map {
            let at = format!("{section}/{good}");
            if !goods.contains_key(good) {
                err(
                    errors,
                    file,
                    at.clone(),
                    format!("unknown good {good:?} (not in businesses.yaml)"),
                );
            }
            if !(x.is_finite() && *x >= 0.0) {
                err(errors, file, at, "must be a number of at least 0".into());
            }
        }
    }
    for (year, share) in &c.online.share_by_year {
        let at = format!("/online/share_by_year/{year}");
        check_year_key(errors, file, at.clone(), year);
        if !(0.0..=1.0).contains(share) {
            err(
                errors,
                file,
                at,
                "an online share must be between 0 and 1".into(),
            );
        }
    }
    if c.online.brand.initial > c.online.brand.max {
        err(
            errors,
            file,
            "/online/brand/initial".into(),
            "must be at most brand.max".into(),
        );
    }
}

// --- ai.yaml: the Landlord, the Toff, the Speculator and Sir Penny Sugar (§10) ---------------

/// One personality block's errors, each under `/<block>/<key>`.
struct Block<'a> {
    errors: &'a mut Vec<ConfigError>,
    name: &'static str,
}

impl Block<'_> {
    fn bad(&mut self, key: &str, message: impl Into<String>) {
        err(
            self.errors,
            ConfigFile::Ai,
            format!("/{}/{key}", self.name),
            message.into(),
        );
    }

    /// A share in `lo..=hi` basis points.
    fn share(&mut self, key: &str, value: super::Ratio, lo: i64, hi: i64, what: &str) {
        if !(lo..=hi).contains(&value.rate().bp()) {
            self.bad(key, format!("must be {what}"));
        }
    }

    /// A vacancy rent rule (R51): `[start_rent, rent_step, raise_at, cut_at, min_rent,
    /// max_rent]`, keys under `prefix` (`""` or `"opportunism/"`).
    fn vacancy_rent(&mut self, prefix: &str, rule: [super::Ratio; 6]) {
        let one = BASIS_POINTS_PER_UNIT;
        let [start, step, raise_at, cut_at, min, max] = rule;
        let key = |k: &str| format!("{prefix}{k}");
        self.share(
            &key("rent_step"),
            step,
            1,
            10 * one,
            "above 0 and at most 10",
        );
        self.share(&key("raise_at"), raise_at, 0, one, UNIT);
        if cut_at.rate() >= raise_at.rate() || cut_at.rate().bp() < 0 {
            self.bad(&key("cut_at"), "must be at least 0 and below raise_at");
        }
        self.share(&key("min_rent"), min, 0, 10 * one, "between 0 and 10");
        self.share(&key("max_rent"), max, 0, 10 * one, "between 0 and 10");
        if min.rate() > max.rate() {
            self.bad(&key("min_rent"), "must be at most max_rent");
        }
        if start.rate() < min.rate() || start.rate() > max.rate() {
            self.bad(&key("start_rent"), "must be between min_rent and max_rent");
        }
    }

    /// `opportunism` (R66): the switch threshold, the cap and the apartments it builds.
    fn opportunism(&mut self, config: &Config, o: &super::ai::OpportunismConfig) {
        let one = BASIS_POINTS_PER_UNIT;
        self.share(
            "opportunism/threshold",
            o.threshold,
            0,
            10 * one,
            "between 0 and 10",
        );
        self.share(
            "opportunism/max_outside_share",
            o.max_outside_share,
            0,
            one,
            UNIT,
        );
        self.vacancy_rent(
            "opportunism/",
            [
                o.start_rent,
                o.rent_step,
                o.raise_at,
                o.cut_at,
                o.min_rent,
                o.max_rent,
            ],
        );
        self.building(
            config,
            "opportunism/apartment",
            config.customers.residential.building_type.as_str(),
            o.apartment_footprint,
            o.apartment_height,
        );
    }

    fn common(&mut self, aggression: super::Ratio, buffer: super::Pounds) {
        self.share(
            "aggression",
            aggression,
            1,
            BASIS_POINTS_PER_UNIT,
            "above 0 and at most 1: a bid can never pass the ceiling (R7)",
        );
        if buffer.money().is_negative() {
            self.bad("cash_buffer", "must not be negative");
        }
    }

    fn rescue(&mut self, config: &Config, after: super::ClockTime) {
        let day_close = config.game.clock.trading_hours.day.close;
        if after.seconds_since_midnight() < day_close.seconds_since_midnight() {
            self.bad(
                "rescue_after",
                format!("must be at or after day trade closes ({day_close}) in game.yaml"),
            );
        }
    }

    /// A building's footprint and height fit the lot and the type (R22, R58).
    fn building(
        &mut self,
        config: &Config,
        prefix: &str,
        type_id: &str,
        size: [u32; 2],
        height: u32,
    ) {
        let lot = config.game.world.lot_size;
        for (axis, (n, lot)) in size.iter().zip(lot.iter()).enumerate() {
            if *n == 0 || n > lot {
                self.bad(
                    &format!("{prefix}_footprint/{axis}"),
                    format!("{n} must be between 1 and the lot size {lot} (R22)"),
                );
            }
        }
        let Some(kind) = config.businesses.types.get(type_id) else {
            return;
        };
        let footprint = crate::land::Footprint {
            x: 0,
            y: 0,
            w: size[0],
            d: size[1],
        };
        if !crate::businesses::placement::meets_min_footprint(footprint, kind.min_footprint) {
            self.bad(
                &format!("{prefix}_footprint"),
                format!(
                    "smaller than {type_id:?}'s min_footprint {:?}",
                    kind.min_footprint
                ),
            );
        }
        if height > kind.max_height {
            self.bad(
                &format!("{prefix}_height"),
                format!(
                    "{height} is taller than {type_id:?}'s max_height {} (R58)",
                    kind.max_height
                ),
            );
        }
    }
}

/// `ai.yaml` `landlord`, `toff`, `speculator`, `sir_penny_sugar` (§10): shares in range, rents
/// ordered, and every building they would put up fits the lot and its type (R22, R58).
fn check_roster(config: &Config, errors: &mut Vec<ConfigError>) {
    check_landlord(config, errors);
    check_toff(config, errors);
    check_speculator(config, errors);
    check_sir_penny_sugar(config, errors);
}

const UNIT: &str = "between 0 and 1";

fn check_landlord(config: &Config, errors: &mut Vec<ConfigError>) {
    let one = BASIS_POINTS_PER_UNIT;
    let apartments = config.customers.residential.building_type.as_str();
    let cfg = &config.ai.landlord;
    let mut block = Block {
        errors,
        name: "landlord",
    };
    block.common(cfg.aggression, cfg.cash_buffer);
    block.share("bid_jitter", cfg.bid_jitter, 0, one, UNIT);
    block.share("min_tier_weight", cfg.min_tier_weight, 0, one, UNIT);
    block.vacancy_rent(
        "",
        [
            cfg.start_rent,
            cfg.rent_step,
            cfg.raise_at,
            cfg.cut_at,
            cfg.min_rent,
            cfg.max_rent,
        ],
    );
    block.building(
        config,
        "apartment",
        apartments,
        cfg.apartment_footprint,
        cfg.apartment_height,
    );
    block.rescue(config, cfg.rescue_after);
}

fn check_toff(config: &Config, errors: &mut Vec<ConfigError>) {
    let one = BASIS_POINTS_PER_UNIT;
    let cfg = &config.ai.toff;
    let mut block = Block {
        errors,
        name: "toff",
    };
    block.common(cfg.aggression, cfg.cash_buffer);
    block.share("bid_jitter", cfg.bid_jitter, 0, one, UNIT);
    block.share("min_tier_weight", cfg.min_tier_weight, 0, one, UNIT);
    block.share(
        "price_premium",
        cfg.price_premium,
        0,
        4 * one,
        "between 0 and 4",
    );
    block.share(
        "initial_order_share",
        cfg.initial_order_share,
        1,
        one,
        "above 0 and at most 1",
    );
    block.share("order_margin", cfg.order_margin, 0, one, UNIT);
    block.rescue(config, cfg.rescue_after);
    block.opportunism(config, &cfg.opportunism);
    for (i, id) in cfg.shop_types.iter().enumerate() {
        match config.businesses.types.get(id) {
            None => block.bad(
                &format!("shop_types/{i}"),
                format!("unknown business type {id:?} in businesses.yaml"),
            ),
            Some(kind) if kind.goods.is_empty() => block.bad(
                &format!("shop_types/{i}"),
                format!("{id:?} is not a shop with goods"),
            ),
            Some(_) => block.building(config, "shop", id, cfg.shop_footprint, cfg.shop_height),
        }
    }
    if cfg.shop_types.is_empty() {
        block.building(config, "shop", "", cfg.shop_footprint, cfg.shop_height);
    }
}

fn check_speculator(config: &Config, errors: &mut Vec<ConfigError>) {
    let one = BASIS_POINTS_PER_UNIT;
    let cfg = &config.ai.speculator;
    let mut block = Block {
        errors,
        name: "speculator",
    };
    block.common(cfg.aggression, cfg.cash_buffer);
    block.share("bid_jitter", cfg.bid_jitter, 0, one, UNIT);
    block.share("min_tier_weight", cfg.min_tier_weight, 0, one, UNIT);
    block.share(
        "max_markup",
        cfg.max_markup,
        0,
        10 * one,
        "between 0 and 10",
    );
    block.share(
        "bargain_share",
        cfg.bargain_share,
        1,
        one,
        "above 0 and at most 1",
    );
    block.rescue(config, cfg.rescue_after);
    block.opportunism(config, &cfg.opportunism);
}

fn check_sir_penny_sugar(config: &Config, errors: &mut Vec<ConfigError>) {
    let one = BASIS_POINTS_PER_UNIT;
    let apartments = config.customers.residential.building_type.as_str();
    let cfg = &config.ai.sir_penny_sugar;
    let mut block = Block {
        errors,
        name: "sir_penny_sugar",
    };
    block.common(cfg.aggression, cfg.cash_buffer);
    block.share("bid_jitter", cfg.bid_jitter, 0, one, UNIT);
    block.share("min_tier_weight", cfg.min_tier_weight, 0, one, UNIT);
    block.share("station_premium", cfg.station_premium, 0, one, UNIT);
    block.vacancy_rent(
        "",
        [
            cfg.start_rent,
            cfg.rent_step,
            cfg.raise_at,
            cfg.cut_at,
            cfg.min_rent,
            cfg.max_rent,
        ],
    );
    block.building(
        config,
        "apartment",
        apartments,
        cfg.apartment_footprint,
        cfg.apartment_height,
    );
    block.rescue(config, cfg.rescue_after);
}
