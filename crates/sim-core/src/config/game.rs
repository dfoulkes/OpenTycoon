//! `game.yaml`: world scale, clock, presets, bankruptcy and player seats (§2, §10b). The money
//! levers (overdraft, net worth, leases, starting cash, sell-back) are in `economy.yaml` (R71).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{ClockTime, Pounds};

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
    /// Bankruptcy (§2.3; R3, R4).
    pub bankruptcy: BankruptcySettings,
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

/// Bankruptcy (§2.3). The sell-back ratio is `economy.yaml` `money.sell_to_city_ratio` (R71).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BankruptcySettings {
    /// Consecutive failed midnights before bankruptcy (default 2).
    #[schemars(range(min = 1, max = 100))]
    pub strikes_to_bankrupt: u32,
    /// A midnight balance at or above this passes (R3, default £0).
    pub solvent_at_or_above: Pounds,
}

/// Player seats (§1). Starting cash is `economy.yaml` `money.starting_cash` (R71).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayerSettings {
    /// Fewest players in a game (default 2).
    #[schemars(range(min = 1, max = 16))]
    pub min: u32,
    /// Most players in a game (default 6).
    #[schemars(range(min = 1, max = 16))]
    pub max: u32,
}
