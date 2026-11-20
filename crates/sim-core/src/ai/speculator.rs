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
//! 4. **Buy.** Opens an auction on the cheapest lot it wants at its base price (R23), keeping
//!    `cash_buffer`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Brain;
use super::kit::{self, Cadence, Temper};
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
            _ => return None,
        };
        Some(cfg.bargain_share.rate().apply(book).min(cap))
    }

    fn strategy(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let cfg = &world.config().ai.speculator;
        if kit::needs_rescue(world, me, cfg.rescue_after.seconds_since_midnight()) {
            return kit::rescue(world, me).into_iter().collect();
        }
        self.track(world, me);
        if let Some(flip) = self.flip(world) {
            return vec![flip];
        }
        self.open_auction(world, me).into_iter().collect()
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
        let hold = u64::from(world.config().ai.speculator.hold_hours) * per_hour;
        let (holding, since) = self
            .holdings
            .iter_mut()
            .find(|(_, since)| now >= since.saturating_add(hold))?;
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
