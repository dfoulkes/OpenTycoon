//! Businesses: buildings, business types, prices, tomorrow's orders, stock, sales, the
//! midnight write-off (§7.1, §7.4; R1, R2, R10, R13, R35, R41, R57, R58).
//!
//! Owner boundary: what is built where and how each shop trades. Money moves only through
//! [`crate::economy`]: this module computes what is owed or earned (build cost, sale revenue,
//! running costs) and the world applies it; economy's settlement raises the 06:00 bills.
//!
//! - [`placement`]: building rules (type, footprint, frontage, overlap, height; R13, R57, R58).
//! - [`trading`]: the player's levers and the sales API customers call (R1, R13).
//! - [`daily`]: the midnight write-off and order lock, and the 06:00 delivery (R1, R2).
//! - [`costs`]: the volume formulas and the daily running cost (R10, R35, R41).
//!
//! How the world drives it (docs/architecture.md tick order):
//!
//! | Phase | Call |
//! |---|---|
//! | 2 Commands | [`Businesses::build`], [`Businesses::set_price`], [`Businesses::set_order`], [`Businesses::set_open`] |
//! | 3 Midnight | net worth first (R31), then [`write_off_stock`] (R1), then [`Businesses::lock_orders`] (R2) |
//! | 5 Settlement | economy bills rent, utilities and [`Business::tomorrow_order`]; then [`Businesses::deliver_orders`] puts exactly that order on the shelves (R2) |
//! | 8 Customers | [`Businesses::try_sell`]; the caller moves the returned revenue through the ledger |

pub mod costs;
pub mod daily;
pub mod formula;
pub mod placement;
pub mod trading;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::economy::AccountId;
use crate::land::{BlockId, Footprint, volume};
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};

pub use costs::RunningCost;
pub use daily::{Delivery, WriteOff, city_defaults, write_off_stock};
pub use placement::{BuildError, BuildRequest, PlacementError};
pub use trading::{LeverError, LostSale};

/// When a business trades (§2.2, §7.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Hours {
    /// Day trade (R1).
    Day,
    /// Night trade (R1).
    Night,
    /// Hotel rooms, 23:30–03:00 (R30).
    Hotel,
    /// Never trades (apartments).
    None,
}

/// Fit-out quality (§7.1). Post-v1 except as a stored field and its build-cost multiplier.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FitOutLevel {
    /// Low.
    Low,
    /// Medium.
    #[default]
    Medium,
    /// High.
    High,
}

/// A stable business id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct BusinessId(pub u32);

/// One building, which is one business of one type (R13).
///
/// Fields are public for reading (net worth, customers, the client). Change them through the
/// [`Businesses`] methods, which keep the invariants: stock is never negative and never above
/// [`Business::capacity_per_good`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Business {
    /// Type id from `businesses.yaml`.
    pub type_id: String,
    /// Block it stands on.
    pub block: BlockId,
    /// Footprint in the lot.
    pub footprint: Footprint,
    /// Height in units (≤ the type's `max_height`, R58).
    pub height: u32,
    /// Owner.
    pub owner: AccountId,
    /// Fit-out.
    pub fit_out: FitOutLevel,
    /// Build cost paid: its property value, forever (R41).
    pub build_cost: Money,
    /// When it trades, copied from its type.
    pub hours: Hours,
    /// Units of stock each good may hold: `volume × stock_density ÷ goods` (R10, §7.1).
    pub capacity_per_good: i64,
    /// Price per good as a share of `ref_price`. Its keys are the goods the business sells.
    pub prices: BTreeMap<String, Rate>,
    /// Stock on hand per good.
    pub stock: BTreeMap<String, i64>,
    /// The order the player sets during the day (the "tomorrow's order" lever). A standing
    /// order: it repeats every day until changed, and locks into
    /// [`Business::tomorrow_order`] at each midnight (R2).
    pub standing_order: BTreeMap<String, i64>,
    /// The order locked at the last midnight: billed by economy's 06:00 settlement and
    /// delivered into stock at the same 06:00, unit for unit (R2).
    pub tomorrow_order: BTreeMap<String, i64>,
    /// Open/closed toggle (§7.1). A closed shop sells nothing even inside its hours.
    pub open: bool,
    /// Revenue since the last midnight.
    pub takings_today: Money,
    /// Units sold per good since the last midnight.
    pub sold_today: BTreeMap<String, i64>,
    /// Lost sales (shopper found too little stock) since the last midnight (§8.3).
    pub lost_sales_today: u32,
}

impl Business {
    /// Building volume `h × w × d` (R10).
    pub fn volume(&self) -> i64 {
        volume(self.height, self.footprint)
    }

    /// True if the business sells `good`.
    pub fn sells(&self, good: &str) -> bool {
        self.prices.contains_key(good)
    }

    /// Units of `good` on hand.
    pub fn stock_of(&self, good: &str) -> i64 {
        self.stock.get(good).copied().unwrap_or(0)
    }
}

/// Every business, by id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Businesses {
    /// Businesses in id order.
    pub by_id: BTreeMap<BusinessId, Business>,
    /// Next id to hand out.
    pub next_id: u32,
}

impl Businesses {
    /// A business by id.
    pub fn get(&self, id: BusinessId) -> Option<&Business> {
        self.by_id.get(&id)
    }

    /// Businesses standing on `block`, in id order.
    pub fn on_block(&self, block: BlockId) -> impl Iterator<Item = (BusinessId, &Business)> {
        self.by_id
            .iter()
            .filter(move |(_, b)| b.block == block)
            .map(|(id, b)| (*id, b))
    }

    /// Businesses owned by `owner`, in id order.
    pub fn owned_by(&self, owner: AccountId) -> impl Iterator<Item = (BusinessId, &Business)> {
        self.by_id
            .iter()
            .filter(move |(_, b)| b.owner == owner)
            .map(|(id, b)| (*id, b))
    }
}

/// `build_cost = volume × build_base_rate × desirability_mult × fit_out_mult` (§5.3, §7.1).
pub fn build_cost(
    height: u32,
    footprint: Footprint,
    build_base_rate: Money,
    desirability_mult: Rate,
    fit_out_mult: Rate,
) -> Money {
    let base = build_base_rate.saturating_mul(volume(height, footprint));
    fit_out_mult.apply(desirability_mult.apply(base))
}

/// `stock_capacity = volume × stock_density`, rounded down to whole units (§5.3, R10).
pub fn stock_capacity(height: u32, footprint: Footprint, stock_density: Rate) -> i64 {
    let scaled = i128::from(volume(height, footprint)) * i128::from(stock_density.bp());
    i64::try_from(scaled / i128::from(BASIS_POINTS_PER_UNIT)).unwrap_or(i64::MAX)
}

/// Capacity for each good when a store sells `goods` goods: they split it evenly, rounding
/// down (§7.1 "multi-good stores split their capacity").
pub fn capacity_per_good(capacity: i64, goods: usize) -> i64 {
    match i64::try_from(goods) {
        Ok(n) if n > 0 => capacity / n,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_and_cost_scale_with_building_volume() {
        let fp = Footprint {
            x: 0,
            y: 0,
            w: 3,
            d: 3,
        };
        assert_eq!(stock_capacity(10, fp, Rate::from_bp(5_000)), 45);
        let cost = build_cost(
            10,
            fp,
            Money::from_pounds(10),
            Rate::from_bp(18_000),
            Rate::ONE,
        );
        assert_eq!(cost, Money::from_pounds(1_620));
    }

    #[test]
    fn multi_good_stores_split_capacity_rounding_down() {
        assert_eq!(capacity_per_good(45, 1), 45);
        assert_eq!(capacity_per_good(45, 2), 22);
        assert_eq!(capacity_per_good(45, 0), 0);
    }
}
