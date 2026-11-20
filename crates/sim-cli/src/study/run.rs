//! Running a study.

use std::path::PathBuf;

use sim_core::ConfigSources;

use super::def::{StudyDef, StudyError};

/// How to run a study.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunOptions {
    /// Output directory.
    pub out: PathBuf,
    /// Worker threads.
    pub threads: Option<usize>,
    /// Write games.csv.
    pub games_csv: bool,
    /// Include wall-time columns.
    pub wall_time: bool,
    /// Discard an existing study in `out`.
    pub fresh: bool,
    /// Stop after this many new models.
    pub stop_after: Option<u32>,
    /// Progress to stderr.
    pub progress: bool,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StudyReport {
    /// Models run now.
    pub ran: u32,
    /// Models already finished.
    pub skipped: u32,
    /// Invalid models.
    pub invalid: u32,
    /// Every model done.
    pub complete: bool,
}

/// Runs a study.
pub fn run_study(
    def: &StudyDef,
    base: &ConfigSources,
    opts: &RunOptions,
) -> Result<StudyReport, StudyError> {
    let _ = (def, base, opts);
    todo!("run_study")
}
