//! The Landlord: "hoards leases and never builds retail" (§10).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `landlord` (plus the `landlord` personality's tier weights); it draws
//! nothing at random.
//!
//! What it does, in priority order:
//!
//! 1. **Auction turns, every tick.** On a lease of a tier it wants (personality weight ≥
//!    `min_tier_weight`), while it holds fewer than `max_leases`, it raises by the smallest
//!    increment up to [`Landlord::max_bid`]; otherwise it passes (R8). It never bids on a
//!    business: it doesn't run them.
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` to midnight, a negative balance (R3) sells its cheapest
//!    building to the city (R5) or, with none, offers a lease at auction.
//! 3. **Rents.** Each apartment block it owns is let at `rent × reference_rent` (R51).
//! 4. **Sell late.** With `sell_within_years` or fewer years left on a lease, it offers the
//!    lease at auction (R33) when the reserve (the land base price, the least a sale can fetch)
//!    beats holding to expiry: book value (§3) + the lot uplift on other owners' buildings
//!    (R39) + their land rent for the days left (§5.2).
//! 5. **Build.** Only apartment blocks (`customers.yaml` `residential.building_type`, if the
//!    era allows it), at most `apartments_per_lot` per lease, never on a lease it is about to
//!    sell, and only if it keeps `cash_buffer`.
//! 6. **Land.** Opens an auction on the cheapest lot it wants at its base price (R23), if that
//!    is within its maximum bid.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, Temper};
use crate::auctions::{Auction, AuctionItem};
use crate::businesses::{BusinessId, FitOutLevel};
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::Money;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// The Landlord's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Landlord {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
    cadence: Cadence,
    temper: Temper,
    /// Apartment blocks whose rent it has set.
    let_out: BTreeSet<BusinessId>,
}

impl Landlord {
    /// The Landlord playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Landlord {
            personality: personality.into(),
            ..Landlord::default()
        }
    }

    fn wants(&self, world: &World, me: PlayerId, block: BlockId) -> bool {
        let cfg = &world.config().ai.landlord;
        let held = u32::try_from(kit::held_lots(world, me).len()).unwrap_or(u32::MAX);
        held < cfg.max_leases
            && kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), block)
    }

    /// The most it pays for the lease on `block` now: `aggression × bid ceiling` (R7) scaled
    /// by the share of a term left on it, plus the lot uplift on buildings others own there
    /// (R39); never more than `aggression × bid ceiling`, and never so much that it drops under
    /// `cash_buffer`. `None` if it doesn't want the lease.
    pub fn max_bid(&self, world: &World, me: PlayerId, block: BlockId) -> Option<Money> {
        if !self.wants(world, me, block) {
            return None;
        }
        let config = world.config();
        let cfg = &config.ai.landlord;
        let share = kit::share_of_ceiling(world, me, cfg.aggression.rate());
        let uplift = config
            .game
            .net_worth
            .lot_property_uplift
            .rate()
            .apply(kit::others_property_on(world, me, block));
        let value = kit::term_share(world, block).apply(share) + uplift;
        Some(
            value
                .min(kit::spare(world, me, cfg.cash_buffer.money()))
                .min(share),
        )
    }

    fn max_for(&self, world: &World, me: PlayerId, auction: &Auction) -> Option<Money> {
        match auction.item {
            AuctionItem::Land(block) => self.max_bid(world, me, block),
            _ => None,
        }
    }

    fn strategy(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let cfg = &world.config().ai.landlord;
        if kit::needs_rescue(world, me, cfg.rescue_after.seconds_since_midnight()) {
            return kit::rescue(world, me).into_iter().collect();
        }
        let mut out = self.let_new_blocks(world, me);
        if let Some(sell) = Self::sell_late(world, me) {
            out.push(sell);
        } else if let Some(build) = Self::build(world, me) {
            out.push(build);
        } else if let Some(open) = self.open_auction(world, me) {
            out.push(open);
        }
        out
    }

    fn let_new_blocks(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let config = world.config();
        let rules = &config.customers.residential;
        let rent = config
            .ai
            .landlord
            .rent
            .rate()
            .apply(rules.reference_rent.money());
        let mut out = Vec::new();
        for (id, b) in world.businesses().owned_by(AccountId::Player(me)) {
            if b.type_id == rules.building_type && self.let_out.insert(id) {
                out.push(Command::SetRent { building: id, rent });
            }
        }
        out
    }

    /// True if the lease on `block` is in its last `sell_within_years`.
    fn is_late(world: &World, block: BlockId) -> bool {
        let within = world.config().ai.landlord.sell_within_years;
        kit::lease_years(world, block).is_some_and(|(left, _)| within > 0 && left <= within)
    }

    /// What holding the lease on `block` to expiry is worth to `me`: book value (§3), the lot
    /// uplift on other owners' buildings (R39) and their land rent for the days left (§5.2).
    pub fn hold_value(world: &World, me: PlayerId, block: BlockId) -> Money {
        let config = world.config();
        let book = kit::lease_book_value(world, block).unwrap_or(Money::ZERO);
        let uplift = config
            .game
            .net_worth
            .lot_property_uplift
            .rate()
            .apply(kit::others_property_on(world, me, block));
        let per_day = config.game.clock.years_per_day.max(1);
        let days_left =
            kit::lease_years(world, block).map_or(0, |(left, _)| left.div_ceil(per_day));
        let rent = kit::others_rent_on(world, me, block).saturating_mul(i64::from(days_left));
        book + uplift + rent
    }

    fn sell_late(world: &World, me: PlayerId) -> Option<Command> {
        if !kit::house_free(world) {
            return None;
        }
        kit::held_lots(world, me)
            .into_iter()
            .filter(|block| Self::is_late(world, *block))
            .find(|block| {
                let reserve = world
                    .map()
                    .block(*block)
                    .map_or(Money::ZERO, |b| b.land_base_price);
                reserve > Self::hold_value(world, me, *block)
            })
            .map(|block| Command::OfferForAuction {
                item: AuctionItem::Land(block),
            })
    }

    fn build(world: &World, me: PlayerId) -> Option<Command> {
        let config = world.config();
        let cfg = &config.ai.landlord;
        let apartments = kit::apartments_type(config, world.clock().year())?;
        kit::held_lots(world, me)
            .into_iter()
            .filter(|block| !Self::is_late(world, *block))
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
        kit::open_lots(world)
            .into_iter()
            .filter(|(id, base)| self.max_bid(world, me, *id).is_some_and(|max| *base <= max))
            .min_by_key(|(id, base)| (*base, *id))
            .map(|(block, bid)| Command::OpenLandAuction { block, bid })
    }
}

impl Brain for Landlord {
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        let jitter = world.config().ai.landlord.bid_jitter.rate();
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
