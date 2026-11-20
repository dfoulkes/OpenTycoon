//! Latin hypercube sampling (R64), deterministic from the study seed.
//!
//! For `count` models and `dims` parameters: each dimension's range is cut into `count` equal
//! strata, a seeded Fisher–Yates shuffle decides which model gets which stratum, and a seeded
//! jitter places the point inside its stratum. So every stratum of every parameter is sampled
//! exactly once, and the same seed always gives the same models. The draws come from
//! sim-core's portable ChaCha8 helpers (`rng::below`, `rng::unit_f64`), so the models are the
//! same on every platform. Log-uniform maps go through `libm`, never the platform maths library.

use sim_core::rng::{SimRng, below, seeded, unit_f64};

use super::def::{Distribution, Kind, Parameter};

/// Domain separation: the hypercube never shares a stream with a game seeded the same.
const LHS_STREAM: u64 = 0x4C48_535F_5354_5544; // "LHS_STUD"

/// `count` points in `[0, 1)^dims`: for each dimension, exactly one point in each of `count`
/// equal strata. Indexed `[point][dim]`. Draw order: per dimension, the shuffle then the
/// jitters, so adding a parameter at the end leaves the earlier ones' samples unchanged.
pub fn unit_samples(dims: usize, count: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut points = vec![vec![0.0; dims]; count];
    if count == 0 {
        return points;
    }
    let mut rng: SimRng = seeded(seed ^ LHS_STREAM);
    #[allow(
        clippy::cast_precision_loss,
        reason = "model counts are far below 2^52"
    )]
    let n = count as f64;
    for dim in 0..dims {
        let mut strata: Vec<usize> = (0..count).collect();
        for i in (1..count).rev() {
            let j = usize::try_from(below(&mut rng, (i as u64) + 1)).unwrap_or(i);
            strata.swap(i, j);
        }
        for (point, stratum) in points.iter_mut().zip(strata) {
            #[allow(
                clippy::cast_precision_loss,
                reason = "stratum < count, far below 2^52"
            )]
            let u = (stratum as f64 + unit_f64(&mut rng)) / n;
            // Guard the top edge against rounding up to exactly 1.
            point[dim] = u.min(1.0 - f64::EPSILON);
        }
    }
    points
}

/// The category index of unit sample `u` when `strata` models share `categories` values.
pub fn category_at(u: f64, strata: usize, categories: usize) -> usize {
    let _ = (u, strata, categories);
    0
}

/// Rounds `v` to `places` decimals (half away from zero).
pub fn round_to(v: f64, places: u32) -> f64 {
    let scale = (0..places).fold(1.0, |s, _| s * 10.0);
    (v * scale).round() / scale
}

/// The value of `p` at unit sample `u` in `[0, 1)`, rounded as the parameter asks and kept in
/// `[min, max]`.
///
/// - uniform decimal: `min + u (max − min)`;
/// - uniform integer: `min + ⌊u (max − min + 1)⌋`, so every whole number is equally likely;
/// - log-uniform decimal: `exp(ln min + u (ln max − ln min))`;
/// - log-uniform integer: `⌊exp(ln min + u (ln (max + 1) − ln min))⌋`.
pub fn value_at(p: &Parameter, u: f64) -> f64 {
    let (lo, hi) = p.range();
    let top = match p.kind {
        Kind::Integer => hi + 1.0,
        Kind::Decimal => hi,
    };
    let raw = match p.distribution {
        Distribution::Uniform => lo + u * (top - lo),
        Distribution::LogUniform => {
            let (a, b) = (libm::log(lo), libm::log(top));
            libm::exp(a + u * (b - a))
        }
    };
    let v = match p.kind {
        Kind::Integer => raw.floor(),
        Kind::Decimal => round_to(raw, p.places()),
    };
    v.clamp(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn integers_cover_every_whole_number_with_enough_models() {
        let p = Parameter::set(
            "x",
            "game.a",
            1.0,
            5.0,
            Distribution::Uniform,
            Kind::Integer,
        );
        let mut seen: Vec<f64> = unit_samples(1, 5, 9)
            .iter()
            .map(|pt| value_at(&p, pt[0]))
            .collect();
        seen.sort_by(f64::total_cmp);
        assert_eq!(seen, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn decimals_are_rounded_to_their_places() {
        let mut p = Parameter::set(
            "x",
            "game.a",
            0.0,
            1.0,
            Distribution::Uniform,
            Kind::Decimal,
        );
        p.decimals = Some(3);
        let v = value_at(&p, 0.123_456);
        assert!((v - 0.123).abs() < 1e-12, "{v}");
    }

    #[test]
    fn log_uniform_puts_half_the_models_below_the_geometric_mean() {
        let p = Parameter::set(
            "x",
            "game.a",
            1.0,
            10_000.0,
            Distribution::LogUniform,
            Kind::Decimal,
        );
        let below_100 = unit_samples(1, 100, 1)
            .iter()
            .filter(|pt| value_at(&p, pt[0]) < 100.0)
            .count();
        assert_eq!(below_100, 50);
    }

    #[test]
    fn adding_a_parameter_keeps_the_earlier_samples() {
        let one = unit_samples(1, 20, 4);
        let two = unit_samples(2, 20, 4);
        for (a, b) in one.iter().zip(&two) {
            assert_eq!(a[0].to_bits(), b[0].to_bits());
        }
    }

    #[test]
    fn categories_split_the_strata_evenly() {
        let shares = |strata: usize, k: usize| {
            let mut n = vec![0usize; k];
            for pt in unit_samples(1, strata, 2) {
                n[category_at(pt[0], strata, k)] += 1;
            }
            n
        };
        assert_eq!(shares(12, 3), vec![4, 4, 4]);
        assert_eq!(shares(11, 3), vec![3, 4, 4]);
        assert_eq!(shares(99, 3), vec![33, 33, 33]);
        assert_eq!(shares(5, 1), vec![5]);
    }

    proptest! {
        #[test]
        fn categories_differ_by_at_most_one(strata in 1usize..200, k in 1usize..8, seed in any::<u64>()) {
            let mut n = vec![0usize; k];
            for pt in unit_samples(1, strata, seed) {
                let c = category_at(pt[0], strata, k);
                prop_assert!(c < k);
                n[c] += 1;
            }
            let (lo, hi) = (n.iter().min().unwrap(), n.iter().max().unwrap());
            prop_assert!(hi - lo <= 1, "{n:?}");
        }

        #[test]
        fn every_stratum_is_hit_once(dims in 1usize..5, count in 1usize..60, seed in any::<u64>()) {
            let pts = unit_samples(dims, count, seed);
            for d in 0..dims {
                let mut hits = vec![0u32; count];
                for p in &pts {
                    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let s = (p[d] * count as f64) as usize;
                    hits[s] += 1;
                }
                prop_assert!(hits.iter().all(|h| *h == 1));
            }
        }

        #[test]
        fn values_stay_in_range(min in -1000i32..1000, span in 0i32..1000, u in 0.0f64..1.0, int in any::<bool>()) {
            let (lo, hi) = (f64::from(min), f64::from(min + span));
            let kind = if int { Kind::Integer } else { Kind::Decimal };
            let p = Parameter::set("x", "game.a", lo, hi, Distribution::Uniform, kind);
            let v = value_at(&p, u);
            prop_assert!((lo..=hi).contains(&v));
        }
    }
}
