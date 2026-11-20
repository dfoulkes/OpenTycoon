//! Shopping lists: `P(good | age, class, sex, hour, year)` from `customers.yaml` `desires`
//! (§8.2, R55).
//!
//! Rules apply in order; every rule whose match fits the shopper sets the weight of each good
//! it names, so a later, narrower rule overrides an earlier, broader one. A `scale` rule
//! multiplies weights already set instead (capped at 1), so one rule can shift a segment in
//! every era. The "hour" is the trading window (day or night, §8.4). The year selects the era
//! tables (R64); on top of them a good is wanted only from its `unlock_year` and fades after its
//! `obsolete_year` ([`availability`], §7.1). Each good is then rolled once: on the list with
//! probability `P(good)`.
#![allow(
    clippy::float_arithmetic,
    reason = "shopping-list probabilities are customer scoring inputs (docs/engineering.md)"
)]

use std::collections::BTreeMap;

use super::shops::{GoodId, GoodsIndex};
use crate::businesses::Hours;
use crate::config::customers::{DesireMatch, DesireMode, Obsolescence};
use crate::config::{Config, CustomersConfig};
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
                match rule.mode {
                    DesireMode::Set => {
                        out.insert(good.as_str(), *p);
                    }
                    DesireMode::Scale => {
                        if let Some(w) = out.get_mut(good.as_str()) {
                            *w = (*w * p).min(1.0);
                        }
                    }
                }
            }
        }
    }
    out
}

/// The share of the table's demand for something that exists in `year` (§7.1): 0 before
/// `unlock_year`, 1 up to `obsolete_year`, then falling linearly to `floor` over `fade_years`
/// and staying there.
pub fn availability(
    unlock_year: Option<i32>,
    obsolete_year: Option<i32>,
    year: i32,
    fade: &Obsolescence,
) -> f64 {
    if unlock_year.is_some_and(|u| year < u) {
        return 0.0;
    }
    match obsolete_year {
        Some(o) if year > o => {
            let gone = f64::from(year - o) / f64::from(fade.fade_years.max(1));
            (1.0 - gone).max(fade.floor)
        }
        _ => 1.0,
    }
}

/// `P(good)` as shoppers act on it in `year`: the table's weight, zero before the good's
/// unlock year and fading after its obsolete year (§7.1, §8.2).
///
/// Goods missing from the catalogue are dropped (config validation forbids them).
pub fn effective_weights(
    config: &Config,
    cohort: Cohort,
    window: Hours,
    year: i32,
) -> BTreeMap<&str, f64> {
    let fade = &config.customers.obsolescence;
    desire_weights(&config.customers, cohort, window, year)
        .into_iter()
        .filter_map(|(name, p)| {
            let good = config.businesses.goods.get(name)?;
            Some((
                name,
                p * availability(good.unlock_year, good.obsolete_year, year, fade),
            ))
        })
        .collect()
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
        let Some(id) = goods.id(name) else {
            continue;
        };
        let (unlock, obsolete) = goods.years(id);
        let p = p * availability(unlock, obsolete, year, &rules.obsolescence);
        if p <= 0.0 {
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    const FADE: Obsolescence = Obsolescence {
        fade_years: 15,
        floor: 0.1,
    };

    #[test]
    fn nothing_is_wanted_before_it_unlocks() {
        assert!(close(availability(Some(1980), None, 1975, &FADE), 0.0));
        assert!(close(availability(Some(1980), None, 1980, &FADE), 1.0));
        assert!(close(availability(None, None, 1930, &FADE), 1.0));
    }

    #[test]
    fn demand_fades_linearly_after_the_obsolete_year_down_to_the_floor() {
        assert!(close(availability(None, Some(2010), 2010, &FADE), 1.0));
        assert!(close(
            availability(None, Some(2010), 2015, &FADE),
            2.0 / 3.0
        ));
        assert!(close(
            availability(None, Some(2010), 2020, &FADE),
            1.0 / 3.0
        ));
        assert!(close(availability(None, Some(2010), 2025, &FADE), 0.1));
        assert!(close(availability(None, Some(2010), 2050, &FADE), 0.1));
    }

    #[test]
    fn a_zero_floor_fades_to_nothing() {
        let gone = Obsolescence {
            fade_years: 5,
            floor: 0.0,
        };
        assert!(close(
            availability(Some(1950), Some(2000), 2005, &gone),
            0.0
        ));
    }
}
