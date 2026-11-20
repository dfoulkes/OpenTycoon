//! `sim-cli perf-budget`: measure the sim and fail if it is over budget.
//!
//! Budgets live in `perf/budgets.yaml` (docs/perf.md explains each). A metric passes while
//! `measured ≤ limit × (1 + tolerance)`. Each budget also records the baseline measured when it
//! was set; drifting more than `tolerance` above the baseline is reported as a warning (flag it
//! in the PR, docs/engineering.md §4) but only the limit fails the run.
//!
//! This is deliberately **not** part of `cargo test`: shared CI runners are slower and noisier
//! than the machine the budgets were set on, so it runs on demand (`cargo perf`) or in a job
//! that opts in.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sim_core::ConfigSources;

use crate::batch::{BatchSpec, run_batch};
use crate::bench::{BenchSpec, bench};
use crate::game::GameSpec;
use crate::overrides::{self, LoadError};
use crate::timing::Stopwatch;

/// Something with a budget. All are "lower is better" upper limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Wall ms to simulate one game day, shipped config, v1 preset, 2 players.
    GameDayMs,
    /// p99 tick time, µs, same scenario.
    TickP99Us,
    /// p99 tick time, µs, with `customers.initial_population = 10000`.
    #[serde(rename = "tick_p99_us_10k_agents")]
    TickP99Us10kAgents,
    /// Seconds for 1,000 v1 games on every core, extrapolated from a sample batch.
    #[serde(rename = "batch_1000_games_s")]
    Batch1000GamesS,
    /// ms to load and validate the shipped config.
    ConfigLoadMs,
}

impl Metric {
    /// Every metric.
    pub const ALL: [Metric; 5] = [
        Metric::GameDayMs,
        Metric::TickP99Us,
        Metric::TickP99Us10kAgents,
        Metric::Batch1000GamesS,
        Metric::ConfigLoadMs,
    ];

    /// Its name in `budgets.yaml`.
    pub const fn name(self) -> &'static str {
        match self {
            Metric::GameDayMs => "game_day_ms",
            Metric::TickP99Us => "tick_p99_us",
            Metric::TickP99Us10kAgents => "tick_p99_us_10k_agents",
            Metric::Batch1000GamesS => "batch_1000_games_s",
            Metric::ConfigLoadMs => "config_load_ms",
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Metric {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Metric::ALL
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| {
                let known: Vec<&str> = Metric::ALL.iter().map(|m| m.name()).collect();
                format!("unknown metric `{s}` (known: {})", known.join(", "))
            })
    }
}

/// One budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Upper limit.
    pub limit: f64,
    /// What it measured on the reference machine when the budget was set.
    pub baseline: Option<f64>,
    /// Where the limit comes from.
    #[serde(default)]
    pub why: String,
}

/// `perf/budgets.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// Allowed overshoot as a fraction (0.2 = 20%).
    pub tolerance: f64,
    /// The machine the baselines were measured on.
    #[serde(default)]
    pub reference_machine: String,
    /// Per metric.
    pub budgets: BTreeMap<Metric, Budget>,
}

impl Budgets {
    /// Parses `budgets.yaml`.
    pub fn from_yaml(text: &str) -> Result<Self, String> {
        let b: Budgets = serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
        if !(b.tolerance.is_finite() && b.tolerance >= 0.0) {
            return Err(format!("tolerance must be ≥ 0, got {}", b.tolerance));
        }
        if let Some((m, _)) = b
            .budgets
            .iter()
            .find(|(_, v)| !(v.limit.is_finite() && v.limit >= 0.0))
        {
            return Err(format!("budget {m}: limit must be a number ≥ 0"));
        }
        Ok(b)
    }
}

/// Pass or fail for one metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Within `limit × (1 + tolerance)`.
    Pass,
    /// Over it.
    Fail,
}

/// The verdict on one metric.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    /// Metric.
    pub metric: Metric,
    /// Measured value.
    pub measured: f64,
    /// Budget limit.
    pub limit: f64,
    /// `limit × (1 + tolerance)`.
    pub allowed: f64,
    /// Baseline, if recorded.
    pub baseline: Option<f64>,
    /// True if more than `tolerance` above the baseline (a warning, not a failure).
    pub drifted: bool,
    /// Pass or fail.
    pub status: Status,
}

/// Compares measurements against budgets. Metrics without a measurement are skipped.
pub fn check(budgets: &Budgets, measured: &BTreeMap<Metric, f64>) -> Vec<Verdict> {
    budgets
        .budgets
        .iter()
        .filter_map(|(metric, budget)| {
            let value = *measured.get(metric)?;
            let allowed = budget.limit * (1.0 + budgets.tolerance);
            Some(Verdict {
                metric: *metric,
                measured: value,
                limit: budget.limit,
                allowed,
                baseline: budget.baseline,
                drifted: budget
                    .baseline
                    .is_some_and(|b| value > b * (1.0 + budgets.tolerance)),
                status: if value <= allowed {
                    Status::Pass
                } else {
                    Status::Fail
                },
            })
        })
        .collect()
}

/// True if every verdict passed.
pub fn all_pass(verdicts: &[Verdict]) -> bool {
    verdicts.iter().all(|v| v.status == Status::Pass)
}

/// Renders verdicts as a table.
pub fn table(verdicts: &[Verdict], tolerance: f64) -> String {
    let mut out = format!(
        "{:<24} {:>12} {:>12} {:>12} {:>12}  status (tolerance {:.0}%)\n",
        "metric",
        "measured",
        "limit",
        "allowed",
        "baseline",
        tolerance * 100.0
    );
    for v in verdicts {
        let baseline = v
            .baseline
            .map_or_else(|| "-".to_owned(), |b| format!("{b:.4}"));
        let status = match (v.status, v.drifted) {
            (Status::Pass, false) => "ok",
            (Status::Pass, true) => "ok (drifted above baseline: flag in the PR)",
            (Status::Fail, _) => "OVER BUDGET",
        };
        let _ = writeln!(
            out,
            "{:<24} {:>12.4} {:>12.4} {:>12.4} {:>12}  {status}",
            v.metric.name(),
            v.measured,
            v.limit,
            v.allowed,
            baseline
        );
    }
    out
}

/// How hard to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureSpec {
    /// Games in the sample batch that `batch_1000_games_s` extrapolates from.
    pub batch_sample: u64,
    /// Throughput repeats per bench (median taken).
    pub repeats: u32,
    /// Preset the scenarios run.
    pub preset: &'static str,
}

impl Default for MeasureSpec {
    fn default() -> Self {
        MeasureSpec {
            batch_sample: 200,
            repeats: 5,
            preset: "v1",
        }
    }
}

/// Why a measurement could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    /// Config or override problem.
    #[error(transparent)]
    Load(#[from] LoadError),
    /// Batch problem.
    #[error(transparent)]
    Batch(#[from] crate::batch::BatchError),
}

/// Measures the given metrics on the config in `sources`.
#[allow(
    clippy::cast_precision_loss,
    reason = "game counts and nanoseconds are far below 2^52; budgets are for display"
)]
pub fn measure(
    sources: &ConfigSources,
    metrics: &[Metric],
    spec: &MeasureSpec,
) -> Result<BTreeMap<Metric, f64>, MeasureError> {
    let mut out = BTreeMap::new();
    let config = overrides::load(sources, Some(spec.preset), &[])?;
    let bench_spec = BenchSpec {
        days: 1,
        repeats: spec.repeats,
        ..BenchSpec::default()
    };
    if metrics.contains(&Metric::GameDayMs) || metrics.contains(&Metric::TickP99Us) {
        let r = bench(&config, "shipped", &bench_spec);
        out.insert(Metric::GameDayMs, r.ms_per_game_day);
        out.insert(Metric::TickP99Us, r.tick_latency.p99_ns as f64 / 1_000.0);
    }
    if metrics.contains(&Metric::TickP99Us10kAgents) {
        let big = overrides::load(
            sources,
            Some(spec.preset),
            &["customers.initial_population=10000".parse()?],
        )?;
        let r = bench(&big, "synthetic-10000-agents", &bench_spec);
        out.insert(
            Metric::TickP99Us10kAgents,
            r.tick_latency.p99_ns as f64 / 1_000.0,
        );
    }
    if metrics.contains(&Metric::Batch1000GamesS) && spec.batch_sample > 0 {
        let report = run_batch(
            &config,
            &BatchSpec {
                games: spec.batch_sample,
                base_seed: 0,
                threads: None,
                game: GameSpec::default(),
            },
        )?;
        let per_game = report.wall.as_secs_f64() / spec.batch_sample as f64;
        out.insert(Metric::Batch1000GamesS, per_game * 1_000.0);
    }
    if metrics.contains(&Metric::ConfigLoadMs) {
        let mut runs: Vec<f64> = (0..21)
            .map(|_| {
                let watch = Stopwatch::start();
                let loaded = sim_core::Config::load(sources);
                let ms = watch.elapsed().as_secs_f64() * 1_000.0;
                std::hint::black_box(loaded.is_ok());
                ms
            })
            .collect();
        runs.sort_by(f64::total_cmp);
        out.insert(
            Metric::ConfigLoadMs,
            runs.get(runs.len() / 2).copied().unwrap_or(0.0),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budgets(limit: f64, baseline: Option<f64>) -> Budgets {
        Budgets {
            tolerance: 0.2,
            reference_machine: String::new(),
            budgets: [(
                Metric::GameDayMs,
                Budget {
                    limit,
                    baseline,
                    why: String::new(),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn measured(v: f64) -> BTreeMap<Metric, f64> {
        [(Metric::GameDayMs, v)].into_iter().collect()
    }

    #[test]
    fn within_limit_passes_over_limit_plus_tolerance_fails() {
        assert!(all_pass(&check(&budgets(10.0, None), &measured(9.0))));
        // 11.9 ≤ 10 × 1.2: inside the tolerance.
        assert!(all_pass(&check(&budgets(10.0, None), &measured(11.9))));
        let v = check(&budgets(10.0, None), &measured(12.1));
        assert!(!all_pass(&v));
        assert_eq!(v[0].status, Status::Fail);
        assert!(table(&v, 0.2).contains("OVER BUDGET"));
    }

    #[test]
    fn drift_above_baseline_warns_but_passes() {
        let v = check(&budgets(50.0, Some(1.0)), &measured(1.5));
        assert!(all_pass(&v));
        assert!(v[0].drifted);
        let v = check(&budgets(50.0, Some(1.0)), &measured(1.1));
        assert!(!v[0].drifted);
    }

    #[test]
    fn unmeasured_metrics_are_skipped() {
        assert_eq!(check(&budgets(1.0, None), &BTreeMap::new()).len(), 0);
    }

    #[test]
    fn yaml_rejects_unknown_metrics_and_bad_numbers() {
        let ok = "tolerance: 0.2\nbudgets:\n  game_day_ms: { limit: 50, baseline: 0.2 }\n";
        assert_eq!(Budgets::from_yaml(ok).unwrap().budgets.len(), 1);
        let err = Budgets::from_yaml("tolerance: 0.2\nbudgets:\n  gaem_day_ms: { limit: 1 }\n")
            .unwrap_err();
        assert!(err.contains("gaem_day_ms"), "{err}");
        assert!(Budgets::from_yaml("tolerance: -1\nbudgets: {}\n").is_err());
        assert!(
            Budgets::from_yaml("tolerance: 0\nbudgets:\n  game_day_ms: { limit: -3 }\n").is_err()
        );
    }

    #[test]
    fn metric_names_round_trip() {
        for m in Metric::ALL {
            assert_eq!(m.name().parse::<Metric>(), Ok(m));
            let yaml = serde_yaml_ng::to_string(&m).unwrap();
            assert_eq!(yaml.trim(), m.name());
        }
    }

    #[test]
    fn the_shipped_budgets_file_parses_and_covers_every_metric() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../perf/budgets.yaml");
        let b = Budgets::from_yaml(&std::fs::read_to_string(path).unwrap()).unwrap();
        for m in Metric::ALL {
            assert!(b.budgets.contains_key(&m), "{m} has no budget");
        }
    }
}
