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
use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use server::view;
use sim_core::World;
use sim_core::economy::PlayerId;

#[test]
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
            .balance(sim_core::economy::AccountId::Player(PlayerId(0)))
            .pence()
    );

    client.send(&ClientMessage::SetSpeed { speed: 4 });
    let mut deltas = 0;
    let mut last = 0;
    while deltas < 5 {
        if let ServerMessage::Delta(d) = client.msg() {
            assert!(d.tick >= last, "ticks only go forward");
            last = d.tick;
            if d.clock.speed == 4 {
                deltas += 1;
            }
        }
    }
    assert!(last > 0);

    client.send(&ClientMessage::Leave);
    assert!(client.until_closed(100_000));
    thread.join().unwrap().unwrap();
}
