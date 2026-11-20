//! Online demand (§7.5; R15, R36): from the Internet's arrival a growing share of each good's
//! demand is bought from online sellers, with no travel, out of the shopper's allowance.
//!
//! - `online_share(year, good)` (`customers.yaml` `online`) is the chance a shopper buys a good
//!   on their list online rather than walking for it. Each shopper rolls once per good, when
//!   they set out (§7.5: "buy online or walk"). Whatever they buy online is off their list, so
//!   physical retail loses exactly that demand.
//! - Online sellers are businesses whose type has `channel: online`. They need the Internet
//!   utility (`utilities.yaml` `internet.upgrade_year`, R11), sell from stock like any shop,
//!   are never walked to, and ignore trading windows (any point in the day).
//! - They compete on price plus a **brand** stat that builds a little every day
//!   (`online.brand`); land tier and distance don't matter.
//! - A shopper whose online roll finds no seller with stock at a price they can pay walks for
//!   it instead.
#![allow(
    clippy::float_arithmetic,
    reason = "online shares and brand are customer scoring inputs (docs/engineering.md)"
)]

use std::collections::BTreeMap;

use crate::businesses::{BusinessId, Businesses};
use crate::config::businesses::Channel;
use crate::config::{Config, CustomersConfig};
use crate::money::{Money, Rate};
use crate::population::Class;
use crate::rng::SimRng;

use super::shops::{GoodId, GoodsIndex};

/// `online_share(year, good)`: `share_by_year(year) × good_multiplier(good)`, capped at 1.
/// `share_by_year` is a straight line between the years given, flat after the last, 0 before
/// the first (§7.5).
pub fn online_share(rules: &CustomersConfig, year: i32, good: &str) -> f64 {
    let _ = (rules, year, good);
    0.0
}

/// True once the telephone exchange has become the Internet (R11): online sellers need it
/// (§6, §7.5).
pub fn internet_is_live(config: &Config, year: i32) -> bool {
    year >= config.utilities.internet.upgrade_year
}

/// True if `type_id` sells online.
pub fn is_online(config: &Config, type_id: &str) -> bool {
    config
        .businesses
        .types
        .get(type_id)
        .is_some_and(|t| t.channel == Channel::Online)
}

/// One online seller's offer of one good.
#[derive(Debug, Clone, PartialEq)]
pub struct OnlineOffer {
    /// The seller.
    pub shop: BusinessId,
    /// The good.
    pub good: GoodId,
    /// Price per unit.
    pub price: Money,
    /// The good's reference price.
    pub ref_price: Money,
    /// Luxury goods gain mildly from price (§8.3).
    pub luxury: bool,
    /// The seller's brand, 0–1.
    pub brand: Rate,
    /// Stock on hand.
    pub stock: i64,
}

/// Every open online seller's offer of every good it sells, in (seller, good) order. Empty
/// before the Internet arrives.
pub fn online_offers(
    businesses: &Businesses,
    config: &Config,
    year: i32,
    goods: &GoodsIndex,
    brand: &BTreeMap<BusinessId, Rate>,
) -> Vec<OnlineOffer> {
    let _ = (businesses, config, year, goods, brand);
    Vec::new()
}

/// A seller's brand (absent: `brand.initial`).
pub fn brand_of(
    brand: &BTreeMap<BusinessId, Rate>,
    shop: BusinessId,
    rules: &CustomersConfig,
) -> Rate {
    let _ = (brand, shop, rules);
    Rate::ZERO
}

/// Brand after one more day: `min(max, brand + growth_per_day)` (§7.5).
pub fn brand_after_day(brand: Rate, rules: &CustomersConfig) -> Rate {
    let _ = rules;
    brand
}

/// Picks an online seller for `good` by weighted random over `price_factor × brand`, among
/// offers with stock the shopper can afford. Returns the index into `offers`.
pub fn choose_online(
    rules: &CustomersConfig,
    class: Class,
    budget: Money,
    good: GoodId,
    offers: &[OnlineOffer],
    rng: &mut SimRng,
) -> Option<usize> {
    let _ = (rules, class, budget, good, offers, rng);
    None
}
