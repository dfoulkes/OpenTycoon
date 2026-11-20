//! The OpenTycoon headless harness (spec §11): everything `sim-cli` does, as a library so the
//! BDD scenarios and tests drive the same code as the binary.
//!
//! - [`overrides`]: `--set file.key=value` and `--preset`, through the normal config loader.
//! - [`brains`]: trivial `idle` and `scripted` players for the seats `ai.yaml` does not fill.
//! - [`game`]: one game to the final whistle.
//! - [`batch`]: N games in parallel, CSV + summary, identical on any thread count.
//! - [`bench`]: ticks/sec, ms per game day, real-time factor, tick latency.
//! - [`budget`]: perf budgets and the regression check.
//! - [`stats`]: nearest-rank percentiles.
//!
//! Nothing here implements a game rule: the harness drives `sim_core::World` and measures
//! whatever it does.

pub mod batch;
pub mod bench;
pub mod brains;
pub mod budget;
pub mod game;
pub mod overrides;
pub mod stats;
pub mod timing;

#[cfg(test)]
pub(crate) mod test_support {
    use sim_core::{Config, ConfigSources};

    /// The repo's shipped config.
    pub fn shipped_config() -> Config {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        Config::load(&ConfigSources::read_dir(&dir).unwrap()).unwrap()
    }
}
