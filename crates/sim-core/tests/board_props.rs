//! Board invariants (§4.1, R14, R26): set membership partitions the buildable blocks, and every
//! block reaches every other by road whenever the river has a bridge. Checked on both shipped
//! boards and, by proptest, on random grids.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs,
    reason = "tests"
)]

mod common;

use std::collections::BTreeSet;

use proptest::prelude::*;
use sim_core::economy::AccountId;
use sim_core::land::{Block, BlockId, BlockKind, Map, RiverCrossing, Tier};
use sim_core::money::Money;

fn shipped_boards() -> Vec<Map> {
    ["v1", "full"]
        .into_iter()
        .map(|board| {
            let mut sources = common::shipped_sources();
            common::set_key(&mut sources, "map.yaml", "/map", board);
            Map::from_config(&sim_core::Config::load(&sources).unwrap().map)
        })
        .collect()
}

/// A `w × h` map with `labels[i]` as block i's set (`None`: a utility block) and an optional
/// river south of `river_row` with `bridges`.
fn grid(w: u32, h: u32, labels: &[Option<u8>], river: Option<(u32, Vec<u32>)>) -> Map {
    let blocks = (0..w * h)
        .map(|i| {
            let set = labels[i as usize].map(|l| format!("set_{l}"));
            Block {
                key: format!("b{i}"),
                name: format!("Block {i}"),
                row: i / w,
                col: i % w,
                tier: Tier::Low,
                kind: if set.is_some() {
                    BlockKind::Buildable
                } else {
                    BlockKind::Utility
                },
                set,
                land_base_price: Money::from_pounds(1),
                lease: None,
                utility: None,
                damaged_in: None,
            }
        })
        .collect();
    Map {
        width: w,
        height: h,
        blocks,
        river: river.map(|(south_of_row, bridges)| RiverCrossing {
            south_of_row,
            bridges,
        }),
    }
}

/// Every buildable block is in exactly one set, and nothing else is in any.
fn assert_partition(map: &Map) {
    let mut seen = BTreeSet::new();
    for (set, blocks) in map.sets() {
        assert!(!blocks.is_empty(), "empty set {set}");
        for b in blocks {
            assert!(seen.insert(b), "{b:?} is in two sets");
            assert_eq!(map.block(b).unwrap().set.as_deref(), Some(set));
        }
    }
    let buildable: BTreeSet<BlockId> = (0..map.blocks.len())
        .map(|i| BlockId(u32::try_from(i).unwrap()))
        .filter(|id| map.block(*id).unwrap().kind == BlockKind::Buildable)
        .collect();
    assert_eq!(seen, buildable);
}

fn all_reach_all(map: &Map) -> bool {
    let n = map.blocks.len();
    (0..n).all(|i| {
        map.reachable_by_road(BlockId(u32::try_from(i).unwrap()), None)
            .len()
            == n
    })
}

#[test]
fn shipped_boards_partition_their_buildable_blocks_into_sets() {
    for map in shipped_boards() {
        assert_partition(&map);
    }
}

#[test]
fn every_shipped_block_reaches_every_other_by_road() {
    for map in shipped_boards() {
        assert!(all_reach_all(&map), "{}×{}", map.width, map.height);
    }
}

#[test]
fn road_neighbours_are_symmetric_on_the_full_board() {
    let map = &shipped_boards()[1];
    for i in 0..map.blocks.len() {
        let a = BlockId(u32::try_from(i).unwrap());
        for b in map.road_neighbours(a) {
            assert!(map.road_neighbours(b).contains(&a), "{a:?} -> {b:?}");
        }
    }
}

fn arb_grid() -> impl Strategy<Value = (u32, u32, Vec<Option<u8>>)> {
    (1u32..=9, 1u32..=9).prop_flat_map(|(w, h)| {
        (
            Just(w),
            Just(h),
            prop::collection::vec(prop::option::weighted(0.9, 0u8..12), (w * h) as usize),
        )
    })
}

proptest! {
    #[test]
    fn set_membership_is_a_partition((w, h, labels) in arb_grid(), lessee in 0u8..3) {
        let mut map = grid(w, h, &labels, None);
        // Leases change hands; membership never does.
        for b in map.blocks.iter_mut().step_by(2) {
            b.lease = Some(sim_core::land::Lease::new(AccountId::Player(sim_core::economy::PlayerId(lessee)), 1930, 30, Money::ZERO));
        }
        assert_partition(&map);
    }

    #[test]
    fn every_block_reaches_every_other_when_the_river_has_a_bridge(
        (w, h, labels) in arb_grid(),
        river_row in 0u32..8,
        bridges in prop::collection::btree_set(0u32..9, 1..4),
    ) {
        let river = (river_row + 1 < h).then(|| (river_row, bridges.into_iter().map(|b| b % w).collect()));
        let map = grid(w, h, &labels, river);
        prop_assert!(all_reach_all(&map));
    }

    #[test]
    fn a_river_with_no_bridge_splits_the_board((w, h) in (1u32..=9, 2u32..=9), river_row in 0u32..8) {
        let row = river_row % (h - 1);
        let map = grid(w, h, &vec![Some(0); (w * h) as usize], Some((row, vec![])));
        let north = map.reachable_by_road(BlockId(0), None);
        prop_assert_eq!(north.len() as u32, (row + 1) * w);
    }
}
