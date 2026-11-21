//! `economy.yaml`: every economy weight, the levers the balance study varies (R71).
//!
//! Adopting an economic model means swapping this one file. A number that sets the level of
//! the economy's money flows (prices, rates, credit, how many people and how much they spend,
//! how much they want in each decade, the residential market's rules) lives here; a description
//! of the world (what exists, where and when, and the relative pattern of who wants what) stays
//! in its own file. The line is drawn in docs/economy.md "Economy weights (R71)".

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::customers::{ClassF64, ClassPounds};
use super::types::{Pounds, Ratio};

/// Root of `economy.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EconomyConfig {
    /// Land rent, leases and how land counts towards net worth (§3, §5).
    pub land: LandEconomy,
    /// What building costs (§5.3).
    pub building: BuildingEconomy,
    /// Utility bills (§5.3, §6).
    pub utilities: UtilityEconomy,
    /// Players' money: starting cash, the overdraft and selling up (§2.3, §3).
    pub money: MoneyEconomy,
    /// How many people there are and how they spend (§8).
    pub population: PopulationEconomy,
    /// A multiplier on what shoppers want, by decade (§8.2, R64): `"1930s": 1.0`. Each applies
    /// from its decade's first year until the next decade named; the first must be at or before
    /// the start of every game. `P(good)` is multiplied after the segment rules and capped at 1.
    pub demand_multipliers: BTreeMap<String, f64>,
    /// The residential market (§7.2; R50–R52, R54).
    pub residential: ResidentialSettings,
}

/// Land (§3, §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LandEconomy {
    /// Land rent per unit of building volume per day, before the tier multiplier (§5.3, R10).
    pub base_rate: Pounds,
    /// Lease length in in-game years (§5.2, default 30).
    #[schemars(range(min = 1, max = 500))]
    pub lease_term_years: u32,
    /// How a held lease's land is valued for net worth (§3; docs/economy.md "Land value").
    /// Default `book_decay`, the spec's `last auction price × years left / term`.
    pub value_method: LandValueMethod,
    /// Share of every property's value credited to the lot's leaseholder (R39, default 0.2).
    pub lot_property_uplift: Ratio,
}

/// How a held lease's land counts towards net worth (§3). The spec's formula is
/// `book_decay`; the others are balance-study candidates (docs/economy.md "Land value").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LandValueMethod {
    /// `last auction price × remaining lease years / term` (§3, the default).
    #[default]
    BookDecay,
    /// `current land base price × remaining lease years / term`: the block's price today
    /// (moved by desirability shifts and land-value events such as the 2008 crash).
    Market,
    /// `current land base price` while the lease runs, nothing once it has run out.
    MarketNoDecay,
}

/// Building (§5.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildingEconomy {
    /// Build cost per unit of building volume, before the tier and fit-out multipliers (§5.3).
    pub base_rate: Pounds,
}

/// Utility bills (§5.3, §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UtilityEconomy {
    /// Bill per unit of building volume per day, before the business's utility factor (§5.3).
    pub base_rates: UtilityRates,
    /// The telephone exchange's base rate once it becomes the internet (R11; the year is
    /// `utilities.yaml` `internet.upgrade_year`).
    pub internet_base_rate: Pounds,
}

/// A rate per utility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UtilityRates {
    /// Electricity.
    pub electricity: Pounds,
    /// Gas.
    pub gas: Pounds,
    /// Water.
    pub water: Pounds,
    /// Telephone exchange, later the internet.
    pub telecoms: Pounds,
}

/// Players' money (§2.3, §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoneyEconomy {
    /// Cash each player starts with (R62, default £7,000), on top of the overdraft.
    pub starting_cash: Pounds,
    /// The overdraft (R7, R32).
    pub overdraft: OverdraftSettings,
    /// Instant sale back to the city, as a share of true value (R5, default 0.5).
    pub sell_to_city_ratio: Ratio,
}

/// Overdraft (R7, R32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OverdraftSettings {
    /// Credit limit at the start (default £10,000).
    pub start_limit: Pounds,
    /// Flat increase per 5-year step (default £2,500).
    pub increase_per_step: Pounds,
    /// Daily interest on overdrawn balances (default 0: R32 says none).
    pub interest_rate: Ratio,
}

/// How many people there are and how they spend (§8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PopulationEconomy {
    /// Multiplier on every era's births per woman (`customers.yaml` `births_per_woman`, the
    /// historical table; R61). 1 = history.
    pub births_multiplier: Ratio,
    /// Migrants added to each morning's arrivals.
    pub migration_per_day: u64,
    /// Daily allowance per class before the age and sex multipliers (`customers.yaml`
    /// `allowance`; §8.2, R55).
    pub allowance_by_class: ClassPounds,
    /// `k_class` price sensitivity (§8.3).
    pub price_sensitivity: ClassF64,
}

/// The residential market (§7.2).
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
    /// A flat whose desirability (class affinity × fit-out appeal) is at least this justifies
    /// stretching to `stretch_share` (R52).
    #[schemars(range(min = 0.0, max = 100.0))]
    pub stretch_min_desirability: f64,
    /// Minimum in-game years between moves (R54, default 10).
    #[schemars(range(min = 0, max = 500))]
    pub move_interval_years: u32,
    /// Rent per flat per day until the owner sets one: the city's default (R4, R51).
    pub default_rent: Pounds,
    /// The reference rent for `price_factor` when people weigh flats (R51).
    pub reference_rent: Pounds,
    /// Study lever, off by default (`null`): the most any flat's rent can be per day. An
    /// owner may set more, but residents see and pay `min(rent, cap)` (docs/economy.md
    /// "Residential levers").
    pub max_rent_per_flat: Option<Pounds>,
    /// Study lever, off by default (0): the share of residents' rent paid to the city as tax
    /// at the 06:00 settlement; the owner gets the rest (docs/economy.md "Residential levers").
    pub rent_tax: Ratio,
    /// Study lever, off by default (0): the share of the city's adults who would rather
    /// commute than live in a flat, even one they can afford. The 05:30 house-hunt houses no
    /// more than `⌊adults × (1 − commute_preference)⌋` (docs/economy.md "Residential levers").
    pub commute_preference: Ratio,
}

/// The first year of a decade key such as `"1930s"`, or `None` if the key is not a decade.
pub fn decade_start(key: &str) -> Option<i32> {
    let digits = key.strip_suffix('s')?;
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = digits.parse().ok()?;
    (year % 10 == 0).then_some(year)
}

impl EconomyConfig {
    /// The demand multiplier for `year`: the latest decade named at or before it, or 1 before
    /// the first (validation requires the first to cover the start of every game).
    pub fn demand_multiplier(&self, year: i32) -> f64 {
        self.demand_multipliers
            .iter()
            .filter_map(|(k, m)| decade_start(k).map(|start| (start, *m)))
            .filter(|(start, _)| *start <= year)
            .max_by_key(|(start, _)| *start)
            .map_or(1.0, |(_, m)| m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decade_keys_are_four_digit_decades_with_an_s() {
        assert_eq!(decade_start("1930s"), Some(1930));
        assert_eq!(decade_start("2050s"), Some(2050));
        for bad in ["1930", "1935s", "193s", "19300s", "abcds", "", "s", "+930s"] {
            assert_eq!(decade_start(bad), None, "{bad}");
        }
    }

    fn with(table: &[(&str, f64)]) -> EconomyConfig {
        let text = r"
land: { base_rate: 0.05, lease_term_years: 30, value_method: book_decay, lot_property_uplift: 0.2 }
building: { base_rate: 10 }
utilities:
  base_rates: { electricity: 0.02, gas: 0.01, water: 0.01, telecoms: 0.01 }
  internet_base_rate: 0.02
money:
  starting_cash: 7000
  overdraft: { start_limit: 10000, increase_per_step: 2500, interest_rate: 0 }
  sell_to_city_ratio: 0.5
population:
  births_multiplier: 1.0
  migration_per_day: 50
  allowance_by_class: { working: 20, middle: 45, upper: 120 }
  price_sensitivity: { working: 3.0, middle: 1.5, upper: 0.5 }
demand_multipliers: {}
residential:
  flat_size: 4
  comfortable_share: 0.2
  stretch_share: 0.5
  stretch_min_desirability: 1.0
  move_interval_years: 10
  default_rent: 3
  reference_rent: 3
  max_rent_per_flat: null
  rent_tax: 0
  commute_preference: 0
";
        let mut e: EconomyConfig = serde_yaml_ng::from_str(text).unwrap();
        e.demand_multipliers = table.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
        e
    }

    #[test]
    fn a_decade_applies_until_the_next_one_named() {
        let e = with(&[("1930s", 1.2), ("1950s", 0.8), ("2030s", 1.5)]);
        assert_eq!(e.demand_multiplier(1930).to_bits(), 1.2_f64.to_bits());
        assert_eq!(e.demand_multiplier(1945).to_bits(), 1.2_f64.to_bits());
        assert_eq!(e.demand_multiplier(1950).to_bits(), 0.8_f64.to_bits());
        assert_eq!(e.demand_multiplier(2025).to_bits(), 0.8_f64.to_bits());
        assert_eq!(e.demand_multiplier(2050).to_bits(), 1.5_f64.to_bits());
        assert_eq!(e.demand_multiplier(1900).to_bits(), 1.0_f64.to_bits());
    }
}
