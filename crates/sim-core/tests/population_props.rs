//! Property tests for population and customers invariants (docs/engineering.md §1):
//! the 05:30 house-hunt neither creates nor loses people and never over-lets a building; the
//! year step without deaths or migration keeps every adult; shoppers never drive stock
//! negative and every unit sold is a unit off the shelf.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use proptest::prelude::*;
use sim_core::World;
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::config::types::Ratio;
use sim_core::customers::{Customers, PhaseInputs, run_phase};
use sim_core::economy::AccountId;
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};
use sim_core::population::residential::listings;
use sim_core::population::{AgeBand, Class, Cohort, Population, Sex, arrivals, year_step};
use sim_core::rng::seeded;

fn cohort(band: u8, sex: u8, class: u8) -> Cohort {
    Cohort {
        band: AgeBand(band),
        sex: Sex::ALL[usize::from(sex)],
        class: Class::ALL[usize::from(class)],
    }
}

fn building(type_id: &str, block: u32, slot: u32, height: u32) -> Business {
    let config = common::shipped_config();
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

#[test]
fn house_hunting_conserves_people_and_never_over_lets() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(48), |(
        seed in any::<u64>(),
        arriving in prop::collection::vec((0u8..4, 0u8..2, 0u8..3, 0u64..60), 0..12),
        pool in prop::collection::vec((0u8..4, 0u8..2, 0u8..3, 0u64..60), 0..12),
        blocks in prop::collection::vec((0u32..9, 1u32..20, 100i64..800), 0..5),
        year in 1930i32..2050,
    )| {
        let config = common::shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        let mut pop = Population::default();
        for (i, (block, height, rent)) in blocks.iter().enumerate() {
            let id = BusinessId(u32::try_from(i).unwrap());
            businesses.by_id.insert(id, building("apartments", *block, 0, *height));
            pop.residential.rents.insert(id, Money::from_pence(*rent));
        }
        for (b, s, c, n) in arriving {
            *pop.pending.entry(cohort(b, s, c)).or_insert(0) += n;
        }
        for (b, s, c, n) in pool {
            *pop.pool.entry(cohort(b, s, c)).or_insert(0) += n;
        }
        let before = pop.total_adults() + pop.pending_arrivals();
        let l = listings(&businesses, &map, &config, &pop.residential);
        let mut rng = seeded(seed);
        let report = arrivals(&mut pop, &l, &config, year, &mut rng);
        prop_assert_eq!(pop.pending_arrivals(), 0);
        prop_assert_eq!(pop.total_adults(), before);
        prop_assert_eq!(report.housed + report.unhoused, report.arrived);
        let occupancy = pop.residential.occupancy();
        for listing in &l {
            prop_assert!(occupancy.get(&listing.building).copied().unwrap_or(0) <= listing.flats);
        }
    });
}

/// The commute-preference lever (docs/economy.md "Residential levers"): the house-hunt
/// never houses more than `⌊adults × (1 − commute_preference)⌋`, and still conserves people.
#[test]
fn commute_preference_caps_the_housed_share() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(48), |(
        seed in any::<u64>(),
        preference_bp in 0i64..=10_000,
        arriving in prop::collection::vec((0u8..4, 0u8..2, 0u8..3, 0u64..60), 0..12),
        pool in prop::collection::vec((0u8..4, 0u8..2, 0u8..3, 0u64..60), 0..12),
        blocks in prop::collection::vec((0u32..9, 1u32..20), 1..5),
    )| {
        let mut config = common::shipped_config();
        config.economy.residential.commute_preference = Ratio(Rate::from_bp(preference_bp));
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        let mut pop = Population::default();
        for (i, (block, height)) in blocks.iter().enumerate() {
            let id = BusinessId(u32::try_from(i).unwrap());
            businesses.by_id.insert(id, building("apartments", *block, 0, *height));
            pop.residential.rents.insert(id, Money::from_pence(100));
        }
        for (b, s, c, n) in arriving {
            *pop.pending.entry(cohort(b, s, c)).or_insert(0) += n;
        }
        for (b, s, c, n) in pool {
            *pop.pool.entry(cohort(b, s, c)).or_insert(0) += n;
        }
        let adults = pop.total_adults() + pop.pending_arrivals();
        let l = listings(&businesses, &map, &config, &pop.residential);
        let mut rng = seeded(seed);
        arrivals(&mut pop, &l, &config, 1950, &mut rng);
        let keep = u64::try_from(10_000 - preference_bp).unwrap();
        prop_assert_eq!(pop.total_adults(), adults);
        prop_assert!(pop.housed() <= adults * keep / 10_000, "{} housed of {}", pop.housed(), adults);
    });
}

#[test]
fn without_deaths_or_migration_the_year_step_keeps_every_adult() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(48), |(
        seed in any::<u64>(),
        pool in prop::collection::vec((0u8..4, 0u8..2, 0u8..3, 0u64..5_000), 0..24),
        year in 1930i32..2050,
    )| {
        let mut sources = common::shipped_sources();
        common::set_key(&mut sources, "economy.yaml", "/population/migration_per_day", "0");
        for b in ["18-29", "30-49", "50-64", "65+"] {
            common::set_key(&mut sources, "customers.yaml", &format!("/mortality_per_step/{b}"), "0");
        }
        let config = sim_core::Config::load(&sources).unwrap();
        let mut pop = Population::default();
        for (b, s, c, n) in pool {
            *pop.pool.entry(cohort(b, s, c)).or_insert(0) += n;
        }
        let before = pop.total_adults();
        let mut rng = seeded(seed);
        let step = year_step(&mut pop, &config, year + 5, 5, &mut rng);
        prop_assert_eq!(pop.total_adults(), before);
        prop_assert_eq!(step.deaths, 0);
        prop_assert_eq!(pop.children() + pop.pending_arrivals(), step.births);
    });
}

#[test]
fn shoppers_never_drive_stock_negative_and_sales_match_the_shelf() {
    // Read config/ before the first case: a broken file fails here once, not as a shrunk case.
    common::preload_shipped_config();
    proptest!(ProptestConfig::with_cases(48), |(
        seed in any::<u64>(),
        stocks in prop::collection::vec((0u32..9, 0i64..40), 1..6),
        people in 0u64..400,
    )| {
        let config = common::shipped_config();
        let map = Map::from_config(&config.map);
        let mut businesses = Businesses::default();
        for (i, (block, stock)) in stocks.iter().enumerate() {
            let mut b = building("bakery", *block, u32::try_from(i % 3).unwrap(), 4);
            b.stock.insert("bread".into(), *stock);
            b.prices.insert("bread".into(), Rate::from_bp(9_000 + 1_000 * i64::try_from(i).unwrap()));
            businesses.by_id.insert(BusinessId(u32::try_from(i).unwrap()), b);
        }
        let shelf_before: i64 = stocks.iter().map(|(_, s)| s).sum();
        let mut pop = Population::default();
        pop.pool.insert(cohort(1, 1, 0), people);
        let mut customers = Customers::default();
        let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
        let mut rng = seeded(seed);
        let mut sold = 0i64;
        for _ in 0..(18 * 360) {
            clock.advance();
            let report = run_phase(
                &mut customers,
                PhaseInputs { config: &config, map: &map, clock: &clock, population: &pop, modifiers: &sim_core::events::Modifiers::default(), streets: &sim_core::customers::streets::Streets::for_config(&map, &config) },
                &mut businesses,
                &mut rng,
            );
            sold += report.sales.iter().map(|s| i64::from(s.qty)).sum::<i64>();
            for b in businesses.by_id.values() {
                prop_assert!(b.stock.values().all(|s| *s >= 0));
            }
        }
        let shelf_after: i64 = businesses.by_id.values().flat_map(|b| b.stock.values()).sum();
        prop_assert_eq!(shelf_before - shelf_after, sold);
        prop_assert!(customers.agents.iter().all(|c| c.budget >= Money::ZERO));
    });
}

#[test]
fn a_full_game_grows_the_ledger_by_exactly_what_was_minted() {
    // R63: allowances are the only new money; every other movement is a transfer.
    let mut world = World::new(common::shipped_config(), 9, 2);
    let start = world.ledger().total();
    for _ in 0..(3 * 8_640) {
        let minted_before = world.ledger().minted();
        let total_before = world.ledger().total();
        world.tick();
        let minted = world.ledger().minted() - minted_before;
        assert!(minted >= Money::ZERO);
        assert_eq!(world.ledger().total(), total_before + minted);
    }
    assert!(world.ledger().minted() > Money::ZERO);
    assert_eq!(world.ledger().total(), start + world.ledger().minted());
    assert!(world.population().total_adults() > 0);
}

proptest! {
    #[test]
    fn minting_and_transfers_conserve_money_plus_what_was_minted(
        ops in prop::collection::vec((any::<bool>(), 0u8..3, 0u8..3, 0i64..1_000_000), 0..200)
    ) {
        use sim_core::economy::{Ledger, PlayerId};
        let acct = |i: u8| match i {
            0 => AccountId::City,
            1 => AccountId::Shoppers,
            _ => AccountId::Player(PlayerId(0)),
        };
        let mut ledger = Ledger::new();
        ledger.open(AccountId::City, Money::from_pounds(1_000));
        let start = ledger.total();
        for (mint, a, b, pence) in ops {
            if mint {
                ledger.mint(acct(a), Money::from_pence(pence));
            } else {
                ledger.transfer(acct(a), acct(b), Money::from_pence(pence));
            }
            prop_assert_eq!(ledger.total(), start + ledger.minted());
        }
    }
}
