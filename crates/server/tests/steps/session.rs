//! Steps for `inspector/session.feature`: drive `server::Session` without a socket.

use cucumber::{given, then, when};
use protocol::{Bid, BidKind, Delta, GameOver, Snapshot};
use server::{ServerConfig, Session, view};
use sim_core::config::economy::LandValueMethod;
use sim_core::economy::net_worth::{lease_land_value, market_land_price};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::{Config, World};

use crate::ServerWorld;
use crate::common;

/// Session scenario state.
#[derive(Debug, Default)]
pub struct SessionState {
    config: Option<Config>,
    server: Option<ServerConfig>,
    session: Option<Session>,
    refused: Option<String>,
    seed: u64,
    players: u8,
    ticks: u64,
    snapshot: Option<Snapshot>,
    delta: Option<Delta>,
    bids: Vec<Bid>,
    auction_block: Option<String>,
    over: Vec<GameOver>,
}

impl SessionState {
    fn session(&mut self) -> &mut Session {
        self.session.as_mut().expect("a game has started")
    }

    fn world(&self) -> &World {
        self.session.as_ref().expect("a game has started").world()
    }

    fn ticks_per_day(&self) -> u64 {
        u64::from(self.world().clock().rules().ticks_per_day())
    }

    /// One tick, keeping any final result.
    fn step(&mut self) {
        self.session().step();
        self.ticks += 1;
        if let Some(over) = self.session().game_over() {
            self.over.push(over);
        }
    }

    /// Takes a delta and keeps its bids.
    fn take_delta(&mut self) -> &Delta {
        let d = self.session().delta();
        self.bids.extend(d.bids.iter().cloned());
        self.delta = Some(d);
        self.delta.as_ref().unwrap()
    }
}

#[given("the shipped config and server settings")]
fn shipped(w: &mut ServerWorld) {
    w.session.config = Some(common::shipped_config());
    w.session.server = Some(common::shipped_server());
}

#[given(expr = "server settings with max_walkers {int}")]
fn max_walkers(w: &mut ServerWorld, n: u32) {
    w.session.server.as_mut().unwrap().max_walkers = n;
}

#[given(expr = "the config with the game ending after {int} game day(s)")]
fn short_game(w: &mut ServerWorld, days: i32) {
    let clock = &mut w.session.config.as_mut().unwrap().game.clock;
    let step = i32::try_from(clock.years_per_day).unwrap();
    clock.end_year = clock.start_year + days * step;
}

#[when(expr = "I start an inspector game with seed {int} and {int} players")]
fn start(w: &mut ServerWorld, seed: u64, players: u8) {
    let s = &mut w.session;
    s.seed = seed;
    s.players = players;
    s.session = Some(
        Session::new(
            s.config.clone().unwrap(),
            seed,
            players,
            s.server.as_ref().unwrap(),
        )
        .unwrap(),
    );
}

#[when(expr = "I try to start an inspector game with seed {int} and {int} players")]
fn try_start(w: &mut ServerWorld, seed: u64, players: u8) {
    let s = &mut w.session;
    match Session::new(
        s.config.clone().unwrap(),
        seed,
        players,
        s.server.as_ref().unwrap(),
    ) {
        Ok(session) => s.session = Some(session),
        Err(e) => s.refused = Some(e.to_string()),
    }
}

#[then(expr = "the game is refused naming {string}")]
fn refused(w: &mut ServerWorld, text: String) {
    let e = w.session.refused.as_ref().expect("the game was refused");
    assert!(e.contains(&text), "{e}");
}

#[then(expr = "there are {int} players")]
fn n_players(w: &mut ServerWorld, n: usize) {
    assert_eq!(
        view::players(w.session.world(), &view::DayTakings::default()).len(),
        n
    );
}

#[then("every seat is played by an AI opponent named after its personality")]
fn all_ai(w: &mut ServerWorld) {
    let world = w.session.world();
    let players = view::players(world, &view::DayTakings::default());
    for (i, p) in players.iter().enumerate() {
        let seat = PlayerId(u8::try_from(i).unwrap());
        let ai = world
            .ai_seats()
            .iter()
            .find(|s| s.player == seat)
            .unwrap_or_else(|| panic!("seat {i} has no AI"));
        let name = &world.config().ai.personalities[&ai.personality].name;
        assert_eq!(&p.name, name);
        assert_eq!(p.id, format!("p{i}"));
    }
}

#[when("I take a snapshot")]
fn snapshot(w: &mut ServerWorld) {
    let snap = w.session.session().snapshot();
    w.session.snapshot = Some(snap);
}

#[when("I take a delta")]
fn delta(w: &mut ServerWorld) {
    w.session.take_delta();
}

#[then(expr = "the board has {int} columns, {int} rows and {int} blocks")]
fn board_size(w: &mut ServerWorld, cols: u32, rows: u32, blocks: usize) {
    let b = &w.session.snapshot.as_ref().unwrap().board;
    assert_eq!((b.cols, b.rows, b.blocks.len()), (cols, rows, blocks));
}

#[then("the snapshot has one lot per block, in board order")]
fn lot_per_block(w: &mut ServerWorld) {
    let s = w.session.snapshot.as_ref().unwrap();
    let blocks: Vec<&String> = s.board.blocks.iter().map(|b| &b.key).collect();
    let lots: Vec<&String> = s.lots.iter().map(|l| &l.key).collect();
    assert_eq!(blocks, lots);
}

#[then(expr = "the board has river water and {int} bridges")]
fn river(w: &mut ServerWorld, bridges: usize) {
    let b = &w.session.snapshot.as_ref().unwrap().board;
    assert!(!b.river.is_empty());
    assert_eq!(b.bridges.len(), bridges);
}

#[then("no street rectangle overlaps the river")]
fn streets_dry(w: &mut ServerWorld) {
    let b = &w.session.snapshot.as_ref().unwrap().board;
    let overlaps = |a: &protocol::Rect, c: &protocol::Rect| {
        a.x < c.x + c.w && c.x < a.x + a.w && a.z < c.z + c.d && c.z < a.z + a.d
    };
    for s in &b.streets {
        for r in b.river.iter().chain(&b.bridges) {
            assert!(!overlaps(s, r), "street {s:?} overlaps {r:?}");
        }
        assert!(s.x >= 0.0 && s.z >= 0.0);
        assert!(s.x + s.w <= b.width_m && s.z + s.d <= b.depth_m);
    }
}

#[when(expr = "I play {int} game day(s) through the session")]
fn play_days(w: &mut ServerWorld, days: u64) {
    let n = days * w.session.ticks_per_day();
    for _ in 0..n {
        w.session.step();
    }
}

#[when(expr = "I play {int} game days through the session, taking a delta every {int} ticks")]
fn play_days_with_deltas(w: &mut ServerWorld, days: u64, every: u64) {
    let n = days * w.session.ticks_per_day();
    for i in 1..=n {
        w.session.step();
        if i % every == 0 {
            w.session.take_delta();
        }
    }
}

#[then("every lot's owner, lease, land value and building match the world")]
fn lots_match(w: &mut ServerWorld) {
    let world = w.session.world();
    let snap = w.session.snapshot.as_ref().unwrap();
    let config = world.config();
    let year = world.clock().year();
    let factors = world.events().land_value_factors(&config.events);
    let mut leased = 0;
    for (i, (block, lot)) in world.map().blocks.iter().zip(&snap.lots).enumerate() {
        assert_eq!(block.key, lot.key);
        match (&block.lease, block.utility) {
            (Some(lease), None) => {
                leased += 1;
                let owner = match lease.holder {
                    AccountId::Player(p) => format!("p{}", p.0),
                    _ => "city".to_owned(),
                };
                assert_eq!(lot.owner, owner, "{}", block.key);
                assert_eq!(
                    lot.lease.unwrap().years_left,
                    lease.remaining_years(year),
                    "{}",
                    block.key
                );
                let market = match config.economy.land.value_method {
                    LandValueMethod::BookDecay => block.land_base_price,
                    _ => market_land_price(block.land_base_price, &factors),
                };
                let value = lease_land_value(
                    config.economy.land.value_method,
                    lease.last_price,
                    market,
                    lease.remaining_years(year),
                    lease.term_years,
                );
                assert_eq!(lot.land_value, value.pence(), "{}", block.key);
            }
            (None, None) => assert!(lot.lease.is_none(), "{}", block.key),
            _ => {}
        }
        let here: Vec<_> = world
            .businesses()
            .by_id
            .values()
            .filter(|b| b.block.0 as usize == i)
            .collect();
        assert_eq!(lot.buildings as usize, here.len(), "{}", block.key);
        assert_eq!(lot.building.is_some(), !here.is_empty(), "{}", block.key);
        if let Some(b) = &lot.business {
            assert!(here.iter().any(|h| h.type_id == b.kind), "{}", block.key);
        }
    }
    assert!(leased > 0, "after 3 days the AIs hold some leases");
}

#[then("every lot's profit per day is its takings plus its rent less its costs")]
fn profit(w: &mut ServerWorld) {
    let snap = w.session.snapshot.as_ref().unwrap();
    for l in &snap.lots {
        assert_eq!(
            l.profit_per_day,
            l.takings_per_day + l.rent_per_day - l.costs_per_day,
            "{}",
            l.key
        );
    }
    assert!(
        snap.lots.iter().any(|l| l.costs_per_day > 0),
        "some building costs money to run"
    );
}

#[then("some lot's business is run by the city")]
fn city_run(w: &mut ServerWorld) {
    let snap = w.session.snapshot.as_ref().unwrap();
    assert!(
        snap.lots
            .iter()
            .any(|l| l.business.as_ref().is_some_and(|b| b.city_run))
    );
}

#[then("the delta has no lots, no players and no bids")]
fn empty_delta(w: &mut ServerWorld) {
    let d = w.session.delta.as_ref().unwrap();
    assert!(
        d.lots.is_empty() && d.players.is_empty() && d.bids.is_empty(),
        "{d:?}"
    );
}

#[then("the delta has some lots or players")]
fn busy_delta(w: &mut ServerWorld) {
    let d = w.session.delta.as_ref().unwrap();
    assert!(!(d.lots.is_empty() && d.players.is_empty()));
}

#[when("I play through the session until an auction opens")]
fn until_auction(w: &mut ServerWorld) {
    let limit = 3 * w.session.ticks_per_day();
    for _ in 0..limit {
        w.session.step();
        if w.session.world().auctions().current().is_some() {
            let d = w.session.take_delta();
            w.session.auction_block = d.auction.as_ref().map(|a| a.block.clone());
            return;
        }
    }
    panic!("no auction opened in 3 game days");
}

#[then("the live auction names a block of the board")]
fn auction_block(w: &mut ServerWorld) {
    let block = w.session.auction_block.clone().expect("an auction is live");
    assert!(
        w.session
            .world()
            .map()
            .blocks
            .iter()
            .any(|b| b.key == block)
    );
}

#[then(expr = "a bid event {string} names that block")]
fn bid_event(w: &mut ServerWorld, kind: String) {
    let block = w.session.auction_block.clone().unwrap();
    let want = match kind.as_str() {
        "open" => BidKind::Open,
        other => panic!("unknown kind {other}"),
    };
    assert!(
        w.session
            .bids
            .iter()
            .any(|b| b.kind == want && b.block == block),
        "{:?}",
        w.session.bids
    );
}

/// A bid kind by its wire name.
fn kind(name: &str) -> BidKind {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).unwrap()
}

#[then(expr = "a bid event {string} or {string} names that block")]
fn bid_result(w: &mut ServerWorld, a: String, b: String) {
    let block = w.session.auction_block.clone().unwrap();
    let kinds = [kind(&a), kind(&b)];
    assert!(
        w.session
            .bids
            .iter()
            .any(|bid| kinds.contains(&bid.kind) && bid.block == block),
        "{:?}",
        w.session.bids
    );
}

#[then("that lot is open for bids")]
fn open_for_bids(w: &mut ServerWorld) {
    let block = w.session.auction_block.clone().unwrap();
    let lots = view::lots(w.session.world(), &view::DayTakings::default());
    assert!(lots.iter().any(|l| l.key == block && l.open_for_bids));
}

#[when("I play through the session until that auction closes")]
fn until_closed(w: &mut ServerWorld) {
    let first = w.session.world().auctions().history().len();
    let limit = 3 * w.session.ticks_per_day();
    for _ in 0..limit {
        w.session.step();
        if w.session.world().auctions().history().len() > first {
            w.session.take_delta();
            return;
        }
    }
    panic!("the auction did not close in 3 game days");
}

#[then("the snapshot's recent bids include that result")]
fn recent_bids(w: &mut ServerWorld) {
    let block = w.session.auction_block.clone().unwrap();
    let snap = w.session.session().snapshot();
    assert!(
        snap.bids
            .iter()
            .any(|b| matches!(b.kind, BidKind::Won | BidKind::Unsold) && b.block == block),
        "{:?}",
        snap.bids
    );
    let ticks: Vec<u64> = snap.bids.iter().map(|b| b.tick).collect();
    assert!(ticks.windows(2).all(|t| t[0] <= t[1]), "newest last");
}

#[when("I play through the session until shoppers are out")]
fn until_shoppers(w: &mut ServerWorld) {
    let limit = w.session.ticks_per_day();
    for _ in 0..limit {
        w.session.step();
        if w.session.world().customers().on_map().next().is_some() {
            return;
        }
    }
    panic!("no shopper came out in a game day");
}

#[then(expr = "the delta has between {int} and {int} walkers")]
fn walkers(w: &mut ServerWorld, lo: usize, hi: usize) {
    let d = w.session.delta.as_ref().unwrap();
    assert_eq!(d.walkers.len() % 2, 0);
    let n = d.walkers.len() / 2;
    assert!((lo..=hi).contains(&n), "{n} walkers");
}

#[then("every walker stands on the board")]
fn walkers_on_board(w: &mut ServerWorld) {
    let board = view::board(w.session.world());
    let d = w.session.delta.as_ref().unwrap();
    for p in d.walkers.chunks(2) {
        let (x, z) = (f64::from(p[0]), f64::from(p[1]));
        assert!((0.0..=board.width_m).contains(&x), "x {x}");
        assert!((0.0..=board.depth_m).contains(&z), "z {z}");
    }
}

#[then(
    expr = "the state hash equals a headless game of seed {int} and {int} AI seats run for as many ticks"
)]
fn same_hash(w: &mut ServerWorld, seed: u64, players: u8) {
    let config = w.session.config.clone().unwrap();
    let ai: Vec<(PlayerId, String)> = config
        .ai
        .opponents
        .iter()
        .zip((0..players).rev())
        .map(|(p, s)| (PlayerId(s), p.clone()))
        .collect();
    let mut headless = World::seated(config, seed, players, &ai);
    for _ in 0..w.session.ticks {
        headless.tick();
    }
    assert!(w.session.ticks > 0);
    assert_eq!(w.session.world().state_hash(), headless.state_hash());
}

#[then("the game is over with a winner")]
fn over(w: &mut ServerWorld) {
    assert!(w.session.session().is_over());
    let over = w.session.over.first().expect("a result was sent");
    assert!(
        over.winner == "tie"
            || over.winner == "none"
            || over.players.iter().any(|p| p.id == over.winner),
        "{}",
        over.winner
    );
    assert_eq!(over.players.len(), usize::from(w.session.players));
}

#[then("the result is sent only once")]
fn once(w: &mut ServerWorld) {
    assert_eq!(w.session.over.len(), 1);
    assert!(w.session.session().game_over().is_none());
}

#[then("every business's goods table matches the world")]
fn goods_match(w: &mut ServerWorld) {
    let world = w.session.world();
    let snap = w.session.snapshot.as_ref().unwrap();
    let goods = &world.config().businesses.goods;
    let mut rows = 0;
    for (i, lot) in snap.lots.iter().enumerate() {
        let Some(business) = &lot.business else {
            continue;
        };
        // The lot card shows the largest building, the lowest id on a tie.
        let (_, b) = world
            .businesses()
            .by_id
            .iter()
            .filter(|(_, b)| b.block.0 as usize == i)
            .max_by_key(|(id, b)| (b.volume(), std::cmp::Reverse(**id)))
            .unwrap();
        assert_eq!(b.type_id, business.kind, "{}", lot.key);
        let wire: Vec<&String> = business.goods.iter().map(|g| &g.good).collect();
        let sim: Vec<&String> = b.prices.keys().collect();
        assert_eq!(wire, sim, "{}", lot.key);
        assert_eq!(
            business.capacity,
            b.capacity_per_good * i64::try_from(b.prices.len()).unwrap()
        );
        for row in &business.goods {
            let g = &goods[&row.good];
            assert_eq!(row.cost, g.purchase_price.money().pence(), "{}", row.good);
            assert_eq!(
                row.price,
                b.prices[&row.good].apply(g.ref_price.money()).pence()
            );
            assert_eq!(row.stock, b.stock_of(&row.good));
            assert_eq!(
                row.order,
                b.standing_order.get(&row.good).copied().unwrap_or(0)
            );
            assert_eq!(
                row.sold_today,
                b.sold_today.get(&row.good).copied().unwrap_or(0)
            );
            rows += 1;
        }
    }
    assert!(rows > 0, "after 3 days some shop sells something");
}

#[when("I play through the session until bidding starts")]
fn until_bidding(w: &mut ServerWorld) {
    let limit = 3 * w.session.ticks_per_day();
    for _ in 0..limit {
        w.session.step();
        let bidding =
            w.session.world().auctions().current().is_some_and(|a| {
                matches!(a.phase, sim_core::auctions::AuctionPhase::Bidding { .. })
            });
        if bidding {
            w.session.take_delta();
            return;
        }
    }
    panic!("bidding did not start in 3 game days");
}

#[then("the auction room has one seat per player and only the seat whose turn it is is to act")]
fn auction_room(w: &mut ServerWorld) {
    let a = w
        .session
        .delta
        .as_ref()
        .unwrap()
        .auction
        .clone()
        .expect("a live auction");
    let ids: Vec<String> = a.seats.iter().map(|s| s.player.clone()).collect();
    let want: Vec<String> = (0..w.session.players).map(|p| format!("p{p}")).collect();
    assert_eq!(ids, want);
    let turn = a.turn.clone().expect("someone's turn");
    for s in &a.seats {
        assert_eq!(
            s.status == protocol::SeatStatus::ToAct,
            s.player == turn,
            "{s:?}"
        );
    }
    assert!(a.round >= 1);
}
