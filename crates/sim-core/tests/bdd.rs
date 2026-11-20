//! BDD harness: runs every `.feature` under `crates/sim-core/features/` with the `cucumber`
//! crate. One world type; each module adds its steps in `tests/steps/<module>.rs` and its state
//! as a field here. Undefined or skipped steps fail the run.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::trivially_copy_pass_by_ref,
    missing_docs,
    reason = "test harness; cucumber step signatures are fixed by the macros"
)]

mod common;
mod steps;

use cucumber::World as _;

/// Scenario state. Every scenario starts from `Default`.
#[derive(Debug, Default, cucumber::World)]
pub struct SimWorld {
    /// Config under test (`steps::config`).
    pub config: steps::config::ConfigState,
    /// Clock under test (`steps::clock`).
    pub clock: steps::clock::ClockState,
    /// Game under test (`steps::economy`).
    pub economy: steps::economy::EconomyState,
}

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features");
    futures::executor::block_on(
        SimWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(features),
    );
}
