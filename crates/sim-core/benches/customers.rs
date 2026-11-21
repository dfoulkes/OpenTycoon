//! Customer benchmarks on the shipped v1 3×3 map: shop choice for N shoppers × M shops, and
//! one simulated day of the customers phase at 1k and 10k adults: the plain 1930 day, a day
//! with a Hotel in every set (tourists' nights, §7.3), and a 2020 day with online sellers
//! (§7.5). Run with `cargo bench -p sim-core --bench customers`.
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
use sim_core::config::businesses::Channel;
use sim_core::customers::shops::{GoodsIndex, shop_offers};
use sim_core::customers::streets::Streets;
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
    let streets = Streets::for_config(&map, &config);
    let goods = GoodsIndex::new(&config.businesses.goods);
    let bread = goods.id("bread").unwrap();
    let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
    while clock.second_of_day() != 12 * 3600 {
        clock.advance();
    }
    let at = streets.entrance(&map.blocks[4]);
    let who = Cohort {
        band: AgeBand(1),
        sex: Sex::Female,
        class: Class::Middle,
    };
    let mut group = c.benchmark_group("shop_choice");
    for per_block in [1u32, 4] {
        let businesses = high_street(&config, per_block, 1_000);
        let offers = shop_offers(&businesses, &map, &config, &clock, &goods, &streets);
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
                            &config,
                            &streets,
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
    let streets = Streets::for_config(&map, &config);
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
                            streets: &streets,
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

/// One day of the customers phase over `businesses`, returning sales + check-ins.
fn one_day(config: &Config, map: &Map, pop: &Population, mut businesses: Businesses) -> usize {
    let streets = Streets::for_config(map, config);
    let mut customers = Customers::default();
    let mut clock = Clock::new(ClockRules::from_config(&config.game.clock));
    let mut rng = seeded(42);
    let mut done = 0usize;
    for _ in 0..8_640 {
        clock.advance();
        let report = run_phase(
            &mut customers,
            PhaseInputs {
                config,
                map,
                clock: &clock,
                population: pop,
                modifiers: &sim_core::events::Modifiers::default(),
                streets: &streets,
            },
            &mut businesses,
            &mut rng,
        );
        done += report.sales.len() + report.online_sales.len() + report.check_ins.len();
    }
    done
}

/// Adds a 3×3×`height` building of `type_id` on `block` in the lot's last free slot.
fn add(
    config: &Config,
    businesses: &mut Businesses,
    type_id: &str,
    block: u32,
    height: u32,
) -> BusinessId {
    let request = BuildRequest {
        type_id: type_id.to_owned(),
        block: BlockId(block),
        footprint: Footprint {
            x: 6,
            y: 6,
            w: 3,
            d: 3,
        },
        height,
        owner: AccountId::City,
        fit_out: FitOutLevel::Medium,
    };
    let id = BusinessId(businesses.next_id);
    businesses.next_id += 1;
    businesses.by_id.insert(
        id,
        Business::new(request, &config.businesses.types[type_id], Money::ZERO),
    );
    id
}

fn bench_day_with_hotels(c: &mut Criterion) {
    let config = shipped_config();
    let map = Map::from_config(&config.map);
    let mut group = c.benchmark_group("customers_day_with_hotels");
    group.sample_size(10);
    for adults in [1_000u64, 10_000] {
        let pop = population(adults);
        let mut businesses = high_street(&config, 2, 100_000);
        // One Hotel per set (rows 0, 1, 2 of the 3×3 map), 3×3×40 = 90 rooms each.
        for block in [1, 4, 7] {
            add(&config, &mut businesses, "hotel", block, 40);
        }
        group.bench_function(BenchmarkId::new("one_day_8640_ticks", adults), |b| {
            b.iter(|| black_box(one_day(&config, &map, &pop, businesses.clone())));
        });
    }
    group.finish();
}

fn bench_day_with_online(c: &mut Criterion) {
    let mut config = shipped_config();
    config.game.clock.start_year = 2020;
    config.businesses.types.insert("web_bakery".into(), {
        let mut t = config.businesses.types["bakery"].clone();
        t.channel = Channel::Online;
        t.max_height = 40;
        t
    });
    let map = Map::from_config(&config.map);
    let mut group = c.benchmark_group("customers_day_with_online_2020");
    group.sample_size(10);
    for adults in [1_000u64, 10_000] {
        let pop = population(adults);
        let mut businesses = high_street(&config, 2, 100_000);
        for block in [0, 2] {
            let id = add(&config, &mut businesses, "web_bakery", block, 40);
            businesses
                .by_id
                .get_mut(&id)
                .unwrap()
                .stock
                .insert("bread".into(), 100_000);
        }
        group.bench_function(BenchmarkId::new("one_day_8640_ticks", adults), |b| {
            b.iter(|| black_box(one_day(&config, &map, &pop, businesses.clone())));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_choice,
    bench_day,
    bench_day_with_hotels,
    bench_day_with_online
);
criterion_main!(benches);
