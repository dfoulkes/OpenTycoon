//! Order statistics for balance summaries (spec §11: "chart median net worth and bankruptcy
//! rate").
//!
//! Percentiles use the **nearest-rank** method on integers (pence), so a summary is exact,
//! needs no interpolation and is identical on every platform: `p`-th percentile of `n` sorted
//! values is the value at rank `ceil(p × n / 100)` (1-based), with rank 1 for `p = 0`.

use serde::Serialize;

/// The spread of a set of integer values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Distribution {
    /// Number of values.
    pub count: usize,
    /// Smallest.
    pub min: i64,
    /// 10th percentile.
    pub p10: i64,
    /// Median (50th percentile, nearest rank: the lower middle for an even count).
    pub median: i64,
    /// 90th percentile.
    pub p90: i64,
    /// Largest.
    pub max: i64,
}

/// The nearest-rank `p`-th percentile of `sorted` (ascending). `None` if empty; `p` above 100
/// is treated as 100.
pub fn percentile(sorted: &[i64], p: u32) -> Option<i64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    let p = u128::from(p.min(100));
    let n_wide = n as u128;
    // ceil(p × n / 100), at least 1. Fits in usize because it is at most n.
    let rank = (p * n_wide).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(n - 1);
    sorted.get(index).copied()
}

/// Summarises `values` (any order). `None` if empty.
pub fn distribution(values: &[i64]) -> Option<Distribution> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Some(Distribution {
        count: sorted.len(),
        min: *sorted.first()?,
        p10: percentile(&sorted, 10)?,
        median: percentile(&sorted, 50)?,
        p90: percentile(&sorted, 90)?,
        max: *sorted.last()?,
    })
}

/// `part / whole` as a fraction in `[0, 1]`; 0 when `whole` is 0.
#[allow(
    clippy::cast_precision_loss,
    reason = "counts of games are far below 2^52; a rate is for display"
)]
pub fn rate(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn nearest_rank_examples() {
        let v = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&v, 10), Some(10));
        assert_eq!(percentile(&v, 50), Some(50));
        assert_eq!(percentile(&v, 90), Some(90));
        assert_eq!(percentile(&v, 0), Some(10));
        assert_eq!(percentile(&v, 100), Some(100));
        assert_eq!(percentile(&v, 101), Some(100));
        assert_eq!(percentile(&[], 50), None);
        assert_eq!(percentile(&[7], 90), Some(7));
        // Even count: the lower middle.
        assert_eq!(percentile(&[1, 2, 3, 4], 50), Some(2));
    }

    #[test]
    fn empty_has_no_distribution_and_rate_of_nothing_is_zero() {
        assert_eq!(distribution(&[]), None);
        assert!(rate(0, 0).abs() < f64::EPSILON);
        assert!((rate(1, 4) - 0.25).abs() < f64::EPSILON);
    }

    proptest! {
        #[test]
        fn percentiles_are_ordered_and_within_min_max(values in prop::collection::vec(any::<i64>(), 1..200)) {
            let d = distribution(&values).unwrap();
            prop_assert_eq!(d.count, values.len());
            prop_assert!(d.min <= d.p10);
            prop_assert!(d.p10 <= d.median);
            prop_assert!(d.median <= d.p90);
            prop_assert!(d.p90 <= d.max);
            prop_assert_eq!(d.min, *values.iter().min().unwrap());
            prop_assert_eq!(d.max, *values.iter().max().unwrap());
        }

        #[test]
        fn every_percentile_is_one_of_the_values(values in prop::collection::vec(any::<i64>(), 1..100), p in 0u32..=100) {
            let mut sorted = values.clone();
            sorted.sort_unstable();
            let q = percentile(&sorted, p).unwrap();
            prop_assert!(values.contains(&q));
        }

        #[test]
        fn order_of_input_does_not_matter(mut values in prop::collection::vec(any::<i64>(), 1..100), seed in any::<u64>()) {
            let before = distribution(&values);
            // Deterministic shuffle: rotate by a seed-derived amount and reverse.
            let len = values.len();
            values.rotate_left(usize::try_from(seed % len as u64).unwrap());
            values.reverse();
            prop_assert_eq!(before, distribution(&values));
        }

        #[test]
        fn median_splits_the_values(values in prop::collection::vec(-1_000i64..1_000, 1..200)) {
            let d = distribution(&values).unwrap();
            let at_or_below = values.iter().filter(|v| **v <= d.median).count();
            let at_or_above = values.iter().filter(|v| **v >= d.median).count();
            prop_assert!(2 * at_or_below >= values.len());
            prop_assert!(2 * at_or_above >= values.len());
        }

        #[test]
        fn a_constant_list_has_that_constant_everywhere(v in any::<i64>(), n in 1usize..50) {
            let d = distribution(&vec![v; n]).unwrap();
            prop_assert_eq!((d.min, d.p10, d.median, d.p90, d.max), (v, v, v, v, v));
        }

        #[test]
        fn rate_is_a_fraction(whole in 0usize..10_000, part_share in 0u32..=100) {
            let part = whole * part_share as usize / 100;
            let r = rate(part, whole);
            prop_assert!((0.0..=1.0).contains(&r));
        }
    }
}
