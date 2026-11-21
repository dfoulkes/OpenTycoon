//! Recording a harness game: a [`Ticker`] that runs each tick through `World::tick_with`, logs
//! every applied command and takes a checkpoint at each midnight. It only reads the world, so a
//! recorded game is the same game as an unrecorded one (same state hash).

use sim_core::World;
use sim_core::clock::ClockEvents;
use sim_core::economy::AccountId;
use sim_core::hash::stable_hash;
use sim_core::world::{Applied, PlayerStatus};

use super::format::{
    CORE_BRAIN, Checkpoint, CommandRecord, Header, Origin, Outcome, Replay, Seat, SeatCheckpoint,
    SeatKind, SeatOutcome, Winner,
};
use crate::brains::BrainKind;
use crate::game::{GameResult, GameSpec, Ticker, play_with, player_net_worth};

/// What every game of a run shares in its header.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecordContext {
    /// The run.
    pub run_id: String,
    /// This build's rules fingerprint.
    pub rules_fingerprint: String,
    /// This build's git commit.
    pub git_commit: String,
    /// Hash of the effective config.
    pub config_hash: String,
    /// The length preset.
    pub preset: String,
}

/// Version of the harness brains' behaviour; bump when `brains.rs` changes what they do.
pub const HARNESS_BRAIN_VERSION: &str = "1";

/// The seats of a world about to be played by the harness: sim-core's AI seats as
/// `core:<personality>` (versioned by a hash of the personality's settings), every other seat
/// as the harness `brain`.
pub fn seats_of(world: &World, brain: BrainKind) -> Vec<Seat> {
    let _ = (world, brain, CORE_BRAIN, stable_hash::<u8>);
    Vec::new()
}

/// Records one game while it plays.
#[derive(Debug)]
pub struct Recorder {
    commands: Vec<CommandRecord>,
    checkpoints: Vec<Checkpoint>,
    tick_buffer: Vec<CommandRecord>,
}

impl Recorder {
    /// An empty recorder.
    pub fn new() -> Self {
        Recorder {
            commands: Vec::new(),
            checkpoints: Vec::new(),
            tick_buffer: Vec::new(),
        }
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Ticker for Recorder {
    fn tick(&mut self, world: &mut World, _external: usize) -> ClockEvents {
        let _ = (
            &self.commands,
            &self.checkpoints,
            &self.tick_buffer,
            Origin::Core,
        );
        world.tick_with(&mut |_| {}, &mut |_: Applied<'_>| {})
    }
}

/// A checkpoint of `world` now.
pub fn checkpoint(world: &World) -> Checkpoint {
    let _ = (
        AccountId::City,
        PlayerStatus::Active,
        SeatCheckpoint::default_for_stub(),
    );
    Checkpoint {
        day: world.clock().day(),
        tick: world.clock().tick(),
        state_hash: 0,
        seats: Vec::new(),
    }
}

impl SeatCheckpoint {
    fn default_for_stub() -> Self {
        SeatCheckpoint {
            net_worth: 0,
            cash: 0,
            leases: 0,
            businesses: 0,
            bankrupt: false,
        }
    }
}

/// Ranks: solvent seats by net worth, best first, ties sharing a rank; then bankrupt seats.
pub fn ranks(seats: &[(i64, bool)]) -> Vec<u8> {
    vec![0; seats.len()]
}

/// Plays `world` to the whistle like [`crate::game::play_from`], recording it. `seats`
/// describes each seat (see [`seats_of`]).
pub fn play_recorded(
    world: World,
    spec: &GameSpec,
    ctx: &RecordContext,
    game_id: &str,
    seats: Vec<Seat>,
) -> (GameResult, Replay) {
    let mut rec = Recorder::new();
    let (result, end) = play_with(world, spec, &mut rec);
    let _ = (player_net_worth, SeatKind::Ai, SeatOutcome::stub);
    let replay = Replay {
        header: Header {
            game_id: game_id.to_owned(),
            run_id: ctx.run_id.clone(),
            git_commit: ctx.git_commit.clone(),
            rules_fingerprint: ctx.rules_fingerprint.clone(),
            config_hash: ctx.config_hash.clone(),
            seed: end.seed(),
            preset: ctx.preset.clone(),
            start_year: 0,
            end_year: 0,
            seats,
            ai_order: Vec::new(),
        },
        commands: rec.commands,
        checkpoints: rec.checkpoints,
        outcome: Outcome {
            winner: Winner::Nobody,
            seats: Vec::new(),
            days_played: 0,
            ticks: 0,
            final_state_hash: 0,
        },
    };
    (result, replay)
}

impl SeatOutcome {
    fn stub() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::shipped_config;
    use proptest::prelude::*;
    use sim_core::World;

    fn v1() -> sim_core::Config {
        let mut c = shipped_config();
        c.game.clock.end_year = 1950;
        c
    }

    fn record(seed: u64, brain: BrainKind) -> (GameResult, Replay) {
        let config = v1();
        let world = World::new(config, seed, 2);
        let seats = seats_of(&world, brain);
        let spec = GameSpec {
            brain,
            ..GameSpec::default()
        };
        play_recorded(world, &spec, &RecordContext::default(), "g", seats)
    }

    #[test]
    fn ranks_put_solvent_seats_first_and_share_ties() {
        assert_eq!(ranks(&[(5, false), (9, false), (9, false)]), vec![3, 1, 1]);
        assert_eq!(ranks(&[(100, true), (1, false)]), vec![2, 1]);
        assert_eq!(ranks(&[(1, true), (2, true)]), vec![2, 1]);
        assert_eq!(ranks(&[]), Vec::<u8>::new());
    }

    #[test]
    fn seats_name_the_core_ai_and_the_harness_brain() {
        let world = World::new(v1(), 1, 2);
        let seats = seats_of(&world, BrainKind::Scripted);
        assert_eq!(seats.len(), 2);
        assert_eq!(seats[0].brain, "harness:scripted");
        assert_eq!(seats[0].brain_version, HARNESS_BRAIN_VERSION);
        let ai = &world.ai_seats()[0];
        let seat = &seats[usize::from(ai.player.0)];
        assert_eq!(seat.core_personality(), Some(ai.personality.as_str()));
        assert_eq!(seat.brain_version.len(), 16);
        assert!(
            seats
                .iter()
                .all(|s| s.kind == SeatKind::Ai && s.player_id.is_none())
        );
    }

    #[test]
    fn a_recorded_game_has_header_log_checkpoints_and_outcome() {
        let (result, replay) = record(3, BrainKind::Scripted);
        assert_eq!(replay.header.seed, 3);
        assert_eq!(
            (replay.header.start_year, replay.header.end_year),
            (1930, 1950)
        );
        assert_eq!(replay.header.seats.len(), 2);
        assert!(!replay.header.ai_order.is_empty());
        assert_eq!(replay.checkpoints.len(), result.days_played as usize);
        assert!(replay.commands.iter().any(|c| c.origin == Origin::External));
        assert!(replay.commands.iter().any(|c| c.origin == Origin::Core));
        assert_eq!(replay.outcome.final_state_hash, result.state_hash);
        assert_eq!(replay.outcome.ticks, result.ticks);
        assert_eq!(replay.outcome.days_played, result.days_played);
        let last = replay.checkpoints.last().unwrap();
        assert_eq!(
            last.state_hash, result.state_hash,
            "the whistle is a midnight"
        );
        for (seat, out) in replay.outcome.seats.iter().enumerate() {
            assert_eq!(out.net_worth, result.players[seat].net_worth.pence());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4))]

        /// Checkpoints are monotonic in time: one per midnight, days 1, 2, 3, … at strictly
        /// increasing ticks a whole day apart; the command log's ticks never go backwards.
        #[test]
        fn checkpoints_are_monotonic_in_time(seed in 0u64..1_000, scripted in any::<bool>()) {
            let brain = if scripted { BrainKind::Scripted } else { BrainKind::Idle };
            let (_, replay) = record(seed, brain);
            let per_day = sim_core::clock::ClockRules::from_config(&v1().game.clock).ticks_per_day();
            prop_assert!(!replay.checkpoints.is_empty());
            for (i, cp) in replay.checkpoints.iter().enumerate() {
                prop_assert_eq!(cp.day as usize, i + 1);
                prop_assert_eq!(cp.tick, u64::from(cp.day) * u64::from(per_day));
            }
            prop_assert!(replay.checkpoints.windows(2).all(|w| w[0].tick < w[1].tick));
            prop_assert!(replay.commands.windows(2).all(|w| w[0].tick <= w[1].tick));
        }
    }
}
