//! Customers: walking agents, shopping lists, shop choice, movement (§8.2–§8.4; R25, R55).
//!
//! Owner boundary: individual shoppers on the map and the scoring that picks a shop. Counts and
//! arrivals come from [`crate::population`]; sales are recorded by [`crate::businesses`].
//!
//! This is the one module allowed floats (scoring). Transcendentals go through `libm` so
//! x86_64 and aarch64 agree bit for bit.
//!
//! Status: scoring primitives are real; agents land in M1.
#![allow(
    clippy::float_arithmetic,
    reason = "customer scoring is the one place floats are allowed (docs/engineering.md)"
)]

use serde::{Deserialize, Serialize};

use crate::economy::AccountId;
use crate::land::BlockId;
use crate::population::{AgeBand, Class, Sex};
use crate::rng::{SimRng, unit_f64};

/// A stable customer id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct CustomerId(pub u64);

/// Where a customer came from (§8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Origin {
    /// Lives in an apartment.
    Resident,
    /// Commuter: always takes the train home (R59).
    Commuter,
    /// Tourist: wants a hotel (R59).
    Tourist,
}

/// One shopper on the map. Money is pence; position is a block for now.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Customer {
    /// Id.
    pub id: CustomerId,
    /// Class.
    pub class: Class,
    /// Age band.
    pub band: AgeBand,
    /// Sex.
    pub sex: Sex,
    /// Origin.
    pub origin: Origin,
    /// Where they are.
    pub at: BlockId,
    /// Their wallet account (spend moves money to shops through the ledger).
    pub wallet: AccountId,
}

/// All customers on the map.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Customers {
    /// Agents, in id order.
    pub agents: Vec<Customer>,
    /// Next id to hand out.
    pub next_id: u64,
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
