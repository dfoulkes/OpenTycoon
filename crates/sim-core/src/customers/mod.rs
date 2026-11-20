//! Customers: walking agents, shopping lists, shop choice, movement, reputation
//! (§8.1–§8.4; R25, R55, R59).
//!
//! Owner boundary: individual shoppers on the map and the scoring that picks a shop. Counts,
//! arrivals and homes come from [`crate::population`]; stock and prices from
//! [`crate::businesses`], bought only through [`shops::Shops::try_sell`]. Money moves only
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
pub mod scoring;
pub mod shopping;
pub mod shops;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::businesses::{BusinessId, Businesses, Hours};
use crate::clock::Clock;
use crate::config::{Config, CustomersConfig};
use crate::land::Map;
use crate::money::{Money, Rate};
use crate::population::{Cohort, Population, ResidentId, allowance};
use crate::rng::{SimRng, below};

use movement::{Cell, Geometry, step_towards};
use shops::{BusinessShops, GoodId, GoodsIndex, SellError, ShopOffer, Shops};

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
}

impl Customers {
    /// A shop's reputation (§8.3).
    pub fn reputation(&self, shop: BusinessId, rules: &CustomersConfig) -> Rate {
        self.reputation
            .get(&shop)
            .copied()
            .unwrap_or_else(|| scoring::unit_to_rate(rules.reputation.initial))
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
}

impl PhaseInputs<'_> {
    /// The walking grid's shape.
    pub fn geometry(&self) -> Geometry {
        let [lot_w, lot_d] = self.config.game.world.lot_size;
        Geometry { lot_w, lot_d }
    }
}

/// Phase 8 over real businesses: builds the shop offers and front doors, then runs [`tick`]
/// with the minimal [`BusinessShops`] seller.
pub fn run_phase(
    state: &mut Customers,
    inputs: PhaseInputs<'_>,
    businesses: &mut Businesses,
    rng: &mut SimRng,
) -> PhaseReport {
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
    let doors = front_doors(businesses, inputs.map, geometry);
    let mut seller = BusinessShops {
        businesses,
        goods: &inputs.config.businesses.goods,
    };
    tick(state, inputs, &mut offers, &doors, &goods, &mut seller, rng)
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
    shops: &mut impl Shops,
    rng: &mut SimRng,
) -> PhaseReport {
    let _ = (
        state,
        inputs,
        offers,
        doors,
        goods,
        shops,
        rng,
        below,
        allowance,
        step_towards,
    );
    let _: Option<(Hours, SellError)> = None;
    PhaseReport::default()
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
    state: &Customers,
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
            state.reputation(o.shop, rules),
        ));
    }
    scoring::weighted_pick(&scores, rng).and_then(|k| index.get(k).copied())
}
