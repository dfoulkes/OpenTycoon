//! The business-stats formula (docs/economy.md "Business stats"; §7.1, §7.4; R10, R35, R41).
//!
//! Every shop type's stats come from one small formula so they stay internally consistent:
//!
//! - **Reference shop**: the type's `min_footprint`, `reference.height` high, on a Medium-tier
//!   block at Medium fit-out ([`reference_footprint`], [`reference_height`]).
//! - **Overheads**: what it pays a day before stock, `rent + utilities` (§5.3), by the same
//!   functions the 06:00 settlement uses ([`daily_overheads`]).
//! - **Target**: overheads in its unlock year plus `build_cost ÷ reference.payback_days`, so the
//!   reference shop covers its bills and earns its build cost back (R41: that cost is its value).
//! - **Contribution** of one unit of good `g` bought and offered: `sell_through × ref_price −
//!   purchase_price`. Unsold units are binned at midnight (R1), so the waste is priced in.
//! - **Stock density** ([`derived_stock_density`]): `target × goods ÷ (volume × Σ contribution)`,
//!   so that `capacity_per_good × Σ contribution = target`. Multi-good stores split their
//!   capacity evenly (§7.1), hence the `goods ÷ Σ`.
//!
//! The shipped `businesses.yaml` densities are this formula rounded up to three significant
//! figures. [`reference_daily_profit`] checks the result through the real capacity, rent and
//! utility functions: a shop of reference size selling its [`reference_footfall`] at reference
//! prices clears rent, utilities and stock on every tier and in every era it trades.
//!
//! All integer: money in pence, ratios in basis points.

use super::{build_cost, capacity_per_good, stock_capacity};
use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::economy::settlement::{utility_base_rate, utility_factor};
use crate::land::{Footprint, Tier, land_rent_per_day, volume};
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::utilities::{UtilityKind, utility_bill};

/// Height of the reference building (`businesses.yaml` `reference.height`).
pub fn reference_height(config: &Config) -> u32 {
    config.businesses.reference.height
}

/// The reference building's footprint: the type's `min_footprint` at the lot corner.
pub fn reference_footprint(kind: &BusinessType) -> Footprint {
    let [w, d] = kind.min_footprint;
    Footprint { x: 0, y: 0, w, d }
}

/// The reference building's volume, `min_footprint × reference.height` (R10).
pub fn reference_volume(kind: &BusinessType, config: &Config) -> i64 {
    volume(reference_height(config), reference_footprint(kind))
}

/// A building's daily bills before stock: `rent + Σ utilities` on `tier` in `year` (§5.3, R10).
/// The same functions the 06:00 settlement charges with, before any holder netting (R24).
pub fn daily_overheads(
    kind: &BusinessType,
    config: &Config,
    volume: i64,
    tier: Tier,
    year: i32,
) -> Money {
    let rent = land_rent_per_day(
        volume,
        config.economy.land.base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
    );
    let utilities: Money = UtilityKind::ALL
        .into_iter()
        .map(|u| {
            utility_bill(
                volume,
                utility_base_rate(config, u, year),
                utility_factor(kind, u),
            )
        })
        .sum();
    rent + utilities
}

/// What a Medium-tier reference shop must make a day from its goods: overheads in its unlock
/// year plus its build cost spread over `reference.payback_days`.
pub fn daily_target(kind: &BusinessType, config: &Config) -> Money {
    let height = reference_height(config);
    let footprint = reference_footprint(kind);
    let vol = volume(height, footprint);
    let build = build_cost(
        height,
        footprint,
        config.economy.building.base_rate.money(),
        config.map.tiers.get(Tier::Medium).desirability_mult.rate(),
        config.businesses.fit_out.medium.build_cost_mult.rate(),
    );
    let payback = i64::from(config.businesses.reference.payback_days.max(1));
    daily_overheads(kind, config, vol, Tier::Medium, kind.unlock_year)
        + Money::from_pence(build.pence() / payback)
}

/// `Σ (sell_through × ref_price − purchase_price)` over the type's goods, in pence × basis
/// points (exact). Unknown goods count nothing.
fn contribution_bp(kind: &BusinessType, config: &Config) -> i128 {
    let s = i128::from(config.businesses.reference.sell_through.rate().bp());
    kind.goods
        .iter()
        .filter_map(|g| config.businesses.goods.get(g))
        .map(|g| {
            s * i128::from(g.ref_price.money().pence())
                - i128::from(g.purchase_price.money().pence()) * i128::from(BASIS_POINTS_PER_UNIT)
        })
        .sum()
}

/// The stock density the formula gives a type: `target × goods ÷ (volume × Σ contribution)`,
/// rounded up to the basis point. `None` for a type with no goods, or whose goods lose money
/// at the reference sell-through (no density can make it pay).
pub fn derived_stock_density(kind: &BusinessType, config: &Config) -> Option<Rate> {
    let contribution = contribution_bp(kind, config);
    let vol = i128::from(reference_volume(kind, config));
    if kind.goods.is_empty() || contribution <= 0 || vol <= 0 {
        return None;
    }
    let unit = i128::from(BASIS_POINTS_PER_UNIT);
    let goods = i128::try_from(kind.goods.len()).ok()?;
    let num = i128::from(daily_target(kind, config).pence()) * goods * unit * unit;
    let den = vol * contribution;
    let bp = (num + den - 1) / den;
    i64::try_from(bp).ok().map(Rate::from_bp)
}

/// Units of each good a reference shop of `kind` holds, with its shipped `stock_density`.
pub fn reference_capacity_per_good(kind: &BusinessType, config: &Config) -> i64 {
    let capacity = stock_capacity(
        reference_height(config),
        reference_footprint(kind),
        kind.stock_density.rate(),
    );
    capacity_per_good(capacity, kind.goods.len())
}

/// Units of each good a reference shop of `kind` sells in a day: its reference footfall,
/// `sell_through × capacity_per_good`, rounded down.
pub fn reference_footfall(kind: &BusinessType, config: &Config) -> i64 {
    let cap = i128::from(reference_capacity_per_good(kind, config));
    let s = i128::from(config.businesses.reference.sell_through.rate().bp());
    i64::try_from(cap * s / i128::from(BASIS_POINTS_PER_UNIT)).unwrap_or(0)
}

/// What a reference shop of `kind` makes in a day on `tier` in `year`: its reference footfall
/// of every good at reference prices, less rent, utilities and a full order of stock (§7.1,
/// R35: no staff). `None` for a type with no goods.
pub fn reference_daily_profit(
    kind: &BusinessType,
    config: &Config,
    tier: Tier,
    year: i32,
) -> Option<Money> {
    if kind.goods.is_empty() {
        return None;
    }
    let cap = reference_capacity_per_good(kind, config);
    let sold = reference_footfall(kind, config);
    let mut takings = Money::ZERO;
    let mut stock = Money::ZERO;
    for good in &kind.goods {
        let g = config.businesses.goods.get(good)?;
        takings += g.ref_price.money().saturating_mul(sold);
        stock += g.purchase_price.money().saturating_mul(cap);
    }
    let overheads = daily_overheads(kind, config, reference_volume(kind, config), tier, year);
    Some(takings - stock - overheads)
}
