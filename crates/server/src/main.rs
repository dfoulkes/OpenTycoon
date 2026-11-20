//! `opentycoon-server`: the authoritative game server (R40, R44).
//!
//! M0 placeholder: loads and validates the config, refusing to start on a bad one (§10b), and
//! prints `ok`. The WebSocket server, snapshots and single-player localhost mode land in M2.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use sim_core::{Config, ConfigSources};

#[derive(Parser)]
#[command(
    name = "opentycoon-server",
    about = "OpenTycoon game server (placeholder)"
)]
struct Args {
    /// Config directory.
    #[arg(long, default_value = "config")]
    config: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match ConfigSources::read_dir(&args.config).and_then(|s| Config::load(&s)) {
        Ok(_config) => {
            println!("ok");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            eprint!("refusing to start: {errors}");
            ExitCode::FAILURE
        }
    }
}
