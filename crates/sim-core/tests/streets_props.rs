//! Property tests for street routing (docs/engineering.md §1; §4.1, R22, R25, R70): routes are
//! the shortest on the streets, a walk takes exactly that many cells and never leaves the
//! street, a shopper on the map never moves more than `cells_per_tick` between sightings (off
//! the map included: they step back out where they left), and the result does not depend on
//! how many threads run games side by side.
//!
//! The street here is worked out independently of `sim_core::customers::streets`: road cells
//! are the ring around every lot, the river takes the road row south of `south_of_row`, and the
//! bridge for column `c` is the road on that column's west side (`x = c × pitch`).
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use std::collections::{BTreeMap, VecDeque};

use proptest::prelude::*;
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::customers::movement::{Cell, Geometry};
use sim_core::customers::streets::Streets;
use sim_core::customers::{Activity, Customers, PhaseInputs, run_phase};
use sim_core::economy::AccountId;
use sim_core::land::{BlockId, Footprint, Map, RiverCrossing};
use sim_core::money::{Money, Rate};
use sim_core::population::{AgeBand, Class, Cohort, Population, Sex};
use sim_core::rng::seeded;
use sim_core::{Config, World};

/// The street cells of a board, independently of the code under test.
struct Oracle {
    px: i32,
    py: i32,
    max_x: i32,
    max_y: i32,
    river_y: Option<i32>,
    bridges: Vec<i32>,
}

impl Oracle {
    fn new(map: &Map, g: Geometry) -> Self {
        let (px, py) = (
            i32::try_from(g.lot_w).unwrap() + 1,
            i32::try_from(g.lot_d).unwrap() + 1,
        );
        Oracle {
            px,
            py,
            max_x: i32::try_from(map.width).unwrap() * px,
            max_y: i32::try_from(map.height).unwrap() * py,
            river_y: map
                .river
                .as_ref()
                .map(|r| (i32::try_from(r.south_of_row).unwrap() + 1) * py),
            bridges: map
                .river
                .as_ref()
                .map(|r| {
                    r.bridges
                        .iter()
                        .map(|c| i32::try_from(*c).unwrap() * px)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn street(&self, c: Cell) -> bool {
        let on = (0..=self.max_x).contains(&c.x) && (0..=self.max_y).contains(&c.y);
        let road = c.x % self.px == 0 || c.y % self.py == 0;
        let water = self.river_y == Some(c.y) && !self.bridges.contains(&c.x);
        on && road && !water
    }

    fn cells(&self) -> Vec<Cell> {
        let mut out = Vec::new();
        for y in 0..=self.max_y {
            for x in 0..=self.max_x {
                let c = Cell { x, y };
                if self.street(c) {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Breadth-first distances over street cells from `from`.
    fn bfs(&self, from: Cell) -> BTreeMap<Cell, u32> {
        let mut dist = BTreeMap::new();
        let mut queue = VecDeque::new();
        dist.insert(from, 0);
        queue.push_back(from);
        while let Some(c) = queue.pop_front() {
            let d = dist[&c];
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let n = Cell {
                    x: c.x + dx,
                    y: c.y + dy,
                };
                if self.street(n) && !dist.contains_key(&n) {
                    dist.insert(n, d + 1);
                    queue.push_back(n);
                }
            }
        }
        dist
    }
}

fn board(width: u32, height: u32, river: Option<(u32, Vec<u32>)>) -> Map {
    Map {
        width,
        height,
        blocks: Vec::new(),
        river: river.map(|(south_of_row, bridges)| RiverCrossing {
            south_of_row,
            bridges,
        }),
    }
}

fn small_board() -> impl Strategy<Value = (Map, Geometry)> {
    (2u32..5, 2u32..5, 2u32..6, 2u32..6).prop_flat_map(|(w, h, lw, ld)| {
        let river =
            proptest::option::of((0..h - 1, proptest::collection::vec(0..w, 1..=w as usize)));
        river.prop_map(move |r| {
            let r = r.map(|(row, mut b)| {
                b.sort_unstable();
                b.dedup();
                (row, b)
            });
            (
                board(w, h, r),
                Geometry {
                    lot_w: lw,
                    lot_d: ld,
                },
            )
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Routes are shortest on the streets: the route length is the breadth-first distance over
    /// street cells, and a walk follows the streets and arrives in exactly that many cells.
    #[test]
    fn routes_are_shortest_on_the_streets(
        (map, g) in small_board(),
        picks in proptest::collection::vec((any::<prop::sample::Index>(), any::<prop::sample::Index>()), 8),
        cells in 1u32..5,
    ) {
        let streets = Streets::new(&map, g);
        let oracle = Oracle::new(&map, g);
        let all = oracle.cells();
        for (a, b) in picks {
            let (from, to) = (*a.get(&all), *b.get(&all));
            // Distances to the destination: each step must close the gap by what it moved.
            let dist = oracle.bfs(to);
            let Some(&want) = dist.get(&from) else { continue };
            prop_assert_eq!(streets.route_len(from, to), want, "{:?} to {:?}", from, to);
            prop_assert_eq!(streets.from(from).route_len(to), want, "measured once from {:?}", from);
            let mut at = from;
            let mut walked = 0;
            while at != to {
                let next = streets.step(at, to, cells);
                let moved = at.manhattan(next);
                prop_assert!(moved >= 1 && moved <= cells, "{:?} to {:?}", at, next);
                prop_assert!(oracle.street(next), "off the street at {:?}", next);
                prop_assert_eq!(dist.get(&next).copied(), dist[&at].checked_sub(moved), "not on a shortest route");
                walked += moved;
                at = next;
            }
            prop_assert_eq!(walked, want);
        }
    }

    /// The shipped full board: crossing the Thames takes a bridge, and distances match the
    /// breadth-first ones.
    #[test]
    fn full_board_routes_are_shortest(
        a in any::<prop::sample::Index>(),
        b in any::<prop::sample::Index>(),
    ) {
        let config = common::shipped_config();
        let map = Map::from_config(&config.map);
        let streets = Streets::for_config(&map, &config);
        let oracle = Oracle::new(&map, streets.geometry());
        let all = oracle.cells();
        let (from, to) = (*a.get(&all), *b.get(&all));
        let want = oracle.bfs(from)[&to];
        prop_assert_eq!(streets.route_len(from, to), want);
    }
}

fn shop(config: &Config, type_id: &str, block: u32, footprint: Footprint) -> Business {
    let request = BuildRequest {
        type_id: type_id.to_owned(),
        block: BlockId(block),
        footprint,
        height: 4,
        owner: AccountId::City,
        fit_out: FitOutLevel::Medium,
    };
    Business::new(request, &config.businesses.types[type_id], Money::ZERO)
}

/// A high street on the full board: each of `blocks` gets a bakery, a grocer, a clothes shop
/// and a bar, one on each edge of the lot, well stocked.
fn high_street(config: &Config, blocks: &[u32]) -> Businesses {
    let mut businesses = Businesses::default();
    let edges = [(3, 0), (0, 3), (6, 3), (3, 6)];
    for (i, block) in blocks.iter().enumerate() {
        for (k, kind) in ["bakery", "grocery", "clothes_store", "bar"]
            .iter()
            .enumerate()
        {
            let (x, y) = edges[(i + k) % 4];
            let mut b = shop(config, kind, *block, Footprint { x, y, w: 3, d: 3 });
            let good = config.businesses.types[*kind].goods[0].clone();
            b.prices.insert(good.clone(), Rate::from_bp(10_000));
            b.stock.insert(good, 100_000);
            let id = BusinessId(businesses.next_id);
            businesses.next_id += 1;
            businesses.by_id.insert(id, b);
        }
    }
    businesses
}

/// What one walker did over a day: the longest move between two sightings, any sighting off
/// the street, and any shopper away between windows somewhere other than their exit.
#[derive(Debug, Default, PartialEq, Eq)]
struct Day {
    longest_move: u32,
    off_street: Vec<Cell>,
    away_elsewhere: Vec<(Cell, Cell)>,
    sightings: u64,
    hash: u64,
}

fn walk_a_day(config: &Config, seed: u64, visitors: u64, blocks: &[u32], residents: u32) -> Day {
    use std::hash::{Hash, Hasher};
    let map = Map::from_config(&config.map);
    let streets = Streets::for_config(&map, config);
    let oracle = Oracle::new(&map, streets.geometry());
    let mut businesses = high_street(config, blocks);
    let mut pop = Population::default();
    let who = Cohort {
        band: AgeBand(1),
        sex: Sex::Male,
        class: Class::Working,
    };
    pop.pool.insert(who, visitors);
    if residents > 0 {
        let flats = shop(
            config,
            "apartments",
            blocks[0],
            Footprint {
                x: 0,
                y: 0,
                w: 3,
                d: 3,
            },
        );
        let home = BusinessId(businesses.next_id);
        businesses.next_id += 1;
        businesses.by_id.insert(home, flats);
        for _ in 0..residents {
            pop.residential.house(who, home, 1930);
        }
    }
    let mut customers = Customers::default();
    let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
    let mut rng = seeded(seed);
    let modifiers = sim_core::events::Modifiers::default();
    let mut last: BTreeMap<u64, Cell> = BTreeMap::new();
    let mut day = Day::default();
    for _ in 0..8_640 {
        clock.advance();
        run_phase(
            &mut customers,
            PhaseInputs {
                config,
                map: &map,
                clock: &clock,
                population: &pop,
                modifiers: &modifiers,
                streets: &streets,
            },
            &mut businesses,
            &mut rng,
        );
        for c in customers.on_map() {
            day.sightings += 1;
            if let Some(before) = last.insert(c.id.0, c.at) {
                day.longest_move = day.longest_move.max(before.manhattan(c.at));
            }
            if !oracle.street(c.at) {
                day.off_street.push(c.at);
            }
        }
        for c in customers.parked.values().flatten() {
            if matches!(c.activity, Activity::Away { .. }) {
                last.insert(c.id.0, c.at);
                if c.at != c.exit {
                    day.away_elsewhere.push((c.at, c.exit));
                }
            }
        }
    }
    let mut h = sim_core::hash::StableHasher::default();
    customers.hash(&mut h);
    businesses.hash(&mut h);
    day.hash = h.finish();
    day
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(6))]

    /// A shopper on the map never moves more than `cells_per_tick` cells between two sightings,
    /// including stepping back out after a spell away, never leaves the street, and between
    /// windows waits at home or at their station.
    #[test]
    fn walkers_never_jump_and_keep_to_the_streets(
        seed in any::<u64>(),
        visitors in 20u64..120,
        blocks in proptest::collection::vec(0u32..81, 1..6),
        residents in 0u32..8,
    ) {
        let config = common::shipped_config();
        let day = walk_a_day(&config, seed, visitors, &blocks, residents);
        let cells = config.customers.movement.cells_per_tick;
        prop_assert!(day.sightings > 0);
        prop_assert!(day.longest_move <= cells, "moved {} cells in a tick", day.longest_move);
        prop_assert!(day.off_street.is_empty(), "off the street: {:?}", &day.off_street[..day.off_street.len().min(5)]);
        prop_assert!(day.away_elsewhere.is_empty(), "away on the pavement: {:?}", &day.away_elsewhere[..day.away_elsewhere.len().min(5)]);
    }
}

/// The same games give the same result whether they run one at a time or side by side on
/// several threads (the street tables are per board, never shared state).
#[test]
fn same_result_on_any_thread_count() {
    let config = common::shipped_config();
    let seeds: Vec<u64> = (1..=8).collect();
    let run = |seed: u64| {
        let mut world = World::new(config.clone(), seed, 2);
        for _ in 0..8_640 {
            world.tick();
        }
        world.state_hash()
    };
    let one: Vec<u64> = seeds.iter().map(|s| run(*s)).collect();
    for threads in [2usize, 4, 8] {
        let mut many = vec![0u64; seeds.len()];
        std::thread::scope(|scope| {
            for (chunk_seeds, chunk_out) in seeds
                .chunks(seeds.len().div_ceil(threads))
                .zip(many.chunks_mut(seeds.len().div_ceil(threads)))
            {
                let run = &run;
                scope.spawn(move || {
                    for (s, out) in chunk_seeds.iter().zip(chunk_out.iter_mut()) {
                        *out = run(*s);
                    }
                });
            }
        });
        assert_eq!(one, many, "{threads} threads");
    }
    // And the street-walk day itself, on the shipped full board.
    let days: Vec<Day> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| walk_a_day(&config, 7, 60, &[13, 58, 40], 4)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(days.windows(2).all(|d| d[0] == d[1]));
}
