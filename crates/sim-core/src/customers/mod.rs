//! Customers: walking agents, shopping lists, shop choice, movement, reputation
//! (§8.1–§8.4; R25, R55, R59).
//!
//! Owner boundary: individual shoppers on the map and the scoring that picks a shop. Counts,
//! arrivals and homes come from [`crate::population`]; stock and prices from
//! [`crate::businesses`], bought only through `Businesses::try_sell`. Money moves only
//! through [`crate::economy`]: this phase returns [`Sale`]s and the world pays them.
//!
//! A day, by default:
//!
//! - **07:00** day trade opens: every adult rolls a day shopping list (R55). Residents leave
//!   their flats; visitors (70% commuters, 30% tourists, R59) arrive at tube stations (R25).
//!   They trickle in over `spawn_spread_minutes`.
//! - Each shopper takes the first good on their list, scores every open shop with stock that
//!   they can afford (§8.3), picks one by weighted random, walks there and buys. A shop that
//!   has sold out by the time they arrive is a **lost sale**: its reputation drops and the
//!   shopper re-scores.
//! - **17:00** day trade closes: day shoppers still on the map despawn (§8.4).
//! - **19:00** night trade: the same again for the night-out goods.
//! - **23:50** night trade closes: residents walk home and visitors walk to the tube, then
//!   despawn. Hotels are post-v1, so in v1 tourists take the train too (R59, partial).
//!
//! Floats live only in [`scoring`] and [`shopping`]; state is integers.

pub mod movement;
pub mod online;
pub mod scoring;
pub mod shopping;
pub mod shops;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::businesses::{BusinessId, Businesses, Hours, LostSale as ShopLostSale};
use crate::clock::Clock;
use crate::config::{Config, CustomersConfig};
use crate::events::Modifiers;
use crate::land::Map;
use crate::money::{Money, Rate};
use crate::population::cohort::split_by_weights;
use crate::population::{Cohort, Population, ResidentId, allowance};
use crate::rng::{SimRng, below};

use movement::{Cell, Geometry, step_towards};
use shops::{GoodId, GoodsIndex, ShopOffer};

/// A stable customer id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub struct CustomerId(pub u64);

/// Where a customer came from (§8.1, R25, R59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Origin {
    /// Lives in an apartment and leaves from home.
    Resident(ResidentId),
    /// Commuter: arrives at a tube station and always takes the train home (R59).
    Commuter,
    /// Tourist: arrives at a tube station; hotel first once hotels exist (R59, post-v1).
    Tourist,
}

/// Which trading window a shopper came out for (§8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Wave {
    /// Day trade: despawns at the close.
    Day,
    /// Night trade: goes home or to the tube at the close.
    Night,
}

/// What a customer is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Activity {
    /// Not on the map yet; steps out at this second of the day.
    Waiting {
        /// Second of day they appear.
        from: u32,
    },
    /// Picking a shop for the first good on their list.
    Choosing,
    /// Walking to a shop to buy a good.
    Walking {
        /// The shop's door.
        to: Cell,
        /// The shop.
        shop: BusinessId,
        /// The good.
        good: GoodId,
    },
    /// Heading for their exit (home or a tube station), then off the map.
    Leaving,
    /// A tourist with nothing left to buy, wandering the streets until the hotels open
    /// (R42). Still footfall; buys nothing.
    Wandering,
    /// A tourist looking for a bed: their last need of the day (R38).
    SeekingBed,
    /// Walking to a Hotel for a room.
    WalkingToHotel {
        /// The Hotel's door.
        to: Cell,
        /// The Hotel.
        hotel: BusinessId,
    },
}

/// One shopper on the map. Money is pence; position is a walking-grid cell.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Customer {
    /// Id.
    pub id: CustomerId,
    /// Band, sex and class.
    pub cohort: Cohort,
    /// Resident, commuter or tourist.
    pub origin: Origin,
    /// The window they came out for.
    pub wave: Wave,
    /// Where they are.
    pub at: Cell,
    /// Where they leave from: their front door, or the station they arrived at.
    pub exit: Cell,
    /// What they have left to spend in this window.
    pub budget: Money,
    /// Goods still to buy, in order.
    pub list: Vec<GoodId>,
    /// What they are doing.
    pub activity: Activity,
}

/// All customers on the map, and shop reputations (§8.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Customers {
    /// Agents, in id order.
    pub agents: Vec<Customer>,
    /// Next id to hand out.
    pub next_id: u64,
    /// Reputation per shop, 0–1 as a rate (absent: `reputation.initial`).
    pub reputation: BTreeMap<BusinessId, Rate>,
    /// Rooms taken tonight per Hotel (R30); cleared when the hotels open.
    #[serde(default)]
    pub hotel_guests: BTreeMap<BusinessId, u32>,
    /// Brand per online seller, 0–1 (absent: `online.brand.initial`, §7.5).
    #[serde(default)]
    pub brand: BTreeMap<BusinessId, Rate>,
}

impl Customers {
    /// A shop's reputation (§8.3).
    pub fn reputation(&self, shop: BusinessId, rules: &CustomersConfig) -> Rate {
        reputation_of(&self.reputation, shop, rules)
    }

    /// Agents on the map (not waiting to appear).
    pub fn on_map(&self) -> impl Iterator<Item = &Customer> {
        self.agents
            .iter()
            .filter(|c| !matches!(c.activity, Activity::Waiting { .. }))
    }
}

/// A completed sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sale {
    /// Shop.
    pub shop: BusinessId,
    /// Good.
    pub good: GoodId,
    /// Units.
    pub qty: u32,
    /// What the customer paid.
    pub amount: Money,
    /// Who bought.
    pub customer: CustomerId,
}

/// A customer reached a shop that had sold out (§8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LostSale {
    /// Shop.
    pub shop: BusinessId,
    /// Good.
    pub good: GoodId,
}

/// A tourist took a room (R29). Room revenue is booked at check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CheckIn {
    /// The Hotel.
    pub hotel: BusinessId,
    /// What the guest paid for the night.
    pub amount: Money,
    /// The guest.
    pub customer: CustomerId,
    /// Where they came from (only ever a tourist, R59).
    pub origin: Origin,
}

/// What one tick of the customers phase did.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PhaseReport {
    /// Agents created (they appear over the spawn spread).
    pub spawned: u64,
    /// Agents removed from the map.
    pub despawned: u64,
    /// Sales, in the order they happened.
    pub sales: Vec<Sale>,
    /// Lost sales, in the order they happened.
    pub lost: Vec<LostSale>,
    /// Online sales (no travel, §7.5), in the order they happened.
    pub online_sales: Vec<Sale>,
    /// Hotel check-ins, in the order they happened (R29).
    pub check_ins: Vec<CheckIn>,
}

/// Read-only inputs for the customers phase.
#[derive(Debug, Clone, Copy)]
pub struct PhaseInputs<'a> {
    /// Validated config.
    pub config: &'a Config,
    /// The map.
    pub map: &'a Map,
    /// The clock (already advanced this tick).
    pub clock: &'a Clock,
    /// Who lives here.
    pub population: &'a Population,
    /// Event modifiers in force (§9): demand, open days, price noise, footfall.
    pub modifiers: &'a Modifiers,
}

impl PhaseInputs<'_> {
    /// The walking grid's shape.
    pub fn geometry(&self) -> Geometry {
        let [lot_w, lot_d] = self.config.game.world.lot_size;
        Geometry { lot_w, lot_d }
    }
}

/// Phase 8 over real businesses: builds the shop offers and front doors, then runs [`tick`]
/// against the real shops.
pub fn run_phase(
    state: &mut Customers,
    inputs: PhaseInputs<'_>,
    businesses: &mut Businesses,
    rng: &mut SimRng,
) -> PhaseReport {
    let trading = inputs.clock.rules().trading;
    let now = inputs.clock.second_of_day();
    if state.agents.is_empty() && now != trading.day.0 && now != trading.night.0 {
        // Nobody about and no window opening: nothing to score (keeps idle ticks cheap).
        return PhaseReport::default();
    }
    let goods = GoodsIndex::new(&inputs.config.businesses.goods);
    let geometry = inputs.geometry();
    let mut offers = shops::shop_offers(
        businesses,
        inputs.map,
        inputs.config,
        inputs.clock,
        &goods,
        geometry,
    );
    shops::apply_modifiers(
        &mut offers,
        inputs.map,
        businesses,
        inputs.modifiers,
        inputs.clock.day(),
        &goods,
    );
    let doors = front_doors(businesses, inputs.map, geometry);
    tick(state, inputs, &mut offers, &doors, &goods, businesses, rng)
}

/// Every building's front door.
pub fn front_doors(
    businesses: &Businesses,
    map: &Map,
    geometry: Geometry,
) -> BTreeMap<BusinessId, Cell> {
    businesses
        .by_id
        .iter()
        .filter_map(|(id, b)| Some((*id, geometry.door(map.block(b.block)?, b.footprint))))
        .collect()
}

/// Tube stations open in `year`, as cells, in config order (§4.3).
pub fn station_cells(config: &Config, map: &Map, year: i32, geometry: Geometry) -> Vec<Cell> {
    config
        .map
        .tube_stations
        .iter()
        .filter(|s| s.opens <= year)
        .filter_map(|s| map.blocks.iter().find(|b| b.key == s.block))
        .map(|b| geometry.centre(b))
        .collect()
}

/// One tick of the customers phase (§8): window opens spawn a wave, closes end it, and every
/// agent on the map takes one step (choose, walk, buy, or leave).
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
    let trading = inputs.clock.rules().trading;
    let now = inputs.clock.second_of_day();

    // Closes first, then opens (§8.4).
    if now == trading.day.1 {
        let before = state.agents.len();
        state.agents.retain(|c| c.wave != Wave::Day);
        report.despawned += (before - state.agents.len()) as u64;
    }
    if now == trading.night.1 {
        let before = state.agents.len();
        state
            .agents
            .retain(|c| !(c.wave == Wave::Night && matches!(c.activity, Activity::Waiting { .. })));
        report.despawned += (before - state.agents.len()) as u64;
        for c in &mut state.agents {
            if c.wave == Wave::Night {
                c.activity = Activity::Leaving;
            }
        }
    }
    if now == trading.day.0 {
        report.spawned += spawn_wave(state, inputs, Wave::Day, doors, goods, rng);
    }
    if now == trading.night.0 {
        report.spawned += spawn_wave(state, inputs, Wave::Night, doors, goods, rng);
    }

    let rules = &inputs.config.customers;
    let geometry = inputs.geometry();
    let cells = rules.movement.cells_per_tick;
    let mut reputation = std::mem::take(&mut state.reputation);
    let before = state.agents.len();
    state.agents.retain_mut(|c| {
        if let Activity::Waiting { from } = c.activity {
            if now < from {
                return true;
            }
            c.activity = Activity::Choosing;
        }
        match c.activity {
            Activity::Waiting { .. } => true,
            Activity::Choosing => {
                c.activity = next_errand(c, offers, &reputation, rules, geometry, rng);
                true
            }
            Activity::Walking { to, shop, good } => {
                c.at = step_towards(c.at, to, cells);
                if c.at == to {
                    let outcome = businesses.try_sell(
                        shop,
                        goods.name(good),
                        1,
                        inputs.clock,
                        &inputs.config.businesses,
                    );
                    let rep = reputation_of(&reputation, shop, rules);
                    match outcome {
                        Ok(amount) => {
                            let amount = till_price(inputs, shop, goods.name(good), amount);
                            c.budget -= amount;
                            c.list.retain(|g| *g != good);
                            report.sales.push(Sale {
                                shop,
                                good,
                                qty: 1,
                                amount,
                                customer: c.id,
                            });
                            reputation.insert(
                                shop,
                                scoring::reputation_after_sale(rep, rules.reputation.sale_recovery),
                            );
                            adjust_offer(offers, shop, good, |s| s - 1);
                        }
                        Err(ShopLostSale::OutOfStock { .. }) => {
                            report.lost.push(LostSale { shop, good });
                            reputation.insert(
                                shop,
                                scoring::reputation_after_lost_sale(
                                    rep,
                                    rules.reputation.lost_sale_penalty,
                                ),
                            );
                            adjust_offer(offers, shop, good, |_| 0);
                        }
                        // Shut, no such shop, not sold there: nothing lost, just re-score.
                        Err(_) => adjust_offer(offers, shop, good, |_| 0),
                    }
                    c.activity = Activity::Choosing;
                }
                true
            }
            Activity::Leaving => {
                c.at = step_towards(c.at, c.exit, cells);
                c.at != c.exit
            }
            Activity::Wandering | Activity::SeekingBed | Activity::WalkingToHotel { .. } => {
                c.activity = Activity::Leaving;
                true
            }
        }
    });
    report.despawned += (before - state.agents.len()) as u64;
    state.reputation = reputation;
    report
}

/// What the till charges: the sale's price with any §9 price noise on this shop and good.
fn till_price(inputs: PhaseInputs<'_>, shop: BusinessId, good: &str, amount: Money) -> Money {
    inputs.modifiers.price_factor(shop, good).apply(amount)
}

fn reputation_of(
    map: &BTreeMap<BusinessId, Rate>,
    shop: BusinessId,
    rules: &CustomersConfig,
) -> Rate {
    map.get(&shop)
        .copied()
        .unwrap_or_else(|| scoring::unit_to_rate(rules.reputation.initial))
}

fn adjust_offer(offers: &mut [ShopOffer], shop: BusinessId, good: GoodId, f: impl Fn(i64) -> i64) {
    for o in offers
        .iter_mut()
        .filter(|o| o.shop == shop && o.good == good)
    {
        o.stock = f(o.stock).max(0);
    }
}

/// The shopper's next move: walk to a shop for the first good on their list that some shop can
/// sell them, dropping goods nobody can; or leave once the list is empty.
fn next_errand(
    c: &mut Customer,
    offers: &[ShopOffer],
    reputation: &BTreeMap<BusinessId, Rate>,
    rules: &CustomersConfig,
    geometry: Geometry,
    rng: &mut SimRng,
) -> Activity {
    while let Some(&good) = c.list.first() {
        if let Some(i) = choose_shop(
            c.cohort, c.at, c.budget, good, offers, reputation, rules, geometry, rng,
        ) {
            return Activity::Walking {
                to: offers[i].door,
                shop: offers[i].shop,
                good,
            };
        }
        c.list.remove(0);
    }
    Activity::Leaving
}

/// What a shopper brings to a window: the day share of their after-rent allowance by day,
/// the rest at night (§8.2: rent comes out first).
pub fn wave_budget(rules: &CustomersConfig, after_rent: Money, wave: Wave) -> Money {
    if after_rent <= Money::ZERO {
        return Money::ZERO;
    }
    let day = rules.allowance.day_share.rate().apply(after_rent);
    match wave {
        Wave::Day => day,
        Wave::Night => after_rent - day,
    }
}

/// Creates a window's shoppers (R25, R55, R59): every resident from their front door and
/// every unhoused adult from a tube station, each with a freshly rolled list and the wave's
/// budget. Those with nothing to buy or nothing to spend stay at home. Visitors split
/// `commuter_share` commuters, the rest tourists, exactly per cohort.
pub fn spawn_wave(
    state: &mut Customers,
    inputs: PhaseInputs<'_>,
    wave: Wave,
    doors: &BTreeMap<BusinessId, Cell>,
    goods: &GoodsIndex,
    rng: &mut SimRng,
) -> u64 {
    let rules = &inputs.config.customers;
    let year = inputs.clock.year();
    let clock_rules = inputs.clock.rules();
    let (open, window) = match wave {
        Wave::Day => (clock_rules.trading.day.0, Hours::Day),
        Wave::Night => (clock_rules.trading.night.0, Hours::Night),
    };
    let tick_secs = clock_rules.tick_game_seconds.max(1);
    let spread_ticks = u64::from(rules.movement.spawn_spread_minutes * 60 / tick_secs);
    let geometry = inputs.geometry();
    let stations = station_cells(inputs.config, inputs.map, year, geometry);
    let default_rent = rules.residential.default_rent.money();
    let pop = inputs.population;
    let mut spawned = 0;
    // At most everyone steps out: reserve once rather than regrow the vector mid-wave.
    let visitors: u64 = if stations.is_empty() {
        0
    } else {
        pop.pool.values().sum()
    };
    let most = u64::try_from(pop.residential.residents.len()).map_or(0, |r| r + visitors);
    state.agents.reserve(usize::try_from(most).unwrap_or(0));
    // Every member of a cohort rolls from the same candidates this wave; resolve them once.
    let mut candidates: BTreeMap<Cohort, shopping::Candidates> = BTreeMap::new();

    let mut spawn = |state: &mut Customers,
                     cohort: Cohort,
                     origin: Origin,
                     at: Cell,
                     budget: Money,
                     rng: &mut SimRng| {
        if budget <= Money::ZERO {
            return;
        }
        let cohort_goods = candidates.entry(cohort).or_insert_with(|| {
            shopping::candidates(rules, goods, cohort, window, year).scaled_by(
                inputs.modifiers,
                goods,
                cohort.class,
            )
        });
        let list = shopping::roll_from(cohort_goods, rng);
        if list.is_empty() {
            return;
        }
        let delay = u32::try_from(below(rng, spread_ticks + 1)).unwrap_or(0) * tick_secs;
        state.agents.push(Customer {
            id: CustomerId(state.next_id),
            cohort,
            origin,
            wave,
            at,
            exit: at,
            budget,
            list,
            activity: Activity::Waiting { from: open + delay },
        });
        state.next_id += 1;
        spawned += 1;
    };

    for (id, r) in &pop.residential.residents {
        let Some(&door) = doors.get(&r.home) else {
            continue;
        };
        let rent = pop.residential.rent(r.home, default_rent);
        let budget = wave_budget(rules, allowance(rules, r.cohort) - rent, wave);
        spawn(state, r.cohort, Origin::Resident(*id), door, budget, rng);
    }
    if !stations.is_empty() {
        let share = rules.visitors.commuter_share.rate().bp().unsigned_abs();
        let unit = crate::money::BASIS_POINTS_PER_UNIT.unsigned_abs();
        for (cohort, n) in &pop.pool {
            let commuters = split_by_weights(*n, &[share, unit.saturating_sub(share)])[0];
            let budget = wave_budget(rules, allowance(rules, *cohort), wave);
            for i in 0..*n {
                let origin = if i < commuters {
                    Origin::Commuter
                } else {
                    Origin::Tourist
                };
                let k = usize::try_from(below(rng, stations.len() as u64)).unwrap_or(0);
                spawn(state, *cohort, origin, stations[k], budget, rng);
            }
        }
    }
    spawned
}

/// Picks a shop for `good` (§8.3): every offer of the good with stock that the shopper can
/// afford is scored `affinity × price_factor × distance_decay × reputation`, then one is
/// picked by weighted random (not argmax). Returns the index into `offers`.
#[allow(clippy::too_many_arguments, reason = "the §8.3 inputs")]
pub fn choose_shop(
    cohort: Cohort,
    at: Cell,
    budget: Money,
    good: GoodId,
    offers: &[ShopOffer],
    reputation: &BTreeMap<BusinessId, Rate>,
    rules: &CustomersConfig,
    geometry: Geometry,
    rng: &mut SimRng,
) -> Option<usize> {
    let mut index = Vec::new();
    let mut scores = Vec::new();
    for (i, o) in offers.iter().enumerate() {
        if o.good != good || o.stock <= 0 || o.price > budget {
            continue;
        }
        index.push(i);
        scores.push(scoring::shop_score(
            rules,
            cohort.class,
            o.affinity[cohort.class.index()],
            o.price,
            o.ref_price,
            o.luxury,
            geometry.blocks_between(at, o.door),
            reputation_of(reputation, o.shop, rules),
        ));
    }
    scoring::weighted_pick(&scores, rng).and_then(|k| index.get(k).copied())
}
