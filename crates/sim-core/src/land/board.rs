//! The board as a whole: roads between blocks and the river (§4.1), colour sets and who holds
//! them (R14, R26), utility blocks (§6), tube stations and their footfall weights (§4.3), and
//! desirability that shifts over the years (§4.2, R16, R23).
//!
//! Customer spawning lives in [`crate::customers`]; this module only says which blocks have
//! open stations in a year and how much footfall each draws ([`stations_in`]).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{BlockId, Map, Tier};
use crate::config::map::MapConfig;
use crate::economy::AccountId;
use crate::utilities::UtilityKind;

/// Why a map operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    /// No such block.
    #[error("no such block")]
    UnknownBlock,
}

/// A block with open tube stations in a year, and its footfall weight: the sum of
/// `footfall_weight` over the lines open there (§4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StationWeight {
    /// The block the station sits on.
    pub block: BlockId,
    /// Relative share of arrivals at this block (> 0).
    pub weight: u32,
}

/// A desirability change applied at a year step (§4.2), for the front page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TierShift {
    /// The block.
    pub block: BlockId,
    /// Tier before.
    pub from: Tier,
    /// Tier after.
    pub to: Tier,
    /// The headline announcing it.
    pub headline: String,
}

impl Map {
    /// The block at `row`, `col`.
    pub fn block_at(&self, _row: u32, _col: u32) -> Option<BlockId> {
        None
    }

    /// Blocks one road segment away: the four grid neighbours, except across the river away
    /// from a bridge (§4.1). In north, west, east, south order.
    pub fn road_neighbours(&self, _id: BlockId) -> Vec<BlockId> {
        Vec::new()
    }

    /// Every block reachable by road from `from`, travelling only through blocks in `within`
    /// (all blocks if `None`). Includes `from`.
    pub fn reachable_by_road(
        &self,
        from: BlockId,
        _within: Option<&BTreeSet<BlockId>>,
    ) -> BTreeSet<BlockId> {
        BTreeSet::from([from])
    }

    /// True if `blocks` form one road-connected group without leaving it (a set's blocks are
    /// adjacent, §4.1). Empty is connected.
    pub fn connected_by_road(&self, _blocks: &[BlockId]) -> bool {
        true
    }

    /// The blocks of a colour set, in id order. Empty for an unknown set.
    pub fn set_blocks(&self, _set: &str) -> Vec<BlockId> {
        Vec::new()
    }

    /// True if `holder` holds the lease on every block of `set` (R14: unlocks the Hotel).
    /// False for an unknown or empty set.
    pub fn holds_full_set(&self, _holder: AccountId, _set: &str) -> bool {
        false
    }

    /// Every set `holder` holds in full, in set-id order.
    pub fn full_sets_held_by(&self, _holder: AccountId) -> Vec<&str> {
        Vec::new()
    }

    /// The block holding a utility, if the board has one.
    pub fn utility_block(&self, _kind: UtilityKind) -> Option<BlockId> {
        None
    }

    /// Records damage to a block in `year` (the Blitz, §4.2, is raised by
    /// [`crate::events`]). The map only records it; what the damage does to buildings is the
    /// caller's to apply through [`crate::businesses`].
    pub fn damage_block(&mut self, _id: BlockId, _year: i32) -> Result<(), MapError> {
        Ok(())
    }

    /// Brings every block's tier and land base price to `year` (§4.2, R16, R23): the latest
    /// desirability shift at or before `year`, else the block's starting values. Returns the
    /// blocks that changed, in id order, with the shift's headline.
    pub fn apply_desirability(&mut self, _config: &MapConfig, _year: i32) -> Vec<TierShift> {
        Vec::new()
    }
}

/// Blocks with an open tube station in `year` and their footfall weights, in the order the
/// stations first appear in `map.yaml` (§4.3). Blocks with no open line are left out.
pub fn stations_in(_map: &Map, _config: &MapConfig, _year: i32) -> Vec<StationWeight> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::land::Lease;
    use crate::money::Money;

    /// The shipped full board, whatever `map` selects.
    #[allow(clippy::expect_used, reason = "test helper")]
    fn full() -> (Map, MapConfig) {
        let file: crate::config::MapFile =
            serde_yaml_ng::from_str(include_str!("../../../../config/map.yaml"))
                .expect("map.yaml parses");
        let config = file
            .board(crate::config::MapChoice::Full)
            .expect("a full board");
        (Map::from_config(&config), config)
    }

    fn id(map: &Map, key: &str) -> BlockId {
        map.block_id(key).unwrap()
    }

    #[test]
    fn block_at_finds_every_cell() {
        let (map, _) = full();
        for (i, b) in map.blocks.iter().enumerate() {
            assert_eq!(map.block_at(b.row, b.col), Some(BlockId(i as u32)));
        }
        assert_eq!(map.block_at(9, 0), None);
    }

    #[test]
    fn the_river_is_crossed_only_at_bridges() {
        let (map, _) = full();
        // Whitechapel (5,7) and Old Kent Road (6,7): Tower Bridge.
        assert!(
            map.road_neighbours(id(&map, "whitechapel"))
                .contains(&id(&map, "old_kent_road"))
        );
        // Tower Hill (5,6) and Bermondsey (6,6): no bridge at column 6.
        assert!(
            !map.road_neighbours(id(&map, "tower_hill"))
                .contains(&id(&map, "bermondsey"))
        );
        // Corners have two neighbours.
        assert_eq!(map.road_neighbours(id(&map, "dollis_hill")).len(), 2);
    }

    #[test]
    fn a_set_is_held_only_with_every_lease() {
        let (mut map, _) = full();
        let p = AccountId::Player(crate::economy::PlayerId(0));
        let q = AccountId::Player(crate::economy::PlayerId(1));
        assert!(!map.holds_full_set(p, "mayfair"));
        map.grant_lease(id(&map, "mayfair"), Lease::new(p, 1930, 30, Money::ZERO));
        assert!(!map.holds_full_set(p, "mayfair"));
        map.grant_lease(id(&map, "belgravia"), Lease::new(q, 1930, 30, Money::ZERO));
        assert!(!map.holds_full_set(p, "mayfair"));
        map.grant_lease(id(&map, "belgravia"), Lease::new(p, 1930, 30, Money::ZERO));
        assert!(map.holds_full_set(p, "mayfair"));
        assert_eq!(map.full_sets_held_by(p), vec!["mayfair"]);
        assert!(!map.holds_full_set(p, "nowhere"));
    }

    #[test]
    fn each_utility_has_its_block() {
        let (map, _) = full();
        assert_eq!(
            map.utility_block(UtilityKind::Electricity),
            Some(id(&map, "battersea_power_station"))
        );
        assert_eq!(
            map.utility_block(UtilityKind::Telecoms),
            Some(id(&map, "dollis_hill"))
        );
    }

    #[test]
    fn damage_is_recorded_and_unknown_blocks_refused() {
        let (mut map, _) = full();
        let b = id(&map, "stepney");
        assert_eq!(map.damage_block(b, 1940), Ok(()));
        assert_eq!(map.block(b).unwrap().damaged_in, Some(1940));
        assert_eq!(
            map.damage_block(BlockId(999), 1940),
            Err(MapError::UnknownBlock)
        );
    }

    #[test]
    fn desirability_is_a_function_of_the_year() {
        let (mut map, config) = full();
        let docklands = id(&map, "docklands");
        let shifts = map.apply_desirability(&config, 1990);
        assert!(
            shifts
                .iter()
                .any(|s| s.block == docklands && s.to == Tier::High)
        );
        assert_eq!(
            map.block(docklands).unwrap().land_base_price,
            Money::from_pounds(12_000)
        );
        // Applying the same year again changes nothing and says nothing.
        assert!(map.apply_desirability(&config, 1990).is_empty());
        // Going back restores the starting values.
        map.apply_desirability(&config, 1930);
        assert_eq!(map.block(docklands).unwrap().tier, Tier::Low);
        assert_eq!(
            map.block(docklands).unwrap().land_base_price,
            Money::from_pounds(1_500)
        );
    }

    #[test]
    fn stations_sum_their_open_lines() {
        let (map, config) = full();
        let weight = |key: &str, year: i32| {
            stations_in(&map, &config, year)
                .iter()
                .find(|s| s.block == id(&map, key))
                .map_or(0, |s| s.weight)
        };
        assert_eq!(weight("mayfair", 1930), 2);
        assert_eq!(weight("mayfair", 2025), 6);
        assert_eq!(weight("hackney", 2050), 0);
    }
}
