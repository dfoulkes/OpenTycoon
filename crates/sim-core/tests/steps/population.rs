//! Steps for `features/population/`, and the city scene shared with `features/customers/`.
#![allow(
    clippy::many_single_char_names,
    reason = "step captures read as class, sex, band: c, s, b"
)]

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::customers::{Customers, PhaseReport};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::Money;
use sim_core::population::residential::{
    self, ApartmentCosts, HouseHunt, RentError, apartment_daily_costs, candidates, listings,
    rent_due,
};
use sim_core::population::{
    AgeBand, ArrivalsReport, Class, Cohort, Population, ResidentId, Sex, YearStep, allowance,
    band_for_age, band_id, year_step,
};
use sim_core::rng::{SimRng, seeded};
use sim_core::{Config, World};

use crate::SimWorld;
use crate::common;

/// A small city built step by step: map, buildings, people, shoppers and a clock.
#[derive(Debug, Default)]
pub struct CityState {
    config: Option<Config>,
    map: Option<Map>,
    /// Buildings.
    pub businesses: Businesses,
    /// Building names used by the scenario.
    pub names: BTreeMap<String, BusinessId>,
    /// People.
    pub population: Population,
    /// Shoppers.
    pub customers: Customers,
    /// The scene's seeded RNG.
    pub rng: Option<SimRng>,
    year: Option<i32>,
    /// The city's clock (customers scenarios).
    pub clock: Option<Clock>,
    last_step: YearStep,
    arrivals: ArrivalsReport,
    resident: Option<ResidentId>,
    rent_result: Option<Result<(), RentError>>,
    /// Every report from the customers phase, in order.
    pub reports: Vec<PhaseReport>,
    /// Every shopper ever spawned (copied as first seen).
    pub spawned: Vec<sim_core::customers::Customer>,
    /// Picks by shop, from "choose where to buy" steps.
    pub picks: BTreeMap<BusinessId, u32>,
    /// Number of shoppers in the last "choose" step.
    pub pick_total: u32,
    world: Option<World>,
    ledger_at_start: Option<Money>,
    daily: Vec<u64>,
    pending_each_morning: Vec<u64>,
}

impl CityState {
    /// The scenario's config (loaded once, after the config Givens).
    pub fn config(w: &mut SimWorld) -> Config {
        if w.city.config.is_none() {
            w.city.config = Some(w.config.load_ok());
        }
        w.city.config.clone().unwrap()
    }

    /// The map.
    pub fn map(w: &mut SimWorld) -> Map {
        if w.city.map.is_none() {
            let config = Self::config(w);
            w.city.map = Some(Map::from_config(&config.map));
        }
        w.city.map.clone().unwrap()
    }

    /// The seeded RNG.
    pub fn rng(&mut self) -> &mut SimRng {
        self.rng.get_or_insert_with(|| seeded(42))
    }

    /// The scenario's year.
    pub fn year(w: &mut SimWorld) -> i32 {
        if let Some(y) = w.city.year {
            return y;
        }
        Self::config(w).game.clock.start_year
    }

    /// The clock, created at 00:00 on the first day.
    pub fn clock(w: &mut SimWorld) -> &mut Clock {
        if w.city.clock.is_none() {
            let config = Self::config(w);
            w.city.clock = Some(Clock::new(ClockRules::from_config(&config.game.clock)));
        }
        w.city.clock.as_mut().unwrap()
    }

    /// A block id by config key.
    pub fn block(w: &mut SimWorld, key: &str) -> BlockId {
        let map = Self::map(w);
        let i = map
            .blocks
            .iter()
            .position(|b| b.key == key)
            .unwrap_or_else(|| panic!("no block {key}"));
        BlockId(u32::try_from(i).unwrap())
    }

    /// A building id by scenario name.
    pub fn building(&self, name: &str) -> BusinessId {
        *self
            .names
            .get(name)
            .unwrap_or_else(|| panic!("no building {name:?}"))
    }

    /// Adds a building with a footprint on the lot rim that doesn't overlap the block's others.
    pub fn add_building(
        w: &mut SimWorld,
        name: &str,
        type_id: &str,
        block: &str,
        (fw, fd, h): (u32, u32, u32),
        owner: AccountId,
    ) -> BusinessId {
        let block = Self::block(w, block);
        let on_block = w
            .city
            .businesses
            .by_id
            .values()
            .filter(|b| b.block == block)
            .count();
        // Up to three buildings along the north edge of a 9×9 lot, then the south edge.
        let slot = u32::try_from(on_block).unwrap();
        let footprint = Footprint {
            x: (slot % 3) * 3,
            y: if slot < 3 { 0 } else { 9 - fd },
            w: fw,
            d: fd,
        };
        let id = BusinessId(w.city.businesses.next_id);
        w.city.businesses.next_id += 1;
        let config = Self::config(w);
        let kind = &config.businesses.types[type_id];
        let request = BuildRequest {
            type_id: type_id.to_owned(),
            block,
            footprint,
            height: h,
            owner,
            fit_out: FitOutLevel::Medium,
        };
        w.city
            .businesses
            .by_id
            .insert(id, Business::new(request, kind, Money::ZERO));
        w.city.names.insert(name.to_owned(), id);
        id
    }
}

/// Parses `working|middle|upper`.
pub fn class(text: &str) -> Class {
    match text {
        "working" => Class::Working,
        "middle" => Class::Middle,
        "upper" => Class::Upper,
        other => panic!("unknown class {other}"),
    }
}

/// Parses `man|men|woman|women`.
pub fn sex(text: &str) -> Sex {
    match text {
        "man" | "men" => Sex::Male,
        "woman" | "women" => Sex::Female,
        other => panic!("unknown sex {other}"),
    }
}

/// A band index by id.
pub fn band(w: &mut SimWorld, id: &str) -> AgeBand {
    let config = CityState::config(w);
    let i = config
        .customers
        .age_bands
        .iter()
        .position(|b| b.id == id)
        .unwrap_or_else(|| panic!("no band {id}"));
    AgeBand(u8::try_from(i).unwrap())
}

/// A cohort from step text.
pub fn cohort(w: &mut SimWorld, c: &str, s: &str, b: &str) -> Cohort {
    Cohort {
        band: band(w, b),
        sex: sex(s),
        class: class(c),
    }
}

/// `£12.34` (without the £) as money.
pub fn pounds(text: &str) -> Money {
    let text = text.trim_start_matches('£');
    let (whole, frac) = text.split_once('.').unwrap_or((text, "0"));
    let frac = format!("{frac:0<2}");
    Money::from_pence(whole.parse::<i64>().unwrap() * 100 + frac[..2].parse::<i64>().unwrap())
}

// ---------------------------------------------------------------------------------------------
// Config shortcuts

#[given("migration is off and nobody dies")]
fn no_migration_no_deaths(w: &mut SimWorld) {
    no_migration(w);
    nobody_dies(w);
}

#[given("migration is off")]
fn no_migration(w: &mut SimWorld) {
    common::set_key(
        w.config.sources_mut(),
        "customers.yaml",
        "/migration_per_day",
        "0",
    );
}

#[given("nobody dies")]
fn nobody_dies(w: &mut SimWorld) {
    for b in ["18-29", "30-49", "50-64", "65+"] {
        common::set_key(
            w.config.sources_mut(),
            "customers.yaml",
            &format!("/mortality_per_step/{b}"),
            "0",
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Cohorts (R60, R61, R6)

#[then(expr = "the age bands are {string}")]
fn age_bands(w: &mut SimWorld, list: String) {
    let config = CityState::config(w);
    let ids: Vec<&str> = config
        .customers
        .age_bands
        .iter()
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids.join(", "), list);
}

#[then(regex = r#"^a person aged (\d+) is (?:a child|in band "([^"]+)")$"#)]
fn person_aged(w: &mut SimWorld, age: u32, expected: String) {
    let config = CityState::config(w);
    let got = band_for_age(&config.customers, age).map(|b| band_id(&config.customers, b));
    if expected.is_empty() {
        assert_eq!(got, None, "age {age} should be a child");
    } else {
        assert_eq!(got, Some(expected.as_str()));
    }
}

#[given(regex = r#"^a city of (\d+) (working|middle|upper) (men|women) aged "([^"]+)"$"#)]
fn city_of(w: &mut SimWorld, n: u64, c: String, s: String, b: String) {
    let key = cohort(w, &c, &s, &b);
    *w.city.population.pool.entry(key).or_insert(0) += n;
}

#[given(expr = "{int} children born by {int}")]
fn children_born(w: &mut SimWorld, n: u64, year: i32) {
    w.city.population.children.insert(year, n);
    w.city.year = Some(year);
}

fn step_year(w: &mut SimWorld, from: i32, to: i32) {
    let config = CityState::config(w);
    let years = u32::try_from(to - from).unwrap();
    let mut rng = w.city.rng().clone();
    w.city.last_step = year_step(
        &mut w.city.population,
        &config.customers,
        to,
        years,
        &mut rng,
    );
    w.city.rng = Some(rng);
    w.city.year = Some(to);
}

#[when(expr = "the year steps from {int} to {int}")]
fn year_steps_from(w: &mut SimWorld, from: i32, to: i32) {
    step_year(w, from, to);
}

#[when(expr = "the year steps to {int}")]
fn year_steps_to(w: &mut SimWorld, to: i32) {
    let config = CityState::config(w);
    let per = i32::try_from(config.game.clock.years_per_day).unwrap();
    let mut year = CityState::year(w);
    while year < to {
        step_year(w, year, year + per);
        year += per;
    }
}

#[then(expr = "{int} children were born")]
fn children_were_born(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.last_step.births, n);
}

#[then(expr = "{int} children remain")]
fn children_remain(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.population.children(), n);
}

#[then(expr = "{int} adults died")]
fn adults_died(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.last_step.deaths, n);
}

#[then(expr = "{int} adults live in the city")]
fn adults_live(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.population.total_adults(), n);
}

#[then(regex = r"^(\d+) (?:people are|person is) waiting for the 05:30 train$")]
fn waiting(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.population.pending_arrivals(), n);
}

#[then(expr = "{int} newcomers aged {string} are waiting for the 05:30 train")]
fn newcomers_waiting(w: &mut SimWorld, n: u64, b: String) {
    let band = band(w, &b);
    let total: u64 = w
        .city
        .population
        .pending
        .iter()
        .filter(|(c, _)| c.band == band)
        .map(|(_, n)| *n)
        .sum();
    assert_eq!(total, n);
    assert_eq!(w.city.population.pending_arrivals(), n);
}

#[then(regex = r#"^there are (\d+) (working|middle|upper) (men|women) aged "([^"]+)"$"#)]
fn there_are(w: &mut SimWorld, n: u64, c: String, s: String, b: String) {
    let key = cohort(w, &c, &s, &b);
    assert_eq!(w.city.population.adults_in(key), n);
}

#[then(expr = "every allowance is the same in every year from {int} to {int}")]
fn allowance_flat(w: &mut SimWorld, from: i32, to: i32) {
    // `allowance` takes no year: there is nothing for inflation to act on (R6). Check the
    // whole table is positive and a resident's budget is the same when the year changes.
    let config = CityState::config(w);
    for c in Class::ALL {
        for s in Sex::ALL {
            for b in 0..config.customers.age_bands.len() {
                let key = Cohort {
                    band: AgeBand(u8::try_from(b).unwrap()),
                    sex: s,
                    class: c,
                };
                let a = allowance(&config.customers, key);
                assert!(a > Money::ZERO);
                let mut year = from;
                while year <= to {
                    assert_eq!(allowance(&config.customers, key), a, "{year}");
                    year += 5;
                }
            }
        }
    }
}

#[given("a fresh city")]
fn new_game(w: &mut SimWorld) {
    let config = CityState::config(w);
    let world = World::new(config, 42, 2);
    w.city.ledger_at_start = Some(world.ledger().total());
    w.city.world = Some(world);
}

#[when(expr = "the fresh city runs until {string}")]
fn fresh_city_until(w: &mut SimWorld, time: String) {
    let parts: Vec<u32> = time.split(':').map(|p| p.parse().unwrap()).collect();
    let target = parts[0] * 3600 + parts[1] * 60 + parts[2];
    let world = w.city.world.as_mut().unwrap();
    loop {
        world.tick();
        if world.clock().second_of_day() == target {
            break;
        }
    }
}

#[then("the shoppers' purse holds every adult's daily allowance")]
fn purse_holds(w: &mut SimWorld) {
    let config = CityState::config(w);
    let world = w.city.world.as_ref().unwrap();
    let due = sim_core::population::daily_allowances(world.population(), &config.customers);
    assert!(due > Money::ZERO);
    assert_eq!(world.ledger().balance(AccountId::Shoppers), due);
}

#[then("the shoppers' purse holds every adult's daily allowance less the residents' rent")]
fn purse_holds_less_rent(w: &mut SimWorld) {
    let config = CityState::config(w);
    let world = w.city.world.as_ref().unwrap();
    let due = sim_core::population::daily_allowances(world.population(), &config.customers);
    let rent: Money = world.rent_due().iter().map(|d| d.amount).sum();
    assert!(rent > Money::ZERO, "nobody owes rent");
    assert_eq!(world.ledger().balance(AccountId::Shoppers), due - rent);
}

#[then("the ledger grew by exactly the allowances minted")]
fn ledger_grew_by_minted(w: &mut SimWorld) {
    let world = w.city.world.as_ref().unwrap();
    let start = w.city.ledger_at_start.unwrap();
    assert!(world.ledger().minted() > Money::ZERO);
    assert_eq!(world.ledger().total(), start + world.ledger().minted());
}

#[then("more was minted than one morning's allowances")]
fn minted_several(w: &mut SimWorld) {
    let config = CityState::config(w);
    let world = w.city.world.as_ref().unwrap();
    let one = sim_core::population::daily_allowances(world.population(), &config.customers);
    assert!(
        world.ledger().minted() > one,
        "{} vs {one}",
        world.ledger().minted()
    );
}

#[then("every city apartment block has residents")]
fn city_blocks_let(w: &mut SimWorld) {
    let config = CityState::config(w);
    let world = w.city.world.as_ref().unwrap();
    let occupancy = world.population().residential.occupancy();
    let blocks: Vec<_> = world
        .businesses()
        .by_id
        .iter()
        .filter(|(_, b)| {
            b.owner == AccountId::City && b.type_id == config.customers.residential.building_type
        })
        .map(|(id, _)| *id)
        .collect();
    assert!(
        !blocks.is_empty(),
        "the city should start with apartment blocks"
    );
    for id in blocks {
        assert!(
            occupancy.get(&id).copied().unwrap_or(0) > 0,
            "{id:?} is empty"
        );
    }
}

#[when(expr = "the fresh city runs for {int} days")]
fn game_runs(w: &mut SimWorld, days: u64) {
    let world = w.city.world.as_mut().unwrap();
    let per_day = u64::from(world.clock().rules().ticks_per_day());
    let arrivals_second = world.clock().rules().arrivals_second;
    let tick_seconds = world.clock().rules().tick_game_seconds;
    for _ in 0..days * per_day {
        if world.clock().second_of_day() + tick_seconds == arrivals_second {
            w.city
                .pending_each_morning
                .push(world.population().pending_arrivals());
        }
        let fired = world.tick();
        if fired.settlement {
            w.city.daily.push(world.population().everyone());
        }
    }
}

#[then("the population grew every day")]
fn grew(w: &mut SimWorld) {
    let d = &w.city.daily;
    assert!(d.len() >= 2, "{d:?}");
    assert!(d.windows(2).all(|p| p[1] > p[0]), "{d:?}");
}

#[then("people arrived on every morning's train")]
fn arrived_every_morning(w: &mut SimWorld) {
    let p = &w.city.pending_each_morning;
    assert!(!p.is_empty() && p.iter().all(|n| *n > 0), "{p:?}");
}

// ---------------------------------------------------------------------------------------------
// Apartments (R50–R54, R56)

#[allow(
    clippy::too_many_arguments,
    reason = "one capture per term of the step"
)]
#[given(
    regex = r#"^an apartment block "([^"]+)" on "([^"]+)" (\d+)×(\d+)×(\d+)(?: with (low|medium|high) fit-out)?(?: owned by player (\d+))?$"#
)]
fn apartment_block(
    w: &mut SimWorld,
    name: String,
    block: String,
    fw: u32,
    fd: u32,
    h: u32,
    fit: String,
    owner: String,
) {
    let config = CityState::config(w);
    let owner = if owner.is_empty() {
        AccountId::City
    } else {
        AccountId::Player(PlayerId(owner.parse().unwrap()))
    };
    let id = CityState::add_building(
        w,
        &name,
        &config.customers.residential.building_type,
        &block,
        (fw, fd, h),
        owner,
    );
    let level = match fit.as_str() {
        "low" => FitOutLevel::Low,
        "high" => FitOutLevel::High,
        _ => FitOutLevel::Medium,
    };
    w.city.businesses.by_id.get_mut(&id).unwrap().fit_out = level;
}

#[given(regex = r#"^the rent at "([^"]+)" is £([\d.]+)$"#)]
fn rent_is(w: &mut SimWorld, name: String, rent: String) {
    let id = w.city.building(&name);
    w.city
        .population
        .residential
        .rents
        .insert(id, pounds(&rent));
}

#[then(regex = r#"^the rent at "([^"]+)" is £([\d.]+)$"#)]
fn rent_now(w: &mut SimWorld, name: String, rent: String) {
    let config = CityState::config(w);
    let id = w.city.building(&name);
    let got = w
        .city
        .population
        .residential
        .rent(id, config.customers.residential.default_rent.money());
    assert_eq!(got, pounds(&rent));
}

#[given(expr = "{string} is full")]
fn is_full(w: &mut SimWorld, name: String) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let id = w.city.building(&name);
    let listing = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    )
    .into_iter()
    .find(|l| l.building == id)
    .unwrap();
    let filler = cohort(w, "working", "men", "18-29");
    for _ in 0..listing.flats {
        w.city.population.residential.house(filler, id, 1930);
    }
}

#[then(expr = "{string} has {int} flats")]
fn has_flats(w: &mut SimWorld, name: String, n: u32) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let id = w.city.building(&name);
    let l = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    );
    let listing = l.iter().find(|l| l.building == id).unwrap();
    assert_eq!(listing.flats, n);
}

fn costs(w: &mut SimWorld, name: &str) -> ApartmentCosts {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let id = w.city.building(name);
    apartment_daily_costs(&w.city.businesses, id, &map, &config).unwrap()
}

#[then(regex = r#"^"([^"]+)" pays £([\d.]+) land rent a day$"#)]
fn pays_land_rent(w: &mut SimWorld, name: String, amount: String) {
    assert_eq!(costs(w, &name).land_rent, pounds(&amount));
}

#[then(regex = r#"^"([^"]+)" pays £([\d.]+) in utility bills a day$"#)]
fn pays_utilities(w: &mut SimWorld, name: String, amount: String) {
    assert_eq!(costs(w, &name).utilities, pounds(&amount));
}

#[then(expr = "{string} can hold {int} stock")]
fn holds_stock(w: &mut SimWorld, name: String, n: i64) {
    assert_eq!(costs(w, &name).stock_capacity, n);
}

#[given(
    regex = r"^(\d+) (working|middle|upper) (man|men|woman|women) aged .(.+). arrives? on the morning train$"
)]
fn arrive(w: &mut SimWorld, n: u64, c: String, s: String, b: String) {
    let key = cohort(w, &c, &s, &b);
    *w.city.population.pending.entry(key).or_insert(0) += n;
}

fn house_hunt(w: &mut SimWorld, year: i32) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let l = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    );
    let mut rng = w.city.rng().clone();
    w.city.arrivals = residential::arrivals(
        &mut w.city.population,
        &l,
        &config.customers,
        year,
        &mut rng,
    );
    w.city.rng = Some(rng);
    w.city.year = Some(year);
}

#[when("the 05:30 arrivals house-hunt")]
fn hunt(w: &mut SimWorld) {
    let year = CityState::year(w);
    house_hunt(w, year);
}

#[when(expr = "the 05:30 arrivals house-hunt in {int}")]
fn hunt_in(w: &mut SimWorld, year: i32) {
    house_hunt(w, year);
}

fn living_in(w: &SimWorld, name: &str) -> u32 {
    let id = w.city.building(name);
    w.city
        .population
        .residential
        .occupancy()
        .get(&id)
        .copied()
        .unwrap_or(0)
}

#[then(regex = r#"^(\d+) (?:person lives|people live) in "([^"]+)"$"#)]
fn live_in(w: &mut SimWorld, n: u32, name: String) {
    assert_eq!(living_in(w, &name), n);
}

#[then(regex = r#"^(\d+) people live in "([^"]+)" or "([^"]+)"$"#)]
fn live_in_either(w: &mut SimWorld, n: u32, a: String, b: String) {
    assert_eq!(living_in(w, &a) + living_in(w, &b), n);
}

#[then(expr = "more people live in {string} than in {string}")]
fn more_live_in(w: &mut SimWorld, a: String, b: String) {
    let (na, nb) = (living_in(w, &a), living_in(w, &b));
    assert!(na > nb, "{a}: {na}, {b}: {nb}");
}

#[then(expr = "{int} arrivals are commuting or visiting")]
fn commuting(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.arrivals.unhoused, n);
    assert_eq!(w.city.population.visitors(), n);
}

#[then(regex = r#"^at the 06:00 settlement "([^"]+)" is owed £([\d.]+) by its residents$"#)]
fn owed(w: &mut SimWorld, name: String, amount: String) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let id = w.city.building(&name);
    let l = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    );
    let due = rent_due(&w.city.population.residential, &l, &config.customers);
    let bill = due.iter().find(|d| d.building == id).unwrap();
    assert_eq!(bill.amount, pounds(&amount));
    assert_eq!(bill.landlord, w.city.businesses.by_id[&id].owner);
}

#[then(regex = r#"^£([\d.]+) of the rent at "([^"]+)" is paid to the city as rent tax$"#)]
fn rent_tax_to_city(w: &mut SimWorld, amount: String, name: String) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let id = w.city.building(&name);
    let l = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    );
    let due = rent_due(&w.city.population.residential, &l, &config.customers);
    let bill = due.iter().find(|d| d.building == id).unwrap();
    assert_eq!(bill.tax, pounds(&amount));
}

#[then(expr = "{int} people in the city are commuting or visiting")]
fn visitors_in_city(w: &mut SimWorld, n: u64) {
    assert_eq!(w.city.population.visitors(), n);
}

#[then(
    regex = r#"^an? (working|middle|upper) (man|woman) aged "([^"]+)" would consider "([^"]+)"$"#
)]
fn would_consider(w: &mut SimWorld, c: String, s: String, b: String, expected: String) {
    let config = CityState::config(w);
    let map = CityState::map(w);
    let key = cohort(w, &c, &s, &b);
    let l = listings(
        &w.city.businesses,
        &map,
        &config,
        &w.city.population.residential,
    );
    let occupancy = w.city.population.residential.occupancy();
    let got = match candidates(key, &l, &occupancy, &config.customers) {
        Ok(idx) => {
            let mut names: Vec<&str> = idx
                .iter()
                .map(|i| {
                    let id = l[*i].building;
                    w.city
                        .names
                        .iter()
                        .find(|(_, v)| **v == id)
                        .map(|(k, _)| k.as_str())
                        .unwrap()
                })
                .collect();
            names.sort_unstable();
            if names.is_empty() {
                "nothing".to_owned()
            } else {
                names.join(", ")
            }
        }
        Err(HouseHunt::PricedOut | HouseHunt::NoVacancy) => "nothing".to_owned(),
        Err(HouseHunt::Housed(_)) => unreachable!(),
    };
    assert_eq!(got, expected);
}

#[when(regex = r#"^player (\d+) sets the rent at "([^"]+)" to £([\d.]+)$"#)]
fn player_sets_rent(w: &mut SimWorld, player: u8, name: String, rent: String) {
    let config = CityState::config(w);
    let id = w.city.building(&name);
    w.city.rent_result = Some(residential::set_rent(
        &mut w.city.population.residential,
        &w.city.businesses,
        &config.customers.residential.building_type,
        PlayerId(player),
        id,
        pounds(&rent),
    ));
}

#[then(expr = "the rent change is refused because {string}")]
fn refused(w: &mut SimWorld, why: String) {
    let expected = match why.as_str() {
        "not the owner" => RentError::NotOwner,
        "not apartments" => RentError::NotApartments,
        _ => RentError::Negative,
    };
    assert_eq!(w.city.rent_result, Some(Err(expected)));
}

#[given(
    regex = r#"^an? (working|middle|upper) (man|woman) aged "([^"]+)" has lived in "([^"]+)" since (\d+)$"#
)]
fn has_lived(w: &mut SimWorld, c: String, s: String, b: String, name: String, year: i32) {
    let key = cohort(w, &c, &s, &b);
    let id = w.city.building(&name);
    let r = w.city.population.residential.house(key, id, year);
    w.city.resident = Some(r);
}

#[then(expr = "the resident lives in {string}")]
fn resident_lives(w: &mut SimWorld, name: String) {
    let id = w.city.building(&name);
    let r = w.city.resident.unwrap();
    assert_eq!(w.city.population.residential.residents[&r].home, id);
}

#[then(expr = "the resident moved in {int}")]
fn resident_moved(w: &mut SimWorld, year: i32) {
    let r = w.city.resident.unwrap();
    assert_eq!(w.city.population.residential.residents[&r].moved_year, year);
}
