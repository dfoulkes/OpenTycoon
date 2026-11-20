//! Events: the newspaper, historical events, random events, elections, insurance (§9, §9.1;
//! R12, R16).
//!
//! Owner boundary: deciding what happens on each year step and each morning, and the modifier
//! stack ([`modifiers`]) other modules query. The world applies the effects that touch other
//! modules' state (destroyed buildings, tiers, land and property values, class shares, utility
//! ownership, compensation) in `world/events_phase.rs`; this module decides and records.
//!
//! On every year step (the midnight reckoning, after lease expiry):
//!
//! 1. modifiers that have run out are dropped;
//! 2. the general election is rolled (§9.1, R12): a weighted roll, Labour or Conservative;
//! 3. every historical event whose year the step covers fires (§9), in config order;
//! 4. from `random_era.from_year`, one random event may fire (§9: 2030+);
//! 5. lasting effects go on the stack; one-off effects are handed to the world.
//!
//! Every 06:00, after settlement, each player's building rolls each random event (fire,
//! burglary, inspection; §9).
//!
//! **Determinism.** Every roll here draws from the events module's own stream, seeded from
//! the game seed, never the world's: an event never shifts shoppers' draws, and the same seed
//! replays the same history.

pub mod modifiers;

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::businesses::{BusinessId, Businesses};
use crate::config::Config;
use crate::config::events::{Effect, ElectionSettings, EventsConfig, HistoricalEvent};
use crate::economy::{AccountId, PlayerId};
use crate::land::Map;
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::rng::{SimRng, below, seeded};

pub use modifiers::{ActiveModifier, Modifier, Modifiers};

/// A front page shown on the year step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct FrontPage {
    /// Year of the step.
    pub year: i32,
    /// Headlines: the election first, then events in config order.
    pub headlines: Vec<String>,
}

/// A general election winner (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Party {
    /// Nationalises the utilities.
    Labour,
    /// Privatises them.
    Conservative,
}

/// An event that fired on a year step.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fired {
    /// Step year.
    pub year: i32,
    /// Event id.
    pub id: String,
}

/// A random event that hit a building (§9).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Incident {
    /// Game day.
    pub day: u32,
    /// Random event id.
    pub id: String,
    /// The building.
    pub building: BusinessId,
    /// Its owner.
    pub owner: AccountId,
    /// What the owner lost (property value, stock at wholesale price, or the fine).
    pub loss: Money,
    /// What the city paid back under insurance.
    pub payout: Money,
}

/// Event state carried between steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    /// The latest front page.
    pub front_page: FrontPage,
    /// Lasting effects in force.
    pub modifiers: Modifiers,
    /// The currency shows decimal (Decimal Day; display only).
    pub decimal_currency: bool,
    /// Players who have taken out insurance (§9).
    pub insured: BTreeSet<PlayerId>,
    /// Every election so far: step year and winner.
    pub elections: Vec<(i32, Party)>,
    /// Every historical and random-era event fired so far.
    pub fired: Vec<Fired>,
    /// Random building events so far.
    pub incidents: Vec<Incident>,
    rng: SimRng,
}

impl Hash for Events {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.front_page.hash(state);
        self.modifiers.hash(state);
        self.decimal_currency.hash(state);
        self.insured.hash(state);
        self.elections.hash(state);
        self.fired.hash(state);
        self.incidents.hash(state);
        self.rng.get_word_pos().hash(state);
    }
}

impl Default for Events {
    fn default() -> Self {
        Events::new(0)
    }
}

/// The events module's own stream: the game seed mixed with a constant, so it never shares
/// draws with the world or an AI seat.
pub fn events_rng(game_seed: u64) -> SimRng {
    seeded(game_seed ^ 0x5DEE_CE66_D1CE_4E5B)
}

/// True with probability `chance` (basis points), one draw.
pub fn roll(rng: &mut SimRng, chance: Rate) -> bool {
    let bp = u64::try_from(chance.bp()).unwrap_or(0);
    below(rng, BASIS_POINTS_PER_UNIT.unsigned_abs()) < bp
}

/// Historical events that fire on the step starting at `step_year` and covering
/// `years_per_step` years: `step_year ≤ year < step_year + years_per_step` (§9).
pub fn events_for_step(
    config: &EventsConfig,
    step_year: i32,
    years_per_step: u32,
) -> Vec<&HistoricalEvent> {
    let end = i64::from(step_year) + i64::from(years_per_step);
    config
        .historical
        .iter()
        .filter(|e| e.year >= step_year && i64::from(e.year) < end)
        .collect()
}

/// The election weights in force for `year`: the entry for the latest year at or before it.
pub fn election_weights(settings: &ElectionSettings, year: i32) -> Option<(Rate, Rate)> {
    settings
        .weights
        .iter()
        .filter_map(|(k, w)| Some((k.parse::<i32>().ok()?, w)))
        .filter(|(y, _)| *y <= year)
        .max_by_key(|(y, _)| *y)
        .map(|(_, w)| (w.labour.rate(), w.conservative.rate()))
}

/// The general election (§9.1, R12): Labour wins with probability
/// `labour / (labour + conservative)`, one draw. `None` if elections are off or no weights
/// cover the year.
pub fn hold_election(settings: &ElectionSettings, year: i32, rng: &mut SimRng) -> Option<Party> {
    if !settings.enabled {
        return None;
    }
    let (labour, conservative) = election_weights(settings, year)?;
    let l = u64::try_from(labour.bp()).unwrap_or(0);
    let c = u64::try_from(conservative.bp()).unwrap_or(0);
    if l + c == 0 {
        return None;
    }
    Some(if below(rng, l + c) < l {
        Party::Labour
    } else {
        Party::Conservative
    })
}

/// What a year step decided, for the world to apply.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StepOutcome {
    /// The election winner, if one was held.
    pub election: Option<Party>,
    /// One-off effects to apply to the world now, with the id of the event that caused them.
    pub effects: Vec<(String, Effect)>,
}

/// What a year step reads of the world.
#[derive(Debug, Clone, Copy)]
pub struct StepInputs<'a> {
    /// Validated config.
    pub config: &'a Config,
    /// The map (block ids for footfall).
    pub map: &'a Map,
    /// Businesses (price noise is drawn per shop and good).
    pub businesses: &'a Businesses,
    /// The new step year.
    pub year: i32,
    /// The game day the step starts.
    pub day: u32,
    /// Years per step.
    pub years_per_step: u32,
}

impl Events {
    /// Every `land_value` factor among the events fired so far, in firing order (§9: the 2008
    /// crash). Net worth's market land price reads them (docs/economy.md "Land value"); an
    /// event id missing from `rules` (config validation rules it out) contributes nothing.
    pub fn land_value_factors(&self, rules: &EventsConfig) -> Vec<Rate> {
        let effects = |id: &str| -> &[Effect] {
            rules
                .historical
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.effects.as_slice())
                .or_else(|| {
                    rules
                        .random_era
                        .events
                        .iter()
                        .find(|e| e.id == id)
                        .map(|e| e.effects.as_slice())
                })
                .unwrap_or(&[])
        };
        self.fired
            .iter()
            .flat_map(|f| effects(&f.id))
            .filter_map(|e| match e {
                Effect::LandValue { factor } => Some(factor.rate()),
                _ => None,
            })
            .collect()
    }

    /// A fresh newspaper for a game seeded with `game_seed`.
    pub fn new(game_seed: u64) -> Self {
        Events {
            front_page: FrontPage::default(),
            modifiers: Modifiers::default(),
            decimal_currency: false,
            insured: BTreeSet::new(),
            elections: Vec::new(),
            fired: Vec::new(),
            incidents: Vec::new(),
            rng: events_rng(game_seed),
        }
    }

    /// The events stream, for rolls the world makes on the module's behalf (which buildings a
    /// raid destroys, random building events).
    pub fn rng_mut(&mut self) -> &mut SimRng {
        &mut self.rng
    }

    /// The modifier stack.
    pub fn modifiers(&self) -> &Modifiers {
        &self.modifiers
    }

    /// Runs the year step that starts game day `day` in `year` (module docs, steps 1–5).
    /// Lasting effects go on the stack; one-off effects are returned for the world.
    pub fn year_step(&mut self, inputs: StepInputs<'_>) -> StepOutcome {
        self.modifiers.expire(inputs.day);
        let rules = &inputs.config.events;
        let year = inputs.year;
        let mut outcome = StepOutcome::default();
        let mut headlines = Vec::new();
        if let Some(party) = hold_election(&rules.elections, year, &mut self.rng) {
            self.elections.push((year, party));
            headlines.push(match party {
                Party::Labour => rules.elections.headlines.labour.clone(),
                Party::Conservative => rules.elections.headlines.conservative.clone(),
            });
            outcome.election = Some(party);
        }
        let mut fired: Vec<(&str, &str, &[Effect])> =
            events_for_step(rules, year, inputs.years_per_step)
                .into_iter()
                .map(|e| (e.id.as_str(), e.headline.as_str(), e.effects.as_slice()))
                .collect();
        let era = &rules.random_era;
        if year >= era.from_year
            && !era.events.is_empty()
            && roll(&mut self.rng, era.chance_per_step.rate())
        {
            let total: u64 = era.events.iter().map(|e| u64::from(e.weight)).sum();
            let mut pick = below(&mut self.rng, total);
            for e in &era.events {
                let w = u64::from(e.weight);
                if pick < w {
                    fired.push((&e.id, &e.headline, &e.effects));
                    break;
                }
                pick -= w;
            }
        }
        for (id, headline, effects) in fired {
            headlines.push(headline.to_owned());
            self.fired.push(Fired {
                year,
                id: id.to_owned(),
            });
            for effect in effects {
                if !self.push_lasting(inputs, id, effect) {
                    outcome.effects.push((id.to_owned(), effect.clone()));
                }
            }
        }
        self.front_page = FrontPage { year, headlines };
        outcome
    }

    /// Puts a lasting effect on the stack, or sets a display flag. Returns false for one-off
    /// effects, which the world applies.
    fn push_lasting(&mut self, inputs: StepInputs<'_>, source: &str, effect: &Effect) -> bool {
        let groups = &inputs.config.events.groups;
        let (days, modifier) = match effect {
            Effect::DemandMultiplier {
                target,
                class,
                factor,
                days,
            } => (
                *days,
                Modifier::Demand {
                    goods: target.as_deref().map(|t| resolve_goods(groups, t)),
                    class: *class,
                    factor: factor.rate(),
                },
            ),
            Effect::SupplyCap {
                target,
                share,
                days,
            } => (
                *days,
                Modifier::SupplyCap {
                    goods: resolve_goods(groups, target),
                    share: share.rate(),
                },
            ),
            Effect::OpenDays { open, of, days } => (
                *days,
                Modifier::OpenDays {
                    open: *open,
                    of: *of,
                },
            ),
            Effect::PriceNoise { spread, days } => (
                *days,
                Modifier::PriceNoise {
                    factors: price_noise(inputs.businesses, spread.rate(), &mut self.rng),
                },
            ),
            Effect::Footfall {
                blocks,
                factor,
                days,
            } => (
                *days,
                Modifier::Footfall {
                    blocks: blocks
                        .iter()
                        .filter_map(|b| inputs.map.block_id(b))
                        .collect(),
                    factor: factor.rate(),
                },
            ),
            Effect::DecimalCurrency => {
                self.decimal_currency = true;
                return true;
            }
            Effect::Flavour => return true,
            Effect::DesirabilityChange { .. }
            | Effect::DestroyBuildings { .. }
            | Effect::LandValue { .. }
            | Effect::PropertyValue { .. }
            | Effect::ClassShift { .. } => return false,
        };
        self.modifiers
            .push(source, inputs.day.saturating_add(days), modifier);
        true
    }
}

/// The goods a target names: a group's goods, or the one good.
pub fn resolve_goods(groups: &BTreeMap<String, Vec<String>>, target: &str) -> BTreeSet<String> {
    groups.get(target).map_or_else(
        || BTreeSet::from([target.to_owned()]),
        |g| g.iter().cloned().collect(),
    )
}

/// Decimal Day's price confusion: for every shop and every good it sells, a factor drawn
/// uniformly from `1 − spread ..= 1 + spread` in whole basis points.
pub fn price_noise(
    businesses: &Businesses,
    spread: Rate,
    rng: &mut SimRng,
) -> BTreeMap<(BusinessId, String), Rate> {
    let spread = spread.bp().max(0);
    let width = u64::try_from(2 * spread + 1).unwrap_or(1);
    let mut out = BTreeMap::new();
    for (id, b) in &businesses.by_id {
        for good in b.prices.keys() {
            let offset = i64::try_from(below(rng, width)).unwrap_or(0) - spread;
            out.insert(
                (*id, good.clone()),
                Rate::from_bp(BASIS_POINTS_PER_UNIT + offset),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::events::{
        ElectionHeadlines, ElectionSettings, HistoricalEvent, InsuranceSettings, PartyWeights,
        RandomEra,
    };
    use crate::config::types::Ratio;

    fn event(id: &str, year: i32) -> HistoricalEvent {
        HistoricalEvent {
            id: id.into(),
            year,
            headline: id.into(),
            effects: vec![],
        }
    }

    fn elections(weights: &[(i32, i64, i64)]) -> ElectionSettings {
        ElectionSettings {
            enabled: true,
            weights: weights
                .iter()
                .map(|(y, l, c)| {
                    (
                        y.to_string(),
                        PartyWeights {
                            labour: Ratio(Rate::from_bp(*l)),
                            conservative: Ratio(Rate::from_bp(*c)),
                        },
                    )
                })
                .collect(),
            headlines: ElectionHeadlines::default(),
        }
    }

    #[test]
    fn each_event_fires_on_the_step_covering_its_year() {
        let config = EventsConfig {
            groups: BTreeMap::new(),
            historical: vec![
                event("blitz", 1940),
                event("decimal", 1971),
                event("crash", 2008),
            ],
            random: vec![],
            insurance: InsuranceSettings::default(),
            random_era: RandomEra::default(),
            elections: elections(&[]),
        };
        let ids = |y| {
            events_for_step(&config, y, 5)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(1940), ["blitz"]);
        assert_eq!(ids(1970), ["decimal"]);
        assert_eq!(ids(2005), ["crash"]);
        assert_eq!(ids(1935), Vec::<String>::new());
    }

    #[test]
    fn a_roll_hits_at_its_chance() {
        let mut rng = events_rng(9);
        let hits = (0..20_000)
            .filter(|_| roll(&mut rng, Rate::from_bp(2_000)))
            .count();
        assert!((3_700..=4_300).contains(&hits), "{hits}");
        assert!((0..100).all(|_| roll(&mut rng, Rate::ONE)));
        assert!((0..100).all(|_| !roll(&mut rng, Rate::ZERO)));
    }

    #[test]
    fn election_weights_carry_forward_from_the_latest_earlier_year() {
        let s = elections(&[(1930, 4_000, 6_000), (1945, 9_000, 1_000)]);
        assert_eq!(
            election_weights(&s, 1940),
            Some((Rate::from_bp(4_000), Rate::from_bp(6_000)))
        );
        assert_eq!(
            election_weights(&s, 1950),
            Some((Rate::from_bp(9_000), Rate::from_bp(1_000)))
        );
        assert_eq!(election_weights(&s, 1925), None);
    }

    #[test]
    fn the_election_roll_follows_its_weights_and_is_seeded() {
        let s = elections(&[(1930, 9_000, 1_000)]);
        let mut rng = events_rng(1);
        let labour = (0..10_000)
            .filter(|_| hold_election(&s, 1945, &mut rng) == Some(Party::Labour))
            .count();
        assert!((8_800..=9_200).contains(&labour), "{labour}");
        let first: Vec<_> = (0..50u64)
            .map(|seed| hold_election(&s, 1945, &mut events_rng(seed)))
            .collect();
        let again: Vec<_> = (0..50u64)
            .map(|seed| hold_election(&s, 1945, &mut events_rng(seed)))
            .collect();
        assert_eq!(first, again);
        let certain = elections(&[(1930, 10_000, 0)]);
        assert!((0..100).all(|_| hold_election(&certain, 1950, &mut rng) == Some(Party::Labour)));
    }

    #[test]
    fn price_noise_stays_within_the_spread() {
        let config = crate::economy::test_support::shipped_config();
        let mut businesses = Businesses::default();
        for i in 0..50 {
            let request = crate::businesses::BuildRequest {
                type_id: "bakery".into(),
                block: crate::land::BlockId(0),
                footprint: crate::land::Footprint {
                    x: 0,
                    y: 0,
                    w: 2,
                    d: 2,
                },
                height: 4,
                owner: AccountId::City,
                fit_out: crate::businesses::FitOutLevel::Medium,
            };
            let kind = &config.businesses.types["bakery"];
            businesses.by_id.insert(
                BusinessId(i),
                crate::businesses::Business::new(request, kind, Money::ZERO),
            );
        }
        let factors = price_noise(&businesses, Rate::from_bp(1_000), &mut events_rng(3));
        assert_eq!(factors.len(), 50);
        assert!(factors.values().all(|r| (9_000..=11_000).contains(&r.bp())));
        assert!(factors.values().any(|r| *r != Rate::ONE));
    }

    #[test]
    fn groups_resolve_to_their_goods_and_anything_else_is_one_good() {
        let groups = BTreeMap::from([("luxury".to_owned(), vec!["clothes".to_owned()])]);
        assert_eq!(
            resolve_goods(&groups, "luxury"),
            BTreeSet::from(["clothes".to_owned()])
        );
        assert_eq!(
            resolve_goods(&groups, "bread"),
            BTreeSet::from(["bread".to_owned()])
        );
    }
}
