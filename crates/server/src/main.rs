//! `opentycoon-server`: the authoritative game server (R40, R44, R88).
//!
//! - `opentycoon-server --config DIR`: loads and validates the nine sim config files and
//!   `server.yaml`, refusing to start on a missing or bad one (§10b, R71), and prints `ok`.
//! - `--serve --token HEX`: then serves one inspector client on `127.0.0.1` (a port the OS
//!   chooses, printed as `listening <addr>`) until it leaves.
//! - `--write-schema DIR`: writes `server.schema.json` from the `server.yaml` types.
//!
//! Single player does not need this binary: `client-ext` runs the same server in-process.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use server::{Server, ServerConfig, config::SCHEMA_FILE_NAME};
use sim_core::{Config, ConfigSources};

#[derive(Parser)]
#[command(name = "opentycoon-server", about = "OpenTycoon game server")]
struct Args {
    /// Config directory.
    #[arg(long, default_value = "config")]
    config: PathBuf,
    /// Serve one inspector client after the checks.
    #[arg(long, requires = "token")]
    serve: bool,
    /// The one-time token the client must say hello with (`server.yaml` `token`).
    #[arg(long)]
    token: Option<String>,
    /// Write `server.schema.json` into this directory and exit.
    #[arg(long)]
    write_schema: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if let Some(dir) = args.write_schema {
        let text = serde_json::to_string_pretty(&ServerConfig::schema()).unwrap_or_default() + "\n";
        return match std::fs::write(dir.join(SCHEMA_FILE_NAME), text) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("writing {SCHEMA_FILE_NAME}: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let sim = ConfigSources::read_dir(&args.config).and_then(|s| Config::load(&s));
    let server = ServerConfig::read_dir(&args.config);
    if sim.is_err() || server.is_err() {
        for errors in [sim.err(), server.err()].into_iter().flatten() {
            eprint!("refusing to start: {errors}");
        }
        return ExitCode::FAILURE;
    }
    if !args.serve {
        println!("ok");
        return ExitCode::SUCCESS;
    }
    let token = args.token.unwrap_or_default();
    let result = Server::bind(&args.config, &token).and_then(|s| {
        println!("listening {}", s.local_addr()?);
        s.serve_one()
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
