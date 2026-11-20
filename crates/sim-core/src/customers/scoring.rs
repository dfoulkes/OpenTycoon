//! Customer scoring: the one place floats are allowed (§8.3; engineering.md §3).
//!
//! `score = class_affinity(class, tier) × fit_out_appeal × price_factor × distance_decay ×
//! reputation`, picked by weighted random, never argmax. Flats are scored the same way without
//! distance or reputation (R51). Transcendentals go through `libm` so x86_64 and aarch64 agree
//! bit for bit; no `mul_add`.
#![allow(
    clippy::float_arithmetic,
    reason = "customer scoring is the one place floats are allowed (docs/engineering.md)"
)]

use crate::config::CustomersConfig;
use crate::land::Tier;
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::population::Class;
use crate::rng::{SimRng, unit_f64};

/// A [`Rate`] as a float (`10_000` bp → `1.0`).
#[allow(
    clippy::cast_precision_loss,
    reason = "basis points are far below 2^53, so the conversion is exact"
)]
pub fn rate_f64(rate: Rate) -> f64 {
    rate.bp() as f64 / BASIS_POINTS_PER_UNIT as f64
}

/// A float in `[0, 1]` as basis points, rounded to nearest.
#[allow(
    clippy::cast_possible_truncation,
    reason = "clamped to [0, 1] first, so the result is within 0..=10_000"
)]
pub fn unit_to_rate(x: f64) -> Rate {
    let clamped = if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    };
    Rate::from_bp((clamped * 10_000.0).round() as i64)
}

/// Pence as a float, for price ratios only.
#[allow(
    clippy::cast_precision_loss,
    reason = "game prices are far below 2^53 pence"
)]
pub fn pence_f64(m: Money) -> f64 {
    m.pence() as f64
}

/// `k_class` price sensitivity (§8.3): working 3.0, middle 1.5, upper 0.5 by default.
pub fn k_class(rules: &CustomersConfig, class: Class) -> f64 {
    let k = &rules.price_sensitivity;
    match class {
        Class::Working => k.working,
        Class::Middle => k.middle,
        Class::Upper => k.upper,
    }
}

/// `class_affinity(class, tier)` from `customers.yaml` (§8.3), before fit-out appeal.
pub fn class_affinity(rules: &CustomersConfig, class: Class, tier: Tier) -> f64 {
    let row = match class {
        Class::Working => &rules.class_affinity.working,
        Class::Middle => &rules.class_affinity.middle,
        Class::Upper => &rules.class_affinity.upper,
    };
    match tier {
        Tier::Low => row.low,
        Tier::Medium => row.medium,
        Tier::High => row.high,
    }
}

/// A place's pull before price: `class_affinity(class, tier) × fit_out_appeal` (§7.2, §8.3).
pub fn desirability(rules: &CustomersConfig, class: Class, tier: Tier, appeal: Rate) -> f64 {
    class_affinity(rules, class, tier) * rate_f64(appeal)
}

/// A flat's score for house-hunting: `desirability × price_factor(k_class, rent, reference)`
/// (R51: nicer lot and building attract more, balanced by price).
pub fn flat_score(
    rules: &CustomersConfig,
    class: Class,
    tier: Tier,
    appeal: Rate,
    rent: Money,
) -> f64 {
    let reference = pence_f64(rules.residential.reference_rent.money()).max(1.0);
    desirability(rules, class, tier, appeal)
        * price_factor(k_class(rules, class), pence_f64(rent), reference)
}

/// A shop's score for one good (§8.3):
/// `affinity × price_factor × distance_decay(blocks) × reputation`. Luxury goods use
/// `−luxury_price_boost` as their sensitivity, so a higher price helps them mildly.
#[allow(
    clippy::too_many_arguments,
    reason = "the §8.3 formula has this many inputs"
)]
pub fn shop_score(
    rules: &CustomersConfig,
    class: Class,
    affinity: f64,
    price: Money,
    ref_price: Money,
    luxury: bool,
    blocks: f64,
    reputation: Rate,
) -> f64 {
    let k = if luxury {
        -rules.luxury_price_boost
    } else {
        k_class(rules, class)
    };
    affinity
        * price_factor(k, pence_f64(price), pence_f64(ref_price).max(1.0))
        * distance_decay(rules.distance_decay.per_block, blocks)
        * rate_f64(reputation)
}

/// `price_factor = exp(-k × (price/ref_price − 1))` (§8.3). `ref_price` must be positive.
pub fn price_factor(k_class: f64, price: f64, ref_price: f64) -> f64 {
    libm::exp(-k_class * (price / ref_price - 1.0))
}

/// `distance_decay = exp(-per_block × blocks)` (§8.3).
pub fn distance_decay(per_block: f64, blocks: f64) -> f64 {
    libm::exp(-per_block * blocks)
}

/// Picks an index by weighted random over non-negative scores (§8.3: not argmax).
/// Returns `None` if every score is zero, negative or not finite.
pub fn weighted_pick(scores: &[f64], rng: &mut SimRng) -> Option<usize> {
    let valid = |s: f64| s.is_finite() && s > 0.0;
    let total: f64 = scores.iter().copied().filter(|s| valid(*s)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut target = unit_f64(rng) * total;
    let mut last = None;
    for (i, s) in scores.iter().copied().enumerate() {
        if !valid(s) {
            continue;
        }
        last = Some(i);
        if target < s {
            return Some(i);
        }
        target -= s;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::seeded;

    #[test]
    fn price_factor_is_one_at_reference_price() {
        assert!((price_factor(3.0, 1.0, 1.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn working_class_is_more_price_sensitive_than_upper() {
        let dear = 1.5;
        assert!(price_factor(3.0, dear, 1.0) < price_factor(0.5, dear, 1.0));
    }

    #[test]
    fn price_factor_bits_are_pinned() {
        // libm is pure Rust, so this exact bit pattern must hold on every target. CI runs this
        // on x86_64 and aarch64; if it ever differs, determinism is broken.
        assert_eq!(
            price_factor(1.5, 1.2, 1.0).to_bits(),
            4_604_847_916_870_499_334
        );
    }

    #[test]
    fn weighted_pick_skips_zero_and_respects_weights() {
        let mut rng = seeded(1);
        assert_eq!(weighted_pick(&[0.0, 0.0], &mut rng), None);
        assert_eq!(weighted_pick(&[0.0, 2.0, 0.0], &mut rng), Some(1));
        let mut counts = [0u32; 2];
        for _ in 0..10_000 {
            if let Some(i) = weighted_pick(&[1.0, 3.0], &mut rng) {
                counts[i] += 1;
            }
        }
        assert!(counts[1] > counts[0] * 2, "{counts:?}");
    }
}
