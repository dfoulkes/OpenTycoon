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

use sim_core::ConfigSources;
use sim_core::config::ConfigFile;
use sim_core::hash::stable_hash;

use super::def::{EvolveDef, EvolveError};
use crate::overrides::{self, Override};
use crate::replay::record::RecordContext;
use crate::replay::writer::RunWriter;
use crate::study::Roster;

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
    rec: Option<&mut EvolveRecording<'_, '_>>,
) -> Result<EvolveReport, EvolveError> {
    let _ = (def, base, opts, rec);
    Err(EvolveError::BaseConfig(
        "sim-cli evolve: not implemented yet".into(),
    )) // red
}
