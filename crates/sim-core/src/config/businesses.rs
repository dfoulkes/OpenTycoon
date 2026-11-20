//! `businesses.yaml`: goods, fit-out levels, and every business type (§7, R10, R13, R58).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{Pounds, Ratio};
use crate::businesses::Hours;

/// Root of `businesses.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BusinessesConfig {
    /// Build cost per unit of building volume, before the tier multiplier (§5.3).
    pub build_base_rate: Pounds,
    /// Fit-out levels (§7.1).
    pub fit_out: FitOutTable,
    /// The reference shop the stats formula is built around (docs/economy.md).
    pub reference: ReferenceShop,
    /// Goods catalogue, keyed by good id.
    pub goods: BTreeMap<String, Good>,
    /// Business types, keyed by type id.
    pub types: BTreeMap<String, BusinessType>,
}

/// Fit-out levels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FitOutTable {
    /// Low.
    pub low: FitOut,
    /// Medium.
    pub medium: FitOut,
    /// High.
    pub high: FitOut,
}

/// One fit-out level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FitOut {
    /// Multiplier on build cost.
    pub build_cost_mult: Ratio,
    /// Multiplier on class affinity when customers score the shop (§8.3).
    pub appeal: Ratio,
}

/// The reference shop of the business-stats formula (docs/economy.md "Business stats"): a
/// building of the type's `min_footprint`, `height` high, on a Medium-tier block, selling
/// `sell_through` of its stock at reference prices. Its stock density is set so that this shop
/// covers its rent and utilities and earns its build cost back in `payback_days`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReferenceShop {
    /// Height of the reference building, in units.
    #[schemars(range(min = 1, max = 100))]
    pub height: u32,
    /// Share of its stock a competently run shop sells in a day (the reference footfall).
    /// Above 0 and at most 1.
    pub sell_through: Ratio,
    /// Days for a Medium-tier reference shop to earn back its build cost.
    #[schemars(range(min = 1, max = 1000))]
    pub payback_days: u32,
}

/// How a business reaches its customers (§7.5).
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
pub enum Channel {
    /// A walk-in shop: shoppers walk to its door (§8.3).
    #[default]
    Shop,
    /// An online seller (§7.5, R15, R36): no walk-in trade; its demand model is e-commerce's.
    Online,
    /// Infrastructure (data centre, lockers, farms, hubs): no walk-in trade.
    Infrastructure,
}

/// A good on sale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Good {
    /// Display name.
    pub name: String,
    /// Reference retail price per unit (`ref_price` in §8.3).
    pub ref_price: Pounds,
    /// Wholesale price per unit paid at the 06:00 settlement.
    pub purchase_price: Pounds,
    /// Luxury goods get a mild boost from a higher price (§8.3).
    #[serde(default)]
    pub luxury: bool,
    /// First year anyone wants it; absent means from the start (§7.4).
    #[schemars(range(min = 1800, max = 2500))]
    pub unlock_year: Option<i32>,
    /// Year after which demand for it fades (§7.1); absent means never.
    #[schemars(range(min = 1800, max = 2500))]
    pub obsolete_year: Option<i32>,
}

/// A business type (§7.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BusinessType {
    /// Display name.
    pub name: String,
    /// Minimum `[width, depth]` footprint within the lot.
    pub min_footprint: [u32; 2],
    /// Maximum height in units: `1 ≤ max_height ≤ world.max_height` (R58).
    #[schemars(range(min = 1))]
    pub max_height: u32,
    /// Units of stock per unit of building volume (R10).
    pub stock_density: Ratio,
    /// When it trades.
    pub hours: Hours,
    /// Good ids it sells. Multi-good stores split capacity.
    pub goods: Vec<String>,
    /// Multipliers on the utility base rates.
    pub utility_factor: UtilityFactors,
    /// First year it can be built.
    #[schemars(range(min = 1800, max = 2500))]
    pub unlock_year: i32,
    /// Optional year after which demand fades: shoppers favour other shops (§7.1).
    #[schemars(range(min = 1800, max = 2500))]
    pub obsolete_year: Option<i32>,
    /// Walk-in shop (the default), online seller or infrastructure (§7.5).
    #[serde(default)]
    pub channel: Channel,
}

/// Utility multipliers for a business type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UtilityFactors {
    /// Electricity.
    pub electricity: Ratio,
    /// Gas.
    pub gas: Ratio,
    /// Water.
    pub water: Ratio,
    /// Telecoms / internet.
    pub telecoms: Ratio,
}
