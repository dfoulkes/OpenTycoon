//! Land: the grid, blocks (lots), colour sets, footprints and leases (§4, §5.2, §5.3).
//!
//! Owner boundary: map construction from `map.yaml`, footprint placement rules (R22), set
//! completion (R14), lease terms and expiry (R9, R33), and the land-rent formula (R10).
//! Auctions live in [`crate::auctions`]; buildings in [`crate::businesses`].

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::map::MapConfig;
use crate::economy::AccountId;
use crate::money::{Money, Rate};

/// Desirability tier (§4.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// What a block is (§4.1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    /// Leasable and buildable.
    Buildable,
    /// A utility block, auctioned like land (R12, R24).
    Utility,
    /// Non-buildable (City Hall).
    Special,
}

/// Colour band, cheapest to dearest (§4.1, R26).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ColourBand {
    /// Brown.
    Brown,
    /// Light blue.
    LightBlue,
    /// Pink.
    Pink,
    /// Orange.
    Orange,
    /// Red.
    Red,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Dark blue.
    DarkBlue,
}

/// A block's index in [`Map::blocks`]. Stable for the life of a game.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct BlockId(pub u32);

/// A lease on a block (§5.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Lease {
    /// Leaseholder.
    pub holder: AccountId,
    /// In-game year the lease started.
    pub start_year: i32,
    /// Length in years (default 30).
    pub term_years: u32,
    /// Price paid at the last auction (feeds `land_value`, §3).
    pub last_price: Money,
}

impl Lease {
    /// Years left at `year`, between 0 and the term (a year before the start counts as the
    /// full term).
    pub fn remaining_years(&self, year: i32) -> u32 {
        let end = i64::from(self.start_year) + i64::from(self.term_years);
        let left = (end - i64::from(year)).clamp(0, i64::from(self.term_years));
        u32::try_from(left).unwrap_or(0)
    }

    /// True once the lease has run out (R9: the block goes back to auction).
    pub fn is_expired(&self, year: i32) -> bool {
        self.remaining_years(year) == 0
    }
}

/// One block on the grid.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Block {
    /// Config id, e.g. `old_kent_road`.
    pub key: String,
    /// District name.
    pub name: String,
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Current tier (events may change it).
    pub tier: Tier,
    /// Kind.
    pub kind: BlockKind,
    /// Set id, if buildable.
    pub set: Option<String>,
    /// Minimum opening bid (R23).
    pub land_base_price: Money,
    /// Current lease, if any. Unleased land pays rent to the city.
    pub lease: Option<Lease>,
}

/// The map: every block, row-major.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Map {
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Blocks, indexed by [`BlockId`].
    pub blocks: Vec<Block>,
}

impl Map {
    /// Builds the map from validated config, ordered row-major.
    pub fn from_config(config: &MapConfig) -> Self {
        let mut blocks: Vec<Block> = config
            .blocks
            .iter()
            .map(|b| Block {
                key: b.id.clone(),
                name: b.name.clone(),
                row: b.row,
                col: b.col,
                tier: b.tier,
                kind: b.kind,
                set: b.set.clone(),
                land_base_price: b
                    .land_base_price
                    .map_or(Money::ZERO, super::config::types::Pounds::money),
                lease: None,
            })
            .collect();
        blocks.sort_by_key(|b| (b.row, b.col));
        Map {
            width: config.grid.width,
            height: config.grid.height,
            blocks,
        }
    }

    /// A block by id.
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(id.0 as usize)
    }

    /// Block ids in each set, in id order.
    pub fn sets(&self) -> BTreeMap<&str, Vec<BlockId>> {
        let mut sets: BTreeMap<&str, Vec<BlockId>> = BTreeMap::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if let (Some(set), Ok(i)) = (&b.set, u32::try_from(i)) {
                sets.entry(set.as_str()).or_default().push(BlockId(i));
            }
        }
        sets
    }
}

/// A building's footprint inside a lot, in lot units. `(x, y)` is the corner nearest the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Footprint {
    /// X of the near corner.
    pub x: u32,
    /// Y of the near corner.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Depth.
    pub d: u32,
}

impl Footprint {
    /// True if the footprint is non-empty and lies inside a `lot_w × lot_d` lot.
    pub fn fits(&self, lot_w: u32, lot_d: u32) -> bool {
        self.w > 0
            && self.d > 0
            && self.x.checked_add(self.w).is_some_and(|e| e <= lot_w)
            && self.y.checked_add(self.d).is_some_and(|e| e <= lot_d)
    }

    /// True if the footprint touches the lot edge: street frontage (R22).
    pub fn touches_edge(&self, lot_w: u32, lot_d: u32) -> bool {
        self.fits(lot_w, lot_d)
            && (self.x == 0 || self.y == 0 || self.x + self.w == lot_w || self.y + self.d == lot_d)
    }

    /// True if two footprints share any cell.
    pub fn overlaps(&self, other: &Footprint) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.d
            && other.y < self.y + self.d
    }
}

/// Building volume `h × w × d` (R10).
pub fn volume(height: u32, footprint: Footprint) -> i64 {
    i64::from(height) * i64::from(footprint.w) * i64::from(footprint.d)
}

/// `land_rent/day = volume × land_base_rate × desirability_mult(tier)` (§5.3, R10).
pub fn land_rent_per_day(volume: i64, land_base_rate: Money, desirability_mult: Rate) -> Money {
    desirability_mult.apply(land_base_rate.saturating_mul(volume))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rim_footprints_touch_the_edge_and_the_centre_does_not() {
        let rim = Footprint {
            x: 0,
            y: 3,
            w: 3,
            d: 3,
        };
        let centre = Footprint {
            x: 3,
            y: 3,
            w: 3,
            d: 3,
        };
        let whole = Footprint {
            x: 0,
            y: 0,
            w: 9,
            d: 9,
        };
        assert!(rim.touches_edge(9, 9));
        assert!(!centre.touches_edge(9, 9));
        assert!(whole.touches_edge(9, 9));
    }

    #[test]
    fn lease_counts_down_and_expires() {
        let lease = Lease {
            holder: AccountId::City,
            start_year: 1930,
            term_years: 30,
            last_price: Money::ZERO,
        };
        assert_eq!(lease.remaining_years(1930), 30);
        assert_eq!(lease.remaining_years(1955), 5);
        assert!(lease.is_expired(1960));
        assert_eq!(lease.remaining_years(2000), 0);
    }

    #[test]
    fn land_rent_follows_the_formula() {
        // 3×3×10 building, £0.05/unit, Medium (×1.0): 90 × 5p = £4.50
        let fp = Footprint {
            x: 0,
            y: 0,
            w: 3,
            d: 3,
        };
        let rent = land_rent_per_day(volume(10, fp), Money::from_pence(5), Rate::ONE);
        assert_eq!(rent, Money::from_pence(450));
    }

    proptest! {
        #[test]
        fn lease_remaining_never_exceeds_term(start in 1900i32..2100, term in 1u32..100, year in 1900i32..2200) {
            let lease = Lease { holder: AccountId::City, start_year: start, term_years: term, last_price: Money::ZERO };
            prop_assert!(lease.remaining_years(year) <= term);
        }

        #[test]
        fn a_footprint_that_fits_on_the_rim_touches_the_edge(w in 1u32..=9, d in 1u32..=9) {
            let fp = Footprint { x: 0, y: 0, w, d };
            prop_assert!(fp.touches_edge(9, 9));
        }
    }
}
