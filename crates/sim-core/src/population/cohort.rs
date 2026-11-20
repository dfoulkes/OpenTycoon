//! The cohort model's year step: births, deaths, ageing, children coming of age, migration
//! (§8.1; R6, R60, R61).
//!
//! Runs once per midnight, after the clock has stepped the year (architecture.md, phase 3).
//! Every rate is integer: counts × basis points, with the fractional person settled by one
//! seeded draw ([`stochastic_round`]) so small cohorts still age and die on average, and the
//! same seed always gives the same city.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{AgeBand, Class, Cohort, Population, Sex};
use crate::config::CustomersConfig;
use crate::money::{BASIS_POINTS_PER_UNIT, Rate};
use crate::rng::{SimRng, below};

/// What one year step did, for reports and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct YearStep {
    /// Children born this step.
    pub births: u64,
    /// Adults who died (pool and residents).
    pub deaths: u64,
    /// Adults who moved up an age band.
    pub aged_up: u64,
    /// Children who reached 18 and joined the next arrivals.
    pub came_of_age: u64,
    /// Migrants added to the next arrivals.
    pub migrants: u64,
}

/// `numer / denom`, rounded down, plus one more with probability `remainder / denom`.
/// Exact (no draw) when `denom` divides `numer`. Returns 0 when `denom` is 0.
pub fn stochastic_round(numer: u128, denom: u128, rng: &mut SimRng) -> u64 {
    if denom == 0 {
        return 0;
    }
    let whole = numer / denom;
    let rem = numer % denom;
    let extra = if rem == 0 {
        0
    } else {
        // denom fits u64 for every caller (bp × years × span); clamp defensively.
        let d = u64::try_from(denom).unwrap_or(u64::MAX);
        let r = u64::try_from(rem).unwrap_or(u64::MAX);
        u128::from(below(rng, d) < r)
    };
    u64::try_from(whole + extra).unwrap_or(u64::MAX)
}

/// Splits `n` people over `weights` in proportion, by largest remainder (ties to the lower
/// index). The parts always sum to `n` (unless every weight is zero: then all zero).
pub fn split_by_weights(n: u64, weights: &[u64]) -> Vec<u64> {
    let total: u128 = weights.iter().map(|w| u128::from(*w)).sum();
    if total == 0 {
        return vec![0; weights.len()];
    }
    let mut parts: Vec<u64> = Vec::with_capacity(weights.len());
    let mut rems: Vec<(u128, usize)> = Vec::with_capacity(weights.len());
    for (i, w) in weights.iter().enumerate() {
        let exact = u128::from(n) * u128::from(*w);
        parts.push(u64::try_from(exact / total).unwrap_or(0));
        rems.push((exact % total, i));
    }
    let given: u64 = parts.iter().sum();
    let mut left = n - given;
    rems.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in rems {
        if left == 0 {
            break;
        }
        parts[i] += 1;
        left -= 1;
    }
    parts
}

/// The entry for `year` in a table keyed by year strings: the latest key at or before `year`
/// (eras are steps, not interpolated). `None` if every key is later.
pub fn era_value<T>(table: &BTreeMap<String, T>, year: i32) -> Option<&T> {
    table
        .iter()
        .filter_map(|(k, v)| k.parse::<i32>().ok().map(|y| (y, v)))
        .filter(|(y, _)| *y <= year)
        .max_by_key(|(y, _)| *y)
        .map(|(_, v)| v)
}

/// The adult age at which children join the arrivals: the youngest band's lower bound (18).
pub fn adult_age(rules: &CustomersConfig) -> u32 {
    rules.age_bands.first().map_or(18, |b| b.min_age)
}

/// Bands wholly inside the childbearing span `[adult_age, adult_age + childbearing_years)`:
/// 18–29 and 30–49 by default (§8.1: women aged 18–49).
pub fn childbearing_bands(rules: &CustomersConfig) -> Vec<AgeBand> {
    let first = adult_age(rules);
    let last = first + rules.childbearing_years.saturating_sub(1);
    rules
        .age_bands
        .iter()
        .enumerate()
        .filter(|(_, b)| b.min_age >= first && b.max_age.is_some_and(|m| m <= last))
        .filter_map(|(i, _)| u8::try_from(i).ok().map(AgeBand))
        .collect()
}

/// Women in the childbearing bands, housed or not.
pub fn women_of_childbearing_age(pop: &Population, rules: &CustomersConfig) -> u64 {
    let bands = childbearing_bands(rules);
    let fertile = |c: &Cohort| c.sex == Sex::Female && bands.contains(&c.band);
    let pool: u64 = pop
        .pool
        .iter()
        .filter(|(c, _)| fertile(c))
        .map(|(_, n)| *n)
        .sum();
    let housed = pop
        .residential
        .residents
        .values()
        .filter(|r| fertile(&r.cohort))
        .count() as u64;
    pool + housed
}

/// `births = women(18–49) × births_per_woman(era) × years ÷ childbearing_years` (§8.1, R61).
///
/// Births per woman is a lifetime figure spread over the childbearing span, so one step of
/// `years` years bears `years / span` of it.
pub fn births(
    women: u64,
    births_per_woman: Rate,
    years: u32,
    childbearing_years: u32,
    rng: &mut SimRng,
) -> u64 {
    let numer =
        u128::from(women) * u128::from(births_per_woman.bp().unsigned_abs()) * u128::from(years);
    let denom = u128::from(BASIS_POINTS_PER_UNIT.unsigned_abs()) * u128::from(childbearing_years);
    stochastic_round(numer, denom, rng)
}

/// Splits `n` newcomers into cohorts by the era's demographics (class × age × sex). With
/// `only_band`, everyone is in that band (children coming of age are all 18–29).
pub fn split_newcomers(
    n: u64,
    rules: &CustomersConfig,
    year: i32,
    only_band: Option<AgeBand>,
) -> BTreeMap<Cohort, u64> {
    let mut out = BTreeMap::new();
    let Some(demo) = era_value(&rules.demographics, year) else {
        return out;
    };
    let bp = |r: &crate::config::types::Ratio| u64::try_from(r.rate().bp()).unwrap_or(0);
    let mut keys = Vec::new();
    let mut weights = Vec::new();
    for (ci, class) in Class::ALL.iter().enumerate() {
        for (bi, band_share) in demo.age.iter().enumerate() {
            let Ok(band) = u8::try_from(bi).map(AgeBand) else {
                continue;
            };
            let age_w = match only_band {
                Some(only) if only == band => BASIS_POINTS_PER_UNIT.unsigned_abs(),
                Some(_) => 0,
                None => bp(band_share),
            };
            for (si, sex) in Sex::ALL.iter().enumerate() {
                keys.push(Cohort {
                    band,
                    sex: *sex,
                    class: *class,
                });
                weights.push(bp(&demo.class[ci]) * age_w * bp(&demo.sex[si]));
            }
        }
    }
    for (key, count) in keys.into_iter().zip(split_by_weights(n, &weights)) {
        if count > 0 {
            out.insert(key, count);
        }
    }
    out
}

/// One midnight's step of the cohort model (§8.1, R61), in this order:
///
/// 1. **Births** from the women of childbearing age, at the births per woman of the era that
///    just ended (`new_year − years`). Children are counted under `new_year`.
/// 2. **Deaths** per band at `mortality_per_step` (pool by count, residents one by one; a
///    resident who dies frees their flat).
/// 3. **Ageing**: each band but the top sends `years / span` of its people up a band.
/// 4. **Coming of age**: children aged `adult_age` or more join the next 05:30 arrivals as
///    18–29s, split by class and sex.
/// 5. **Migration**: `migration_per_day` newcomers join the next arrivals.
pub fn year_step(
    pop: &mut Population,
    rules: &CustomersConfig,
    new_year: i32,
    years: u32,
    rng: &mut SimRng,
) -> YearStep {
    let mut report = YearStep::default();
    let old_year = new_year - i32::try_from(years).unwrap_or(0);
    let bpw = era_value(&rules.births_per_woman, old_year).map_or(Rate::ZERO, |r| r.rate());
    let women = women_of_childbearing_age(pop, rules);
    report.births = births(women, bpw, years, rules.childbearing_years, rng);
    report.deaths = deaths(pop, rules, rng);
    report.aged_up = ageing(pop, rules, years, rng);
    if report.births > 0 {
        *pop.children.entry(new_year).or_insert(0) += report.births;
    }
    report.came_of_age = coming_of_age(pop, rules, new_year);
    report.migrants = rules.migration_per_day;
    for (cohort, k) in split_newcomers(rules.migration_per_day, rules, new_year, None) {
        *pop.pending.entry(cohort).or_insert(0) += k;
    }
    report
}

/// Mortality per band, as basis points per step, in band order.
fn mortality_bp(rules: &CustomersConfig) -> Vec<u64> {
    rules
        .age_bands
        .iter()
        .map(|b| {
            rules
                .mortality_per_step
                .get(&b.id)
                .map_or(0, |r| r.rate().bp().unsigned_abs())
        })
        .collect()
}

/// Deaths per band at `mortality_per_step`: the pool by count, residents one by one (a
/// resident who dies frees their flat). Returns the number who died.
fn deaths(pop: &mut Population, rules: &CustomersConfig, rng: &mut SimRng) -> u64 {
    let mortality = mortality_bp(rules);
    let rate_of = |band: AgeBand| mortality.get(usize::from(band.0)).copied().unwrap_or(0);
    let unit = BASIS_POINTS_PER_UNIT.unsigned_abs();
    let mut dead_total = 0;
    for (cohort, n) in &mut pop.pool {
        let rate = rate_of(cohort.band);
        let dead = stochastic_round(u128::from(*n) * u128::from(rate), u128::from(unit), rng);
        let dead = dead.min(*n);
        *n -= dead;
        dead_total += dead;
    }
    pop.pool.retain(|_, n| *n > 0);
    let mut died = Vec::new();
    for (id, r) in &pop.residential.residents {
        let rate = rate_of(r.cohort.band);
        if rate > 0 && below(rng, unit) < rate {
            died.push(*id);
        }
    }
    dead_total += died.len() as u64;
    for id in died {
        pop.residential.residents.remove(&id);
    }
    dead_total
}

/// Ageing: each band but the open-ended top one sends `years / span` of its people up a band
/// (the pool by count, residents one by one). Every move is worked out before any is applied,
/// so nobody ages twice. Returns the number who moved up.
fn ageing(pop: &mut Population, rules: &CustomersConfig, years: u32, rng: &mut SimRng) -> u64 {
    let span = |band: AgeBand| -> Option<u32> {
        let b = rules.age_bands.get(usize::from(band.0))?;
        let next = rules.age_bands.get(usize::from(band.0) + 1);
        next.and(b.max_age.map(|m| m.saturating_sub(b.min_age) + 1))
    };
    let mut aged = 0;
    let moves: Vec<(Cohort, u64)> = pop
        .pool
        .iter()
        .filter_map(|(cohort, n)| {
            let s = span(cohort.band)?;
            let up = stochastic_round(u128::from(*n) * u128::from(years), u128::from(s), rng);
            Some((*cohort, up.min(*n)))
        })
        .collect();
    for (cohort, up) in moves.into_iter().filter(|(_, up)| *up > 0) {
        if let Some(n) = pop.pool.get_mut(&cohort) {
            *n -= up;
        }
        let older = Cohort {
            band: AgeBand(cohort.band.0 + 1),
            ..cohort
        };
        *pop.pool.entry(older).or_insert(0) += up;
        aged += up;
    }
    pop.pool.retain(|_, n| *n > 0);
    for r in pop.residential.residents.values_mut() {
        let Some(s) = span(r.cohort.band) else {
            continue;
        };
        if below(rng, u64::from(s)) < u64::from(years) {
            r.cohort.band = AgeBand(r.cohort.band.0 + 1);
            aged += 1;
        }
    }
    aged
}

/// Children aged `adult_age` or more join the next 05:30 arrivals as 18–29s, split by the
/// era's class and sex mix. Returns how many came of age.
fn coming_of_age(pop: &mut Population, rules: &CustomersConfig, new_year: i32) -> u64 {
    let adult = i64::from(adult_age(rules));
    let grown: Vec<i32> = pop
        .children
        .keys()
        .copied()
        .filter(|born| i64::from(new_year) - i64::from(*born) >= adult)
        .collect();
    let mut total = 0;
    for born in grown {
        let n = pop.children.remove(&born).unwrap_or(0);
        total += n;
        for (cohort, k) in split_newcomers(n, rules, new_year, Some(AgeBand(0))) {
            *pop.pending.entry(cohort).or_insert(0) += k;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::seeded;
    use proptest::prelude::*;

    #[test]
    fn stochastic_round_is_exact_when_it_divides() {
        let mut rng = seeded(1);
        assert_eq!(stochastic_round(1000, 10, &mut rng), 100);
        assert_eq!(stochastic_round(5, 0, &mut rng), 0);
    }

    #[test]
    fn stochastic_round_is_unbiased_on_average() {
        let mut rng = seeded(2);
        let total: u64 = (0..10_000).map(|_| stochastic_round(1, 4, &mut rng)).sum();
        assert!((2_300..2_700).contains(&total), "{total}");
    }

    #[test]
    fn split_by_weights_uses_largest_remainder() {
        assert_eq!(split_by_weights(10, &[1, 1, 1]), vec![4, 3, 3]);
        assert_eq!(split_by_weights(7, &[0, 0]), vec![0, 0]);
        assert_eq!(split_by_weights(100, &[7, 3]), vec![70, 30]);
    }

    #[test]
    fn era_value_takes_the_latest_key_at_or_before() {
        let table: BTreeMap<String, u32> = [("1930".into(), 1), ("1960".into(), 2)].into();
        assert_eq!(era_value(&table, 1929), None);
        assert_eq!(era_value(&table, 1955), Some(&1));
        assert_eq!(era_value(&table, 1960), Some(&2));
        assert_eq!(era_value(&table, 2050), Some(&2));
    }

    proptest! {
        #[test]
        fn split_by_weights_conserves_people(n in 0u64..1_000_000, w in prop::collection::vec(0u64..100_000, 1..30)) {
            let parts = split_by_weights(n, &w);
            let total: u64 = parts.iter().sum();
            if w.iter().all(|x| *x == 0) {
                prop_assert_eq!(total, 0);
            } else {
                prop_assert_eq!(total, n);
            }
        }

        #[test]
        fn stochastic_round_stays_within_one(numer in 0u128..1_000_000_000, denom in 1u128..1_000_000, seed in any::<u64>()) {
            let mut rng = seeded(seed);
            let r = u128::from(stochastic_round(numer, denom, &mut rng));
            prop_assert!(r == numer / denom || r == numer / denom + 1);
        }
    }
}
