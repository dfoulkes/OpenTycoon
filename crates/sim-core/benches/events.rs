//! Events benchmarks (§9): a year step with Decimal Day's price noise over N shops, and the
//! modifier stack's demand query with several modifiers in force (customers ask it once per
//! cohort, good and wave). Run with `cargo bench -p sim-core --bench events`.
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
use sim_core::config::ConfigFile;
use sim_core::economy::AccountId;
use sim_core::events::{Events, Modifier, Modifiers, StepInputs};
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};
use sim_core::population::Class;
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

fn shops(config: &Config, n: u32) -> Businesses {
    let kind = &config.businesses.types["bakery"];
    let mut businesses = Businesses::default();
    for i in 0..n {
        let request = BuildRequest {
            type_id: "bakery".into(),
            block: BlockId(i % 9),
            footprint: Footprint {
                x: 0,
                y: 0,
                w: 2,
                d: 2,
            },
            height: 10,
            owner: AccountId::City,
            fit_out: FitOutLevel::Medium,
        };
        businesses
            .by_id
            .insert(BusinessId(i), Business::new(request, kind, Money::ZERO));
    }
    businesses
}

fn year_step(c: &mut Criterion) {
    let config = shipped_config();
    let map = Map::from_config(&config.map);
    let mut group = c.benchmark_group("events_year_step_decimal_day");
    for n in [100u32, 1_000, 10_000] {
        let businesses = shops(&config, n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let mut events = Events::new(42);
                black_box(events.year_step(StepInputs {
                    config: &config,
                    map: &map,
                    businesses: &businesses,
                    year: 1970,
                    day: 8,
                    years_per_step: 5,
                }))
            });
        });
    }
    group.finish();
}

fn demand_query(c: &mut Criterion) {
    let mut modifiers = Modifiers::default();
    for (i, good) in ["clothes", "meals", "drinks", "bread", "groceries"]
        .into_iter()
        .enumerate()
    {
        modifiers.push(
            "bench",
            10,
            Modifier::Demand {
                goods: Some([good.to_owned()].into()),
                class: (i % 2 == 0).then_some(Class::Upper),
                factor: Rate::from_bp(8_000),
            },
        );
    }
    c.bench_function("modifiers_demand_factor_5_in_force", |b| {
        b.iter(|| black_box(modifiers.demand_factor(black_box("bread"), Class::Upper)));
    });
}

criterion_group!(benches, year_step, demand_query);
criterion_main!(benches);
