//! Steps for `features/events/` and `features/utilities/`: elections, history, random building
//! events, insurance and utility ownership, driven through `World::tick` on the economy steps'
//! game (`steps::economy`), so the tick order is under test too.

use cucumber::{given, then, when};
use sim_core::World;
use sim_core::auctions::{AuctionError, AuctionItem, AuctionPhase};
use sim_core::economy::AccountId;
use sim_core::events::{Modifier, Party};
use sim_core::land::Tier;
use sim_core::money::Rate;
use sim_core::population::Class;
use sim_core::utilities::UtilityKind;
use sim_core::world::Command;

use super::economy::{
    account, block_id, business, business_id, money, owns_building, player, rebaseline, world,
};
use crate::SimWorld;

fn utility(name: &str) -> UtilityKind {
    UtilityKind::from_name(name).unwrap_or_else(|| panic!("no utility called {name}"))
}

fn class(name: &str) -> Class {
    match name {
        "working" => Class::Working,
        "middle" => Class::Middle,
        "upper" => Class::Upper,
        other => panic!("no class {other}"),
    }
}

/// Ticks until `done`, at most `days` game days.
fn tick_until(world: &mut World, days: u32, mut done: impl FnMut(&World) -> bool) {
    let limit = u64::from(days) * u64::from(world.clock().rules().ticks_per_day());
    for _ in 0..limit {
        world.tick();
        if done(world) {
            return;
        }
    }
    panic!("condition not reached within {days} days");
}

// ---- Given ----------------------------------------------------------------------------------

#[given(expr = "player {int} holds the {string} utility bought for {string}")]
fn holds_utility(w: &mut SimWorld, n: u8, name: String, price: String) {
    let holder = AccountId::Player(player(n));
    world(w)
        .utilities_mut()
        .grant(utility(&name), holder, money(&price));
}

#[given(expr = "the {string} on {string} has room for {int} of each good")]
fn has_room(w: &mut SimWorld, type_id: String, block: String, units: i64) {
    business(w, &type_id, &block).capacity_per_good = units;
}

#[given(expr = "the {string} on {string} keeps a standing order of {int} {string}")]
fn standing_order(w: &mut SimWorld, type_id: String, block: String, units: i64, good: String) {
    let b = business(w, &type_id, &block);
    b.prices.entry(good.clone()).or_insert(Rate::ONE);
    b.standing_order.insert(good, units);
}

#[given(expr = "player {int} takes out insurance")]
fn takes_out_insurance(w: &mut SimWorld, n: u8) {
    let world = world(w);
    world.submit(player(n), Command::SetInsurance { insured: true });
    world.tick();
    rebaseline(w);
}

#[given(expr = "player {int} has a bakery selling bread on each of {string}")]
fn bakeries_on(w: &mut SimWorld, n: u8, blocks: String) {
    for block in blocks.split(',').map(str::trim) {
        owns_building(
            w,
            format!("player {n}"),
            10,
            2,
            2,
            "bakery".into(),
            block.to_owned(),
        );
        standing_order(w, "bakery".into(), block.to_owned(), 300, "bread".into());
    }
}

// ---- When -----------------------------------------------------------------------------------

#[when(expr = "player {int} opens an auction on the {string} utility with a bid of {string}")]
fn opens_utility_auction(w: &mut SimWorld, n: u8, name: String, bid: String) {
    let world = world(w);
    world.submit(
        player(n),
        Command::OpenUtilityAuction {
            utility: utility(&name),
            bid: money(&bid),
        },
    );
    world.tick();
}

#[when(expr = "player {int} offers the {string} utility for auction")]
fn offers_utility(w: &mut SimWorld, n: u8, name: String) {
    let world = world(w);
    world.submit(
        player(n),
        Command::OfferForAuction {
            item: AuctionItem::Utility(utility(&name)),
        },
    );
    world.tick();
}

#[when(expr = "player {int} raises the utility bid by {string}")]
fn raises_utility_bid(w: &mut SimWorld, n: u8, by: String) {
    let me = player(n);
    let world = world(w);
    tick_until(world, 1, |world| {
        world
            .auctions()
            .current()
            .is_some_and(|a| matches!(a.phase, AuctionPhase::Bidding { turn, .. } if turn == me))
    });
    world.submit(me, Command::Raise { by: money(&by) });
    world.tick();
}

#[when("everyone else passes until the auction closes")]
fn others_pass(w: &mut SimWorld) {
    let world = world(w);
    let limit = world.clock().rules().ticks_per_day();
    let item = world.auctions().current().map(|a| a.item);
    for _ in 0..limit {
        let Some(auction) = world.auctions().current().filter(|a| Some(a.item) == item) else {
            return;
        };
        if let AuctionPhase::Bidding { turn, .. } = auction.phase
            && auction.leader != Some(turn)
        {
            world.submit(turn, Command::Pass);
        }
        world.tick();
    }
    panic!("the auction did not close within a day");
}

#[when(expr = "two games with seed {int} are played to {int}")]
fn two_games(w: &mut SimWorld, seed: u64, year: i32) {
    let mut config = w.config.load_ok();
    config.ai.opponents.clear();
    // Make every random thing likely, so the comparison means something.
    for event in &mut config.events.random {
        event.daily_chance = sim_core::config::types::Ratio(Rate::from_bp(5_000));
    }
    let mut games = [
        World::new(config.clone(), seed, 3),
        World::new(config, seed, 3),
    ];
    for g in &mut games {
        // Something to burn, rob and inspect.
        let me = AccountId::Player(sim_core::economy::PlayerId(0));
        g.businesses_mut()
            .by_id
            .values_mut()
            .for_each(|b| b.owner = me);
        tick_until(g, 40, |g| g.clock().year() >= year);
    }
    w.events.games = games.into_iter().collect();
}

// ---- Then -----------------------------------------------------------------------------------

#[then(
    regex = r"^the auction house refused player (\d+) as (below the minimum|not for sale|busy)$"
)]
fn refused(w: &mut SimWorld, n: u8, why: String) {
    let refusal = world(w)
        .auctions()
        .last_refusal()
        .expect("nothing was refused");
    assert_eq!(refusal.player, player(n));
    let ok = match why.as_str() {
        "below the minimum" => matches!(refusal.error, AuctionError::BelowMinimum { .. }),
        "not for sale" => refusal.error == AuctionError::NotForSale,
        _ => refusal.error == AuctionError::Busy,
    };
    assert!(ok, "refused for {:?}", refusal.error);
}

#[then(expr = "player {int} holds the {string} utility")]
fn holds_utility_now(w: &mut SimWorld, n: u8, name: String) {
    let payee = world(w).utilities().payee(utility(&name));
    assert_eq!(payee, AccountId::Player(player(n)));
}

#[then(expr = "nobody holds the {string} utility")]
fn nobody_holds(w: &mut SimWorld, name: String) {
    assert_eq!(world(w).utilities().payee(utility(&name)), AccountId::City);
}

#[then(expr = "the {string} utility is nationalised")]
fn is_nationalised(w: &mut SimWorld, name: String) {
    let u = world(w).utilities();
    let kind = utility(&name);
    assert!(u.nationalised.contains(&kind), "{name} is not nationalised");
    assert_eq!(u.payee(kind), AccountId::City);
}

#[then(regex = r#"^the "([^"]+)" utility is up for auction, sold by (the city|player \d+)$"#)]
fn up_for_auction(w: &mut SimWorld, name: String, seller: String) {
    let auction = world(w).auctions().current().expect("no auction is on");
    assert_eq!(auction.item, AuctionItem::Utility(utility(&name)));
    assert_eq!(auction.seller, account(&seller));
}

#[then(expr = "the {string} utility is waiting for auction")]
fn waiting_for_auction(w: &mut SimWorld, name: String) {
    let item = AuctionItem::Utility(utility(&name));
    assert!(
        world(w).auctions().queue().iter().any(|s| s.item == item),
        "{name} is not queued"
    );
}

#[then(expr = "the {string} utility is called {string}")]
fn utility_called(w: &mut SimWorld, name: String, called: String) {
    let world = world(w);
    let upgrade = world.config().utilities.internet.upgrade_year;
    assert_eq!(utility(&name).name(world.clock().year(), upgrade), called);
}

#[then(regex = r"^(Labour|Conservative) won the (\d+) election$")]
fn won_election(w: &mut SimWorld, party: String, year: i32) {
    let party = if party == "Labour" {
        Party::Labour
    } else {
        Party::Conservative
    };
    let held = &world(w).events().elections;
    assert!(held.contains(&(year, party)), "elections: {held:?}");
}

#[then(expr = "the front page for {int} reads {string}")]
fn front_page_reads(w: &mut SimWorld, year: i32, headline: String) {
    let page = &world(w).events().front_page;
    assert_eq!(page.year, year);
    assert!(page.headlines.contains(&headline), "{:?}", page.headlines);
}

#[then(expr = "the front page for {int} carries one of the random 2030+ headlines")]
fn random_era_headline(w: &mut SimWorld, year: i32) {
    let world = world(w);
    let pool: Vec<&String> = world
        .config()
        .events
        .random_era
        .events
        .iter()
        .map(|e| &e.headline)
        .collect();
    let page = &world.events().front_page;
    assert_eq!(page.year, year);
    assert!(
        page.headlines.iter().any(|h| pool.contains(&h)),
        "{:?}",
        page.headlines
    );
}

#[then("the currency shows decimal")]
fn decimal(w: &mut SimWorld) {
    assert!(world(w).events().decimal_currency);
}

#[then("the currency shows pounds, shillings and pence")]
fn lsd(w: &mut SimWorld) {
    assert!(!world(w).events().decimal_currency);
}

/// Every (shop, good) on sale and its price factor today.
fn shelf_factors(world: &World) -> Vec<Rate> {
    let modifiers = world.events().modifiers();
    world
        .businesses()
        .by_id
        .iter()
        .flat_map(|(id, b)| b.prices.keys().map(move |g| modifiers.price_factor(*id, g)))
        .collect()
}

#[then("every shelf price is within 10% of usual and some are off")]
fn prices_noisy(w: &mut SimWorld) {
    let factors = shelf_factors(world(w));
    assert_ne!(factors, Vec::new(), "nothing is on sale");
    assert!(
        factors.iter().all(|r| (9_000..=11_000).contains(&r.bp())),
        "{factors:?}"
    );
    assert!(factors.iter().any(|r| *r != Rate::ONE), "{factors:?}");
}

#[then("every shelf price is back to usual")]
fn prices_usual(w: &mut SimWorld) {
    assert!(shelf_factors(world(w)).iter().all(|r| *r == Rate::ONE));
}

#[then(regex = r#"^demand for "(\w+)" by (working|middle|upper) shoppers is at (\d+)%$"#)]
fn demand_is(w: &mut SimWorld, good: String, who: String, percent: i64) {
    let factor = world(w)
        .events()
        .modifiers()
        .demand_factor(&good, class(&who));
    assert_eq!(factor, Rate::from_bp(percent * 100));
}

#[then(expr = "the {string} on {string} is gone")]
fn is_gone(w: &mut SimWorld, type_id: String, block: String) {
    let block = block_id(w, &block);
    let standing = world(w)
        .businesses()
        .by_id
        .values()
        .any(|b| b.type_id == type_id && b.block == block);
    assert!(!standing, "the {type_id} still stands");
}

#[then(expr = "the {string} on {string} still stands")]
fn still_stands(w: &mut SimWorld, type_id: String, block: String) {
    business_id(w, &type_id, &block);
}

#[then(expr = "the {string} on {string} is worth {string}")]
fn is_worth(w: &mut SimWorld, type_id: String, block: String, value: String) {
    assert_eq!(business(w, &type_id, &block).build_cost, money(&value));
}

#[then(expr = "{string} is a {word}-tier block")]
fn tier_is(w: &mut SimWorld, block: String, tier: String) {
    let id = block_id(w, &block);
    let expected = match tier.as_str() {
        "low" => Tier::Low,
        "medium" => Tier::Medium,
        _ => Tier::High,
    };
    assert_eq!(world(w).map().blocks[id.0 as usize].tier, expected);
}

#[then(expr = "the lease on {string} is valued at {string}")]
fn lease_valued(w: &mut SimWorld, block: String, value: String) {
    let id = block_id(w, &block);
    let lease = world(w).map().blocks[id.0 as usize].lease.clone();
    assert_eq!(lease.map(|l| l.last_price), Some(money(&value)));
}

#[then(expr = "shops on {string} draw {int}% of their usual footfall")]
fn footfall_is(w: &mut SimWorld, block: String, percent: i64) {
    let id = block_id(w, &block);
    let factor = world(w).events().modifiers().footfall(id);
    assert_eq!(factor, Rate::from_bp(percent * 100));
}

/// The bakeries on the map and whether the open-days rule lets each trade today.
fn bakeries_allowed(world: &World) -> Vec<(sim_core::businesses::BusinessId, bool)> {
    let day = world.clock().day();
    let modifiers = world.events().modifiers();
    assert!(
        modifiers
            .active()
            .iter()
            .any(|m| matches!(m.modifier, Modifier::OpenDays { .. })),
        "no open-days rule in force"
    );
    world
        .businesses()
        .by_id
        .iter()
        .filter(|(_, b)| b.type_id == "bakery")
        .map(|(id, _)| (*id, modifiers.may_trade(*id, day)))
        .collect()
}

#[then(expr = "{int} of the {int} bakeries may open today")]
fn bakeries_may_open(w: &mut SimWorld, open: usize, total: usize) {
    let allowed = bakeries_allowed(world(w));
    assert_eq!(allowed.len(), total);
    assert_eq!(allowed.iter().filter(|(_, ok)| *ok).count(), open);
}

#[then("only the bakeries allowed to open sold bread today")]
fn only_allowed_sold(w: &mut SimWorld) {
    let world = world(w);
    let allowed = bakeries_allowed(world);
    let sold = |id| {
        world
            .businesses()
            .get(id)
            .and_then(|b| b.sold_today.get("bread").copied())
            .unwrap_or(0)
    };
    for (id, ok) in &allowed {
        if !ok {
            assert_eq!(sold(*id), 0, "shut bakery {id:?} sold bread");
        }
    }
    assert!(
        allowed.iter().any(|(id, ok)| *ok && sold(*id) > 0),
        "no open bakery sold anything"
    );
}

#[then("both games held the same elections, events and incidents")]
fn games_match(w: &mut SimWorld) {
    let [a, b] = &w.events.games[..] else {
        panic!("play two games first");
    };
    let (ea, eb) = (a.events(), b.events());
    assert!(!ea.elections.is_empty(), "no elections were held");
    assert!(!ea.incidents.is_empty(), "nothing happened to any building");
    assert_eq!(ea.elections, eb.elections);
    assert_eq!(ea.fired, eb.fired);
    assert_eq!(ea.incidents, eb.incidents);
    assert_eq!(a.state_hash(), b.state_hash());
    let ledger = a.ledger();
    assert_eq!(
        ledger.total() - ledger.minted(),
        b.ledger().total() - b.ledger().minted()
    );
}

/// Two games for the determinism scenario.
#[derive(Debug, Default)]
pub struct EventsState {
    games: Vec<World>,
}
