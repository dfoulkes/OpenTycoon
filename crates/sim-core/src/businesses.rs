//! Businesses: buildings, business types, prices, tomorrow's orders, stock, sales, the
//! midnight write-off (§7.1, §7.4; R1, R2, R10, R13, R35, R41, R58).
//!
//! Owner boundary: what is built where and how each shop trades. Placement validity uses
//! [`crate::land::Footprint`]; money moves only through [`crate::economy`].
//!
//! Status: types and cost formulas are real; trading lands in M1.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::economy::AccountId;
use crate::land::{BlockId, Footprint, volume};
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};

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

/// Fit-out quality (§7.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FitOutLevel {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// A stable business id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct BusinessId(pub u32);

/// One building, which is one business (R13).
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
    /// Build cost paid: its property value (R41).
    pub build_cost: Money,
    /// Price per good as a share of `ref_price`.
    pub prices: BTreeMap<String, Rate>,
    /// Stock on hand per good.
    pub stock: BTreeMap<String, i64>,
    /// Tomorrow's order per good, locked at midnight, delivered at 06:00 (R2).
    pub tomorrow_order: BTreeMap<String, i64>,
    /// Open/closed toggle.
    pub open: bool,
}

/// Every business, by id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Businesses {
    /// Businesses in id order.
    pub by_id: BTreeMap<BusinessId, Business>,
    /// Next id to hand out.
    pub next_id: u32,
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
}
