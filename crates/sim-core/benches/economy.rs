//! Economy benchmarks: the 06:00 settlement and the midnight net-worth pass over N players and
//! N buildings on the shipped v1 3×3 map. Run with `cargo bench -p sim-core --bench economy`.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benchmark harness"
)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use sim_core::businesses::{Business, BusinessId, Businesses, FitOutLevel};
use sim_core::config::ConfigFile;
use sim_core::economy::settlement::settlement_bills;
use sim_core::economy::{AccountId, Ledger, PlayerId, net_worth, net_worth::net_worth_inputs};
use sim_core::land::{BlockId, Footprint, Lease, Map};
use sim_core::money::{Money, Rate};
use sim_core::utilities::Utilities;
use sim_core::{Config, ConfigSources};

fn shipped_config() -> Config {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        let text = std::fs::read_to_string(dir.join(file.file_name())).expect("config file");
        sources.insert(file.file_name(), text);
    }
    Config::load(&sources).expect("shipped config is valid")
}

/// `players` seats, every block leased round-robin, `buildings` bakeries spread over the map.
fn setup(config: &Config, players: u8, buildings: u32) -> (Ledger, Map, Businesses) {
    let account =
        |i: u32| AccountId::Player(PlayerId(u8::try_from(i % u32::from(players)).unwrap()));
    let mut ledger = Ledger::new();
    ledger.open(AccountId::City, Money::ZERO);
    for p in 0..players {
        ledger.open(AccountId::Player(PlayerId(p)), Money::from_pounds(1_000));
    }
    let mut map = Map::from_config(&config.map);
    let blocks = u32::try_from(map.blocks.len()).unwrap();
    for (i, block) in (0u32..).zip(map.blocks.iter_mut()) {
        block.lease = Some(Lease {
            holder: account(i),
            start_year: 1930,
            term_years: 30,
            last_price: Money::from_pounds(5_000),
        });
    }
    let mut businesses = Businesses::default();
    for i in 0..buildings {
        businesses.by_id.insert(
            BusinessId(i),
            Business {
                type_id: "bakery".into(),
                block: BlockId(i % blocks),
                footprint: Footprint {
                    x: 0,
                    y: 0,
                    w: 2,
                    d: 2,
                },
                height: 10,
                owner: account(i + 1),
                fit_out: FitOutLevel::Medium,
                build_cost: Money::from_pounds(240),
                prices: [("bread".to_owned(), Rate::ONE)].into(),
                stock: [("bread".to_owned(), 50)].into(),
                tomorrow_order: [("bread".to_owned(), 100)].into(),
                open: true,
            },
        );
    }
    businesses.next_id = buildings;
    (ledger, map, businesses)
}

fn bench_settlement(c: &mut Criterion) {
    let config = shipped_config();
    let utilities = Utilities::default();
    let mut group = c.benchmark_group("economy_settlement");
    for (players, buildings) in [(2u8, 10u32), (6, 100), (6, 1_000), (16, 10_000)] {
        let (ledger, map, businesses) = setup(&config, players, buildings);
        group.bench_with_input(
            BenchmarkId::new("bills_and_settle", format!("{players}p_{buildings}b")),
            &(),
            |b, ()| {
                b.iter(|| {
                    let mut ledger = ledger.clone();
                    let bills = settlement_bills(&map, &businesses, &utilities, &config, 1930);
                    ledger.settle(&bills);
                    black_box(ledger.total())
                });
            },
        );
    }
    group.finish();
}

fn bench_net_worth(c: &mut Criterion) {
    let config = shipped_config();
    let uplift = config.game.net_worth.lot_property_uplift.rate();
    let mut group = c.benchmark_group("economy_net_worth");
    for (players, buildings) in [(2u8, 10u32), (6, 100), (6, 1_000)] {
        let (ledger, map, businesses) = setup(&config, players, buildings);
        group.bench_with_input(
            BenchmarkId::new("every_player", format!("{players}p_{buildings}b")),
            &(),
            |b, ()| {
                b.iter(|| {
                    let total: Money = (0..players)
                        .map(|p| {
                            let inputs = net_worth_inputs(
                                AccountId::Player(PlayerId(p)),
                                &ledger,
                                &map,
                                &businesses,
                                &config.businesses.goods,
                                1930,
                            );
                            net_worth(&inputs, uplift)
                        })
                        .sum();
                    black_box(total)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_settlement, bench_net_worth);
criterion_main!(benches);
