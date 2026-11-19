//! `auctions.yaml`: countdown, turn timer, increments (§5.1).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::Pounds;

/// Root of `auctions.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuctionsConfig {
    /// Real seconds between the opening bid and the start of bidding (default 120).
    #[schemars(range(min = 1, max = 3600))]
    pub countdown_real_seconds: u32,
    /// Real seconds per bidding turn; running out counts as a pass (default 10).
    #[schemars(range(min = 1, max = 600))]
    pub turn_real_seconds: u32,
    /// Allowed raises (default £50 / £100 / £500 / £1,000).
    #[schemars(length(min = 1))]
    pub increments: Vec<Pounds>,
    /// Auctions scheduled or running at once across the game (default 1).
    #[schemars(range(min = 1, max = 1))]
    pub max_concurrent: u32,
}
