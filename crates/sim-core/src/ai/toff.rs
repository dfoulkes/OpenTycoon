//! The Toff: "only goes for High tier" (§10).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `toff` (plus the `toff` personality's tier weights); it draws nothing
//! at random.
//!
//! What it does, in priority order:
//!
//! 1. **Auction turns, every tick.** Only on a lease of a tier its personality weighs at least
//!    `min_tier_weight` (High). It pays up, to [`Toff::max_bid`]; otherwise it passes (R8).
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` to midnight, a negative balance (R3) sells its cheapest
//!    shop to the city (R5).
//! 3. **New shops.** Prices every good at `1 + price_premium` of reference, for the upper
//!    class, and orders `initial_order_share` of capacity.
//! 4. **Evening orders.** After each trading window closes, tomorrow's order follows today's
//!    demand (sales plus the shoppers turned away once sold out) plus `order_margin`
//!    ([`super::grocer::demand_seen`], [`super::grocer::next_order`], R2, R10).
//! - **Hotel.** Holding a whole colour set, it puts up a Hotel there before anything else
//!   (§7.3; R14, R29), if the era has a Hotel type and it keeps its buffer.
//! - **Luxury flats (R66).** When the day's reading says apartment blocks earn `threshold` more
//!   per pound than its shops ([`kit::opportunity`]), it puts up `opportunism.fit_out` (high)
//!   apartment blocks on its High lots, at most `apartments_per_lot` a lease and never more
//!   than `max_outside_share` of its buildings by build cost, and lets them by vacancy from
//!   `start_rent` (twice the reference rent) ([`kit::rent`]).
//! 5. **Build.** At most `shops_per_lot` shops a lease, `fit_out` (high) fit-out, of the next
//!    type in rotation from `shop_types` that the era allows (else any retail type, dearest
//!    goods first), if it keeps its large `cash_buffer`.
//! 6. **Land.** Opens an auction on the cheapest High lot whose base price is within its
//!    maximum bid (R23).

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, Line, Opportunist, ShopBook, Temper};
use crate::auctions::{Auction, AuctionItem};
use crate::businesses::Hours;
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::{Money, Rate};
use crate::rng::SimRng;
use crate::world::{Command, World};

/// The Toff's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Toff {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
    cadence: Cadence,
    temper: Temper,
    shops: ShopBook,
    /// The day's reading of what each line earns, and the rents on its flats (R66).
    opportunist: Opportunist,
}

impl Toff {
    /// The Toff playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Toff {
            personality: personality.into(),
            ..Toff::default()
        }
    }

    /// Shop types it would build this year, best first.
    fn shop_types(world: &World) -> Vec<String> {
        let config = world.config();
        kit::prefer(
            &config.ai.toff.shop_types,
            kit::retail_types(config, world.clock().year(), &[Hours::Day, Hours::Night]),
        )
    }

    /// The type it builds next: its shop types in rotation by how many shops it owns.
    fn next_type(world: &World, me: PlayerId) -> Option<String> {
        let types = Self::shop_types(world);
        let owned = world
            .businesses()
            .owned_by(AccountId::Player(me))
            .filter(|(_, b)| b.hours != Hours::None)
            .count();
        types.get(owned % types.len().max(1)).cloned()
    }

    /// The most it pays for the lease on `block` now: `aggression × bid ceiling` (R7), scaled
    /// by the share of a term left, and never so much that it couldn't then build one shop
    /// there and keep `cash_buffer`. `None` if the tier is beneath it.
    pub fn max_bid(&self, world: &World, me: PlayerId, block: BlockId) -> Option<Money> {
        let config = world.config();
        let cfg = &config.ai.toff;
        if !kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), block) {
            return None;
        }
        let tier = world.map().block(block)?.tier;
        let shop = kit::building_cost(
            config,
            tier,
            &Self::next_type(world, me)?,
            cfg.shop_footprint,
            cfg.shop_height,
            cfg.fit_out,
        )?;
        let share = kit::share_of_ceiling(world, me, cfg.aggression.rate());
        let keep = kit::spare(world, me, cfg.cash_buffer.money()) - shop;
        Some(kit::term_share(world, block).apply(share).min(keep))
    }

    fn max_for(&self, world: &World, me: PlayerId, auction: &Auction) -> Option<Money> {
        match auction.item {
            AuctionItem::Land(block) => self.max_bid(world, me, block),
            _ => None,
        }
    }

    fn strategy(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let cfg = &world.config().ai.toff;
        if kit::needs_rescue(world, me, cfg.rescue_after.seconds_since_midnight()) {
            return kit::rescue(world, me).into_iter().collect();
        }
        let price = Rate::from_bp(Rate::ONE.bp() + cfg.price_premium.rate().bp());
        let mut out = self
            .shops
            .open_new(world, me, price, cfg.initial_order_share.rate());
        out.extend(
            self.shops
                .evening_orders(world, me, cfg.order_margin.rate()),
        );
        out.extend(self.opportunist.let_flats(world, me, &cfg.opportunism));
        if let Some(build) = self.build(world, me) {
            out.push(build);
        } else if !Self::has_room(world, me)
            && let Some(open) = self.open_auction(world, me)
        {
            out.push(open);
        }
        out
    }

    fn lots_with_room(world: &World, me: PlayerId) -> Vec<kit::Plan> {
        let cfg = &world.config().ai.toff;
        let Some(type_id) = Self::next_type(world, me) else {
            return Vec::new();
        };
        let apartments = &world.config().customers.residential.building_type;
        kit::held_lots(world, me)
            .into_iter()
            .filter(|block| {
                kit::owned_on(world, me, *block, |t| t != apartments) < cfg.shops_per_lot
            })
            .filter_map(|block| {
                kit::plan_on(
                    world,
                    me,
                    block,
                    &type_id,
                    cfg.shop_footprint,
                    cfg.shop_height,
                    cfg.fit_out,
                )
            })
            .collect()
    }

    fn has_room(world: &World, me: PlayerId) -> bool {
        !Self::lots_with_room(world, me).is_empty()
    }

    fn build(&self, world: &World, me: PlayerId) -> Option<Command> {
        let cfg_h = &world.config().ai.toff;
        if let Some(plan) = kit::hotel_plan(
            world,
            me,
            cfg_h.shop_footprint,
            cfg_h.shop_height,
            cfg_h.fit_out,
        ) && kit::spare(world, me, cfg_h.cash_buffer.money()) >= plan.cost
            && world.can_afford(me, plan.cost)
        {
            return Some(plan.command());
        }
        let buffer = world.config().ai.toff.cash_buffer.money();
        if let Some(flats) = self.luxury_flats(world, me) {
            return Some(flats.command());
        }
        Self::lots_with_room(world, me)
            .into_iter()
            .find(|plan| {
                kit::spare(world, me, buffer) >= plan.cost && world.can_afford(me, plan.cost)
            })
            .map(kit::Plan::command)
    }

    /// Luxury flats on a High lot it holds, when flats out-earn its shops (R66).
    fn luxury_flats(&self, world: &World, me: PlayerId) -> Option<kit::Plan> {
        let cfg = &world.config().ai.toff;
        if !self
            .opportunist
            .wants(&cfg.opportunism, Line::Retail, Line::Residential)
        {
            return None;
        }
        let lots: Vec<_> = kit::held_lots(world, me)
            .into_iter()
            .filter(|b| kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), *b))
            .collect();
        kit::opportunity::flats_plan(
            world,
            me,
            &cfg.opportunism,
            Line::Retail,
            &lots,
            cfg.cash_buffer.money(),
        )
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

impl Brain for Toff {
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        self.opportunist.observe(world, me);
        let jitter = world.config().ai.toff.bid_jitter.rate();
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
