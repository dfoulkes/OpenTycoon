//! `map.yaml`: the boards (grid, blocks, districts, colour sets, land prices, tube stations),
//! the tier multipliers and the land rate (§4, §5).
//!
//! The file holds every board a game can be played on, keyed by name, and picks one with the
//! top-level `map` key (`v1 | full`). The tier multipliers and land rate are economics, not
//! geography, so every board shares them. [`MapFile::board`] resolves a board into the
//! [`MapConfig`] the rest of the sim reads as `config.map`.

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{Pounds, Ratio};
use crate::land::{BlockKind, ColourBand, Tier};
use crate::utilities::UtilityKind;

/// Which board a game is played on (R26).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MapChoice {
    /// The grey-box 3×3 test map (§10c, §11).
    V1,
    /// The full 9×9 board: 26 colour sets, 4 utilities and City Hall (§4.1, R26).
    Full,
}

impl MapChoice {
    /// Every board name.
    pub const ALL: [MapChoice; 2] = [MapChoice::V1, MapChoice::Full];

    /// The key under `boards` in `map.yaml`.
    pub const fn key(self) -> &'static str {
        match self {
            MapChoice::V1 => "v1",
            MapChoice::Full => "full",
        }
    }
}

impl fmt::Display for MapChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Root of `map.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapFile {
    /// The board this game is played on: a key of `boards`.
    pub map: MapChoice,
    /// Per-tier multipliers (§5.3), shared by every board.
    pub tiers: TierTable,
    /// Land rent per unit of building volume per day, before the tier multiplier (§5.3, R10).
    pub land_base_rate: Pounds,
    /// Tube lines by id, shared by every board (§4.3).
    pub tube_lines: BTreeMap<String, TubeLine>,
    /// Every board, keyed by name.
    pub boards: Boards,
}

/// The boards in `map.yaml`, by name. A file needs only the board it selects; the shipped file
/// carries both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Boards {
    /// The grey-box 3×3 test map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Board")]
    pub v1: Option<Board>,
    /// The full 9×9 board (R26).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Board")]
    pub full: Option<Board>,
}

impl Boards {
    /// A board by name.
    pub fn get(&self, choice: MapChoice) -> Option<&Board> {
        match choice {
            MapChoice::V1 => self.v1.as_ref(),
            MapChoice::Full => self.full.as_ref(),
        }
    }
}

impl MapFile {
    /// The board `choice`, resolved with the shared settings, or `None` if the file has no
    /// such board.
    pub fn board(&self, choice: MapChoice) -> Option<MapConfig> {
        let board = self.boards.get(choice)?.clone();
        Some(MapConfig {
            board: choice,
            tiers: self.tiers.clone(),
            land_base_rate: self.land_base_rate,
            tube_lines: self.tube_lines.clone(),
            grid: board.grid,
            river: board.river,
            desirability_shifts: board.desirability_shifts,
            sets: board.sets,
            blocks: board.blocks,
            tube_stations: board.tube_stations,
            city_apartments: board.city_apartments,
        })
    }
}

/// One board in `map.yaml`: its geography.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Board {
    /// Grid size in blocks.
    pub grid: GridSettings,
    /// The river running between two rows, crossed only at bridges (§4.1). None: no river.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "River")]
    pub river: Option<River>,
    /// Desirability shifts over the years (§4.2, R16), applied at the midnight year step.
    #[serde(default)]
    pub desirability_shifts: Vec<DesirabilityShift>,
    /// Colour sets (R14, R26), keyed by set id.
    pub sets: BTreeMap<String, SetSettings>,
    /// Every block on the grid, exactly one per cell.
    pub blocks: Vec<BlockSettings>,
    /// Tube stations: spawn and despawn points (§4.3, R25).
    pub tube_stations: Vec<TubeStation>,
    /// The city's starting apartment blocks: one in each Low-tier set (R28).
    pub city_apartments: CityApartments,
}

/// The board a game is played on, resolved from `map.yaml`: the selected [`Board`] plus the
/// shared tier table and land rate. This is `config.map`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapConfig {
    /// Which board this is.
    pub board: MapChoice,
    /// Grid size in blocks.
    pub grid: GridSettings,
    /// Per-tier multipliers (§5.3).
    pub tiers: TierTable,
    /// Land rent per unit of building volume per day, before the tier multiplier (§5.3, R10).
    pub land_base_rate: Pounds,
    /// Tube lines by id (§4.3).
    pub tube_lines: BTreeMap<String, TubeLine>,
    /// The river, if any (§4.1).
    pub river: Option<River>,
    /// Desirability shifts over the years (§4.2, R16).
    pub desirability_shifts: Vec<DesirabilityShift>,
    /// Colour sets (R14, R26), keyed by set id.
    pub sets: BTreeMap<String, SetSettings>,
    /// Every block on the grid, exactly one per cell.
    pub blocks: Vec<BlockSettings>,
    /// Tube stations: spawn and despawn points (§4.3, R25).
    pub tube_stations: Vec<TubeStation>,
    /// The city's starting apartment blocks: one in each Low-tier set (R28).
    pub city_apartments: CityApartments,
}

impl MapConfig {
    /// The JSON pointer in `map.yaml` of `rest` inside this board, e.g. `/boards/full/blocks/3`.
    pub fn pointer(&self, rest: &str) -> String {
        format!("/boards/{}{rest}", self.board.key())
    }
}

/// The apartment blocks the city owns at the start of the game (R28).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CityApartments {
    /// Business type id in `businesses.yaml` the blocks are built as.
    pub business_type: String,
    /// Where each block stands: exactly one per Low-tier set.
    pub sites: Vec<CityApartmentSite>,
}

/// One city apartment block (R28).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CityApartmentSite {
    /// Block id it stands on; must be in a Low-tier set.
    pub block: String,
    /// `[x, y]` of the near corner inside the lot.
    pub position: [u32; 2],
    /// `[width, depth]` footprint; must touch the lot edge (R22).
    pub size: [u32; 2],
    /// Height in units (at most the business type's `max_height`, R58).
    #[schemars(range(min = 1))]
    pub height: u32,
}

/// Grid size in blocks (§4.1: 9×9 for the full game).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSettings {
    /// Columns.
    #[schemars(range(min = 1, max = 32))]
    pub width: u32,
    /// Rows.
    #[schemars(range(min = 1, max = 32))]
    pub height: u32,
}

/// Settings per desirability tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TierTable {
    /// Low tier.
    pub low: TierSettings,
    /// Medium tier.
    pub medium: TierSettings,
    /// High tier.
    pub high: TierSettings,
}

impl TierTable {
    /// Settings for one tier.
    pub fn get(&self, tier: Tier) -> &TierSettings {
        match tier {
            Tier::Low => &self.low,
            Tier::Medium => &self.medium,
            Tier::High => &self.high,
        }
    }
}

/// Settings for one desirability tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TierSettings {
    /// `desirability_mult(tier)` in the rent and build-cost formulas (§5.3; 0.6 / 1.0 / 1.8).
    pub desirability_mult: Ratio,
}

/// A colour set (R14, R26).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSettings {
    /// Display name.
    pub name: String,
    /// Colour band, cheapest to dearest.
    pub band: ColourBand,
}

/// One block (lot) on the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockSettings {
    /// Stable id, `snake_case`.
    pub id: String,
    /// District name shown to players.
    pub name: String,
    /// Row, 0-based from the top.
    pub row: u32,
    /// Column, 0-based from the left.
    pub col: u32,
    /// Desirability tier.
    pub tier: Tier,
    /// What the block is.
    pub kind: BlockKind,
    /// Colour set id, for buildable blocks.
    pub set: Option<String>,
    /// Minimum opening bid at auction (R23). Required for buildable blocks.
    pub land_base_price: Option<Pounds>,
    /// Which utility stands here. Required on utility blocks, refused on any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "UtilitySite")]
    pub utility: Option<UtilitySite>,
}

/// The utility a utility block holds (§6, R24). Not a set.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum UtilitySite {
    /// Electricity.
    Electricity,
    /// Gas.
    Gas,
    /// Water.
    Water,
    /// Telephone exchange, the internet from 1995 (R11).
    Telecoms,
}

impl UtilitySite {
    /// The utility it holds.
    pub const fn kind(self) -> UtilityKind {
        match self {
            UtilitySite::Electricity => UtilityKind::Electricity,
            UtilitySite::Gas => UtilityKind::Gas,
            UtilitySite::Water => UtilityKind::Water,
            UtilitySite::Telecoms => UtilityKind::Telecoms,
        }
    }
}

/// A river between two rows of blocks, crossed by road only at bridges (§4.1: "the Thames runs
/// between rows as a gap with bridges, not as a row of blocks").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct River {
    /// Display name.
    pub name: String,
    /// The river runs along the south edge of this row (0-based), so between it and the next.
    pub south_of_row: u32,
    /// Columns with a bridge over it. A board split by a river needs at least one.
    pub bridges: Vec<u32>,
}

/// One tube line (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TubeLine {
    /// Display name.
    pub name: String,
    /// Footfall weight each of its open stations adds to the block it sits on: customers
    /// arrive at a station block in proportion to the sum over its open lines.
    #[schemars(range(min = 1, max = 1000))]
    pub footfall_weight: u32,
}

/// A district's desirability shift (§4.2, R16): from `year` the block is `tier` and its
/// minimum opening bid is `land_base_price` (R23: the price tracks desirability). Applied at
/// the first midnight year step that reaches `year`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesirabilityShift {
    /// Block id.
    pub block: String,
    /// First year of the new tier.
    #[schemars(range(min = 1800, max = 2500))]
    pub year: i32,
    /// The new tier.
    pub tier: Tier,
    /// The new minimum opening bid (R23).
    pub land_base_price: Pounds,
    /// Front-page headline announcing the shift on the year step (§4.2).
    pub headline: String,
}

/// A tube station sitting on a block (§4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TubeStation {
    /// Block id the station sits on.
    pub block: String,
    /// Line id: a key of `tube_lines`.
    pub line: String,
    /// Year the line opens at this station.
    #[schemars(range(min = 1800, max = 2500))]
    pub opens: i32,
}
