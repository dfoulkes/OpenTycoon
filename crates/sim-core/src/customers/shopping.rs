//! Shopping lists: `P(good | age, class, sex, hour, year)` from `customers.yaml` `desires`
//! (§8.2, R55).
//!
//! Rules apply in order; every rule whose match fits the shopper sets the weight of each good
//! it names, so a later, narrower rule overrides an earlier, broader one. The "hour" is the
//! trading window (day or night, §8.4). Each good is then rolled once: on the list with
//! probability `P(good)`.
#![allow(
    clippy::float_arithmetic,
    reason = "shopping-list probabilities are customer scoring inputs (docs/engineering.md)"
)]

use std::collections::BTreeMap;

use super::shops::{GoodId, GoodsIndex};
use crate::businesses::Hours;
use crate::config::CustomersConfig;
use crate::config::customers::DesireMatch;
use crate::events::Modifiers;
use crate::money::BASIS_POINTS_PER_UNIT;
use crate::population::{Class, Cohort, band_id};
use crate::rng::{SimRng, unit_f64};

/// True if a desire rule's match covers this shopper, window and year.
pub fn rule_matches(
    m: &DesireMatch,
    rules: &CustomersConfig,
    cohort: Cohort,
    window: Hours,
    year: i32,
) -> bool {
    m.class.is_none_or(|c| c == cohort.class)
        && m.sex.is_none_or(|s| s == cohort.sex)
        && m.age
            .as_deref()
            .is_none_or(|a| a == band_id(rules, cohort.band))
        && m.window.is_none_or(|w| w == window)
        && m.from_year.is_none_or(|y| year >= y)
        && m.until_year.is_none_or(|y| year <= y)
}

/// `P(good)` for every good this shopper might want in this window and year (§8.2, R55).
pub fn desire_weights(
    rules: &CustomersConfig,
    cohort: Cohort,
    window: Hours,
    year: i32,
) -> BTreeMap<&str, f64> {
    let mut out = BTreeMap::new();
    for rule in &rules.desires {
        if rule_matches(&rule.matcher, rules, cohort, window, year) {
            for (good, p) in &rule.goods {
                out.insert(good.as_str(), *p);
            }
        }
    }
    out
}

/// The goods a shopper might roll, with `P(good) > 0`, in catalogue (name) order: the
/// [`desire_weights`] of one cohort, window and year resolved to ids once, so a wave rolls
/// every member of a cohort from the same list ([`roll_from`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Candidates(Vec<(GoodId, f64)>);

/// [`Candidates`] for a cohort, window and year.
pub fn candidates(
    rules: &CustomersConfig,
    goods: &GoodsIndex,
    cohort: Cohort,
    window: Hours,
    year: i32,
) -> Candidates {
    let mut out = Vec::new();
    for (name, p) in desire_weights(rules, cohort, window, year) {
        if p <= 0.0 {
            continue;
        }
        let Some(id) = goods.id(name) else {
            continue;
        };
        out.push((id, p));
    }
    Candidates(out)
}

impl Candidates {
    /// The candidates with each `P(good)` scaled by the event modifiers' demand factor for the
    /// shopper's class (§9), capped at 1. A no-op with no modifiers in force.
    #[must_use]
    pub fn scaled_by(mut self, modifiers: &Modifiers, goods: &GoodsIndex, class: Class) -> Self {
        if modifiers.is_empty() {
            return self;
        }
        for (id, p) in &mut self.0 {
            let factor = modifiers.demand_factor(goods.name(*id), class);
            #[allow(
                clippy::cast_precision_loss,
                reason = "basis points are small integers"
            )]
            let f = factor.bp() as f64 / BASIS_POINTS_PER_UNIT as f64;
            *p = (*p * f).clamp(0.0, 1.0);
        }
        self
    }
}

/// Rolls a shopping list from [`candidates`]: each good is on it with probability `P`, one
/// draw per candidate, in order.
pub fn roll_from(candidates: &Candidates, rng: &mut SimRng) -> Vec<GoodId> {
    let mut list: Vec<GoodId> = Vec::new();
    for &(id, p) in &candidates.0 {
        if unit_f64(rng) < p {
            list.push(id);
        }
    }
    list.sort();
    list
}

/// Rolls a shopping list: each good with `P > 0` is on it with probability `P`, in catalogue
/// order. One draw per candidate good.
pub fn roll_list(
    rules: &CustomersConfig,
    goods: &GoodsIndex,
    cohort: Cohort,
    window: Hours,
    year: i32,
    rng: &mut SimRng,
) -> Vec<GoodId> {
    roll_from(&candidates(rules, goods, cohort, window, year), rng)
}
