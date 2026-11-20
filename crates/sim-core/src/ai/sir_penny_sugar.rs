//! Sir Penny Sugar: "deals mostly in residential" (§10, §7.2).
//!
//! He plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `sir_penny_sugar` (plus his personality's tier weights); he draws
//! nothing at random.
//!
//! What he does, in priority order:
//!
//! 1. **Auction turns, every tick.** On a lease of a tier he wants (Low and Medium), up to
//!    [`PennySugar::max_bid`], more for a lot with an open tube station; on an apartment block
//!    up for auction (the city's, R28, or a player's), up to its build cost (R41). Otherwise he
//!    passes (R8).
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` to midnight, a negative balance (R3) sells his cheapest
//!    block to the city (R5).
//! 3. **Rents.** A new block is let at `start_rent × reference_rent`. Each morning after the
//!    05:30 house-hunt (R53) he looks at each block's occupancy: at or above `raise_at` he puts
//!    the rent up by `rent_step`, at or below `cut_at` he cuts it, within `min_rent`–`max_rent`
//!    (R51, R52).
//! 4. **Build.** Apartment blocks (`customers.yaml` `residential.building_type`, if the era
//!    allows it), at most `apartments_per_lot` a lease, if he keeps `cash_buffer`.
//! 5. **Land.** Opens an auction on a lot he wants, station lots first, then the cheapest (R23);
//!    failing that, on one of the city's apartment blocks at its build cost (R28).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, Temper};
use crate::auctions::{Auction, AuctionItem};
use crate::businesses::{BusinessId, FitOutLevel};
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::{Money, Rate};
use crate::population::residential::flats;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// Sir Penny Sugar's memory between decisions. Everything else he reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PennySugar {
    /// His personality id in `ai.yaml` (for the tier weights).
    personality: String,
    cadence: Cadence,
    temper: Temper,
    /// The rent he last set on each block.
    rents: BTreeMap<BusinessId, Money>,
    /// The day he last reviewed rents.
    reviewed_on: Option<u32>,
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

impl PennySugar {
    /// Sir Penny Sugar playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        PennySugar {
            personality: personality.into(),
            ..PennySugar::default()
        }
    }

    fn reference(world: &World, share: Rate) -> Money {
        share.apply(world.config().customers.residential.reference_rent.money())
    }

    /// The most he pays for the lease on `block` now: `aggression × bid ceiling` (R7), plus
    /// `station_premium × bid ceiling` if the lot has an open tube station, scaled by the share
    /// of a term left, and never so much that he couldn't then build one block there and keep
    /// `cash_buffer`. `None` if he doesn't want it.
    pub fn max_bid(&self, world: &World, me: PlayerId, block: BlockId) -> Option<Money> {
        let config = world.config();
        let cfg = &config.ai.sir_penny_sugar;
        if !kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), block) {
            return None;
        }
        let tier = world.map().block(block)?.tier;
        let apartments = kit::apartments_type(config, world.clock().year())?;
        let one = kit::building_cost(
            config,
            tier,
            apartments,
            cfg.apartment_footprint,
            cfg.apartment_height,
            FitOutLevel::Medium,
        )?;
        let premium = if kit::has_station(world, block) {
            cfg.station_premium.rate().bp()
        } else {
            0
        };
        let share = kit::share_of_ceiling(
            world,
            me,
            Rate::from_bp(cfg.aggression.rate().bp() + premium),
        );
        let keep = kit::spare(world, me, cfg.cash_buffer.money()) - one;
        Some(kit::term_share(world, block).apply(share).min(keep))
    }

    /// The most he pays for an apartment block on auction: its build cost (R41), if it stands
    /// on a tier he wants and he keeps `cash_buffer`.
    fn max_for_block(&self, world: &World, me: PlayerId, id: BusinessId) -> Option<Money> {
        let config = world.config();
        let cfg = &config.ai.sir_penny_sugar;
        let b = world.businesses().get(id)?;
        let wanted = cfg.buy_city_blocks
            && b.type_id == config.customers.residential.building_type
            && kit::wants_block(
                world,
                &self.personality,
                cfg.min_tier_weight.rate(),
                b.block,
            );
        wanted.then(|| {
            b.build_cost
                .min(kit::spare(world, me, cfg.cash_buffer.money()))
                .min(kit::share_of_ceiling(world, me, Rate::ONE))
        })
    }

    fn max_for(&self, world: &World, me: PlayerId, auction: &Auction) -> Option<Money> {
        match auction.item {
            AuctionItem::Land(block) => self.max_bid(world, me, block),
            AuctionItem::Business(id) => self.max_for_block(world, me, id),
            AuctionItem::Utility(_) => None,
        }
    }

    fn strategy(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let cfg = &world.config().ai.sir_penny_sugar;
        if kit::needs_rescue(world, me, cfg.rescue_after.seconds_since_midnight()) {
            return kit::rescue(world, me).into_iter().collect();
        }
        let mut out = self.rents(world, me);
        if let Some(build) = Self::build(world, me) {
            out.push(build);
        } else if let Some(open) = self.open_auction(world, me) {
            out.push(open);
        }
        out
    }

    /// New blocks at the starting rent; once a morning, every block's rent by occupancy.
    fn rents(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let config = world.config();
        let cfg = &config.ai.sir_penny_sugar;
        let rules = &config.customers.residential;
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
                let rent = Self::reference(world, cfg.start_rent.rate());
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
        let step = Self::reference(world, cfg.rent_step.rate());
        let bounds = (
            Self::reference(world, cfg.min_rent.rate()),
            Self::reference(world, cfg.max_rent.rate()),
        );
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
                (cfg.raise_at.rate(), cfg.cut_at.rate()),
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

    fn build(world: &World, me: PlayerId) -> Option<Command> {
        let config = world.config();
        let cfg = &config.ai.sir_penny_sugar;
        let apartments = kit::apartments_type(config, world.clock().year())?;
        kit::held_lots(world, me)
            .into_iter()
            .filter(|block| {
                kit::owned_on(world, me, *block, |t| t == apartments) < cfg.apartments_per_lot
            })
            .filter_map(|block| {
                kit::plan_on(
                    world,
                    me,
                    block,
                    apartments,
                    cfg.apartment_footprint,
                    cfg.apartment_height,
                    FitOutLevel::Medium,
                )
            })
            .find(|plan| {
                kit::spare(world, me, cfg.cash_buffer.money()) >= plan.cost
                    && world.can_afford(me, plan.cost)
            })
            .map(kit::Plan::command)
    }

    fn open_auction(&self, world: &World, me: PlayerId) -> Option<Command> {
        if !kit::house_free(world) {
            return None;
        }
        let land = kit::open_lots(world)
            .into_iter()
            .filter(|(id, base)| self.max_bid(world, me, *id).is_some_and(|max| *base <= max))
            .min_by_key(|(id, base)| (!kit::has_station(world, *id), *base, *id))
            .map(|(block, bid)| Command::OpenLandAuction { block, bid });
        land.or_else(|| {
            world
                .businesses()
                .owned_by(AccountId::City)
                .filter(|(id, b)| {
                    self.max_for_block(world, me, *id)
                        .is_some_and(|max| b.build_cost <= max)
                })
                .min_by_key(|(id, b)| (b.build_cost, *id))
                .map(|(business, b)| Command::OpenBusinessAuction {
                    business,
                    bid: b.build_cost,
                })
        })
    }
}

impl Brain for PennySugar {
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        let jitter = world.config().ai.sir_penny_sugar.bid_jitter.rate();
        let mut temper = self.temper;
        let mut out: Vec<Command> = kit::tempered_turn(world, me, &mut temper, jitter, rng, |a| {
            self.max_for(world, me, a)
        })
        .into_iter()
        .collect();
        self.temper = temper;
        if self.cadence.due(world) {
            out.extend(self.strategy(world, me));
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
        /// Whatever the occupancy, the rent stays within his bounds.
        #[test]
        fn rent_stays_within_bounds(rent in 0i64..2_000, let_flats in 0u32..100, total in 0u32..100) {
            let next = rent_for_occupancy(
                Money::from_pence(rent), let_flats.min(total), total, STEP, LIMITS, BOUNDS,
            );
            prop_assert!(next >= BOUNDS.0 && next <= BOUNDS.1);
        }
    }
}
