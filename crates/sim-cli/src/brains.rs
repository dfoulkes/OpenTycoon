//! Harness brains: trivial, deterministic players for the seats `ai.yaml` does not fill.
//!
//! They implement sim-core's [`Brain`] trait, so they see the world read-only and return
//! [`Command`]s exactly like a human client (R40). The real opponents (the Grocer, wave 3) live
//! in sim-core's `ai` module and play their own seats inside `World::tick`; these only stand in
//! for the human seat(s) so a balance batch can run without a person.
//!
//! - `idle`: does nothing. The control group: what the economy does to a player who never acts.
//! - `scripted`: a fixed opening that exercises land, building and stock. Once day trade is
//!   open and no auction is running, it opens an auction on the cheapest unleased buildable
//!   block at its base price (R23), retrying (at most [`SCRIPTED_BIDS`] times) if it is
//!   outbid. Once it holds a lease it builds one [`SCRIPTED_SHOP`] at the first legal site on
//!   that lot (R22, R58) and orders its full capacity of every good it sells (R2, R10).

use std::fmt;
use std::str::FromStr;

use sim_core::ai::{Brain, find_site};
use sim_core::businesses::FitOutLevel;
use sim_core::clock::TradingWindow;
use sim_core::economy::{AccountId, PlayerId};
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

/// Opening bids the scripted brain makes before it gives up on land.
pub const SCRIPTED_BIDS: u32 = 3;

/// The shop the scripted brain builds: a grocery, 3×3, 10 units tall (within its `max_height`).
pub const SCRIPTED_SHOP: (&str, [u32; 2], u32) = ("grocery", [3, 3], 10);

/// Where the scripted opening has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Step {
    /// Bidding for a lot until it holds one.
    #[default]
    Bid,
    /// Holds a lease; building one shop.
    Build,
    /// Built; ordering stock.
    Stock,
    /// Script finished.
    Done,
}

/// A fixed, deterministic opening. Uses no randomness.
#[derive(Debug, Clone, Default)]
pub struct Scripted {
    step: Step,
    bids: u32,
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

/// The first block `me` holds the lease on.
fn held_lot(world: &World, me: AccountId) -> Option<BlockId> {
    world
        .map()
        .blocks
        .iter()
        .position(|b| b.lease.as_ref().is_some_and(|l| l.holder == me))
        .and_then(|i| u32::try_from(i).ok())
        .map(BlockId)
}

impl Scripted {
    fn bid(&mut self, world: &World, me: AccountId) -> Vec<Command> {
        if !world.clock().is_open(TradingWindow::Day) || world.auctions().is_busy() {
            return Vec::new();
        }
        if held_lot(world, me).is_some() {
            self.step = Step::Build;
            return Vec::new();
        }
        if self.bids >= SCRIPTED_BIDS {
            self.step = Step::Done;
            return Vec::new();
        }
        let Some((block, bid)) = cheapest_free_lot(world) else {
            return Vec::new();
        };
        self.bids += 1;
        vec![Command::OpenLandAuction { block, bid }]
    }

    fn build(&mut self, world: &World, me: PlayerId) -> Vec<Command> {
        let account = AccountId::Player(me);
        let (type_id, size, height) = SCRIPTED_SHOP;
        let Some(block) = held_lot(world, account) else {
            return Vec::new();
        };
        match find_site(world, account, block, type_id, size, height) {
            Some((footprint, cost)) if world.can_afford(me, cost) => {
                self.step = Step::Stock;
                vec![Command::Build {
                    block,
                    type_id: type_id.to_owned(),
                    footprint,
                    height,
                    fit_out: FitOutLevel::Medium,
                }]
            }
            // No room or no money: the script stops rather than retry for ever.
            _ => {
                self.step = Step::Done;
                Vec::new()
            }
        }
    }

    fn stock(&mut self, world: &World, me: AccountId) -> Vec<Command> {
        let Some((business, shop)) = world.businesses().owned_by(me).next() else {
            return Vec::new(); // the build applies at the next tick
        };
        self.step = Step::Done;
        shop.prices
            .keys()
            .map(|good| Command::SetOrder {
                business,
                good: good.clone(),
                quantity: shop.capacity_per_good,
            })
            .collect()
    }
}

impl Brain for Scripted {
    fn decide(&mut self, world: &World, me: PlayerId, _rng: &mut SimRng) -> Vec<Command> {
        let account = AccountId::Player(me);
        match self.step {
            Step::Bid => self.bid(world, account),
            Step::Build => self.build(world, me),
            Step::Stock => self.stock(world, account),
            Step::Done => Vec::new(),
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
