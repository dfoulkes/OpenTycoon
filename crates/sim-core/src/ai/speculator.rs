//! The Speculator: "bids hard and flips" (§10).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `speculator` (plus the `speculator` personality's tier weights); it
//! draws nothing at random.
//!
//! It buys cheap and sells on. It takes the most risk of the roster: its bids run on credit
//! (R7) and its buffer is the smallest, so it is the likeliest to be caught short at midnight.
//!
//! 1. **Auction turns, every tick** ([`Speculator::max_for`]):
//!    - a lease the city sells (a fresh lot, or one whose lease expired, R9) of a tier it
//!      wants: up to `1 + max_markup` of its base price, within `aggression × bid ceiling`,
//!      credit included;
//!    - a lease or business a player offers (R5, R33), the distressed and the expiring: up to
//!      `bargain_share` of its book value (land value, §3; build cost, R41), and within
//!      `aggression × bid ceiling`.
//!
//!    Never while it already holds `max_holdings` purchases.
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` to midnight, a negative balance (R3) sells a building to
//!    the city (R5) or offers a lease at auction.
//! 3. **Flip.** A purchase held `hold_hours` goes back up for auction (R5, R33). Unsold, it is
//!    offered again after another `hold_hours`.
//! - **Apartment blocks (R66).** When the day's reading says apartment blocks earn `threshold`
//!   more per pound than land ([`kit::opportunity`]), it builds one on a lease it holds, or
//!   opens an auction on the city's cheapest apartment block at its build cost (R28, R41), and
//!   bids up to build cost on one the city sells; never past `max_outside_share` of its
//!   holdings. It lets them by vacancy ([`kit::rent`]) and re-lists them after
//!   `apartment_hold_hours`, having collected some rent.
//! 4. **Buy.** Opens an auction on the cheapest lot it wants at its base price (R23), keeping
//!    `cash_buffer`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, Line, Opportunist, Temper};
use crate::auctions::{Auction, AuctionItem};
use crate::businesses::BusinessId;
use crate::economy::{AccountId, PlayerId};
use crate::land::BlockId;
use crate::money::{Money, Rate};
use crate::rng::SimRng;
use crate::world::{Command, World};

/// Something it holds to sell on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum Holding {
    Lease(BlockId),
    Business(BusinessId),
}

impl Holding {
    fn item(self) -> AuctionItem {
        match self {
            Holding::Lease(b) => AuctionItem::Land(b),
            Holding::Business(b) => AuctionItem::Business(b),
        }
    }
}

/// The Speculator's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Speculator {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
    cadence: Cadence,
    temper: Temper,
    /// What it holds, and the tick it got it (or last offered it).
    holdings: BTreeMap<Holding, u64>,
    /// The day's reading of what each line earns, and the rents on its flats (R66).
    opportunist: Opportunist,
}

impl Speculator {
    /// The Speculator playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Speculator {
            personality: personality.into(),
            ..Speculator::default()
        }
    }

    fn held_now(world: &World, me: PlayerId) -> Vec<Holding> {
        let mut out: Vec<Holding> = kit::held_lots(world, me)
            .into_iter()
            .map(Holding::Lease)
            .collect();
        out.extend(
            world
                .businesses()
                .owned_by(AccountId::Player(me))
                .map(|(id, _)| Holding::Business(id)),
        );
        out
    }

    fn has_room(world: &World, me: PlayerId) -> bool {
        let max = world.config().ai.speculator.max_holdings;
        u32::try_from(Self::held_now(world, me).len()).unwrap_or(u32::MAX) < max
    }

    /// The most it bids on what is up for auction now: a fresh lease from the city up to
    /// `1 + max_markup` of its base price; a player's lease or business up to `bargain_share` of
    /// its book value; both within `aggression × bid ceiling` (credit included, R7). `None` if
    /// it doesn't want it.
    pub fn max_for(&self, world: &World, me: PlayerId, auction: &Auction) -> Option<Money> {
        let cfg = &world.config().ai.speculator;
        if !Self::has_room(world, me) {
            return None;
        }
        let cap = kit::share_of_ceiling(world, me, cfg.aggression.rate());
        let from_player = matches!(auction.seller, AccountId::Player(_));
        let book = match auction.item {
            AuctionItem::Land(block) => {
                if !kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), block) {
                    return None;
                }
                if !from_player {
                    let base = world.map().block(block)?.land_base_price;
                    let markup = Rate::from_bp(Rate::ONE.bp() + cfg.max_markup.rate().bp());
                    return Some(markup.apply(base).min(cap));
                }
                kit::lease_book_value(world, block)?
            }
            AuctionItem::Business(id) if from_player => world.businesses().get(id)?.build_cost,
            AuctionItem::Business(id) => return self.max_for_city_block(world, me, id, cap),
            AuctionItem::Utility(_) => return None,
        };
        Some(cfg.bargain_share.rate().apply(book).min(cap))
    }

    /// On an apartment block the city sells (R28): up to its build cost, the auction's minimum
    /// (R41), within `cap`, if apartment blocks pay and it stays within its holdings cap.
    fn max_for_city_block(
        &self,
        world: &World,
        me: PlayerId,
        id: BusinessId,
        cap: Money,
    ) -> Option<Money> {
        let config = world.config();
        let cfg = &config.ai.speculator.opportunism;
        let b = world.businesses().get(id)?;
        let wanted = b.type_id == config.customers.residential.building_type
            && self.opportunist.wants(cfg, Line::Land, Line::Residential)
            && kit::opportunity::within_cap(
                world,
                me,
                Line::Land,
                b.build_cost,
                cfg.max_outside_share.rate(),
            );
        wanted.then_some(b.build_cost.min(cap))
    }

    fn strategy(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let cfg = &world.config().ai.speculator;
        if kit::needs_rescue(world, me, cfg.rescue_after.seconds_since_midnight()) {
            return kit::rescue(world, me).into_iter().collect();
        }
        self.track(world, me);
        let mut out = self.opportunist.let_flats(world, me, &cfg.opportunism);
        if let Some(flip) = self.flip(world) {
            out.push(flip);
        } else if let Some(flats) = self.flats(world, me) {
            out.push(flats);
        } else if let Some(open) = self.open_auction(world, me) {
            out.push(open);
        }
        out
    }

    /// When apartment blocks out-earn land (R66): build one on a lease it holds, else open an
    /// auction on the city's cheapest apartment block at its build cost.
    fn flats(&self, world: &World, me: PlayerId) -> Option<Command> {
        let config = world.config();
        let cfg = &config.ai.speculator;
        let opp = &cfg.opportunism;
        if !Self::has_room(world, me) || !self.opportunist.wants(opp, Line::Land, Line::Residential)
        {
            return None;
        }
        let lots = kit::held_lots(world, me);
        if let Some(plan) =
            kit::opportunity::flats_plan(world, me, opp, Line::Land, &lots, cfg.cash_buffer.money())
        {
            return Some(plan.command());
        }
        if !kit::house_free(world) {
            return None;
        }
        let cap = kit::share_of_ceiling(world, me, cfg.aggression.rate()).min(kit::spare(
            world,
            me,
            cfg.cash_buffer.money(),
        ));
        world
            .businesses()
            .owned_by(AccountId::City)
            .filter(|(id, b)| {
                self.max_for_city_block(world, me, *id, cap)
                    .is_some_and(|max| b.build_cost <= max)
            })
            .min_by_key(|(id, b)| (b.build_cost, *id))
            .map(|(business, b)| Command::OpenBusinessAuction {
                business,
                bid: b.build_cost,
            })
    }

    /// Notes new purchases (from now) and forgets what it no longer holds.
    fn track(&mut self, world: &World, me: PlayerId) {
        let now = world.clock().tick();
        let held = Self::held_now(world, me);
        self.holdings.retain(|h, _| held.contains(h));
        for h in held {
            self.holdings.entry(h).or_insert(now);
        }
    }

    fn flip(&mut self, world: &World) -> Option<Command> {
        if !kit::house_free(world) {
            return None;
        }
        let now = world.clock().tick();
        let per_hour = u64::from(world.clock().rules().ticks_per_day() / 24);
        let cfg = &world.config().ai.speculator;
        let apartments = &world.config().customers.residential.building_type;
        let hold = |h: &Holding| {
            let hours = match h {
                Holding::Business(id)
                    if world
                        .businesses()
                        .get(*id)
                        .is_some_and(|b| &b.type_id == apartments) =>
                {
                    cfg.apartment_hold_hours
                }
                _ => cfg.hold_hours,
            };
            u64::from(hours) * per_hour
        };
        let (holding, since) = self
            .holdings
            .iter_mut()
            .find(|(h, since)| now >= since.saturating_add(hold(h)))?;
        *since = now;
        Some(Command::OfferForAuction {
            item: holding.item(),
        })
    }

    fn open_auction(&self, world: &World, me: PlayerId) -> Option<Command> {
        let cfg = &world.config().ai.speculator;
        if !kit::house_free(world) || !Self::has_room(world, me) {
            return None;
        }
        let cap = kit::share_of_ceiling(world, me, cfg.aggression.rate()).min(kit::spare(
            world,
            me,
            cfg.cash_buffer.money(),
        ));
        kit::open_lots(world)
            .into_iter()
            .filter(|(id, base)| {
                *base <= cap
                    && kit::wants_block(world, &self.personality, cfg.min_tier_weight.rate(), *id)
            })
            .min_by_key(|(id, base)| (*base, *id))
            .map(|(block, bid)| Command::OpenLandAuction { block, bid })
    }
}

impl Brain for Speculator {
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        self.opportunist.observe(world, me);
        let jitter = world.config().ai.speculator.bid_jitter.rate();
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
