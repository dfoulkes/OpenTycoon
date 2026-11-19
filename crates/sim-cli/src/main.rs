//! `sim-cli`: the headless harness.
//!
//! - `sim-cli validate-config <dir>`: load and validate a config directory; exit 1 naming every
//!   bad key and file.
//! - `sim-cli write-schemas <dir>`: regenerate `*.schema.json` from the Rust config types.
//! - `sim-cli run --ticks N --seed S <dir>`: run a headless game and print the state hash.
//! - `sim-cli spec-sync [--check]`: refresh `docs/spec.md` from the vault spec and report which
//!   rulings were added, removed or reworded (see `spec.rs`).
//!
//! The 1,000-game balance run and `bench` land in M1 (docs/engineering.md §Performance).

mod spec;

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
    /// Refresh docs/spec.md from the canonical vault spec and report ruling changes.
    SpecSync {
        /// Vault spec. Default: `$OPENTYCOON_VAULT_SPEC`, else
        /// `~/Vaults/Homelab/Projects/opentycoon/spec.md`.
        #[arg(long)]
        vault: Option<PathBuf>,
        /// Repo root (holds docs/spec.md and crates/sim-core/features).
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Only report; exit 1 if the snapshot is out of date. Writes nothing.
        #[arg(long)]
        check: bool,
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
        Command::SpecSync { vault, repo, check } => spec_sync(vault, &repo, check),
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

fn spec_sync(vault: Option<PathBuf>, repo: &std::path::Path, check: bool) -> Result<ExitCode> {
    let vault = vault
        .or_else(spec::default_vault_path)
        .context("no vault path: pass --vault or set OPENTYCOON_VAULT_SPEC")?;
    let vault_text = std::fs::read_to_string(&vault)
        .with_context(|| format!("reading the vault spec at {}", vault.display()))?;
    let repo_spec = repo.join("docs/spec.md");
    let current = std::fs::read_to_string(&repo_spec)
        .with_context(|| format!("reading {}", repo_spec.display()))?;
    let old_body = spec::body(&current);
    if old_body == vault_text {
        println!(
            "docs/spec.md is up to date with {} (snapshot taken {})",
            vault.display(),
            spec::taken(&current).unwrap_or("unknown")
        );
        return Ok(ExitCode::SUCCESS);
    }

    let changes = spec::ruling_changes(old_body, &vault_text);
    let fmt = |ids: &std::collections::BTreeSet<u32>| {
        if ids.is_empty() {
            "(none)".to_owned()
        } else {
            ids.iter()
                .map(|i| format!("R{i}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    println!(
        "docs/spec.md differs from {} (snapshot taken {})",
        vault.display(),
        spec::taken(&current).unwrap_or("unknown")
    );
    println!("  rulings added:    {}", fmt(&changes.added));
    println!("  rulings removed:  {}", fmt(&changes.removed));
    println!("  rulings reworded: {}", fmt(&changes.reworded));
    let mut recheck = changes.reworded.clone();
    recheck.extend(&changes.removed);
    let tagged = spec::features_tagging(&repo.join("crates/sim-core/features"), &recheck);
    for (id, files) in &tagged {
        println!("    re-check R{id}: {}", files.join(", "));
    }
    if changes.is_empty() {
        println!("  (no ruling changed; prose only)");
    }

    if check {
        println!("run `cargo spec-sync` to refresh it");
        return Ok(ExitCode::FAILURE);
    }
    let date = spec::today_utc();
    std::fs::write(&repo_spec, spec::snapshot(&vault_text, &date))
        .with_context(|| format!("writing {}", repo_spec.display()))?;
    println!("wrote {} (taken {date})", repo_spec.display());
    println!(
        "next: `cargo trace` (new rulings fail until tagged, pending or out of scope), then \
         re-read the scenarios listed above and commit docs/spec.md"
    );
    Ok(ExitCode::SUCCESS)
}
