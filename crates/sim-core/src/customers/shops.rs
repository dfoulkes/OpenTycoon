//! What shoppers see of the high street: every open shop's offer of every good, scored by
//! customers (§8.3). Selling itself is the businesses module's `Businesses::try_sell`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::movement::{Cell, Geometry};
use super::scoring;
use super::shopping::availability;
use crate::businesses::{BusinessId, Businesses, Hours};
use crate::clock::Clock;
use crate::config::Config;
use crate::config::businesses::{Channel, Good};
use crate::events::Modifiers;
use crate::land::Map;
use crate::money::{BASIS_POINTS_PER_UNIT, Money};
use crate::population::Class;
use crate::population::residential::fit_out_appeal;

/// A good's index in the goods catalogue (`businesses.yaml` `goods`, in key order).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct GoodId(pub u16);

/// Good ids ↔ names, in catalogue key order, with each good's unlock and obsolete years.
///
/// Built on every tick a shopper acts, so it holds the names in one buffer (one allocation,
/// not one per good).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoodsIndex {
    /// Every name, back to back.
    text: String,
    /// Where each name ends in `text`, by id.
    ends: Vec<usize>,
    years: Vec<(Option<i32>, Option<i32>)>,
}

impl GoodsIndex {
    /// The index for a goods catalogue.
    pub fn new(goods: &BTreeMap<String, Good>) -> Self {
        let mut text = String::with_capacity(goods.keys().map(String::len).sum());
        let mut ends = Vec::with_capacity(goods.len());
        for name in goods.keys() {
            text.push_str(name);
            ends.push(text.len());
        }
        GoodsIndex {
            text,
            ends,
            years: goods
                .values()
                .map(|g| (g.unlock_year, g.obsolete_year))
                .collect(),
        }
    }

    fn name_at(&self, i: usize) -> &str {
        let start = if i == 0 { 0 } else { self.ends[i - 1] };
        self.text.get(start..self.ends[i]).unwrap_or("")
    }

    /// A good's `(unlock_year, obsolete_year)` by id (`(None, None)` if unknown).
    pub fn years(&self, id: GoodId) -> (Option<i32>, Option<i32>) {
        self.years
            .get(usize::from(id.0))
            .copied()
            .unwrap_or((None, None))
    }

    /// How many goods the catalogue holds.
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// True if the catalogue is empty.
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// A good's id by name: a binary search, as the catalogue is in key (byte) order.
    pub fn id(&self, name: &str) -> Option<GoodId> {
        let (mut lo, mut hi) = (0, self.ends.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.name_at(mid).cmp(name) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return u16::try_from(mid).ok().map(GoodId),
            }
        }
        None
    }

    /// A good's name by id (`""` if unknown).
    pub fn name(&self, id: GoodId) -> &str {
        let i = usize::from(id.0);
        if i < self.ends.len() {
            self.name_at(i)
        } else {
            ""
        }
    }
}

/// One shop's offer of one good, as scored by customers this tick.
#[derive(Debug, Clone, PartialEq)]
pub struct ShopOffer {
    /// The shop.
    pub shop: BusinessId,
    /// The good.
    pub good: GoodId,
    /// Where its door is.
    pub door: Cell,
    /// Price per unit.
    pub price: Money,
    /// The good's reference price.
    pub ref_price: Money,
    /// Luxury goods gain mildly from price (§8.3).
    pub luxury: bool,
    /// `class_affinity(class, tier) × fit_out_appeal` per class, in [`Class::ALL`] order.
    pub affinity: [f64; 3],
    /// Stock on the shelf right now (customers only score shops with stock).
    pub stock: i64,
}

/// Every open shop's offer of every good it sells, in (shop, good) order (§8.3, §8.4:
/// night businesses only score while open). Only walk-in shops make offers: online sellers and
/// infrastructure have no door to walk to (§7.5). A shop whose type is past its obsolete year
/// pulls fewer shoppers: its affinity is scaled by the same fade as obsolete goods (§7.1).
pub fn shop_offers(
    businesses: &Businesses,
    map: &Map,
    config: &Config,
    clock: &Clock,
    goods: &GoodsIndex,
    geometry: Geometry,
) -> Vec<ShopOffer> {
    let mut out = Vec::new();
    for (id, b) in &businesses.by_id {
        // Shut now (flats never trade) or a Hotel, which sells rooms to tourists
        // (crate::hotels): checked first, as it is cheap and rules out most buildings.
        if b.hours == Hours::Hotel || !b.is_trading(clock) {
            continue;
        }
        let Some(kind) = config.businesses.types.get(&b.type_id) else {
            continue;
        };
        // Only walk-in shops are on the high street: online sellers and infrastructure have no
        // door (§7.5).
        if kind.channel != Channel::Shop {
            continue;
        }
        let Some(block) = map.block(b.block) else {
            continue;
        };
        let appeal = fit_out_appeal(config, b.fit_out);
        let fade = availability(
            None,
            kind.obsolete_year,
            clock.year(),
            &config.customers.obsolescence,
        );
        let affinity = Class::ALL.map(|c| {
            scoring::faded(
                scoring::desirability(&config.customers, c, block.tier, appeal),
                fade,
            )
        });
        let door = geometry.door(block, b.footprint);
        for name in &kind.goods {
            let (Some(good), Some(catalogue)) = (goods.id(name), config.businesses.goods.get(name))
            else {
                continue;
            };
            let ref_price = catalogue.ref_price.money();
            let Some(price) = b.unit_price(name, &config.businesses) else {
                continue;
            };
            out.push(ShopOffer {
                shop: *id,
                good,
                door,
                price,
                ref_price,
                luxury: catalogue.luxury,
                affinity,
                stock: b.stock_of(name),
            });
        }
    }
    out
}

/// Applies the event modifiers in force (§9) to this tick's offers: while a power cut has the
/// shops dark at `second` of the day (R69) there are none, shelf prices carry any price noise,
/// and each shop's pull is scaled by its block's footfall. A no-op with no modifiers in force.
#[allow(
    clippy::cast_precision_loss,
    clippy::float_arithmetic,
    reason = "footfall scales the shop's pull, a customer scoring input (docs/engineering.md)"
)]
pub fn apply_modifiers(
    offers: &mut Vec<ShopOffer>,
    map: &Map,
    businesses: &Businesses,
    modifiers: &Modifiers,
    second: u32,
    goods: &GoodsIndex,
) {
    if modifiers.is_empty() {
        return;
    }
    if modifiers.dark_until(second).is_some() {
        offers.clear();
        return;
    }
    for o in offers.iter_mut() {
        o.price = modifiers
            .price_factor(o.shop, goods.name(o.good))
            .apply(o.price);
        let block = businesses.get(o.shop).map(|b| b.block);
        if let Some(block) = block.filter(|b| map.block(*b).is_some()) {
            let f = modifiers.footfall(block).bp() as f64 / BASIS_POINTS_PER_UNIT as f64;
            for a in &mut o.affinity {
                *a *= f;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_names_round_trip_over_the_shipped_catalogue() {
        let text = include_str!("../../../../config/businesses.yaml");
        let file: serde_json::Value = serde_yaml_ng::from_str(text).unwrap_or_default();
        let goods: BTreeMap<String, Good> =
            serde_json::from_value(file["goods"].clone()).unwrap_or_default();
        assert!(goods.len() > 50, "the shipped catalogue");
        let index = GoodsIndex::new(&goods);
        for (i, name) in goods.keys().enumerate() {
            let id = index.id(name);
            assert_eq!(id, u16::try_from(i).ok().map(GoodId), "{name}");
            assert_eq!(index.name(GoodId(u16::try_from(i).unwrap_or(0))), name);
        }
        assert_eq!(index.id("caviar"), None);
        assert_eq!(index.id(""), None);
        assert_eq!(index.name(GoodId(u16::MAX)), "");
    }
}
