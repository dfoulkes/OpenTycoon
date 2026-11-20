//! Wall-clock measurement for the harness. sim-core never reads the wall clock (clippy bans
//! `Instant::now` workspace-wide); the harness measures how long the sim takes, so this is the
//! one sanctioned place it reads it. Timings never feed back into a game.

use std::time::{Duration, Instant};

/// A started stopwatch.
#[derive(Debug, Clone, Copy)]
pub struct Stopwatch(Instant);

impl Stopwatch {
    /// Starts now.
    #[allow(
        clippy::disallowed_methods,
        reason = "the harness measures wall time; the reading never reaches the sim"
    )]
    pub fn start() -> Self {
        Stopwatch(Instant::now())
    }

    /// Time since start.
    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    /// Time from `earlier` to this stopwatch's start (zero if `earlier` started later).
    pub fn since(&self, earlier: &Stopwatch) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }
}
