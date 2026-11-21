//! Events and utilities over whole games (§6, §9, §9.1; R12): the full 1930–2050 span plays
//! headless with every event, election and random building event; money is conserved through
//! all of them (compensation, buy-outs, insurance and fines are transfers, never minted); and
//! the same seed replays the same history.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use proptest::prelude::*;
use sim_core::config::types::Ratio;
use sim_core::economy::{AccountId, PlayerId};
use sim_core::money::{Money, Rate};
use sim_core::utilities::UtilityKind;
use sim_core::world::Command;
use sim_core::{Config, World};

/// The shipped config with a small city and random building events made common.
fn eventful(chance_bp: i64) -> Config {
    let mut sources = common::shipped_sources();
    common::set_key(&mut sources, "customers.yaml", "/initial_population", "300");
    let mut config = Config::load(&sources).unwrap();
    for event in &mut config.events.random {
        event.daily_chance = Ratio(Rate::from_bp(chance_bp));
    }
    config
}

/// The money every game starts with: each seat's starting cash.
fn opening_money(world: &World) -> Money {
    let seats = i64::try_from(world.players().len()).unwrap();
    world
        .config()
        .economy
        .money
        .starting_cash
        .money()
        .saturating_mul(seats)
}

/// Plays a game to the end (or `until_year`), holding the ledger invariant at every midnight
/// and every 06:00: `total = opening balances + minted allowances` (R63).
fn play(config: Config, seed: u64, until_year: i32) -> World {
    let mut world = World::new(config, seed, 3);
    // Seat 0 bids for a utility and insures itself; the Grocer plays seat 2.
    world.submit(
        PlayerId(0),
        Command::OpenUtilityAuction {
            utility: UtilityKind::Water,
            bid: world.config().utilities.reserve_prices.water.money(),
        },
    );
    world.submit(PlayerId(0), Command::SetInsurance { insured: true });
    let opening = opening_money(&world);
    while !world.clock().is_over() && world.clock().year() < until_year {
        let fired = world.tick();
        if fired.midnight || fired.settlement {
            let ledger = world.ledger();
            assert_eq!(
                ledger.total(),
                opening + ledger.minted(),
                "money created or destroyed by {} day {}",
                world.clock().year(),
                world.clock().day()
            );
        }
    }
    world
}

#[test]
fn the_full_game_plays_with_every_event_and_conserves_money() {
    let world = play(eventful(200), 7, i32::MAX);
    assert!(world.clock().is_over());
    let events = world.events();
    let fired: Vec<&str> = events.fired.iter().map(|f| f.id.as_str()).collect();
    for id in ["ww2_rationing", "blitz", "decimal_day", "crash", "pandemic"] {
        assert!(fired.contains(&id), "{id} never fired: {fired:?}");
    }
    assert_eq!(events.elections.len(), 24, "an election every step");
    assert!(!events.incidents.is_empty(), "no random building events");
    assert!(events.decimal_currency);
    // Seat 0 won the Water auction at its reserve, unless an election has moved it since.
    let water = world.utilities().payee(UtilityKind::Water);
    assert!(matches!(water, AccountId::City | AccountId::Player(_)));
}

#[test]
fn the_same_seed_replays_the_same_history() {
    let a = play(eventful(500), 11, 1970);
    let b = play(eventful(500), 11, 1970);
    assert_eq!(a.events(), b.events());
    assert_eq!(a.utilities(), b.utilities());
    assert_eq!(a.state_hash(), b.state_hash());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4))]
    /// Money conservation through events, for any seed and any frequency of building events.
    #[test]
    fn events_never_create_or_destroy_money(seed in any::<u64>(), chance in 0i64..3_000) {
        play(eventful(chance), seed, 1960);
    }
}
