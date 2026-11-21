//! Steps for `features/ai/`: a whole [`World`] with the Grocer in seat 1, driven tick by tick
//! through public commands only, and a scripted rival in seat 0.
//!
//! Each tick the rival moves first (if it is its turn in an auction), then the Grocer decides
//! from the world's public state, then the world ticks. Everything the Grocer issues is
//! recorded with what it could see at the time, so scenarios can check its discipline.

use cucumber::{given, then, when};
use sim_core::ai::{Brain, Grocer, seat_rng};
use sim_core::auctions::{AuctionItem, AuctionPhase};
use sim_core::businesses::{BusinessId, Hours};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{BlockId, Tier};
use sim_core::rng::SimRng;
use sim_core::world::{Command, PlayerStatus};
use sim_core::{Money, World};

use crate::SimWorld;
use crate::common;
use crate::steps::land::block_named;

const RIVAL: PlayerId = PlayerId(0);
const ME: PlayerId = PlayerId(1);
const MY_ACCOUNT: AccountId = AccountId::Player(ME);

/// What the rival does on its auction turns.
#[derive(Debug, Clone, Copy, Default)]
enum Rival {
    /// Passes.
    #[default]
    Pass,
    /// Raises by this much while it can.
    Raise(Money),
}

/// One command the Grocer issued, and what it could see then.
#[derive(Debug, Clone)]
struct Issued {
    command: Command,
    /// The item on auction, if any.
    on_auction: Option<AuctionItem>,
    /// The high bid at the time (the opening bid for an opening).
    high_bid: Money,
    /// Its bid ceiling, `balance + credit limit` (R7).
    ceiling: Money,
    /// Capacity of the shop a `SetOrder` names.
    capacity: Option<i64>,
}

/// The game, the Grocer and what happened.
#[derive(Debug, Default)]
pub struct AiState {
    game: Option<World>,
    grocer: Option<(Grocer, SimRng)>,
    rival: Rival,
    issued: Vec<Issued>,
    /// Balance after each tick on which it gained a lot or a shop.
    after_buying: Vec<Money>,
    /// Most strikes it ever had.
    max_strikes: u32,
    /// Shops it owned at the last check, for the rescue scenario.
    shops_before: Option<usize>,
    /// State hashes of the determinism games.
    hashes: Vec<u64>,
    /// Grocer shop counts in the determinism games.
    shop_counts: Vec<usize>,
}

fn game(w: &mut SimWorld) -> &mut World {
    if w.ai.game.is_none() {
        // The Grocer is driven by these steps, so no AI seat plays inside the world.
        common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[]");
        let world = World::new(w.config.load_ok(), 42, 2);
        w.ai.grocer = Some((Grocer::new("grocer"), seat_rng(42, ME)));
        w.ai.game = Some(world);
    }
    w.ai.game.as_mut().unwrap()
}

fn pounds(text: &str) -> Money {
    let negative = text.starts_with('-');
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let m = Money::from_pounds(digits.parse().unwrap());
    if negative { -m } else { m }
}

fn shops(world: &World) -> usize {
    world
        .businesses()
        .owned_by(MY_ACCOUNT)
        .filter(|(_, b)| b.hours != Hours::None)
        .count()
}

fn leases(world: &World) -> usize {
    world
        .map()
        .blocks
        .iter()
        .filter(|b| b.lease.as_ref().is_some_and(|l| l.holder == MY_ACCOUNT))
        .count()
}

/// One tick: the rival's turn, the Grocer's decision, then the world.
fn advance(w: &mut SimWorld) {
    game(w);
    let state = &mut w.ai;
    let world = state.game.as_mut().unwrap();
    if let Some(a) = world.auctions().current()
        && let AuctionPhase::Bidding { turn, .. } = a.phase
        && turn == RIVAL
    {
        let command = match state.rival {
            Rival::Raise(by) if world.can_afford(RIVAL, a.high_bid + by) => Command::Raise { by },
            _ => Command::Pass,
        };
        world.submit(RIVAL, command);
    }
    let (grocer, rng) = state.grocer.as_mut().unwrap();
    for command in grocer.decide(world, ME, rng) {
        let current = world.auctions().current();
        let capacity = match &command {
            Command::SetOrder { business, .. } => world
                .businesses()
                .get(*business)
                .map(|b| b.capacity_per_good),
            _ => None,
        };
        let high_bid = match &command {
            Command::OpenLandAuction { bid, .. } => *bid,
            _ => current.map_or(Money::ZERO, |a| a.high_bid),
        };
        state.issued.push(Issued {
            on_auction: current.map(|a| a.item),
            high_bid,
            ceiling: world.bid_ceiling(ME).unwrap_or(Money::ZERO),
            capacity,
            command: command.clone(),
        });
        world.submit(ME, command);
    }
    let before = (shops(world), leases(world));
    world.tick();
    if shops(world) > before.0 || leases(world) > before.1 {
        state.after_buying.push(world.ledger().balance(MY_ACCOUNT));
    }
    state.max_strikes = state.max_strikes.max(world.strikes(ME));
}

fn play_ticks(w: &mut SimWorld, ticks: u64) {
    for _ in 0..ticks {
        advance(w);
    }
}

fn ticks_per_hour(w: &mut SimWorld) -> u64 {
    u64::from(game(w).clock().rules().ticks_per_day() / 24)
}

fn cash_buffer(w: &mut SimWorld) -> Money {
    game(w).config().ai.grocer.cash_buffer.money()
}

fn block_of(item: Option<AuctionItem>) -> Option<BlockId> {
    match item {
        Some(AuctionItem::Land(b)) => Some(b),
        _ => None,
    }
}

fn holder(world: &World, name: &str) -> Option<AccountId> {
    let block = block_named(world, name);
    world.map().block(block)?.lease.as_ref().map(|l| l.holder)
}

// --- Given -----------------------------------------------------------------------------------

#[given("nobody comes shopping")]
fn nobody_shops(w: &mut SimWorld) {
    let sources = w.config.sources_mut();
    common::set_key(sources, "customers.yaml", "/initial_population", "0");
    common::set_key(
        sources,
        "economy.yaml",
        "/population/migration_per_day",
        "0",
    );
}

#[given(expr = "the {string} game length")]
fn game_length(w: &mut SimWorld, preset: String) {
    assert_eq!(preset, "v1", "only the v1 preset is wired here");
    common::set_key(
        w.config.sources_mut(),
        "game.yaml",
        "/clock/end_year",
        "1950",
    );
}

#[given("the rival holds the lease on every Low-tier block")]
fn rival_holds_all_low(w: &mut SimWorld) {
    rival_holds_low(w, String::new());
}

#[given(regex = r#"^the rival holds the lease on every Low-tier block except "(.+)"$"#)]
fn rival_holds_low(w: &mut SimWorld, except: String) {
    let world = game(w);
    let year = world.clock().year();
    let term = world.config().economy.land.lease_term_years;
    for block in &mut world.map_mut().blocks {
        if block.tier == Tier::Low && block.name != except {
            block.lease = Some(sim_core::land::Lease::new(
                AccountId::Player(RIVAL),
                year,
                term,
                block.land_base_price,
            ));
        }
    }
}

// --- When ------------------------------------------------------------------------------------

#[when(regex = r"^the Grocer plays for (\d+) game (hour|hours|day|days)$")]
fn plays_for(w: &mut SimWorld, n: u64, unit: String) {
    let per = if unit.starts_with("hour") { 1 } else { 24 };
    let ticks = n * per * ticks_per_hour(w);
    play_ticks(w, ticks);
}

#[when(regex = r"^the Grocer plays until (\d\d):(\d\d) on day (\d+)$")]
fn plays_until(w: &mut SimWorld, h: u32, m: u32, day: u32) {
    let second = h * 3600 + m * 60;
    loop {
        let clock = game(w).clock();
        if clock.day() == day && clock.second_of_day() == second {
            break;
        }
        assert!(
            clock.day() <= day,
            "already past {h:02}:{m:02} on day {day}"
        );
        advance(w);
    }
}

#[when("the Grocer plays the whole game")]
fn plays_whole_game(w: &mut SimWorld) {
    while !game(w).clock().is_over() {
        advance(w);
    }
}

#[when(regex = r#"^the rival opens an auction on "(.+)" at (£[\d,]+)$"#)]
fn rival_opens(w: &mut SimWorld, name: String, bid: String) {
    let world = game(w);
    let block = block_named(world, &name);
    world.submit(
        RIVAL,
        Command::OpenLandAuction {
            block,
            bid: pounds(&bid),
        },
    );
    advance(w);
}

#[when("the auction runs to the close with the rival passing")]
fn auction_runs_passing(w: &mut SimWorld) {
    auction_runs(w, Rival::Pass);
}

#[when(regex = r"^the auction runs to the close with the rival raising by (£[\d,]+) every turn$")]
fn auction_runs_raising(w: &mut SimWorld, by: String) {
    auction_runs(w, Rival::Raise(pounds(&by)));
}

fn auction_runs(w: &mut SimWorld, rival: Rival) {
    w.ai.rival = rival;
    let limit = 10 * ticks_per_hour(w);
    let mut ticks = 0;
    while game(w).auctions().is_busy() {
        advance(w);
        ticks += 1;
        assert!(ticks < limit, "the auction never closed");
    }
    w.ai.rival = Rival::Pass;
}

#[when(regex = r"^the Grocer's first shop had (\d+) delivered this morning and has sold (\d+)$")]
fn first_shop_sales(w: &mut SimWorld, delivered: i64, sold: i64) {
    let world = game(w);
    let id = first_shop(world);
    let shop = world.businesses_mut().by_id.get_mut(&id).unwrap();
    let good = shop.prices.keys().next().unwrap().clone();
    shop.tomorrow_order.insert(good.clone(), delivered);
    shop.sold_today.insert(good.clone(), sold);
    shop.stock.insert(good, delivered - sold);
}

#[when(
    regex = r"^the Grocer's first shop had (\d+) delivered this morning, sold (\d+) and turned away (\d+) shoppers$"
)]
fn first_shop_turned_away(w: &mut SimWorld, delivered: i64, sold: i64, lost: u32) {
    first_shop_sales(w, delivered, sold);
    let world = game(w);
    let id = first_shop(world);
    world
        .businesses_mut()
        .by_id
        .get_mut(&id)
        .unwrap()
        .lost_sales_today = lost;
}

#[when(regex = r"^the Grocer's balance drops to (-?£[\d,]+)$")]
fn balance_drops(w: &mut SimWorld, to: String) {
    let world = game(w);
    let target = pounds(&to);
    let now = world.ledger().balance(MY_ACCOUNT);
    world
        .ledger_mut()
        .transfer(MY_ACCOUNT, AccountId::City, now - target);
    let count = shops(world);
    w.ai.shops_before = Some(count);
}

#[when(
    regex = r"^two games with seed (\d+) are played to the whistle with ai.yaml's Grocer in seat 1$"
)]
fn two_games(w: &mut SimWorld, seed: u64) {
    // The Grocer alone in seat 1: pin the opponents, whatever ai.yaml ships.
    common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[grocer]");
    let config = w.config.load_ok();
    assert_eq!(config.ai.opponents, vec!["grocer".to_owned()]);
    for _ in 0..2 {
        let mut world = World::new(config.clone(), seed, 2);
        assert_eq!(world.ai_seats()[0].player, ME);
        while !world.clock().is_over() {
            world.tick();
        }
        w.ai.hashes.push(world.state_hash());
        w.ai.shop_counts.push(shops(&world));
    }
}

fn first_shop(world: &World) -> BusinessId {
    world
        .businesses()
        .owned_by(MY_ACCOUNT)
        .find(|(_, b)| b.hours != Hours::None)
        .map(|(id, _)| id)
        .expect("the Grocer owns no shop")
}

// --- Then ------------------------------------------------------------------------------------

#[then(regex = r#"^the Grocer opened an auction on "(.+)" at (£[\d,]+)$"#)]
fn opened_on(w: &mut SimWorld, name: String, bid: String) {
    let block = block_named(game(w), &name);
    let want = pounds(&bid);
    assert!(
        w.ai.issued
            .iter()
            .any(|i| i.command == Command::OpenLandAuction { block, bid: want }),
        "{:?}",
        w.ai.issued
    );
}

#[then("the Grocer opened no auction")]
fn opened_none(w: &mut SimWorld) {
    let opened: Vec<_> =
        w.ai.issued
            .iter()
            .filter(|i| matches!(i.command, Command::OpenLandAuction { .. }))
            .collect();
    assert!(opened.is_empty(), "{opened:?}");
}

#[then(regex = r#"^the Grocer (raised|passed) on "(.+)"$"#)]
fn moved_on(w: &mut SimWorld, how: String, name: String) {
    let block = block_named(game(w), &name);
    let found = w.ai.issued.iter().any(|i| {
        block_of(i.on_auction) == Some(block)
            && match how.as_str() {
                "raised" => matches!(i.command, Command::Raise { .. }),
                _ => i.command == Command::Pass,
            }
    });
    assert!(found, "no {how} on {name}: {:?}", w.ai.issued);
}

#[then(regex = r#"^the Grocer never bid on "(.+)"$"#)]
fn never_bid(w: &mut SimWorld, name: String) {
    let block = block_named(game(w), &name);
    let bids: Vec<_> =
        w.ai.issued
            .iter()
            .filter(|i| match &i.command {
                Command::Raise { .. } => block_of(i.on_auction) == Some(block),
                Command::OpenLandAuction { block: b, .. } => *b == block,
                _ => false,
            })
            .collect();
    assert!(bids.is_empty(), "{bids:?}");
}

#[then(regex = r"^every bid the Grocer made was at most (\d+)% of its bid ceiling at the time$")]
fn bids_within_share(w: &mut SimWorld, percent: i64) {
    let share = sim_core::Rate::from_bp(percent * 100);
    assert_eq!(
        game(w).config().ai.grocer.max_bid_share.rate(),
        share,
        "the scenario assumes the shipped max_bid_share"
    );
    let mut bids = 0;
    for i in &w.ai.issued {
        let bid = match &i.command {
            Command::Raise { by } => i.high_bid + *by,
            Command::OpenLandAuction { bid, .. } => *bid,
            _ => continue,
        };
        bids += 1;
        assert!(
            bid <= share.apply(i.ceiling),
            "bid {bid} over {percent}% of ceiling {}",
            i.ceiling
        );
    }
    assert!(bids > 0, "the Grocer never bid");
}

#[then(regex = r#"^the (rival|Grocer) holds the lease on "(.+)"$"#)]
fn who_holds(w: &mut SimWorld, who: String, name: String) {
    let want = if who == "rival" {
        AccountId::Player(RIVAL)
    } else {
        MY_ACCOUNT
    };
    assert_eq!(holder(game(w), &name), Some(want));
}

#[then(regex = r"^the Grocer owns at least (\d+) shops$")]
fn owns_shops_anywhere(w: &mut SimWorld, n: usize) {
    owns_shops(w, n, String::new());
}

#[then(regex = r#"^the Grocer owns at least (\d+) shops on "(.+)"$"#)]
fn owns_shops(w: &mut SimWorld, n: usize, name: String) {
    let world = game(w);
    let on = (!name.is_empty()).then(|| block_named(world, &name));
    let count = world
        .businesses()
        .owned_by(MY_ACCOUNT)
        .filter(|(_, b)| on.is_none_or(|block| b.block == block))
        .count();
    assert!(count >= n, "{count} shops");
}

#[then("every Grocer shop is a day-trade type from its shop list")]
fn shops_are_day_trade(w: &mut SimWorld) {
    let world = game(w);
    let types = &world.config().ai.grocer.shop_types;
    for (id, b) in world.businesses().owned_by(MY_ACCOUNT) {
        assert!(types.contains(&b.type_id), "{id:?} is a {}", b.type_id);
        assert_eq!(b.hours, Hours::Day, "{id:?}");
    }
}

#[then("every Grocer shop touches the lot edge and overlaps no other building")]
fn shops_are_placed_legally(w: &mut SimWorld) {
    let world = game(w);
    let [lw, ld] = world.config().game.world.lot_size;
    for (id, b) in world.businesses().owned_by(MY_ACCOUNT) {
        assert!(b.footprint.touches_edge(lw, ld), "{id:?} has no frontage");
        for (other, o) in world.businesses().on_block(b.block) {
            assert!(
                other == id || !o.footprint.overlaps(&b.footprint),
                "{id:?} overlaps {other:?}"
            );
        }
    }
}

#[then("every Grocer shop is no taller than its type's max_height")]
fn shops_within_height(w: &mut SimWorld) {
    let world = game(w);
    for (id, b) in world.businesses().owned_by(MY_ACCOUNT) {
        let max = world.config().businesses.types[&b.type_id].max_height;
        assert!(b.height <= max, "{id:?} is {} high, max {max}", b.height);
    }
}

#[then("the Grocer's balance is at least its cash buffer")]
fn balance_over_buffer(w: &mut SimWorld) {
    let buffer = cash_buffer(w);
    let balance = game(w).ledger().balance(MY_ACCOUNT);
    assert!(balance >= buffer, "balance {balance} under buffer {buffer}");
}

#[then(regex = r"^the Grocer's first shop's order for tomorrow is (\d+)$")]
fn first_shop_order(w: &mut SimWorld, order: i64) {
    let world = game(w);
    let shop = world.businesses().get(first_shop(world)).unwrap();
    let good = shop.prices.keys().next().unwrap();
    assert_eq!(shop.standing_order[good], order);
}

#[then("every order the Grocer set was within its shop's capacity")]
fn orders_within_capacity(w: &mut SimWorld) {
    let mut orders = 0;
    for i in &w.ai.issued {
        if let Command::SetOrder { quantity, .. } = i.command {
            orders += 1;
            let cap = i.capacity.expect("an order for a shop it can see");
            assert!((0..=cap).contains(&quantity), "{quantity} over {cap}");
        }
    }
    assert!(orders > 0, "the Grocer never ordered");
}

#[then("the Grocer had at least its cash buffer after every lot and shop it bought")]
fn kept_buffer(w: &mut SimWorld) {
    let buffer = cash_buffer(w);
    assert!(!w.ai.after_buying.is_empty(), "the Grocer bought nothing");
    for balance in &w.ai.after_buying {
        assert!(*balance >= buffer, "{balance} under {buffer}");
    }
}

#[then("the Grocer never took a strike")]
fn no_strikes(w: &mut SimWorld) {
    assert_eq!(w.ai.max_strikes, 0);
}

#[then("the Grocer is not bankrupt")]
fn not_bankrupt(w: &mut SimWorld) {
    let world = game(w);
    assert!(
        world
            .players()
            .iter()
            .any(|(p, s)| *p == ME && *s == PlayerStatus::Active)
    );
}

#[then("the Grocer sold a shop back to the city")]
fn sold_back(w: &mut SimWorld) {
    assert!(
        w.ai.issued
            .iter()
            .any(|i| matches!(i.command, Command::SellToCity { .. })),
        "{:?}",
        w.ai.issued
    );
    let before = w.ai.shops_before.expect("no balance drop recorded");
    let now = shops(game(w));
    assert!(now < before, "{now} shops, was {before}");
}

#[then("both games end with the same state hash")]
fn same_hash(w: &mut SimWorld) {
    assert_eq!(w.ai.hashes.len(), 2);
    assert_eq!(w.ai.hashes[0], w.ai.hashes[1]);
}

#[then("the Grocer owns shops in both games")]
fn shops_in_both(w: &mut SimWorld) {
    assert_eq!(w.ai.shop_counts.len(), 2);
    assert!(
        w.ai.shop_counts.iter().all(|n| *n > 0),
        "{:?}",
        w.ai.shop_counts
    );
}
