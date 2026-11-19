//! `sim-cli`: the headless harness.
//!
//! - `sim-cli validate-config <dir>`: load and validate a config directory; exit 1 naming every
//!   bad key and file.
//! - `sim-cli write-schemas <dir>`: regenerate `*.schema.json` from the Rust config types.
//! - `sim-cli run --ticks N --seed S <dir>`: run a headless game and print the state hash.
//!
//! The 1,000-game balance run and `bench` land in M1 (docs/engineering.md §Performance).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use sim_core::config::ConfigFile;
use sim_core::{Config, ConfigSources, World};

#[derive(Parser)]
#[command(name = "sim-cli", about = "OpenTycoon headless harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
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
        /// Config directory.
        dir: PathBuf,
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
}

fn load(dir: &std::path::Path) -> Result<Config, sim_core::ConfigErrors> {
    let sources = ConfigSources::read_dir(dir)?;
    Config::load(&sources)
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
        Command::ValidateConfig { dir } => match load(&dir) {
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
        },
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
            dir,
            ticks,
            seed,
            players,
        } => match load(&dir) {
            Ok(config) => {
                let mut world = World::new(config, seed, players);
                for _ in 0..ticks {
                    world.tick();
                }
                println!(
                    "ticks={ticks} seed={seed} year={} day={} hash={:016x}",
                    world.clock().year(),
                    world.clock().day(),
                    world.state_hash()
                );
                Ok(ExitCode::SUCCESS)
            }
            Err(errors) => {
                eprint!("{errors}");
                Ok(ExitCode::FAILURE)
            }
        },
    }
}
