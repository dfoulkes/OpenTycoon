//! `sim-cli`: the headless harness (spec §11). See README "Running the harness" and
//! docs/perf.md.
//!
//! - `sim-cli validate-config <dir>`: load and validate a config directory; exit 1 naming every
//!   bad key and file.
//! - `sim-cli write-schemas <dir>`: regenerate `*.schema.json` from the Rust config types.
//! - `sim-cli run <dir> --ticks N --seed S`: run a headless game and print the state hash (CI
//!   compares it across Linux x86_64 and macOS aarch64; keep the output format stable).
//! - `sim-cli batch <dir>`: N games of a preset in parallel → CSV + summary.
//! - `sim-cli bench <dir>`: ticks/sec and real-time factor as JSON.
//! - `sim-cli perf-budget <dir>`: measure and compare against `perf/budgets.yaml`; exit 1 if
//!   over budget.
//! - `sim-cli study <dir>`: the balance study (R64, R65): sampled models × rotated games →
//!   `models.csv`, `summary.json`, overlays of the best balanced models (docs/study.md).
//! - `sim-cli record <dir> --dest <s3|dir>`: play and record N games (docs/replay.md);
//!   `batch` and `study` take `--record <s3|dir>` too.
//! - `sim-cli replay --verify <manifest|segment>`: re-simulate recorded games and compare
//!   every midnight; exit 1 on a mismatch, 2 if they were recorded under other rules.
//! - `sim-cli extract <manifest|segment> --out <file>`: the stub feature extractor.
//! - `sim-cli rules-fingerprint`: this build's rules fingerprint.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use sim_cli::batch::{
    BatchRecording, BatchSpec, Timing, run_batch, run_batch_recorded, summarise, write_csv,
};
use sim_cli::bench::{BenchSpec, bench};
use sim_cli::brains::BrainKind;
use sim_cli::budget::{
    Budget, Budgets, MeasureSpec, Metric, all_pass, check, consistency, measure, table,
};
use sim_cli::game::GameSpec;
use sim_cli::overrides::{self, LoadError, Override};
use sim_cli::replay::fingerprint::{GIT_COMMIT, rules_fingerprint};
use sim_cli::replay::manifest::{RulesWeight, Weights};
use sim_cli::replay::record::RecordContext;
use sim_cli::replay::snapshot::snapshot;
use sim_cli::replay::store::{LocalStore, ObjectStore};
use sim_cli::replay::verify::{VerifyError, verify};
use sim_cli::replay::writer::{RunWriter, WriteStats, WriterOptions};
use sim_cli::replay::{self, extract, load};
use sim_cli::study::{RunOptions, StudyDef, StudyRecording, run_study, run_study_recorded};
use sim_cli::timing::Stopwatch;
use sim_core::clock::ClockRules;
use sim_core::config::ConfigFile;
use sim_core::hash::stable_hash;
use sim_core::{Config, ConfigSources, World};

#[derive(Parser)]
#[command(name = "sim-cli", about = "OpenTycoon headless harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Config selection shared by run, batch and bench.
#[derive(Args, Debug, Clone)]
struct ConfigArgs {
    /// Config directory.
    dir: PathBuf,
    /// Override one config value, `file.key.path=value` (repeatable), e.g.
    /// `--set economy.money.overdraft.start_limit=20000`. Validated like a hand-edited file.
    #[arg(long = "set", value_name = "FILE.KEY=VALUE", value_parser = parse_override)]
    overrides: Vec<Override>,
}

fn parse_override(s: &str) -> Result<Override, String> {
    s.parse().map_err(|e: LoadError| e.to_string())
}

fn parse_brain(s: &str) -> Result<BrainKind, String> {
    s.parse()
}

fn parse_metric(s: &str) -> Result<Metric, String> {
    s.parse()
}

fn parse_budget(s: &str) -> Result<(Metric, f64), String> {
    let (m, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected `metric=limit`, got `{s}`"))?;
    let limit: f64 = v
        .parse()
        .map_err(|e| format!("budget `{s}`: limit is not a number: {e}"))?;
    Ok((m.parse()?, limit))
}

#[derive(Subcommand)]
enum Command {
    /// Validate a config directory (the eight YAML files of spec §10b).
    ValidateConfig {
        /// Directory holding game.yaml, map.yaml, ...
        dir: PathBuf,
    },
    /// Write the JSON Schemas generated from the config types into a directory.
    WriteSchemas {
        /// Output directory, normally config/schema.
        dir: PathBuf,
    },
    /// Run a headless game for N ticks and print the final state hash.
    Run {
        #[command(flatten)]
        config: ConfigArgs,
        /// Ticks to run.
        #[arg(long, default_value_t = 8_640)]
        ticks: u64,
        /// RNG seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Player seats.
        #[arg(long, default_value_t = 2)]
        players: u8,
    },
    /// Play N independent games of a preset in parallel; write one CSV row per game and print a
    /// summary (median/p10/p90 net worth, bankruptcy rate, games/sec).
    Batch(BatchArgs),
    /// Measure headless throughput (ticks/sec, ms per game day, real-time factor, tick
    /// latency) and print JSON.
    Bench(BenchArgs),
    /// Measure the sim and compare against the perf budgets; exit 1 if any is over budget.
    /// Not part of `cargo test`: run it on a quiet machine (`cargo perf`).
    PerfBudget(PerfArgs),
    /// Balance study (R64, R65): sample economic models, play each over many games with the
    /// seats rotated, and rank the balanced ones. Resumes an interrupted study in `--out`.
    Study(StudyArgs),
    /// Play N games of a preset and record every one (docs/replay.md).
    Record(RecordCmdArgs),
    /// Re-simulate recorded games and compare every midnight with the recording.
    Replay(ReplayArgs),
    /// Stub feature extractor: one Parquet row per recorded command, with lineage and weights.
    Extract(ExtractArgs),
    /// Print this build's rules fingerprint: the final state hash of the reference game.
    RulesFingerprint,
}

/// Recording options shared by batch, study and record.
#[derive(Args, Debug, Clone)]
struct RecordOpts {
    /// Run id (default: derived from the run's inputs, so a rerun overwrites itself).
    #[arg(long)]
    run_id: Option<String>,
    /// Training weight of a game with a human seat (AI-only games weigh 1).
    #[arg(long, default_value_t = 5.0)]
    human_weight: f64,
    /// Target replay segment size in MiB.
    #[arg(long, default_value_t = 8)]
    segment_mib: usize,
}

#[derive(Args, Debug)]
struct RecordCmdArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// Where to record: `s3` (from `OPENTYCOON_S3_*`) or a local directory.
    #[arg(long)]
    dest: String,
    /// Games to play; seeds are `seed..seed+games`.
    #[arg(long, default_value_t = 100)]
    games: u64,
    /// First seed.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Game length preset from game.yaml.
    #[arg(long, default_value = "v1")]
    preset: String,
    /// Brain for the seats the AI opponents do not play: idle or scripted.
    #[arg(long, default_value = "idle", value_parser = parse_brain)]
    brain: BrainKind,
    /// Player seats.
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Worker threads (default: every core).
    #[arg(long)]
    threads: Option<usize>,
    #[command(flatten)]
    record: RecordOpts,
}

#[derive(Args, Debug)]
struct ReplayArgs {
    /// A manifest (`*.parquet`) or replay segment (`*.otr`) to verify: a local path inside a
    /// store directory, or an object key with `--store`.
    #[arg(long)]
    verify: String,
    /// The store to read from: `s3` or a local directory (default: found from the path).
    #[arg(long)]
    store: Option<String>,
    /// Verify only this game.
    #[arg(long)]
    game: Option<String>,
    /// Worker threads (default: every core).
    #[arg(long)]
    threads: Option<usize>,
}

#[derive(Args, Debug)]
struct ExtractArgs {
    /// A manifest (`*.parquet`) or replay segment (`*.otr`): a local path inside a store
    /// directory, or an object key with `--store`.
    source: String,
    /// The store to read from: `s3` or a local directory (default: found from the path).
    #[arg(long)]
    store: Option<String>,
    /// Output Parquet file.
    #[arg(long)]
    out: PathBuf,
    /// Older rules fingerprints, newest first, for the rules weight.
    #[arg(long, value_delimiter = ',')]
    rules_history: Vec<String>,
    /// Rules weight multiplier per generation of age.
    #[arg(long, default_value_t = 0.5)]
    rules_decay: f64,
    /// Lowest rules weight.
    #[arg(long, default_value_t = 0.0)]
    rules_floor: f64,
    /// Weight of a human game read from a bare segment (manifests carry their own).
    #[arg(long, default_value_t = 5.0)]
    human_weight: f64,
}

#[derive(Args, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches, not a state machine"
)]
struct StudyArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// Study definition file (docs/study.md).
    #[arg(long = "def", default_value = "study/study.yaml")]
    def: PathBuf,
    /// Output directory: models.csv, games.csv, summary.json, models/<id>/.
    #[arg(long, default_value = "study-out")]
    out: PathBuf,
    /// Override the definition's model count.
    #[arg(long)]
    models: Option<u32>,
    /// Override the definition's games per model.
    #[arg(long)]
    games: Option<u32>,
    /// Override the definition's preset.
    #[arg(long)]
    preset: Option<String>,
    /// Override the definition's base seed.
    #[arg(long)]
    seed: Option<u64>,
    /// Override the definition's roster: comma-separated personality ids and `idle`.
    #[arg(long, value_delimiter = ',')]
    roster: Option<Vec<String>>,
    /// Override how many balanced models get an overlay.
    #[arg(long)]
    top: Option<u32>,
    /// Worker threads (default: every core). Outputs are identical for any value.
    #[arg(long)]
    threads: Option<usize>,
    /// Also write games.csv (one row per game).
    #[arg(long)]
    games_csv: bool,
    /// Leave out wall-time columns so two runs diff byte for byte.
    #[arg(long)]
    no_wall_time: bool,
    /// Discard any study already in --out and start over.
    #[arg(long)]
    fresh: bool,
    /// Stop after running this many models (resume later with the same command).
    #[arg(long)]
    stop_after: Option<u32>,
    /// No progress lines on stderr.
    #[arg(long)]
    quiet: bool,
    /// Record every game: `s3` (from `OPENTYCOON_S3_*`) or a local directory.
    #[arg(long)]
    record: Option<String>,
    #[command(flatten)]
    record_opts: RecordOpts,
}

#[derive(Args, Debug)]
struct BatchArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// Games to play; seeds are `seed..seed+games`.
    #[arg(long, default_value_t = 1_000)]
    games: u64,
    /// First seed.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Game length preset from game.yaml (`v1` is 1930–1950).
    #[arg(long, default_value = "v1")]
    preset: String,
    /// Brain for the seats the AI opponents do not play: idle or scripted.
    #[arg(long, default_value = "idle", value_parser = parse_brain)]
    brain: BrainKind,
    /// Player seats.
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Worker threads (default: every core). The CSV is identical for any value.
    #[arg(long)]
    threads: Option<usize>,
    /// CSV output file. Without it the CSV goes to stdout and the summary to stderr.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Leave out the `wall_ms` column so two runs diff byte for byte.
    #[arg(long)]
    no_wall_time: bool,
    /// Also write the summary as JSON to this file.
    #[arg(long)]
    summary_json: Option<PathBuf>,
    /// Record every game: `s3` (from `OPENTYCOON_S3_*`) or a local directory.
    #[arg(long)]
    record: Option<String>,
    #[command(flatten)]
    record_opts: RecordOpts,
}

#[derive(Args, Debug)]
struct BenchArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// Game days per run.
    #[arg(long, default_value_t = 1)]
    days: u32,
    /// Throughput runs; the median is reported.
    #[arg(long, default_value_t = 5)]
    repeats: u32,
    /// Seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Player seats.
    #[arg(long, default_value_t = 2)]
    players: u8,
    /// Synthetic large scenario: set `customers.initial_population` to this.
    #[arg(long)]
    agents: Option<u64>,
    /// Game length preset from game.yaml.
    #[arg(long, default_value = "v1")]
    preset: String,
    /// Also time each tick phase (one extra run; adds a `phases` object to the JSON).
    #[arg(long)]
    phases: bool,
}

#[derive(Args, Debug)]
struct PerfArgs {
    /// Config directory.
    dir: PathBuf,
    /// Budgets file.
    #[arg(long, default_value = "perf/budgets.yaml")]
    budgets: PathBuf,
    /// Replace one budget's limit, `metric=limit` (repeatable), e.g. to try a tighter one.
    #[arg(long = "budget", value_name = "METRIC=LIMIT", value_parser = parse_budget)]
    limits: Vec<(Metric, f64)>,
    /// Measure only these metrics (repeatable; default: every budgeted metric).
    #[arg(long = "only", value_name = "METRIC", value_parser = parse_metric)]
    only: Vec<Metric>,
    /// Games in the sample batch for `batch_1000_games_s`.
    #[arg(long, default_value_t = 200)]
    batch_sample: u64,
    /// Throughput repeats per bench.
    #[arg(long, default_value_t = 5)]
    repeats: u32,
    /// Print verdicts as JSON instead of a table.
    #[arg(long)]
    json: bool,
}

fn read_sources(dir: &Path) -> Option<ConfigSources> {
    match ConfigSources::read_dir(dir) {
        Ok(s) => Some(s),
        Err(errors) => {
            eprint!("{errors}");
            None
        }
    }
}

/// Loads with a preset and overrides; prints every error and returns `None` on failure.
fn load_with(args: &ConfigArgs, preset: Option<&str>) -> Option<Config> {
    let sources = read_sources(&args.dir)?;
    match overrides::load(&sources, preset, &args.overrides) {
        Ok(config) => Some(config),
        Err(LoadError::Invalid(errors)) => {
            eprintln!("error: config with overrides is invalid:");
            eprint!("{errors}");
            None
        }
        Err(e) => {
            eprintln!("error: {e}");
            None
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::ValidateConfig { dir } => {
            let Some(sources) = read_sources(&dir) else {
                return Ok(ExitCode::FAILURE);
            };
            match Config::load(&sources) {
                Ok(config) => {
                    println!(
                        "ok: {} valid ({} blocks, {} business types, {}–{})",
                        dir.display(),
                        config.map.blocks.len(),
                        config.businesses.types.len(),
                        config.game.clock.start_year,
                        config.game.clock.end_year
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(errors) => {
                    eprint!("{errors}");
                    Ok(ExitCode::FAILURE)
                }
            }
        }
        Command::WriteSchemas { dir } => {
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            for file in ConfigFile::ALL {
                let path = dir.join(file.schema_file_name());
                let text = serde_json::to_string_pretty(&file.schema())? + "\n";
                std::fs::write(&path, text)
                    .with_context(|| format!("writing {}", path.display()))?;
                println!("wrote {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Run {
            config,
            ticks,
            seed,
            players,
        } => {
            let Some(config) = load_with(&config, None) else {
                return Ok(ExitCode::FAILURE);
            };
            let mut world = World::new(config, seed, players);
            for _ in 0..ticks {
                world.tick();
            }
            // CI compares this line across OSes (.github/workflows/ci.yml): keep it stable.
            println!(
                "ticks={ticks} seed={seed} year={} day={} hash={:016x}",
                world.clock().year(),
                world.clock().day(),
                world.state_hash()
            );
            Ok(ExitCode::SUCCESS)
        }
        Command::Batch(args) => batch_cmd(args),
        Command::Bench(args) => bench_cmd(args),
        Command::PerfBudget(args) => perf_budget_cmd(args),
        Command::Study(args) => study_cmd(args),
        Command::Record(args) => record_cmd(args),
        Command::Replay(args) => replay_cmd(&args),
        Command::Extract(args) => extract_cmd(&args),
        Command::RulesFingerprint => {
            println!("{}", rules_fingerprint().map_err(anyhow::Error::msg)?);
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// An open recording: the store, the writer's options and the header context.
struct Recording {
    store: Box<dyn ObjectStore>,
    run_id: String,
    opts: WriterOptions,
    ctx: RecordContext,
}

impl Recording {
    fn open(dest: &str, opts: &RecordOpts, default_run_id: String) -> Result<Self> {
        let store = replay::store::open(dest)?;
        let run_id = opts.run_id.clone().unwrap_or(default_run_id);
        replay::store::check_key(&run_id)
            .with_context(|| format!("run id {run_id:?} must be a plain name"))?;
        let fingerprint = rules_fingerprint().map_err(anyhow::Error::msg)?;
        Ok(Recording {
            store,
            opts: WriterOptions {
                segment_bytes: opts.segment_mib.max(1) << 20,
                weights: Weights {
                    human_multiplier: opts.human_weight,
                },
            },
            ctx: RecordContext {
                run_id: run_id.clone(),
                rules_fingerprint: fingerprint.to_owned(),
                git_commit: GIT_COMMIT.to_owned(),
                ..RecordContext::default()
            },
            run_id,
        })
    }

    fn report(&self, stats: &WriteStats, wall: std::time::Duration) -> String {
        let per = stats.replay_bytes.checked_div(stats.games).unwrap_or(0);
        format!(
            "recorded {} games into {}, run {}: {} replay bytes ({per} bytes/game), {} segments, \
             {} manifest parts, configs {} written / {} already stored, {} objects, {} bytes; \
             rules fingerprint {}; {:.2}s",
            stats.games,
            self.store.describe(),
            self.run_id,
            stats.replay_bytes,
            stats.segments,
            stats.manifest_parts,
            stats.configs_written,
            stats.configs_present,
            stats.objects,
            stats.bytes,
            self.ctx.rules_fingerprint,
            wall.as_secs_f64()
        )
    }
}

/// A run id derived from what decides the run's games, so a rerun overwrites itself.
fn default_run_id(kind: &str, inputs: &impl std::hash::Hash) -> String {
    format!("{kind}-{:012x}", stable_hash(inputs) & 0xffff_ffff_ffff)
}

fn record_cmd(args: RecordCmdArgs) -> Result<ExitCode> {
    let RecordCmdArgs {
        config,
        dest,
        games,
        seed,
        preset,
        brain,
        players,
        threads,
        record,
    } = args;
    record_batch(
        &config,
        &BatchSpec {
            games,
            base_seed: seed,
            threads,
            game: GameSpec {
                players,
                brain,
                ..GameSpec::default()
            },
        },
        &preset,
        &dest,
        &record,
        "record",
    )
    .map(|(_, code)| code)
}

/// Plays and records a batch; returns its report (unless it failed) and the exit code.
fn record_batch(
    config: &ConfigArgs,
    spec: &BatchSpec,
    preset: &str,
    dest: &str,
    opts: &RecordOpts,
    source: &str,
) -> Result<(Option<sim_cli::batch::BatchReport>, ExitCode)> {
    let Some(sources) = read_sources(&config.dir) else {
        return Ok((None, ExitCode::FAILURE));
    };
    let files = match overrides::resolve(&sources, Some(preset), &config.overrides) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok((None, ExitCode::FAILURE));
        }
    };
    let loaded = match Config::load(&files) {
        Ok(c) => c,
        Err(errors) => {
            eprintln!("error: config with overrides is invalid:");
            eprint!("{errors}");
            return Ok((None, ExitCode::FAILURE));
        }
    };
    let snap = snapshot(&files);
    let run_id = default_run_id(
        source,
        &(
            &snap.hash,
            preset,
            spec.base_seed,
            spec.games,
            spec.game.brain.name(),
            spec.game.players,
        ),
    );
    let mut rec = Recording::open(dest, opts, run_id)?;
    let watch = Stopwatch::start();
    let mut writer = RunWriter::new(rec.store.as_ref(), &rec.run_id, rec.opts);
    rec.ctx.config_hash.clone_from(&snap.hash);
    preset.clone_into(&mut rec.ctx.preset);
    let mut recording = BatchRecording {
        ctx: rec.ctx.clone(),
        snapshot: snap,
        writer: &mut writer,
        chunk: 4096,
        source: source.to_owned(),
    };
    let report = run_batch_recorded(&loaded, spec, &mut recording)?;
    let stats = writer.stats();
    eprintln!("{}", rec.report(&stats, watch.elapsed()));
    Ok((Some(report), ExitCode::SUCCESS))
}

/// The store and key a `--verify`/`extract` argument names.
fn locate(path: &str, store: Option<&str>) -> Result<(Box<dyn ObjectStore>, String)> {
    if let Some(dest) = store {
        return Ok((replay::store::open(dest)?, path.to_owned()));
    }
    let Some((root, key)) = load::split_local(Path::new(path)) else {
        anyhow::bail!(
            "{path} is not inside a recording directory (no replays/ or manifests/ in the \
             path); pass --store <s3|dir> and an object key"
        );
    };
    Ok((Box::new(LocalStore::new(root)), key))
}

fn replay_cmd(args: &ReplayArgs) -> Result<ExitCode> {
    let (store, key) = locate(&args.verify, args.store.as_deref())?;
    let mut games = load::games(store.as_ref(), &key).map_err(anyhow::Error::msg)?;
    if let Some(id) = &args.game {
        games.retain(|g| &g.replay.header.game_id == id);
        if games.is_empty() {
            anyhow::bail!("no game {id} in {key}");
        }
    }
    let build = rules_fingerprint().map_err(anyhow::Error::msg)?;
    // Refuse before simulating anything if any game was recorded under other rules.
    let foreign: Vec<_> = games
        .iter()
        .filter(|g| g.replay.header.rules_fingerprint != build)
        .collect();
    if let Some(first) = foreign.first() {
        let h = &first.replay.header;
        eprintln!(
            "error: {} of {} games were recorded under other rules; {}",
            foreign.len(),
            games.len(),
            VerifyError::RulesMismatch {
                game: h.game_id.clone(),
                replay: h.rules_fingerprint.clone(),
                build: build.to_owned(),
                commit: h.git_commit.clone(),
            }
        );
        return Ok(ExitCode::from(2));
    }
    let mut configs = std::collections::BTreeMap::new();
    for g in &games {
        let hash = &g.replay.header.config_hash;
        if !configs.contains_key(hash) {
            let c = load::config(store.as_ref(), hash).map_err(anyhow::Error::msg)?;
            configs.insert(hash.clone(), c);
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads.unwrap_or(0))
        .build()?;
    let watch = Stopwatch::start();
    let results: Vec<Result<replay::verify::Verified, VerifyError>> = pool.install(|| {
        use rayon::prelude::*;
        games
            .par_iter()
            .map(|g| verify(&g.replay, &configs[&g.replay.header.config_hash], build))
            .collect()
    });
    let mut failed = 0;
    let (mut midnights, mut commands) = (0, 0);
    for r in &results {
        match r {
            Ok(v) => {
                midnights += v.checkpoints;
                commands += v.commands;
            }
            Err(e) => {
                failed += 1;
                eprintln!("error: {e}");
            }
        }
    }
    if failed > 0 {
        eprintln!("{failed} of {} games failed verification", results.len());
        return Ok(ExitCode::FAILURE);
    }
    println!(
        "{} games verified: {midnights} midnights and {commands} commands match, rules \
         fingerprint {build}; {:.2}s",
        results.len(),
        watch.elapsed().as_secs_f64()
    );
    Ok(ExitCode::SUCCESS)
}

fn extract_cmd(args: &ExtractArgs) -> Result<ExitCode> {
    let (store, key) = locate(&args.source, args.store.as_deref())?;
    let games = load::games(store.as_ref(), &key).map_err(anyhow::Error::msg)?;
    let rules = RulesWeight {
        current: rules_fingerprint().map_err(anyhow::Error::msg)?.to_owned(),
        history: args.rules_history.clone(),
        decay: args.rules_decay,
        floor: args.rules_floor,
    };
    let sources: Vec<extract::Source<'_>> = games
        .iter()
        .map(|g| extract::Source {
            replay: &g.replay,
            object_key: &g.object_key,
            weight: g.row.as_ref().map(|r| r.weight),
        })
        .collect();
    let weights = Weights {
        human_multiplier: args.human_weight,
    };
    let (bytes, rows) = extract::extract(&sources, &rules, &weights)?;
    std::fs::write(&args.out, &bytes).with_context(|| format!("writing {}", args.out.display()))?;
    println!(
        "extract {} v{}: {rows} rows from {} games -> {}",
        extract::EXTRACTOR_ID,
        extract::EXTRACTOR_VERSION,
        games.len(),
        args.out.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn study_cmd(args: StudyArgs) -> Result<ExitCode> {
    let text = std::fs::read_to_string(&args.def)
        .with_context(|| format!("reading {}", args.def.display()))?;
    let mut def = match StudyDef::from_yaml(&text) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}: {e}", args.def.display());
            return Ok(ExitCode::FAILURE);
        }
    };
    if let Some(v) = args.models {
        def.models = v;
    }
    if let Some(v) = args.games {
        def.games_per_model = v;
    }
    if let Some(v) = args.preset {
        def.preset = v;
    }
    if let Some(v) = args.seed {
        def.seed = v;
    }
    if let Some(v) = args.roster {
        def.roster = Some(v.into_iter().map(|s| s.trim().to_owned()).collect());
    }
    if let Some(v) = args.top {
        def.top_n = v;
    }
    let Some(mut base) = read_sources(&args.config.dir) else {
        return Ok(ExitCode::FAILURE);
    };
    for ov in &args.config.overrides {
        if let Err(e) = overrides::apply(&mut base, ov) {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    }
    let opts = RunOptions {
        out: args.out.clone(),
        threads: args.threads,
        games_csv: args.games_csv,
        wall_time: !args.no_wall_time,
        fresh: args.fresh,
        stop_after: args.stop_after,
        progress: !args.quiet,
    };
    let watch = Stopwatch::start();
    let result = match &args.record {
        None => run_study(&def, &base, &opts),
        Some(dest) => {
            let base_hash = snapshot(&base).hash;
            let def_json = serde_json::to_string(&def)?;
            let run_id = default_run_id("study", &(def_json, base_hash));
            let rec = Recording::open(dest, &args.record_opts, run_id)?;
            let mut writer = RunWriter::new(rec.store.as_ref(), &rec.run_id, rec.opts);
            let mut recording = StudyRecording {
                ctx: rec.ctx.clone(),
                writer: &mut writer,
            };
            let r = run_study_recorded(&def, &base, &opts, Some(&mut recording));
            eprintln!("{}", rec.report(&writer.stats(), watch.elapsed()));
            r
        }
    };
    let report = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };
    println!(
        "study: {} models ({} run now, {} resumed), {} invalid, {} balanced; preset {}, {} games \
         per model; {} games in {:.1}s{}",
        report.ran + report.skipped,
        report.ran,
        report.skipped,
        report.invalid,
        report.balanced.len(),
        report.preset,
        report.games_per_model,
        report.games,
        report.wall.as_secs_f64(),
        if report.complete {
            String::new()
        } else {
            format!(
                "; incomplete, {} models left: rerun to resume",
                def.models.saturating_sub(report.ran + report.skipped)
            )
        }
    );
    println!("wrote {}", args.out.display());
    Ok(ExitCode::SUCCESS)
}

fn batch_cmd(args: BatchArgs) -> Result<ExitCode> {
    let BatchArgs {
        config,
        games,
        seed,
        preset,
        brain,
        players,
        threads,
        out,
        no_wall_time,
        summary_json,
        record,
        record_opts,
    } = args;
    let spec = BatchSpec {
        games,
        base_seed: seed,
        threads,
        game: GameSpec {
            players,
            brain,
            ..GameSpec::default()
        },
    };
    let report = if let Some(dest) = &record {
        match record_batch(&config, &spec, &preset, dest, &record_opts, "batch")? {
            (Some(report), _) => report,
            (None, code) => return Ok(code),
        }
    } else {
        let Some(loaded) = load_with(&config, Some(&preset)) else {
            return Ok(ExitCode::FAILURE);
        };
        run_batch(&loaded, &spec)?
    };
    let timing = if no_wall_time {
        Timing::Omit
    } else {
        Timing::Include
    };
    let summary = summarise(&report);
    let sets = config.overrides.iter().fold(String::new(), |mut s, o| {
        s.push_str(" --set ");
        s.push_str(&o.to_string());
        s
    });
    let header = format!(
        "batch: preset={preset} brain={brain} players={players} seeds={seed}..{}{sets}",
        seed.saturating_add(games)
    );
    if let Some(path) = &out {
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut w = BufWriter::new(file);
        write_csv(&mut w, &report.results, timing)?;
        w.flush()?;
        print!("{header}\n{summary}wrote {}\n", path.display());
    } else {
        let mut w = BufWriter::new(std::io::stdout().lock());
        write_csv(&mut w, &report.results, timing)?;
        w.flush()?;
        eprint!("{header}\n{summary}");
    }
    if let Some(path) = summary_json {
        std::fs::write(&path, serde_json::to_string_pretty(&summary)? + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn bench_cmd(args: BenchArgs) -> Result<ExitCode> {
    let BenchArgs {
        mut config,
        days,
        repeats,
        seed,
        players,
        agents,
        preset,
        phases,
    } = args;
    let scenario = match agents {
        Some(n) => {
            config
                .overrides
                .push(format!("customers.initial_population={n}").parse()?);
            format!("synthetic-{n}-agents")
        }
        None => "shipped".to_owned(),
    };
    let Some(loaded) = load_with(&config, Some(&preset)) else {
        return Ok(ExitCode::FAILURE);
    };
    let spec = BenchSpec {
        days,
        repeats,
        seed,
        players,
        phases,
    };
    let report = bench(&loaded, &scenario, &spec);
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(ExitCode::SUCCESS)
}

fn perf_budget_cmd(args: PerfArgs) -> Result<ExitCode> {
    let PerfArgs {
        dir,
        budgets,
        limits,
        only,
        batch_sample,
        repeats,
        json,
    } = args;
    let text = std::fs::read_to_string(&budgets)
        .with_context(|| format!("reading {}", budgets.display()))?;
    let mut file =
        Budgets::from_yaml(&text).map_err(|e| anyhow::anyhow!("{}: {e}", budgets.display()))?;
    let Some(sources) = read_sources(&dir) else {
        return Ok(ExitCode::FAILURE);
    };
    // The file's own limits must agree with each other (docs/perf.md); command-line overrides
    // are experiments and are not held to it.
    let clock = ClockRules::from_config(&overrides::load(&sources, None, &[])?.game.clock);
    let issues = consistency(&file, clock.ticks_per_day(), clock.real_millis_per_day());
    if !issues.is_empty() {
        for issue in &issues {
            eprintln!(
                "perf-budget: {}: inconsistent budgets: {issue}",
                budgets.display()
            );
        }
        return Ok(ExitCode::FAILURE);
    }
    for (metric, limit) in limits {
        file.budgets
            .entry(metric)
            .and_modify(|b| b.limit = limit)
            .or_insert(Budget {
                limit,
                baseline: None,
                why: "--budget on the command line".into(),
            });
    }
    if !only.is_empty() {
        file.budgets.retain(|m, _| only.contains(m));
    }
    let metrics: Vec<Metric> = file.budgets.keys().copied().collect();
    let spec = MeasureSpec {
        batch_sample,
        repeats,
        ..MeasureSpec::default()
    };
    let measured = measure(&sources, &metrics, &spec)?;
    let verdicts = check(&file, &measured);
    if json {
        println!("{}", serde_json::to_string_pretty(&verdicts)?);
    } else {
        print!("{}", table(&verdicts, file.tolerance));
    }
    if all_pass(&verdicts) {
        return Ok(ExitCode::SUCCESS);
    }
    for v in verdicts.iter().filter(|v| v.measured > v.allowed) {
        eprintln!(
            "perf-budget: {} is over budget: measured {:.4}, allowed {:.4} (limit {} + {:.0}%)",
            v.metric,
            v.measured,
            v.allowed,
            v.limit,
            file.tolerance * 100.0
        );
    }
    Ok(ExitCode::FAILURE)
}
