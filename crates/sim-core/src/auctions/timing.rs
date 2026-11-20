//! Converting the auction's real-second timers to ticks (§5.1).
//!
//! The countdown (120 s) and the turn timer (10 s) are real seconds. The clock runs a game day
//! in `real_minutes_per_day` real minutes (default 20) and `ticks_per_day` ticks (default
//! 8,640), so one real second is 7.2 ticks: 120 s = 864 ticks, 10 s = 72 ticks.

use serde::{Deserialize, Serialize};

use crate::clock::ClockRules;
use crate::config::AuctionsConfig;
use crate::money::Money;

/// Ticks covering `real_seconds` at 1× speed, rounded up so a timer never runs short:
/// `⌈real_seconds × ticks_per_day ÷ (real_minutes_per_day × 60)⌉`.
pub fn real_seconds_to_ticks(real_seconds: u32, clock: &ClockRules) -> u64 {
    let real_seconds_per_day = u64::from(clock.real_minutes_per_day).max(1) * 60;
    (u64::from(real_seconds) * u64::from(clock.ticks_per_day())).div_ceil(real_seconds_per_day)
}

/// Auction rules in ticks and pence, derived once from validated config.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct AuctionRules {
    /// Ticks from the opening bid to the first turn (120 real seconds).
    pub countdown_ticks: u64,
    /// Ticks per turn before it counts as a pass (10 real seconds).
    pub turn_ticks: u64,
    /// Allowed raises (£50 / £100 / £500 / £1,000).
    pub increments: Vec<Money>,
}

impl AuctionRules {
    /// Derives the rules from `auctions.yaml` and the clock.
    pub fn from_config(config: &AuctionsConfig, clock: &ClockRules) -> Self {
        AuctionRules {
            countdown_ticks: real_seconds_to_ticks(config.countdown_real_seconds, clock),
            turn_ticks: real_seconds_to_ticks(config.turn_real_seconds, clock),
            increments: config.increments.iter().map(|p| p.money()).collect(),
        }
    }
}
