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

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use sim_cli::batch::{BatchSpec, Timing, run_batch, summarise, write_csv};
use sim_cli::bench::{BenchSpec, bench};
use sim_cli::brains::BrainKind;
use sim_cli::budget::{Budget, Budgets, MeasureSpec, Metric, all_pass, check, measure, table};
use sim_cli::game::GameSpec;
use sim_cli::overrides::{self, LoadError, Override};
use sim_core::config::ConfigFile;
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
    /// `--set game.overdraft.start_limit=20000`. Validated like a hand-edited file.
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
            // CI compares this line across OSes (docs/ci/ci.yml.proposed): keep it stable.
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
    }
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
    } = args;
    let Some(loaded) = load_with(&config, Some(&preset)) else {
        return Ok(ExitCode::FAILURE);
    };
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
    let report = run_batch(&loaded, &spec)?;
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
    let Some(sources) = read_sources(&dir) else {
        return Ok(ExitCode::FAILURE);
    };
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
