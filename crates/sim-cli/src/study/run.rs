//! Running a study: every model in id order, each model's games in parallel on a `rayon`
//! pool. A game depends only on `(model config, seed, seating)` and results are collected in
//! plan order, so every output (except the wall-time columns) is identical on any thread count.
//!
//! **Resume.** `models.csv` is appended one finished model at a time. A restart with the same
//! study (same definition, base config and options: the fingerprint in `study.json`) skips
//! every model already in it, drops a line cut off mid-write and any `games.csv` rows of
//! unfinished models, and carries on. A different study in the same directory is refused unless
//! `fresh` is set.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rayon::prelude::*;
use serde_json::json;
use sim_core::config::ConfigFile;
use sim_core::hash::stable_hash;
use sim_core::{Config, ConfigSources, World};

use super::def::{StudyDef, StudyError};
use super::metrics::{GameOutcome, Tally, metrics};
use super::models::{ModelSpec, baseline_labels, generate, model_config, node_at};
use super::output::{
    Columns, Outcome, Row, csv_text, game_row, games_header, model_row, models_header, rank,
    read_models, split_csv_line, write_overlays,
};
use super::seats::{GamePlan, Roster, effective_games, plan, resolve_roster};
use crate::brains::BrainKind;
use crate::game::{GameSpec, Winner, play_from};
use crate::overrides;
use crate::timing::Stopwatch;

/// How to run a study.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches, not a state machine"
)]
pub struct RunOptions {
    /// Output directory (created if missing).
    pub out: PathBuf,
    /// Worker threads; `None` uses every core. Outputs are identical for any value.
    pub threads: Option<usize>,
    /// Also write `games.csv`, one row per game.
    pub games_csv: bool,
    /// Include the wall-time columns (`games_per_sec`, `wall_ms`, summary timing). Off, two
    /// runs diff byte for byte.
    pub wall_time: bool,
    /// Discard whatever study is in `out` and start over.
    pub fresh: bool,
    /// Stop after running this many models (the rest are left for a resume).
    pub stop_after: Option<u32>,
    /// Print progress (models done, ETA) to stderr.
    pub progress: bool,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StudyReport {
    /// Models run by this call.
    pub ran: u32,
    /// Models found finished in `models.csv` and skipped.
    pub skipped: u32,
    /// Invalid models among those run now and before.
    pub invalid: u32,
    /// Balanced models (ids, rank order).
    pub balanced: Vec<u32>,
    /// Every model is in `models.csv`.
    pub complete: bool,
    /// Games played by this call.
    pub games: u64,
    /// The preset the games ran (the asked-for one, or the longest if it is missing).
    pub preset: String,
    /// Games per model after rounding up to whole seat rotations.
    pub games_per_model: u32,
    /// Wall time of this call.
    pub wall: Duration,
}

/// The preset to run: `wanted` if `game.yaml` has it, else the longest (latest end on a tie,
/// then name).
pub fn resolve_preset(config: &Config, wanted: &str) -> Option<String> {
    if config.game.presets.contains_key(wanted) {
        return Some(wanted.to_owned());
    }
    config
        .game
        .presets
        .iter()
        .max_by(|(an, a), (bn, b)| {
            (a.end_year.saturating_sub(a.start_year), a.end_year)
                .cmp(&(b.end_year.saturating_sub(b.start_year), b.end_year))
                .then_with(|| bn.cmp(an))
        })
        .map(|(n, _)| n.clone())
}

/// A hex fingerprint of everything that decides a study's results and file layout.
pub fn fingerprint(
    def: &StudyDef,
    base: &ConfigSources,
    preset: &str,
    roster: &Roster,
    opts: &RunOptions,
) -> String {
    let files: Vec<(&str, &str)> = ConfigFile::ALL
        .iter()
        .map(|f| (f.file_name(), base.get(f.file_name()).unwrap_or("")))
        .collect();
    let def_json = serde_json::to_string(def).unwrap_or_default();
    format!(
        "{:016x}",
        stable_hash(&(
            def_json,
            files,
            preset,
            roster.names(),
            opts.games_csv,
            opts.wall_time
        ))
    )
}

/// Plays one game of a model.
fn play_one(config: &Config, roster: &Roster, g: &GamePlan) -> (GameOutcome, Duration) {
    let seats = u8::try_from(roster.len()).unwrap_or(u8::MAX);
    let world = World::seated(config.clone(), g.seed, seats, &roster.ai_seats(&g.seating));
    let result = play_from(
        world,
        &GameSpec {
            players: seats,
            brain: BrainKind::Idle,
            ..GameSpec::default()
        },
    );
    let (winner, tie) = match result.winner {
        Winner::Player(p) => (g.seating.get(usize::from(p)).copied(), false),
        Winner::Tie => (None, true),
        Winner::Nobody => (None, false),
    };
    let outcome = GameOutcome {
        index: g.index,
        seed: g.seed,
        rotation: g.rotation,
        seating: g.seating.clone(),
        winner,
        tie,
        net_worth: result.players.iter().map(|p| p.net_worth.pence()).collect(),
        bankrupt: result.players.iter().map(|p| p.bankrupt).collect(),
        days: result.days_played,
        ticks: result.ticks,
        state_hash: result.state_hash,
    };
    (outcome, result.wall)
}

fn read_opt(path: &Path) -> Result<Option<String>, StudyError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(StudyError::io(path, e)),
    }
}

fn write(path: &Path, text: &str) -> Result<(), StudyError> {
    std::fs::write(path, text).map_err(|e| StudyError::io(path, e))
}

fn append(path: &Path, lines: &str) -> Result<(), StudyError> {
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(|e| StudyError::io(path, e))?;
    f.write_all(lines.as_bytes())
        .and_then(|()| f.flush())
        .map_err(|e| StudyError::io(path, e))
}

fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{:.1}s", d.as_secs_f64())
    }
}

/// Checks every non-optional parameter key exists in the base config, so a typo fails before
/// any game instead of making every model invalid.
fn check_keys(def: &StudyDef, base: &ConfigSources) -> Result<(), StudyError> {
    for (i, p) in def.parameters.iter().enumerate() {
        for k in p.all_keys() {
            if node_at(base, k).is_none() {
                if p.optional {
                    eprintln!(
                        "study: parameter {} skipped: no key {k} in the base config",
                        p.name
                    );
                } else {
                    return Err(StudyError::def(
                        format!("parameters[{i}].key"),
                        format!(
                            "no key {k} in the base config (mark the parameter `optional: true` \
                             to skip it until that content lands)"
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Everything resolved before the first game: the study's fixed context.
struct Study<'a> {
    def: &'a StudyDef,
    base: &'a ConfigSources,
    opts: &'a RunOptions,
    preset: String,
    roster: Roster,
    models: Vec<ModelSpec>,
    baseline: Vec<Option<String>>,
    plans: Vec<GamePlan>,
    header: Vec<String>,
    games_header: Vec<String>,
}

impl<'a> Study<'a> {
    /// Validates the definition against the base config and resolves preset, roster, models
    /// and the game plan.
    fn prepare(
        def: &'a StudyDef,
        base: &'a ConfigSources,
        opts: &'a RunOptions,
    ) -> Result<Self, StudyError> {
        def.validate()?;
        let base_config =
            overrides::load(base, None, &[]).map_err(|e| StudyError::BaseConfig(e.to_string()))?;
        let preset = resolve_preset(&base_config, &def.preset)
            .ok_or_else(|| StudyError::BaseConfig("game.yaml has no presets".into()))?;
        if preset != def.preset {
            eprintln!(
                "study: preset {:?} not in game.yaml; using the longest, {preset:?}",
                def.preset
            );
        }
        let roster = resolve_roster(def, &base_config)?;
        check_keys(def, base)?;
        Ok(Study {
            models: generate(def)?,
            baseline: baseline_labels(def, base),
            plans: plan(def.games_per_model, roster.len(), def.seed),
            header: models_header(def, &roster, opts.wall_time),
            games_header: games_header(roster.len(), opts.wall_time),
            def,
            base,
            opts,
            preset,
            roster,
        })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.opts.out.join(name)
    }

    /// Creates the output directory, checks it holds this study (or nothing), and returns
    /// the models already finished, after dropping cut-off lines and unfinished games.
    fn open(&self) -> Result<Vec<Row>, StudyError> {
        let dir = &self.opts.out;
        std::fs::create_dir_all(dir).map_err(|e| StudyError::io(dir, e))?;
        let (models_path, games_path, state_path) = (
            self.path("models.csv"),
            self.path("games.csv"),
            self.path("study.json"),
        );
        if self.opts.fresh {
            for p in [
                &models_path,
                &games_path,
                &state_path,
                &self.path("summary.json"),
            ] {
                if p.exists() {
                    std::fs::remove_file(p).map_err(|e| StudyError::io(p, e))?;
                }
            }
        }
        let print = fingerprint(self.def, self.base, &self.preset, &self.roster, self.opts);
        let mismatch = |found: String| StudyError::Mismatch {
            dir: dir.display().to_string(),
            found,
            expected: print.clone(),
        };
        match read_opt(&state_path)? {
            Some(text) => {
                let found = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v["fingerprint"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                if found != print {
                    return Err(mismatch(found));
                }
            }
            None if models_path.exists() => return Err(mismatch("none".into())),
            None => {}
        }
        let state = json!({
            "fingerprint": print,
            "note": "sim-cli study state: a restart with the same fingerprint resumes",
        });
        write(
            &state_path,
            &(serde_json::to_string_pretty(&state).unwrap_or_default() + "\n"),
        )?;
        let mut rows: Vec<Row> = match read_opt(&models_path)? {
            Some(text) => read_models(&text, &self.header).map_err(|h| mismatch(h.join(",")))?,
            None => Vec::new(),
        };
        rows.retain(|r| r.id < self.def.models);
        write(
            &models_path,
            &csv_text(&self.header, rows.iter().map(|r| r.fields.clone())),
        )?;
        if self.opts.games_csv {
            let finished = |l: &&str| {
                let f = split_csv_line(l);
                f.len() == self.games_header.len()
                    && f[0]
                        .parse::<u32>()
                        .is_ok_and(|id| rows.iter().any(|r| r.id == id))
            };
            let mut text = self.games_header.join(",") + "\n";
            for l in read_opt(&games_path)?
                .unwrap_or_default()
                .lines()
                .skip(1)
                .filter(finished)
            {
                text.push_str(l);
                text.push('\n');
            }
            write(&games_path, &text)?;
        }
        Ok(rows)
    }

    /// Plays one model's games (unless its config is invalid) and appends them to
    /// `games.csv`. Returns the outcome and the number of games played.
    fn run_model(
        &self,
        pool: &rayon::ThreadPool,
        model: &ModelSpec,
    ) -> Result<(Outcome, u64), StudyError> {
        let watch = Stopwatch::start();
        let config = match model_config(self.def, self.base, model, &self.preset) {
            Err(e) => return Ok((Outcome::Invalid(e.to_string()), 0)),
            Ok(c) => c,
        };
        let games: Vec<(GameOutcome, Duration)> = pool.install(|| {
            self.plans
                .par_iter()
                .map(|g| play_one(&config, &self.roster, g))
                .collect()
        });
        let mut tally = Tally::new(self.roster.len());
        for (g, _) in &games {
            tally.add(&self.roster, g);
        }
        if self.opts.games_csv {
            let mut text = String::new();
            for (g, wall) in &games {
                let ms = self.opts.wall_time.then_some(wall.as_secs_f64() * 1000.0);
                text.push_str(&game_row(model.id, &self.roster, g, ms).join(","));
                text.push('\n');
            }
            append(&self.path("games.csv"), &text)?;
        }
        let secs = watch.elapsed().as_secs_f64();
        #[allow(clippy::cast_precision_loss, reason = "game counts are small")]
        let gps = if secs > 0.0 {
            games.len() as f64 / secs
        } else {
            0.0
        };
        let m = metrics(&self.roster, &tally, &self.def.criteria);
        Ok((Outcome::Ok(m, gps), games.len() as u64))
    }

    /// One progress line on stderr: this model, how far the run is, and the ETA.
    fn progress(
        &self,
        model: u32,
        outcome: &Outcome,
        report: &StudyReport,
        todo: usize,
        per: Duration,
    ) {
        let done = report.ran;
        let left = u32::try_from(todo).unwrap_or(u32::MAX).saturating_sub(done);
        let status = match outcome {
            Outcome::Ok(m, gps) => format!(
                "ok, {}balanced, max win share {:.1}%, {gps:.0} games/s",
                if m.balanced { "" } else { "not " },
                m.max_win_share * 100.0
            ),
            Outcome::Invalid(_) => "invalid config".to_owned(),
        };
        eprintln!(
            "study: model {model} ({done}/{todo} this run, {} of {} done): {status}; ETA {}",
            done + report.skipped,
            self.def.models,
            fmt_duration(per * left)
        );
    }

    /// Sorts and ranks the rows, rewrites `models.csv`, writes the overlays (when complete)
    /// and `summary.json`.
    fn finish(&self, mut rows: Vec<Row>, report: &mut StudyReport) -> Result<(), StudyError> {
        rows.sort_by_key(|r| r.id);
        let cols = Columns::new(self.header.clone());
        let order = rank(&cols, &mut rows);
        write(
            &self.path("models.csv"),
            &csv_text(&self.header, rows.iter().map(|r| r.fields.clone())),
        )?;
        report.invalid = u32::try_from(rows.iter().filter(|r| !cols.is_ok(r)).count()).unwrap_or(0);
        report.balanced = order
            .iter()
            .copied()
            .filter(|id| rows.iter().any(|r| r.id == *id && cols.is_balanced(r)))
            .collect();
        report.complete = rows.len() == self.models.len();
        let top: Vec<u32> = report
            .balanced
            .iter()
            .copied()
            .take(usize::try_from(self.def.top_n).unwrap_or(usize::MAX))
            .collect();
        if report.complete {
            write_overlays(
                &self.opts.out,
                self.def,
                self.base,
                &self.models,
                &top,
                &cols,
                &rows,
            )?;
        }
        let summary = summary(self, report, &cols, &rows, &order, &top);
        write(
            &self.path("summary.json"),
            &(serde_json::to_string_pretty(&summary).unwrap_or_default() + "\n"),
        )
    }
}

/// Runs (or resumes) a study on `base` (the config directory plus any global `--set`).
pub fn run_study(
    def: &StudyDef,
    base: &ConfigSources,
    opts: &RunOptions,
) -> Result<StudyReport, StudyError> {
    let watch = Stopwatch::start();
    let study = Study::prepare(def, base, opts)?;
    let mut rows = study.open()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.unwrap_or(0))
        .build()?;
    let todo: Vec<&ModelSpec> = study
        .models
        .iter()
        .filter(|m| !rows.iter().any(|r| r.id == m.id))
        .collect();
    let mut report = StudyReport {
        skipped: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        preset: study.preset.clone(),
        games_per_model: effective_games(def.games_per_model, study.roster.len()),
        ..StudyReport::default()
    };
    let run_watch = Stopwatch::start();
    for model in &todo {
        if opts.stop_after.is_some_and(|n| report.ran >= n) {
            break;
        }
        let (outcome, games) = study.run_model(&pool, model)?;
        report.games += games;
        let fields = model_row(
            def,
            &study.roster,
            model,
            &study.baseline,
            &outcome,
            opts.wall_time,
        );
        append(&study.path("models.csv"), &(fields.join(",") + "\n"))?;
        rows.push(Row {
            id: model.id,
            fields,
        });
        report.ran += 1;
        if opts.progress {
            let per = run_watch.elapsed() / report.ran.max(1);
            study.progress(model.id, &outcome, &report, todo.len(), per);
        }
    }
    report.wall = watch.elapsed();
    study.finish(rows, &mut report)?;
    Ok(report)
}

fn summary(
    study: &Study<'_>,
    report: &StudyReport,
    cols: &Columns,
    rows: &[Row],
    order: &[u32],
    top: &[u32],
) -> serde_json::Value {
    let (def, roster, opts, games_header) =
        (study.def, &study.roster, study.opts, &study.games_header);
    let ranking: Vec<serde_json::Value> = order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = rows.iter().find(|r| r.id == *id)?;
            let num = |c: &str| cols.json(r)[c].clone();
            Some(json!({
                "rank": i + 1,
                "model_id": id,
                "balanced": cols.is_balanced(r),
                "score": num("score"),
                "max_win_share": num("max_win_share"),
                "ai_bankruptcy_rate": num("ai_bankruptcy_rate"),
                "idle_last_rate": num("idle_last_rate"),
                "idle_below_solvent_rate": num("idle_below_solvent_rate"),
            }))
        })
        .collect();
    let invalid: Vec<serde_json::Value> = rows
        .iter()
        .filter(|r| !cols.is_ok(r))
        .map(|r| json!({ "model_id": r.id, "error": cols.get(r, "error") }))
        .collect();
    let mut s = json!({
        "study": {
            "definition": def,
            "preset": report.preset,
            "roster": roster.names(),
            "seats": roster.len(),
            "games_per_model": report.games_per_model,
            "seeds": format!("{}..{}", def.seed, def.seed.saturating_add(u64::from(report.games_per_model / u32::try_from(roster.len().max(1)).unwrap_or(1)))),
        },
        "models": {
            "total": def.models,
            "finished": rows.len(),
            "complete": report.complete,
            "ok": rows.len() - invalid.len(),
            "invalid": invalid.len(),
            "balanced": report.balanced.len(),
        },
        "ranking": ranking,
        "top": top,
        "invalid": invalid,
        "baseline": rows.iter().find(|r| r.id == 0).map(|r| cols.json(r)),
        "columns": {
            "models.csv": cols.header(),
            "games.csv": if opts.games_csv { json!(games_header) } else { json!(null) },
        },
    });
    if opts.wall_time {
        let secs = report.wall.as_secs_f64();
        #[allow(clippy::cast_precision_loss, reason = "game counts are small")]
        let gps = if secs > 0.0 {
            report.games as f64 / secs
        } else {
            0.0
        };
        s["timing"] = json!({
            "wall_s": secs,
            "games_this_run": report.games,
            "games_per_sec": gps,
            "threads": opts.threads,
        });
    }
    s
}
