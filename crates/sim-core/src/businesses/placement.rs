//! Putting up a building (§4.1, §7.1; R13, R22, R57, R58).
//!
//! A building is one business of one type (R13). It is refused unless:
//!
//! - its type exists and is unlocked (`unlock_year ≤ year`);
//! - `1 ≤ height ≤ type.max_height` (R58). The cap is per type and never per era (R57); config
//!   already guarantees `type.max_height ≤ world.max_height`;
//! - its footprint meets the type's `min_footprint`, either way round;
//! - the lot accepts it: inside the lot, touching the lot edge, overlapping no other building
//!   ([`can_place`], R22).
//!
//! Who may build on a lot (the leaseholder) is checked by the world, which knows the lease.

use serde::{Deserialize, Serialize};

use super::{
    Business, BusinessId, Businesses, FitOutLevel, build_cost, capacity_per_good, stock_capacity,
};
use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::economy::AccountId;
use crate::land::{BlockId, BlockKind, Footprint, Lot, Map, PlacementError as LotError};
use crate::money::{Money, Rate};

/// What a player asks to build (§7.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildRequest {
    /// Business type id from `businesses.yaml`.
    pub type_id: String,
    /// Block (lot) to build on.
    pub block: BlockId,
    /// Footprint in the lot, in lot units.
    pub footprint: Footprint,
    /// Height in units.
    pub height: u32,
    /// Who will own the building.
    pub owner: AccountId,
    /// Fit-out (stored; post-v1 it shifts appeal).
    pub fit_out: FitOutLevel,
}

/// Why a footprint can't go on a lot (§4.1, R22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlacementError {
    /// Empty, or not wholly inside the lot.
    OutsideLot,
    /// Doesn't touch the lot edge: no street frontage (R22).
    NoStreetFrontage,
    /// Shares cells with another building on the lot.
    Overlaps(BusinessId),
}

/// Why a building was refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildError {
    /// No such business type in `businesses.yaml`.
    UnknownType(String),
    /// No such block.
    NoSuchBlock(BlockId),
    /// The block isn't buildable (a utility or City Hall).
    NotBuildable(BlockId),
    /// The type isn't available yet (§7.4).
    NotUnlocked {
        /// First year it can be built.
        unlock_year: i32,
        /// The year now.
        year: i32,
    },
    /// Zero height.
    TooShort,
    /// Taller than the type allows (R58).
    TooTall {
        /// Requested height.
        height: u32,
        /// The type's cap.
        max_height: u32,
    },
    /// Smaller than the type's minimum footprint (§7.1).
    FootprintTooSmall {
        /// The type's `[width, depth]` minimum.
        min: [u32; 2],
    },
    /// The lot refused it (R22).
    Placement(PlacementError),
}

/// The lot rules for one footprint (§4.1, R22): inside the `lot_size` lot, touching its edge,
/// overlapping none of `existing`. Delegates to land's lot model, [`Lot::check`], the one
/// implementation of the rule; this maps its errors to the building's terms.
pub fn can_place<'a>(
    lot_size: [u32; 2],
    existing: impl IntoIterator<Item = (BusinessId, &'a Footprint)>,
    footprint: Footprint,
) -> Result<(), PlacementError> {
    let existing: Vec<(BusinessId, Footprint)> =
        existing.into_iter().map(|(id, f)| (id, *f)).collect();
    let lot = Lot::of_size(lot_size).with_occupied(existing.iter().map(|(_, f)| *f));
    lot.check(footprint).map_err(|e| match e {
        LotError::Empty | LotError::OutsideLot => PlacementError::OutsideLot,
        LotError::NoFrontage => PlacementError::NoStreetFrontage,
        LotError::Overlaps(other) => PlacementError::Overlaps(
            existing
                .iter()
                .find(|(_, f)| *f == other)
                .map(|(id, _)| *id)
                .unwrap_or_default(),
        ),
    })
}

/// True if `footprint` meets `min` `[width, depth]` either way round (§7.1).
pub fn meets_min_footprint(footprint: Footprint, min: [u32; 2]) -> bool {
    let fits = |w: u32, d: u32| footprint.w >= w && footprint.d >= d;
    fits(min[0], min[1]) || fits(min[1], min[0])
}

/// Height rule: `1 ≤ height ≤ max_height` (R58). There is no era term (R57).
pub fn check_height(height: u32, max_height: u32) -> Result<(), BuildError> {
    if height == 0 {
        Err(BuildError::TooShort)
    } else if height > max_height {
        Err(BuildError::TooTall { height, max_height })
    } else {
        Ok(())
    }
}

fn fit_out_mult(config: &Config, level: FitOutLevel) -> Rate {
    let table = &config.businesses.fit_out;
    match level {
        FitOutLevel::Low => table.low.build_cost_mult.rate(),
        FitOutLevel::Medium => table.medium.build_cost_mult.rate(),
        FitOutLevel::High => table.high.build_cost_mult.rate(),
    }
}

impl Businesses {
    /// Checks a request against every building rule and prices it, without building.
    pub fn quote_build(
        &self,
        request: &BuildRequest,
        config: &Config,
        map: &Map,
        year: i32,
    ) -> Result<Money, BuildError> {
        let kind = config
            .businesses
            .types
            .get(&request.type_id)
            .ok_or_else(|| BuildError::UnknownType(request.type_id.clone()))?;
        let block = map
            .block(request.block)
            .ok_or(BuildError::NoSuchBlock(request.block))?;
        if block.kind != BlockKind::Buildable {
            return Err(BuildError::NotBuildable(request.block));
        }
        if kind.unlock_year > year {
            return Err(BuildError::NotUnlocked {
                unlock_year: kind.unlock_year,
                year,
            });
        }
        check_height(request.height, kind.max_height)?;
        if !meets_min_footprint(request.footprint, kind.min_footprint) {
            return Err(BuildError::FootprintTooSmall {
                min: kind.min_footprint,
            });
        }
        can_place(
            config.game.world.lot_size,
            self.on_block(request.block)
                .map(|(id, b)| (id, &b.footprint)),
            request.footprint,
        )
        .map_err(BuildError::Placement)?;
        Ok(build_cost(
            request.height,
            request.footprint,
            config.economy.building.base_rate.money(),
            config.map.tiers.get(block.tier).desirability_mult.rate(),
            fit_out_mult(config, request.fit_out),
        ))
    }

    /// Builds if every rule passes and returns the new id and its build cost, which the
    /// caller charges to the owner. Prices start at 100% of reference; stock and orders start
    /// empty; the shop starts open.
    pub fn build(
        &mut self,
        request: BuildRequest,
        config: &Config,
        map: &Map,
        year: i32,
    ) -> Result<(BusinessId, Money), BuildError> {
        let cost = self.quote_build(&request, config, map, year)?;
        let kind = config
            .businesses
            .types
            .get(&request.type_id)
            .ok_or_else(|| BuildError::UnknownType(request.type_id.clone()))?;
        let id = BusinessId(self.next_id);
        self.next_id += 1;
        self.by_id.insert(id, Business::new(request, kind, cost));
        Ok((id, cost))
    }
}

impl Business {
    /// A new building of type `kind`, without checking the building rules (for setup such as
    /// the city's starting apartments, R28; play goes through [`Businesses::build`]). Prices
    /// start at 100% of reference; stock and orders start empty; the shop starts open.
    pub fn new(request: BuildRequest, kind: &BusinessType, cost: Money) -> Business {
        new_business(request, kind, cost)
    }
}

fn new_business(request: BuildRequest, kind: &BusinessType, cost: Money) -> Business {
    let capacity = stock_capacity(request.height, request.footprint, kind.stock_density.rate());
    let zeroes = || kind.goods.iter().map(|g| (g.clone(), 0)).collect();
    Business {
        type_id: request.type_id,
        block: request.block,
        footprint: request.footprint,
        height: request.height,
        owner: request.owner,
        fit_out: request.fit_out,
        build_cost: cost,
        hours: kind.hours,
        capacity_per_good: capacity_per_good(capacity, kind.goods.len()),
        prices: kind.goods.iter().map(|g| (g.clone(), Rate::ONE)).collect(),
        stock: zeroes(),
        standing_order: zeroes(),
        tomorrow_order: zeroes(),
        open: true,
        takings_today: Money::ZERO,
        sold_today: zeroes(),
        lost_sales_today: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn fp(x: u32, y: u32, w: u32, d: u32) -> Footprint {
        Footprint { x, y, w, d }
    }

    #[test]
    fn min_footprint_either_way_round() {
        assert!(meets_min_footprint(fp(0, 0, 3, 2), [2, 3]));
        assert!(meets_min_footprint(fp(0, 0, 2, 3), [2, 3]));
        assert!(!meets_min_footprint(fp(0, 0, 2, 2), [2, 3]));
        assert!(!meets_min_footprint(fp(0, 0, 1, 9), [2, 3]));
    }

    #[test]
    fn eight_rim_buildings_fit_round_a_gap() {
        // §4.1: Old Kent Road as eight 3×3 businesses around an empty centre.
        let mut placed: Vec<(BusinessId, Footprint)> = Vec::new();
        let rim = [
            (0, 0),
            (3, 0),
            (6, 0),
            (0, 3),
            (6, 3),
            (0, 6),
            (3, 6),
            (6, 6),
        ];
        for (i, (x, y)) in (0u32..).zip(rim) {
            let f = fp(x, y, 3, 3);
            assert_eq!(
                can_place([9, 9], placed.iter().map(|(id, f)| (*id, f)), f),
                Ok(())
            );
            placed.push((BusinessId(i), f));
        }
        assert_eq!(
            can_place(
                [9, 9],
                placed.iter().map(|(id, f)| (*id, f)),
                fp(3, 3, 3, 3)
            ),
            Err(PlacementError::NoStreetFrontage)
        );
    }

    #[test]
    fn height_rule_has_no_era_term() {
        assert_eq!(check_height(0, 12), Err(BuildError::TooShort));
        assert_eq!(check_height(12, 12), Ok(()));
        assert_eq!(
            check_height(13, 12),
            Err(BuildError::TooTall {
                height: 13,
                max_height: 12
            })
        );
    }

    proptest! {
        /// Whatever is attempted, accepted footprints never overlap and all have frontage.
        #[test]
        fn accepted_footprints_never_overlap(
            cands in prop::collection::vec((0u32..9, 0u32..9, 1u32..=9, 1u32..=9), 0..40)
        ) {
            let mut placed: Vec<(BusinessId, Footprint)> = Vec::new();
            for (i, (x, y, w, d)) in (0u32..).zip(cands) {
                let f = fp(x, y, w, d);
                if can_place([9, 9], placed.iter().map(|(id, f)| (*id, f)), f).is_ok() {
                    placed.push((BusinessId(i), f));
                }
            }
            for (i, (_, a)) in placed.iter().enumerate() {
                prop_assert!(a.touches_edge(9, 9));
                for (_, b) in placed.iter().skip(i + 1) {
                    prop_assert!(!a.overlaps(b));
                }
            }
        }
    }
}
