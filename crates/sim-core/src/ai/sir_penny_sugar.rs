//! Sir Penny Sugar: "deals mostly in residential" (§10, §7.2).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `sir_penny_sugar` (plus the `sir_penny_sugar` personality's tier weights); every random
//! draw comes from the RNG it is handed, which for an AI seat is the seat's own stream.

use serde::{Deserialize, Serialize};

use super::Brain;
use crate::economy::PlayerId;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// Sir Penny Sugar's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PennySugar {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
}

impl PennySugar {
    /// Sir Penny Sugar playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        PennySugar {
            personality: personality.into(),
        }
    }
}

impl Brain for PennySugar {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new() // not yet: the scenarios in features/ai/ come first
    }
}
