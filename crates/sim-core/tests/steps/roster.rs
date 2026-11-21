//! Steps for the AI roster (`features/ai/{landlord,toff,speculator,sir_penny_sugar,roster}`):
//! a whole [`World`] whose AI seats are driven here, tick by tick, through public commands
//! only, so every command can be recorded with what the AI could see when it sent it.
//!
//! Two set-ups:
//!
//! - **One personality** in seat 1 against a scripted rival in seat 0, which passes on every
//!   auction turn and otherwise does only what a scenario tells it to.
//! - **The full roster**: seat 0 idle, seats 1–5 the Grocer, the Landlord, the Toff, the
//!   Speculator and Sir Penny Sugar, each on its own seat RNG stream ([`AiSeat`]).

use std::collections::{BTreeMap, BTreeSet};

use cucumber::{given, then, when};
use sim_core::ai::{AiSeat, kit};
use sim_core::auctions::{AuctionItem, AuctionPhase};
use sim_core::businesses::{BusinessId, FitOutLevel, Hours};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{BlockId, Lease, Tier};
use sim_core::world::{Command, PlayerStatus};
use sim_core::{Config, Money, Rate, World};

use crate::SimWorld;
use crate::common;
use crate::steps::land::block_named;

const RIVAL: PlayerId = PlayerId(0);
const FOCUS: PlayerId = PlayerId(1);

/// Seats 1–5 of the full roster, in order.
const ROSTER: [&str; 5] = [
    "grocer",
    "landlord",
    "toff",
    "speculator",
    "sir_penny_sugar",
];

/// A personality's id from how a scenario names it.
fn id_of(who: &str) -> &'static str {
    match who {
        "the Grocer" => "grocer",
        "the Landlord" => "landlord",
        "the Toff" => "toff",
        "the Speculator" => "speculator",
        "Sir Penny Sugar" => "sir_penny_sugar",
        other => panic!("no personality called {other:?}"),
    }
}

/// One command an AI sent, and what it could see then.
#[derive(Debug, Clone)]
struct Issued {
    player: PlayerId,
    command: Command,
    /// The item on auction, if any.
    on_auction: Option<AuctionItem>,
    /// The high bid at the time (the opening bid for an opening).
    high_bid: Money,
    /// Its bid ceiling, `balance + credit limit` (R7).
    ceiling: Money,
    /// Book value of the lease on auction, if land is on auction and leased (§3).
    book_value: Option<Money>,
}

/// A building that went up.
#[derive(Debug, Clone)]
struct Built {
    owner: AccountId,
    id: BusinessId,
    type_id: String,
    year: i32,
}

/// The game, its driven seats and what happened.
#[derive(Debug, Default)]
pub struct RosterState {
    game: Option<Box<World>>,
    /// The personality in seat 1 when one plays against the rival.
    focus: Option<&'static str>,
    seats: Vec<AiSeat>,
    issued: Vec<Issued>,
    built: Vec<Built>,
    /// (player, block) for every lease a player gained.
    won: Vec<(PlayerId, BlockId)>,
    /// Balance after each tick on which a player gained a lease.
    after_lease: Vec<(PlayerId, Money)>,
    /// Balance after each tick on which a player gained a building.
    after_building: Vec<(PlayerId, Money)>,
    hashes: Vec<u64>,
    holdings: Vec<(usize, usize)>,
    /// (player, building) for every existing building a player gained (bought at auction).
    bought: Vec<(PlayerId, BusinessId)>,
}

fn state(w: &mut SimWorld) -> &mut RosterState {
    &mut w.roster
}

/// The one-personality game: seat 0 the rival, seat 1 the focus. No AI seat plays inside the
/// world; the seats are driven here.
fn game(w: &mut SimWorld) -> &mut World {
    if w.roster.game.is_none() {
        let focus = w.roster.focus.expect("no personality seated");
        common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[]");
        let world = World::new(w.config.load_ok(), 42, 2);
        w.roster.seats = vec![AiSeat::new(FOCUS, focus, 42)];
        w.roster.game = Some(Box::new(world));
    }
    w.roster.game.as_mut().unwrap()
}

fn pounds(text: &str) -> Money {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    Money::from_pounds(digits.parse().unwrap())
}

fn holders(world: &World) -> Vec<Option<AccountId>> {
    world
        .map()
        .blocks
        .iter()
        .map(|b| b.lease.as_ref().map(|l| l.holder))
        .collect()
}

/// One tick: the rival's auction turn (a pass), every driven seat's decision, then the world.
fn advance(st: &mut RosterState, rival: bool) {
    let world = st.game.as_mut().unwrap();
    if rival
        && let Some(a) = world.auctions().current()
        && let AuctionPhase::Bidding { turn, .. } = a.phase
        && turn == RIVAL
    {
        world.submit(RIVAL, Command::Pass);
    }
    for seat in &mut st.seats {
        if !world.is_active(seat.player) {
            continue;
        }
        let me = seat.player;
        for command in seat.play(world) {
            let current = world.auctions().current();
            let high_bid = match &command {
                Command::OpenLandAuction { bid, .. } => *bid,
                _ => current.map_or(Money::ZERO, |a| a.high_bid),
            };
            let book_value = current
                .and_then(kit::land_item)
                .and_then(|b| kit::lease_book_value(world, b));
            st.issued.push(Issued {
                player: me,
                on_auction: current.map(|a| a.item),
                high_bid,
                ceiling: world.bid_ceiling(me).unwrap_or(Money::ZERO),
                book_value,
                command: command.clone(),
            });
            world.submit(me, command);
        }
    }
    let before_ids: BTreeSet<BusinessId> = world.businesses().by_id.keys().copied().collect();
    let before_owners: BTreeMap<BusinessId, AccountId> = world
        .businesses()
        .by_id
        .iter()
        .map(|(id, b)| (*id, b.owner))
        .collect();
    let before_leases = holders(world);
    world.tick();
    let year = world.clock().year();
    let mut gained_building = BTreeSet::new();
    for (id, b) in &world.businesses().by_id {
        if !before_ids.contains(id) {
            st.built.push(Built {
                owner: b.owner,
                id: *id,
                type_id: b.type_id.clone(),
                year,
            });
            if let AccountId::Player(p) = b.owner {
                gained_building.insert(p);
            }
        }
    }
    for (id, b) in &world.businesses().by_id {
        if let (Some(before), AccountId::Player(p)) = (before_owners.get(id), b.owner)
            && *before != b.owner
        {
            st.bought.push((p, *id));
        }
    }
    let mut gained_lease = BTreeSet::new();
    for (i, (before, now)) in before_leases.iter().zip(holders(world)).enumerate() {
        if let Some(AccountId::Player(p)) = now
            && *before != now
        {
            st.won.push((p, BlockId(u32::try_from(i).unwrap())));
            gained_lease.insert(p);
        }
    }
    for p in gained_building {
        st.after_building
            .push((p, world.ledger().balance(AccountId::Player(p))));
    }
    for p in gained_lease {
        st.after_lease
            .push((p, world.ledger().balance(AccountId::Player(p))));
    }
}

fn play_ticks(w: &mut SimWorld, ticks: u64) {
    game(w);
    for _ in 0..ticks {
        advance(&mut w.roster, true);
    }
}

fn ticks_per_hour(w: &mut SimWorld) -> u64 {
    u64::from(game(w).clock().rules().ticks_per_day() / 24)
}

fn player_of(st: &RosterState, who: &str) -> PlayerId {
    let id = id_of(who);
    st.seats
        .iter()
        .find(|s| s.personality == id)
        .map_or_else(|| panic!("{who} is not seated"), |s| s.player)
}

fn aggression(config: &Config, id: &str) -> Rate {
    let ai = &config.ai;
    match id {
        "grocer" => ai.grocer.max_bid_share,
        "landlord" => ai.landlord.aggression,
        "toff" => ai.toff.aggression,
        "speculator" => ai.speculator.aggression,
        "sir_penny_sugar" => ai.sir_penny_sugar.aggression,
        other => panic!("{other}"),
    }
    .rate()
}

fn cash_buffer(config: &Config, id: &str) -> Money {
    let ai = &config.ai;
    match id {
        "grocer" => ai.grocer.cash_buffer,
        "landlord" => ai.landlord.cash_buffer,
        "toff" => ai.toff.cash_buffer,
        "speculator" => ai.speculator.cash_buffer,
        "sir_penny_sugar" => ai.sir_penny_sugar.cash_buffer,
        other => panic!("{other}"),
    }
    .money()
}

/// The bid a command makes, if it is one.
fn bid_of(i: &Issued) -> Option<(Money, Option<BlockId>)> {
    match &i.command {
        Command::Raise { by } => Some((
            i.high_bid + *by,
            match i.on_auction {
                Some(AuctionItem::Land(b)) => Some(b),
                _ => None,
            },
        )),
        Command::OpenLandAuction { bid, block } => Some((*bid, Some(*block))),
        Command::OpenBusinessAuction { bid, .. } => Some((*bid, None)),
        _ => None,
    }
}

fn has_station(world: &World, block: BlockId) -> bool {
    let key = &world.map().block(block).unwrap().key;
    let year = world.clock().year();
    world
        .config()
        .map
        .tube_stations
        .iter()
        .any(|s| &s.block == key && s.opens <= year)
}

// --- Given -----------------------------------------------------------------------------------

#[given(
    regex = r"^(the Grocer|the Landlord|the Toff|the Speculator|Sir Penny Sugar) plays seat 1 against a scripted rival in seat 0$"
)]
fn seat_focus(w: &mut SimWorld, who: String) {
    state(w).focus = Some(id_of(&who));
}

#[given(regex = r#"^the rival holds the lease on every block except "(.+)"$"#)]
fn rival_holds_all(w: &mut SimWorld, except: String) {
    let world = game(w);
    let year = world.clock().year();
    let term = world.config().economy.land.lease_term_years;
    for block in &mut world.map_mut().blocks {
        if block.name != except && block.kind == sim_core::land::BlockKind::Buildable {
            block.lease = Some(Lease::new(
                AccountId::Player(RIVAL),
                year,
                term,
                block.land_base_price,
            ));
        }
    }
}

#[given(regex = r#"^the rival's lease on "(.+)" was bought for (£[\d,]+)$"#)]
fn rival_paid(w: &mut SimWorld, name: String, paid: String) {
    let world = game(w);
    let block = block_named(world, &name);
    let lease = world.map_mut().blocks[block.0 as usize]
        .lease
        .as_mut()
        .unwrap();
    lease.last_price = pounds(&paid);
}

#[given(
    regex = r#"^(the Grocer|the Landlord|the Toff|the Speculator|Sir Penny Sugar) holds the lease on "([^"]+)"$"#
)]
fn focus_holds(w: &mut SimWorld, who: String, name: String) {
    let term = game(w).config().economy.land.lease_term_years;
    focus_holds_with(w, who, name, term, None);
}

#[given(
    regex = r#"^the Landlord holds the lease on "(.+)" with (\d+) years left, bought for (£[\d,]+)$"#
)]
fn landlord_holds_late(w: &mut SimWorld, name: String, left: u32, paid: String) {
    focus_holds_with(w, "the Landlord".into(), name, left, Some(pounds(&paid)));
}

fn focus_holds_with(w: &mut SimWorld, who: String, name: String, left: u32, paid: Option<Money>) {
    assert_eq!(state(w).focus, Some(id_of(&who)));
    let world = game(w);
    let block = block_named(world, &name);
    let year = world.clock().year();
    let term = world.config().economy.land.lease_term_years;
    let start = year - i32::try_from(term - left).unwrap();
    let b = &mut world.map_mut().blocks[block.0 as usize];
    let price = paid.unwrap_or(b.land_base_price);
    b.lease = Some(Lease::new(AccountId::Player(FOCUS), start, term, price));
}

#[given(regex = r#"^(the Landlord|the Toff|Sir Penny Sugar) holds every lease in the "(.+)" set$"#)]
fn focus_holds_set(w: &mut SimWorld, who: String, set: String) {
    let names: Vec<String> = {
        let world = game(w);
        let key = set_key_named(world, &set);
        world
            .map()
            .blocks
            .iter()
            .filter(|b| b.set.as_deref() == Some(key.as_str()))
            .map(|b| b.name.clone())
            .collect()
    };
    assert!(!names.is_empty(), "no blocks in {set:?}");
    for name in names {
        focus_holds(w, who.clone(), name);
    }
}

/// The config id of the colour set called `name` (`map.yaml` `sets`).
fn set_key_named(world: &World, name: &str) -> String {
    world
        .config()
        .map
        .sets
        .iter()
        .find(|(_, s)| s.name == name)
        .map_or_else(|| panic!("no set called {name:?}"), |(k, _)| k.clone())
}

#[given(regex = r"^the Speculator's balance is (£[\d,]+)$")]
fn speculator_balance(w: &mut SimWorld, to: String) {
    let world = game(w);
    let account = AccountId::Player(FOCUS);
    let now = world.ledger().balance(account);
    world
        .ledger_mut()
        .transfer(AccountId::City, account, pounds(&to) - now);
}

#[given(regex = r"^(\d+) people live in the city$")]
fn people(w: &mut SimWorld, n: u64) {
    let sources = w.config.sources_mut();
    common::set_key(
        sources,
        "customers.yaml",
        "/initial_population",
        &n.to_string(),
    );
    if n == 0 {
        common::set_key(
            sources,
            "economy.yaml",
            "/population/migration_per_day",
            "0",
        );
    }
}

#[given(regex = r"^every player starts with (£[\d,]+)$")]
fn starting_cash(w: &mut SimWorld, cash: String) {
    let pounds = pounds(&cash).pence() / 100;
    common::set_key(
        w.config.sources_mut(),
        "economy.yaml",
        "/money/starting_cash",
        &pounds.to_string(),
    );
}

#[given(regex = r"^groceries unlock in (\d+)$")]
fn groceries_unlock(w: &mut SimWorld, year: i32) {
    common::set_key(
        w.config.sources_mut(),
        "businesses.yaml",
        "/types/grocery/unlock_year",
        &year.to_string(),
    );
}

#[given(regex = r"^apartments are obsolete from (\d+)$")]
fn apartments_obsolete(w: &mut SimWorld, year: i32) {
    common::set_key(
        w.config.sources_mut(),
        "businesses.yaml",
        "/types/apartments/obsolete_year",
        &year.to_string(),
    );
}

// --- When ------------------------------------------------------------------------------------

#[when(
    regex = r"^(the Landlord|the Toff|the Speculator|Sir Penny Sugar) plays for (\d+) game (hour|hours|day|days)$"
)]
fn plays_for(w: &mut SimWorld, who: String, n: u64, unit: String) {
    assert_eq!(state(w).focus, Some(id_of(&who)));
    let per = if unit.starts_with("hour") { 1 } else { 24 };
    let ticks = n * per * ticks_per_hour(w);
    play_ticks(w, ticks);
}

#[when(
    regex = r"^(the Landlord|the Toff|the Speculator|Sir Penny Sugar) plays until (\d\d):(\d\d) on day (\d+)$"
)]
fn plays_until(w: &mut SimWorld, who: String, h: u32, m: u32, day: u32) {
    assert_eq!(state(w).focus, Some(id_of(&who)));
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
        advance(&mut w.roster, true);
    }
}

#[when(regex = r#"^the rival bids (£[\d,]+) to open an auction on "(.+)"$"#)]
fn rival_opens(w: &mut SimWorld, bid: String, name: String) {
    let world = game(w);
    let block = block_named(world, &name);
    world.submit(
        RIVAL,
        Command::OpenLandAuction {
            block,
            bid: pounds(&bid),
        },
    );
    advance(&mut w.roster, true);
}

#[when(regex = r#"^the rival offers "(.+)" for auction$"#)]
fn rival_offers(w: &mut SimWorld, name: String) {
    let world = game(w);
    let block = block_named(world, &name);
    world.submit(
        RIVAL,
        Command::OfferForAuction {
            item: AuctionItem::Land(block),
        },
    );
    advance(&mut w.roster, true);
}

#[when("the auction is played out with the rival passing")]
fn auction_played_out(w: &mut SimWorld) {
    let limit = 10 * ticks_per_hour(w);
    let mut ticks = 0;
    while game(w).auctions().is_busy() {
        advance(&mut w.roster, true);
        ticks += 1;
        assert!(ticks < limit, "the auction never closed");
    }
}

#[when("the full roster plays the whole game")]
fn full_roster(w: &mut SimWorld) {
    common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[]");
    let config = w.config.load_ok();
    let st = state(w);
    *st = RosterState::default();
    st.game = Some(Box::new(World::new(config, 42, 6)));
    st.seats = ROSTER
        .iter()
        .zip(1u8..)
        .map(|(id, p)| AiSeat::new(PlayerId(p), *id, 42))
        .collect();
    while !st.game.as_ref().unwrap().clock().is_over() {
        advance(st, false);
    }
}

#[when(
    regex = r"^two games with seed (\d+) are played to the whistle with the full roster seated by ai.yaml$"
)]
fn two_roster_games(w: &mut SimWorld, seed: u64) {
    let list = format!("[{}]", ROSTER.join(", "));
    common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", &list);
    let config = w.config.load_ok();
    for _ in 0..2 {
        let mut world = World::new(config.clone(), seed, 6);
        assert_eq!(world.ai_seats().len(), 5);
        while !world.clock().is_over() {
            world.tick();
        }
        let ai = |a: &AccountId| matches!(a, AccountId::Player(p) if p.0 > 0);
        let leases = world
            .map()
            .blocks
            .iter()
            .filter(|b| b.lease.as_ref().is_some_and(|l| ai(&l.holder)))
            .count();
        let buildings = world
            .businesses()
            .by_id
            .values()
            .filter(|b| ai(&b.owner))
            .count();
        w.roster.hashes.push(world.state_hash());
        w.roster.holdings.push((leases, buildings));
    }
}

// --- Then ------------------------------------------------------------------------------------

#[then(
    regex = r#"^(the Landlord|the Toff|the Speculator|Sir Penny Sugar) opened an auction on "(.+)" at (£[\d,]+)$"#
)]
fn opened_on(w: &mut SimWorld, who: String, name: String, bid: String) {
    let block = block_named(game(w), &name);
    let me = player_of(&w.roster, &who);
    let want = Command::OpenLandAuction {
        block,
        bid: pounds(&bid),
    };
    let opened: Vec<_> = w
        .roster
        .issued
        .iter()
        .filter(|i| i.player == me && matches!(i.command, Command::OpenLandAuction { .. }))
        .collect();
    assert_eq!(
        opened.first().map(|i| &i.command),
        Some(&want),
        "{opened:?}"
    );
}

#[then(regex = r#"^(the Landlord|the Toff|the Speculator|Sir Penny Sugar) never bid on "(.+)"$"#)]
fn never_bid(w: &mut SimWorld, who: String, name: String) {
    let block = block_named(game(w), &name);
    let me = player_of(&w.roster, &who);
    let bids: Vec<_> = w
        .roster
        .issued
        .iter()
        .filter(|i| i.player == me && bid_of(i).is_some_and(|(_, b)| b == Some(block)))
        .collect();
    assert!(bids.is_empty(), "{bids:?}");
}

#[then(regex = r#"^"(.+)" stays with the rival$"#)]
fn stays_with_rival(w: &mut SimWorld, name: String) {
    let world = game(w);
    let block = block_named(world, &name);
    let holder = world
        .map()
        .block(block)
        .unwrap()
        .lease
        .as_ref()
        .map(|l| l.holder);
    assert_eq!(holder, Some(AccountId::Player(RIVAL)));
}

#[then(regex = r#"^the Speculator raised on "(.+)"$"#)]
fn speculator_raised(w: &mut SimWorld, name: String) {
    let block = block_named(game(w), &name);
    assert!(
        w.roster.issued.iter().any(|i| i.player == FOCUS
            && i.on_auction == Some(AuctionItem::Land(block))
            && matches!(i.command, Command::Raise { .. })),
        "{:?}",
        w.roster.issued
    );
}

#[then(regex = r#"^the Speculator won "([^"]+)"(?: at auction)?$"#)]
fn speculator_won(w: &mut SimWorld, name: String) {
    let block = block_named(game(w), &name);
    assert!(
        w.roster.won.contains(&(FOCUS, block)),
        "won {:?}",
        w.roster.won
    );
}

#[then(
    regex = r#"^(the Landlord|the Speculator) (offered|did offer|didn't offer) "(.+)" for auction$"#
)]
fn offered(w: &mut SimWorld, who: String, did: String, name: String) {
    let block = block_named(game(w), &name);
    let me = player_of(&w.roster, &who);
    let offered = w.roster.issued.iter().any(|i| {
        i.player == me
            && i.command
                == Command::OfferForAuction {
                    item: AuctionItem::Land(block),
                }
    });
    assert_eq!(offered, did != "didn't offer", "{:?}", w.roster.issued);
}

#[then(regex = r#"^every bid the Speculator made on "(.+)" was at most (\d+)% of its book value$"#)]
fn within_book(w: &mut SimWorld, name: String, percent: i64) {
    let block = block_named(game(w), &name);
    let share = Rate::from_bp(percent * 100);
    assert_eq!(game(w).config().ai.speculator.bargain_share.rate(), share);
    for i in &w.roster.issued {
        if i.player != FOCUS || i.on_auction != Some(AuctionItem::Land(block)) {
            continue;
        }
        if let Some((bid, _)) = bid_of(i) {
            let book = i.book_value.expect("a leased lot has a book value");
            assert!(
                bid <= share.apply(book),
                "bid {bid} over {percent}% of {book}"
            );
        }
    }
}

#[then(
    regex = r#"^(the Landlord|Sir Penny Sugar) owns (\d+|at least \d+) apartment blocks? on "(.+)"$"#
)]
fn owns_blocks(w: &mut SimWorld, who: String, n: String, name: String) {
    let world = game(w);
    let block = block_named(world, &name);
    let me = AccountId::Player(player_of(&w.roster, &who));
    let world = w.roster.game.as_ref().unwrap();
    let apartments = &world.config().customers.residential.building_type;
    let count = world
        .businesses()
        .on_block(block)
        .filter(|(_, b)| b.owner == me && &b.type_id == apartments)
        .count();
    let want: usize = n.trim_start_matches("at least ").parse().unwrap();
    if n.starts_with("at least") {
        assert!(count >= want, "{count} blocks");
    } else {
        assert_eq!(count, want);
    }
}

#[then(regex = r"^(the Landlord|Sir Penny Sugar) set the rent on every apartment block it built$")]
fn rent_set(w: &mut SimWorld, who: String) {
    let st = &w.roster;
    let me = player_of(st, &who);
    let world = st.game.as_ref().unwrap();
    let apartments = &world.config().customers.residential.building_type;
    let mine: Vec<_> = st
        .built
        .iter()
        .filter(|b| b.owner == AccountId::Player(me) && &b.type_id == apartments)
        .collect();
    assert!(!mine.is_empty(), "built no apartment block");
    for b in mine {
        assert!(
            st.issued.iter().any(|i| i.player == me
                && matches!(i.command, Command::SetRent { building, .. } if building == b.id)),
            "never set the rent on {:?}",
            b.id
        );
    }
}

#[then(regex = r"^(the Landlord|the Toff) held at least (\d+) leases?$")]
fn held_leases(w: &mut SimWorld, who: String, n: usize) {
    let me = player_of(&w.roster, &who);
    let count = w.roster.won.iter().filter(|(p, _)| *p == me).count();
    assert!(count >= n, "won {count} leases");
}

#[then(regex = r"^every building (the Landlord|Sir Penny Sugar) put up is the residential type$")]
fn only_residential(w: &mut SimWorld, who: String) {
    let st = &w.roster;
    let me = AccountId::Player(player_of(st, &who));
    let apartments = &st
        .game
        .as_ref()
        .unwrap()
        .config()
        .customers
        .residential
        .building_type;
    let other: Vec<_> = st
        .built
        .iter()
        .filter(|b| b.owner == me && &b.type_id != apartments)
        .collect();
    assert!(other.is_empty(), "{other:?}");
}

#[then(
    regex = r"^(the Grocer|the Toff|Sir Penny Sugar) put up at least (\d+) (shop|apartment block)s?$"
)]
fn put_up(w: &mut SimWorld, who: String, n: usize, what: String) {
    let st = &w.roster;
    let me = AccountId::Player(player_of(st, &who));
    let apartments = &st
        .game
        .as_ref()
        .unwrap()
        .config()
        .customers
        .residential
        .building_type;
    let count = st
        .built
        .iter()
        .filter(|b| b.owner == me && ((&b.type_id == apartments) == (what != "shop")))
        .count();
    assert!(count >= n, "{count} built");
}

#[then("the Toff never bid on a lot below High tier")]
fn toff_high_only(w: &mut SimWorld) {
    let st = &w.roster;
    let me = player_of(st, "the Toff");
    let world = st.game.as_ref().unwrap();
    for i in st.issued.iter().filter(|i| i.player == me) {
        if let Some((_, block)) = bid_of(i) {
            let block = block.expect("the Toff bids only on land");
            assert_eq!(world.map().block(block).unwrap().tier, Tier::High, "{i:?}");
        }
    }
}

#[then(regex = r#"^the Toff owns (at least|at most) (\d+) shops? on "(.+)"$"#)]
fn toff_shops(w: &mut SimWorld, how: String, n: usize, name: String) {
    let world = game(w);
    let block = block_named(world, &name);
    let count = world
        .businesses()
        .on_block(block)
        .filter(|(_, b)| b.owner == AccountId::Player(FOCUS) && b.hours != Hours::None)
        .count();
    if how == "at least" {
        assert!(count >= n, "{count} shops");
    } else {
        assert!(count <= n, "{count} shops");
    }
}

#[then("every Toff shop has high fit-out")]
fn toff_fit_out(w: &mut SimWorld) {
    let world = game(w);
    for (id, b) in world.businesses().owned_by(AccountId::Player(FOCUS)) {
        assert_eq!(b.fit_out, FitOutLevel::High, "{id:?}");
    }
}

#[then(regex = r"^every Toff price is (\d+)% of reference$")]
fn toff_prices(w: &mut SimWorld, percent: i64) {
    let world = game(w);
    let mut priced = 0;
    for (id, b) in world.businesses().owned_by(AccountId::Player(FOCUS)) {
        for (good, price) in &b.prices {
            priced += 1;
            assert_eq!(price.bp(), percent * 100, "{id:?} {good}");
        }
    }
    assert!(priced > 0, "no prices");
}

#[then("the Toff's balance is at least its cash buffer")]
fn toff_buffer(w: &mut SimWorld) {
    let world = game(w);
    let buffer = world.config().ai.toff.cash_buffer.money();
    let balance = world.ledger().balance(AccountId::Player(FOCUS));
    assert!(balance >= buffer, "{balance} under {buffer}");
}

#[then(regex = r"^the rent on (?:his|its) first block is (above|below) £(\d+)\.(\d\d)$")]
fn first_rent(w: &mut SimWorld, how: String, pounds: i64, pence: i64) {
    let world = game(w);
    let rules = &world.config().customers.residential;
    let (id, _) = world
        .businesses()
        .owned_by(AccountId::Player(FOCUS))
        .find(|(_, b)| b.type_id == rules.building_type)
        .expect("no block");
    let rent = world
        .population()
        .residential
        .rent(id, world.config().economy.residential.default_rent.money());
    let mark = Money::from_pence(pounds * 100 + pence);
    if how == "above" {
        assert!(rent > mark, "rent {rent}");
    } else {
        assert!(rent < mark, "rent {rent}");
    }
}

#[then(
    regex = r"^every bid (the Landlord|the Toff|the Speculator|Sir Penny Sugar) made was within its aggression share of its bid ceiling$"
)]
fn within_aggression(w: &mut SimWorld, who: String) {
    let st = &w.roster;
    let me = player_of(st, &who);
    let world = st.game.as_ref().unwrap();
    let config = world.config();
    let id = id_of(&who);
    let base = aggression(config, id);
    let mut bids = 0;
    for i in st.issued.iter().filter(|i| i.player == me) {
        let Some((bid, block)) = bid_of(i) else {
            continue;
        };
        bids += 1;
        let premium = match block {
            Some(b) if id == "sir_penny_sugar" && has_station(world, b) => {
                config.ai.sir_penny_sugar.station_premium.rate().bp()
            }
            _ => 0,
        };
        let share = Rate::from_bp(base.bp() + premium);
        assert!(
            bid <= share.apply(i.ceiling),
            "bid {bid} over {share:?} of ceiling {}: {i:?}",
            i.ceiling
        );
    }
    // The Toff cannot afford a High lot on the shipped economy, so it may never bid.
    assert!(bids > 0 || id == "toff", "{who} never bid");
}

#[then(
    regex = r"^(the Grocer|the Landlord|the Toff|the Speculator|Sir Penny Sugar) had at least its cash buffer after every (building it put up|lease it won)$"
)]
fn kept_buffer(w: &mut SimWorld, who: String, what: String) {
    let st = &w.roster;
    let me = player_of(st, &who);
    let buffer = cash_buffer(st.game.as_ref().unwrap().config(), id_of(&who));
    let list = if what.starts_with("building") {
        &st.after_building
    } else {
        &st.after_lease
    };
    let mine: Vec<Money> = list
        .iter()
        .filter(|(p, _)| *p == me)
        .map(|(_, m)| *m)
        .collect();
    assert!(!mine.is_empty(), "{who} bought nothing");
    for balance in mine {
        assert!(balance >= buffer, "{balance} under {buffer}");
    }
}

#[then("no AI is bankrupt")]
fn none_bankrupt(w: &mut SimWorld) {
    let world = w.roster.game.as_ref().unwrap();
    let out: Vec<_> = world
        .players()
        .iter()
        .filter(|(_, s)| *s == PlayerStatus::Bankrupt)
        .collect();
    assert!(out.is_empty(), "{out:?}");
}

#[then(regex = r"^nobody built a grocery before (\d+)$")]
fn no_early_grocery(w: &mut SimWorld, year: i32) {
    let early: Vec<_> = w
        .roster
        .built
        .iter()
        .filter(|b| b.type_id == "grocery" && b.year < year)
        .collect();
    assert!(early.is_empty(), "{early:?}");
}

#[then(regex = r"^nobody built apartments in or after (\d+)$")]
fn no_late_apartments(w: &mut SimWorld, year: i32) {
    let late: Vec<_> = w
        .roster
        .built
        .iter()
        .filter(|b| b.type_id == "apartments" && b.year >= year)
        .collect();
    assert!(late.is_empty(), "{late:?}");
}

#[then(regex = r"^the AIs put up at least (\d+) buildings$")]
fn ais_built(w: &mut SimWorld, n: usize) {
    let by: BTreeMap<String, usize> = w.roster.built.iter().fold(BTreeMap::new(), |mut m, b| {
        *m.entry(b.type_id.clone()).or_insert(0) += 1;
        m
    });
    assert!(w.roster.built.len() >= n, "{by:?}");
}

#[then(regex = r"^the Grocer put up a shop before (\d+)$")]
fn grocer_built_early(w: &mut SimWorld, year: i32) {
    let me = AccountId::Player(player_of(&w.roster, "the Grocer"));
    assert!(
        w.roster
            .built
            .iter()
            .any(|b| b.owner == me && b.year < year),
        "{:?}",
        w.roster.built
    );
}

#[then(
    regex = r#"^the first building (the Landlord|the Toff|Sir Penny Sugar) put up is a Hotel in the "(.+)" set$"#
)]
fn first_is_hotel(w: &mut SimWorld, who: String, set: String) {
    let me = AccountId::Player(player_of(&w.roster, &who));
    let world = w.roster.game.as_ref().unwrap();
    let key = set_key_named(world, &set);
    let first = w
        .roster
        .built
        .iter()
        .find(|b| b.owner == me)
        .unwrap_or_else(|| panic!("{who} built nothing"));
    assert!(
        sim_core::hotels::is_hotel(world.config(), &first.type_id),
        "first built {first:?}"
    );
    let block = world.businesses().get(first.id).unwrap().block;
    assert_eq!(
        world.map().block(block).unwrap().set.as_deref(),
        Some(key.as_str())
    );
}

#[then(regex = r"^(the Landlord|the Toff|Sir Penny Sugar) never tried to build a Hotel$")]
fn never_tried_hotel(w: &mut SimWorld, who: String) {
    let me = player_of(&w.roster, &who);
    let config = w.roster.game.as_ref().unwrap().config();
    let tried: Vec<_> = w
        .roster
        .issued
        .iter()
        .filter(|i| {
            i.player == me
                && matches!(&i.command, Command::Build { type_id, .. }
                    if sim_core::hotels::is_hotel(config, type_id))
        })
        .collect();
    assert!(tried.is_empty(), "{tried:?}");
    assert!(
        w.roster
            .built
            .iter()
            .any(|b| b.owner == AccountId::Player(me)),
        "{who} built nothing at all"
    );
}

#[then("both roster games end with the same state hash")]
fn same_hash(w: &mut SimWorld) {
    assert_eq!(w.roster.hashes.len(), 2);
    assert_eq!(w.roster.hashes[0], w.roster.hashes[1]);
}

#[then("the AIs held leases and buildings at the end of both games")]
fn held_at_end(w: &mut SimWorld) {
    assert_eq!(w.roster.holdings.len(), 2);
    for (leases, buildings) in &w.roster.holdings {
        assert!(*leases > 0 && *buildings > 0, "{:?}", w.roster.holdings);
    }
}

// --- R66: opportunism -----------------------------------------------------------------------

#[given(regex = r"^flats let at £(\d+)\.(\d\d) a day$")]
fn flats_let_at(w: &mut SimWorld, pounds: u32, pence: u32) {
    // The city's default rent and the reference rent residents judge prices by, together.
    let rent = format!("{pounds}.{pence:02}");
    let sources = w.config.sources_mut();
    common::set_key(sources, "economy.yaml", "/residential/default_rent", &rent);
    common::set_key(
        sources,
        "economy.yaml",
        "/residential/reference_rent",
        &rent,
    );
}

#[when(regex = r#"^(the Grocer|the Toff) is handed the lease on "([^"]+)"$"#)]
fn handed_lease(w: &mut SimWorld, who: String, name: String) {
    focus_holds(w, who, name);
}

#[given("the rival holds every lease")]
fn rival_holds_every_lease(w: &mut SimWorld) {
    rival_holds_all(w, String::new());
}

#[when(regex = r"^seat 1 plays for (\d+) game days?$")]
fn seat_plays_for(w: &mut SimWorld, days: u64) {
    let ticks = days * 24 * ticks_per_hour(w);
    play_ticks(w, ticks);
}

fn apartment_type(st: &RosterState) -> String {
    st.game
        .as_ref()
        .unwrap()
        .config()
        .customers
        .residential
        .building_type
        .clone()
}

/// The focus's apartment blocks standing now.
fn focus_blocks(st: &RosterState) -> Vec<(BusinessId, sim_core::businesses::Business)> {
    let apartments = apartment_type(st);
    let world = st.game.as_ref().unwrap();
    world
        .businesses()
        .owned_by(AccountId::Player(FOCUS))
        .filter(|(_, b)| b.type_id == apartments)
        .map(|(id, b)| (id, b.clone()))
        .collect()
}

#[then(regex = r"^the Toff (did|didn't) put up an apartment block, only ever on High tier$")]
fn toff_flats(w: &mut SimWorld, did: String) {
    let st = &w.roster;
    let apartments = apartment_type(st);
    let world = st.game.as_ref().unwrap();
    let mut count = 0;
    for b in &st.built {
        if b.owner == AccountId::Player(FOCUS) && b.type_id == apartments {
            let block = world.businesses().get(b.id).unwrap().block;
            assert_eq!(world.map().block(block).unwrap().tier, Tier::High, "{b:?}");
            count += 1;
        }
    }
    assert_eq!(count > 0, did == "did", "{count} apartment blocks");
}

#[then("every Toff apartment block has high fit-out")]
fn toff_flats_high(w: &mut SimWorld) {
    let blocks = focus_blocks(&w.roster);
    assert!(!blocks.is_empty(), "no apartment block");
    for (id, b) in blocks {
        assert_eq!(b.fit_out, FitOutLevel::High, "{id:?}");
    }
}

#[then(regex = r"^the rent on every Toff apartment block is above £(\d+)\.(\d\d)$")]
fn toff_rent_above(w: &mut SimWorld, pounds: i64, pence: i64) {
    let floor = Money::from_pence(pounds * 100 + pence);
    let blocks = focus_blocks(&w.roster);
    assert!(!blocks.is_empty(), "no apartment block");
    let world = w.roster.game.as_ref().unwrap();
    for (id, _) in blocks {
        let rent = world.population().residential.rents.get(&id).copied();
        assert!(rent.is_some_and(|r| r > floor), "{id:?} let at {rent:?}");
    }
}

#[then(regex = r"^the (Grocer|Toff) never put up an apartment block$")]
fn never_flats(w: &mut SimWorld, who: String) {
    let st = &w.roster;
    let player = player_of(st, &format!("the {who}"));
    let me = AccountId::Player(player);
    let apartments = apartment_type(st);
    let flats: Vec<_> = st
        .built
        .iter()
        .filter(|b| b.owner == me && b.type_id == apartments)
        .collect();
    assert!(flats.is_empty(), "{flats:?}");
    let tried: Vec<_> = st
        .issued
        .iter()
        .filter(|i| {
            i.player == player
                && matches!(&i.command, Command::Build { type_id, .. } if *type_id == apartments)
        })
        .collect();
    assert!(tried.is_empty(), "{tried:?}");
}

#[then(regex = r"^(?:the Speculator|the Landlord) (bought|never bid on) an apartment block$")]
fn speculator_flats(w: &mut SimWorld, how: String) {
    let st = &w.roster;
    let apartments = apartment_type(st);
    let world = st.game.as_ref().unwrap();
    let is_flats = |id: BusinessId| {
        world
            .businesses()
            .get(id)
            .is_some_and(|b| b.type_id == apartments)
    };
    if how == "bought" {
        assert!(
            st.bought.iter().any(|(p, id)| *p == FOCUS && is_flats(*id)),
            "bought {:?}",
            st.bought
        );
    } else {
        let bids: Vec<_> = st
            .issued
            .iter()
            .filter(|i| {
                i.player == FOCUS
                    && match i.command {
                        Command::OpenBusinessAuction { business, .. } => is_flats(business),
                        Command::Raise { .. } => {
                            matches!(i.on_auction, Some(AuctionItem::Business(id)) if is_flats(id))
                        }
                        _ => false,
                    }
            })
            .collect();
        assert!(bids.is_empty(), "{bids:?}");
    }
}

#[then("the Speculator offered an apartment block it bought for auction")]
fn speculator_relisted(w: &mut SimWorld) {
    let st = &w.roster;
    let relisted = st.bought.iter().any(|(p, id)| {
        *p == FOCUS
            && st.issued.iter().any(|i| {
                i.player == FOCUS
                    && i.command
                        == Command::OfferForAuction {
                            item: AuctionItem::Business(*id),
                        }
            })
    });
    assert!(relisted, "bought {:?}", st.bought);
}

#[then(
    regex = r"^after every building it put up, at most (\d+)% of the Toff's buildings by build cost were apartment blocks$"
)]
fn toff_within_cap(w: &mut SimWorld, percent: i64) {
    let st = &w.roster;
    let apartments = apartment_type(st);
    let world = st.game.as_ref().unwrap();
    assert_eq!(
        world.config().ai.toff.opportunism.max_outside_share.rate(),
        Rate::from_bp(percent * 100)
    );
    let (mut flats, mut all) = (0i64, 0i64);
    let mut checked = 0;
    for b in st
        .built
        .iter()
        .filter(|b| b.owner == AccountId::Player(FOCUS))
    {
        let cost = world.businesses().get(b.id).unwrap().build_cost.pence();
        all += cost;
        if b.type_id == apartments {
            flats += cost;
            checked += 1;
        }
        assert!(
            flats * 100 <= percent * all,
            "{flats} of {all} in apartment blocks after {:?}",
            b.id
        );
    }
    assert!(checked > 0, "built no apartment block");
}
