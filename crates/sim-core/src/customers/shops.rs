//! What shoppers see of the high street: every open shop's offer of every good, scored by
//! customers (§8.3). Selling itself is the businesses module's `Businesses::try_sell`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::movement::{Cell, Geometry};
use super::scoring;
use crate::businesses::{BusinessId, Businesses};
use crate::clock::Clock;
use crate::config::Config;
use crate::config::businesses::Good;
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

/// Good ids ↔ names, in catalogue key order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GoodsIndex {
    names: Vec<String>,
}

impl GoodsIndex {
    /// The index for a goods catalogue.
    pub fn new(goods: &BTreeMap<String, Good>) -> Self {
        GoodsIndex {
            names: goods.keys().cloned().collect(),
        }
    }

    /// A good's id by name.
    pub fn id(&self, name: &str) -> Option<GoodId> {
        self.names
            .iter()
            .position(|n| n == name)
            .and_then(|i| u16::try_from(i).ok())
            .map(GoodId)
    }

    /// A good's name by id (`""` if unknown).
    pub fn name(&self, id: GoodId) -> &str {
        self.names.get(usize::from(id.0)).map_or("", String::as_str)
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
/// night businesses only score while open).
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
        let Some(kind) = config.businesses.types.get(&b.type_id) else {
            continue;
        };
        if !b.is_trading(clock) {
            continue;
        }
        let Some(block) = map.block(b.block) else {
            continue;
        };
        let appeal = fit_out_appeal(config, b.fit_out);
        let affinity =
            Class::ALL.map(|c| scoring::desirability(&config.customers, c, block.tier, appeal));
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

/// Applies the event modifiers in force (§9) to this tick's offers: shops shut by an open-days
/// rule (the three-day week) drop out, shelf prices carry any price noise, and each shop's pull
/// is scaled by its block's footfall. A no-op with no modifiers in force.
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
    day: u32,
    goods: &GoodsIndex,
) {
    if modifiers.is_empty() {
        return;
    }
    offers.retain(|o| modifiers.may_trade(o.shop, day));
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
