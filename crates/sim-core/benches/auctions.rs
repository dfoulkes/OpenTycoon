//! Auction benchmarks: one full auction cycle (opening bid, 120 s countdown, a contested round
//! of bidding, a closing round of passes) on the auction house alone and through the world.
//! Run with `cargo bench -p sim-core --bench auctions`.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benchmark harness"
)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use sim_core::auctions::{AuctionHouse, AuctionItem, AuctionRules, Opening};
use sim_core::config::ConfigFile;
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::BlockId;
use sim_core::world::Command;
use sim_core::{Config, ConfigSources, Money, World};

fn shipped_config() -> Config {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        let text = std::fs::read_to_string(dir.join(file.file_name())).expect("config file");
        sources.insert(file.file_name(), text);
    }
    Config::load(&sources).expect("shipped config is valid")
}

fn house_cycle(rules: &AuctionRules, bidders: &[PlayerId]) -> usize {
    let mut house = AuctionHouse::new(rules.clone());
    let ceiling = Money::from_pounds(1_000_000);
    house
        .open(
            Opening {
                item: AuctionItem::Land(BlockId(0)),
                seller: AccountId::City,
                opening_bid: Some((PlayerId(0), Money::from_pounds(2_000))),
                minimum: Money::from_pounds(2_000),
            },
            ceiling,
            0,
        )
        .unwrap();
    let mut now = 0;
    // Countdown, one tick at a time, as the world runs it.
    while now < rules.countdown_ticks {
        now += 1;
        house.advance(now, bidders);
    }
    // Twenty contested raises, then a closing round of passes.
    for _ in 0..20 {
        let turn = match house.current().unwrap().phase {
            sim_core::auctions::AuctionPhase::Bidding { turn, .. } => turn,
            sim_core::auctions::AuctionPhase::Countdown { .. } => unreachable!(),
        };
        house
            .raise(turn, Money::from_pounds(100), ceiling, now, bidders)
            .unwrap();
        now += 1;
        house.advance(now, bidders);
    }
    while house.is_busy() {
        if let Some(sim_core::auctions::AuctionPhase::Bidding { turn, .. }) =
            house.current().map(|a| a.phase)
        {
            house.pass(turn, now, bidders).unwrap();
        }
        now += 1;
        house.advance(now, bidders);
    }
    house.history().len()
}

fn bench_auction_cycle(c: &mut Criterion) {
    let config = shipped_config();
    let world = World::new(config.clone(), 42, 4);
    let rules = world.auctions().rules().clone();
    let bidders: Vec<PlayerId> = (0..4).map(PlayerId).collect();
    let mut group = c.benchmark_group("auctions");
    group.bench_function("full_cycle_house_4_players", |b| {
        b.iter(|| black_box(house_cycle(&rules, &bidders)));
    });
    group.sample_size(20);
    group.bench_function("full_cycle_world_4_players", |b| {
        b.iter(|| {
            let mut world = World::new(config.clone(), 42, 4);
            world.submit(
                PlayerId(0),
                Command::OpenLandAuction {
                    block: BlockId(0),
                    bid: Money::from_pounds(2_000),
                },
            );
            // Nobody else acts: the countdown and three turn timers run out, then it closes.
            while world.auctions().history().is_empty() {
                world.tick();
            }
            black_box(world.state_hash())
        });
    });
    group.finish();
}

criterion_group!(benches, bench_auction_cycle);
criterion_main!(benches);
