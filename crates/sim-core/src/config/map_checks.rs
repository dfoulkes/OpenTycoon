//! `map.yaml` rules that JSON Schema cannot express, run on every board in the file (owner:
//! land). Pointers name the board: `/boards/full/blocks/3/set`.

use std::collections::BTreeSet;

use super::map::{CityApartmentSite, MapChoice, MapConfig, UtilitySite};
use super::{Config, ConfigError, ConfigFile};
use crate::land::{BlockId, BlockKind, ColourBand, Footprint, Map, Position, Size, Tier};

fn err(errors: &mut Vec<ConfigError>, pointer: String, message: String) {
    errors.push(ConfigError::new(
        ConfigFile::Map.file_name(),
        pointer,
        message,
    ));
}

/// Every rule for one board, in the context of the rest of the config.
pub fn check_board(config: &Config, board: &MapConfig) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    check_grid(board, &mut errors);
    let grid_ok = errors.is_empty();
    check_kinds(board, &mut errors);
    check_river(board, &mut errors);
    if grid_ok && board.river_fits() {
        check_roads(board, &mut errors);
    }
    check_set_sizes(board, &mut errors);
    check_stations_and_shifts(board, &mut errors);
    if board.board == MapChoice::Full {
        check_full_board(board, &mut errors);
    }
    check_city_apartments(config, board, &mut errors);
    errors
}

/// The number of blocks a set in `band` has (§4.1, R26): two at either end, three between.
pub const fn set_size(band: ColourBand) -> usize {
    match band {
        ColourBand::Brown | ColourBand::DarkBlue => 2,
        _ => 3,
    }
}

/// Utilities only on utility blocks, each at most once (§6); sets only on buildable blocks.
fn check_kinds(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let mut utilities = BTreeSet::new();
    for (i, block) in map.blocks.iter().enumerate() {
        let at = map.pointer(&format!("/blocks/{i}"));
        match (block.kind, block.utility) {
            (BlockKind::Utility, None) => err(
                errors,
                format!("{at}/utility"),
                format!("utility block {:?} needs a utility (§6)", block.id),
            ),
            (BlockKind::Utility, Some(u)) => {
                if !utilities.insert(u) {
                    err(
                        errors,
                        format!("{at}/utility"),
                        format!(
                            "a second {u:?} utility block, {:?} (§6: one each)",
                            block.id
                        ),
                    );
                }
            }
            (_, Some(_)) => err(
                errors,
                format!("{at}/utility"),
                format!(
                    "block {:?} names a utility, allowed only on a utility block",
                    block.id
                ),
            ),
            (_, None) => {}
        }
        if block.kind != BlockKind::Buildable && block.set.is_some() {
            err(
                errors,
                format!("{at}/set"),
                format!(
                    "block {:?} is not buildable, so it cannot join a set (R24: utilities are not a set)",
                    block.id
                ),
            );
        }
    }
}

/// The river lies between two rows of the grid and its bridges on real columns (§4.1).
fn check_river(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let Some(river) = &map.river else { return };
    if !map.river_fits() {
        err(
            errors,
            map.pointer("/river/south_of_row"),
            format!(
                "the river runs south of row {}, but the grid's last row is {}: it must lie between two rows",
                river.south_of_row,
                map.grid.height.saturating_sub(1)
            ),
        );
    }
    let mut seen = BTreeSet::new();
    for (i, col) in river.bridges.iter().enumerate() {
        if *col >= map.grid.width || !seen.insert(*col) {
            err(
                errors,
                map.pointer(&format!("/river/bridges/{i}")),
                format!(
                    "bridge at column {col} is outside the grid or repeated (columns 0..{})",
                    map.grid.width
                ),
            );
        }
    }
}

/// Every set's blocks are adjacent by road, and every block reaches every other (§4.1).
fn check_roads(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let land = Map::from_config(map);
    for (set, blocks) in land.sets() {
        if !land.connected_by_road(&blocks) {
            err(
                errors,
                map.pointer(&format!("/sets/{set}")),
                format!(
                    "set {set:?} is not adjacent: its blocks are not joined by road without leaving the set (§4.1)"
                ),
            );
        }
    }
    let reached = land.reachable_by_road(BlockId(0), None);
    if let Some((i, cut_off)) = land
        .blocks
        .iter()
        .enumerate()
        .find(|(i, _)| u32::try_from(*i).is_ok_and(|i| !reached.contains(&BlockId(i))))
    {
        err(
            errors,
            map.pointer(&format!("/blocks/{i}")),
            format!(
                "block {:?} cannot reach block {:?} by road; the river needs a bridge (§4.1)",
                land.blocks[0].key, cut_off.key
            ),
        );
    }
}

/// Each set has the size of its band (R14, R26).
fn check_set_sizes(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    for (set, settings) in &map.sets {
        let n = map
            .blocks
            .iter()
            .filter(|b| b.set.as_deref() == Some(set.as_str()))
            .count();
        let want = set_size(settings.band);
        if n != want {
            err(
                errors,
                map.pointer(&format!("/sets/{set}")),
                format!(
                    "set {set:?} has {n} blocks; a {:?} set has {want} (R14, R26)",
                    settings.band
                ),
            );
        }
    }
}

/// Stations on known lines (§4.3); shifts on known buildable blocks, one per block and year
/// (§4.2).
fn check_stations_and_shifts(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    for (i, station) in map.tube_stations.iter().enumerate() {
        if !map.tube_lines.contains_key(&station.line) {
            err(
                errors,
                map.pointer(&format!("/tube_stations/{i}/line")),
                format!("unknown line {:?} (not in tube_lines)", station.line),
            );
        }
    }
    let mut seen = BTreeSet::new();
    for (i, shift) in map.desirability_shifts.iter().enumerate() {
        let at = map.pointer(&format!("/desirability_shifts/{i}"));
        match map.blocks.iter().find(|b| b.id == shift.block) {
            None => err(
                errors,
                format!("{at}/block"),
                format!("unknown block {:?}", shift.block),
            ),
            Some(b) if b.kind != BlockKind::Buildable => err(
                errors,
                format!("{at}/block"),
                format!(
                    "block {:?} is not buildable; only districts shift",
                    shift.block
                ),
            ),
            Some(_) => {}
        }
        if !seen.insert((shift.block.as_str(), shift.year)) {
            err(
                errors,
                format!("{at}/year"),
                format!(
                    "block {:?} already shifts in {}; one shift per block and year",
                    shift.block, shift.year
                ),
            );
        }
    }
}

/// The full board's shape (§4.1, R26): one brown and one dark blue set, four sets in each band
/// between (26 in all), one block for each of the four utilities, and City Hall alone at the
/// centre.
fn check_full_board(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let bands = [
        ColourBand::Brown,
        ColourBand::LightBlue,
        ColourBand::Pink,
        ColourBand::Orange,
        ColourBand::Red,
        ColourBand::Yellow,
        ColourBand::Green,
        ColourBand::DarkBlue,
    ];
    for band in bands {
        let n = map.sets.values().filter(|s| s.band == band).count();
        let want = if set_size(band) == 2 { 1 } else { 4 };
        if n != want {
            err(
                errors,
                map.pointer("/sets"),
                format!("the full board has {n} {band:?} sets; R26 needs {want}"),
            );
        }
    }
    for utility in [
        UtilitySite::Electricity,
        UtilitySite::Gas,
        UtilitySite::Water,
        UtilitySite::Telecoms,
    ] {
        if !map.blocks.iter().any(|b| b.utility == Some(utility)) {
            err(
                errors,
                map.pointer("/blocks"),
                format!(
                    "no {} utility block (R26: one each for electricity, gas, water and telecoms)",
                    format!("{utility:?}").to_lowercase()
                ),
            );
        }
    }
    let specials: Vec<(usize, &super::map::BlockSettings)> = map
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == BlockKind::Special)
        .collect();
    let centre = (map.grid.height / 2, map.grid.width / 2);
    match specials.as_slice() {
        [(i, b)] if (b.row, b.col) != centre => err(
            errors,
            map.pointer(&format!("/blocks/{i}")),
            format!(
                "City Hall {:?} must stand at the centre, row {} col {} (R26)",
                b.id, centre.0, centre.1
            ),
        ),
        [_] => {}
        _ => err(
            errors,
            map.pointer("/blocks"),
            format!(
                "the full board needs exactly one City Hall (a special block) at the centre; found {} (R26)",
                specials.len()
            ),
        ),
    }
}

/// One block per cell, unique ids, known sets, prices and sets on buildable blocks, stations
/// on known blocks.
fn check_grid(map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let expected = u64::from(map.grid.width) * u64::from(map.grid.height);
    if map.blocks.len() as u64 != expected {
        err(
            errors,
            map.pointer("/blocks"),
            format!(
                "grid is {}×{} = {expected} cells but {} blocks are defined; every cell needs exactly one",
                map.grid.width,
                map.grid.height,
                map.blocks.len()
            ),
        );
    }
    let mut ids = BTreeSet::new();
    let mut cells = BTreeSet::new();
    for (i, block) in map.blocks.iter().enumerate() {
        let at = map.pointer(&format!("/blocks/{i}"));
        if !ids.insert(block.id.as_str()) {
            err(
                errors,
                format!("{at}/id"),
                format!("duplicate block id {:?}", block.id),
            );
        }
        if block.row >= map.grid.height || block.col >= map.grid.width {
            err(
                errors,
                at.clone(),
                format!(
                    "block {:?} at row {}, col {} is outside the grid",
                    block.id, block.row, block.col
                ),
            );
        } else if !cells.insert((block.row, block.col)) {
            err(
                errors,
                at.clone(),
                format!(
                    "block {:?} shares row {}, col {} with another block",
                    block.id, block.row, block.col
                ),
            );
        }
        if let Some(set) = &block.set
            && !map.sets.contains_key(set)
        {
            err(errors, format!("{at}/set"), format!("unknown set {set:?}"));
        }
        if block.kind == BlockKind::Buildable {
            if block.land_base_price.is_none() {
                err(
                    errors,
                    format!("{at}/land_base_price"),
                    format!(
                        "buildable block {:?} needs a land_base_price (R23)",
                        block.id
                    ),
                );
            }
            if block.set.is_none() {
                err(
                    errors,
                    format!("{at}/set"),
                    format!("buildable block {:?} needs a set (R14)", block.id),
                );
            }
        }
    }
    for (i, station) in map.tube_stations.iter().enumerate() {
        if !ids.contains(station.block.as_str()) {
            err(
                errors,
                map.pointer(&format!("/tube_stations/{i}/block")),
                format!("unknown block {:?}", station.block),
            );
        }
    }
}

/// `city_apartments` (R28): one block in each Low-tier set, on a known block, with street
/// frontage (R22), of a known business type and within its limits (R58).
fn check_city_apartments(config: &Config, map: &MapConfig, errors: &mut Vec<ConfigError>) {
    let apartments = &map.city_apartments;
    if !config
        .businesses
        .types
        .contains_key(&apartments.business_type)
    {
        err(
            errors,
            map.pointer("/city_apartments/business_type"),
            format!(
                "unknown business type {:?} (not in businesses.yaml)",
                apartments.business_type
            ),
        );
    }
    // A Low-tier set is a set whose blocks are all Low tier.
    let mut low_sets: BTreeSet<&str> = map.sets.keys().map(String::as_str).collect();
    for block in &map.blocks {
        if let Some(set) = &block.set
            && block.tier != Tier::Low
        {
            low_sets.remove(set.as_str());
        }
    }
    let mut covered = BTreeSet::new();
    for (i, site) in apartments.sites.iter().enumerate() {
        let at = map.pointer(&format!("/city_apartments/sites/{i}"));
        let Some(block) = map.blocks.iter().find(|b| b.id == site.block) else {
            err(
                errors,
                format!("{at}/block"),
                format!("unknown block {:?}", site.block),
            );
            continue;
        };
        match block.set.as_deref() {
            Some(set) if low_sets.contains(set) => {
                if !covered.insert(set) {
                    err(
                        errors,
                        format!("{at}/block"),
                        format!(
                            "set {set:?} already has a city apartment block (R28: one per Low-tier set)"
                        ),
                    );
                }
            }
            _ => err(
                errors,
                format!("{at}/block"),
                format!("block {:?} is not in a Low-tier set (R28)", site.block),
            ),
        }
        check_city_apartment_site(config, map, site, &at, errors);
    }
    for set in low_sets.difference(&covered) {
        err(
            errors,
            map.pointer("/city_apartments/sites"),
            format!("Low-tier set {set:?} has no city apartment block (R28: one per Low-tier set)"),
        );
    }
}

/// One `city_apartments` site: street frontage (R22) and the business type's limits (R58).
fn check_city_apartment_site(
    config: &Config,
    map: &MapConfig,
    site: &CityApartmentSite,
    at: &str,
    errors: &mut Vec<ConfigError>,
) {
    let [lot_w, lot_d] = config.game.world.lot_size;
    let kind = config
        .businesses
        .types
        .get(&map.city_apartments.business_type);
    let footprint = Footprint::at(
        Size {
            w: site.size[0],
            d: site.size[1],
        },
        Position {
            x: site.position[0],
            y: site.position[1],
        },
    );
    if !footprint.touches_edge(lot_w, lot_d) {
        err(
            errors,
            at.to_owned(),
            format!(
                "footprint {}×{} at {},{} must lie inside the {lot_w}×{lot_d} lot and touch its edge (R22)",
                site.size[0], site.size[1], site.position[0], site.position[1]
            ),
        );
    }
    if let Some(kind) = kind {
        if site.height > kind.max_height {
            err(
                errors,
                format!("{at}/height"),
                format!(
                    "{} exceeds the type's max_height {} (R58)",
                    site.height, kind.max_height
                ),
            );
        }
        if site.size[0] < kind.min_footprint[0] || site.size[1] < kind.min_footprint[1] {
            err(
                errors,
                format!("{at}/size"),
                format!(
                    "smaller than the type's min_footprint {}×{}",
                    kind.min_footprint[0], kind.min_footprint[1]
                ),
            );
        }
    }
}
