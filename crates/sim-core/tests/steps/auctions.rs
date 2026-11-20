//! Steps for `features/auctions/` (and the game-level land steps): a whole [`World`] driven
//! through player commands, one tick per command, exactly as a client would.

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_core::auctions::{AuctionError, AuctionItem, AuctionPhase, real_seconds_to_ticks};
use sim_core::economy::{AccountId, PlayerId};
use sim_core::world::Command;
use sim_core::{Money, World};

use crate::SimWorld;
use crate::common;
use crate::steps::land::{apartment_block, block_named};

/// The game under test and every account's balance when it started.
#[derive(Debug, Default)]
pub struct AuctionState {
    game: Option<World>,
    start: BTreeMap<AccountId, Money>,
}

/// The game, panicking if no `Given a game with N players` ran.
pub fn game(w: &SimWorld) -> &World {
    w.auctions
        .game
        .as_ref()
        .expect("no game: add 'Given a game with N players'")
}

/// The game, mutably.
pub fn game_mut(w: &mut SimWorld) -> &mut World {
    w.auctions
        .game
        .as_mut()
        .expect("no game: add 'Given a game with N players'")
}

/// A player's account.
pub fn account(player: u8) -> AccountId {
    AccountId::Player(PlayerId(player))
}

/// `2,000` → £2,000.
fn pounds(text: &str) -> Money {
    Money::from_pounds(text.replace(',', "").parse().unwrap())
}

fn command(w: &mut SimWorld, player: u8, command: Command) {
    let world = game_mut(w);
    world.submit(PlayerId(player), command);
    world.tick();
}

fn run_ticks(w: &mut SimWorld, ticks: u64) {
    let world = game_mut(w);
    for _ in 0..ticks {
        world.tick();
    }
}

fn turn(world: &World) -> Option<PlayerId> {
    match world.auctions().current()?.phase {
        AuctionPhase::Bidding { turn, .. } => Some(turn),
        AuctionPhase::Countdown { .. } => None,
    }
}

/// Runs the clock until the current auction's countdown ends.
fn to_bidding(w: &mut SimWorld) {
    let world = game_mut(w);
    while matches!(
        world.auctions().current().map(|a| a.phase),
        Some(AuctionPhase::Countdown { .. })
    ) {
        world.tick();
    }
}

/// Everyone whose turn it is passes until the auction closes.
fn everyone_passes(w: &mut SimWorld) {
    to_bidding(w);
    while let Some(p) = turn(game(w)) {
        command(w, p.0, Command::Pass);
    }
}

#[given(expr = "a game with {int} players")]
fn a_game(w: &mut SimWorld, players: u8) {
    // No AI seats: scenarios script every player's moves themselves.
    common::set_key(w.config.sources_mut(), "ai.yaml", "/opponents", "[]");
    let world = World::new(w.config.load_ok(), 42, players);
    w.auctions.start = world.ledger().accounts().collect();
    w.auctions.game = Some(world);
}

#[when(regex = r#"^player (\d+) opens an auction on "(.+)" with £([\d,]+)$"#)]
fn opens_land(w: &mut SimWorld, player: u8, name: String, bid: String) {
    let block = block_named(game(w), &name);
    command(
        w,
        player,
        Command::OpenLandAuction {
            block,
            bid: pounds(&bid),
        },
    );
}

#[when(
    regex = r#"^player (\d+) opens an auction on the apartment block on "(.+)" with (its value|£50 less than its value|£[\d,]+)$"#
)]
fn opens_business(w: &mut SimWorld, player: u8, name: String, bid: String) {
    let business = apartment_block(game(w), &name);
    let value = game(w).businesses().by_id[&business].build_cost;
    let bid = match bid.as_str() {
        "its value" => value,
        "£50 less than its value" => value - Money::from_pounds(50),
        other => pounds(other.trim_start_matches('£')),
    };
    command(w, player, Command::OpenBusinessAuction { business, bid });
}

#[when(regex = r#"^player (\d+) wins the lease on "(.+)" for £([\d,]+)$"#)]
fn wins_lease(w: &mut SimWorld, player: u8, name: String, bid: String) {
    opens_land(w, player, name, bid);
    assert_eq!(game(w).auctions().last_refusal(), None);
    everyone_passes(w);
}

#[given(regex = r#"^player (\d+) has won the apartment block on "(.+)" at auction$"#)]
fn has_won_apartments(w: &mut SimWorld, player: u8, name: String) {
    opens_business(w, player, name.clone(), "its value".into());
    assert_eq!(game(w).auctions().last_refusal(), None);
    everyone_passes(w);
    let id = apartment_block(game(w), &name);
    assert_eq!(game(w).businesses().by_id[&id].owner, account(player));
}

#[when(regex = r#"^player (\d+) offers their lease on "(.+)" for auction$"#)]
fn offers_lease(w: &mut SimWorld, player: u8, name: String) {
    let block = block_named(game(w), &name);
    command(
        w,
        player,
        Command::OfferForAuction {
            item: AuctionItem::Land(block),
        },
    );
}

#[when(regex = r#"^player (\d+) offers the apartment block on "(.+)" for auction$"#)]
fn offers_business(w: &mut SimWorld, player: u8, name: String) {
    let business = apartment_block(game(w), &name);
    command(
        w,
        player,
        Command::OfferForAuction {
            item: AuctionItem::Business(business),
        },
    );
}

#[when(regex = r"^player (\d+) raises by £([\d,]+)$")]
fn raises(w: &mut SimWorld, player: u8, by: String) {
    command(w, player, Command::Raise { by: pounds(&by) });
}

#[when(regex = r"^player (\d+) passes$")]
fn passes(w: &mut SimWorld, player: u8) {
    command(w, player, Command::Pass);
}

#[when(regex = r"^(\d+) real seconds? pass(?:es)?$")]
fn real_seconds(w: &mut SimWorld, seconds: u32) {
    let ticks = real_seconds_to_ticks(seconds, game(w).clock().rules());
    run_ticks(w, ticks);
}

#[when("the auction runs to its close with everyone passing")]
fn runs_to_close(w: &mut SimWorld) {
    everyone_passes(w);
}

#[then(regex = r"^the command is (accepted|refused: .+)$")]
fn command_result(w: &mut SimWorld, result: String) {
    let refusal = game(w).auctions().last_refusal();
    let Some(reason) = result.strip_prefix("refused: ") else {
        assert_eq!(refusal, None, "expected the command to be accepted");
        return;
    };
    let error = refusal
        .unwrap_or_else(|| panic!("expected a refusal ({reason}), but it was accepted"))
        .error;
    let ok = match reason {
        "below the minimum" => matches!(error, AuctionError::BelowMinimum { .. }),
        "another auction is on" => error == AuctionError::Busy,
        "bidding has not started" => error == AuctionError::InCountdown,
        "not your turn" => error == AuctionError::NotYourTurn,
        "not an increment" => error == AuctionError::BadIncrement,
        "over the ceiling" => matches!(error, AuctionError::OverCeiling { .. }),
        "not for sale" => error == AuctionError::NotForSale,
        "not yours" => error == AuctionError::NotYours,
        other => panic!("unknown refusal {other:?}"),
    };
    assert!(ok, "expected {reason}, got {error:?}");
}

#[then(regex = r"^it is player (\d+)'s turn$")]
fn is_turn(w: &mut SimWorld, player: u8) {
    assert_eq!(turn(game(w)), Some(PlayerId(player)));
}

#[then("bidding has not started")]
fn not_started(w: &mut SimWorld) {
    let phase = game(w).auctions().current().map(|a| a.phase);
    assert!(
        matches!(phase, Some(AuctionPhase::Countdown { .. })),
        "{phase:?}"
    );
}

#[then(regex = r"^(no auction|an auction) is on$")]
fn auction_on(w: &mut SimWorld, which: String) {
    assert_eq!(game(w).auctions().is_busy(), which == "an auction");
}

#[then(regex = r"^the high bid is £([\d,]+) by player (\d+)$")]
fn high_bid(w: &mut SimWorld, bid: String, player: u8) {
    let auction = game(w).auctions().current().expect("no auction");
    assert_eq!(auction.high_bid, pounds(&bid));
    assert_eq!(auction.leader, Some(PlayerId(player)));
}

#[then(regex = r#"^a re-auction of "(.+)" is open with the city as the seller$"#)]
fn reauction(w: &mut SimWorld, name: String) {
    let block = block_named(game(w), &name);
    let auction = game(w).auctions().current().expect("no auction");
    assert_eq!(auction.item, AuctionItem::Land(block));
    assert_eq!(auction.seller, AccountId::City);
    assert_eq!(
        auction.leader, None,
        "a plain re-auction: nobody has bid yet"
    );
}

fn change(w: &SimWorld, who: AccountId) -> Money {
    let start = w.auctions.start.get(&who).copied().unwrap_or_default();
    game(w).ledger().balance(who) - start
}

#[then(regex = r"^player (\d+) has paid £([\d,]+)$")]
fn has_paid(w: &mut SimWorld, player: u8, amount: String) {
    assert_eq!(-change(w, account(player)), pounds(&amount));
}

#[then(regex = r"^player (\d+) has gained £([\d,]+)$")]
fn has_gained(w: &mut SimWorld, player: u8, amount: String) {
    assert_eq!(change(w, account(player)), pounds(&amount));
}

#[then(regex = r"^the city has received £([\d,]+) from auctions$")]
fn city_received(w: &mut SimWorld, amount: String) {
    assert_eq!(change(w, AccountId::City), pounds(&amount));
}

#[then(regex = r"^player (\d+) is overdrawn$")]
fn overdrawn(w: &mut SimWorld, player: u8) {
    assert!(game(w).ledger().balance(account(player)).is_negative());
}

#[then(regex = r#"^the minimum opening bid on "(.+)" is more than on "(.+)"$"#)]
fn dearer(w: &mut SimWorld, dear: String, cheap: String) {
    let world = game(w);
    let price = |name: &str| {
        world
            .map()
            .block(block_named(world, name))
            .unwrap()
            .land_base_price
    };
    assert!(price(&dear) > price(&cheap));
}

#[then(regex = r"^the sale paid player (\d+) £([\d,]+)$")]
fn sale_paid(w: &mut SimWorld, player: u8, amount: String) {
    let last = game(w).auctions().history().last().copied();
    let Some(sim_core::auctions::AuctionOutcome::Sold { seller, price, .. }) = last else {
        panic!("the last auction was not a sale: {last:?}");
    };
    assert_eq!((seller, price), (account(player), pounds(&amount)));
}

#[when(regex = r"^player (\d+) spends £([\d,]+) elsewhere$")]
fn spends_elsewhere(w: &mut SimWorld, player: u8, amount: String) {
    game_mut(w)
        .ledger_mut()
        .transfer(account(player), AccountId::City, pounds(&amount));
}

#[then("the sale defaulted")]
fn defaulted(w: &mut SimWorld) {
    let last = game(w).auctions().history().last().copied();
    assert!(
        matches!(
            last,
            Some(sim_core::auctions::AuctionOutcome::Defaulted { .. })
        ),
        "{last:?}"
    );
}
