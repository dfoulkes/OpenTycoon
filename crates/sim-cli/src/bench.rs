//! `sim-cli bench`: headless throughput of `World::tick`, as machine-readable JSON.
//!
//! Two passes over fresh worlds with the same seed:
//!
//! 1. **Throughput**: one untimed warm-up, then `repeats` runs of `days` game days, timed as a whole (no per-tick clock
//!    reads, which would cost as much as a cheap tick). The median run gives ticks/sec, ms per
//!    game day and the real-time factor (how many times faster than 1× play).
//! 2. **Latency**: `repeats` runs timing every tick; each percentile (p50/p90/p99/p99.9/max)
//!    is the median over the runs, so one preempted tick on a busy machine does not become the
//!    reported max.
//! 3. **Phases** (`--phases`, optional): one run through [`World::tick_observed`], reading the
//!    clock after every phase, for where a day's time goes. The clock reads cost tens of ns
//!    each, so the phase totals sit a little above the throughput number; compare phases with
//!    each other, not with the day budget.
//!
//! The synthetic large scenario is just config: `--agents N` overrides
//! `customers.initial_population`, so it exercises whatever the population and customer phases
//! do with N people.

use std::collections::BTreeMap;
use std::hint::black_box;

use serde::Serialize;
use sim_core::clock::ClockRules;
use sim_core::world::Phase;
use sim_core::{Config, World};

use crate::stats::percentile;
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
    /// Also time each tick phase (one extra run).
    pub phases: bool,
}

impl Default for BenchSpec {
    fn default() -> Self {
        BenchSpec {
            days: 1,
            repeats: 5,
            seed: 42,
            players: 2,
            phases: false,
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
    /// 99.9th percentile.
    pub p999_ns: i64,
    /// Slowest tick.
    pub max_ns: i64,
}

/// Where one phase's time went in the `--phases` run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PhaseTime {
    /// Wall ms spent in the phase per game day.
    pub ms_per_day: f64,
    /// Share of the summed phase time, 0–1.
    pub share: f64,
    /// Ticks per game day on which the phase ran.
    pub runs_per_day: f64,
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
    /// Mean wall µs per tick (median run): `ms_per_game_day × 1000 / ticks per day`.
    pub tick_mean_us: f64,
    /// Wall ms to simulate one game day (median run).
    pub ms_per_game_day: f64,
    /// Real ms one game day lasts at 1× (20 minutes by default).
    pub real_ms_per_game_day: u64,
    /// `real_ms_per_game_day / ms_per_game_day`: how many times faster than 1× play.
    pub real_time_factor: f64,
    /// Per-tick latency from the timed-per-tick passes.
    pub tick_latency: TickLatency,
    /// Per-phase breakdown, with `--phases`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phases: Option<BTreeMap<Phase, PhaseTime>>,
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

/// One run timing every tick, sorted ascending, ns.
fn tick_times(config: &Config, spec: &BenchSpec, ticks: u64) -> Vec<i64> {
    let mut world = World::new(config.clone(), spec.seed, spec.players);
    let mut per_tick: Vec<i64> = Vec::new();
    for _ in 0..ticks {
        let watch = Stopwatch::start();
        black_box(world.tick());
        per_tick.push(i64::try_from(watch.elapsed().as_nanos()).unwrap_or(i64::MAX));
    }
    per_tick.sort_unstable();
    per_tick
}

/// The nearest-rank `per_mille`-th per-mille of `sorted` (ascending), as [`percentile`] does
/// for per cent.
fn per_mille(sorted: &[i64], per_mille: u64) -> i64 {
    let n = sorted.len() as u64;
    if n == 0 {
        return 0;
    }
    let rank = (per_mille.min(1_000) * n).div_ceil(1_000).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted.get(index).copied().unwrap_or(0)
}

/// Median of `values` (lower middle), 0 if empty.
fn median(mut values: Vec<i64>) -> i64 {
    values.sort_unstable();
    values
        .get(values.len().saturating_sub(1) / 2)
        .copied()
        .unwrap_or(0)
}

/// `repeats` latency runs; each percentile is the median over the runs.
fn latency(config: &Config, spec: &BenchSpec, ticks: u64, repeats: u32) -> TickLatency {
    let runs: Vec<Vec<i64>> = (0..repeats)
        .map(|_| tick_times(config, spec, ticks))
        .collect();
    let pick = |f: &dyn Fn(&[i64]) -> i64| median(runs.iter().map(|r| f(r)).collect());
    TickLatency {
        p50_ns: pick(&|r| percentile(r, 50).unwrap_or(0)),
        p90_ns: pick(&|r| percentile(r, 90).unwrap_or(0)),
        p99_ns: pick(&|r| percentile(r, 99).unwrap_or(0)),
        p999_ns: pick(&|r| per_mille(r, 999)),
        max_ns: pick(&|r| r.last().copied().unwrap_or(0)),
    }
}

/// One run through `tick_observed`, reading the clock after every phase.
#[allow(
    clippy::cast_precision_loss,
    reason = "nanosecond totals and tick counts are far below 2^52; for display"
)]
fn phase_times(config: &Config, spec: &BenchSpec, ticks: u64) -> BTreeMap<Phase, PhaseTime> {
    let mut world = World::new(config.clone(), spec.seed, spec.players);
    let mut total: BTreeMap<Phase, (u128, u64)> = BTreeMap::new();
    for _ in 0..ticks {
        let mut last = Stopwatch::start();
        black_box(world.tick_observed(&mut |phase| {
            let now = Stopwatch::start();
            let entry = total.entry(phase).or_insert((0, 0));
            entry.0 += now.since(&last).as_nanos();
            entry.1 += 1;
            last = now;
        }));
    }
    let days = f64::from(spec.days.max(1));
    let sum: u128 = total.values().map(|(ns, _)| ns).sum();
    total
        .into_iter()
        .map(|(phase, (ns, runs))| {
            let time = PhaseTime {
                ms_per_day: ns as f64 / 1e6 / days,
                share: if sum == 0 {
                    0.0
                } else {
                    ns as f64 / sum as f64
                },
                runs_per_day: runs as f64 / days,
            };
            (phase, time)
        })
        .collect()
}

/// Runs the benchmark.
#[allow(
    clippy::cast_precision_loss,
    reason = "tick counts and nanoseconds are far below 2^52; rates are for display"
)]
pub fn bench(config: &Config, scenario: &str, spec: &BenchSpec) -> BenchReport {
    let rules = ClockRules::from_config(&config.game.clock);
    let ticks_per_day = u64::from(rules.ticks_per_day());
    let ticks = u64::from(spec.days) * ticks_per_day;
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

    let tick_latency = latency(config, spec, ticks, repeats);
    let phases = spec.phases.then(|| phase_times(config, spec, ticks));

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
        tick_mean_us: if ticks_per_day == 0 {
            0.0
        } else {
            ms_per_game_day * 1_000.0 / ticks_per_day as f64
        },
        ms_per_game_day,
        real_ms_per_game_day: real_ms,
        real_time_factor: if ms_per_game_day > 0.0 {
            real_ms as f64 / ms_per_game_day
        } else {
            0.0
        },
        tick_latency,
        phases,
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

    #[test]
    fn the_mean_tick_is_the_day_per_tick_and_phases_cover_the_tick() {
        let config = crate::test_support::shipped_config();
        let r = bench(
            &config,
            "shipped",
            &BenchSpec {
                days: 1,
                repeats: 1,
                phases: true,
                ..BenchSpec::default()
            },
        );
        assert!((r.tick_mean_us * 8_640.0 / 1_000.0 - r.ms_per_game_day).abs() < 1e-9);
        assert!(r.tick_latency.p99_ns <= r.tick_latency.p999_ns);
        assert!(r.tick_latency.p999_ns <= r.tick_latency.max_ns);
        let phases = r.phases.expect("asked for phases");
        assert_eq!(phases.len(), Phase::ALL.len());
        assert!((phases.values().map(|p| p.share).sum::<f64>() - 1.0).abs() < 1e-9);
        assert!((phases[&Phase::Customers].runs_per_day - 8_640.0).abs() < f64::EPSILON);
        assert!((phases[&Phase::Midnight].runs_per_day - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn per_mille_and_median_use_nearest_rank() {
        let sorted: Vec<i64> = (1..=1_000).collect();
        assert_eq!(per_mille(&sorted, 999), 999);
        assert_eq!(per_mille(&sorted, 1_000), 1_000);
        assert_eq!(per_mille(&[], 999), 0);
        assert_eq!(median(vec![5, 1, 3]), 3);
        assert_eq!(median(vec![4, 1, 3, 2]), 2);
        assert_eq!(median(Vec::new()), 0);
    }
}
