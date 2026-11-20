//! Steps for the full board (`features/land/board.feature`, `sets.feature`,
//! `desirability.feature`, `tube.feature`): shape and validation (R26), set holding (R14),
//! desirability shifts (R16, R23) and tube footfall (§4.3). Game-level steps share the game in
//! `steps::auctions`.

use std::collections::{BTreeMap, BTreeSet};

use cucumber::{gherkin::Step, given, then, when};
use sim_core::World;
use sim_core::land::{
    BlockId, BlockKind, ColourBand, Footprint, Lease, Map, Tier, land_rent_per_day, stations_in,
    volume,
};
use sim_core::money::Money;
use sim_core::rng::seeded;
use sim_core::utilities::UtilityKind;
use sim_core::{Config, customers};

use crate::SimWorld;
use crate::steps::auctions::{account, game, game_mut};
use crate::steps::land::block_named;

/// A map outside any game, brought to a year.
#[derive(Debug, Default)]
pub struct BoardState {
    // Boxed: the scenario world lives on the harness's stack.
    map: Option<Box<Map>>,
    config: Option<Box<Config>>,
    arrivals: Vec<u64>,
    year: i32,
    weights: Vec<u32>,
}

fn band(name: &str) -> ColourBand {
    serde_json::from_value(serde_json::Value::String(name.into())).unwrap()
}

fn tier(name: &str) -> Tier {
    serde_json::from_value(serde_json::Value::String(name.to_lowercase())).unwrap()
}

fn pounds(text: &str) -> Money {
    let text = text.trim_start_matches('£').replace(',', "");
    let (whole, pence) = text.split_once('.').unwrap_or((&text, "0"));
    let pence = format!("{pence:0<2}");
    Money::from_pence(whole.parse::<i64>().unwrap() * 100 + pence[..2].parse::<i64>().unwrap())
}

/// The map under test: the stand-alone board if there is one, else the game's.
fn map(w: &SimWorld) -> &Map {
    match &w.board.map {
        Some(map) => map,
        None => game(w).map(),
    }
}

fn block_by_name(map: &Map, name: &str) -> BlockId {
    let i = map
        .blocks
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("no block named {name:?}"));
    BlockId(u32::try_from(i).unwrap())
}

fn loaded(w: &mut SimWorld) -> Config {
    w.config.load_ok()
}

// ---- shape -------------------------------------------------------------------------------

#[then(expr = "the board is {int} blocks wide and {int} blocks deep")]
fn board_size(w: &mut SimWorld, width: u32, height: u32) {
    let map = Map::from_config(&loaded(w).map);
    assert_eq!((map.width, map.height), (width, height));
    assert_eq!(map.blocks.len(), (width * height) as usize);
}

#[then(expr = "the board has {int} colour sets")]
fn set_count(w: &mut SimWorld, n: usize) {
    let map = Map::from_config(&loaded(w).map);
    assert_eq!(map.sets().len(), n);
}

#[then("the board has these sets per band:")]
fn sets_per_band(w: &mut SimWorld, step: &Step) {
    let config = loaded(w);
    let map = Map::from_config(&config.map);
    let mut by_band: BTreeMap<ColourBand, Vec<usize>> = BTreeMap::new();
    for (set, blocks) in map.sets() {
        by_band
            .entry(config.map.sets[set].band)
            .or_default()
            .push(blocks.len());
    }
    for row in step.table.as_ref().unwrap().rows.iter().skip(1) {
        let sizes = &by_band[&band(&row[0])];
        assert_eq!(sizes.len(), row[1].parse::<usize>().unwrap(), "{}", row[0]);
        let per: usize = row[2].parse().unwrap();
        assert!(sizes.iter().all(|s| *s == per), "{}: {sizes:?}", row[0]);
    }
}

#[then("the board has one utility block each for electricity, gas, water and telecoms")]
fn utilities(w: &mut SimWorld) {
    let map = Map::from_config(&loaded(w).map);
    let utility_blocks: Vec<_> = map
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Utility)
        .collect();
    assert_eq!(utility_blocks.len(), 4);
    for kind in UtilityKind::ALL {
        let id = map
            .utility_block(kind)
            .unwrap_or_else(|| panic!("no {kind:?} block"));
        let block = map.block(id).unwrap();
        assert_eq!(block.kind, BlockKind::Utility);
        assert!(block.set.is_none(), "utilities are not a set (R24)");
    }
}

#[then(expr = "{string} stands at the centre and cannot be built on")]
fn city_hall(w: &mut SimWorld, name: String) {
    let map = Map::from_config(&loaded(w).map);
    let id = block_by_name(&map, &name);
    let block = map.block(id).unwrap();
    assert_eq!((block.row, block.col), (map.height / 2, map.width / 2));
    assert_eq!(block.kind, BlockKind::Special);
    assert!(!map.is_open_for_bids(id));
    let specials = map
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Special)
        .count();
    assert_eq!(specials, 1);
}

#[then("every block is a real district, a utility or City Hall")]
fn districts(w: &mut SimWorld) {
    let map = Map::from_config(&loaded(w).map);
    let names: BTreeSet<&str> = map.blocks.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names.len(), 81, "district names are unique");
    // The §4.2 starter districts on the board.
    for name in [
        "Mayfair",
        "Belgravia",
        "Knightsbridge",
        "Kensington",
        "Chelsea",
        "St James's",
        "Marylebone",
        "Covent Garden",
        "Hampstead",
        "Soho",
        "Holborn",
        "Bloomsbury",
        "Islington",
        "Clerkenwell",
        "Fitzrovia",
        "Southwark",
        "Camden",
        "Notting Hill",
        "Fulham",
        "Greenwich",
        "Richmond",
        "Whitechapel",
        "Bethnal Green",
        "Hackney",
        "Shoreditch",
        "Bermondsey",
        "Peckham",
        "Brixton",
        "Stepney",
        "Deptford",
        "Old Kent Road",
        "Elephant & Castle",
        "Lambeth",
        "Battersea",
        "The City",
        "Docklands",
    ] {
        assert!(names.contains(name), "{name} is missing");
    }
}

#[then("every colour set's blocks are connected by road without leaving the set")]
fn sets_connected(w: &mut SimWorld) {
    let map = map(w);
    for (set, blocks) in map.sets() {
        assert!(map.connected_by_road(&blocks), "set {set} is not adjacent");
    }
}

#[then("every block reaches every other block by road")]
fn all_reachable(w: &mut SimWorld) {
    let map = map(w);
    let reached = map.reachable_by_road(BlockId(0), None);
    assert_eq!(reached.len(), map.blocks.len());
}

#[then(expr = "{string} and {string} are joined by a bridge")]
fn bridged(w: &mut SimWorld, a: String, b: String) {
    let map = map(w);
    let (a, b) = (block_by_name(map, &a), block_by_name(map, &b));
    assert!(map.road_neighbours(a).contains(&b));
    assert!(map.road_neighbours(b).contains(&a));
}

#[then(expr = "{string} and {string} are not joined: no bridge there")]
fn not_bridged(w: &mut SimWorld, a: String, b: String) {
    let map = map(w);
    let (ia, ib) = (block_by_name(map, &a), block_by_name(map, &b));
    let (ba, bb) = (map.block(ia).unwrap(), map.block(ib).unwrap());
    assert_eq!(
        ba.col, bb.col,
        "{a} and {b} are grid neighbours across the river"
    );
    assert_eq!(ba.row + 1, bb.row);
    assert!(!map.road_neighbours(ia).contains(&ib));
}

fn price(map: &Map, id: BlockId) -> i64 {
    map.block(id).unwrap().land_base_price.pence()
}

#[then("the mean land base price falls with each ring out from City Hall")]
fn rings(w: &mut SimWorld) {
    let map = map(w);
    let (cr, cc) = (map.height / 2, map.width / 2);
    let mut rings: BTreeMap<u32, Vec<i64>> = BTreeMap::new();
    for b in map.blocks.iter().filter(|b| b.kind == BlockKind::Buildable) {
        let ring = b.row.abs_diff(cr).max(b.col.abs_diff(cc));
        rings
            .entry(ring)
            .or_default()
            .push(b.land_base_price.pence());
    }
    let means: Vec<i64> = rings
        .values()
        .map(|p| p.iter().sum::<i64>() / i64::try_from(p.len()).unwrap())
        .collect();
    assert!(
        means.windows(2).all(|m| m[0] > m[1]),
        "ring means {means:?}"
    );
}

#[then(
    "every block in a dearer band has a higher land base price than any block in a cheaper band"
)]
fn bands_ordered(w: &mut SimWorld) {
    let config = w.config.load_ok();
    let map = map(w);
    let mut by_band: BTreeMap<ColourBand, (i64, i64)> = BTreeMap::new();
    for (i, b) in map.blocks.iter().enumerate() {
        let Some(set) = &b.set else { continue };
        let p = price(map, BlockId(u32::try_from(i).unwrap()));
        let e = by_band
            .entry(config.map.sets[set].band)
            .or_insert((i64::MAX, i64::MIN));
        *e = (e.0.min(p), e.1.max(p));
    }
    let ranges: Vec<_> = by_band.values().collect();
    assert!(
        ranges.windows(2).all(|r| r[0].1 < r[1].0),
        "bands overlap: {by_band:?}"
    );
}

#[then(expr = "{string} has the highest land base price and {string} the lowest")]
fn extremes(w: &mut SimWorld, high: String, low: String) {
    let map = map(w);
    let buildable: Vec<_> = (0..map.blocks.len())
        .map(|i| BlockId(u32::try_from(i).unwrap()))
        .filter(|id| map.block(*id).unwrap().kind == BlockKind::Buildable)
        .collect();
    let max = buildable.iter().max_by_key(|id| price(map, **id)).unwrap();
    let min = buildable.iter().min_by_key(|id| price(map, **id)).unwrap();
    assert_eq!(map.block(*max).unwrap().name, high);
    assert_eq!(map.block(*min).unwrap().name, low);
}

fn bands_have_tier(w: &mut SimWorld, bands: &[&str], expected: Tier) {
    let config = w.config.load_ok();
    let map = map(w);
    for b in &map.blocks {
        let Some(set) = &b.set else { continue };
        let band = config.map.sets[set].band;
        if bands.iter().any(|n| self::band(n) == band) {
            assert_eq!(b.tier, expected, "{}", b.name);
        }
    }
}

#[then("brown, light blue and pink blocks are Low tier")]
fn low_bands(w: &mut SimWorld) {
    bands_have_tier(w, &["brown", "light_blue", "pink"], Tier::Low);
}

#[then("orange, red and yellow blocks are Medium tier")]
fn medium_bands(w: &mut SimWorld) {
    bands_have_tier(w, &["orange", "red", "yellow"], Tier::Medium);
}

#[then("green and dark blue blocks are High tier")]
fn high_bands(w: &mut SimWorld) {
    bands_have_tier(w, &["green", "dark_blue"], Tier::High);
}

// ---- broken boards -----------------------------------------------------------------------

/// Edits the full board in the scenario's `map.yaml`.
fn edit_full_board(w: &mut SimWorld, edit: impl FnOnce(&mut serde_yaml_ng::Value)) {
    let sources = w.config.sources_mut();
    let text = sources.get("map.yaml").unwrap().to_owned();
    let mut doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    edit(&mut doc["boards"]["full"]);
    sources.insert("map.yaml", serde_yaml_ng::to_string(&doc).unwrap());
}

fn block_entry<'a>(
    board: &'a mut serde_yaml_ng::Value,
    name: &str,
) -> &'a mut serde_yaml_ng::Value {
    board["blocks"]
        .as_sequence_mut()
        .unwrap()
        .iter_mut()
        .find(|b| b["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no block named {name:?}"))
}

#[given(regex = r"^the full board with (.+)$")]
fn broken_board(w: &mut SimWorld, change: String) {
    use serde_yaml_ng::Value;
    let quoted: Vec<String> = change
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    edit_full_board(w, |board| {
        if change.ends_with("set") && change.contains("moved to the") {
            block_entry(board, &quoted[0])["set"] = Value::from(quoted[1].as_str());
        } else if change == "the gas works holding water instead" {
            block_entry(board, "East Greenwich Gasworks")["utility"] = Value::from("water");
        } else if change.contains("made a second City Hall") {
            let block = block_entry(board, &quoted[0]).as_mapping_mut().unwrap();
            block.insert("kind".into(), "special".into());
            block.remove("set");
            block.remove("land_base_price");
        } else if change == "no bridges over the Thames" {
            board["river"]["bridges"] = Value::Sequence(vec![]);
        } else if change.starts_with("a station on the") {
            board["tube_stations"].as_sequence_mut().unwrap().push(
                serde_yaml_ng::from_str("{ block: hackney, line: overground, opens: 2010 }")
                    .unwrap(),
            );
        } else if change.starts_with("a desirability shift for") {
            board["desirability_shifts"]
                .as_sequence_mut()
                .unwrap()
                .push(
                serde_yaml_ng::from_str(
                    "{ block: atlantis, year: 2000, tier: high, land_base_price: 1, headline: x }",
                )
                .unwrap(),
            );
        } else if change.starts_with("a utility named on") {
            block_entry(board, &quoted[0])["utility"] = Value::from("water");
        } else {
            panic!("unknown change {change:?}");
        }
    });
}

// ---- sets --------------------------------------------------------------------------------

fn names(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[given(regex = r#"^player (\d+) holds the leases on ((?:"[^"]+"(?:, | and )?)+)$"#)]
fn holds_leases(w: &mut SimWorld, player: u8, blocks: String) {
    let year = game(w).clock().year();
    let term = game(w).config().game.leases.term_years;
    for name in names(&blocks) {
        let id = block_named(game(w), &name);
        game_mut(w).map_mut().blocks[id.0 as usize].lease =
            Some(Lease::new(account(player), year, term, Money::ZERO));
    }
}

#[given(regex = r#"^the lease on "(.+)" started (\d+) years ago$"#)]
fn lease_started(w: &mut SimWorld, name: String, years: i32) {
    let year = game(w).clock().year();
    let id = block_named(game(w), &name);
    let block = &mut game_mut(w).map_mut().blocks[id.0 as usize];
    block.lease.as_mut().unwrap().start_year = year - years;
}

#[then(regex = r#"^player (\d+) (holds|does not hold) the "(.+)" set in full$"#)]
fn holds_set(w: &mut SimWorld, player: u8, holds: String, set: String) {
    let held = game(w).map().holds_full_set(account(player), &set);
    assert_eq!(held, holds == "holds", "player {player} {holds} {set}");
}

#[then(regex = r"^player (\d+) holds exactly these sets in full: (.+)$")]
fn exactly_these(w: &mut SimWorld, player: u8, sets: String) {
    let held: Vec<String> = game(w)
        .map()
        .full_sets_held_by(account(player))
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(held, names(&sets));
}

// ---- desirability ------------------------------------------------------------------------

#[given(expr = "the full board brought to {int}")]
fn board_at(w: &mut SimWorld, year: i32) {
    let config = w.config.load_ok();
    let mut map = Map::from_config(&config.map);
    let span = i32::try_from(config.game.clock.years_per_day).unwrap();
    map.apply_desirability(&config.map, i32::MIN..year + span);
    w.board.year = year;
    w.board.map = Some(Box::new(map));
    w.board.config = Some(Box::new(config));
}

#[given("the full board")]
fn full_board(w: &mut SimWorld) {
    board_at(w, 1930);
}

#[when(expr = "the board's year steps to {int}")]
fn year_steps(w: &mut SimWorld, year: i32) {
    let config = w.board.config.clone().unwrap();
    let span = i32::try_from(config.game.clock.years_per_day).unwrap();
    let from = w.board.year + span;
    w.board
        .map
        .as_mut()
        .unwrap()
        .apply_desirability(&config.map, from..year + span);
    w.board.year = year;
}

#[then(regex = r#"^"(.+)" is (Low|Medium|High) tier$"#)]
fn is_tier(w: &mut SimWorld, name: String, expected: String) {
    let map = w.board.map.as_ref().unwrap();
    let block = map.block(block_by_name(map, &name)).unwrap();
    assert_eq!(block.tier, tier(&expected), "{name}");
}

#[then(regex = r#"^"(.+)" is (Low|Medium|High) tier in the game$"#)]
fn is_tier_in_game(w: &mut SimWorld, name: String, expected: String) {
    let world = game(w);
    let block = world.map().block(block_named(world, &name)).unwrap();
    assert_eq!(block.tier, tier(&expected), "{name}");
}

#[then(regex = r#"^the land base price of "(.+)" is (£[\d,.]+)$"#)]
fn base_price(w: &mut SimWorld, name: String, expected: String) {
    let map = w.board.map.as_ref().unwrap();
    let block = map.block(block_by_name(map, &name)).unwrap();
    assert_eq!(block.land_base_price, pounds(&expected));
}

#[then(regex = r#"^the game's front page for (\d+) carries "(.+)"$"#)]
fn game_front_page(w: &mut SimWorld, year: i32, headline: String) {
    let page = &game(w).events().front_page;
    assert_eq!(page.year, year);
    assert!(page.headlines.contains(&headline), "{:?}", page.headlines);
}

#[then(regex = r#"^a 3x3 building 10 high on "(.+)" would pay (£[\d.]+) land rent a day$"#)]
fn would_pay(w: &mut SimWorld, name: String, expected: String) {
    let world: &World = game(w);
    let block = world.map().block(block_named(world, &name)).unwrap();
    let fp = Footprint {
        x: 0,
        y: 0,
        w: 3,
        d: 3,
    };
    let rent = land_rent_per_day(
        volume(10, fp),
        world.config().map.land_base_rate.money(),
        world
            .config()
            .map
            .tiers
            .get(block.tier)
            .desirability_mult
            .rate(),
    );
    assert_eq!(rent, pounds(&expected));
}

// ---- tube --------------------------------------------------------------------------------

fn weight_in(w: &SimWorld, name: &str, year: i32) -> u32 {
    let map = w.board.map.as_ref().unwrap();
    let config = w.board.config.as_ref().unwrap();
    let id = block_by_name(map, name);
    stations_in(map, &config.map, year)
        .iter()
        .find(|s| s.block == id)
        .map_or(0, |s| s.weight)
}

#[then(regex = r#"^the footfall weight of "(.+)" is (\d+) in (\d+)$"#)]
fn footfall(w: &mut SimWorld, name: String, weight: u32, year: i32) {
    assert_eq!(weight_in(w, &name, year), weight, "{name} in {year}");
}

#[then(regex = r#"^in (\d+) "(.+)" has no station$"#)]
fn no_station(w: &mut SimWorld, year: i32, name: String) {
    let map = w.board.map.as_ref().unwrap();
    let config = w.board.config.as_ref().unwrap();
    let id = block_by_name(map, &name);
    assert!(
        stations_in(map, &config.map, year)
            .iter()
            .all(|s| s.block != id)
    );
}

#[then(regex = r"^in (\d+) every station block has a weight of at least 1$")]
fn weights_positive(w: &mut SimWorld, year: i32) {
    let map = w.board.map.as_ref().unwrap();
    let config = w.board.config.as_ref().unwrap();
    let stations = stations_in(map, &config.map, year);
    assert_ne!(stations.len(), 0);
    assert!(stations.iter().all(|s| s.weight >= 1));
}

#[given(regex = r"^stations with footfall weights (\d+) and (\d+)$")]
fn station_weights(w: &mut SimWorld, a: u32, b: u32) {
    w.board.weights = vec![a, 0, b];
}

#[when(regex = r"^(\d+) visitors pick an arrival station$")]
fn visitors_pick(w: &mut SimWorld, n: u64) {
    let mut rng = seeded(7);
    let mut counts = vec![0u64; w.board.weights.len()];
    for _ in 0..n {
        let k = customers::pick_station(&w.board.weights, &mut rng).expect("a station");
        counts[k] += 1;
    }
    w.board.arrivals = counts;
}

#[then(regex = r"^about (\d+) arrive at the first and (\d+) at the second, within (\d+)%$")]
fn arrivals_split(w: &mut SimWorld, first: u64, second: u64, pct: u64) {
    let got = &w.board.arrivals;
    for (expected, actual) in [(first, got[0]), (second, got[2])] {
        assert!(
            actual.abs_diff(expected) * 100 <= expected * pct,
            "expected about {expected}, got {actual} ({got:?})"
        );
    }
}

#[then("none arrive at a station of weight 0")]
fn none_at_zero(w: &mut SimWorld) {
    assert_eq!(w.board.arrivals[1], 0);
}

// ---- the full board as the real game ------------------------------------------------------

#[then(expr = "the game's board is {int} blocks wide and {int} blocks deep")]
fn game_board_size(w: &mut SimWorld, width: u32, height: u32) {
    let map = game(w).map();
    assert_eq!((map.width, map.height), (width, height));
    assert_eq!(map.blocks.len(), (width * height) as usize);
}

#[then(expr = "{string} is on the game's board")]
fn on_game_board(w: &mut SimWorld, name: String) {
    block_named(game(w), &name);
}

#[then(regex = r"^these blocks were damaged in (\d+):$")]
fn damaged_in(w: &mut SimWorld, year: i32, step: &Step) {
    let world = game(w);
    for row in step.table.as_ref().unwrap().rows.iter().skip(1) {
        let block = world.map().block(block_named(world, &row[0])).unwrap();
        assert_eq!(block.damaged_in, Some(year), "{}", row[0]);
    }
}

#[then(expr = "{string} was not damaged")]
fn not_damaged(w: &mut SimWorld, name: String) {
    let world = game(w);
    let block = world.map().block(block_named(world, &name)).unwrap();
    assert_eq!(block.damaged_in, None);
}

#[then(expr = "the {string} event acts on {string}")]
fn event_acts_on(w: &mut SimWorld, event: String, name: String) {
    use sim_core::config::events::Effect;
    let config = w.config.load_ok();
    let map = Map::from_config(&config.map);
    let key = &map.block(block_by_name(&map, &name)).unwrap().key;
    let event = config
        .events
        .historical
        .iter()
        .find(|e| e.id == event)
        .unwrap_or_else(|| panic!("no event {event}"));
    let named = event.effects.iter().any(|e| match e {
        Effect::DestroyBuildings { blocks, .. }
        | Effect::DesirabilityChange { blocks, .. }
        | Effect::Footfall { blocks, .. } => blocks.contains(key),
        _ => false,
    });
    assert!(named, "{} does not act on {name}", event.id);
}

#[given("the shipped config on the v1 map")]
fn shipped_on_v1(w: &mut SimWorld) {
    let mut sources = crate::common::shipped_sources();
    crate::common::set_key(&mut sources, "map.yaml", "/map", "v1");
    w.config.sources = Some(sources);
    w.clock.reset();
}
