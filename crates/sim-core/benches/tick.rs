//! Tick benchmarks on the shipped v1 3×3 map. Run with `cargo bench -p sim-core`.
//! Budgets live in docs/engineering.md §Performance; record results there as features land.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benchmark harness"
)]

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use sim_core::config::ConfigFile;
use sim_core::{Config, ConfigSources, World};

fn shipped_config() -> Config {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        let text = std::fs::read_to_string(dir.join(file.file_name())).expect("config file");
        sources.insert(file.file_name(), text);
    }
    Config::load(&sources).expect("shipped config is valid")
}

fn bench_tick(c: &mut Criterion) {
    let config = shipped_config();
    let mut group = c.benchmark_group("tick_3x3");
    group.bench_function("one_tick", |b| {
        let mut world = World::new(config.clone(), 42, 2);
        b.iter(|| black_box(world.tick()));
    });
    group.sample_size(10);
    group.bench_function("one_day_8640_ticks", |b| {
        b.iter(|| {
            let mut world = World::new(config.clone(), 42, 2);
            for _ in 0..8_640 {
                black_box(world.tick());
            }
            black_box(world.state_hash())
        });
    });
    group.finish();
}

fn bench_config_load(c: &mut Criterion) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        sources.insert(
            file.file_name(),
            std::fs::read_to_string(dir.join(file.file_name())).unwrap(),
        );
    }
    c.bench_function("config_load_and_validate", |b| {
        b.iter(|| black_box(Config::load(&sources).unwrap()));
    });
}

criterion_group!(benches, bench_tick, bench_config_load);
criterion_main!(benches);
