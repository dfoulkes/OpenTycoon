//! The businesses' part of midnight and the 06:00 settlement (§2.2; R1, R2).
//!
//! - **Midnight**, after net worth has been scored (R31): [`write_off_stock`] bins unsold stock
//!   (R1); [`Businesses::lock_orders`] locks each standing order into
//!   [`super::Business::tomorrow_order`] and resets the day's tallies (R2).
//!   [`Businesses::midnight`] does both, in that order.
//! - **06:00**: economy's settlement bills `tomorrow_order × purchase_price`
//!   (`economy::settlement::stock_order_bill`), then [`Businesses::deliver_orders`] puts exactly
//!   that order on the shelves. Billed and delivered are the same map, so they can't diverge.
//!   Orders are capped at capacity when set and when locked, and shelves are empty after the
//!   write-off, so a delivery never overfills a building (R10).
//!
//! Orders are standing orders: the order a player sets repeats each day until changed.

use serde::{Deserialize, Serialize};

use super::{Business, BusinessId, Businesses, capacity_per_good, stock_capacity};
use crate::config::BusinessesConfig;
use crate::config::businesses::BusinessType;
use crate::economy::AccountId;
use crate::money::{Money, Rate};

/// Stock binned at midnight (R1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WriteOff {
    /// Business.
    pub business: BusinessId,
    /// Its owner, who bears the loss.
    pub owner: AccountId,
    /// Good.
    pub good: String,
    /// Units binned.
    pub units: i64,
}

impl WriteOff {
    /// What the binned units cost at purchase price.
    pub fn value(&self, rules: &BusinessesConfig) -> Money {
        purchase_price(rules, &self.good).saturating_mul(self.units)
    }
}

/// Stock put on a shelf at 06:00 (R2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delivery {
    /// Business.
    pub business: BusinessId,
    /// Good.
    pub good: String,
    /// Units delivered: exactly the units billed.
    pub units: i64,
}

fn purchase_price(rules: &BusinessesConfig, good: &str) -> Money {
    rules
        .goods
        .get(good)
        .map_or(Money::ZERO, |g| g.purchase_price.money())
}

/// Bins every unit left on every shelf (R1) and reports what went, in business then good
/// order. The world calls this at midnight after net worth has been scored (R31).
pub fn write_off_stock(businesses: &mut Businesses) -> Vec<WriteOff> {
    let mut out = Vec::new();
    for (id, b) in &mut businesses.by_id {
        for (good, units) in &mut b.stock {
            if *units > 0 {
                out.push(WriteOff {
                    business: *id,
                    owner: b.owner,
                    good: good.clone(),
                    units: *units,
                });
            }
            *units = 0;
        }
    }
    out
}

/// Default settings for a city-run business (R4): reference prices for every good it sells,
/// a standing order for its full capacity split evenly across its goods, locked for the next
/// 06:00 too, and open for trade. Owned by businesses; economy calls it on
/// bankruptcy hand-over.
pub fn city_defaults(business: &mut Business, business_type: &BusinessType) {
    let capacity = stock_capacity(
        business.height,
        business.footprint,
        business_type.stock_density.rate(),
    );
    let per_good = capacity_per_good(capacity, business_type.goods.len());
    business.capacity_per_good = per_good;
    business.hours = business_type.hours;
    business.prices.clear();
    business.standing_order.clear();
    business.tomorrow_order.clear();
    for good in &business_type.goods {
        business.prices.insert(good.clone(), Rate::ONE);
        business.standing_order.insert(good.clone(), per_good);
        business.tomorrow_order.insert(good.clone(), per_good);
        business.stock.entry(good.clone()).or_insert(0);
    }
    business.open = true;
}

impl Businesses {
    /// Stock `owner` holds, valued at purchase price (§3).
    pub fn stock_value(&self, owner: AccountId, rules: &BusinessesConfig) -> Money {
        self.owned_by(owner)
            .flat_map(|(_, b)| b.stock.iter())
            .map(|(good, units)| purchase_price(rules, good).saturating_mul(*units))
            .sum()
    }

    /// Locks each standing order into tomorrow's order for the next 06:00, capped at what the
    /// building holds, and resets the day's takings, units sold and lost sales (R2).
    pub fn lock_orders(&mut self) {
        for b in self.by_id.values_mut() {
            let cap = b.capacity_per_good;
            b.tomorrow_order = b
                .standing_order
                .iter()
                .map(|(good, units)| (good.clone(), (*units).clamp(0, cap)))
                .collect();
            b.takings_today = Money::ZERO;
            b.lost_sales_today = 0;
            b.sold_today.values_mut().for_each(|n| *n = 0);
        }
    }

    /// The businesses' midnight, after net worth (R31): write-off (R1), then order lock (R2).
    /// Returns the write-offs for the front page and the client.
    pub fn midnight(&mut self) -> Vec<WriteOff> {
        let written_off = write_off_stock(self);
        self.lock_orders();
        written_off
    }

    /// 06:00, after economy has billed it: puts exactly the locked order on the shelves (R2).
    /// Nothing else ever adds stock, so there is no mid-day restock.
    pub fn deliver_orders(&mut self) -> Vec<Delivery> {
        let mut out = Vec::new();
        for (id, b) in &mut self.by_id {
            for (good, units) in &b.tomorrow_order {
                if *units > 0 {
                    *b.stock.entry(good.clone()).or_insert(0) += *units;
                    out.push(Delivery {
                        business: *id,
                        good: good.clone(),
                        units: *units,
                    });
                }
            }
        }
        out
    }
}
