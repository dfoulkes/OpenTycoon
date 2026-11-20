//! Steps for `features/economy/retail_eras.feature` (R68): one shopkeeper and no rivals in the
//! shipped city, running one shop of reference size through the full 1930–2050 game, and its
//! books day by day.
//!
//! The shopkeeper plays through public commands only. Its daily books are read on the last
//! tick before midnight, once the day's trade is over and before the write-off (R1): takings,
//! the stock delivered that morning at purchase price (binned waste included), the land rent
//! and the utility bills the 06:00 settlement charged (§5.3).

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_core::ai::find_site;
use sim_core::businesses::{BusinessId, FitOutLevel, Hours, formula};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::Lease;
use sim_core::money::{BASIS_POINTS_PER_UNIT, Money};
use sim_core::world::Command;
use sim_core::{Rate, World};

use crate::SimWorld;
use crate::common;
use crate::steps::land::block_named;

const SHOPKEEPER: PlayerId = PlayerId(0);
const ACCOUNT: AccountId = AccountId::Player(SHOPKEEPER);

/// One day of the shop's books.
#[derive(Debug, Clone, Copy, Default)]
struct Day {
    year: i32,
    takings: Money,
    stock: Money,
    rent: Money,
    utilities: Money,
    delivered: i64,
    sold: i64,
}

impl Day {
    fn profit(&self) -> Money {
        self.takings - self.stock - self.rent - self.utilities
    }
}

/// The game, the shop and its books.
#[derive(Debug, Default)]
pub struct RetailState {
    game: Option<World>,
    shop: Option<BusinessId>,
    type_id: String,
    days: Vec<Day>,
}

fn game(w: &mut SimWorld) -> &mut World {
    w.retail.game.as_mut().expect("set up the city first")
}

#[given("a city with one shopkeeper and no rivals")]
fn lone_city(w: &mut SimWorld) {
    common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[]");
    w.retail.game = Some(World::new(w.config.load_ok(), 7, 1));
}

#[given(
    regex = r#"^the shopkeeper holds the lease on "([^"]+)" and runs an? (\w+) of reference size there at reference prices$"#
)]
fn runs_reference_shop(w: &mut SimWorld, block: String, type_id: String) {
    let world = game(w);
    let id = block_named(world, &block);
    let start = world.clock().year();
    // Long enough for the whole game: the scenario is about the shop, not the lease (R9).
    world.map_mut().blocks[id.0 as usize].lease =
        Some(Lease::new(ACCOUNT, start, 200, Money::ZERO));
    let config = world.config().clone();
    let kind = &config.businesses.types[&type_id];
    let height = formula::reference_height(&config);
    let (footprint, _) =
        find_site(world, ACCOUNT, id, &type_id, kind.min_footprint, height).expect("a site");
    world.submit(
        SHOPKEEPER,
        Command::Build {
            block: id,
            type_id: type_id.clone(),
            footprint,
            height,
            fit_out: FitOutLevel::Medium,
        },
    );
    world.tick();
    let (shop, b) = world
        .businesses()
        .owned_by(ACCOUNT)
        .next()
        .map(|(i, b)| (i, b.clone()))
        .expect("the shop went up");
    for good in b.prices.keys() {
        assert_eq!(b.prices[good], Rate::ONE, "reference prices");
        world.submit(
            SHOPKEEPER,
            Command::SetOrder {
                business: shop,
                good: good.clone(),
                quantity: b.capacity_per_good,
            },
        );
    }
    w.retail.shop = Some(shop);
    w.retail.type_id = type_id;
}

/// `⌈(sold + turned away) × 1.1⌉`, within capacity; `None` if nothing arrived today.
fn competent_order(world: &World, shop: BusinessId, good: &str) -> Option<i64> {
    let b = world.businesses().get(shop)?;
    if b.tomorrow_order.get(good).copied().unwrap_or(0) <= 0 {
        return None;
    }
    let sold = b.sold_today.get(good).copied().unwrap_or(0);
    let lost = if b.stock_of(good) == 0 {
        let sold_out = b
            .prices
            .keys()
            .filter(|g| b.stock_of(g) == 0)
            .count()
            .max(1);
        let sold_out = i64::try_from(sold_out).unwrap();
        (i64::from(b.lost_sales_today) + sold_out - 1) / sold_out
    } else {
        0
    };
    let unit = BASIS_POINTS_PER_UNIT;
    let wanted = ((sold + lost) * (unit + 1_000) + unit - 1) / unit;
    Some(wanted.clamp(1, b.capacity_per_good))
}

#[when(
    "the full game is played with the shopkeeper ordering what it sold plus the shoppers it turned away, plus 10%"
)]
fn play_full_game(w: &mut SimWorld) {
    let shop = w.retail.shop.expect("a shop");
    let type_id = w.retail.type_id.clone();
    let world = game(w);
    let rules = world.clock().rules().clone();
    let kind = world.config().businesses.types[&type_id].clone();
    let close = match kind.hours {
        Hours::Night => rules.trading.night.1,
        _ => rules.trading.day.1,
    };
    let last_tick = 86_400 - rules.tick_game_seconds;
    let mut ordered_on = None;
    let mut days = Vec::new();
    while !world.clock().is_over() {
        let now = world.clock().second_of_day();
        let today = world.clock().day();
        if now >= close && ordered_on != Some(today) {
            ordered_on = Some(today);
            let goods: Vec<String> = kind.goods.clone();
            for good in goods {
                if let Some(quantity) = competent_order(world, shop, &good) {
                    world.submit(
                        SHOPKEEPER,
                        Command::SetOrder {
                            business: shop,
                            good,
                            quantity,
                        },
                    );
                }
            }
        }
        world.tick();
        if world.clock().second_of_day() == last_tick {
            days.push(books(world, shop));
        }
    }
    assert!(world.is_active(SHOPKEEPER), "the shopkeeper went bankrupt");
    w.retail.days = days;
}

fn books(world: &World, shop: BusinessId) -> Day {
    let config = world.config();
    let year = world.clock().year();
    let b = world.businesses().get(shop).expect("the shop stands");
    let cost = world
        .businesses()
        .running_cost(shop, config, world.map(), year)
        .expect("running cost");
    let price = |good: &str| config.businesses.goods[good].purchase_price.money();
    Day {
        year,
        takings: b.takings_today,
        stock: b
            .tomorrow_order
            .iter()
            .map(|(g, u)| price(g).saturating_mul(*u))
            .sum(),
        rent: cost.rent,
        utilities: cost.utilities,
        delivered: b.tomorrow_order.values().sum(),
        sold: b.sold_today.values().sum(),
    }
}

#[then(regex = r"^the (\w+) made a net profit in every decade from (\d{4}) to (\d{4})$")]
fn profit_every_decade(w: &mut SimWorld, type_id: String, from: i32, to: i32) {
    assert_eq!(type_id, w.retail.type_id, "the shop under test");
    let mut decades: BTreeMap<i32, Money> = BTreeMap::new();
    for d in &w.retail.days {
        *decades.entry(d.year / 10 * 10).or_default() += d.profit();
    }
    let table: Vec<String> = decades.iter().map(|(y, p)| format!("{y}s {p}")).collect();
    for decade in (from..=to).step_by(10) {
        let profit = decades.get(&decade).copied().unwrap_or_default();
        assert!(
            profit > Money::ZERO,
            "{decade}s made {profit}; by decade: {}; days: {:?}",
            table.join(", "),
            w.retail.days
        );
    }
}

#[then(regex = r"^the (\w+) sold at least its reference footfall on most days it had stock$")]
fn reference_footfall(w: &mut SimWorld, type_id: String) {
    let config = w.config.load_ok();
    let kind = &config.businesses.types[&type_id];
    let footfall =
        formula::reference_footfall(kind, &config) * i64::try_from(kind.goods.len()).unwrap();
    let stocked: Vec<&Day> = w.retail.days.iter().filter(|d| d.delivered > 0).collect();
    let busy = stocked.iter().filter(|d| d.sold >= footfall).count();
    assert!(
        busy * 2 > stocked.len(),
        "reference footfall {footfall} reached on {busy} of {} stocked days: {:?}",
        stocked.len(),
        w.retail.days
    );
}
