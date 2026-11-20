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
//!    (R51, R52; the shared rule, [`kit::rent`]).
//! - **Hotel.** Holding a whole colour set, it puts up a Hotel there before anything else
//!   (§7.3; R14, R29), if the era has a Hotel type and it keeps its buffer.
//! 4. **Build.** Apartment blocks (`customers.yaml` `residential.building_type`, if the era
//!    allows it), at most `apartments_per_lot` a lease, if he keeps `cash_buffer`.
//! 5. **Land.** Opens an auction on a lot he wants, station lots first, then the cheapest (R23);
//!    failing that, on one of the city's apartment blocks at its build cost (R28).

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, RentBook, Temper, VacancyRule};
use crate::auctions::{Auction, AuctionItem};
use crate::businesses::{BusinessId, FitOutLevel};
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::{Money, Rate};
use crate::rng::SimRng;
use crate::world::{Command, World};

/// Sir Penny Sugar's memory between decisions. Everything else he reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PennySugar {
    /// His personality id in `ai.yaml` (for the tier weights).
    personality: String,
    cadence: Cadence,
    temper: Temper,
    /// The rents he has set, reviewed each morning by vacancy ([`kit::RentBook`]).
    rents: RentBook,
}

impl PennySugar {
    /// Sir Penny Sugar playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        PennySugar {
            personality: personality.into(),
            ..PennySugar::default()
        }
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
        let cfg = &world.config().ai.sir_penny_sugar;
        let rule = VacancyRule {
            start: cfg.start_rent.rate(),
            step: cfg.rent_step.rate(),
            raise_at: cfg.raise_at.rate(),
            cut_at: cfg.cut_at.rate(),
            min: cfg.min_rent.rate(),
            max: cfg.max_rent.rate(),
        };
        self.rents.review(world, me, &rule)
    }

    fn build(world: &World, me: PlayerId) -> Option<Command> {
        let cfg_h = &world.config().ai.sir_penny_sugar;
        if let Some(plan) = kit::hotel_plan(
            world,
            me,
            cfg_h.apartment_footprint,
            cfg_h.apartment_height,
            FitOutLevel::Medium,
        ) && kit::spare(world, me, cfg_h.cash_buffer.money()) >= plan.cost
            && world.can_afford(me, plan.cost)
        {
            return Some(plan.command());
        }
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
