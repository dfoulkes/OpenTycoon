//! The Grocer: "opens many cheap Low-tier shops" (§10; the one v1 AI, §10c).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `grocer` (plus the `grocer` personality's tier weights); every random
//! draw comes from the RNG it is handed, which for an AI seat is the seat's own stream.
//!
//! What it does, in priority order:
//!
//! 1. **Auction turns, every tick.** On its turn in a land auction for a tier it wants
//!    (personality weight ≥ `min_tier_weight`, i.e. Low), it raises by the smallest increment
//!    while the new bid stays within [`Grocer::max_bid`]: `max_bid_share` of its bid ceiling
//!    (R7), and never so much that it couldn't then build one shop and keep its cash buffer.
//!    Otherwise it passes (R8).
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` until midnight, if its balance is below the solvency line
//!    (R3) it sells its cheapest building back to the city (R5) rather than take a strike.
//! 3. **New shops.** Prices each good at `1 − working_class_discount` of reference, ± up to
//!    `price_jitter` from its RNG, and sets a first order of `initial_order_share` of capacity.
//! 4. **Evening orders.** Once day trade has closed, sets each stocked shop's order for
//!    tomorrow from today's sales: [`next_order`] (R2, R10).
//! 5. **Build.** On a lot it leases, puts up the next shop type in rotation at the first legal
//!    site ([`super::find_site`], R22, R58), if it can pay and still keep `cash_buffer`.
//! 6. **Land.** Only when every lot it holds is full: opens an auction on the cheapest lot it
//!    wants at its land base price (R23), if that is within [`Grocer::max_bid`].

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Brain;
use crate::businesses::BusinessId;
use crate::config::ai::GrocerConfig;
use crate::economy::PlayerId;
use crate::money::Rate;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// The Grocer's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Grocer {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
    /// Tick of its last strategic decision.
    last_decision: Option<u64>,
    /// Shops it has priced and given a first order.
    known: BTreeSet<BusinessId>,
    /// The day it last set evening orders.
    ordered_on_day: Option<u32>,
}

impl Grocer {
    /// A Grocer playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Grocer {
            personality: personality.into(),
            ..Grocer::default()
        }
    }
}

impl Brain for Grocer {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new() // red: the behaviour lands in the next commit
    }
}

/// Its price for a good, as a share of reference (red: stub).
pub fn shop_price(_cfg: &GrocerConfig, _rng: &mut SimRng) -> Rate {
    Rate::ONE
}

/// A new shop's first order (red: stub).
pub fn initial_order(_capacity: i64, _share: Rate) -> i64 {
    0
}

/// Tomorrow's order from today's sales (red: stub).
pub fn next_order(_sold: i64, _capacity: i64, _margin: Rate) -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TEN_PERCENT: Rate = Rate::from_bp(1_000);

    #[test]
    fn tomorrow_follows_today_plus_the_margin() {
        assert_eq!(next_order(20, 64, TEN_PERCENT), 22);
        assert_eq!(next_order(40, 64, TEN_PERCENT), 44);
        assert_eq!(next_order(21, 64, TEN_PERCENT), 24); // 23.1 rounds up
        assert_eq!(next_order(60, 64, TEN_PERCENT), 64); // capped at capacity
        assert_eq!(next_order(0, 64, TEN_PERCENT), 1); // keep a toe in the market
        assert_eq!(next_order(5, 0, TEN_PERCENT), 0); // a shop that holds nothing
    }

    #[test]
    fn first_order_is_a_share_of_capacity() {
        assert_eq!(initial_order(64, Rate::from_bp(5_000)), 32);
        assert_eq!(initial_order(1, Rate::from_bp(5_000)), 1);
        assert_eq!(initial_order(0, Rate::from_bp(5_000)), 0);
        assert_eq!(initial_order(64, Rate::ONE), 64);
    }

    proptest! {
        /// Orders are never negative and never above capacity (R10), whatever sold.
        #[test]
        fn orders_stay_within_capacity(sold in 0i64..10_000, cap in 0i64..10_000, m in 0i64..=10_000) {
            let order = next_order(sold, cap, Rate::from_bp(m));
            prop_assert!(order >= 0 && order <= cap);
            prop_assert!(order >= sold.min(cap)); // never orders less than it sold
        }

        /// The price stays within the jitter of `1 − discount`.
        #[test]
        fn price_stays_within_its_spread(seed in any::<u64>(), disc in 0i64..5_000, jit in 0i64..1_000) {
            let cfg = GrocerConfig {
                working_class_discount: crate::config::Ratio(Rate::from_bp(disc)),
                price_jitter: crate::config::Ratio(Rate::from_bp(jit)),
                ..test_config()
            };
            let p = shop_price(&cfg, &mut crate::rng::seeded(seed)).bp();
            prop_assert!((p - (10_000 - disc)).abs() <= jit);
        }
    }

    fn test_config() -> GrocerConfig {
        use crate::config::{ClockTime, Pounds, Ratio};
        use crate::money::Money;
        GrocerConfig {
            min_tier_weight: Ratio(Rate::from_bp(5_000)),
            max_bid_share: Ratio(Rate::from_bp(3_000)),
            cash_buffer: Pounds(Money::from_pounds(300)),
            shop_types: vec!["grocery".into()],
            shop_footprint: [3, 3],
            shop_height: 12,
            working_class_discount: Ratio(Rate::from_bp(500)),
            price_jitter: Ratio(Rate::from_bp(200)),
            initial_order_share: Ratio(Rate::from_bp(5_000)),
            order_margin: Ratio(TEN_PERCENT),
            rescue_after: ClockTime::from_hm(22, 0).unwrap_or_default(),
        }
    }
}
