//! `map.yaml` rules that JSON Schema cannot express, run on every board in the file (owner:
//! land). Pointers name the board: `/boards/full/blocks/3/set`.

use std::collections::BTreeSet;

use super::map::{CityApartmentSite, MapConfig};
use super::{Config, ConfigError, ConfigFile};
use crate::land::{BlockKind, Footprint, Position, Size, Tier};

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
    check_city_apartments(config, board, &mut errors);
    errors
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
