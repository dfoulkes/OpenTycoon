//! Population: the cohort model, births, ageing, deaths, migration, and the 05:30 arrivals
//! (§8.1; R6, R53, R59, R60, R61).
//!
//! Owner boundary: *how many* people of each kind exist and arrive. Individual walking agents
//! are [`crate::customers`]; who gets a flat is residential (post-v1, §7.2).
//!
//! Status: types only (M0). Cohort maths lands in M1.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Social class (§8.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// Working class.
    Working,
    /// Middle class.
    Middle,
    /// Upper class.
    Upper,
}

/// Sex, for the cohort model and allowances (R55, R61).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    /// Female.
    Female,
    /// Male.
    Male,
}

/// Index into `customers.yaml` `age_bands` (R60).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct AgeBand(pub u8);

/// A cohort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cohort {
    /// Age band.
    pub band: AgeBand,
    /// Sex.
    pub sex: Sex,
    /// Class.
    pub class: Class,
}

/// Counts per cohort. Children are a number, not agents (R61).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Population {
    /// Adults per cohort.
    pub adults: BTreeMap<Cohort, u64>,
    /// Children under 18, tracked as a count.
    pub children: u64,
    /// Adults living in apartments (feeds `tourist_pool`, R27).
    pub housed: u64,
    /// Adults arriving at the next 05:30 (R53).
    pub pending_arrivals: u64,
}

impl Population {
    /// Total adults.
    pub fn total_adults(&self) -> u64 {
        self.adults.values().sum()
    }

    /// `tourist_pool = total − housed` before the commuter split (R27, R59).
    pub fn visitors(&self) -> u64 {
        self.total_adults().saturating_sub(self.housed)
    }
}
