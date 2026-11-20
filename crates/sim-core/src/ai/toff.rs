//! The Toff: "only goes for High tier" (§10).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `toff` (plus the `toff` personality's tier weights); every random
//! draw comes from the RNG it is handed, which for an AI seat is the seat's own stream.

use serde::{Deserialize, Serialize};

use super::Brain;
use crate::economy::PlayerId;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// The Toff's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Toff {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
}

impl Toff {
    /// The Toff playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Toff {
            personality: personality.into(),
        }
    }
}

impl Brain for Toff {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new() // not yet: the scenarios in features/ai/ come first
    }
}
