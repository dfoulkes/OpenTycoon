//! A scripted headless client (R88, R44): start the server, connect over a real socket, play a
//! new game with seed 42, check a lot in the snapshot against a `World` built directly with the
//! same seed, speed up, receive deltas, leave.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs,
    reason = "test code"
)]

mod common;

use common::Client;
use protocol::{ClientMessage, PROTOCOL_VERSION, SeatStatus, ServerMessage};
use server::view;
use sim_core::World;
use sim_core::economy::{AccountId, PlayerId};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one scripted session, start to leave"
)]
fn a_scripted_client_plays_watches_and_leaves() {
    let token = common::token();
    let (addr, thread) = server::spawn(&common::config_dir(), &token).unwrap();
    assert!(addr.ip().is_loopback());
    let mut client = Client::connect(addr);

    client.send(&ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token,
    });
    assert_eq!(
        client.msg(),
        ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION
        }
    );

    client.send(&ClientMessage::NewGame {
        seed: 42,
        economy: None,
        players: 4,
    });
    let ServerMessage::Snapshot(snap) = client.msg() else {
        panic!("expected a snapshot");
    };
    assert_eq!(snap.tick, 0);
    assert_eq!(snap.clock.speed, common::shipped_server().start_speed);
    assert_eq!(snap.players.len(), 4);

    // The same game built directly: every seat an AI, from the last seat down.
    let config = common::shipped_config();
    let ai: Vec<(PlayerId, String)> = config
        .ai
        .opponents
        .iter()
        .zip((0..4u8).rev())
        .map(|(p, s)| (PlayerId(s), p.clone()))
        .collect();
    let world = World::seated(config, 42, 4, &ai);
    let lots = view::lots(&world, &view::DayTakings::default());
    let key = "whitechapel";
    let block = world.map().blocks.iter().find(|b| b.key == key).unwrap();
    let wire = snap.lots.iter().find(|l| l.key == key).unwrap();
    assert_eq!(wire, lots.iter().find(|l| l.key == key).unwrap());
    assert_eq!(wire.land_value, block.land_base_price.pence());
    assert_eq!(wire.owner, "unowned");
    assert!(wire.lease.is_none());
    assert!(
        wire.business.as_ref().is_some_and(|b| b.city_run),
        "{wire:?}"
    );
    assert_eq!(
        snap.players[0].cash,
        world
            .ledger()
            .balance(AccountId::Player(PlayerId(0)))
            .pence()
    );

    // Fast-forward and keep the client's copy of the game up to date (lots and players are
    // replaced by key), until an auction is in its bidding rounds.
    client.send(&ClientMessage::SetSpeed { speed: 4 });
    let mut lots = snap.lots.clone();
    let mut players = snap.players.clone();
    let mut last = 0;
    let (tick, live) = client.until(20_000, |m| {
        let ServerMessage::Delta(d) = m else {
            return None;
        };
        assert!(d.tick >= last, "ticks only go forward");
        last = d.tick;
        for l in &d.lots {
            let at = lots.iter().position(|x| x.key == l.key).unwrap();
            lots[at] = l.clone();
        }
        for p in &d.players {
            let at = players.iter().position(|x| x.id == p.id).unwrap();
            players[at] = p.clone();
        }
        d.auction
            .as_ref()
            .filter(|a| a.round >= 1 && d.clock.speed == 4)
            .map(|a| (d.tick, a.clone()))
    });

    // The same game, run headless to the same tick.
    let mut world = world;
    while world.clock().tick() < tick {
        world.tick();
    }
    let house = world
        .auctions()
        .current()
        .expect("the auction is live headless too");
    assert_eq!(live.high_bid, house.high_bid.pence());
    assert_eq!(live.high_bidder, house.leader.map(|p| format!("p{}", p.0)));
    let turn = match house.phase {
        sim_core::auctions::AuctionPhase::Bidding { turn, .. } => format!("p{}", turn.0),
        sim_core::auctions::AuctionPhase::Countdown { .. } => panic!("still counting down"),
    };
    assert_eq!(live.turn.as_deref(), Some(turn.as_str()));
    // The room: one seat per player in seat order; the turn's seat is to act, the leader's
    // (if another) holds the high bid.
    let ids: Vec<&str> = live.seats.iter().map(|s| s.player.as_str()).collect();
    assert_eq!(ids, ["p0", "p1", "p2", "p3"]);
    let to_act: Vec<&str> = live
        .seats
        .iter()
        .filter(|s| s.status == SeatStatus::ToAct)
        .map(|s| s.player.as_str())
        .collect();
    assert_eq!(to_act, [turn.as_str()]);
    if let Some(leader) = live.high_bidder.as_ref().filter(|l| **l != turn) {
        let seat = live.seats.iter().find(|s| &s.player == leader).unwrap();
        assert_eq!(seat.status, SeatStatus::Raised);
        assert_eq!(seat.last_bid, Some(live.high_bid));
    }
    assert!(live.opening_bid > 0 && live.opening_bid <= live.high_bid);
    // Everything the client pieced together from deltas matches the world at that tick.
    for (block, lot) in world.map().blocks.iter().zip(&lots) {
        assert_eq!(block.key, lot.key);
        let holder = block.lease.as_ref().map(|l| view::account_id(l.holder));
        if block.utility.is_none() {
            let nobody = if block.kind == sim_core::land::BlockKind::Special {
                "city" // City Hall
            } else {
                "unowned"
            };
            assert_eq!(
                lot.owner,
                holder.unwrap_or_else(|| nobody.into()),
                "{}",
                lot.key
            );
        }
        let on_block = world
            .businesses()
            .by_id
            .values()
            .filter(|b| b.block.0 as usize == lots.iter().position(|l| l.key == lot.key).unwrap())
            .count();
        assert_eq!(lot.buildings as usize, on_block, "{}", lot.key);
    }
    for (i, p) in players.iter().enumerate() {
        let seat = PlayerId(u8::try_from(i).unwrap());
        assert_eq!(
            p.cash,
            world.ledger().balance(AccountId::Player(seat)).pence(),
            "{}",
            p.id
        );
    }

    client.send(&ClientMessage::Leave);
    assert!(client.until_closed(100_000));
    thread.join().unwrap().unwrap();
}
