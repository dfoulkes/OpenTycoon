//! `sim-cli replay --verify`: re-simulate a replay and compare every midnight.
//!
//! The replay's world is rebuilt from its config snapshot, seed and seats; the external
//! commands are re-submitted at their ticks; sim-core's AI regenerates its own. At every
//! midnight the state hash and each seat's net worth must equal the checkpoint. A replay
//! recorded under different rules (another rules fingerprint) is refused before anything runs.

use sim_core::Config;

use super::format::Replay;

/// Why a replay did not verify.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    /// Recorded under other rules: re-simulating would prove nothing.
    #[error(
        "game {game}: recorded under rules fingerprint {replay} (commit {commit}); this build's \
         rules fingerprint is {build}. Refusing to verify: check out that commit to re-simulate it"
    )]
    RulesMismatch {
        /// The game.
        game: String,
        /// The replay's fingerprint.
        replay: String,
        /// This build's.
        build: String,
        /// The commit that recorded it.
        commit: String,
    },
    /// A midnight differs.
    #[error(
        "game {game}: diverges at midnight {day} (tick {tick}): {what}{}",
        first_command.as_ref().map(|c| format!("; first command difference: {c}")).unwrap_or_default()
    )]
    Diverged {
        /// The game.
        game: String,
        /// The first diverging midnight.
        day: u32,
        /// Its tick.
        tick: u64,
        /// What differs.
        what: String,
        /// The first command that differed before it, if any.
        first_command: Option<String>,
    },
    /// Every midnight matched but something else did not (command log, outcome).
    #[error("game {game}: {what}")]
    Mismatch {
        /// The game.
        game: String,
        /// What differs.
        what: String,
    },
    /// The world could not be rebuilt.
    #[error("game {game}: {what}")]
    Setup {
        /// The game.
        game: String,
        /// What went wrong.
        what: String,
    },
}

/// A verified game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The game.
    pub game_id: String,
    /// Midnights compared.
    pub checkpoints: usize,
    /// Commands compared.
    pub commands: usize,
    /// Ticks re-simulated.
    pub ticks: u64,
}

/// Re-simulates `replay` under `config` (its snapshot) and compares every midnight. Refuses a
/// replay whose rules fingerprint is not `build_fingerprint`.
pub fn verify(
    replay: &Replay,
    config: &Config,
    build_fingerprint: &str,
) -> Result<Verified, VerifyError> {
    let _ = (config, build_fingerprint);
    Err(VerifyError::Setup {
        game: replay.header.game_id.clone(),
        what: "not implemented".into(),
    })
}
