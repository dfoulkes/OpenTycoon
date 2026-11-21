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
    let text = read_opt(&opts.from.join("summary.json"))?
        .ok_or_else(|| err(format!("{}: no summary.json", opts.from.display())))?;
    let summary: serde_json::Value = serde_json::from_str(&text).map_err(err)?;
    let mut def: StudyDef = serde_json::from_value(summary["study"]["definition"].clone())
        .map_err(|e| err(format!("summary.json study.definition: {e}")))?;
    let roster: Vec<String> =
        serde_json::from_value(summary["study"]["roster"].clone()).map_err(err)?;
    let seats = roster.len().max(1);
    let blocks = |games: u32| u64::from(effective_games(games, seats)) / seats as u64;
    let (old_first, old_end) = (def.seed, def.seed + blocks(def.games_per_model));
    if let Some(g) = opts.games {
        def.games_per_model = g;
    }
    let new_end = opts.seed.saturating_add(blocks(def.games_per_model));
    if opts.seed < old_end && old_first < new_end {
        return Err(err(format!(
            "seeds {}..{new_end} overlap the study's {old_first}..{old_end}: pick a fresh --seed \
             (at least {old_end})",
            opts.seed
        )));
    }
    def.roster = Some(roster);
    let run = RunOptions {
        out: opts.out.clone(),
        threads: opts.threads,
        games_csv: false,
        wall_time: false,
        fresh: true,
        stop_after: None,
        progress: opts.progress,
        only: Some(opts.models.clone()),
        game_seed: Some(opts.seed),
    };
    let report = run_study(&def, base, &run)?;
    let read = |dir: &PathBuf| -> Result<(Columns, Vec<super::output::Row>), StudyError> {
        let text = read_opt(&dir.join("models.csv"))?
            .ok_or_else(|| err(format!("{}: no models.csv", dir.display())))?;
        let header = text.lines().next().map(split_csv_line).unwrap_or_default();
        let rows = read_models(&text, &header).map_err(|h| err(h.join(",")))?;
        Ok((Columns::new(header), rows))
    };
    let (ocols, orows) = read(&opts.from)?;
    let (ncols, nrows) = read(&opts.out)?;
    let mut out = Vec::new();
    for id in &opts.models {
        let o = orows.iter().find(|r| r.id == *id);
        let n = nrows.iter().find(|r| r.id == *id);
        for m in COMPARED {
            out.push(Compared {
                model_id: *id,
                metric: m.to_owned(),
                original: o.map(|r| ocols.get(r, m).to_owned()).unwrap_or_default(),
                confirm: n.map(|r| ncols.get(r, m).to_owned()).unwrap_or_default(),
            });
        }
    }
    let mut csv = String::from("model_id,metric,original,confirm\n");
    for c in &out {
        let _ = writeln!(
            csv,
            "{},{},{},{}",
            c.model_id, c.metric, c.original, c.confirm
        );
    }
    write(&opts.out.join("confirm.csv"), &csv)?;
    Ok((report, out))
}
