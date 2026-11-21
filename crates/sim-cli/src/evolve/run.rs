//! Running an evolution: generation by generation, every candidate of every evolving
//! personality evaluated in parallel against the previous generation's frozen champions, then
//! the new champions' lineup played together (docs/evolve.md).
//!
//! A game depends only on `(config, seed, seating)` and results are collected in plan order, so
//! every output (bar `summary.json`'s timing) is identical on any thread count.
//!
//! **Resume.** A generation is finished when its `champions/<gen>/lineup.json` exists; its rows
//! are in `generations.csv`. A restart with the same fingerprint (`evolve.json`) keeps every
//! finished generation, drops the rows of an unfinished one, and breeds on from the last
//! finished generation's rows, exactly as an uninterrupted run would.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value as Json, json};
use sim_core::ConfigSources;
use sim_core::config::ConfigFile;
use sim_core::hash::stable_hash;

use super::def::{EvolveDef, EvolveError};
use super::evolution::Evolution;
use super::output::{COLUMNS, Candidate, csv, curve};
use crate::overrides::{self, Override};
use crate::replay::record::RecordContext;
use crate::replay::writer::RunWriter;
use crate::study::Roster;
use crate::study::run::{append, fmt_duration, read_opt, write};
use crate::timing::Stopwatch;

/// How to run an evolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EvolveOptions {
    /// Output directory (created if missing).
    pub out: PathBuf,
    /// Worker threads; `None` uses every core. Outputs are identical for any value.
    pub threads: Option<usize>,
    /// Put wall time in `summary.json`. Off, two runs diff byte for byte.
    pub wall_time: bool,
    /// Discard whatever evolution is in `out` and start over.
    pub fresh: bool,
    /// Stop after running this many generations (the rest are left for a resume).
    pub stop_after: Option<u32>,
    /// Print progress (generations done, ETA) to stderr.
    pub progress: bool,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EvolveReport {
    /// Generations run by this call.
    pub ran: u32,
    /// Generations found finished and skipped.
    pub skipped: u32,
    /// Every generation is finished.
    pub complete: bool,
    /// Games played by this call.
    pub games: u64,
    /// Invalid candidates among those run now.
    pub invalid: u64,
    /// The preset the games ran.
    pub preset: String,
    /// Games per evaluation after rounding up to whole seat rotations.
    pub games_per_eval: u32,
    /// The evolving personalities.
    pub personalities: Vec<String>,
    /// Wall time of this call.
    pub wall: Duration,
}

/// Recording an evolution (`--record`): every game of every candidate and lineup, one manifest
/// part per candidate.
#[derive(Debug)]
pub struct EvolveRecording<'w, 'a> {
    /// Header fields every game shares; the config hash and preset are filled per candidate.
    pub ctx: RecordContext,
    /// The run's writer.
    pub writer: &'w mut RunWriter<'a>,
}

/// The base sources an evolution runs on: the config directory, its `economy.yaml` replaced by
/// the definition's adopted one (if any), then `sets`.
pub fn base_sources(
    dir: &Path,
    def: &EvolveDef,
    sets: &[Override],
) -> Result<ConfigSources, EvolveError> {
    let mut base =
        ConfigSources::read_dir(dir).map_err(|e| EvolveError::BaseConfig(e.to_string()))?;
    if let Some(path) = &def.economy {
        let text =
            std::fs::read_to_string(path).map_err(|e| EvolveError::io(Path::new(path), e))?;
        base.insert(ConfigFile::Economy.file_name(), text);
    }
    for ov in sets {
        overrides::apply(&mut base, ov).map_err(|e| EvolveError::BaseConfig(e.to_string()))?;
    }
    Ok(base)
}

/// A hex fingerprint of everything that decides an evolution's results.
pub fn fingerprint(
    def: &EvolveDef,
    base: &ConfigSources,
    preset: &str,
    roster: &Roster,
    personalities: &[String],
    wall_time: bool,
) -> String {
    let files: Vec<(&str, &str)> = ConfigFile::ALL
        .iter()
        .map(|f| (f.file_name(), base.get(f.file_name()).unwrap_or("")))
        .collect();
    // Where games are recorded does not change them.
    let def_json = serde_json::to_string(&EvolveDef {
        record: None,
        ..def.clone()
    })
    .unwrap_or_default();
    format!(
        "{:016x}",
        stable_hash(&(
            def_json,
            files,
            preset,
            roster.names(),
            personalities,
            wall_time
        ))
    )
}

/// The first seed of generation `g`'s games: generations play disjoint seed blocks,
/// `seed + g × blocks`, and every candidate of a generation plays the same seeds.
pub fn generation_seed(seed: u64, generation: u32, blocks: u32) -> u64 {
    seed.wrapping_add(u64::from(generation) * u64::from(blocks))
}

/// Hash of a list of games' final state hashes, as 16 hex digits.
pub fn digest(hashes: &[u64]) -> String {
    format!("{:016x}", stable_hash(&hashes))
}

impl Evolution<'_> {
    /// Opens the output directory, checks it holds this evolution (or nothing), and returns
    /// the candidates of every finished generation, after dropping an unfinished one.
    fn open(&self) -> Result<Vec<Candidate>, EvolveError> {
        let dir = &self.opts.out;
        std::fs::create_dir_all(dir).map_err(|e| EvolveError::io(dir, e))?;
        let (csv_path, state_path) = (self.path("generations.csv"), self.path("evolve.json"));
        if self.opts.fresh {
            for p in [&csv_path, &state_path, &self.path("summary.json")] {
                if p.exists() {
                    std::fs::remove_file(p).map_err(|e| EvolveError::io(p, e))?;
                }
            }
            let champs = self.path("champions");
            if champs.exists() {
                std::fs::remove_dir_all(&champs).map_err(|e| EvolveError::io(&champs, e))?;
            }
        }
        let print = fingerprint(
            self.def,
            self.base,
            &self.preset,
            &self.roster,
            &self.personalities,
            self.opts.wall_time,
        );
        let mismatch = |found: String| EvolveError::Mismatch {
            dir: dir.display().to_string(),
            found,
            expected: print.clone(),
        };
        match read_opt(&state_path).map_err(EvolveError::from)? {
            Some(text) => {
                let found = serde_json::from_str::<Json>(&text)
                    .ok()
                    .and_then(|v| v["fingerprint"].as_str().map(str::to_owned))
                    .unwrap_or_default();
                if found != print {
                    return Err(mismatch(found));
                }
            }
            None if csv_path.exists() => return Err(mismatch("none".into())),
            None => {}
        }
        let state = json!({
            "fingerprint": print,
            "note": "sim-cli evolve state: a restart with the same fingerprint resumes",
        });
        write(
            &state_path,
            &(serde_json::to_string_pretty(&state).unwrap_or_default() + "\n"),
        )?;
        let text = read_opt(&csv_path)?.unwrap_or_default();
        let mut rows: Vec<Candidate> = text
            .lines()
            .skip(1)
            .filter_map(|l| Candidate::parse(l, |p| self.genes_of(p)))
            .collect();
        let per_gen = self.personalities.len() * usize::try_from(self.def.population).unwrap_or(0);
        let mut finished = 0u32;
        while finished < self.def.generations
            && self.champion_dir(finished).join("lineup.json").exists()
            && rows.iter().filter(|c| c.generation == finished).count() == per_gen
        {
            finished += 1;
        }
        rows.retain(|c| c.generation < finished);
        write(
            &csv_path,
            &csv(rows
                .iter()
                .map(|c| c.row(&self.genes[&c.personality], self.weights()))),
        )?;
        for g in finished..self.def.generations {
            let d = self.champion_dir(g);
            if d.exists() {
                std::fs::remove_dir_all(&d).map_err(|e| EvolveError::io(&d, e))?;
            }
        }
        Ok(rows)
    }

    fn summary(&self, rows: &[Candidate], report: &EvolveReport, finished: u32) -> Json {
        let mut personalities = serde_json::Map::new();
        for p in &self.personalities {
            let mut c = curve(rows, p, finished, self.weights());
            let lineup: Vec<Json> = (0..finished)
                .map(|g| {
                    std::fs::read_to_string(self.champion_dir(g).join("lineup.json"))
                        .ok()
                        .and_then(|t| serde_json::from_str::<Json>(&t).ok())
                        .map_or(Json::Null, |v| v["entries"][p]["win_share"].clone())
                })
                .collect();
            c["lineup_win_share"] = json!(lineup);
            personalities.insert(p.clone(), c);
        }
        let mut s = json!({
            "evolution": {
                "definition": self.def,
                "preset": self.preset,
                "roster": self.roster.names(),
                "personalities": self.personalities,
                "games_per_eval": self.games_per_eval(),
                "coevolution": "simultaneous, against the previous generation's frozen champions",
            },
            "generations": {
                "total": self.def.generations,
                "finished": finished,
                "complete": report.complete,
            },
            "personalities": personalities,
            "columns": COLUMNS,
        });
        if self.opts.wall_time {
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
                "threads": self.opts.threads,
            });
        }
        s
    }
}

/// Runs (or resumes) an evolution on `base` (see [`base_sources`]).
pub fn run_evolution(
    def: &EvolveDef,
    base: &ConfigSources,
    opts: &EvolveOptions,
) -> Result<EvolveReport, EvolveError> {
    run_evolution_recorded(def, base, opts, None)
}

/// [`run_evolution`], recording every game it plays into `rec` when given. A resumed run
/// records only the generations it runs now. Results are identical with or without recording.
pub fn run_evolution_recorded(
    def: &EvolveDef,
    base: &ConfigSources,
    opts: &EvolveOptions,
    mut rec: Option<&mut EvolveRecording<'_, '_>>,
) -> Result<EvolveReport, EvolveError> {
    let watch = Stopwatch::start();
    let evo = Evolution::prepare(def, base, opts)?;
    let mut rows = evo.open()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads.unwrap_or(0))
        .build()?;
    let first = rows.iter().map(|c| c.generation + 1).max().unwrap_or(0);
    let mut report = EvolveReport {
        skipped: first,
        preset: evo.preset.clone(),
        games_per_eval: evo.games_per_eval(),
        personalities: evo.personalities.clone(),
        ..EvolveReport::default()
    };
    if opts.progress {
        eprintln!(
            "evolve: {} personalities × {} genomes × {} games, {} generations ({} finished); \
             preset {}",
            evo.personalities.len(),
            def.population,
            evo.games_per_eval(),
            def.generations,
            first,
            evo.preset
        );
    }
    let run_watch = Stopwatch::start();
    let mut finished = first;
    for g in first..def.generations {
        if opts.stop_after.is_some_and(|n| report.ran >= n) {
            break;
        }
        let champs = evo.champions_after(&rows, g.checked_sub(1));
        let pops = evo.population(&rows, g);
        let done = evo.run_generation(&pool, g, &pops, &champs, rec.as_deref_mut())?;
        let text: String = done
            .candidates
            .iter()
            .map(|c| c.row(&evo.genes[&c.personality], evo.weights()).join(",") + "\n")
            .collect();
        append(&evo.path("generations.csv"), &text)?;
        report.invalid += done.candidates.iter().filter(|c| c.result.is_err()).count() as u64;
        // Output only (R79): before lineup.json, which marks the generation finished.
        super::lineup_fun::write(&opts.out, &evo.roster, g, &done.lineup)?;
        evo.write_champions(g, &done)?;
        let games = done.games;
        rows.extend(done.candidates);
        report.games += games;
        report.ran += 1;
        finished = g + 1;
        if opts.progress {
            let per = run_watch.elapsed() / report.ran.max(1);
            let left = def.generations - finished;
            let best: Vec<String> = evo
                .personalities
                .iter()
                .map(|p| {
                    let champ = rows
                        .iter()
                        .find(|c| c.generation == g && &c.personality == p && c.champion);
                    match champ.and_then(|c| c.result.as_ref().ok()) {
                        Some(e) => format!("{p} {:.3}", e.fitness(evo.weights())),
                        None => format!("{p} -"),
                    }
                })
                .collect();
            eprintln!(
                "evolve: generation {g} done ({finished}/{} finished): best fitness {}; {} games \
                 in {}; ETA {}",
                def.generations,
                best.join(", "),
                games,
                fmt_duration(per),
                fmt_duration(per * left)
            );
        }
    }
    report.complete = finished == def.generations;
    report.wall = watch.elapsed();
    let summary = evo.summary(&rows, &report, finished);
    write(
        &evo.path("summary.json"),
        &(serde_json::to_string_pretty(&summary).unwrap_or_default() + "\n"),
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped() -> ConfigSources {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        ConfigSources::read_dir(&dir).unwrap()
    }

    /// The shipped definition must parse, evolve every personality with keys that exist, start
    /// from a valid base genome and mostly breed valid genomes: a content change that breaks it
    /// shows up here, not in an overnight run.
    #[test]
    fn the_shipped_evolution_definition_is_usable() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../study/evolve.yaml");
        let def = EvolveDef::from_yaml(&std::fs::read_to_string(path).unwrap()).unwrap();
        let base = shipped();
        let opts = EvolveOptions::default();
        let evo = Evolution::prepare(&def, &base, &opts).unwrap();
        assert_eq!(
            evo.personalities,
            [
                "grocer",
                "landlord",
                "sir_penny_sugar",
                "speculator",
                "toff"
            ]
        );
        assert_eq!(evo.roster.len(), 6);
        assert!(evo.lineup_config(&evo.base_genomes).is_ok());
        let pops = evo.population(&[], 0);
        let mut invalid = 0;
        for (p, pop) in &pops {
            assert_eq!(pop.len(), def.population as usize);
            for (g, _) in pop {
                let mut genomes = evo.base_genomes.clone();
                genomes.insert(p.clone(), g.clone());
                invalid += usize::from(evo.lineup_config(&genomes).is_err());
            }
        }
        assert!(
            invalid * 10 <= pops.len() * def.population as usize,
            "{invalid} invalid"
        );
    }

    #[test]
    fn generations_play_disjoint_seed_blocks() {
        assert_eq!(generation_seed(7, 0, 20), 7);
        assert_eq!(generation_seed(7, 3, 20), 67);
    }
}
