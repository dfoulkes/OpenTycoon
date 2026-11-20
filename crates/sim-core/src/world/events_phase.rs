//! The world's side of events and utilities (§6, §9, §9.1; R11, R12, R24): applying what the
//! events module decides to the other modules' state, utility auctions, insurance, and the
//! building-damage hook.
//!
//! Money moves only through the ledger, and only between accounts: war-damage compensation,
//! insurance payouts and nationalisation buy-outs are paid **by the city** (which can't go
//! bust, R4), premiums and fines are paid **to** it. None of them mints money, so the ledger
//! invariant `total = opening balances + minted allowances` (R63) holds through every event.

use std::collections::{BTreeMap, BTreeSet};

use crate::auctions::{AuctionError, AuctionItem, Opening, QueuedSale};
use crate::businesses::{Business, BusinessId};
use crate::config::events::{Effect, IncidentEffect};
use crate::economy::{AccountId, Bill, PlayerId};
use crate::events::{Incident, Party, StepInputs, roll};
use crate::land::BlockId;
use crate::money::{BASIS_POINTS_PER_UNIT, Money, Rate};
use crate::population::residential::ResidentId;
use crate::population::{Class, Cohort, Population};
use crate::utilities::{Utilities, UtilityKind};

use super::World;

impl World {
    /// Phase 3, events' part (after lease expiry): the year step's election, history and
    /// random-era events (§9, §9.1), then rationing caps and closures on the orders just
    /// locked.
    pub(super) fn events_step(&mut self) {
        let inputs = StepInputs {
            config: &self.config,
            map: &self.map,
            businesses: &self.businesses,
            year: self.clock.year(),
            day: self.clock.day(),
            years_per_step: self.clock.rules().years_per_day,
        };
        let outcome = self.events.year_step(inputs);
        if let Some(party) = outcome.election {
            self.apply_election(party);
        }
        for (_, effect) in &outcome.effects {
            self.apply_effect(effect);
        }
        self.apply_supply_caps();
    }

    /// §9.1: Labour nationalises (holders bought out by the city at book value); the
    /// Conservatives put every nationalised utility up for auction.
    fn apply_election(&mut self, party: Party) {
        match party {
            Party::Labour => {
                let ratio = self.config.utilities.nationalisation.buyout_ratio.rate();
                let buyouts = self.utilities.nationalise(ratio);
                self.ledger.settle(&buyouts);
            }
            Party::Conservative => {
                for kind in self.utilities.privatise() {
                    let reserve = self.utility_reserve(kind);
                    self.auctions.enqueue(QueuedSale {
                        item: AuctionItem::Utility(kind),
                        seller: AccountId::City,
                        reserve,
                    });
                }
            }
        }
    }

    /// A one-off effect (§9). Lasting effects are on the events module's stack already.
    fn apply_effect(&mut self, effect: &Effect) {
        match effect {
            Effect::DestroyBuildings {
                blocks,
                chance,
                compensation,
            } => {
                let at_risk: BTreeSet<BlockId> =
                    blocks.iter().filter_map(|b| self.map.block_id(b)).collect();
                // The map records the damage (land's hook), then buildings fall.
                let year = self.clock.year();
                for block in &at_risk {
                    let _known = self.map.damage_block(*block, year);
                }
                self.raid(&at_risk, chance.rate(), compensation.rate());
            }
            Effect::DesirabilityChange { blocks, tier } => {
                for key in blocks {
                    if let Some(id) = self.map.block_id(key) {
                        self.map.blocks[id.0 as usize].tier = *tier;
                    }
                }
            }
            Effect::LandValue { factor } => {
                for block in &mut self.map.blocks {
                    if let Some(lease) = block.lease.as_mut() {
                        lease.last_price = factor.rate().apply(lease.last_price);
                    }
                }
            }
            Effect::PropertyValue { types, factor } => {
                for b in self.businesses.by_id.values_mut() {
                    if types.contains(&b.type_id) {
                        b.build_cost = factor.rate().apply(b.build_cost);
                    }
                }
            }
            Effect::ClassShift { from, to, share } => {
                shift_class(&mut self.population.pool, *from, *to, share.rate());
            }
            _ => {}
        }
    }

    /// The Blitz (and any raid or flood): every building on `blocks` is destroyed with
    /// probability `chance`, one roll each in id order from the events stream; the city pays
    /// its owner `compensation × property value` (war-damage compensation).
    fn raid(&mut self, blocks: &BTreeSet<BlockId>, chance: Rate, compensation: Rate) {
        let candidates: Vec<BusinessId> = self
            .businesses
            .by_id
            .iter()
            .filter(|(_, b)| blocks.contains(&b.block))
            .map(|(id, _)| *id)
            .collect();
        for id in candidates {
            if !roll(self.events.rng_mut(), chance) {
                continue;
            }
            if let Some(gone) = self.destroy_building(id) {
                self.ledger.transfer(
                    AccountId::City,
                    gone.owner,
                    compensation.apply(gone.build_cost),
                );
            }
        }
    }

    /// **The block-damage hook**: removes a building from the map. Its stock goes with it,
    /// its residents return to the unhoused pool, and shoppers forget its reputation. A
    /// building up for auction is spared (the sale must have something to sell). Returns the
    /// building, so the caller can compensate its owner.
    ///
    /// This is the narrow interface events need; a richer damage model (partial damage,
    /// rebuilding) can replace the body without changing callers.
    pub fn destroy_building(&mut self, id: BusinessId) -> Option<Business> {
        if self.auctions.is_listed(AuctionItem::Business(id)) {
            return None;
        }
        let gone = self.businesses.by_id.remove(&id)?;
        self.customers.reputation.remove(&id);
        evict(&mut self.population, id);
        Some(gone)
    }

    /// Rationing and closures (§9), on the orders just locked, so the 06:00 bill and delivery
    /// match: every order of a capped good is capped at the cap's share of the shop's
    /// capacity, and a shop an event keeps shut all day (the three-day week) takes no delivery
    /// at all, rather than pay for stock it can only bin at midnight (R1).
    fn apply_supply_caps(&mut self) {
        let modifiers = &self.events.modifiers;
        if modifiers.is_empty() {
            return;
        }
        let day = self.clock.day();
        for (id, b) in &mut self.businesses.by_id {
            let capacity = b.capacity_per_good;
            let shut = !modifiers.may_trade(*id, day);
            for (good, units) in &mut b.tomorrow_order {
                if shut {
                    *units = 0;
                } else if let Some(share) = modifiers.supply_cap(good) {
                    *units = (*units).min(share_of(capacity, share));
                }
            }
        }
    }

    /// Phase 5, after settlement and delivery: every player's building rolls each random
    /// event (§9), one draw each in id order from the events stream.
    pub(super) fn random_incidents(&mut self) {
        let rules = &self.config.events.random;
        if rules.is_empty() {
            return;
        }
        let rules = rules.clone();
        let day = self.clock.day();
        let ids: Vec<BusinessId> = self
            .businesses
            .by_id
            .iter()
            .filter(|(_, b)| matches!(b.owner, AccountId::Player(_)))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            for event in &rules {
                if !roll(self.events.rng_mut(), event.daily_chance.rate()) {
                    continue;
                }
                let Some(owner) = self.businesses.by_id.get(&id).map(|b| b.owner) else {
                    break;
                };
                let loss = match &event.effect {
                    IncidentEffect::Destroy => self
                        .destroy_building(id)
                        .map_or(Money::ZERO, |b| b.build_cost),
                    IncidentEffect::StockLoss { share } => self.steal_stock(id, share.rate()),
                    IncidentEffect::Fine { amount } => {
                        self.ledger.transfer(owner, AccountId::City, amount.money());
                        amount.money()
                    }
                };
                let insured =
                    matches!(owner, AccountId::Player(p) if self.events.insured.contains(&p));
                let payout = if insured {
                    event.insured_payout.rate().apply(loss)
                } else {
                    Money::ZERO
                };
                if payout > Money::ZERO {
                    self.ledger.transfer(AccountId::City, owner, payout);
                }
                self.events.incidents.push(Incident {
                    day,
                    id: event.id.clone(),
                    building: id,
                    owner,
                    loss,
                    payout,
                });
            }
        }
    }

    /// A burglary: `share` of every good on the shelf is gone. Returns its wholesale value.
    fn steal_stock(&mut self, id: BusinessId, share: Rate) -> Money {
        let Some(b) = self.businesses.by_id.get_mut(&id) else {
            return Money::ZERO;
        };
        let mut lost_value = Money::ZERO;
        for (good, units) in &mut b.stock {
            let lost = share_of(*units, share);
            *units -= lost;
            if let Some(g) = self.config.businesses.goods.get(good) {
                lost_value += g.purchase_price.money().saturating_mul(lost);
            }
        }
        lost_value
    }

    /// Phase 5: each insured player's premium, `premium_rate × property value` per building,
    /// paid to the city (§9).
    pub(super) fn insurance_bills(&self) -> Vec<Bill> {
        let rate = self.config.events.insurance.premium_rate.rate();
        self.events
            .insured
            .iter()
            .filter(|p| self.is_active(**p))
            .map(|p| {
                let me = AccountId::Player(*p);
                let value: Money = self
                    .businesses
                    .owned_by(me)
                    .map(|(_, b)| b.build_cost)
                    .sum();
                Bill {
                    from: me,
                    to: AccountId::City,
                    amount: rate.apply(value),
                }
            })
            .filter(|b| b.amount > Money::ZERO)
            .collect()
    }

    /// Phase 2: take out or cancel insurance (§9).
    pub(super) fn set_insurance(&mut self, player: PlayerId, insured: bool) {
        if insured {
            self.events.insured.insert(player);
        } else {
            self.events.insured.remove(&player);
        }
    }

    /// The minimum opening bid for a utility (R12, like R23 for land).
    pub(super) fn utility_reserve(&self, kind: UtilityKind) -> Money {
        let r = &self.config.utilities.reserve_prices;
        match kind {
            UtilityKind::Electricity => r.electricity.money(),
            UtilityKind::Gas => r.gas.money(),
            UtilityKind::Water => r.water.money(),
            UtilityKind::Telecoms => r.telecoms.money(),
        }
    }

    /// A player's opening bid on an unheld, privately ownable utility (R12): the same house,
    /// queue and rules as land; the minimum is the utility's reserve price.
    pub(super) fn open_utility_auction(
        &mut self,
        player: PlayerId,
        kind: UtilityKind,
        bid: Money,
    ) -> Result<(), AuctionError> {
        if !self.utilities.is_open_for_bids(kind) {
            return Err(AuctionError::NotForSale);
        }
        let opening = Opening {
            item: AuctionItem::Utility(kind),
            seller: AccountId::City,
            opening_bid: Some((player, bid)),
            minimum: self.utility_reserve(kind),
        };
        let ceiling = self.bid_ceiling(player).unwrap_or(Money::ZERO);
        self.auctions.open(opening, ceiling, self.clock.tick())
    }

    /// The reserve when a holder offers their utility for auction, or why they can't.
    pub(super) fn utility_offer_reserve(
        &self,
        player: PlayerId,
        kind: UtilityKind,
    ) -> Result<Money, AuctionError> {
        if self.utilities.payee(kind) == AccountId::Player(player) {
            Ok(self.utility_reserve(kind))
        } else {
            Err(AuctionError::NotYours)
        }
    }

    /// True if a closed utility sale can't go through: Labour nationalised it while it was
    /// under the hammer, so nothing changes hands.
    pub(super) fn utility_sale_void(&self, item: AuctionItem) -> bool {
        matches!(item, AuctionItem::Utility(kind) if self.utilities.nationalised.contains(&kind))
    }

    /// Utilities, mutable, for scenario setup and tools. Play changes holders through auctions
    /// and elections.
    #[doc(hidden)]
    pub fn utilities_mut(&mut self) -> &mut Utilities {
        &mut self.utilities
    }
}

/// Moves `floor(n × share)` of every `from`-class cohort in the unhoused pool to the same
/// band and sex in class `to` (§9: the Big Bang's Upper-class share). Residents keep their
/// class and their flat. Head-count is conserved.
pub fn shift_class(pool: &mut BTreeMap<Cohort, u64>, from: Class, to: Class, share: Rate) {
    if from == to {
        return;
    }
    let moves: Vec<(Cohort, u64)> = pool
        .iter()
        .filter(|(c, _)| c.class == from)
        .map(|(c, n)| {
            let moved =
                u64::try_from(share_of(i64::try_from(*n).unwrap_or(i64::MAX), share)).unwrap_or(0);
            (*c, moved.min(*n))
        })
        .collect();
    for (cohort, moved) in moves {
        if moved == 0 {
            continue;
        }
        if let Some(n) = pool.get_mut(&cohort) {
            *n -= moved;
        }
        *pool
            .entry(Cohort {
                class: to,
                ..cohort
            })
            .or_insert(0) += moved;
    }
}

/// A destroyed apartment block's residents return to the unhoused pool and its rent is
/// forgotten (§7.2).
fn evict(population: &mut Population, building: BusinessId) {
    let residential = &mut population.residential;
    residential.rents.remove(&building);
    let out: Vec<ResidentId> = residential
        .residents
        .iter()
        .filter(|(_, r)| r.home == building)
        .map(|(id, _)| *id)
        .collect();
    for id in out {
        if let Some(r) = residential.residents.remove(&id) {
            *population.pool.entry(r.cohort).or_insert(0) += 1;
        }
    }
}

/// `floor(units × share)`, never negative.
fn share_of(units: i64, share: Rate) -> i64 {
    let scaled = i128::from(units.max(0)) * i128::from(share.bp().max(0))
        / i128::from(BASIS_POINTS_PER_UNIT);
    i64::try_from(scaled).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_of_rounds_down_and_never_overflows() {
        assert_eq!(share_of(101, Rate::from_bp(5_000)), 50);
        assert_eq!(share_of(-5, Rate::from_bp(5_000)), 0);
        assert_eq!(share_of(i64::MAX, Rate::ONE), i64::MAX);
    }

    #[test]
    fn a_class_shift_moves_a_share_and_keeps_head_count() {
        use crate::population::{AgeBand, Sex};
        let c = |class| Cohort {
            band: AgeBand(1),
            sex: Sex::Female,
            class,
        };
        let mut pool = BTreeMap::from([(c(Class::Middle), 100), (c(Class::Upper), 10)]);
        shift_class(&mut pool, Class::Middle, Class::Upper, Rate::from_bp(1_000));
        assert_eq!(pool[&c(Class::Middle)], 90);
        assert_eq!(pool[&c(Class::Upper)], 20);
        assert_eq!(pool.values().sum::<u64>(), 110);
    }
}
