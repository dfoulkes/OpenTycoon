//! `sim-cli study rescore`: the fun metrics (R79) of a recorded study, from its manifests and
//! replays, without playing a game.
//!
//! Every replay holds a checkpoint at each midnight (each seat's net worth, cash, leases,
//! businesses and bankruptcy) and the outcome, which is everything the per-game fun data needs
//! except the full holdings mix: a checkpoint only counts leases and businesses, so a rescored
//! study measures playstyle distinctness on those (`playstyle: checkpoint`).
//!
//! Two modes:
//!
//! - **with the study's output directory** (`models.csv`, `summary.json`): its rows are kept as
//!   they are (parameters, R65 metrics), the fun columns are replaced or appended, and the
//!   models are re-ranked. The definition, roster and bands come from `summary.json`, the bands
//!   optionally from another definition file;
//! - **from the recordings alone**: every metric, R65 included, is computed from the outcomes;
//!   `models.csv` then has no parameter columns.
//!
//! Each model's games come from its manifest part (`part-m<id>`); parts are read in parallel and
//! results are collected in model order, so the output is identical on any thread count.

use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::{Value as Json, json};

use super::model::{FunMetrics, fun_metrics};
use super::score::Playstyle;
use crate::replay::format::{CORE_BRAIN, Replay, Winner};
use crate::replay::load;
use crate::replay::store::ObjectStore;
use crate::study::def::{IDLE, StudyDef, StudyError};
use crate::study::metrics::{GameOutcome, Tally, metrics};
use crate::study::models::ModelSpec;
use crate::study::output::{
    Columns, FUN_COLUMNS, Outcome, Row, apply_fun_scores, csv_text, fun_fields, model_row,
    models_header, rank, ranking_json, read_models, split_csv_line,
};
use crate::study::run::{boards_of, read_opt, write};
use crate::study::seats::{Entry, Roster};

use super::FunGame;
use super::game::{Holdings, fun_game};

/// How to rescore.
#[derive(Debug, Clone, Default)]
pub struct RescoreOptions {
    /// The recording's run id.
    pub run_id: String,
    /// The study's output directory, if there is one.
    pub study: Option<PathBuf>,
    /// A definition whose `fun:` bands replace the study's (or, without a study directory, the
    /// whole definition: criteria, roster order, `top_n` and bands).
    pub def: Option<StudyDef>,
    /// Where to write `models.csv` and `summary.json`.
    pub out: PathBuf,
    /// Worker threads; `None` uses every core. Output is identical for any value.
    pub threads: Option<usize>,
}

/// What a rescore did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescoreReport {
    /// Models rescored from their recordings.
    pub models: usize,
    /// Games read.
    pub games: u64,
    /// Ok models of the study with no manifest part in the recording (fun columns left empty).
    pub missing: Vec<u32>,
    /// The ranking, best first.
    pub order: Vec<u32>,
}

fn err(e: impl std::fmt::Display) -> StudyError {
    StudyError::Record(e.to_string())
}

/// The model ids with a manifest part in `run_id`, ascending.
fn parts(store: &dyn ObjectStore, run_id: &str) -> Result<Vec<(u32, String)>, StudyError> {
    let prefix = format!("manifests/{run_id}/");
    let mut out: Vec<(u32, String)> = store
        .list(&prefix)
        .map_err(err)?
        .into_iter()
        .filter_map(|(key, _)| {
            let name = key.strip_prefix(&prefix)?;
            let id = name.strip_prefix("part-m")?.strip_suffix(".parquet")?;
            Some((id.parse().ok()?, key.clone()))
        })
        .collect();
    out.sort();
    Ok(out)
}

/// The one study run in `store` (a run id starting `study-` with `part-m*` manifests), for a
/// study directory written before `summary.json` named its run. Errors listing the candidates
/// if there is not exactly one.
pub fn find_study_run(store: &dyn ObjectStore) -> Result<String, StudyError> {
    let mut runs: Vec<String> = store
        .list("manifests/")
        .map_err(err)?
        .into_iter()
        .filter_map(|(key, _)| {
            let rest = key.strip_prefix("manifests/")?;
            let (run, part) = rest.split_once('/')?;
            part.starts_with("part-m").then(|| run.to_owned())
        })
        .collect();
    runs.dedup();
    match runs.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(err(
            "no study run in the recordings (no manifests/<run>/part-m*.parquet)",
        )),
        many => Err(err(format!(
            "several study runs in the recordings ({}): name one with --run-id",
            many.join(", ")
        ))),
    }
}

/// The roster entry a recorded seat played: `core:<personality>` or a harness brain (the idle
/// seat).
fn entry_of(brain: &str) -> Entry {
    match brain.strip_prefix(CORE_BRAIN) {
        Some(p) => Entry::from_name(p),
        None => Entry::from_name(IDLE),
    }
}

/// A recorded game as the study sees it, with its fun data from the checkpoints.
fn outcome_of(replay: &Replay, roster: &Roster) -> Result<GameOutcome, StudyError> {
    let h = &replay.header;
    let index: u32 = h
        .game_id
        .split_once("-g")
        .and_then(|(_, g)| g.parse().ok())
        .ok_or_else(|| err(format!("game {}: not a study game id", h.game_id)))?;
    let seating: Vec<usize> = h
        .seats
        .iter()
        .map(|s| {
            let e = entry_of(&s.brain);
            roster
                .entries()
                .iter()
                .position(|r| *r == e)
                .ok_or_else(|| err(format!("game {}: {} is not in the roster", h.game_id, e)))
        })
        .collect::<Result<_, _>>()?;
    let o = &replay.outcome;
    let (winner, tie, seat) = match o.winner {
        Winner::Seat(s) => (seating.get(usize::from(s)).copied(), false, Some(s)),
        Winner::Tie => (None, true, None),
        Winner::Nobody => (None, false, None),
    };
    let holdings: Vec<Holdings> = replay
        .checkpoints
        .last()
        .map(|c| {
            c.seats
                .iter()
                .map(|s| Holdings {
                    leases: s.leases,
                    businesses: s.businesses,
                    ..Holdings::default()
                })
                .collect()
        })
        .unwrap_or_default();
    let fun: FunGame = fun_game(&boards_of(replay), seat, holdings, false);
    Ok(GameOutcome {
        index,
        seed: h.seed,
        rotation: u32::try_from(seating.first().copied().unwrap_or(0)).unwrap_or(0),
        winner,
        tie,
        net_worth: o.seats.iter().map(|s| s.net_worth).collect(),
        bankrupt: o.seats.iter().map(|s| s.bankrupt).collect(),
        days: o.days_played,
        ticks: o.ticks,
        state_hash: o.final_state_hash,
        fun,
        seating,
    })
}

/// One model's games from its manifest part, in plan order.
fn model_games(
    store: &dyn ObjectStore,
    key: &str,
    roster: &Roster,
) -> Result<Vec<GameOutcome>, StudyError> {
    let mut games: Vec<GameOutcome> = load::games(store, key)
        .map_err(err)?
        .iter()
        .map(|g| outcome_of(&g.replay, roster))
        .collect::<Result<_, _>>()?;
    games.sort_by_key(|g| g.index);
    Ok(games)
}

/// The roster of a recording with no study directory: game 0 is rotation 0, so its seats hold
/// the roster in order.
fn roster_from(store: &dyn ObjectStore, key: &str) -> Result<Roster, StudyError> {
    let games = load::games(store, key).map_err(err)?;
    let first = games
        .iter()
        .find(|g| g.replay.header.game_id.ends_with("-g000000"))
        .or_else(|| games.first())
        .ok_or_else(|| err(format!("{key}: no games")))?;
    Ok(Roster::new(
        first
            .replay
            .header
            .seats
            .iter()
            .map(|s| entry_of(&s.brain))
            .collect(),
    ))
}

/// Reads the study directory's definition, roster and rows.
struct StudyDir {
    summary: Json,
    def: StudyDef,
    roster: Roster,
    header: Vec<String>,
    rows: Vec<Row>,
}

/// Reads the study directory. Without a `summary.json` (a study still running its first
/// chunk under a build that writes it only at the end), the definition must come from `--def`
/// (`fallback`) and the roster from the recordings (`roster`).
fn read_study(
    dir: &Path,
    fallback: Option<&StudyDef>,
    roster: impl FnOnce() -> Result<Roster, StudyError>,
) -> Result<StudyDir, StudyError> {
    let (summary, def, roster) = if let Some(text) = read_opt(&dir.join("summary.json"))? {
        let summary: Json = serde_json::from_str(&text).map_err(err)?;
        let def: StudyDef = serde_json::from_value(summary["study"]["definition"].clone())
            .map_err(|e| err(format!("summary.json study.definition: {e}")))?;
        let names: Vec<String> = serde_json::from_value(summary["study"]["roster"].clone())
            .map_err(|e| err(format!("summary.json study.roster: {e}")))?;
        let roster = Roster::new(names.iter().map(|n| Entry::from_name(n)).collect());
        (summary, def, roster)
    } else {
        let def = fallback.cloned().ok_or_else(|| {
            err(format!(
                "{}: no summary.json; pass --def with the study's definition",
                dir.display()
            ))
        })?;
        let roster = roster()?;
        let summary = json!({ "study": { "definition": def, "roster": roster.names() } });
        (summary, def, roster)
    };
    let models = read_opt(&dir.join("models.csv"))?
        .ok_or_else(|| err(format!("{}: no models.csv", dir.display())))?;
    let header = models
        .lines()
        .next()
        .map(split_csv_line)
        .unwrap_or_default();
    let rows = read_models(&models, &header).map_err(|h| err(h.join(",")))?;
    Ok(StudyDir {
        summary,
        def,
        roster,
        header,
        rows,
    })
}

/// One model's results from its recording.
struct Scored {
    id: u32,
    games: u64,
    fun: FunMetrics,
    tally: Tally,
}

/// Reads and scores every model's part in parallel (results in model order). Only the
/// per-model results are kept, so memory stays flat however large the study.
fn score_parts(
    store: &dyn ObjectStore,
    parts: &[&(u32, String)],
    roster: &Roster,
    threads: Option<usize>,
) -> Result<Vec<Scored>, StudyError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.unwrap_or(0))
        .build()?;
    let results: Vec<Result<Scored, StudyError>> = pool.install(|| {
        parts
            .par_iter()
            .map(|(id, key)| {
                let games = model_games(store, key, roster)?;
                let mut tally = Tally::new(roster.len());
                for g in &games {
                    tally.add(roster, g);
                }
                Ok(Scored {
                    id: *id,
                    games: games.len() as u64,
                    fun: fun_metrics(roster, &games),
                    tally,
                })
            })
            .collect()
    });
    results.into_iter().collect()
}

/// The study's rows with their fun columns replaced (or appended) by the rescored ones. Ok
/// models without a recording are listed in `missing` and keep empty fun columns.
fn study_rows(s: &StudyDir, scored: &[Scored], missing: &mut Vec<u32>) -> (Vec<String>, Vec<Row>) {
    let keep: Vec<usize> = (0..s.header.len())
        .filter(|i| !FUN_COLUMNS.contains(&s.header[*i].as_str()))
        .collect();
    let mut header: Vec<String> = keep.iter().map(|i| s.header[*i].clone()).collect();
    header.extend(FUN_COLUMNS.iter().map(|c| (*c).to_owned()));
    let cols = Columns::new(s.header.clone());
    let rows = s
        .rows
        .iter()
        .map(|r| {
            let mut fields: Vec<String> = keep.iter().map(|i| r.fields[*i].clone()).collect();
            if let Some(m) = scored
                .iter()
                .find(|m| m.id == r.id)
                .filter(|_| cols.is_ok(r))
            {
                fields.extend(fun_fields(&s.roster, &m.fun));
            } else {
                if cols.is_ok(r) {
                    missing.push(r.id);
                }
                fields.extend(std::iter::repeat_n(String::new(), FUN_COLUMNS.len()));
            }
            Row { id: r.id, fields }
        })
        .collect();
    (header, rows)
}

/// Rows built from the recordings alone: every metric from the outcomes, no parameters.
fn recorded_rows(def: &StudyDef, roster: &Roster, scored: &[Scored]) -> (Vec<String>, Vec<Row>) {
    let bare = StudyDef {
        parameters: Vec::new(),
        ..def.clone()
    };
    let header = models_header(&bare, roster, false);
    let rows = scored
        .iter()
        .map(|m| {
            let metrics = metrics(roster, &m.tally, &def.criteria);
            let model = ModelSpec {
                id: m.id,
                units: None,
                values: None,
            };
            let outcome = Outcome::Ok(metrics, Box::new(m.fun.clone()), 0.0);
            Row {
                id: m.id,
                fields: model_row(&bare, roster, &model, &[], &outcome, false),
            }
        })
        .collect();
    (header, rows)
}

/// Rescores a recorded study (see the module docs).
pub fn rescore(
    store: &dyn ObjectStore,
    opts: &RescoreOptions,
) -> Result<RescoreReport, StudyError> {
    let parts = parts(store, &opts.run_id)?;
    let study = opts
        .study
        .as_deref()
        .map(|dir| {
            read_study(dir, opts.def.as_ref(), || match parts.first() {
                Some((_, key)) => roster_from(store, key),
                None => Err(err(format!("no manifests/{}/part-m*.parquet", opts.run_id))),
            })
        })
        .transpose()?;
    let mut def = match (&study, &opts.def) {
        (Some(s), Some(d)) => StudyDef {
            fun: d.fun.clone(),
            ..s.def.clone()
        },
        (Some(s), None) => s.def.clone(),
        (None, Some(d)) => d.clone(),
        (None, None) => StudyDef::default(),
    };
    // A checkpoint counts leases and businesses only: that is the playstyle basis here.
    let asked = def.fun.playstyle;
    def.fun.playstyle = Playstyle::Checkpoint;
    let roster = match (&study, parts.first()) {
        (Some(s), _) => s.roster.clone(),
        (None, Some((_, key))) => roster_from(store, key)?,
        (None, None) => {
            return Err(err(format!(
                "no manifests/{}/part-m*.parquet in the recordings",
                opts.run_id
            )));
        }
    };
    let wanted: Vec<&(u32, String)> = parts
        .iter()
        .filter(|(id, _)| {
            study
                .as_ref()
                .is_none_or(|s| s.rows.iter().any(|r| r.id == *id))
        })
        .collect();
    let scored = score_parts(store, &wanted, &roster, opts.threads)?;
    let mut report = RescoreReport {
        models: scored.len(),
        games: scored.iter().map(|m| m.games).sum(),
        ..RescoreReport::default()
    };
    let (header, mut rows) = if let Some(s) = &study {
        study_rows(s, &scored, &mut report.missing)
    } else {
        recorded_rows(&def, &roster, &scored)
    };
    let cols = Columns::new(header.clone());
    apply_fun_scores(&cols, &mut rows, &def.fun);
    let order = rank(&cols, &mut rows);
    std::fs::create_dir_all(&opts.out).map_err(|e| StudyError::io(&opts.out, e))?;
    write(
        &opts.out.join("models.csv"),
        &csv_text(&header, rows.iter().map(|r| r.fields.clone())),
    )?;
    let balanced: Vec<u32> = order
        .iter()
        .copied()
        .filter(|id| rows.iter().any(|r| r.id == *id && cols.is_balanced(r)))
        .collect();
    let top: Vec<u32> = balanced
        .iter()
        .copied()
        .take(usize::try_from(def.top_n).unwrap_or(usize::MAX))
        .collect();
    let mut summary = study.as_ref().map_or_else(
        || json!({ "study": { "definition": def, "roster": roster.names() } }),
        |s| s.summary.clone(),
    );
    summary["models"]["balanced"] = json!(balanced.len());
    summary["ranking"] = json!(ranking_json(&cols, &rows, &order));
    summary["top"] = json!(top);
    summary["columns"]["models.csv"] = json!(header);
    summary["fun"] = json!({
        "definition": def.fun,
        "playstyle_asked": asked,
        "playstyle_used": Playstyle::Checkpoint,
    });
    summary["rescore"] = json!({
        "run_id": opts.run_id,
        "models": report.models,
        "games": report.games,
        "missing": report.missing,
        "note": "fun columns computed from the recordings' checkpoints; playstyle measured on \
                 leases and businesses. Overlays are not rewritten: see models/<id> of the study.",
    });
    write(
        &opts.out.join("summary.json"),
        &(serde_json::to_string_pretty(&summary).unwrap_or_default() + "\n"),
    )?;
    report.order = order;
    Ok(report)
}
