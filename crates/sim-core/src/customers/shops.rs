//! What shoppers see of the high street, and the narrow selling interface they buy through.
//!
//! [`Shops::try_sell`] is the only way a customer takes stock off a shelf. The businesses
//! module owns stock; until it ships its own implementation, [`BusinessShops`] is a minimal
//! one over [`Businesses`] (stock never goes negative; price = `ref_price × price share`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::movement::{Cell, Geometry};
use super::scoring;
use crate::businesses::{BusinessId, Businesses, Hours};
use crate::clock::{Clock, TradingWindow};
use crate::config::Config;
use crate::config::businesses::Good;
use crate::land::Map;
use crate::money::{Money, Rate};
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

/// Why a sale didn't happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SellError {
    /// No such shop, or it doesn't sell the good.
    UnknownShop,
    /// The shop is shut.
    Closed,
    /// Not enough stock: a lost sale (§8.3).
    OutOfStock,
}

/// The selling interface customers buy through (owned by businesses).
pub trait Shops {
    /// Sells `qty` of `good` at the shop's current price. On success, stock has gone down by
    /// `qty` and the return value is what the customer pays.
    fn try_sell(&mut self, shop: BusinessId, good: &str, qty: u32) -> Result<Money, SellError>;
}

/// A minimal [`Shops`] over [`Businesses`] until the businesses module provides its own.
#[derive(Debug)]
pub struct BusinessShops<'a> {
    /// Businesses whose stock is sold.
    pub businesses: &'a mut Businesses,
    /// The goods catalogue (reference prices).
    pub goods: &'a BTreeMap<String, Good>,
}

/// A shop's price for a good: `ref_price × price share` (§7.1; share 1.0 if unset).
pub fn shop_price(prices: &BTreeMap<String, Rate>, good: &str, ref_price: Money) -> Money {
    prices
        .get(good)
        .copied()
        .unwrap_or(Rate::ONE)
        .apply(ref_price)
}

impl Shops for BusinessShops<'_> {
    fn try_sell(&mut self, shop: BusinessId, good: &str, qty: u32) -> Result<Money, SellError> {
        let business = self
            .businesses
            .by_id
            .get_mut(&shop)
            .ok_or(SellError::UnknownShop)?;
        let catalogue = self.goods.get(good).ok_or(SellError::UnknownShop)?;
        if !business.open {
            return Err(SellError::Closed);
        }
        let stock = business.stock.get_mut(good).ok_or(SellError::OutOfStock)?;
        let qty = i64::from(qty);
        if *stock < qty {
            return Err(SellError::OutOfStock);
        }
        *stock -= qty;
        Ok(shop_price(&business.prices, good, catalogue.ref_price.money()).saturating_mul(qty))
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

/// Is a business of these hours trading now?
pub fn trading_now(clock: &Clock, hours: Hours) -> bool {
    match hours {
        Hours::Day => clock.is_open(TradingWindow::Day),
        Hours::Night => clock.is_open(TradingWindow::Night),
        Hours::Hotel => clock.is_open(TradingWindow::Hotel),
        Hours::None => false,
    }
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
        if !b.open || !trading_now(clock, kind.hours) {
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
            out.push(ShopOffer {
                shop: *id,
                good,
                door,
                price: shop_price(&b.prices, name, ref_price),
                ref_price,
                luxury: catalogue.luxury,
                affinity,
                stock: b.stock.get(name).copied().unwrap_or(0),
            });
        }
    }
    out
}
