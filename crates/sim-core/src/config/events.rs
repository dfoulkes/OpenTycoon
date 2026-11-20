//! `events.yaml`: historical events, random events, elections (§9, §9.1; R12, R16).
//!
//! Every effect is a typed, configurable modifier from a closed set ([`Effect`]), so a typo
//! fails loudly. Effects either change the world once when they fire (destroy buildings, move
//! a block's tier, scale land or property values, shift class shares) or install a modifier on
//! the events module's stack for a number of days (demand multipliers, supply caps, opening
//! days, price noise, footfall), which other modules query.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{Pounds, Ratio};
use crate::land::Tier;
use crate::population::Class;

/// Root of `events.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventsConfig {
    /// Named groups of goods that effects target (`luxury`, `staples`, ...). An effect's
    /// `target` is a group name or a single good id from `businesses.yaml`.
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
    /// Fixed history. Each fires on the year step covering its year (§9).
    pub historical: Vec<HistoricalEvent>,
    /// Per-building daily random events (§9: fire, burglary, inspection).
    pub random: Vec<RandomEvent>,
    /// Optional insurance against random events (§9).
    #[serde(default)]
    pub insurance: InsuranceSettings,
    /// Random front-page events from a year onwards (§9: 2030+).
    #[serde(default)]
    pub random_era: RandomEra,
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
    /// Scale shoppers' demand (the chance a good is on their list) for `days` days. `target`
    /// is a good id or a group in `groups`; `class` limits it to one class. With no target it
    /// applies to every good.
    DemandMultiplier {
        /// Good id or group name.
        #[serde(default)]
        target: Option<String>,
        /// Only shoppers of this class.
        #[serde(default)]
        class: Option<Class>,
        /// Multiplier on demand.
        factor: Ratio,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Cap tomorrow's order of the target goods at `share` of each shop's capacity (rationing).
    SupplyCap {
        /// Good id or group name.
        target: String,
        /// Share of `capacity_per_good` a shop may stock.
        share: Ratio,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Shops may open only `open` days in `of` (the three-day week). Shops rotate: on game
    /// day `d`, shop `id` trades iff `(id + d) mod of < open`.
    OpenDays {
        /// Days open.
        open: u32,
        /// Out of.
        of: u32,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Every shop's price of every good is off by a random amount within `±spread` (Decimal
    /// Day's price confusion), drawn once when the event fires.
    PriceNoise {
        /// Largest deviation, as a share of the price.
        spread: Ratio,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Shops on these blocks draw `factor` × the footfall (their pull on shoppers) for `days`
    /// days.
    Footfall {
        /// Block ids.
        blocks: Vec<String>,
        /// Multiplier on the shop's pull.
        factor: Ratio,
        /// How many in-game days it lasts.
        days: u32,
    },
    /// Move blocks to a new desirability tier (permanent).
    DesirabilityChange {
        /// Block ids.
        blocks: Vec<String>,
        /// New tier.
        tier: Tier,
    },
    /// Destroy buildings at random, with compensation paid by the city.
    DestroyBuildings {
        /// Block ids at risk.
        blocks: Vec<String>,
        /// Chance per building.
        chance: Ratio,
        /// Compensation as a share of property value.
        compensation: Ratio,
    },
    /// Scale every lease's land value (its last auction price) once.
    LandValue {
        /// Multiplier.
        factor: Ratio,
    },
    /// Scale the property value (build cost) of every building of these types once.
    PropertyValue {
        /// Business type ids.
        types: Vec<String>,
        /// Multiplier.
        factor: Ratio,
    },
    /// Move `share` of the unhoused adults of class `from` to class `to` (permanent).
    ClassShift {
        /// From.
        from: Class,
        /// To.
        to: Class,
        /// Share moved.
        share: Ratio,
    },
    /// Switch the currency display to decimal (a display flag only).
    DecimalCurrency,
    /// Headline only; no mechanical effect.
    Flavour,
}

/// What a random per-building event does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IncidentEffect {
    /// The building is destroyed.
    Destroy,
    /// `share` of every good on the shelf is lost.
    StockLoss {
        /// Share lost.
        share: Ratio,
    },
    /// The owner pays the city a fine.
    Fine {
        /// Amount.
        amount: Pounds,
    },
}

/// A random per-building event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RandomEvent {
    /// Stable id.
    pub id: String,
    /// Chance per building per day.
    pub daily_chance: Ratio,
    /// What happens.
    pub effect: IncidentEffect,
    /// Share of the loss an insured owner is paid back by the city (0: not insurable).
    #[serde(default)]
    pub insured_payout: Ratio,
}

/// Insurance (§9: an optional product).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct InsuranceSettings {
    /// Daily premium as a share of each insured building's property value, paid to the city at
    /// 06:00.
    pub premium_rate: Ratio,
}

/// Random front-page events (§9: 2030+).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct RandomEra {
    /// First step year that can roll one.
    pub from_year: i32,
    /// Chance per year step that one fires.
    pub chance_per_step: Ratio,
    /// The pool; one is picked by weight.
    pub events: Vec<EraEvent>,
}

/// One event in the random pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EraEvent {
    /// Stable id.
    pub id: String,
    /// Newspaper headline.
    pub headline: String,
    /// Relative weight.
    #[schemars(range(min = 1))]
    pub weight: u32,
    /// What it does.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// Elections (R12, §9.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElectionSettings {
    /// Whether every year step holds a general election.
    pub enabled: bool,
    /// Win weights per step year. Steps without an entry use the latest earlier one.
    pub weights: BTreeMap<String, PartyWeights>,
    /// Front-page headlines per result.
    #[serde(default)]
    pub headlines: ElectionHeadlines,
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

/// Election headlines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ElectionHeadlines {
    /// Labour win.
    pub labour: String,
    /// Conservative win.
    pub conservative: String,
}

impl Default for ElectionHeadlines {
    fn default() -> Self {
        ElectionHeadlines {
            labour: "Labour Win the General Election".into(),
            conservative: "Conservatives Win the General Election".into(),
        }
    }
}
