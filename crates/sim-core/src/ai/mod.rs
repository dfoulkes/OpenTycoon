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
//! - [`landlord`]: the Landlord, "hoards leases and never builds retail".
//! - [`toff`]: the Toff, "only goes for High tier".
//! - [`speculator`]: the Speculator, "bids hard and flips".
//! - [`sir_penny_sugar`]: Sir Penny Sugar, "deals mostly in residential" (§7.2).
//! - [`kit`]: what they share. Every personality chooses business types from what `businesses.yaml`
//!   says is unlocked and not obsolete in the current year ([`kit::in_era`]), so they keep
//!   working as era content lands.
//!
//! A personality id with no brain (none in the shipped `ai.yaml`) sits inert.

pub mod grocer;
pub mod kit;
pub mod landlord;
pub mod sir_penny_sugar;
pub mod speculator;
pub mod toff;

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::businesses::BuildRequest;
use crate::economy::{AccountId, PlayerId};
use crate::land::{BlockId, Footprint};
use crate::money::Money;
use crate::rng::{SimRng, seeded};
use crate::world::{Command, World};

pub use grocer::Grocer;
pub use landlord::Landlord;
pub use sir_penny_sugar::PennySugar;
pub use speculator::Speculator;
pub use toff::Toff;

/// Decides an AI player's commands.
pub trait Brain {
    /// Looks at the world and returns this tick's commands. Must be deterministic given the
    /// world and the RNG.
    fn decide(&mut self, world: &World, me: PlayerId, rng: &mut SimRng) -> Vec<Command>;
}

/// The personality id that plays as the [`Grocer`].
pub const GROCER: &str = "grocer";
/// The personality id that plays as the [`Landlord`].
pub const LANDLORD: &str = "landlord";
/// The personality id that plays as the [`Toff`].
pub const TOFF: &str = "toff";
/// The personality id that plays as the [`Speculator`].
pub const SPECULATOR: &str = "speculator";
/// The personality id that plays as [`PennySugar`].
pub const SIR_PENNY_SUGAR: &str = "sir_penny_sugar";

/// What plays an AI seat.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeatBrain {
    /// The Grocer (§10).
    Grocer(Grocer),
    /// The Landlord (§10).
    Landlord(Landlord),
    /// The Toff (§10).
    Toff(Toff),
    /// The Speculator (§10).
    Speculator(Speculator),
    /// Sir Penny Sugar (§10, §7.2).
    PennySugar(PennySugar),
    /// A personality id with no brain: does nothing.
    Inert,
}

impl SeatBrain {
    /// The brain for a personality id from `ai.yaml`.
    pub fn for_personality(personality: &str) -> Self {
        match personality {
            GROCER => SeatBrain::Grocer(Grocer::new(personality)),
            LANDLORD => SeatBrain::Landlord(Landlord::new(personality)),
            TOFF => SeatBrain::Toff(Toff::new(personality)),
            SPECULATOR => SeatBrain::Speculator(Speculator::new(personality)),
            SIR_PENNY_SUGAR => SeatBrain::PennySugar(PennySugar::new(personality)),
            _ => SeatBrain::Inert,
        }
    }

    /// The brain as a [`Brain`], or `None` if inert.
    pub fn brain_mut(&mut self) -> Option<&mut dyn Brain> {
        match self {
            SeatBrain::Grocer(b) => Some(b),
            SeatBrain::Landlord(b) => Some(b),
            SeatBrain::Toff(b) => Some(b),
            SeatBrain::Speculator(b) => Some(b),
            SeatBrain::PennySugar(b) => Some(b),
            SeatBrain::Inert => None,
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
        match self.brain.brain_mut() {
            Some(brain) => brain.decide(world, self.player, &mut self.rng),
            None => Vec::new(),
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
    fn every_shipped_personality_has_a_brain() {
        for id in [GROCER, LANDLORD, TOFF, SPECULATOR, SIR_PENNY_SUGAR] {
            assert_ne!(SeatBrain::for_personality(id), SeatBrain::Inert, "{id}");
        }
        assert_eq!(SeatBrain::for_personality("nobody"), SeatBrain::Inert);
    }
}
