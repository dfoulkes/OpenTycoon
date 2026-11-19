//! The game clock (§2.1, §2.2; R1, R30, R34).
//!
//! - One tick is `tick_game_seconds` game-seconds (default 10), so a day is 8,640 ticks.
//! - One day lasts `real_minutes_per_day` real minutes (default 20). The clock itself never
//!   reads wall time: the server paces ticks using [`ClockRules::real_millis_per_day`].
//! - Tick 0 is 00:00 on day 0 in `start_year`. Each later midnight is the reckoning, and the
//!   year steps by `years_per_day` (default +5). The game ends at the midnight the year reaches
//!   `end_year` (1930 → 2050 = 24 days).
//! - Trading windows are open-inclusive, close-exclusive, and may wrap midnight (hotels).

use serde::{Deserialize, Serialize};

use crate::config::game::{ClockSettings, TradingHours, Window};

/// Seconds in a game day.
pub const SECONDS_PER_DAY: u32 = 86_400;

/// Clock rules derived once from validated config.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClockRules {
    /// Game-seconds per tick.
    pub tick_game_seconds: u32,
    /// Real minutes per game day.
    pub real_minutes_per_day: u32,
    /// Years added each midnight.
    pub years_per_day: u32,
    /// First year.
    pub start_year: i32,
    /// Year at which the game ends.
    pub end_year: i32,
    /// Trading windows.
    pub trading: TradingWindows,
    /// Second of day the 05:30 arrivals fire.
    pub arrivals_second: u32,
    /// Second of day the 06:00 settlement fires.
    pub settlement_second: u32,
}

/// Trading windows as seconds of day: `(open, close)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TradingWindows {
    /// Day trade.
    pub day: (u32, u32),
    /// Night trade.
    pub night: (u32, u32),
    /// Hotel rooms.
    pub hotel: (u32, u32),
}

impl TradingWindows {
    fn from_config(hours: &TradingHours) -> Self {
        let w = |w: Window| {
            (
                w.open.seconds_since_midnight(),
                w.close.seconds_since_midnight(),
            )
        };
        TradingWindows {
            day: w(hours.day),
            night: w(hours.night),
            hotel: w(hours.hotel),
        }
    }
}

impl ClockRules {
    /// Derives rules from validated config.
    pub fn from_config(clock: &ClockSettings) -> Self {
        ClockRules {
            tick_game_seconds: clock.tick_game_seconds,
            real_minutes_per_day: clock.real_minutes_per_day,
            years_per_day: clock.years_per_day,
            start_year: clock.start_year,
            end_year: clock.end_year,
            trading: TradingWindows::from_config(&clock.trading_hours),
            arrivals_second: clock.schedule.arrivals.seconds_since_midnight(),
            settlement_second: clock.schedule.settlement.seconds_since_midnight(),
        }
    }

    /// Ticks in one game day.
    pub fn ticks_per_day(&self) -> u32 {
        0
    }

    /// Real milliseconds one game day lasts at 1× speed.
    pub fn real_millis_per_day(&self) -> u64 {
        0
    }

    /// Days in a full game: `(end_year − start_year) / years_per_day`.
    pub fn total_days(&self) -> u32 {
        0
    }
}

/// A trading window (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TradingWindow {
    /// Day trade, 07:00–17:00 by default (R1).
    Day,
    /// Night trade, 19:00–23:50 by default (R1).
    Night,
    /// Hotel rooms, 23:30–03:00 by default (R30).
    Hotel,
}

/// What happens on a tick, reported by [`Clock::advance`]. Flags, not a list, so the world's
/// tick order (docs/architecture.md) stays in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent flags; several can fire on one tick in odd configs"
)]
pub struct ClockEvents {
    /// 00:00 reckoning: net worth, write-off, solvency, then the year steps.
    pub midnight: bool,
    /// 05:30 train arrivals (R53).
    pub arrivals: bool,
    /// 06:00 settlement.
    pub settlement: bool,
    /// This midnight reached `end_year`: the final whistle.
    pub game_over: bool,
}

/// The game clock: a tick counter interpreted through [`ClockRules`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Clock {
    rules: ClockRules,
    tick: u64,
}

impl Clock {
    /// A clock at 00:00 on day 0 of `start_year`.
    pub fn new(rules: ClockRules) -> Self {
        Clock { rules, tick: 0 }
    }

    /// The rules.
    pub fn rules(&self) -> &ClockRules {
        &self.rules
    }

    /// Ticks since the start.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Whole days since the start (= midnights passed = year steps taken).
    pub fn day(&self) -> u32 {
        0
    }

    /// Game-seconds since midnight.
    pub fn second_of_day(&self) -> u32 {
        0
    }

    /// `(hour, minute, second)`.
    pub fn time_of_day(&self) -> (u32, u32, u32) {
        let s = self.second_of_day();
        (s / 3600, (s % 3600) / 60, s % 60)
    }

    /// The in-game year.
    pub fn year(&self) -> i32 {
        self.rules.start_year
    }

    /// True once the final midnight has passed.
    pub fn is_over(&self) -> bool {
        false
    }

    /// True if `window` is open now.
    pub fn is_open(&self, _window: TradingWindow) -> bool {
        false
    }

    /// Moves one tick forward and reports what fires at the new time.
    pub fn advance(&mut self) -> ClockEvents {
        self.tick += 1;
        ClockEvents::default()
    }
}
