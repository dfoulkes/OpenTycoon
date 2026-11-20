//! Lots: every block is a `lot_size` lot (default 9×9) holding several businesses, and each
//! business must have street frontage by touching the lot edge (§4.1, R22).
//!
//! A [`Lot`] is a value built from the footprints already on a block; it answers "can this
//! business go here?" and never stores anything the businesses module does not also hold.
//! The businesses module calls [`Lot::can_place`] before it builds.

use serde::{Deserialize, Serialize};

use super::Footprint;

/// A business's size inside a lot, in lot units: width × depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Size {
    /// Width (x).
    pub w: u32,
    /// Depth (y).
    pub d: u32,
}

/// Where a business's near corner sits inside a lot, in lot units from the lot's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    /// X of the near corner.
    pub x: u32,
    /// Y of the near corner.
    pub y: u32,
}

impl Footprint {
    /// The footprint of a `size` business with its near corner at `position`.
    pub fn at(size: Size, position: Position) -> Self {
        Footprint {
            x: position.x,
            y: position.y,
            w: size.w,
            d: size.d,
        }
    }
}

/// Why a business cannot go where it was asked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlacementError {
    /// Zero width or depth.
    Empty,
    /// Some of it lies outside the lot.
    OutsideLot,
    /// It does not touch the lot edge, so it has no street frontage (R22).
    NoFrontage,
    /// It shares cells with a business already on the lot.
    Overlaps(Footprint),
}

/// One block's lot: its size and the footprints already built on it (R22).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Lot {
    width: u32,
    depth: u32,
    occupied: Vec<Footprint>,
}

impl Lot {
    /// An empty `width × depth` lot.
    pub fn new(width: u32, depth: u32) -> Self {
        Lot {
            width,
            depth,
            occupied: Vec::new(),
        }
    }

    /// An empty lot of `game.yaml` `world.lot_size` (`[width, depth]`).
    pub fn of_size(lot_size: [u32; 2]) -> Self {
        Lot::new(lot_size[0], lot_size[1])
    }

    /// The same lot with these footprints already built (e.g. every business on the block).
    #[must_use]
    pub fn with_occupied(mut self, footprints: impl IntoIterator<Item = Footprint>) -> Self {
        self.occupied.extend(footprints);
        self
    }

    /// Width in lot units.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Depth in lot units.
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Footprints already on the lot, in placement order.
    pub fn occupied(&self) -> &[Footprint] {
        &self.occupied
    }

    /// Checks whether a `size` business can go at `position`: non-empty, inside the lot,
    /// touching the lot edge (R22) and clear of every business already there. Returns the
    /// footprint it would take.
    pub fn can_place(&self, size: Size, position: Position) -> Result<Footprint, PlacementError> {
        let footprint = Footprint::at(size, position);
        self.check(footprint)?;
        Ok(footprint)
    }

    /// [`Lot::can_place`] for a footprint that already carries its position.
    pub fn check(&self, footprint: Footprint) -> Result<(), PlacementError> {
        if footprint.w == 0 || footprint.d == 0 {
            return Err(PlacementError::Empty);
        }
        if !footprint.fits(self.width, self.depth) {
            return Err(PlacementError::OutsideLot);
        }
        if !footprint.touches_edge(self.width, self.depth) {
            return Err(PlacementError::NoFrontage);
        }
        if let Some(other) = self.occupied.iter().find(|o| o.overlaps(&footprint)) {
            return Err(PlacementError::Overlaps(*other));
        }
        Ok(())
    }

    /// Places a business if [`Lot::can_place`] allows it, and records its footprint.
    pub fn place(&mut self, size: Size, position: Position) -> Result<Footprint, PlacementError> {
        let footprint = self.can_place(size, position)?;
        self.occupied.push(footprint);
        Ok(footprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const S3: Size = Size { w: 3, d: 3 };

    fn pos(x: u32, y: u32) -> Position {
        Position { x, y }
    }

    #[test]
    fn eight_rim_businesses_fit_and_the_centre_is_left_empty() {
        let mut lot = Lot::new(9, 9);
        for (x, y) in [
            (0, 0),
            (3, 0),
            (6, 0),
            (0, 3),
            (6, 3),
            (0, 6),
            (3, 6),
            (6, 6),
        ] {
            assert!(lot.place(S3, pos(x, y)).is_ok(), "{x},{y}");
        }
        assert_eq!(lot.occupied().len(), 8);
        assert_eq!(
            lot.can_place(S3, pos(3, 3)),
            Err(PlacementError::NoFrontage)
        );
    }

    #[test]
    fn errors_name_the_reason() {
        let lot = Lot::new(9, 9).with_occupied([Footprint::at(S3, pos(0, 0))]);
        assert_eq!(
            lot.can_place(Size { w: 0, d: 3 }, pos(0, 0)),
            Err(PlacementError::Empty)
        );
        assert_eq!(
            lot.can_place(S3, pos(7, 0)),
            Err(PlacementError::OutsideLot)
        );
        assert_eq!(
            lot.can_place(S3, pos(2, 0)),
            Err(PlacementError::Overlaps(Footprint::at(S3, pos(0, 0))))
        );
        assert_eq!(
            lot.can_place(Size { w: 9, d: 9 }, pos(0, 0)),
            Err(PlacementError::Overlaps(Footprint::at(S3, pos(0, 0))))
        );
    }

    proptest! {
        /// Whatever is placed, every footprint on the lot is inside it, on the edge, and
        /// overlaps no other (R22).
        #[test]
        fn placed_businesses_always_have_frontage_and_never_overlap(
            tries in prop::collection::vec((1u32..=9, 1u32..=9, 0u32..9, 0u32..9), 0..40)
        ) {
            let mut lot = Lot::new(9, 9);
            for (w, d, x, y) in tries {
                let _ = lot.place(Size { w, d }, pos(x, y));
            }
            let fps = lot.occupied();
            for (i, a) in fps.iter().enumerate() {
                prop_assert!(a.fits(9, 9));
                prop_assert!(a.touches_edge(9, 9));
                for b in &fps[i + 1..] {
                    prop_assert!(!a.overlaps(b));
                }
            }
        }
    }
}
