//! `sim-cli bench`: headless throughput of `World::tick`, as machine-readable JSON.
//!
//! Two passes over fresh worlds with the same seed:
//!
//! 1. **Throughput**: one untimed warm-up, then `repeats` runs of `days` game days, timed as a whole (no per-tick clock
//!    reads, which would cost as much as a cheap tick). The median run gives ticks/sec, ms per
//!    game day and the real-time factor (how many times faster than 1× play).
//! 2. **Latency**: one run timing every tick, for the p50/p90/p99/max tick time.
//!
//! The synthetic large scenario is just config: `--agents N` overrides
//! `customers.initial_population`, so it exercises whatever the population and customer phases
//! do with N people. Until those phases land it costs the same as the shipped scenario.

use std::hint::black_box;

use serde::Serialize;
use sim_core::clock::ClockRules;
use sim_core::{Config, World};

use crate::stats::{distribution, percentile};
use crate::timing::Stopwatch;

/// What to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchSpec {
    /// Game days per run.
    pub days: u32,
    /// Throughput runs (the median is reported).
    pub repeats: u32,
    /// Seed.
    pub seed: u64,
    /// Player seats.
    pub players: u8,
}

impl Default for BenchSpec {
    fn default() -> Self {
        BenchSpec {
            days: 1,
            repeats: 5,
            seed: 42,
            players: 2,
        }
    }
}

/// Per-tick latency, nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TickLatency {
    /// Median.
    pub p50_ns: i64,
    /// 90th percentile.
    pub p90_ns: i64,
    /// 99th percentile.
    pub p99_ns: i64,
    /// Slowest tick.
    pub max_ns: i64,
}

/// The JSON `sim-cli bench` prints.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchReport {
    /// `shipped` or `synthetic-<N>-agents`.
    pub scenario: String,
    /// `customers.initial_population` the scenario ran with.
    pub agents: u64,
    /// Player seats.
    pub players: u8,
    /// Game days per run.
    pub days: u32,
    /// Ticks per run.
    pub ticks: u64,
    /// Throughput runs.
    pub repeats: u32,
    /// Median wall time of one run, ms.
    pub wall_ms_median: f64,
    /// Ticks per wall second (median run).
    pub ticks_per_second: f64,
    /// Wall ms to simulate one game day (median run).
    pub ms_per_game_day: f64,
    /// Real ms one game day lasts at 1× (20 minutes by default).
    pub real_ms_per_game_day: u64,
    /// `real_ms_per_game_day / ms_per_game_day`: how many times faster than 1× play.
    pub real_time_factor: f64,
    /// Per-tick latency from the timed-per-tick pass.
    pub tick_latency: TickLatency,
    /// State hash after the run (same for every repeat; proves the runs were the same game).
    pub state_hash: String,
}

fn run_days(config: &Config, spec: &BenchSpec, ticks: u64) -> (f64, u64) {
    let mut world = World::new(config.clone(), spec.seed, spec.players);
    let watch = Stopwatch::start();
    for _ in 0..ticks {
        black_box(world.tick());
    }
    let ms = watch.elapsed().as_secs_f64() * 1_000.0;
    (ms, world.state_hash())
}

/// Runs the benchmark.
#[allow(
    clippy::cast_precision_loss,
    reason = "tick counts and nanoseconds are far below 2^52; rates are for display"
)]
pub fn bench(config: &Config, scenario: &str, spec: &BenchSpec) -> BenchReport {
    let rules = ClockRules::from_config(&config.game.clock);
    let ticks = u64::from(spec.days) * u64::from(rules.ticks_per_day());
    let repeats = spec.repeats.max(1);
    // One untimed warm-up run, so cold caches and a sleeping CPU core don't skew the median.
    let (_, mut hash) = run_days(config, spec, ticks);
    let mut runs: Vec<f64> = Vec::new();
    for _ in 0..repeats {
        let (ms, h) = run_days(config, spec, ticks);
        runs.push(ms);
        hash = h;
    }
    runs.sort_by(f64::total_cmp);
    let wall_ms_median = runs.get(runs.len() / 2).copied().unwrap_or(0.0);

    let mut world = World::new(config.clone(), spec.seed, spec.players);
    let mut per_tick: Vec<i64> = Vec::new();
    for _ in 0..ticks {
        let watch = Stopwatch::start();
        black_box(world.tick());
        per_tick.push(i64::try_from(watch.elapsed().as_nanos()).unwrap_or(i64::MAX));
    }
    per_tick.sort_unstable();
    let latency = distribution(&per_tick);
    let tick_latency = TickLatency {
        p50_ns: latency.map_or(0, |d| d.median),
        p90_ns: latency.map_or(0, |d| d.p90),
        p99_ns: percentile(&per_tick, 99).unwrap_or(0),
        max_ns: latency.map_or(0, |d| d.max),
    };

    let secs = wall_ms_median / 1_000.0;
    let ms_per_game_day = if spec.days == 0 {
        0.0
    } else {
        wall_ms_median / f64::from(spec.days)
    };
    let real_ms = rules.real_millis_per_day();
    BenchReport {
        scenario: scenario.to_owned(),
        agents: config.customers.initial_population,
        players: spec.players,
        days: spec.days,
        ticks,
        repeats,
        wall_ms_median,
        ticks_per_second: if secs > 0.0 { ticks as f64 / secs } else { 0.0 },
        ms_per_game_day,
        real_ms_per_game_day: real_ms,
        real_time_factor: if ms_per_game_day > 0.0 {
            real_ms as f64 / ms_per_game_day
        } else {
            0.0
        },
        tick_latency,
        state_hash: format!("{hash:016x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_day_bench_counts_ticks_and_is_internally_consistent() {
        let config = crate::test_support::shipped_config();
        let spec = BenchSpec {
            days: 1,
            repeats: 1,
            ..BenchSpec::default()
        };
        let r = bench(&config, "shipped", &spec);
        assert_eq!(r.ticks, 8_640);
        assert_eq!(r.real_ms_per_game_day, 20 * 60 * 1_000);
        assert!(r.ticks_per_second > 0.0);
        assert!(r.tick_latency.p50_ns <= r.tick_latency.p99_ns);
        assert!(r.tick_latency.p99_ns <= r.tick_latency.max_ns);
        let expected_rtf =
            f64::from(u32::try_from(r.real_ms_per_game_day).unwrap()) / r.ms_per_game_day;
        assert!((r.real_time_factor - expected_rtf).abs() < 1e-6);
    }

    #[test]
    fn the_hash_matches_a_plain_run_of_the_same_ticks() {
        let config = crate::test_support::shipped_config();
        let r = bench(
            &config,
            "shipped",
            &BenchSpec {
                days: 1,
                repeats: 2,
                ..BenchSpec::default()
            },
        );
        let mut world = World::new(config, 42, 2);
        for _ in 0..8_640 {
            world.tick();
        }
        assert_eq!(r.state_hash, format!("{:016x}", world.state_hash()));
    }
}
