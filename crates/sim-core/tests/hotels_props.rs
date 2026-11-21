//! Property tests for Hotels and online demand (docs/engineering.md §1): a Hotel never lets
//! more rooms than its volume allows, only tourists take them, and no guest pays more than
//! they brought out (R29, R30, R59); with Hotels
//! and online sellers trading, the ledger still grows by exactly what was minted and the
//! shoppers' purse never goes below zero (R63).
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use std::collections::BTreeMap;

use proptest::prelude::*;
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::customers::{CustomerId, Customers, Origin, PhaseInputs, run_phase, wave_budget};
use sim_core::economy::AccountId;
use sim_core::hotels;
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};
use sim_core::population::{AgeBand, Class, Cohort, Population, Sex, allowance};
use sim_core::rng::seeded;
use sim_core::{Config, ConfigSources, World};

fn building(config: &Config, type_id: &str, block: u32, slot: u32, height: u32) -> Business {
    let request = BuildRequest {
        type_id: type_id.to_owned(),
        block: BlockId(block),
        footprint: Footprint {
            x: (slot % 3) * 3,
            y: 0,
            w: 3,
            d: 3,
        },
        height,
        owner: AccountId::City,
        fit_out: FitOutLevel::Medium,
    };
    Business::new(request, &config.businesses.types[type_id], Money::ZERO)
}

/// The shipped config plus an online bakery type, starting in `year`.
fn config_with_online(year: i32) -> Config {
    let mut sources: ConfigSources = common::shipped_sources();
    common::set_key(
        &mut sources,
        "businesses.yaml",
        "/types/web_bakery",
        "{name: Web Bakery, min_footprint: [2, 2], max_height: 20, stock_density: 0.5, \
         hours: day, goods: [bread], utility_factor: {electricity: 1.0, gas: 0.0, water: 0.0, \
         telecoms: 2.0}, unlock_year: 1995, channel: online}",
    );
    common::set_key(
        &mut sources,
        "game.yaml",
        "/clock/start_year",
        &year.to_string(),
    );
    Config::load(&sources).unwrap()
}

#[test]
fn hotels_never_let_more_rooms_than_they_have_and_only_to_tourists() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(24), |(
        seed in any::<u64>(),
        heights in prop::collection::vec((0u32..9, 1u32..12, 30i64..300), 1..4),
        people in 0u64..600,
        class in 0usize..3,
    )| {
        let config = common::shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        for (i, (block, height, price)) in heights.iter().enumerate() {
            let mut b = building(&config, "hotel", *block, u32::try_from(i % 3).unwrap(), *height);
            b.prices.insert("room_night".into(), Rate::from_bp(price * 100));
            businesses.by_id.insert(BusinessId(u32::try_from(i).unwrap()), b);
        }
        let mut bar = building(&config, "bar", 4, 2, 4);
        bar.stock.insert("drinks".into(), 10_000);
        businesses.by_id.insert(BusinessId(99), bar);
        let rooms: BTreeMap<BusinessId, u32> = businesses
            .by_id
            .iter()
            .filter(|(_, b)| hotels::is_hotel(&config, &b.type_id))
            .map(|(id, b)| (*id, hotels::rooms_in(&config, b)))
            .collect();
        let mut pop = Population::default();
        let who = Cohort { band: AgeBand(1), sex: Sex::Male, class: Class::ALL[class] };
        pop.pool.insert(who, people);
        let mut customers = Customers::default();
        let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
        while clock.second_of_day() != 18 * 3600 + 59 * 60 + 50 {
            clock.advance();
        }
        let mut rng = seeded(seed);
        let mut let_tonight: BTreeMap<BusinessId, u32> = BTreeMap::new();
        let mut budgets: BTreeMap<CustomerId, Money> = BTreeMap::new();
        let mut spent: BTreeMap<CustomerId, Money> = BTreeMap::new();
        // 19:00 to 04:00 the next morning.
        for _ in 0..(9 * 360) {
            clock.advance();
            let report = run_phase(
                &mut customers,
                PhaseInputs { config: &config, map: &map, clock: &clock, population: &pop, modifiers: &sim_core::events::Modifiers::default() },
                &mut businesses,
                &mut rng,
            );
            for c in &report.check_ins {
                prop_assert_eq!(c.origin, Origin::Tourist);
                *let_tonight.entry(c.hotel).or_default() += 1;
                *spent.entry(c.customer).or_default() += c.amount;
            }
            for s in &report.sales {
                *spent.entry(s.customer).or_default() += s.amount;
            }
            for c in &customers.agents {
                budgets.entry(c.id).or_insert_with(|| {
                    wave_budget(&config.customers, allowance(&config, c.cohort), c.wave)
                });
            }
            for (hotel, taken) in &customers.hotel_guests {
                prop_assert!(*taken <= rooms[hotel], "{} of {} rooms", taken, rooms[hotel]);
            }
            prop_assert!(customers.agents.iter().all(|c| c.budget >= Money::ZERO));
        }
        for (who, paid) in &spent {
            prop_assert!(*paid <= budgets[who], "{:?} paid {} of {}", who, paid, budgets[who]);
        }
        for (hotel, n) in &let_tonight {
            prop_assert!(*n <= rooms[hotel]);
        }
        prop_assert!(
            customers.agents.is_empty()
                && customers.wanderers.is_empty()
                && customers.parked.is_empty(),
            "everyone is in bed or gone by 04:00"
        );
    });
}

/// A 2000 game with a Hotel and a well-stocked online seller: every tick the ledger grows by
/// exactly what was minted (R63), and the shoppers' purse never goes negative, while rooms are
/// let and bread is sold online.
fn run_game_with_hotels_and_online(seed: u64, ticks: u32) -> (u32, i64) {
    let config = config_with_online(2000);
    let mut world = World::new(config.clone(), seed, 2);
    let mut hotel = building(&config, "hotel", 8, 0, 10);
    hotel.owner = AccountId::Player(sim_core::economy::PlayerId(0));
    let mut web = building(&config, "web_bakery", 0, 0, 20);
    web.owner = AccountId::Player(sim_core::economy::PlayerId(0));
    web.stock.insert("bread".into(), 5_000);
    let businesses = world.businesses_mut();
    let hotel_id = BusinessId(businesses.next_id);
    let web_id = BusinessId(businesses.next_id + 1);
    businesses.next_id += 2;
    businesses.by_id.insert(hotel_id, hotel);
    businesses.by_id.insert(web_id, web);
    let start = world.ledger().total();
    let mut guests = 0;
    let mut online = 0;
    for _ in 0..ticks {
        let minted = world.ledger().minted();
        let total = world.ledger().total();
        world.tick();
        let new_money = world.ledger().minted() - minted;
        assert_eq!(
            world.ledger().total(),
            total + new_money,
            "tick {}",
            world.clock().tick()
        );
        assert!(world.ledger().balance(AccountId::Shoppers) >= Money::ZERO);
        guests = guests.max(
            world
                .customers()
                .hotel_guests
                .get(&hotel_id)
                .copied()
                .unwrap_or(0),
        );
        let b = &world.businesses().by_id[&web_id];
        online = online.max(b.sold_today.get("bread").copied().unwrap_or(0));
    }
    assert_eq!(world.ledger().total(), start + world.ledger().minted());
    (guests, online)
}

#[test]
fn hotels_and_online_sales_conserve_money_plus_what_was_minted() {
    let (guests, online) = run_game_with_hotels_and_online(9, 8_640 + 1_200);
    assert!(guests > 0, "nobody checked in, so the test proves nothing");
    assert!(
        online > 0,
        "nothing sold online, so the test proves nothing"
    );
}

#[test]
fn money_is_conserved_with_hotels_and_online_for_any_seed() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(3), |(seed in any::<u64>())| {
        run_game_with_hotels_and_online(seed, 8_640 + 600);
    });
}
