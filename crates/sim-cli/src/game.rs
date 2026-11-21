//! One headless game, start to whistle, with harness brains in the free seats.
//!
//! Each game owns its `World` and brains outright and shares only the read-only `Config`, so
//! games are independent of each other and of thread scheduling: the result depends only on
//! `(config, seed, spec)`.

use std::time::Duration;

use serde::Serialize;
use sim_core::ai::Brain;
use sim_core::clock::ClockEvents;
use sim_core::economy::{AccountId, NetWorthInputs, PlayerId, net_worth};
use sim_core::rng::{SimRng, seeded};
use sim_core::world::PlayerStatus;
use sim_core::{Config, Money, World};

use crate::brains::BrainKind;
use crate::timing::Stopwatch;

/// How to play each game of a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameSpec {
    /// Player seats (the last `ai.opponents.len()` are AI opponents).
    pub players: u8,
    /// Brain for every seat the AI opponents do not play.
    pub brain: BrainKind,
    /// Safety stop: never run more ticks than this (the clock normally ends the game).
    pub max_ticks: u64,
}

impl Default for GameSpec {
    fn default() -> Self {
        GameSpec {
            players: 2,
            brain: BrainKind::Idle,
            max_ticks: u64::MAX,
        }
    }
}

/// Who won (spec §1: highest net worth at the whistle among solvent players).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Winner {
    /// One player had the highest net worth.
    Player(u8),
    /// Several solvent players shared the highest net worth.
    Tie,
    /// Nobody was left solvent.
    Nobody,
}

impl std::fmt::Display for Winner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Winner::Player(p) => write!(f, "p{p}"),
            Winner::Tie => f.write_str("tie"),
            Winner::Nobody => f.write_str("none"),
        }
    }
}

/// One player's end state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlayerResult {
    /// Net worth at the end.
    pub net_worth: Money,
    /// Went bankrupt (R4).
    pub bankrupt: bool,
}

/// One game's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameResult {
    /// The seed.
    pub seed: u64,
    /// Winner.
    pub winner: Winner,
    /// Per seat, in player order.
    pub players: Vec<PlayerResult>,
    /// Midnights passed.
    pub days_played: u32,
    /// Ticks run.
    pub ticks: u64,
    /// `World::state_hash()` at the end: equal to `sim-cli run` for the same seed and ticks
    /// when the free seats are idle.
    pub state_hash: u64,
    /// Wall time this game took. The only non-deterministic field.
    #[serde(skip)]
    pub wall: Duration,
}

/// A player's net worth as the harness scores it: the economy's nightly board
/// (`World::net_worth`, R19, R31, R39), scored at each midnight before the write-off, so after
/// the final whistle it is the final score. A bankrupt player keeps the value from their last
/// scored midnight. Falls back to `economy::net_worth` of cash alone only if the player was never
/// scored (the run stopped before the first midnight).
pub fn player_net_worth(world: &World, player: PlayerId) -> Money {
    world.net_worth().get(player).unwrap_or_else(|| {
        let inputs = NetWorthInputs {
            cash: world.ledger().balance(AccountId::Player(player)),
            ..NetWorthInputs::default()
        };
        net_worth(
            &inputs,
            world.config().economy.land.lot_property_uplift.rate(),
        )
    })
}

/// The winner among solvent players.
pub fn winner(players: &[PlayerResult]) -> Winner {
    let solvent = || players.iter().enumerate().filter(|(_, p)| !p.bankrupt);
    let Some(best) = solvent().map(|(_, p)| p.net_worth).max() else {
        return Winner::Nobody;
    };
    let mut leaders = solvent().filter(|(_, p)| p.net_worth == best);
    match (leaders.next(), leaders.next()) {
        (Some((i, _)), None) => u8::try_from(i).map_or(Winner::Tie, Winner::Player),
        _ => Winner::Tie,
    }
}

/// A harness brain's private RNG stream, separate from the world's so it never shifts the
/// world's draws. Derived from the game seed and the seat only.
fn brain_rng(seed: u64, player: PlayerId) -> SimRng {
    seeded(seed ^ 0xB5AD_4ECE_DA1C_E2A9_u64.wrapping_mul(u64::from(player.0) + 1))
}

/// Plays one game to the final whistle (or `max_ticks`).
pub fn play(config: &Config, seed: u64, spec: &GameSpec) -> GameResult {
    play_from(World::new(config.clone(), seed, spec.players), spec)
}

/// Runs each tick of a harness game: plainly ([`Plain`]), or recording it
/// (`replay::record`). A ticker only reads the world around `World::tick`, so it cannot change
/// the game.
pub trait Ticker {
    /// Runs one tick. `external` commands were submitted for it (the last ones in the queue).
    fn tick(&mut self, world: &mut World, external: usize) -> ClockEvents;
}

/// `World::tick`, nothing else.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl Ticker for Plain {
    fn tick(&mut self, world: &mut World, _external: usize) -> ClockEvents {
        world.tick()
    }
}

/// Plays an already-built world (e.g. one set up by a test or tool) to the final whistle
/// (or `max_ticks`). `spec.players` is ignored: the world's seats are used.
pub fn play_from(world: World, spec: &GameSpec) -> GameResult {
    play_with(world, spec, &mut Plain).0
}

/// [`play_from`] through a [`Ticker`]; also returns the finished world.
pub fn play_with(
    mut world: World,
    spec: &GameSpec,
    ticker: &mut impl Ticker,
) -> (GameResult, World) {
    let watch = Stopwatch::start();
    let seed = world.seed();
    let ai_held: Vec<PlayerId> = world.ai_seats().iter().map(|s| s.player).collect();
    let mut seats: Vec<(PlayerId, Box<dyn Brain>, SimRng)> = world
        .players()
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| !ai_held.contains(p))
        .map(|p| (p, spec.brain.build(), brain_rng(seed, p)))
        .collect();
    let mut ticks = 0u64;
    let mut commands = Vec::new();
    while !world.clock().is_over() && ticks < spec.max_ticks {
        for (player, brain, rng) in &mut seats {
            let active = world
                .players()
                .iter()
                .any(|(p, s)| p == player && *s == PlayerStatus::Active);
            if active {
                commands.extend(
                    brain
                        .decide(&world, *player, rng)
                        .into_iter()
                        .map(|c| (*player, c)),
                );
            }
        }
        let external = commands.len();
        for (player, command) in commands.drain(..) {
            world.submit(player, command);
        }
        ticker.tick(&mut world, external);
        ticks += 1;
    }
    let players: Vec<PlayerResult> = world
        .players()
        .iter()
        .map(|(p, status)| PlayerResult {
            net_worth: player_net_worth(&world, *p),
            bankrupt: *status == PlayerStatus::Bankrupt,
        })
        .collect();
    let result = GameResult {
        seed,
        winner: winner(&players),
        players,
        days_played: world.clock().day(),
        ticks,
        state_hash: world.state_hash(),
        wall: watch.elapsed(),
    };
    (result, world)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pounds: i64, bankrupt: bool) -> PlayerResult {
        PlayerResult {
            net_worth: Money::from_pounds(pounds),
            bankrupt,
        }
    }

    #[test]
    fn highest_solvent_net_worth_wins() {
        assert_eq!(winner(&[p(5, false), p(9, false)]), Winner::Player(1));
        assert_eq!(winner(&[p(5, false), p(9, true)]), Winner::Player(0));
        assert_eq!(winner(&[p(9, false), p(9, false)]), Winner::Tie);
        assert_eq!(winner(&[p(9, true), p(9, true)]), Winner::Nobody);
        assert_eq!(winner(&[]), Winner::Nobody);
    }

    #[test]
    fn winner_prints_for_csv() {
        assert_eq!(Winner::Player(3).to_string(), "p3");
        assert_eq!(Winner::Tie.to_string(), "tie");
        assert_eq!(Winner::Nobody.to_string(), "none");
    }

    #[test]
    fn brain_streams_differ_by_seat_and_seed() {
        use rand_core_check::next;
        let a = next(&mut brain_rng(1, PlayerId(0)));
        let b = next(&mut brain_rng(1, PlayerId(1)));
        let c = next(&mut brain_rng(2, PlayerId(0)));
        assert!(a != b && a != c && b != c);
    }

    /// One draw, through sim-core's portable helper (no direct `rand_core` dependency).
    mod rand_core_check {
        pub fn next(rng: &mut sim_core::rng::SimRng) -> u64 {
            sim_core::rng::below(rng, u64::MAX)
        }
    }
}
