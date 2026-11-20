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
    pub fn block_at(&self, row: u32, col: u32) -> Option<BlockId> {
        if row >= self.height || col >= self.width {
            return None;
        }
        // Blocks are row-major with one per cell on a valid board; fall back to a search if
        // the board is not (config validation reports that).
        let guess = row.checked_mul(self.width)?.checked_add(col)?;
        match self.block(BlockId(guess)) {
            Some(b) if b.row == row && b.col == col => Some(BlockId(guess)),
            _ => self
                .blocks
                .iter()
                .position(|b| b.row == row && b.col == col)
                .and_then(|i| u32::try_from(i).ok())
                .map(BlockId),
        }
    }

    /// True if a road joins the two vertically adjacent rows `north` and `north + 1` at `col`:
    /// always, unless the river runs between them away from a bridge (§4.1).
    fn crossable(&self, north: u32, col: u32) -> bool {
        self.river
            .as_ref()
            .is_none_or(|r| r.south_of_row != north || r.bridges.contains(&col))
    }

    /// Blocks one road segment away: the four grid neighbours, except across the river away
    /// from a bridge (§4.1). In north, west, east, south order.
    pub fn road_neighbours(&self, id: BlockId) -> Vec<BlockId> {
        let Some(b) = self.block(id) else {
            return Vec::new();
        };
        let (row, col) = (b.row, b.col);
        let mut out = Vec::with_capacity(4);
        if row > 0 && self.crossable(row - 1, col) {
            out.extend(self.block_at(row - 1, col));
        }
        if col > 0 {
            out.extend(self.block_at(row, col - 1));
        }
        out.extend(self.block_at(row, col + 1));
        if self.crossable(row, col) {
            out.extend(self.block_at(row + 1, col));
        }
        out
    }

    /// Every block reachable by road from `from`, travelling only through blocks in `within`
    /// (all blocks if `None`). Includes `from`; empty if `from` is unknown or outside `within`.
    pub fn reachable_by_road(
        &self,
        from: BlockId,
        within: Option<&BTreeSet<BlockId>>,
    ) -> BTreeSet<BlockId> {
        let allowed = |id: &BlockId| within.is_none_or(|w| w.contains(id));
        let mut seen = BTreeSet::new();
        if self.block(from).is_none() || !allowed(&from) {
            return seen;
        }
        seen.insert(from);
        let mut frontier = vec![from];
        while let Some(at) = frontier.pop() {
            for next in self.road_neighbours(at) {
                if allowed(&next) && seen.insert(next) {
                    frontier.push(next);
                }
            }
        }
        seen
    }

    /// True if `blocks` form one road-connected group without leaving it (a set's blocks are
    /// adjacent, §4.1). Empty is connected.
    pub fn connected_by_road(&self, blocks: &[BlockId]) -> bool {
        let Some(first) = blocks.first() else {
            return true;
        };
        let group: BTreeSet<BlockId> = blocks.iter().copied().collect();
        self.reachable_by_road(*first, Some(&group)) == group
    }

    /// The blocks of a colour set, in id order. Empty for an unknown set.
    pub fn set_blocks(&self, set: &str) -> Vec<BlockId> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.set.as_deref() == Some(set))
            .filter_map(|(i, _)| u32::try_from(i).ok().map(BlockId))
            .collect()
    }

    /// True if `holder` holds the lease on every block of `set` (R14: unlocks the Hotel).
    /// False for an unknown or empty set.
    pub fn holds_full_set(&self, holder: AccountId, set: &str) -> bool {
        let blocks = self.set_blocks(set);
        !blocks.is_empty()
            && blocks.iter().all(|id| {
                self.block(*id)
                    .and_then(|b| b.lease.as_ref())
                    .is_some_and(|l| l.holder == holder)
            })
    }

    /// Every set `holder` holds in full, in set-id order.
    pub fn full_sets_held_by(&self, holder: AccountId) -> Vec<&str> {
        self.sets()
            .into_iter()
            .filter(|(_, blocks)| {
                blocks.iter().all(|id| {
                    self.block(*id)
                        .and_then(|b| b.lease.as_ref())
                        .is_some_and(|l| l.holder == holder)
                })
            })
            .map(|(set, _)| set)
            .collect()
    }

    /// The block holding a utility, if the board has one.
    pub fn utility_block(&self, kind: UtilityKind) -> Option<BlockId> {
        self.blocks
            .iter()
            .position(|b| b.utility == Some(kind))
            .and_then(|i| u32::try_from(i).ok())
            .map(BlockId)
    }

    /// Records damage to a block in `year` (the Blitz, §4.2, is raised by
    /// [`crate::events`]). The map only records it; what the damage does to buildings is the
    /// caller's to apply through [`crate::businesses`].
    pub fn damage_block(&mut self, id: BlockId, year: i32) -> Result<(), MapError> {
        let block = self
            .blocks
            .get_mut(id.0 as usize)
            .ok_or(MapError::UnknownBlock)?;
        block.damaged_in = Some(year);
        Ok(())
    }

    /// Applies the desirability shifts whose year falls in `years` (§4.2, R16, R23), in year
    /// order: each sets its block's tier and land base price. Returns what changed, with each
    /// shift's headline. Shifts are applied once, as the years go by, so a change made by an
    /// event in between (Big Bang's `desirability_change`) stands until a later shift.
    ///
    /// A shift lands on the year step whose span covers its year, the same rule as historical
    /// events (§9): the world passes `year..year + years_per_step` at each midnight step, and
    /// `..start + years_per_step` when a game is created.
    pub fn apply_desirability(
        &mut self,
        config: &MapConfig,
        years: std::ops::Range<i32>,
    ) -> Vec<TierShift> {
        let mut due: Vec<&crate::config::map::DesirabilityShift> = config
            .desirability_shifts
            .iter()
            .filter(|s| years.contains(&s.year))
            .collect();
        due.sort_by_key(|s| s.year);
        let mut changes = Vec::new();
        for shift in due {
            let Some(id) = self.block_id(&shift.block) else {
                continue;
            };
            let Some(block) = self.blocks.get_mut(id.0 as usize) else {
                continue;
            };
            let from = block.tier;
            block.tier = shift.tier;
            block.land_base_price = shift.land_base_price.money();
            changes.push(TierShift {
                block: id,
                from,
                to: shift.tier,
                headline: shift.headline.clone(),
            });
        }
        changes
    }
}

/// Blocks with an open tube station in `year` and their footfall weights, in the order the
/// stations first appear in `map.yaml` (§4.3). Blocks with no open line are left out; a line
/// missing from `tube_lines` (refused at load) adds nothing.
pub fn stations_in(map: &Map, config: &MapConfig, year: i32) -> Vec<StationWeight> {
    let mut out: Vec<StationWeight> = Vec::new();
    for station in config.tube_stations.iter().filter(|s| s.opens <= year) {
        let Some(line) = config.tube_lines.get(&station.line) else {
            continue;
        };
        let Some(block) = map.block_id(&station.block) else {
            continue;
        };
        match out.iter_mut().find(|s| s.block == block) {
            Some(s) => s.weight = s.weight.saturating_add(line.footfall_weight),
            None => out.push(StationWeight {
                block,
                weight: line.footfall_weight,
            }),
        }
    }
    out.retain(|s| s.weight > 0);
    out
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
            assert_eq!(
                map.block_at(b.row, b.col),
                Some(BlockId(u32::try_from(i).unwrap()))
            );
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
    fn shifts_land_once_on_the_step_covering_their_year() {
        let (mut map, config) = full();
        let docklands = id(&map, "docklands");
        // 1987 falls in the 1985 step (1985..1990), as historical events do (§9).
        assert_eq!(map.apply_desirability(&config, 1980..1985), vec![]);
        let shifts = map.apply_desirability(&config, 1985..1990);
        assert!(
            shifts
                .iter()
                .any(|s| s.block == docklands && s.to == Tier::High)
        );
        assert_eq!(
            map.block(docklands).unwrap().land_base_price,
            Money::from_pounds(12_000)
        );
        // An event's change in between stands: the next step does not undo it.
        map.blocks[docklands.0 as usize].tier = Tier::Medium;
        map.apply_desirability(&config, 1990..1995);
        assert_eq!(map.block(docklands).unwrap().tier, Tier::Medium);
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
