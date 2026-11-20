//! Customer benchmarks on the shipped v1 3×3 map: shop choice for N shoppers × M shops, and
//! one simulated day of the customers phase at 1k and 10k adults. Run with
//! `cargo bench -p sim-core --bench customers`.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benchmark harness"
)]

use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use sim_core::businesses::{BuildRequest, Business, BusinessId, Businesses, FitOutLevel};
use sim_core::clock::{Clock, ClockRules};
use sim_core::config::ConfigFile;
use sim_core::customers::movement::Geometry;
use sim_core::customers::shops::{GoodsIndex, shop_offers};
use sim_core::customers::{Customers, PhaseInputs, choose_shop, run_phase};
use sim_core::economy::AccountId;
use sim_core::land::{BlockId, Footprint, Map};
use sim_core::money::{Money, Rate};
use sim_core::population::{AgeBand, Class, Cohort, Population, Sex};
use sim_core::rng::seeded;
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

/// `per_block` shops of each day and night type on every block, well stocked.
fn high_street(config: &Config, per_block: u32, stock: i64) -> Businesses {
    let mut businesses = Businesses::default();
    let kinds = ["bakery", "grocery", "clothes_store", "bar", "restaurant"];
    for block in 0..9 {
        for slot in 0..per_block {
            for (k, kind) in kinds.iter().enumerate() {
                let good = config.businesses.types[*kind].goods[0].clone();
                let n = businesses.next_id;
                businesses.next_id += 1;
                let i = slot * 5 + u32::try_from(k).unwrap();
                let request = BuildRequest {
                    type_id: (*kind).to_owned(),
                    block: BlockId(block),
                    footprint: Footprint {
                        x: (i % 3) * 3,
                        y: (i / 3 % 3) * 3,
                        w: 3,
                        d: 3,
                    },
                    height: 4,
                    owner: AccountId::City,
                    fit_out: FitOutLevel::Medium,
                };
                let mut b = Business::new(request, &config.businesses.types[*kind], Money::ZERO);
                b.prices
                    .insert(good.clone(), Rate::from_bp(8_000 + 500 * i64::from(i % 8)));
                b.stock.insert(good, stock);
                businesses.by_id.insert(BusinessId(n), b);
            }
        }
    }
    businesses
}

fn population(adults: u64) -> Population {
    let mut pop = Population::default();
    let cells = 4 * 2 * 3;
    for band in 0..4u8 {
        for sex in Sex::ALL {
            for class in Class::ALL {
                pop.pool.insert(
                    Cohort {
                        band: AgeBand(band),
                        sex,
                        class,
                    },
                    adults / cells,
                );
            }
        }
    }
    pop
}

fn bench_choice(c: &mut Criterion) {
    let config = shipped_config();
    let map = Map::from_config(&config.map);
    let [lot_w, lot_d] = config.game.world.lot_size;
    let geometry = Geometry { lot_w, lot_d };
    let goods = GoodsIndex::new(&config.businesses.goods);
    let bread = goods.id("bread").unwrap();
    let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
    while clock.second_of_day() != 12 * 3600 {
        clock.advance();
    }
    let at = geometry.centre(&map.blocks[4]);
    let who = Cohort {
        band: AgeBand(1),
        sex: Sex::Female,
        class: Class::Middle,
    };
    let mut group = c.benchmark_group("shop_choice");
    for per_block in [1u32, 4] {
        let businesses = high_street(&config, per_block, 1_000);
        let offers = shop_offers(&businesses, &map, &config, &clock, &goods, geometry);
        let bakeries = offers.iter().filter(|o| o.good == bread).count();
        let reputation = BTreeMap::new();
        group.bench_function(
            BenchmarkId::new("1000_customers_x_bakeries", bakeries),
            |b| {
                let mut rng = seeded(1);
                b.iter(|| {
                    for _ in 0..1_000 {
                        black_box(choose_shop(
                            who,
                            at,
                            Money::from_pounds(100),
                            bread,
                            &offers,
                            &reputation,
                            &config.customers,
                            geometry,
                            &mut rng,
                        ));
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_day(c: &mut Criterion) {
    let config = shipped_config();
    let map = Map::from_config(&config.map);
    let mut group = c.benchmark_group("customers_day");
    group.sample_size(10);
    for adults in [1_000u64, 10_000] {
        let pop = population(adults);
        group.bench_function(BenchmarkId::new("one_day_8640_ticks", adults), |b| {
            b.iter(|| {
                let mut businesses = high_street(&config, 2, 100_000);
                let mut customers = Customers::default();
                let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
                let mut rng = seeded(42);
                let mut sales = 0usize;
                for _ in 0..8_640 {
                    clock.advance();
                    let report = run_phase(
                        &mut customers,
                        PhaseInputs {
                            config: &config,
                            map: &map,
                            clock: &clock,
                            population: &pop,
                            modifiers: &sim_core::events::Modifiers::default(),
                        },
                        &mut businesses,
                        &mut rng,
                    );
                    sales += report.sales.len();
                }
                black_box(sales)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_choice, bench_day);
criterion_main!(benches);
