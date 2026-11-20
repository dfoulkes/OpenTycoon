//! AI opponents: personalities sharing one evaluation function (§10).
//!
//! Owner boundary: an AI sees the world read-only and returns [`Command`]s, exactly like a
//! human client (R40). It never mutates state directly, so AI and human play go through one
//! path: commands queued at phase 7 apply at phase 2 of the next tick.
//!
//! Each AI seat draws from **its own RNG stream**, seeded from the game seed and the seat, so
//! an AI's choices never shift the world's draws (docs/engineering.md §3).
//!
//! - [`grocer`]: the Grocer, "opens many cheap Low-tier shops" (§10, §10c v1).
//! - Other personalities in `ai.yaml` (Landlord, Toff, Speculator, Sir Penny Sugar) are post-v1;
//!   their seats sit inert.

pub mod grocer;

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::businesses::BuildRequest;
use crate::economy::{AccountId, PlayerId};
use crate::land::{BlockId, Footprint};
use crate::money::Money;
use crate::rng::{SimRng, seeded};
use crate::world::{Command, World};

pub use grocer::Grocer;

/// Decides an AI player's commands.
pub trait Brain {
    /// Looks at the world and returns this tick's commands. Must be deterministic given the
    /// world and the RNG.
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command>;
}

/// The personality id that plays as the [`Grocer`].
pub const GROCER: &str = "grocer";

/// What plays an AI seat.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeatBrain {
    /// The Grocer (§10).
    Grocer(Grocer),
    /// A personality with no brain yet (post-v1): does nothing.
    Inert,
}

impl SeatBrain {
    /// The brain for a personality id from `ai.yaml`.
    pub fn for_personality(personality: &str) -> Self {
        if personality == GROCER {
            SeatBrain::Grocer(Grocer::new(personality))
        } else {
            SeatBrain::Inert
        }
    }
}

/// An AI seat: which player it plays, which personality from `ai.yaml`, its brain's memory and
/// its own RNG stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSeat {
    /// The player it plays.
    pub player: PlayerId,
    /// Personality id.
    pub personality: String,
    /// The brain and what it remembers.
    pub brain: SeatBrain,
    /// The seat's own random stream: never the world's (§3 determinism).
    rng: SimRng,
}

impl AiSeat {
    /// A seat for `player` playing `personality`, its RNG derived from the game seed and the
    /// seat only.
    pub fn new(player: PlayerId, personality: impl Into<String>, game_seed: u64) -> Self {
        let personality = personality.into();
        AiSeat {
            player,
            brain: SeatBrain::for_personality(&personality),
            personality,
            rng: seat_rng(game_seed, player),
        }
    }

    /// Decides this tick's commands from the seat's own RNG stream.
    pub fn play(&mut self, world: &World) -> Vec<Command> {
        match &mut self.brain {
            SeatBrain::Grocer(g) => g.decide(world, self.player, &mut self.rng),
            SeatBrain::Inert => Vec::new(),
        }
    }
}

impl Hash for AiSeat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.player.hash(state);
        self.personality.hash(state);
        self.brain.hash(state);
        self.rng.get_word_pos().hash(state);
    }
}

/// An AI seat's private stream: the game seed mixed with the seat, so two seats (or two
/// games) never share one, and the world's own stream is never touched.
pub fn seat_rng(game_seed: u64, player: PlayerId) -> SimRng {
    seeded(game_seed ^ 0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(u64::from(player.0) + 101))
}

/// The first place on `block` where a `size` building of `type_id`, `height` tall, passes
/// every building rule (type, height R58, minimum footprint, inside the lot, street frontage
/// R22, no overlap), with its build cost. Scans the lot row by row from the origin and tries
/// both orientations, so the answer is deterministic. Reads only public state and runs the
/// same check the world runs for a `Build` command.
pub fn find_site(
    world: &World,
    owner: AccountId,
    block: BlockId,
    type_id: &str,
    size: [u32; 2],
    height: u32,
) -> Option<(Footprint, Money)> {
    let [lot_w, lot_d] = world.config().game.world.lot_size;
    let mut shapes = vec![(size[0], size[1])];
    if size[0] != size[1] {
        shapes.push((size[1], size[0]));
    }
    let year = world.clock().year();
    for y in 0..lot_d {
        for x in 0..lot_w {
            for &(w, d) in &shapes {
                let footprint = Footprint { x, y, w, d };
                if !footprint.touches_edge(lot_w, lot_d) {
                    continue;
                }
                let request = BuildRequest {
                    type_id: type_id.to_owned(),
                    block,
                    footprint,
                    height,
                    owner,
                    fit_out: crate::businesses::FitOutLevel::Medium,
                };
                if let Ok(cost) =
                    world
                        .businesses()
                        .quote_build(&request, world.config(), world.map(), year)
                {
                    return Some((footprint, cost));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seat_streams_differ_by_seat_and_seed() {
        let draw = |seed, p| crate::rng::below(&mut seat_rng(seed, PlayerId(p)), u64::MAX);
        assert_ne!(draw(1, 0), draw(1, 1));
        assert_ne!(draw(1, 0), draw(2, 0));
        assert_eq!(draw(3, 1), draw(3, 1));
    }

    #[test]
    fn only_the_grocer_has_a_brain_in_v1() {
        assert!(matches!(
            SeatBrain::for_personality("grocer"),
            SeatBrain::Grocer(_)
        ));
        assert_eq!(SeatBrain::for_personality("toff"), SeatBrain::Inert);
    }
}
