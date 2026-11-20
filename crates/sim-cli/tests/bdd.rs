//! BDD harness for sim-cli: runs every `.feature` under `crates/sim-cli/features/` (batch,
//! bench, overrides, determinism, perf budget). Scenarios drive the library directly, or the
//! built `sim-cli` binary when the behaviour is the command line itself (exit codes, files).
//! Undefined or skipped steps fail the run.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::trivially_copy_pass_by_ref,
    missing_docs,
    reason = "test harness; cucumber step signatures are fixed by the macros"
)]

mod steps;

use cucumber::World as _;

/// Scenario state. Every scenario starts from `Default`.
#[derive(Debug, Default, cucumber::World)]
pub struct HarnessWorld {
    /// Config, preset and override steps (`steps::config`).
    pub config: steps::config::ConfigState,
    /// Batches and brains (`steps::batch`).
    pub batch: steps::batch::BatchState,
    /// Command-line runs (`steps::cli`).
    pub cli: steps::cli::CliState,
}

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features");
    futures::executor::block_on(
        HarnessWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(features),
    );
}
