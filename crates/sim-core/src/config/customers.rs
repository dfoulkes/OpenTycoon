//! `customers.yaml`: population, cohorts, allowances, desires, price sensitivity, residential
//! affordability (§7.2, §8; R50, R52–R55, R59–R61).
//!
//! Floats appear here only for customer scoring inputs (`price_sensitivity`, desire weights,
//! reputation, distance decay). Shares that split populations are fixed-point [`Ratio`]s.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{Pounds, Ratio};
use crate::population::{Class, Sex};

/// Root of `customers.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomersConfig {
    /// Starting population (adults), split by `demographics` for the start year.
    pub initial_population: u64,
    /// Visitor split (R59).
    pub visitors: VisitorSettings,
    /// Adult age bands (R60), youngest first.
    #[schemars(length(min = 1))]
    pub age_bands: Vec<AgeBandSettings>,
    /// Historical births per woman, keyed by step year (R61).
    pub births_per_woman: BTreeMap<String, Ratio>,
    /// Childbearing span in years, used to turn births per woman into births per step (R61).
    #[schemars(range(min = 1, max = 60))]
    pub childbearing_years: u32,
    /// Share of each band dying per 5-year step, keyed by band id.
    pub mortality_per_step: BTreeMap<String, Ratio>,
    /// Migrants added to each morning's arrivals.
    pub migration_per_day: u64,
    /// Daily allowance (R55).
    pub allowance: AllowanceSettings,
    /// Shopping-list weights per segment (R55).
    pub desires: Vec<DesireRule>,
    /// Population mix by era, keyed by year.
    pub demographics: BTreeMap<String, Demographics>,
    /// `k_class` price sensitivity (§8.3).
    pub price_sensitivity: ClassF64,
    /// Shop reputation (§8.3).
    pub reputation: ReputationSettings,
    /// How fast a shop's pull falls with walking distance.
    pub distance_decay: DistanceDecay,
    /// Apartment affordability (R50, R52, R54).
    pub residential: ResidentialSettings,
}

/// Visitor split (R59).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VisitorSettings {
    /// Share of visitors who are commuters (default 0.7); the rest are tourists.
    pub commuter_share: Ratio,
}

/// An adult age band (R60).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgeBandSettings {
    /// Band id, e.g. `"18-29"`.
    pub id: String,
    /// Youngest age in the band.
    #[schemars(range(min = 0, max = 150))]
    pub min_age: u32,
    /// Oldest age in the band; absent for the open-ended top band.
    #[schemars(range(min = 0, max = 150))]
    pub max_age: Option<u32>,
}

/// Daily allowance (R55).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AllowanceSettings {
    /// Base allowance per class.
    pub base_by_class: ClassPounds,
    /// Multiplier per age band id.
    pub age_multiplier: BTreeMap<String, Ratio>,
    /// Multiplier per sex.
    pub sex_multiplier: SexRatios,
}

/// Money per class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassPounds {
    /// Working class.
    pub working: Pounds,
    /// Middle class.
    pub middle: Pounds,
    /// Upper class.
    pub upper: Pounds,
}

/// A float per class (scoring inputs only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassF64 {
    /// Working class.
    #[schemars(range(min = 0.0, max = 100.0))]
    pub working: f64,
    /// Middle class.
    #[schemars(range(min = 0.0, max = 100.0))]
    pub middle: f64,
    /// Upper class.
    #[schemars(range(min = 0.0, max = 100.0))]
    pub upper: f64,
}

/// A ratio per sex.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SexRatios {
    /// Female.
    pub female: Ratio,
    /// Male.
    pub male: Ratio,
}

/// Shopping-list weights for a segment. Unset match fields match everyone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesireRule {
    /// Which customers this rule applies to.
    #[serde(rename = "match")]
    pub matcher: DesireMatch,
    /// `P(good)` weight per good id.
    pub goods: BTreeMap<String, f64>,
}

/// Segment selector for a [`DesireRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesireMatch {
    /// Class, if restricted.
    pub class: Option<Class>,
    /// Age band id, if restricted.
    pub age: Option<String>,
    /// Sex, if restricted.
    pub sex: Option<Sex>,
}

/// Population mix for an era. Each list must sum to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Demographics {
    /// Working / middle / upper shares.
    pub class: [Ratio; 3],
    /// Share per age band, in `age_bands` order.
    pub age: Vec<Ratio>,
    /// Female / male shares.
    pub sex: [Ratio; 2],
}

/// Shop reputation (§8.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReputationSettings {
    /// Reputation of a new shop, 0–1.
    #[schemars(range(min = 0.0, max = 1.0))]
    pub initial: f64,
    /// Drop per lost sale, 0–1.
    #[schemars(range(min = 0.0, max = 1.0))]
    pub lost_sale_penalty: f64,
}

/// Distance decay (§8.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DistanceDecay {
    /// Exponential decay per block walked.
    #[schemars(range(min = 0.0, max = 100.0))]
    pub per_block: f64,
}

/// Apartment affordability (§7.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResidentialSettings {
    /// Units of volume per flat (R50, default 4).
    #[schemars(range(min = 1, max = 10000))]
    pub flat_size: u32,
    /// Comfortable rent share of allowance (R52, default 0.2).
    pub comfortable_share: Ratio,
    /// Stretch rent share for desirable flats (R52, default 0.5).
    pub stretch_share: Ratio,
    /// Minimum in-game years between moves (R54, default 10).
    #[schemars(range(min = 0, max = 500))]
    pub move_interval_years: u32,
}
