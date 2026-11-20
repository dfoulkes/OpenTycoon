//! One tick of the customers phase (§7.3, §7.5, §8): the window edges (closes, then opens),
//! then one step for every agent on the map. The day's shape is in the module docs of
//! [`super`].

use std::collections::{BTreeMap, BTreeSet};

use super::movement::{Cell, Geometry, step_towards};
use super::online::{self, OnlineOffer};
use super::shops::{GoodId, GoodsIndex, ShopOffer};
use super::{
    Activity, CheckIn, Customer, Customers, LostSale, Origin, PhaseInputs, PhaseReport, Sale, Wave,
    adjust_offer, next_errand, reputation_of, scoring, spawn_wave, till_price,
};
use crate::businesses::{BusinessId, Businesses, LostSale as ShopLostSale};
use crate::clock::TradingWindow;
use crate::hotels::{self, HotelOffer, NoRoom};
use crate::money::Rate;
use crate::rng::SimRng;

/// One tick of the customers phase (§8): window opens spawn a wave, closes end it, and every
/// agent on the map takes one step (choose, walk, buy, look for a bed, or leave).
pub fn tick(
    state: &mut Customers,
    inputs: PhaseInputs<'_>,
    offers: &mut [ShopOffer],
    doors: &BTreeMap<BusinessId, Cell>,
    goods: &GoodsIndex,
    businesses: &mut Businesses,
    rng: &mut SimRng,
) -> PhaseReport {
    let mut report = PhaseReport::default();
    window_edges(state, inputs, doors, goods, businesses, rng, &mut report);

    let clock = inputs.clock;
    let now = clock.second_of_day();
    let beds_open = clock.is_open(TradingWindow::Hotel);
    let beds = if beds_open {
        hotels::hotel_offers(
            businesses,
            inputs.map,
            inputs.config,
            clock,
            inputs.geometry(),
            &state.hotel_guests,
        )
    } else {
        Vec::new()
    };
    let mut step = Step {
        inputs,
        now,
        beds_open,
        offers,
        beds,
        web: None,
        brand: &state.brand,
        goods,
        businesses,
        rng,
        reputation: std::mem::take(&mut state.reputation),
        guests: std::mem::take(&mut state.hotel_guests),
        wanderers: Vec::new(),
        report,
    };
    let before = state.agents.len();
    state.agents.retain_mut(|c| step.agent(c));
    let Step {
        mut report,
        reputation,
        guests,
        wanderers,
        ..
    } = step;
    report.despawned += (before - state.agents.len() - wanderers.len()) as u64;
    state.wanderers.extend(wanderers);
    state.reputation = reputation;
    state.hotel_guests = guests;
    report
}

/// Closes first, then opens (§8.4, §7.3): day and night trade, and the hotel window.
fn window_edges(
    state: &mut Customers,
    inputs: PhaseInputs<'_>,
    doors: &BTreeMap<BusinessId, Cell>,
    goods: &GoodsIndex,
    businesses: &Businesses,
    rng: &mut SimRng,
    report: &mut PhaseReport,
) {
    let config = inputs.config;
    let clock = inputs.clock;
    let trading = clock.rules().trading;
    let now = clock.second_of_day();
    let beds_open = clock.is_open(TradingWindow::Hotel);
    if now == trading.day.1 {
        let before = state.agents.len();
        state.agents.retain(|c| c.wave != Wave::Day);
        report.despawned += (before - state.agents.len()) as u64;
    }
    if now == trading.night.1 && !beds_open {
        // No hotel window to wait for (an unusual config): the wanderers go home too.
        state.agents.append(&mut state.wanderers);
        state.agents.sort_by_key(|c| c.id);
    }
    if now == trading.night.1 {
        let before = state.agents.len();
        state
            .agents
            .retain(|c| !(c.wave == Wave::Night && matches!(c.activity, Activity::Waiting { .. })));
        report.despawned += (before - state.agents.len()) as u64;
        for c in state.agents.iter_mut().filter(|c| c.wave == Wave::Night) {
            c.activity = after_night_trade(c, beds_open);
        }
    }
    if now == trading.hotel.1 {
        for c in &mut state.agents {
            if matches!(
                c.activity,
                Activity::Wandering | Activity::SeekingBed | Activity::WalkingToHotel { .. }
            ) {
                c.activity = Activity::Leaving;
            }
        }
    }
    if now == trading.hotel.0 {
        // A new night: every room is free again, and the wanderers go looking (R38, R42).
        state.hotel_guests.clear();
        for mut c in state.wanderers.drain(..) {
            c.activity = Activity::SeekingBed;
            state.agents.push(c);
        }
        state.agents.sort_by_key(|c| c.id);
    }
    if now == trading.day.0 {
        online::grow_brands(&mut state.brand, businesses, config, clock.year());
        report.spawned += spawn_wave(state, inputs, Wave::Day, doors, goods, false, rng);
    }
    if now == trading.night.0 {
        let beds = hotels::any_hotel(config, businesses);
        report.spawned += spawn_wave(state, inputs, Wave::Night, doors, goods, beds, rng);
    }
}

/// What a shopper does once nothing left on their list can be bought (§7.3, §8.4): a night
/// tourist wants a bed (R38), wandering until the hotels open (R42); everyone else leaves
/// (commuters by train, R59; residents home).
fn after_shopping(c: &Customer, beds_open: bool) -> Activity {
    match (c.origin, c.wave) {
        (Origin::Tourist, Wave::Night) if beds_open => Activity::SeekingBed,
        (Origin::Tourist, Wave::Night) => Activity::Wandering,
        _ => Activity::Leaving,
    }
}

/// What a night shopper does when night trade closes: a tourist still out looks for a bed if
/// the hotels are open (R38), or keeps walking to the one they chose; everyone else leaves.
fn after_night_trade(c: &Customer, beds_open: bool) -> Activity {
    match c.activity {
        Activity::WalkingToHotel { .. } if beds_open => c.activity,
        _ if c.origin == Origin::Tourist && beds_open => Activity::SeekingBed,
        _ => Activity::Leaving,
    }
}

/// Everything one agent's step reads and changes this tick.
struct Step<'a, 'i> {
    inputs: PhaseInputs<'i>,
    now: u32,
    beds_open: bool,
    offers: &'a mut [ShopOffer],
    beds: Vec<HotelOffer>,
    /// Online offers, built when the first shopper sets out this tick.
    web: Option<(Vec<OnlineOffer>, BTreeSet<GoodId>)>,
    brand: &'a BTreeMap<BusinessId, Rate>,
    goods: &'a GoodsIndex,
    businesses: &'a mut Businesses,
    rng: &'a mut SimRng,
    reputation: BTreeMap<BusinessId, Rate>,
    guests: BTreeMap<BusinessId, u32>,
    /// Tourists who started wandering this tick (R42).
    wanderers: Vec<Customer>,
    report: PhaseReport,
}

impl Step<'_, '_> {
    fn geometry(&self) -> Geometry {
        self.inputs.geometry()
    }

    fn cells(&self) -> u32 {
        self.inputs.config.customers.movement.cells_per_tick
    }

    /// One agent's step. Returns false when the agent leaves the map (home, the tube, or bed).
    fn agent(&mut self, c: &mut Customer) -> bool {
        if let Activity::Waiting { from } = c.activity {
            if self.now < from {
                return true;
            }
            c.activity = Activity::Choosing;
            self.shop_online(c);
        }
        match c.activity {
            Activity::Waiting { .. } | Activity::Wandering => true,
            Activity::Choosing => {
                let rules = &self.inputs.config.customers;
                let geometry = self.geometry();
                c.activity =
                    next_errand(c, self.offers, &self.reputation, rules, geometry, self.rng);
                if c.activity == Activity::Leaving {
                    c.activity = after_shopping(c, self.beds_open);
                }
                if c.activity == Activity::Wandering {
                    // Off to wait for the hotels: out of the busy list, still on the map.
                    self.wanderers.push(c.clone());
                    return false;
                }
                true
            }
            Activity::Walking { to, shop, good } => {
                c.at = step_towards(c.at, to, self.cells());
                if c.at == to {
                    self.buy(c, shop, good);
                    c.activity = Activity::Choosing;
                }
                true
            }
            Activity::Leaving => {
                c.at = step_towards(c.at, c.exit, self.cells());
                c.at != c.exit
            }
            Activity::SeekingBed => {
                c.activity = hotels::choose_hotel(
                    c.cohort,
                    c.at,
                    c.budget,
                    &self.beds,
                    &self.reputation,
                    &self.inputs.config.customers,
                    self.geometry(),
                    self.rng,
                )
                .map_or(Activity::Leaving, |i| Activity::WalkingToHotel {
                    to: self.beds[i].door,
                    hotel: self.beds[i].hotel,
                });
                true
            }
            Activity::WalkingToHotel { to, hotel } => {
                c.at = step_towards(c.at, to, self.cells());
                c.at != to || !self.check_in(c, hotel)
            }
        }
    }

    /// A shopper setting out buys online first (§7.5), once the Internet is live and an
    /// online seller trades.
    fn shop_online(&mut self, c: &mut Customer) {
        let config = self.inputs.config;
        let year = self.inputs.clock.year();
        if !online::internet_is_live(config, year) {
            return;
        }
        let (web, only_online) = self.web.get_or_insert_with(|| {
            (
                online::online_offers(self.businesses, config, year, self.goods, self.brand),
                online::online_only_goods(config, self.goods),
            )
        });
        if web.is_empty() {
            return;
        }
        let sales = online::shop_online(
            c,
            web,
            only_online,
            self.inputs,
            self.goods,
            self.businesses,
            self.rng,
        );
        self.report.online_sales.extend(sales);
    }

    /// At a shop's door: buy one, or a lost sale (§8.3).
    fn buy(&mut self, c: &mut Customer, shop: BusinessId, good: GoodId) {
        let rules = &self.inputs.config.customers;
        let outcome = self.businesses.try_sell(
            shop,
            self.goods.name(good),
            1,
            self.inputs.clock,
            &self.inputs.config.businesses,
        );
        let rep = reputation_of(&self.reputation, shop, rules);
        match outcome {
            Ok(amount) => {
                let amount = till_price(self.inputs, shop, self.goods.name(good), amount);
                c.budget -= amount;
                c.list.retain(|g| *g != good);
                self.report.sales.push(Sale {
                    shop,
                    good,
                    qty: 1,
                    amount,
                    customer: c.id,
                });
                let after = scoring::reputation_after_sale(rep, rules.reputation.sale_recovery);
                self.reputation.insert(shop, after);
                adjust_offer(self.offers, shop, good, |s| s - 1);
            }
            Err(ShopLostSale::OutOfStock { .. }) => {
                self.report.lost.push(LostSale { shop, good });
                let after =
                    scoring::reputation_after_lost_sale(rep, rules.reputation.lost_sale_penalty);
                self.reputation.insert(shop, after);
                adjust_offer(self.offers, shop, good, |_| 0);
            }
            // Shut, no such shop, not sold there: nothing lost, just re-score.
            Err(_) => adjust_offer(self.offers, shop, good, |_| 0),
        }
    }

    /// At a Hotel's door (R29): check in and leave the map for the night (true), or, with no
    /// room to be had there, look again (false). A full Hotel is a lost sale (§8.3).
    fn check_in(&mut self, c: &mut Customer, hotel: BusinessId) -> bool {
        let rules = &self.inputs.config.customers;
        let rep = reputation_of(&self.reputation, hotel, rules);
        let outcome = hotels::check_in(
            self.businesses,
            &mut self.guests,
            self.inputs.config,
            self.inputs.clock,
            hotel,
            c.budget,
        );
        let free = |beds: &mut Vec<HotelOffer>, f: &dyn Fn(u32) -> u32| {
            for b in beds.iter_mut().filter(|b| b.hotel == hotel) {
                b.free = f(b.free);
            }
        };
        match outcome {
            Ok(amount) => {
                c.budget -= amount;
                self.report.check_ins.push(CheckIn {
                    hotel,
                    amount,
                    customer: c.id,
                    origin: c.origin,
                });
                let after = scoring::reputation_after_sale(rep, rules.reputation.sale_recovery);
                self.reputation.insert(hotel, after);
                free(&mut self.beds, &|n| n.saturating_sub(1));
                true
            }
            Err(why) => {
                if why == NoRoom::Full {
                    let after = scoring::reputation_after_lost_sale(
                        rep,
                        rules.reputation.lost_sale_penalty,
                    );
                    self.reputation.insert(hotel, after);
                }
                free(&mut self.beds, &|_| 0);
                c.activity = Activity::SeekingBed;
                false
            }
        }
    }
}
