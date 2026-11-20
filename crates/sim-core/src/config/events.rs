//! `events.yaml`: historical events, random events, elections (§9, R12, R16).
//!
//! Effect kinds are a closed set so a typo fails loudly. The events module owner extends
//! [`Effect`] as mechanics land.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::Ratio;
use crate::land::Tier;

/// Root of `events.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsConfig {
    /// Fixed history. Each fires on the year step covering its year (§9).
    pub historical: Vec<HistoricalEvent>,
    /// Per-building daily random events.
    pub random: Vec<RandomEvent>,
    /// General elections (§9.1).
    pub elections: ElectionSettings,
}

/// A historical event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoricalEvent {
    /// Stable id.
    pub id: String,
    /// Historical year; fires on the step that covers it.
    #[schemars(range(min = 1800, max = 2500))]
    pub year: i32,
    /// Newspaper headline.
    pub headline: String,
    /// What it does.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// An event effect. Tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    /// Scale demand for a target (a good id, a tag such as `luxury`, `physical_retail`, `online`).
    DemandMultiplier {
        /// What demand is scaled.
        target: String,
        /// Multiplier.
        factor: Ratio,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Move blocks to a new desirability tier.
    DesirabilityChange {
        /// Block ids.
        blocks: Vec<String>,
        /// New tier.
        tier: Tier,
    },
    /// Destroy buildings at random, with compensation.
    DestroyBuildings {
        /// Block ids at risk.
        blocks: Vec<String>,
        /// Chance per building.
        chance: Ratio,
        /// Compensation as a share of property value.
        compensation: Ratio,
    },
    /// Switch the currency display to decimal.
    DecimalCurrency,
    /// Headline only; no mechanical effect yet.
    Flavour,
}

/// A random per-building event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RandomEvent {
    /// Stable id.
    pub id: String,
    /// Chance per building per day.
    pub daily_chance: Ratio,
}

/// Elections (R12, §9.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElectionSettings {
    /// Whether every year step holds a general election.
    pub enabled: bool,
    /// Win weights per step year. Steps without an entry use the latest earlier one.
    pub weights: BTreeMap<String, PartyWeights>,
}

/// Election weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartyWeights {
    /// Labour (nationalises utilities).
    pub labour: Ratio,
    /// Conservative (privatises utilities).
    pub conservative: Ratio,
}
