//! Walking: a cheap, deterministic grid walk at tick granularity (§4.1, §8).
//!
//! The map is a grid of cells, one per lot unit. Each block is a `lot_w × lot_d` lot with a
//! one-cell road on every side, so block `(row, col)` starts at cell
//! `(col × (lot_w + 1) + 1, row × (lot_d + 1) + 1)`. Walkers move up to `cells_per_tick` cells
//! a tick, along x first and then y (an L-shaped route on the street grid). No pathfinding
//! state, no floats.

use serde::{Deserialize, Serialize};

use crate::land::{Block, Footprint};

/// A cell on the walking grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cell {
    /// Column of cells (east).
    pub x: i32,
    /// Row of cells (south).
    pub y: i32,
}

impl Cell {
    /// Manhattan distance in cells.
    pub fn manhattan(self, other: Cell) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

/// The walking grid's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Geometry {
    /// Lot width in cells.
    pub lot_w: u32,
    /// Lot depth in cells.
    pub lot_d: u32,
}

impl Geometry {
    /// Cells from one block's origin to the next across (lot plus road).
    pub fn pitch_x(&self) -> i32 {
        i32::try_from(self.lot_w + 1).unwrap_or(i32::MAX)
    }

    /// Cells from one block's origin to the next down.
    pub fn pitch_y(&self) -> i32 {
        i32::try_from(self.lot_d + 1).unwrap_or(i32::MAX)
    }

    /// The first cell of a block's lot.
    pub fn origin(&self, block: &Block) -> Cell {
        let col = i32::try_from(block.col).unwrap_or(0);
        let row = i32::try_from(block.row).unwrap_or(0);
        Cell {
            x: col * self.pitch_x() + 1,
            y: row * self.pitch_y() + 1,
        }
    }

    /// A block's centre: where its tube station is.
    pub fn centre(&self, block: &Block) -> Cell {
        let o = self.origin(block);
        Cell {
            x: o.x + i32::try_from(self.lot_w / 2).unwrap_or(0),
            y: o.y + i32::try_from(self.lot_d / 2).unwrap_or(0),
        }
    }

    /// A building's door: the centre of its footprint.
    pub fn door(&self, block: &Block, footprint: Footprint) -> Cell {
        let o = self.origin(block);
        let dx = footprint.x + footprint.w / 2;
        let dy = footprint.y + footprint.d / 2;
        Cell {
            x: o.x + i32::try_from(dx).unwrap_or(0),
            y: o.y + i32::try_from(dy).unwrap_or(0),
        }
    }

    /// Manhattan distance in blocks, for `distance_decay` (§8.3).
    #[allow(
        clippy::float_arithmetic,
        reason = "distance in blocks feeds customer scoring, the one float module"
    )]
    pub fn blocks_between(&self, a: Cell, b: Cell) -> f64 {
        let pitch = f64::from(self.lot_w + 1);
        f64::from(a.manhattan(b)) / pitch
    }
}

/// Moves `from` up to `cells` cells towards `to`, x first then y. Returns the new cell.
pub fn step_towards(from: Cell, to: Cell, cells: u32) -> Cell {
    let budget = i32::try_from(cells).unwrap_or(i32::MAX);
    let dx = to.x - from.x;
    let mx = dx.signum() * dx.abs().min(budget);
    let left = budget - mx.abs();
    let dy = to.y - from.y;
    let my = dy.signum() * dy.abs().min(left);
    Cell {
        x: from.x + mx,
        y: from.y + my,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn walks_x_then_y() {
        let a = Cell { x: 0, y: 0 };
        let b = Cell { x: 2, y: 5 };
        assert_eq!(step_towards(a, b, 3), Cell { x: 2, y: 1 });
        assert_eq!(step_towards(b, b, 3), b);
    }

    proptest! {
        #[test]
        fn every_step_closes_the_gap_by_the_budget_or_arrives(
            ax in -100i32..100, ay in -100i32..100, bx in -100i32..100, by in -100i32..100, cells in 1u32..10
        ) {
            let (a, b) = (Cell { x: ax, y: ay }, Cell { x: bx, y: by });
            let next = step_towards(a, b, cells);
            let before = a.manhattan(b);
            let after = next.manhattan(b);
            prop_assert!(after == 0 || after + cells == before);
            prop_assert!(a.manhattan(next) <= cells);
        }
    }
}
