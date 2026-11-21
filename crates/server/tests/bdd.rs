//! BDD harness for the server: runs every `.feature` under `crates/server/features/` (the
//! inspector session and connection, R88, R40, R44). Session scenarios drive
//! `server::Session` directly; connection scenarios talk to a real server over a socket.
//! Undefined or skipped steps fail the run.
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
pub struct ServerWorld {
    /// Session scenarios (`steps::session`).
    pub session: steps::session::SessionState,
    /// Connection scenarios (`steps::connection`).
    pub net: steps::connection::NetState,
}

fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/features");
    futures::executor::block_on(
        ServerWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit(features),
    );
}
