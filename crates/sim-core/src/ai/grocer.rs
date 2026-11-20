//! The Grocer: "opens many cheap Low-tier shops" (§10; the one v1 AI, §10c).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `grocer` (plus the `grocer` personality's tier weights); every random
//! draw comes from the RNG it is handed, which for an AI seat is the seat's own stream.
//!
//! What it does, in priority order:
//!
//! 1. **Auction turns, every tick.** On its turn in a land auction for a tier it wants
//!    (personality weight ≥ `min_tier_weight`, i.e. Low), it raises by the smallest increment
//!    while the new bid stays within [`Grocer::max_bid`]: `max_bid_share` of its bid ceiling
//!    (R7), and never so much that it couldn't then build one shop and keep its cash buffer.
//!    Otherwise it passes (R8).
//!
//! Then, once every `decision_interval_ticks`:
//!
//! 2. **Rescue.** From `rescue_after` until midnight, if its balance is below the solvency line
//!    (R3) it sells its cheapest building back to the city (R5) rather than take a strike.
//! 3. **New shops.** Prices each good at `1 − working_class_discount` of reference, ± up to
//!    `price_jitter` from its RNG, and sets a first order of `initial_order_share` of capacity.
//! 4. **Evening orders.** Once day trade has closed, sets each stocked shop's order for
//!    tomorrow from today's sales: [`next_order`] (R2, R10).
//! 5. **Build.** On a lot it leases, puts up the next shop type in rotation at the first legal
//!    site ([`super::find_site`], R22, R58), if it can pay and still keep `cash_buffer`.
//! 6. **Land.** Only when every lot it holds is full: opens an auction on the cheapest lot it
//!    wants at its land base price (R23), if that is within [`Grocer::max_bid`].

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Brain, find_site};
use crate::auctions::{AuctionItem, AuctionPhase};
use crate::businesses::{BusinessId, FitOutLevel, Hours, build_cost};
use crate::config::Config;
use crate::config::ai::{GrocerConfig, PersonalityWeights};
use crate::economy::{AccountId, PlayerId};
use crate::land::{BlockId, Tier};
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::rng::{SimRng, below};
use crate::world::{Command, World};

/// The Grocer's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Grocer {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
    /// Tick of its last strategic decision.
    last_decision: Option<u64>,
    /// Shops it has priced and given a first order.
    known: BTreeSet<BusinessId>,
    /// The day it last set evening orders.
    ordered_on_day: Option<u32>,
}

impl Grocer {
    /// A Grocer playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Grocer {
            personality: personality.into(),
            ..Grocer::default()
        }
    }

    fn weights<'a>(&self, config: &'a Config) -> Option<&'a PersonalityWeights> {
        config
            .ai
            .personalities
            .get(&self.personality)
            .map(|p| &p.weights)
    }

    /// True if the Grocer wants lots of this tier: its personality weight for the tier is at
    /// least `min_tier_weight`.
    pub fn wants_tier(&self, config: &Config, tier: Tier) -> bool {
        let Some(w) = self.weights(config) else {
            return false;
        };
        let weight = match tier {
            Tier::Low => w.tier_low,
            Tier::Medium => w.tier_medium,
            Tier::High => w.tier_high,
        };
        weight.rate() >= config.ai.grocer.min_tier_weight.rate()
    }

    /// The most it will bid for `block` now: `max_bid_share × bid ceiling` (R7), and no more
    /// than leaves it enough to build one shop there and keep `cash_buffer`. `None` if it
    /// doesn't want the block or can't bid at all.
    pub fn max_bid(&self, world: &World, me: PlayerId, block: BlockId) -> Option<Money> {
        let config = world.config();
        let cfg = &config.ai.grocer;
        let tier = world.map().block(block)?.tier;
        if !self.wants_tier(config, tier) {
            return None;
        }
        let ceiling = world.bid_ceiling(me)?;
        let balance = world.ledger().balance(AccountId::Player(me));
        let shop = one_shop_cost(config, tier, &Self::next_shop_type(world, me)?)?;
        let share = cfg.max_bid_share.rate().apply(ceiling);
        let keep = balance - cfg.cash_buffer.money() - shop;
        Some(share.min(keep).min(ceiling))
    }

    /// The shop type it builds next: `shop_types` in rotation by how many shops it owns.
    fn next_shop_type(world: &World, me: PlayerId) -> Option<String> {
        let types = &world.config().ai.grocer.shop_types;
        let owned = world
            .businesses()
            .owned_by(AccountId::Player(me))
            .filter(|(_, b)| b.hours != Hours::None)
            .count();
        types.get(owned % types.len().max(1)).cloned()
    }

    /// Phase 1: its move if it is its turn in an auction.
    fn auction_turn(&self, world: &World, me: PlayerId) -> Option<Command> {
        let auction = world.auctions().current()?;
        let AuctionPhase::Bidding { turn, .. } = auction.phase else {
            return None;
        };
        if turn != me || auction.leader == Some(me) {
            return None;
        }
        let by = world.auctions().rules().increments.iter().min().copied();
        let raise = match (auction.item, by) {
            (AuctionItem::Land(block), Some(by)) => self
                .max_bid(world, me, block)
                .filter(|max| auction.high_bid + by <= *max)
                .map(|_| Command::Raise { by }),
            _ => None,
        };
        Some(raise.unwrap_or(Command::Pass))
    }

    /// Phases 2–6, once per decision interval.
    fn strategy(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        let config = world.config();
        let cfg = &config.ai.grocer;
        let account = AccountId::Player(me);
        let balance = world.ledger().balance(account);
        let now = world.clock().second_of_day();

        if now >= cfg.rescue_after.seconds_since_midnight()
            && balance < config.game.bankruptcy.solvent_at_or_above.money()
        {
            return rescue(world, account).into_iter().collect();
        }

        let mut out = self.price_new_shops(world, account, cfg, rng);
        let close = world.clock().rules().trading.day.1;
        let today = world.clock().day();
        if now >= close && self.ordered_on_day != Some(today) {
            self.ordered_on_day = Some(today);
            out.extend(self.evening_orders(world, account, cfg));
        }
        if let Some(build) = Self::build(world, me, balance, cfg) {
            out.push(build);
        } else if !Self::has_free_site(world, me)
            && let Some(open) = self.open_auction(world, me)
        {
            out.push(open);
        }
        out
    }

    fn price_new_shops(
        &mut self,
        world: &World,
        account: AccountId,
        cfg: &GrocerConfig,
        rng: &mut SimRng,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        for (id, shop) in world.businesses().owned_by(account) {
            if shop.prices.is_empty() || !self.known.insert(id) {
                continue;
            }
            for good in shop.prices.keys() {
                out.push(Command::SetPrice {
                    business: id,
                    good: good.clone(),
                    price: shop_price(cfg, rng),
                });
                out.push(Command::SetOrder {
                    business: id,
                    good: good.clone(),
                    quantity: initial_order(shop.capacity_per_good, cfg.initial_order_share.rate()),
                });
            }
        }
        out
    }

    fn evening_orders(
        &self,
        world: &World,
        account: AccountId,
        cfg: &GrocerConfig,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        for (id, shop) in world.businesses().owned_by(account) {
            if !self.known.contains(&id) {
                continue;
            }
            for good in shop.prices.keys() {
                let delivered = shop.tomorrow_order.get(good).copied().unwrap_or(0);
                if delivered <= 0 {
                    continue; // nothing arrived today, so today's sales say nothing
                }
                let sold = shop.sold_today.get(good).copied().unwrap_or(0);
                let order = next_order(sold, shop.capacity_per_good, cfg.order_margin.rate());
                if shop.standing_order.get(good).copied() != Some(order) {
                    out.push(Command::SetOrder {
                        business: id,
                        good: good.clone(),
                        quantity: order,
                    });
                }
            }
        }
        out
    }

    /// Lots it leases, in block order.
    fn held_lots(world: &World, me: PlayerId) -> impl Iterator<Item = BlockId> + '_ {
        world
            .map()
            .blocks
            .iter()
            .enumerate()
            .filter(move |(_, b)| {
                b.lease
                    .as_ref()
                    .is_some_and(|l| l.holder == AccountId::Player(me))
            })
            .filter_map(|(i, _)| u32::try_from(i).ok().map(BlockId))
    }

    fn shop_height(config: &Config, type_id: &str) -> Option<u32> {
        let kind = config.businesses.types.get(type_id)?;
        Some(config.ai.grocer.shop_height.min(kind.max_height))
    }

    fn site_on(
        world: &World,
        me: PlayerId,
        block: BlockId,
    ) -> Option<(String, crate::land::Footprint, u32, Money)> {
        let type_id = Self::next_shop_type(world, me)?;
        let height = Self::shop_height(world.config(), &type_id)?;
        let size = world.config().ai.grocer.shop_footprint;
        let (footprint, cost) =
            find_site(world, AccountId::Player(me), block, &type_id, size, height)?;
        Some((type_id, footprint, height, cost))
    }

    fn has_free_site(world: &World, me: PlayerId) -> bool {
        Self::held_lots(world, me).any(|block| Self::site_on(world, me, block).is_some())
    }

    /// Phase 5: one shop on the first lot with room, if it can pay and keep its buffer.
    fn build(world: &World, me: PlayerId, balance: Money, cfg: &GrocerConfig) -> Option<Command> {
        Self::held_lots(world, me).find_map(|block| {
            let (type_id, footprint, height, cost) = Self::site_on(world, me, block)?;
            let affordable =
                balance - cost >= cfg.cash_buffer.money() && world.can_afford(me, cost);
            affordable.then_some(Command::Build {
                block,
                type_id,
                footprint,
                height,
                fit_out: FitOutLevel::Medium,
            })
        })
    }

    /// Phase 6: an opening bid at the base price on the cheapest lot it wants and can afford.
    fn open_auction(&self, world: &World, me: PlayerId) -> Option<Command> {
        let auctions = world.auctions();
        if auctions.is_busy() || !auctions.queue().is_empty() {
            return None;
        }
        world
            .map()
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| Some((BlockId(u32::try_from(i).ok()?), b.land_base_price)))
            .filter(|(id, _)| world.map().is_open_for_bids(*id))
            .filter(|(id, base)| self.max_bid(world, me, *id).is_some_and(|max| *base <= max))
            .min_by_key(|(id, base)| (*base, *id))
            .map(|(block, bid)| Command::OpenLandAuction { block, bid })
    }
}

impl Brain for Grocer {
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command> {
        let mut out: Vec<Command> = self.auction_turn(world, me).into_iter().collect();
        let now = world.clock().tick();
        let interval = u64::from(world.config().ai.decision_interval_ticks);
        if self
            .last_decision
            .is_some_and(|last| now < last.saturating_add(interval))
        {
            return out;
        }
        self.last_decision = Some(now);
        out.extend(self.strategy(world, me, rng));
        out
    }
}

/// Its cheapest building, sold back to the city (R5), lowest id on a tie.
fn rescue(world: &World, account: AccountId) -> Option<Command> {
    world
        .businesses()
        .owned_by(account)
        .min_by_key(|(id, b)| (b.build_cost, *id))
        .map(|(business, _)| Command::SellToCity { business })
}

/// What one of its shops costs on a lot of `tier` (§5.3), before it has a site.
fn one_shop_cost(config: &Config, tier: Tier, type_id: &str) -> Option<Money> {
    let [w, d] = config.ai.grocer.shop_footprint;
    let height = Grocer::shop_height(config, type_id)?;
    Some(build_cost(
        height,
        crate::land::Footprint { x: 0, y: 0, w, d },
        config.businesses.build_base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
        config.businesses.fit_out.medium.build_cost_mult.rate(),
    ))
}

/// Its price for a good, as a share of reference: `1 − working_class_discount`, plus a whole
/// number of basis points drawn uniformly from `±price_jitter`. Never below one basis point.
pub fn shop_price(cfg: &GrocerConfig, rng: &mut SimRng) -> Rate {
    let jitter = cfg.price_jitter.rate().bp().max(0);
    let spread = u64::try_from(2 * jitter + 1).unwrap_or(1);
    let draw = i64::try_from(below(rng, spread)).unwrap_or(0) - jitter;
    let base = BASIS_POINTS_PER_UNIT - cfg.working_class_discount.rate().bp();
    Rate::from_bp((base + draw).max(1))
}

/// A new shop's first order: `share × capacity`, rounded down, at least 1 and at most capacity
/// (R10). 0 only if the shop holds nothing.
pub fn initial_order(capacity: i64, share: Rate) -> i64 {
    let units =
        i128::from(capacity.max(0)) * i128::from(share.bp()) / i128::from(BASIS_POINTS_PER_UNIT);
    i64::try_from(units)
        .unwrap_or(i64::MAX)
        .clamp(1, capacity.max(1))
        .min(capacity.max(0))
}

/// Tomorrow's order from today's sales (R2, R10): `⌈sold × (1 + margin)⌉`, at least 1 so a
/// shop never stops trading, and never more than the shop holds. A shop that sold out grows by
/// the margin; one that sold half shrinks to half plus the margin, so it doesn't order into
/// waste. 0 only if the shop holds nothing.
pub fn next_order(sold: i64, capacity: i64, margin: Rate) -> i64 {
    let num = i128::from(sold.max(0)) * i128::from(BASIS_POINTS_PER_UNIT + margin.bp().max(0));
    let units = num.div_euclid(i128::from(BASIS_POINTS_PER_UNIT))
        + i128::from(num.rem_euclid(i128::from(BASIS_POINTS_PER_UNIT)) != 0);
    i64::try_from(units)
        .unwrap_or(i64::MAX)
        .clamp(1, capacity.max(1))
        .min(capacity.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TEN_PERCENT: Rate = Rate::from_bp(1_000);

    #[test]
    fn tomorrow_follows_today_plus_the_margin() {
        assert_eq!(next_order(20, 64, TEN_PERCENT), 22);
        assert_eq!(next_order(40, 64, TEN_PERCENT), 44);
        assert_eq!(next_order(21, 64, TEN_PERCENT), 24); // 23.1 rounds up
        assert_eq!(next_order(60, 64, TEN_PERCENT), 64); // capped at capacity
        assert_eq!(next_order(0, 64, TEN_PERCENT), 1); // keep a toe in the market
        assert_eq!(next_order(5, 0, TEN_PERCENT), 0); // a shop that holds nothing
    }

    #[test]
    fn first_order_is_a_share_of_capacity() {
        assert_eq!(initial_order(64, Rate::from_bp(5_000)), 32);
        assert_eq!(initial_order(1, Rate::from_bp(5_000)), 1);
        assert_eq!(initial_order(0, Rate::from_bp(5_000)), 0);
        assert_eq!(initial_order(64, Rate::ONE), 64);
    }

    proptest! {
        /// Orders are never negative and never above capacity (R10), whatever sold.
        #[test]
        fn orders_stay_within_capacity(sold in 0i64..10_000, cap in 0i64..10_000, m in 0i64..=10_000) {
            let order = next_order(sold, cap, Rate::from_bp(m));
            prop_assert!(order >= 0 && order <= cap);
            prop_assert!(order >= sold.min(cap)); // never orders less than it sold
        }

        /// The price stays within the jitter of `1 − discount`.
        #[test]
        fn price_stays_within_its_spread(seed in any::<u64>(), disc in 0i64..5_000, jit in 0i64..1_000) {
            let cfg = GrocerConfig {
                working_class_discount: crate::config::Ratio(Rate::from_bp(disc)),
                price_jitter: crate::config::Ratio(Rate::from_bp(jit)),
                ..test_config()
            };
            let p = shop_price(&cfg, &mut crate::rng::seeded(seed)).bp();
            prop_assert!((p - (10_000 - disc)).abs() <= jit);
        }
    }

    fn test_config() -> GrocerConfig {
        use crate::config::{ClockTime, Pounds, Ratio};
        use crate::money::Money;
        GrocerConfig {
            min_tier_weight: Ratio(Rate::from_bp(5_000)),
            max_bid_share: Ratio(Rate::from_bp(3_000)),
            cash_buffer: Pounds(Money::from_pounds(300)),
            shop_types: vec!["grocery".into()],
            shop_footprint: [3, 3],
            shop_height: 12,
            working_class_discount: Ratio(Rate::from_bp(500)),
            price_jitter: Ratio(Rate::from_bp(200)),
            initial_order_share: Ratio(Rate::from_bp(5_000)),
            order_margin: Ratio(TEN_PERCENT),
            rescue_after: ClockTime::from_hm(22, 0).unwrap_or_default(),
        }
    }
}
