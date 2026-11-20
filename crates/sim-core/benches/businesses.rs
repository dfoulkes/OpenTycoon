//! Businesses benchmarks: one game day of sales over N shops on the shipped config.
//! Run with `cargo bench -p sim-core --bench businesses`. Budgets: docs/engineering.md §4.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benchmark harness"
)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::config::ConfigFile;
use sim_core::economy::{AccountId, PlayerId};
use sim_core::land::{BlockId, Footprint};
use sim_core::money::Money;
use sim_core::{Config, ConfigSources};

fn shipped_config() -> Config {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        let text = std::fs::read_to_string(dir.join(file.file_name())).expect("config file");
        sources.insert(file.file_name(), text);
    }
    // The 3×3 grey-box map: these benches and their baselines are written against it. The
    // full 9×9 board has its own group in benches/tick.rs.
    let map =
        sources
            .get("map.yaml")
            .expect("map.yaml")
            .replacen("\nmap: full\n", "\nmap: v1\n", 1);
    sources.insert("map.yaml", map);
    Config::load(&sources).expect("shipped config is valid")
}

/// `n` shops, alternating day (bakery) and night (bar), each with a standing order to fill.
fn shops(config: &Config, n: u32) -> Businesses {
    let mut b = Businesses::default();
    for i in 0..n {
        let type_id = if i % 2 == 0 { "bakery" } else { "bar" };
        let kind = &config.businesses.types[type_id];
        let good = kind.goods[0].clone();
        let request = BuildRequest {
            type_id: type_id.into(),
            block: BlockId(i % 9),
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 3,
                d: 3,
            },
            height: 10,
            owner: AccountId::Player(PlayerId(u8::try_from(i % 4).unwrap())),
            fit_out: FitOutLevel::Medium,
        };
        let mut shop = Business::new(request, kind, Money::ZERO);
        shop.standing_order.insert(good, shop.capacity_per_good);
        b.by_id.insert(BusinessId(i), shop);
    }
    b.next_id = n;
    b.lock_orders();
    b
}

fn bench_day_of_sales(c: &mut Criterion) {
    let config = shipped_config();
    let rules = ClockRules::from_config(&config.game.clock);
    let mut group = c.benchmark_group("businesses_day_of_sales");
    group.sample_size(10);
    for n in [10u32, 100, 1_000] {
        let start = shops(&config, n);
        let goods: Vec<String> = start
            .by_id
            .values()
            .map(|s| s.prices.keys().next().unwrap().clone())
            .collect();
        // Every tick, 8 shoppers each try to buy one unit somewhere: ~69k attempts a day.
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut shops = start.clone();
                let mut clock = Clock::new(rules.clone());
                let mut takings = Money::ZERO;
                let mut cursor = 0u32;
                for _ in 0..rules.ticks_per_day() {
                    let fired = clock.advance();
                    if fired.settlement {
                        black_box(shops.deliver_orders());
                    }
                    for _ in 0..8 {
                        cursor = (cursor + 7) % n;
                        let id = BusinessId(cursor);
                        let good = &goods[cursor as usize];
                        if let Ok(m) = shops.try_sell(id, good, 1, &clock, &config.businesses) {
                            takings += m;
                        }
                    }
                }
                black_box(shops.midnight());
                black_box(takings)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_day_of_sales);
criterion_main!(benches);
