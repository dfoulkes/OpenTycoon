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
    /// Businesses under test (`steps::businesses`).
    pub businesses: steps::businesses::BusinessesState,
    /// Lot under test (`steps::land`).
    pub land: steps::land::LandState,
    /// Game driven through auction commands (`steps::auctions`).
    pub auctions: steps::auctions::AuctionState,
    /// City scene: population, apartments, shoppers (`steps::population`, `steps::customers`).
    pub city: steps::population::CityState,
    /// A game with the Grocer in seat 1 (`steps::ai`).
    pub ai: steps::ai::AiState,
    /// Two games for determinism checks (`steps::events`).
    pub events: steps::events::EventsState,
    /// Hotel scenarios (`steps::hotels`).
    pub hotels: steps::hotels::HotelsState,
    /// The AI roster, driven seat by seat (`steps::roster`).
    pub roster: steps::roster::RosterState,
    /// The full board outside a game (`steps::board`).
    pub board: steps::board::BoardState,
}

/// Stack for the thread that runs the scenarios. Unoptimised builds keep every temporary of a
/// step's `World` on the stack, and the steps run inside one deep future, so the default main
/// thread stack (8 MiB) is too tight.
const STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features");
    let runner = std::thread::Builder::new()
        .name("bdd".into())
        .stack_size(STACK_BYTES)
        .spawn(move || {
            futures::executor::block_on(
                SimWorld::cucumber()
                    .fail_on_skipped()
                    .run_and_exit(features),
            );
        })
        .expect("spawn the scenario thread");
    if runner.join().is_err() {
        std::process::exit(1);
    }
}
