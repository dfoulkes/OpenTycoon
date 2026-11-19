//! `map.yaml`: grid, blocks, districts, tiers, colour sets, land prices, tube stations (§4, §5).
//!
//! A different city is a different `map.yaml` (§10b). The shipped default is the v1 grey-box
//! 3×3 map: one set per row, one tier per row (§10c, §11).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{Pounds, Ratio};
use crate::land::{BlockKind, ColourBand, Tier};

/// Root of `map.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapConfig {
    /// Grid size in blocks.
    pub grid: GridSettings,
    /// Per-tier multipliers (§5.3).
    pub tiers: TierTable,
    /// Land rent per unit of building volume per day, before the tier multiplier (§5.3, R10).
    pub land_base_rate: Pounds,
    /// Colour sets (R14, R26), keyed by set id.
    pub sets: BTreeMap<String, SetSettings>,
    /// Every block on the grid, exactly one per cell.
    pub blocks: Vec<BlockSettings>,
    /// Tube stations: spawn and despawn points (§4.3, R25).
    pub tube_stations: Vec<TubeStation>,
}

/// Grid size in blocks (§4.1: 9×9 for the full game).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSettings {
    /// Columns.
    #[schemars(range(min = 1, max = 32))]
    pub width: u32,
    /// Rows.
    #[schemars(range(min = 1, max = 32))]
    pub height: u32,
}

/// Settings per desirability tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TierTable {
    /// Low tier.
    pub low: TierSettings,
    /// Medium tier.
    pub medium: TierSettings,
    /// High tier.
    pub high: TierSettings,
}

impl TierTable {
    /// Settings for one tier.
    pub fn get(&self, tier: Tier) -> &TierSettings {
        match tier {
            Tier::Low => &self.low,
            Tier::Medium => &self.medium,
            Tier::High => &self.high,
        }
    }
}

/// Settings for one desirability tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TierSettings {
    /// `desirability_mult(tier)` in the rent and build-cost formulas (§5.3; 0.6 / 1.0 / 1.8).
    pub desirability_mult: Ratio,
}

/// A colour set (R14, R26).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSettings {
    /// Display name.
    pub name: String,
    /// Colour band, cheapest to dearest.
    pub band: ColourBand,
}

/// One block (lot) on the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockSettings {
    /// Stable id, `snake_case`.
    pub id: String,
    /// District name shown to players.
    pub name: String,
    /// Row, 0-based from the top.
    pub row: u32,
    /// Column, 0-based from the left.
    pub col: u32,
    /// Desirability tier.
    pub tier: Tier,
    /// What the block is.
    pub kind: BlockKind,
    /// Colour set id, for buildable blocks.
    pub set: Option<String>,
    /// Minimum opening bid at auction (R23). Required for buildable blocks.
    pub land_base_price: Option<Pounds>,
}

/// A tube station sitting on a block (§4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TubeStation {
    /// Block id the station sits on.
    pub block: String,
    /// Line name.
    pub line: String,
    /// Year the line opens at this station.
    #[schemars(range(min = 1800, max = 2500))]
    pub opens: i32,
}
