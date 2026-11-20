//! Rent by vacancy (§7.2; R51, R52): the rule every AI landlord uses. A new block is let at a
//! starting rent; each morning after the 05:30 house-hunt (R53) the rent goes up a step when the
//! block is nearly full and down a step when it stands half empty, within bounds. Sir Penny
//! Sugar, the Landlord (R67) and any personality that moves into flats (R66) share it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::businesses::BusinessId;
use crate::config::ai::OpportunismConfig;
use crate::economy::{AccountId, PlayerId};
use crate::money::{Money, Rate};
use crate::population::residential::flats;
use crate::world::{Command, World};

/// A vacancy rent rule. Rents are shares of `customers.yaml` `residential.reference_rent`;
/// occupancy thresholds are shares of a block's flats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VacancyRule {
    /// A new block's rent.
    pub start: Rate,
    /// How far the rent moves in one morning.
    pub step: Rate,
    /// Occupancy at or above which it goes up.
    pub raise_at: Rate,
    /// Occupancy at or below which it comes down.
    pub cut_at: Rate,
    /// Lowest rent.
    pub min: Rate,
    /// Highest rent.
    pub max: Rate,
}

impl From<&OpportunismConfig> for VacancyRule {
    fn from(o: &OpportunismConfig) -> Self {
        VacancyRule {
            start: o.start_rent.rate(),
            step: o.rent_step.rate(),
            raise_at: o.raise_at.rate(),
            cut_at: o.cut_at.rate(),
            min: o.min_rent.rate(),
            max: o.max_rent.rate(),
        }
    }
}

/// The new rent for a block from its occupancy (§7.2, R51): `rent + step` when `let ÷ flats ≥
/// raise_at`, `rent − step` when it is `≤ cut_at`, otherwise unchanged; always within
/// `min..=max`. An empty building (no flats) keeps its rent.
pub fn rent_for_occupancy(
    rent: Money,
    let_flats: u32,
    total: u32,
    step: Money,
    (raise_at, cut_at): (Rate, Rate),
    (min, max): (Money, Money),
) -> Money {
    if total == 0 {
        return rent.max(min).min(max);
    }
    let occupancy = i64::from(let_flats) * crate::money::BASIS_POINTS_PER_UNIT / i64::from(total);
    let next = if occupancy >= raise_at.bp() {
        rent + step
    } else if occupancy <= cut_at.bp() {
        rent - step
    } else {
        rent
    };
    next.max(min).min(max)
}

/// The rents an AI has set on its apartment blocks, and the day it last reviewed them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct RentBook {
    rents: BTreeMap<BusinessId, Money>,
    reviewed_on: Option<u32>,
}

impl RentBook {
    /// The rent it last set on `block`.
    pub fn rent_of(&self, block: BusinessId) -> Option<Money> {
        self.rents.get(&block).copied()
    }

    /// Lets each new block of `me`'s at `rule.start`; once a morning after the house-hunt,
    /// moves every block's rent by its occupancy ([`rent_for_occupancy`]).
    pub fn review(&mut self, world: &World, me: PlayerId, rule: &VacancyRule) -> Vec<Command> {
        let rules = &world.config().customers.residential;
        let reference = |share: Rate| share.apply(rules.reference_rent.money());
        let mine: Vec<(BusinessId, u32)> = world
            .businesses()
            .owned_by(AccountId::Player(me))
            .filter(|(_, b)| b.type_id == rules.building_type)
            .map(|(id, b)| (id, flats(b.volume(), rules.flat_size)))
            .collect();
        self.rents.retain(|id, _| mine.iter().any(|(m, _)| m == id));
        let mut out = Vec::new();
        for (id, _) in &mine {
            if !self.rents.contains_key(id) {
                let rent = reference(rule.start);
                self.rents.insert(*id, rent);
                out.push(Command::SetRent {
                    building: *id,
                    rent,
                });
            }
        }
        let today = world.clock().day();
        let after_hunt = world.clock().second_of_day() >= world.clock().rules().arrivals_second;
        if !after_hunt || self.reviewed_on == Some(today) {
            return out;
        }
        self.reviewed_on = Some(today);
        let occupancy = world.population().residential.occupancy();
        let step = reference(rule.step);
        let bounds = (reference(rule.min), reference(rule.max));
        for (id, total) in mine {
            let Some(rent) = self.rents.get_mut(&id) else {
                continue;
            };
            let let_flats = occupancy.get(&id).copied().unwrap_or(0);
            let next = rent_for_occupancy(
                *rent,
                let_flats,
                total,
                step,
                (rule.raise_at, rule.cut_at),
                bounds,
            );
            if next != *rent {
                *rent = next;
                out.push(Command::SetRent {
                    building: id,
                    rent: next,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const STEP: Money = Money::from_pence(30);
    const LIMITS: (Rate, Rate) = (Rate::from_bp(9_500), Rate::from_bp(6_000));
    const BOUNDS: (Money, Money) = (Money::from_pence(150), Money::from_pence(900));

    #[test]
    fn rent_follows_vacancy() {
        let three = Money::from_pence(300);
        let rent = |let_flats| rent_for_occupancy(three, let_flats, 20, STEP, LIMITS, BOUNDS);
        assert_eq!(rent(20), Money::from_pence(330)); // full: up
        assert_eq!(rent(19), Money::from_pence(330)); // 95%: up
        assert_eq!(rent(15), three); // 75%: hold
        assert_eq!(rent(12), Money::from_pence(270)); // 60%: down
        assert_eq!(rent(0), Money::from_pence(270)); // empty: down
        assert_eq!(rent_for_occupancy(three, 0, 0, STEP, LIMITS, BOUNDS), three);
    }

    proptest! {
        /// Whatever the occupancy, the rent stays within the bounds.
        #[test]
        fn rent_stays_within_bounds(rent in 0i64..2_000, let_flats in 0u32..100, total in 0u32..100) {
            let next = rent_for_occupancy(
                Money::from_pence(rent), let_flats.min(total), total, STEP, LIMITS, BOUNDS,
            );
            prop_assert!(next >= BOUNDS.0 && next <= BOUNDS.1);
        }
    }
}
