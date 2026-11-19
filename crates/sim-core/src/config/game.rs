//! `game.yaml`: world scale, clock, presets, bankruptcy, overdraft, net worth, leases (§2, §3, §10b).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{ClockTime, Pounds, Ratio};

/// Root of `game.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GameConfig {
    /// World geometry.
    pub world: WorldSettings,
    /// Game clock (R34) and trading hours (R1, R30).
    pub clock: ClockSettings,
    /// Game length presets (§2.4), keyed by name.
    pub presets: BTreeMap<String, Preset>,
    /// Bankruptcy and selling up (§2.3; R3, R4, R5).
    pub bankruptcy: BankruptcySettings,
    /// Overdraft (R7, R32).
    pub overdraft: OverdraftSettings,
    /// Net worth scoring (R19, R39).
    pub net_worth: NetWorthSettings,
    /// Land leases (§5.2).
    pub leases: LeaseSettings,
    /// Player seats.
    pub players: PlayerSettings,
}

/// World geometry (§4.1, R48, R57).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldSettings {
    /// Metres per lot unit (R48, default 4).
    #[schemars(range(min = 1, max = 100))]
    pub metres_per_lot_unit: u32,
    /// Lot size in units, `[width, depth]` (R22, default 9×9).
    pub lot_size: [u32; 2],
    /// Global building height cap in units (default 100). Per-type caps may not exceed it (R58).
    #[schemars(range(min = 1, max = 1000))]
    pub max_height: u32,
}

/// Game clock (R34).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClockSettings {
    /// Real minutes per in-game day (R34, default 20).
    #[schemars(range(min = 1, max = 1440))]
    pub real_minutes_per_day: u32,
    /// Years added at each midnight (R34, default 5).
    #[schemars(range(min = 1, max = 100))]
    pub years_per_day: u32,
    /// First in-game year (default 1930).
    #[schemars(range(min = 1800, max = 2500))]
    pub start_year: i32,
    /// Final year: the game ends at the midnight the year reaches this (default 2050).
    #[schemars(range(min = 1800, max = 2500))]
    pub end_year: i32,
    /// Game-seconds per simulation tick (§10a, default 10). Must divide 60.
    #[schemars(range(min = 1, max = 60))]
    pub tick_game_seconds: u32,
    /// Trading windows.
    pub trading_hours: TradingHours,
    /// Fixed daily schedule points.
    pub schedule: DailySchedule,
}

/// Trading windows (R1, R30). Open is inclusive, close is exclusive; a window may wrap midnight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TradingHours {
    /// Day trade (default 07:00–17:00).
    pub day: Window,
    /// Night trade (default 19:00–23:50).
    pub night: Window,
    /// Hotel rooms (default 23:30–03:00, wraps midnight).
    pub hotel: Window,
}

/// One trading window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    /// Opening time (inclusive).
    pub open: ClockTime,
    /// Closing time (exclusive).
    pub close: ClockTime,
}

/// Fixed daily schedule (§2.2). Midnight reckoning is the day boundary and is not configurable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DailySchedule {
    /// New population arrives by train (R53, default 05:30).
    pub arrivals: ClockTime,
    /// Bills paid, stock arrives, population joins (default 06:00).
    pub settlement: ClockTime,
}

/// A game length preset (§2.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    /// First year.
    #[schemars(range(min = 1800, max = 2500))]
    pub start_year: i32,
    /// Final year.
    #[schemars(range(min = 1800, max = 2500))]
    pub end_year: i32,
}

/// Bankruptcy and selling up (§2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BankruptcySettings {
    /// Consecutive failed midnights before bankruptcy (default 2).
    #[schemars(range(min = 1, max = 100))]
    pub strikes_to_bankrupt: u32,
    /// A midnight balance at or above this passes (R3, default £0).
    pub solvent_at_or_above: Pounds,
    /// Instant sale back to the city, as a share of true value (R5, default 0.5).
    pub sell_to_city_ratio: Ratio,
}

/// Overdraft (R7, R32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OverdraftSettings {
    /// Credit limit at the start (default £10,000).
    pub start_limit: Pounds,
    /// Flat increase per 5-year step (default £2,500).
    pub increase_per_step: Pounds,
    /// Daily interest on overdrawn balances (default 0: R32 says none).
    pub interest_rate: Ratio,
}

/// Net worth scoring (R19, R39).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetWorthSettings {
    /// Share of every property's value credited to the lot's leaseholder (R39, default 0.2).
    pub lot_property_uplift: Ratio,
}

/// Land leases (§5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeaseSettings {
    /// Lease length in in-game years (default 30).
    #[schemars(range(min = 1, max = 500))]
    pub term_years: u32,
}

/// Player seats (§1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayerSettings {
    /// Fewest players in a game (default 2).
    #[schemars(range(min = 1, max = 16))]
    pub min: u32,
    /// Most players in a game (default 6).
    #[schemars(range(min = 1, max = 16))]
    pub max: u32,
    /// Cash each player starts with. Not in the spec; placeholder £0 (players start on the
    /// overdraft, R7).
    pub starting_cash: Pounds,
}
