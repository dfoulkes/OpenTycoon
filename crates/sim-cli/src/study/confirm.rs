//! `sim-cli study confirm`: re-runs chosen models of a finished study on fresh seeds and puts
//! their R65 and fun (R79) metrics next to the study's, to guard against the winner's curse
//! (the best of 300 noisy estimates is flattered by its own noise).
//!
//! The models are regenerated from the study's definition (read from its `summary.json`), so
//! they are exactly the study's models; only the game seeds change. Run it on the same base
//! config (config directory and `--set`) as the study, or the models' overrides land on a
//! different base.

use std::fmt::Write as _;
use std::path::PathBuf;

use sim_core::ConfigSources;

use super::def::{StudyDef, StudyError};
use super::output::{Columns, read_models, split_csv_line};
use super::run::{RunOptions, StudyReport, read_opt, run_study, write};
use super::seats::effective_games;

/// The metrics compared, in order.
pub const COMPARED: [&str; 13] = [
    "balanced",
    "max_win_share",
    "min_win_share",
    "ai_bankruptcy_rate",
    "idle_below_solvent_rate",
    "fun_upset_rate",
    "fun_lead_changes_mean",
    "fun_comeback_rate",
    "fun_margin_median",
    "fun_distinctness",
    "fun_streak_z",
    "fun_score",
    "rank",
];

/// How to confirm.
#[derive(Debug, Clone, Default)]
pub struct ConfirmOptions {
    /// The study's output directory.
    pub from: PathBuf,
    /// The model ids to re-run.
    pub models: Vec<u32>,
    /// Games per model (rounded up to whole seat rotations); `None` keeps the study's.
    pub games: Option<u32>,
    /// First game seed. Must not overlap the study's seeds.
    pub seed: u64,
    /// Output directory: the re-run's `models.csv` and `summary.json`, and `confirm.csv`.
    pub out: PathBuf,
    /// Worker threads; `None` uses every core.
    pub threads: Option<usize>,
    /// Print progress to stderr.
    pub progress: bool,
}

/// One compared value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compared {
    /// Model id.
    pub model_id: u32,
    /// Metric (a `models.csv` column).
    pub metric: String,
    /// The study's value (empty if the study has no such column).
    pub original: String,
    /// The re-run's value.
    pub confirm: String,
}

fn err(e: impl std::fmt::Display) -> StudyError {
    StudyError::def("confirm", e.to_string())
}

/// Re-runs `opts.models` of the study in `opts.from` on fresh seeds and writes `confirm.csv`
/// (`model_id, metric, original, confirm`, one row per model and metric of [`COMPARED`]).
pub fn confirm(
    base: &ConfigSources,
    opts: &ConfirmOptions,
) -> Result<(StudyReport, Vec<Compared>), StudyError> {
    // red: not yet implemented
    let _ = (base, opts);
    Err(err("study confirm is not implemented yet"))
}
