//! `ai.yaml`: AI personalities and their weights (§10).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{ClockTime, Pounds, Ratio};

/// Root of `ai.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    /// Ticks between an AI player's decisions.
    #[schemars(range(min = 1, max = 100_000))]
    pub decision_interval_ticks: u32,
    /// Personality ids seated by default (v1: one Grocer).
    pub opponents: Vec<String>,
    /// Personalities, keyed by id. All share one evaluation function with different weights.
    pub personalities: BTreeMap<String, Personality>,
    /// How the Grocer plays (§10: "opens many cheap Low-tier shops"). Its tier choice comes
    /// from the `grocer` personality's weights; these are its operating rules.
    pub grocer: GrocerConfig,
}

/// The Grocer's tunables (§10). Money is decimal pounds; shares are decimals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrocerConfig {
    /// Tiers whose `grocer` personality weight is at least this are worth bidding on
    /// (`tier_low: 1.0` passes, `tier_high: 0.1` does not). Between 0 and 1.
    pub min_tier_weight: Ratio,
    /// The most it will pay for a lot, as a share of its bid ceiling (`balance + credit
    /// limit`, R7). Above 0, at most 1.
    pub max_bid_share: Ratio,
    /// Cash it keeps in hand: it never spends (land, buildings) below this balance, so the
    /// 06:00 bills don't take it under zero by midnight (R3).
    pub cash_buffer: Pounds,
    /// Day-trade shop types it builds, in rotation (`businesses.yaml` ids).
    pub shop_types: Vec<String>,
    /// Footprint of each shop, `[width, depth]` in lot units.
    pub shop_footprint: [u32; 2],
    /// Height of each shop in units; capped at the type's `max_height` (R58).
    #[schemars(range(min = 1, max = 100))]
    pub shop_height: u32,
    /// Working-class bias: price this far under the reference price (0.05 = 95% of
    /// `ref_price`). At least 0, below 1.
    pub working_class_discount: Ratio,
    /// Random spread either side of its price, from its own RNG stream. Below `1 −
    /// working_class_discount`.
    pub price_jitter: Ratio,
    /// A new shop's first order, as a share of its capacity. Above 0, at most 1.
    pub initial_order_share: Ratio,
    /// Tomorrow's order = today's sales × (1 + this), capped at capacity (R2, R10). At most 1.
    pub order_margin: Ratio,
    /// From this time until midnight, a negative balance makes it sell a shop back to the
    /// city (R5) rather than take a strike (R3). Must be after day trade closes.
    pub rescue_after: ClockTime,
}

/// An AI personality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    /// Display name.
    pub name: String,
    /// One line on how it plays.
    pub description: String,
    /// Evaluation weights.
    pub weights: PersonalityWeights,
}

/// Weights for the shared evaluation function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonalityWeights {
    /// Appetite for land leases.
    pub land: Ratio,
    /// Appetite for retail businesses.
    pub retail: Ratio,
    /// Appetite for residential.
    pub residential: Ratio,
    /// Willingness to bid high and run an overdraft.
    pub risk: Ratio,
    /// Preference for low-tier blocks.
    pub tier_low: Ratio,
    /// Preference for medium-tier blocks.
    pub tier_medium: Ratio,
    /// Preference for high-tier blocks.
    pub tier_high: Ratio,
}
