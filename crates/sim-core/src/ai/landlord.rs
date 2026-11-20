//! The Landlord: "hoards leases and never builds retail" (§10).
//!
//! It plays only through public [`Command`]s and reads only the world's public state. Every
//! tunable is `ai.yaml` `landlord` (plus the `landlord` personality's tier weights); every random
//! draw comes from the RNG it is handed, which for an AI seat is the seat's own stream.

use serde::{Deserialize, Serialize};

use super::Brain;
use crate::economy::PlayerId;
use crate::rng::SimRng;
use crate::world::{Command, World};

/// The Landlord's memory between decisions. Everything else it reads from the world.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Landlord {
    /// Its personality id in `ai.yaml` (for the tier weights).
    personality: String,
}

impl Landlord {
    /// The Landlord playing `personality` from `ai.yaml`.
    pub fn new(personality: impl Into<String>) -> Self {
        Landlord {
            personality: personality.into(),
        }
    }
}

impl Brain for Landlord {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new() // not yet: the scenarios in features/ai/ come first
    }
}
