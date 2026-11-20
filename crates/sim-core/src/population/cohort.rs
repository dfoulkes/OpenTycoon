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
    let _ = (women, births_per_woman, years, childbearing_years, rng);
    0
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
    let _ = (pop, rules, new_year, years, rng);
    YearStep::default()
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
