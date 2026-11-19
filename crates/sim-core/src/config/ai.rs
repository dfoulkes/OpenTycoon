//! `ai.yaml`: AI personalities and their weights (§10).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::Ratio;

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
