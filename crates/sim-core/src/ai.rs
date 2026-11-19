//! AI opponents: personalities sharing one evaluation function (§10).
//!
//! Owner boundary: an AI sees the world read-only and returns [`Command`]s, exactly like a
//! human client. It never mutates state directly, so AI and human play go through one path.
//!
//! Status: the trait and a do-nothing seat are real; the Grocer lands in M1.

use serde::{Deserialize, Serialize};

use crate::economy::PlayerId;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// Decides an AI player's commands.
pub trait Brain {
    /// Looks at the world and returns this tick's commands. Must be deterministic given the
    /// world and the RNG.
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command>;
}

/// An AI seat: which player it plays and which personality from `ai.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AiSeat {
    /// The player it plays.
    pub player: PlayerId,
    /// Personality id.
    pub personality: String,
}

impl Brain for AiSeat {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new()
    }
}
