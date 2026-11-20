//! What every personality shares (§10: "all of them share one evaluation function with
//! different weights"): reading its weights, the era's buildable types, the lots it holds, its
//! auction turn, a decision cadence, running shops and the late-evening rescue.
//!
//! Everything here reads only the world's public state and returns [`Command`]s, so a
//! personality built from it plays exactly like a client (R40).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::find_site;
use super::grocer::{initial_order, next_order};
use crate::auctions::{Auction, AuctionItem, AuctionPhase};
use crate::businesses::{BusinessId, FitOutLevel, Hours, build_cost};
use crate::config::Config;
use crate::config::businesses::BusinessType;
use crate::economy::{AccountId, PlayerId, net_worth::land_value};
use crate::land::{BlockId, Footprint, Tier};
use crate::money::{Money, Rate};
use crate::world::{Command, World};

/// True if `kind` can be built in `year`: unlocked (`unlock_year`) and not yet obsolete
/// (`obsolete_year`, if set) (§7.1, §7.4).
pub fn in_era(kind: &BusinessType, year: i32) -> bool {
    kind.unlock_year <= year && kind.obsolete_year.is_none_or(|end| year < end)
}

/// Retail types (goods, and a day or night window) buildable in `year`, dearest best good
/// first, then by id: the order a personality with no preference falls back to.
pub fn retail_types(config: &Config, year: i32, hours: &[Hours]) -> Vec<String> {
    let goods = &config.businesses.goods;
    let mut found: Vec<(Money, &String)> = config
        .businesses
        .types
        .iter()
        .filter(|(_, k)| in_era(k, year) && !k.goods.is_empty() && hours.contains(&k.hours))
        .map(|(id, k)| {
            let dearest = k
                .goods
                .iter()
                .filter_map(|g| goods.get(g).map(|g| g.ref_price.money()))
                .max()
                .unwrap_or(Money::ZERO);
            (dearest, id)
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    found.into_iter().map(|(_, id)| id.clone()).collect()
}

/// `preferred` types that are in `available`, in preference order; if none is, all of
/// `available`. So a config list steers the choice while era content keeps it working.
pub fn prefer(preferred: &[String], available: Vec<String>) -> Vec<String> {
    let picked: Vec<String> = preferred
        .iter()
        .filter(|t| available.contains(t))
        .cloned()
        .collect();
    if picked.is_empty() { available } else { picked }
}

/// The residential type (`customers.yaml` `residential.building_type`) if it can be built in
/// `year` (§7.2).
pub fn apartments_type(config: &Config, year: i32) -> Option<&str> {
    let id = config.customers.residential.building_type.as_str();
    config
        .businesses
        .types
        .get(id)
        .filter(|k| in_era(k, year))
        .map(|_| id)
}

/// A personality's weight for `tier` (`ai.yaml` `personalities.<id>.weights`).
pub fn tier_weight(config: &Config, personality: &str, tier: Tier) -> Rate {
    config
        .ai
        .personalities
        .get(personality)
        .map_or(Rate::ZERO, |p| match tier {
            Tier::Low => p.weights.tier_low.rate(),
            Tier::Medium => p.weights.tier_medium.rate(),
            Tier::High => p.weights.tier_high.rate(),
        })
}

/// True if `block` is of a tier the personality weighs at least `min`.
pub fn wants_block(world: &World, personality: &str, min: Rate, block: BlockId) -> bool {
    world
        .map()
        .block(block)
        .is_some_and(|b| tier_weight(world.config(), personality, b.tier) >= min)
}

/// Lots `me` leases, in block order.
pub fn held_lots(world: &World, me: PlayerId) -> Vec<BlockId> {
    let account = AccountId::Player(me);
    world
        .map()
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.lease.as_ref().is_some_and(|l| l.holder == account))
        .filter_map(|(i, _)| u32::try_from(i).ok().map(BlockId))
        .collect()
}

/// Blocks a player may open an auction on now (§5.1), with their base price (R23), in block
/// order.
pub fn open_lots(world: &World) -> Vec<(BlockId, Money)> {
    world
        .map()
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| Some((BlockId(u32::try_from(i).ok()?), b.land_base_price)))
        .filter(|(id, _)| world.map().is_open_for_bids(*id))
        .collect()
}

/// True if the auction house will take an opening now: nothing running and nothing queued.
pub fn house_free(world: &World) -> bool {
    !world.auctions().is_busy() && world.auctions().queue().is_empty()
}

/// A lease's book value now: `last price × years left ÷ term` (§3, R39). `None` if unleased.
pub fn lease_book_value(world: &World, block: BlockId) -> Option<Money> {
    let lease = world.map().block(block)?.lease.as_ref()?;
    let left = lease.remaining_years(world.clock().year());
    Some(land_value(lease.last_price, left, lease.term_years))
}

/// Years left on a block's lease now, and its term. `None` if unleased.
pub fn lease_years(world: &World, block: BlockId) -> Option<(u32, u32)> {
    let lease = world.map().block(block)?.lease.as_ref()?;
    Some((
        lease.remaining_years(world.clock().year()),
        lease.term_years,
    ))
}

/// The share of a full term a lease on `block` still has: `years left ÷ term` for a lease
/// someone holds (they can only sell what is left, R33), 1 for a fresh lease from the city.
pub fn term_share(world: &World, block: BlockId) -> Rate {
    match lease_years(world, block) {
        Some((left, term)) if term > 0 => {
            Rate::from_bp(i64::from(left) * crate::money::BASIS_POINTS_PER_UNIT / i64::from(term))
        }
        _ => Rate::ONE,
    }
}

/// The value of every building on `block` not owned by `me` (build cost, R41): the 20% lot
/// uplift (R39) and the land rent (§5.2) a leaseholder collects come from these.
pub fn others_property_on(world: &World, me: PlayerId, block: BlockId) -> Money {
    world
        .businesses()
        .on_block(block)
        .filter(|(_, b)| b.owner != AccountId::Player(me))
        .map(|(_, b)| b.build_cost)
        .sum()
}

/// Land rent per day that buildings not owned by `me` pay the holder of `block` (§5.3).
pub fn others_rent_on(world: &World, me: PlayerId, block: BlockId) -> Money {
    let config = world.config();
    let Some(tier) = world.map().block(block).map(|b| b.tier) else {
        return Money::ZERO;
    };
    let mult = config.map.tiers.get(tier).desirability_mult.rate();
    world
        .businesses()
        .on_block(block)
        .filter(|(_, b)| b.owner != AccountId::Player(me))
        .map(|(_, b)| {
            crate::land::land_rent_per_day(b.volume(), config.map.land_base_rate.money(), mult)
        })
        .sum()
}

/// True if `block` has a tube station open this year (§4.3, `map.yaml` `tube_stations`).
pub fn has_station(world: &World, block: BlockId) -> bool {
    let Some(key) = world.map().block(block).map(|b| &b.key) else {
        return false;
    };
    let year = world.clock().year();
    world
        .config()
        .map
        .tube_stations
        .iter()
        .any(|s| &s.block == key && s.opens <= year)
}

/// `balance − buffer`: what it can spend and still keep its buffer.
pub fn spare(world: &World, me: PlayerId, buffer: Money) -> Money {
    world.ledger().balance(AccountId::Player(me)) - buffer
}

/// `share × bid ceiling` (R7), or nothing if it can't bid.
pub fn share_of_ceiling(world: &World, me: PlayerId, share: Rate) -> Money {
    world
        .bid_ceiling(me)
        .map_or(Money::ZERO, |c| share.apply(c).min(c))
}

/// Its move on its turn in the current auction: raise by the smallest increment while the new
/// bid stays within `max(auction)`, otherwise pass (R8). `None` if it is not its turn, or it
/// already leads, or it is the seller.
pub fn auction_turn(
    world: &World,
    me: PlayerId,
    max: impl FnOnce(&Auction) -> Option<Money>,
) -> Option<Command> {
    let auction = world.auctions().current()?;
    let AuctionPhase::Bidding { turn, .. } = auction.phase else {
        return None;
    };
    if turn != me || auction.leader == Some(me) || auction.seller == AccountId::Player(me) {
        return None;
    }
    let by = world.auctions().rules().increments.iter().min().copied();
    let raise = by
        .zip(max(auction))
        .filter(|(by, max)| auction.high_bid + *by <= *max)
        .map(|(by, _)| Command::Raise { by });
    Some(raise.unwrap_or(Command::Pass))
}

/// The block an auction sells, if it sells land.
pub fn land_item(auction: &Auction) -> Option<BlockId> {
    match auction.item {
        AuctionItem::Land(block) => Some(block),
        _ => None,
    }
}

/// True from `after` until midnight while its balance is below the solvency line (R3).
pub fn needs_rescue(world: &World, me: PlayerId, after: u32) -> bool {
    world.clock().second_of_day() >= after
        && world.ledger().balance(AccountId::Player(me))
            < world.config().game.bankruptcy.solvent_at_or_above.money()
}

/// Caught short late in the day: sell its cheapest building to the city (R5, lowest id on a
/// tie); with no building, offer its most valuable lease at auction (R33) if the house is free.
pub fn rescue(world: &World, me: PlayerId) -> Option<Command> {
    let account = AccountId::Player(me);
    if let Some((business, _)) = world
        .businesses()
        .owned_by(account)
        .min_by_key(|(id, b)| (b.build_cost, *id))
    {
        return Some(Command::SellToCity { business });
    }
    if !house_free(world) {
        return None;
    }
    held_lots(world, me)
        .into_iter()
        .max_by_key(|b| (lease_book_value(world, *b).unwrap_or(Money::ZERO), *b))
        .map(|block| Command::OfferForAuction {
            item: AuctionItem::Land(block),
        })
}

/// How keen it is on the item now up for auction: a factor drawn once per auction from its own
/// RNG stream, uniform in `[1 − bid_jitter, 1]`, that scales its maximum bid. It never raises a
/// maximum, so a personality's ceiling holds; it only makes the same personality bid a little
/// differently from game to game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Temper {
    current: Option<(AuctionItem, Rate)>,
}

impl Temper {
    /// The factor for the auction on `item`, drawn when a new auction starts.
    pub fn factor(
        &mut self,
        item: AuctionItem,
        jitter: Rate,
        rng: &mut crate::rng::SimRng,
    ) -> Rate {
        if let Some((seen, f)) = self.current
            && seen == item
        {
            return f;
        }
        let spread =
            u64::try_from(jitter.bp().clamp(0, crate::money::BASIS_POINTS_PER_UNIT)).unwrap_or(0);
        let off = i64::try_from(crate::rng::below(rng, spread + 1)).unwrap_or(0);
        let f = Rate::from_bp(Rate::ONE.bp() - off);
        self.current = Some((item, f));
        f
    }

    /// Forgets the auction once the house is quiet, so a re-listed item draws afresh.
    pub fn settle(&mut self, world: &World) {
        if world.auctions().current().is_none() {
            self.current = None;
        }
    }
}

/// Its auction turn ([`auction_turn`]) with its maximum scaled by its [`Temper`].
pub fn tempered_turn(
    world: &World,
    me: PlayerId,
    temper: &mut Temper,
    jitter: Rate,
    rng: &mut crate::rng::SimRng,
    max: impl FnOnce(&Auction) -> Option<Money>,
) -> Option<Command> {
    temper.settle(world);
    auction_turn(world, me, |a| {
        let f = temper.factor(a.item, jitter, rng);
        max(a).map(|m| f.apply(m))
    })
}

/// One decision every `decision_interval_ticks` (`ai.yaml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Cadence {
    last: Option<u64>,
}

impl Cadence {
    /// True (and remembered) if a decision is due this tick.
    pub fn due(&mut self, world: &World) -> bool {
        let now = world.clock().tick();
        let interval = u64::from(world.config().ai.decision_interval_ticks);
        if self
            .last
            .is_some_and(|last| now < last.saturating_add(interval))
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// A building it plans: type, where, how tall, what fit-out, and what it costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Block.
    pub block: BlockId,
    /// Business type id.
    pub type_id: String,
    /// Footprint.
    pub footprint: Footprint,
    /// Height.
    pub height: u32,
    /// Fit-out.
    pub fit_out: FitOutLevel,
    /// Build cost (§5.3).
    pub cost: Money,
}

impl Plan {
    /// The command that builds it.
    pub fn command(self) -> Command {
        Command::Build {
            block: self.block,
            type_id: self.type_id,
            footprint: self.footprint,
            height: self.height,
            fit_out: self.fit_out,
        }
    }
}

/// A legal site for `type_id` on `block` ([`find_site`]), at `height` capped by the type's
/// `max_height` (R58), priced at `fit_out` (§7.1).
pub fn plan_on(
    world: &World,
    me: PlayerId,
    block: BlockId,
    type_id: &str,
    size: [u32; 2],
    height: u32,
    fit_out: FitOutLevel,
) -> Option<Plan> {
    let config = world.config();
    let height = height.min(config.businesses.types.get(type_id)?.max_height);
    let (footprint, _) = find_site(world, AccountId::Player(me), block, type_id, size, height)?;
    let tier = world.map().block(block)?.tier;
    let cost = build_cost(
        height,
        footprint,
        config.businesses.build_base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
        fit_out_mult(config, fit_out),
    );
    Some(Plan {
        block,
        type_id: type_id.to_owned(),
        footprint,
        height,
        fit_out,
        cost,
    })
}

/// A fit-out level's build-cost multiplier (§7.1).
pub fn fit_out_mult(config: &Config, level: FitOutLevel) -> Rate {
    let f = &config.businesses.fit_out;
    match level {
        FitOutLevel::Low => f.low.build_cost_mult,
        FitOutLevel::Medium => f.medium.build_cost_mult,
        FitOutLevel::High => f.high.build_cost_mult,
    }
    .rate()
}

/// What one `size` × `height` building of `type_id` at `fit_out` costs on a `tier` lot, before
/// it has a site (§5.3).
pub fn building_cost(
    config: &Config,
    tier: Tier,
    type_id: &str,
    size: [u32; 2],
    height: u32,
    fit_out: FitOutLevel,
) -> Option<Money> {
    let height = height.min(config.businesses.types.get(type_id)?.max_height);
    Some(build_cost(
        height,
        Footprint {
            x: 0,
            y: 0,
            w: size[0],
            d: size[1],
        },
        config.businesses.build_base_rate.money(),
        config.map.tiers.get(tier).desirability_mult.rate(),
        fit_out_mult(config, fit_out),
    ))
}

/// How many buildings `me` owns on `block` whose type satisfies `is`.
pub fn owned_on(world: &World, me: PlayerId, block: BlockId, is: impl Fn(&str) -> bool) -> u32 {
    let n = world
        .businesses()
        .on_block(block)
        .filter(|(_, b)| b.owner == AccountId::Player(me) && is(&b.type_id))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A shopkeeper's book: which shops it has priced, and when it last set each window's orders.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ShopBook {
    known: BTreeSet<BusinessId>,
    ordered: BTreeMap<Hours, u32>,
}

impl ShopBook {
    /// Prices each new shop's goods at `price` and orders `first_share` of its capacity.
    pub fn open_new(
        &mut self,
        world: &World,
        me: PlayerId,
        price: Rate,
        first_share: Rate,
    ) -> Vec<Command> {
        let mut out = Vec::new();
        for (id, shop) in world.businesses().owned_by(AccountId::Player(me)) {
            if shop.prices.is_empty() || !self.known.insert(id) {
                continue;
            }
            for good in shop.prices.keys() {
                out.push(Command::SetPrice {
                    business: id,
                    good: good.clone(),
                    price,
                });
                out.push(Command::SetOrder {
                    business: id,
                    good: good.clone(),
                    quantity: initial_order(shop.capacity_per_good, first_share),
                });
            }
        }
        out
    }

    /// Once each trading window has closed for the day, sets tomorrow's order for every shop
    /// trading in it from today's sales ([`next_order`], R2, R10).
    pub fn evening_orders(&mut self, world: &World, me: PlayerId, margin: Rate) -> Vec<Command> {
        let rules = world.clock().rules();
        let now = world.clock().second_of_day();
        let today = world.clock().day();
        let mut out = Vec::new();
        for (hours, close) in [
            (Hours::Day, rules.trading.day.1),
            (Hours::Night, rules.trading.night.1),
        ] {
            if now < close || self.ordered.get(&hours) == Some(&today) {
                continue;
            }
            self.ordered.insert(hours, today);
            for (id, shop) in world.businesses().owned_by(AccountId::Player(me)) {
                if shop.hours != hours || !self.known.contains(&id) {
                    continue;
                }
                for good in shop.prices.keys() {
                    if shop.tomorrow_order.get(good).copied().unwrap_or(0) <= 0 {
                        continue; // nothing arrived today, so today's sales say nothing
                    }
                    let sold = shop.sold_today.get(good).copied().unwrap_or(0);
                    let order = next_order(sold, shop.capacity_per_good, margin);
                    if shop.standing_order.get(good).copied() != Some(order) {
                        out.push(Command::SetOrder {
                            business: id,
                            good: good.clone(),
                            quantity: order,
                        });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(unlock: i32, obsolete: Option<i32>) -> BusinessType {
        let mut config = crate::economy::test_support::shipped_config();
        let mut k = config.businesses.types.remove("grocery").unwrap();
        k.unlock_year = unlock;
        k.obsolete_year = obsolete;
        k
    }

    #[test]
    fn era_is_unlock_inclusive_and_obsolete_exclusive() {
        assert!(in_era(&kind(1930, None), 1930));
        assert!(!in_era(&kind(1935, None), 1930));
        assert!(in_era(&kind(1930, Some(1950)), 1945));
        assert!(!in_era(&kind(1930, Some(1950)), 1950));
    }

    #[test]
    fn preferences_steer_but_never_strand() {
        let avail = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(prefer(&["b".into(), "z".into()], avail.clone()), vec!["b"]);
        assert_eq!(prefer(&["z".into()], avail.clone()), avail);
        assert_eq!(prefer(&[], avail.clone()), avail);
    }
}
