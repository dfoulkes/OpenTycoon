//! Harness brains: trivial, deterministic players for the seats `ai.yaml` does not fill.
//!
//! They implement sim-core's [`Brain`] trait, so they see the world read-only and return
//! [`Command`]s exactly like a human client (R40). The real opponents (the Grocer, wave 3) live
//! in sim-core's `ai` module and play their own seats inside `World::tick`; these only stand in
//! for the human seat(s) so a balance batch can run without a person.
//!
//! - `idle`: does nothing. The control group: what the economy does to a player who never acts.
//! - `scripted`: a fixed opening. Once day trade is open and no auction is running, it opens an
//!   auction on the cheapest unleased buildable block at its base price (R23), once. Building
//!   one shop is the second step of the script; it waits for a build command in sim-core
//!   (businesses, wave 2) and is a no-op until then.

use std::fmt;
use std::str::FromStr;

use sim_core::ai::Brain;
use sim_core::clock::TradingWindow;
use sim_core::economy::PlayerId;
use sim_core::land::{BlockId, BlockKind};
use sim_core::rng::SimRng;
use sim_core::world::{Command, World};

/// Which harness brain plays the free seats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum BrainKind {
    /// Does nothing.
    #[default]
    Idle,
    /// Fixed opening: bid on the cheapest lot (then build one shop, when sim-core can).
    Scripted,
}

impl BrainKind {
    /// Every kind, for help text and tests.
    pub const ALL: [BrainKind; 2] = [BrainKind::Idle, BrainKind::Scripted];

    /// A fresh brain of this kind for one seat of one game.
    pub fn build(self) -> Box<dyn Brain> {
        match self {
            BrainKind::Idle => Box::new(Idle),
            BrainKind::Scripted => Box::new(Scripted::default()),
        }
    }

    /// Its command-line name.
    pub const fn name(self) -> &'static str {
        match self {
            BrainKind::Idle => "idle",
            BrainKind::Scripted => "scripted",
        }
    }
}

impl fmt::Display for BrainKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for BrainKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BrainKind::ALL
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| format!("unknown brain `{s}` (expected idle or scripted)"))
    }
}

/// Never does anything.
#[derive(Debug, Clone, Copy, Default)]
pub struct Idle;

impl Brain for Idle {
    fn decide(&mut self, _world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        Vec::new()
    }
}

/// Where the scripted opening has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Step {
    /// Waiting to open the auction.
    #[default]
    Bid,
    /// Auction opened; building waits for sim-core's build command.
    Build,
}

/// A fixed, deterministic opening. Uses no randomness.
#[derive(Debug, Clone, Default)]
pub struct Scripted {
    step: Step,
}

/// The cheapest unleased buildable block, lowest id on a tie.
pub fn cheapest_free_lot(world: &World) -> Option<(BlockId, sim_core::Money)> {
    world
        .map()
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == BlockKind::Buildable && b.lease.is_none())
        .filter_map(|(i, b)| Some((BlockId(u32::try_from(i).ok()?), b.land_base_price)))
        .min_by_key(|(id, price)| (*price, *id))
}

impl Brain for Scripted {
    fn decide(&mut self, world: &World, _me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        match self.step {
            Step::Bid => {
                if !world.clock().is_open(TradingWindow::Day) || world.auctions().is_busy() {
                    return Vec::new();
                }
                let Some((block, bid)) = cheapest_free_lot(world) else {
                    return Vec::new();
                };
                self.step = Step::Build;
                vec![Command::OpenLandAuction { block, bid }]
            }
            // No build command exists in sim-core yet (businesses, wave 2).
            Step::Build => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for kind in BrainKind::ALL {
            assert_eq!(kind.name().parse::<BrainKind>(), Ok(kind));
        }
        assert!("grocer".parse::<BrainKind>().is_err());
    }
}
