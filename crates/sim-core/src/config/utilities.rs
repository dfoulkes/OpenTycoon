//! `utilities.yaml`: the internet upgrade, nationalisation and reserve prices (§6, R11, R12, R24).
//! The bill rates are `economy.yaml` `utilities` (R71).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::economy::UtilityRates;
use super::types::Ratio;

/// Root of `utilities.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UtilitiesConfig {
    /// The telephone exchange's upgrade to the internet (R11).
    pub internet: InternetSettings,
    /// Elections and ownership (R12).
    pub nationalisation: NationalisationSettings,
    /// Minimum opening bid for each utility at auction, like a block's land base price (R12,
    /// R23).
    pub reserve_prices: UtilityRates,
}

/// The internet upgrade (R11, R15, R36).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InternetSettings {
    /// Year the telephone exchange becomes the internet (default 1995). From then telecoms
    /// bills use `economy.yaml` `utilities.internet_base_rate`.
    #[schemars(range(min = 1800, max = 2500))]
    pub upgrade_year: i32,
}

/// Nationalisation and privatisation (R12, §9.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NationalisationSettings {
    /// Buy-out price on nationalisation, as a share of book value (default 1.0). Book value is
    /// what the holder paid for the utility at auction: utilities don't wear, like buildings
    /// (R41).
    pub buyout_ratio: Ratio,
}
