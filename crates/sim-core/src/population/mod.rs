//! Population: the cohort model, births, ageing, deaths, migration, the 05:30 arrivals and who
//! lives in which flat (§7.2, §8.1; R6, R50–R54, R56, R59, R60, R61).
//!
//! Owner boundary: *how many* people of each kind exist, who arrives each morning and where
//! residents live. Individual walking agents are [`crate::customers`]. Apartment buildings are
//! [`crate::businesses`] of the type named by `customers.yaml` `residential.building_type`;
//! this module owns their tenants and rents. Money moves only through [`crate::economy`]: this
//! module reports rent owed ([`residential::rent_due`]) and economy settles it at 06:00.
//!
//! The model:
//!
//! - **Adults** are either in the city's **pool** (unhoused: commuters and tourists, counted
//!   per [`Cohort`]) or **residents** (individuals with a home, [`residential::Resident`]).
//! - **Children** are a count per step year, never agents (R61). They join the next 05:30
//!   arrivals once they reach the first adult age (18).
//! - **Pending** arrivals are counted per cohort and house-hunt at 05:30 (R53).
//! - There is no inflation: allowances never depend on the year (R6). Growth comes from the
//!   cohort model and migration.

pub mod cohort;
pub mod residential;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::{Config, CustomersConfig};
use crate::money::{Money, Rate};

pub use cohort::{YearStep, year_step};
pub use residential::{
    ArrivalsReport, FlatListing, RentDue, Resident, ResidentId, Residential, arrivals,
};

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

impl Class {
    /// All classes in config order (working, middle, upper).
    pub const ALL: [Class; 3] = [Class::Working, Class::Middle, Class::Upper];

    /// Index in [`Class::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Class::Working => 0,
            Class::Middle => 1,
            Class::Upper => 2,
        }
    }
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

impl Sex {
    /// Both, in config order (female, male).
    pub const ALL: [Sex; 2] = [Sex::Female, Sex::Male];
}

/// Index into `customers.yaml` `age_bands` (R60), youngest first.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct AgeBand(pub u8);

/// A cohort key: everyone sharing a band, sex and class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cohort {
    /// Age band.
    pub band: AgeBand,
    /// Sex.
    pub sex: Sex,
    /// Class.
    pub class: Class,
}

/// Everyone in the city, plus tomorrow morning's arrivals.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Population {
    /// Unhoused adults per cohort: the visitors who commute or tour (R59, `tourist_pool`).
    pub pool: BTreeMap<Cohort, u64>,
    /// Adults arriving by train at the next 05:30, per cohort (R53).
    pub pending: BTreeMap<Cohort, u64>,
    /// Children under 18, counted per step year (the year the step that bore them ended).
    pub children: BTreeMap<i32, u64>,
    /// Residents and rents (§7.2).
    pub residential: Residential,
}

impl Population {
    /// The starting city: `initial_population` adults, split by the start year's demographics,
    /// arrive on the first morning's train and house-hunt like anyone else.
    pub fn new(rules: &CustomersConfig, start_year: i32) -> Self {
        Population {
            pending: cohort::split_newcomers(rules.initial_population, rules, start_year, None),
            ..Population::default()
        }
    }

    /// Adults in the city: the unhoused pool plus residents (not counting pending arrivals).
    pub fn total_adults(&self) -> u64 {
        self.visitors() + self.housed()
    }

    /// Adults living in apartments.
    pub fn housed(&self) -> u64 {
        self.residential.residents.len() as u64
    }

    /// Unhoused adults: `total − housed`, before the commuter/tourist split (R27, R59).
    pub fn visitors(&self) -> u64 {
        self.pool.values().sum()
    }

    /// Adults waiting for the next 05:30 train.
    pub fn pending_arrivals(&self) -> u64 {
        self.pending.values().sum()
    }

    /// Children (not agents, R61).
    pub fn children(&self) -> u64 {
        self.children.values().sum()
    }

    /// Everyone: adults, children and pending arrivals.
    pub fn everyone(&self) -> u64 {
        self.total_adults() + self.children() + self.pending_arrivals()
    }

    /// Adults in a cohort, housed or not.
    pub fn adults_in(&self, cohort: Cohort) -> u64 {
        let housed = self
            .residential
            .residents
            .values()
            .filter(|r| r.cohort == cohort)
            .count() as u64;
        self.pool.get(&cohort).copied().unwrap_or(0) + housed
    }
}

/// Tourists in the city: the share of the unhoused pool (`total − housed`, R27) who are not
/// commuters (R59), split exactly per cohort as the shoppers are. Only they want a Hotel.
pub fn tourists(pop: &Population, rules: &CustomersConfig) -> u64 {
    let share = rules.visitors.commuter_share.rate().bp().unsigned_abs();
    let unit = crate::money::BASIS_POINTS_PER_UNIT.unsigned_abs();
    pop.pool
        .values()
        .map(|n| n - cohort::split_by_weights(*n, &[share, unit.saturating_sub(share)])[0])
        .sum()
}

/// The band id (`"18-29"`) for a band index, or `""` if out of range.
pub fn band_id(rules: &CustomersConfig, band: AgeBand) -> &str {
    rules
        .age_bands
        .get(usize::from(band.0))
        .map_or("", |b| b.id.as_str())
}

/// The band a person of `age` falls in, or `None` for a child (R60).
pub fn band_for_age(rules: &CustomersConfig, age: u32) -> Option<AgeBand> {
    rules
        .age_bands
        .iter()
        .position(|b| age >= b.min_age && b.max_age.is_none_or(|max| age <= max))
        .and_then(|i| u8::try_from(i).ok())
        .map(AgeBand)
}

/// Daily allowance before rent: `base(class) × age_multiplier(band) × sex_multiplier(sex)`
/// (§8.2, R55), the base from `economy.yaml` `population.allowance_by_class` (R71), the
/// multipliers from `customers.yaml` `allowance`. It never depends on the year: no inflation
/// (R6).
pub fn allowance(config: &Config, cohort: Cohort) -> Money {
    let rules = &config.customers;
    let a = &rules.allowance;
    let by_class = &config.economy.population.allowance_by_class;
    let base = match cohort.class {
        Class::Working => by_class.working,
        Class::Middle => by_class.middle,
        Class::Upper => by_class.upper,
    }
    .money();
    let age = a
        .age_multiplier
        .get(band_id(rules, cohort.band))
        .map_or(Rate::ONE, |r| r.rate());
    let sex = match cohort.sex {
        Sex::Female => a.sex_multiplier.female,
        Sex::Male => a.sex_multiplier.male,
    }
    .rate();
    sex.apply(age.apply(base))
}

/// Every adult's daily allowance, summed: the new money minted each morning (§8.2, R63).
/// Residents' rent is owed out of it ([`residential::rent_due`]).
pub fn daily_allowances(pop: &Population, config: &Config) -> Money {
    let pool: Money = pop
        .pool
        .iter()
        .map(|(c, n)| allowance(config, *c).saturating_mul(i64::try_from(*n).unwrap_or(i64::MAX)))
        .sum();
    let housed: Money = pop
        .residential
        .residents
        .values()
        .map(|r| allowance(config, r.cohort))
        .sum();
    pool + housed
}
