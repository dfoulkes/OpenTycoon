//! `sim-cli study`: the balance study (R64, R65; docs/study.md).
//!
//! A study samples ~100 economic models (parameter sets) with a Latin hypercube, plays each
//! over hundreds of games with every roster entry rotated through every seat, and measures win
//! shares, evenness, bankruptcy and the idle control seat, so balanced models can be found and
//! adopted. Like the rest of the harness it implements no game rule: each model is a config
//! overlay validated exactly like `--set`, and the games are ordinary `World`s.
//!
//! - [`def`]: the definition file.
//! - [`lhs`]: deterministic Latin hypercube sampling.
//! - [`models`]: models, their overrides and configs.
//! - [`seats`]: the roster and the seat rotation.
//! - [`metrics`]: Wilson intervals, evenness, the R65 flag and the ranking score.
//! - [`run`]: playing a study, in parallel, resumable.
//! - [`output`]: `models.csv`, `games.csv`, `summary.json` and the overlays.

pub mod def;
pub mod lhs;
pub mod metrics;
pub mod models;
pub mod output;
pub mod run;
pub mod seats;

pub use def::{
    Criteria, Distribution, Expectation, IDLE, Kind, Mode, Parameter, StudyDef, StudyError,
};
pub use metrics::{
    EntryMetrics, GameOutcome, ModelMetrics, Tally, gini, metrics, normalised_entropy, wilson,
};
pub use models::{ModelSpec, baseline_values, generate, model_config, overrides_for};
pub use output::split_csv_line;
pub use run::{RunOptions, StudyReport, run_study};
pub use seats::{Entry, GamePlan, Roster, plan, resolve_roster};
