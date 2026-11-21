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

use std::collections::{BTreeMap, BTreeSet};

use crate::businesses::{BusinessId, Businesses};
use crate::config::businesses::Channel;
use crate::config::{Config, CustomersConfig};
use crate::money::{Money, Rate};
use crate::population::Class;
use crate::rng::{SimRng, unit_f64};

use super::scoring::{k_class, pence_f64, price_factor, rate_f64, unit_to_rate, weighted_pick};
use super::shops::{GoodId, GoodsIndex};
use super::{Customer, PhaseInputs, Sale};

/// `online_share(year, good)`: `share_by_year(year) × good_multiplier(good)`, capped at 1.
/// `share_by_year` is a straight line between the years given, flat after the last, 0 before
/// the first (§7.5).
pub fn online_share(rules: &CustomersConfig, year: i32, good: &str) -> f64 {
    let online = &rules.online;
    let curve = share_by_year(&online.share_by_year, year);
    let mult = online.good_multiplier.get(good).copied().unwrap_or(1.0);
    (curve * mult).clamp(0.0, 1.0)
}

/// The `share_by_year` curve at `year`: 0 before the first year given, straight lines between
/// years, flat after the last. Keys that are not years are ignored (config rejects them).
pub fn share_by_year(points: &BTreeMap<String, f64>, year: i32) -> f64 {
    let mut points: Vec<(i32, f64)> = points
        .iter()
        .filter_map(|(k, v)| k.parse::<i32>().ok().map(|y| (y, *v)))
        .collect();
    points.sort_by_key(|p| p.0);
    let mut before: Option<(i32, f64)> = None;
    for (y, v) in points {
        if y == year {
            return v;
        }
        if y > year {
            return before.map_or(0.0, |(y0, v0)| {
                v0 + (v - v0) * f64::from(year - y0) / f64::from(y - y0)
            });
        }
        before = Some((y, v));
    }
    before.map_or(0.0, |(_, v)| v)
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
    let mut out = Vec::new();
    if !internet_is_live(config, year) {
        return out;
    }
    for (id, b) in &businesses.by_id {
        if !b.open || !is_online(config, &b.type_id) {
            continue;
        }
        let rate = brand_of(brand, *id, &config.customers);
        for name in b.prices.keys() {
            let (Some(good), Some(catalogue)) = (goods.id(name), config.businesses.goods.get(name))
            else {
                continue;
            };
            let Some(price) = b.unit_price(name, &config.businesses) else {
                continue;
            };
            out.push(OnlineOffer {
                shop: *id,
                good,
                price,
                ref_price: catalogue.ref_price.money(),
                luxury: catalogue.luxury,
                brand: rate,
                stock: b.stock_of(name),
            });
        }
    }
    out
}

/// Grows every online seller's brand by one day (§7.5), once the Internet is live.
pub fn grow_brands(
    brand: &mut BTreeMap<BusinessId, Rate>,
    businesses: &Businesses,
    config: &Config,
    year: i32,
) {
    if !internet_is_live(config, year) {
        return;
    }
    for (id, b) in &businesses.by_id {
        if is_online(config, &b.type_id) {
            let now = brand_of(brand, *id, &config.customers);
            brand.insert(*id, brand_after_day(now, &config.customers));
        }
    }
}

/// A seller's brand (absent: `brand.initial`).
pub fn brand_of(
    brand: &BTreeMap<BusinessId, Rate>,
    shop: BusinessId,
    rules: &CustomersConfig,
) -> Rate {
    brand
        .get(&shop)
        .copied()
        .unwrap_or_else(|| unit_to_rate(rules.online.brand.initial))
}

/// Brand after one more day: `min(max, brand + growth_per_day)` (§7.5).
pub fn brand_after_day(brand: Rate, rules: &CustomersConfig) -> Rate {
    let b = &rules.online.brand;
    unit_to_rate((rate_f64(brand) + b.growth_per_day).min(b.max))
}

/// Goods that only online sellers sell (`channel: online` types, no walk-in shop type): a
/// shopper who wants one buys it online or not at all (§7.5).
pub fn online_only_goods(config: &Config, goods: &GoodsIndex) -> BTreeSet<GoodId> {
    let types = config.businesses.types.values();
    let sold_by = |channel: Channel| -> BTreeSet<&str> {
        types
            .clone()
            .filter(|t| t.channel == channel)
            .flat_map(|t| t.goods.iter().map(String::as_str))
            .collect()
    };
    let walk_in = sold_by(Channel::Shop);
    sold_by(Channel::Online)
        .into_iter()
        .filter(|g| !walk_in.contains(g))
        .filter_map(|g| goods.id(g))
        .collect()
}

/// A shopper setting out buys online (§7.5): for each good on their list, one roll at
/// `online_share(year, good)` (certain for goods only sold online); on a hit they buy it from an online seller
/// ([`choose_online`]) and it comes off their list and out of their budget. A hit with no
/// seller able to sell it leaves the good on the list, to be walked for. Returns the sales,
/// in order; the world pays each from the shoppers' purse to the seller's owner.
pub fn shop_online(
    c: &mut Customer,
    offers: &mut [OnlineOffer],
    only_online: &BTreeSet<GoodId>,
    inputs: PhaseInputs<'_>,
    goods: &GoodsIndex,
    businesses: &mut Businesses,
    rng: &mut SimRng,
) -> Vec<Sale> {
    let config = inputs.config;
    let rules = &config.customers;
    let year = inputs.clock.year();
    let mut sales = Vec::new();
    let mut i = 0;
    while i < c.list.len() {
        let good = c.list[i].good;
        let name = goods.name(good);
        let share = if only_online.contains(&good) {
            1.0
        } else {
            online_share(rules, year, name)
        };
        if share > 0.0 && unit_f64(rng) < share {
            if let Some(k) = choose_online(config, c.cohort.class, c.budget, good, offers, rng) {
                let shop = offers[k].shop;
                match businesses.try_sell_online(shop, name, 1, &config.businesses) {
                    Ok(amount) => {
                        offers[k].stock -= 1;
                        c.budget -= amount;
                        c.list.remove(i);
                        sales.push(Sale {
                            shop,
                            good,
                            qty: 1,
                            amount,
                            customer: c.id,
                        });
                        continue;
                    }
                    Err(_) => offers[k].stock = 0,
                }
            }
        }
        i += 1;
    }
    sales
}

/// Picks an online seller for `good` by weighted random over `price_factor × brand`, among
/// offers with stock the shopper can afford. Returns the index into `offers`.
pub fn choose_online(
    config: &Config,
    class: Class,
    budget: Money,
    good: GoodId,
    offers: &[OnlineOffer],
    rng: &mut SimRng,
) -> Option<usize> {
    let rules = &config.customers;
    let k = k_class(&config.economy, class);
    let mut index = Vec::new();
    let mut scores = Vec::new();
    for (i, o) in offers.iter().enumerate() {
        if o.good != good || o.stock <= 0 || o.price > budget {
            continue;
        }
        let k = if o.luxury {
            -rules.luxury_price_boost
        } else {
            k
        };
        index.push(i);
        scores.push(
            price_factor(k, pence_f64(o.price), pence_f64(o.ref_price).max(1.0))
                * rate_f64(o.brand),
        );
    }
    weighted_pick(&scores, rng).and_then(|k| index.get(k).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::test_support::shipped_config;

    fn points(p: &[(&str, f64)]) -> BTreeMap<String, f64> {
        p.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    #[test]
    fn the_share_curve_is_zero_before_straight_between_and_flat_after() {
        let p = points(&[("1995", 0.05), ("2020", 0.5)]);
        assert!(share_by_year(&p, 1990).abs() < 1e-12);
        assert!((share_by_year(&p, 1995) - 0.05).abs() < 1e-12);
        assert!((share_by_year(&p, 2005) - 0.23).abs() < 1e-12);
        assert!((share_by_year(&p, 2020) - 0.5).abs() < 1e-12);
        assert!((share_by_year(&p, 2050) - 0.5).abs() < 1e-12);
        assert!(share_by_year(&BTreeMap::new(), 2050).abs() < 1e-12);
    }

    #[test]
    fn the_good_multiplier_scales_the_share_and_caps_it_at_one() {
        let mut config = shipped_config();
        config.customers.online.share_by_year = points(&[("1995", 0.6)]);
        config.customers.online.good_multiplier = points(&[("bread", 0.5), ("meals", 3.0)]);
        let rules = &config.customers;
        assert!((online_share(rules, 2000, "bread") - 0.3).abs() < 1e-12);
        assert!((online_share(rules, 2000, "meals") - 1.0).abs() < 1e-12);
        assert!((online_share(rules, 2000, "clothes") - 0.6).abs() < 1e-12);
        assert!(online_share(rules, 1990, "clothes").abs() < 1e-12);
    }

    #[test]
    fn a_brand_grows_each_day_up_to_its_cap() {
        let mut config = shipped_config();
        config.customers.online.brand.initial = 0.2;
        config.customers.online.brand.growth_per_day = 0.1;
        config.customers.online.brand.max = 0.45;
        let rules = &config.customers;
        let shop = BusinessId(7);
        let start = brand_of(&BTreeMap::new(), shop, rules);
        assert_eq!(start, unit_to_rate(0.2));
        let day1 = brand_after_day(start, rules);
        assert_eq!(day1, unit_to_rate(0.3));
        let day3 = brand_after_day(brand_after_day(day1, rules), rules);
        assert_eq!(day3, unit_to_rate(0.45));
    }

    #[test]
    fn the_internet_arrives_in_its_upgrade_year() {
        let config = shipped_config();
        let year = config.utilities.internet.upgrade_year;
        assert!(!internet_is_live(&config, year - 5));
        assert!(internet_is_live(&config, year));
    }
}
