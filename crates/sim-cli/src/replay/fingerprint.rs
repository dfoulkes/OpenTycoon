//! The build's rules fingerprint and git commit.
//!
//! The **rules fingerprint** is the final state hash of a fixed reference game (seed
//! [`REFERENCE_SEED`], the [`REFERENCE_PRESET`] preset, [`REFERENCE_PLAYERS`] seats, the
//! *shipped* config compiled into this binary) as this build plays it. Two builds with the same
//! fingerprint play the reference game identically, so a replay recorded by one can be
//! re-simulated by the other. Any rule change moves it. It is computed once per process.

use sim_core::ConfigSources;

/// Seed of the reference game.
pub const REFERENCE_SEED: u64 = 42;
/// Preset of the reference game.
pub const REFERENCE_PRESET: &str = "full";
/// Seats in the reference game.
pub const REFERENCE_PLAYERS: u8 = 2;

/// The sim build's git commit (`unknown` if it was built outside a git checkout).
pub const GIT_COMMIT: &str = "unknown";

/// The shipped config, as compiled into this binary.
pub fn shipped_sources() -> ConfigSources {
    ConfigSources::new()
}

/// The reference game's final state hash under `sources`, as 16 hex digits.
pub fn reference_fingerprint(sources: &ConfigSources) -> Result<String, String> {
    let _ = sources;
    Err("not implemented".into())
}

/// This build's rules fingerprint, computed on first use and cached for the process.
pub fn rules_fingerprint() -> Result<&'static str, String> {
    Err("not implemented".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::World;

    #[test]
    fn the_compiled_in_config_is_the_shipped_config() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        assert_eq!(shipped_sources(), ConfigSources::read_dir(&dir).unwrap());
    }

    /// The fingerprint is the reference game's final state hash: the same number
    /// `sim-cli run config --seed 42 --ticks 207360` prints for the full game.
    #[test]
    fn the_fingerprint_is_the_reference_games_final_hash() {
        let sources = shipped_sources();
        let config = crate::overrides::load(&sources, Some(REFERENCE_PRESET), &[]).unwrap();
        let mut world = World::new(config, REFERENCE_SEED, REFERENCE_PLAYERS);
        while !world.clock().is_over() {
            world.tick();
        }
        let expected = format!("{:016x}", world.state_hash());
        assert_eq!(reference_fingerprint(&sources).unwrap(), expected);
        assert_eq!(rules_fingerprint().unwrap(), expected);
    }

    #[test]
    fn the_git_commit_is_a_hash_or_unknown() {
        assert!(
            GIT_COMMIT == "unknown"
                || (GIT_COMMIT.len() == 40 && GIT_COMMIT.chars().all(|c| c.is_ascii_hexdigit())),
            "{GIT_COMMIT}"
        );
    }
}
