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
//! - [`study`]: the balance study (R64, R65): sampled models, rotated seats, balance metrics.
//!
//! Nothing here implements a game rule: the harness drives `sim_core::World` and measures
//! whatever it does.

pub mod batch;
pub mod bench;
pub mod brains;
pub mod budget;
pub mod game;
pub mod overrides;
pub mod replay;
pub mod stats;
pub mod study;
pub mod timing;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::OnceLock;

    use sim_core::{Config, ConfigSources};

    /// The repo's shipped config. `config/` is read once per test binary, so a file that
    /// changes while the tests run (an edit, a branch switch in the same checkout) can't change
    /// the config half way through; property tests call this before their first case, so a
    /// broken file fails the test once instead of being shrunk as a counterexample.
    pub fn shipped_config() -> Config {
        static SHIPPED: OnceLock<Result<ConfigSources, String>> = OnceLock::new();
        let sources = SHIPPED.get_or_init(|| {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
            ConfigSources::read_dir(&dir).map_err(|e| format!("reading shipped config: {e}"))
        });
        match sources {
            Ok(sources) => Config::load(sources).unwrap(),
            Err(e) => panic!("{e}"),
        }
    }
}
