//! Steps for `features/customers/streets.feature`: walkers keep to the streets, cross the river
//! only on a bridge, and go home (or back to their station) between shopping windows (§4.1,
//! R22, R25, R70).
//!
//! The street and river here are worked out from the config on their own (lot size, grid,
//! river row and bridges), not through `sim_core::customers::streets`, so the checks do not
//! trust the code they check. A bridge for column `c` carries the road on the west side of that
//! column (`x = c × pitch`) over the river.

use std::collections::{BTreeMap, BTreeSet};

use cucumber::{given, then};
use sim_core::Config;
use sim_core::customers::movement::Cell;
use sim_core::customers::streets::Streets;
use sim_core::customers::{Activity, Origin, front_doors, station_cells};

use super::population::CityState;
use crate::SimWorld;

/// What the streets scenarios watch.
#[derive(Debug, Default)]
pub struct StreetsState {
    /// The scenario's streets, built once (`steps::customers`).
    pub board: Option<Streets>,
    /// Record every walker's position every tick.
    pub watching: bool,
    /// Every on-map position per shopper id, tick by tick: (second of day, cell).
    pub trail: BTreeMap<u64, Vec<(u32, Cell)>>,
    /// Where each shopper away between windows was last seen, by id.
    pub away_at: BTreeMap<u64, Cell>,
    /// The longest single move between two sightings of the same shopper (off-map gaps
    /// included: they must step back out where they left).
    pub max_move: u32,
    /// The move behind `max_move`: id, from, to, second of day.
    pub worst: Option<(u64, Cell, Cell, u32)>,
}

/// The walking grid as the config describes it, independent of the code under test.
struct Board {
    px: i32,
    py: i32,
    max_x: i32,
    max_y: i32,
    river_y: Option<i32>,
    bridges: BTreeSet<i32>,
}

impl Board {
    fn of(config: &Config) -> Self {
        let [lot_w, lot_d] = config.game.world.lot_size;
        let (px, py) = (
            i32::try_from(lot_w).unwrap() + 1,
            i32::try_from(lot_d).unwrap() + 1,
        );
        let grid = &config.map.grid;
        let river = config.map.river.as_ref();
        Board {
            px,
            py,
            max_x: i32::try_from(grid.width).unwrap() * px,
            max_y: i32::try_from(grid.height).unwrap() * py,
            river_y: river.map(|r| (i32::try_from(r.south_of_row).unwrap() + 1) * py),
            bridges: river
                .map(|r| {
                    r.bridges
                        .iter()
                        .map(|c| i32::try_from(*c).unwrap() * px)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    fn inside_lot(&self, c: Cell) -> bool {
        c.x.rem_euclid(self.px) != 0 && c.y.rem_euclid(self.py) != 0
    }

    fn on_board(&self, c: Cell) -> bool {
        (0..=self.max_x).contains(&c.x) && (0..=self.max_y).contains(&c.y)
    }

    fn bridge(&self, c: Cell) -> bool {
        self.river_y == Some(c.y) && self.bridges.contains(&c.x)
    }

    fn water(&self, c: Cell) -> bool {
        self.river_y == Some(c.y) && !self.bridge(c)
    }
}

/// Records where every walker on the map is this tick. Called by the customers steps after
/// each tick while watching.
pub fn observe(w: &mut SimWorld) {
    if !w.streets.watching {
        return;
    }
    let Some(clock) = w.city.clock.as_ref() else {
        return;
    };
    let now = clock.second_of_day();
    let s = &mut w.streets;
    for c in w.city.customers.on_map() {
        let trail = s.trail.entry(c.id.0).or_default();
        // Back out after a spell away: measured from where they went away.
        let last = trail
            .last()
            .map(|(_, at)| *at)
            .or_else(|| s.away_at.get(&c.id.0).copied());
        if let Some(last) = last {
            let moved = last.manhattan(c.at);
            if moved > s.max_move {
                s.max_move = moved;
                s.worst = Some((c.id.0, last, c.at, now));
            }
        }
        trail.push((now, c.at));
    }
    for cs in w.city.customers.parked.values() {
        for c in cs {
            if matches!(c.activity, Activity::Away { .. }) {
                s.away_at.insert(c.id.0, c.at);
                if let Some(trail) = s.trail.get_mut(&c.id.0) {
                    // Their next sighting is measured from where they went away.
                    trail.push((now, c.at));
                }
            }
        }
    }
}

fn every_sighting(w: &SimWorld) -> impl Iterator<Item = (u64, u32, Cell)> + '_ {
    w.streets
        .trail
        .iter()
        .flat_map(|(id, t)| t.iter().map(move |(s, c)| (*id, *s, *c)))
}

#[given("every walker is watched")]
fn watched(w: &mut SimWorld) {
    w.streets.watching = true;
}

#[then("no walker ever stood on the river except on a bridge")]
fn never_on_water(w: &mut SimWorld) {
    let board = Board::of(&CityState::config(w));
    assert!(board.river_y.is_some(), "the board has no river");
    assert!(!w.streets.trail.is_empty(), "nobody walked");
    let wet: Vec<_> = every_sighting(w)
        .filter(|(_, _, c)| board.water(*c))
        .take(5)
        .collect();
    assert!(wet.is_empty(), "on the river: {wet:?}");
}

#[then("no walker ever stood inside a lot")]
fn never_in_a_lot(w: &mut SimWorld) {
    let board = Board::of(&CityState::config(w));
    assert!(!w.streets.trail.is_empty(), "nobody walked");
    let inside: Vec<_> = every_sighting(w)
        .filter(|(_, _, c)| board.inside_lot(*c) || !board.on_board(*c))
        .take(5)
        .collect();
    assert!(inside.is_empty(), "off the street: {inside:?}");
}

/// The resident's sightings up to their first arrival at `shop`'s door, and the door.
fn way_to(w: &mut SimWorld, shop: &str) -> (Vec<Cell>, Cell) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let streets = Streets::for_config(&map, &config);
    let id = w.city.building(shop);
    let door = front_doors(&w.city.businesses, &map, &streets)[&id];
    let resident = w
        .city
        .spawned
        .iter()
        .find(|c| matches!(c.origin, Origin::Resident(_)))
        .expect("no resident came out")
        .id
        .0;
    let trail: Vec<Cell> = w.streets.trail[&resident].iter().map(|(_, c)| *c).collect();
    let arrived = trail
        .iter()
        .position(|c| *c == door)
        .expect("never reached the shop");
    (trail[..=arrived].to_vec(), door)
}

#[then(expr = "the resident crossed the river on a bridge on the way to {string}")]
fn crossed_on_bridge(w: &mut SimWorld, shop: String) {
    let board = Board::of(&CityState::config(w));
    let (way, _) = way_to(w, &shop);
    let river = board.river_y.expect("the board has no river");
    let (start, end) = (way[0], *way.last().unwrap());
    assert!(
        (start.y < river) != (end.y < river),
        "{shop} is not across the river: {start:?} to {end:?}"
    );
    assert!(
        way.iter().any(|c| board.bridge(*c)),
        "no bridge on the way: {way:?}"
    );
    // Between two sightings they walked a straight line of at most a few cells: none of the
    // cells in between were water either.
    for pair in way.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let crossed = (a.y - river).signum() * (b.y - river).signum() < 0;
        assert!(
            !crossed || board.bridges.contains(&a.x),
            "jumped the river: {a:?} to {b:?}"
        );
    }
}

#[then(expr = "the resident walked further to {string} than the straight-line distance")]
fn walked_further(w: &mut SimWorld, shop: String) {
    let (way, door) = way_to(w, &shop);
    let walked: u32 = way.windows(2).map(|p| p[0].manhattan(p[1])).sum();
    let straight = way[0].manhattan(door);
    assert!(
        walked > straight,
        "walked {walked}, straight line {straight}"
    );
}

#[then("every shopper away between windows is at home or at the station they arrived at")]
fn away_at_home(w: &mut SimWorld) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let streets = Streets::for_config(&map, &config);
    let year = CityState::clock(w).year();
    let stations: BTreeSet<Cell> = station_cells(&config, &map, year, &streets)
        .into_iter()
        .collect();
    let doors = front_doors(&w.city.businesses, &map, &streets);
    let away: Vec<_> = w
        .city
        .customers
        .parked
        .values()
        .flatten()
        .filter(|c| matches!(c.activity, Activity::Away { .. }))
        .collect();
    assert!(!away.is_empty(), "nobody is away between windows");
    for c in away {
        let spot = match c.origin {
            Origin::Resident(r) => {
                let home = w.city.population.residential.residents[&r].home;
                assert_eq!(c.exit, doors[&home], "{c:?}");
                c.exit
            }
            Origin::Commuter | Origin::Tourist => {
                assert!(stations.contains(&c.exit), "{c:?}");
                c.exit
            }
        };
        assert_eq!(c.at, spot, "away on the pavement: {c:?}");
    }
}

#[then("every shopper back out stepped out where they went away")]
fn back_where_they_left(w: &mut SimWorld) {
    let back: Vec<_> = w
        .city
        .customers
        .on_map()
        .filter(|c| w.streets.away_at.contains_key(&c.id.0))
        .collect();
    assert!(!back.is_empty(), "nobody came back out");
    for c in back {
        assert_eq!(c.at, w.streets.away_at[&c.id.0], "{c:?}");
        assert_eq!(
            c.at, c.exit,
            "went away somewhere other than home or the station: {c:?}"
        );
    }
}

#[then(expr = "no walker ever moved more than {int} cells in a tick")]
fn never_jumped(w: &mut SimWorld, cells: u32) {
    assert!(!w.streets.trail.is_empty(), "nobody walked");
    assert!(w.streets.max_move <= cells, "jumped: {:?}", w.streets.worst);
}
