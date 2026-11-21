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
use crate::game::{self, GameResult, GameSpec, Ticker, play_with, player_net_worth};

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

/// A core personality's brain version: the stable hash (16 hex digits) of everything in
/// `ai.yaml` that decides how it plays, its `personalities.<id>` weights **and** its own
/// `<id>:` block of tunables (aggression, buffers, rent rules, opportunism). Two seats with the
/// same version play by the same settings; `sim-cli evolve` uses it as the genome id, so a
/// replay names the variant that played.
pub fn brain_version(config: &sim_core::Config, personality: &str) -> String {
    let ai = serde_json::to_value(&config.ai).unwrap_or_default();
    let settings = serde_json::json!({
        "weights": ai["personalities"][personality],
        "tunables": ai[personality],
    });
    format!("{:016x}", stable_hash(&settings.to_string()))
}

/// The seats of a world about to be played by the harness: sim-core's AI seats as
/// `core:<personality>` (versioned by a hash of the personality's settings), every other seat
/// as the harness `brain`.
pub fn seats_of(world: &World, brain: BrainKind) -> Vec<Seat> {
    world
        .players()
        .iter()
        .map(
            |(p, _)| match world.ai_seats().iter().find(|s| s.player == *p) {
                Some(ai) => Seat {
                    kind: SeatKind::Ai,
                    brain: format!("{CORE_BRAIN}{}", ai.personality),
                    brain_version: brain_version(world.config(), &ai.personality),
                    player_id: None,
                },
                None => Seat {
                    kind: SeatKind::Ai,
                    brain: format!("harness:{}", brain.name()),
                    brain_version: HARNESS_BRAIN_VERSION.to_owned(),
                    player_id: None,
                },
            },
        )
        .collect()
}

/// Records one game while it plays.
#[derive(Debug, Default)]
pub struct Recorder {
    commands: Vec<CommandRecord>,
    checkpoints: Vec<Checkpoint>,
}

impl Recorder {
    /// An empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// The commands logged so far.
    pub fn commands(&self) -> &[CommandRecord] {
        &self.commands
    }

    /// The checkpoints taken so far.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }
}

/// Runs one tick through `World::tick_with`, appending what phase 2 applied to `log`. The
/// queue holds sim-core's AI commands (queued by the previous tick's AI phase) followed by the
/// `external` commands submitted since, so the last `external` applied are external.
pub fn tick_logged(
    world: &mut World,
    external: usize,
    log: &mut Vec<CommandRecord>,
) -> ClockEvents {
    let first = log.len();
    let fired = world.tick_with(&mut |_| {}, &mut |a: Applied<'_>| {
        log.push(CommandRecord {
            tick: a.tick,
            seat: a.player.0,
            origin: Origin::Core,
            accepted: a.accepted,
            command: a.command.clone(),
        });
    });
    let applied = log.len() - first;
    for rec in &mut log[first + applied.saturating_sub(external)..] {
        rec.origin = Origin::External;
    }
    fired
}

impl Ticker for Recorder {
    fn tick(&mut self, world: &mut World, external: usize) -> ClockEvents {
        let fired = tick_logged(world, external, &mut self.commands);
        if fired.midnight {
            self.checkpoints.push(checkpoint(world));
        }
        fired
    }
}

/// A checkpoint of `world` now: the state hash and each seat's net worth and holdings.
pub fn checkpoint(world: &World) -> Checkpoint {
    let seats = world
        .players()
        .iter()
        .map(|(p, status)| {
            let me = AccountId::Player(*p);
            SeatCheckpoint {
                net_worth: player_net_worth(world, *p).pence(),
                cash: world.ledger().balance(me).pence(),
                leases: count(
                    world
                        .map()
                        .blocks
                        .iter()
                        .filter(|b| b.lease.as_ref().is_some_and(|l| l.holder == me)),
                ),
                businesses: count(world.businesses().owned_by(me)),
                bankrupt: *status == PlayerStatus::Bankrupt,
            }
        })
        .collect();
    Checkpoint {
        day: world.clock().day(),
        tick: world.clock().tick(),
        state_hash: world.state_hash(),
        seats,
    }
}

fn count<T>(it: impl Iterator<Item = T>) -> u32 {
    u32::try_from(it.count()).unwrap_or(u32::MAX)
}

/// Ranks: solvent seats by net worth, best first, ties sharing a rank; then bankrupt seats,
/// also by net worth.
pub fn ranks(seats: &[(i64, bool)]) -> Vec<u8> {
    seats
        .iter()
        .map(|(worth, bankrupt)| {
            let ahead = seats
                .iter()
                .filter(|(w, b)| {
                    (*b, std::cmp::Reverse(*w)) < (*bankrupt, std::cmp::Reverse(*worth))
                })
                .count();
            u8::try_from(ahead + 1).unwrap_or(u8::MAX)
        })
        .collect()
}

/// The outcome of a finished game.
pub fn outcome(result: &GameResult) -> Outcome {
    let pairs: Vec<(i64, bool)> = result
        .players
        .iter()
        .map(|p| (p.net_worth.pence(), p.bankrupt))
        .collect();
    let seats = pairs
        .iter()
        .zip(ranks(&pairs))
        .map(|((net_worth, bankrupt), rank)| SeatOutcome {
            rank,
            net_worth: *net_worth,
            bankrupt: *bankrupt,
        })
        .collect();
    Outcome {
        winner: match result.winner {
            game::Winner::Player(p) => Winner::Seat(p),
            game::Winner::Tie => Winner::Tie,
            game::Winner::Nobody => Winner::Nobody,
        },
        seats,
        days_played: result.days_played,
        ticks: result.ticks,
        final_state_hash: result.state_hash,
    }
}

/// The header of a game about to start.
pub fn header(world: &World, ctx: &RecordContext, game_id: &str, seats: Vec<Seat>) -> Header {
    let clock = &world.config().game.clock;
    Header {
        game_id: game_id.to_owned(),
        run_id: ctx.run_id.clone(),
        git_commit: ctx.git_commit.clone(),
        rules_fingerprint: ctx.rules_fingerprint.clone(),
        config_hash: ctx.config_hash.clone(),
        seed: world.seed(),
        preset: ctx.preset.clone(),
        start_year: clock.start_year,
        end_year: clock.end_year,
        seats,
        ai_order: world.ai_seats().iter().map(|s| s.player.0).collect(),
    }
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
    let (result, replay, _) = play_recorded_world(world, spec, ctx, game_id, seats);
    (result, replay)
}

/// [`play_recorded`], also returning the finished world (for measures taken at the whistle,
/// such as the study's end-of-game holdings).
pub fn play_recorded_world(
    world: World,
    spec: &GameSpec,
    ctx: &RecordContext,
    game_id: &str,
    seats: Vec<Seat>,
) -> (GameResult, Replay, World) {
    let header = header(&world, ctx, game_id, seats);
    let mut rec = Recorder::new();
    let (result, world) = play_with(world, spec, &mut rec);
    let replay = Replay {
        header,
        commands: rec.commands,
        checkpoints: rec.checkpoints,
        outcome: outcome(&result),
    };
    (result, replay, world)
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
        // Five AI opponents fill seats 5..1; the harness brain plays seat 0.
        let world = World::new(config, seed, 6);
        let seats = seats_of(&world, brain);
        let spec = GameSpec {
            players: 6,
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
        let world = World::new(v1(), 1, 6);
        let seats = seats_of(&world, BrainKind::Scripted);
        assert_eq!(seats.len(), 6);
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
        assert_eq!(replay.header.seats.len(), 6);
        assert_ne!(replay.header.ai_order.len(), 0);
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

    /// Checkpoints are monotonic in time: one per midnight, days 1, 2, 3, … at strictly
    /// increasing ticks a whole day apart; the command log's ticks never go backwards.
    #[test]
    fn checkpoints_are_monotonic_in_time() {
        // Read config/ before the first case: a broken file fails here once, not as a shrunk
        // case.
        let _ = shipped_config();
        proptest!(ProptestConfig::with_cases(4), |(seed in 0u64..1_000, scripted in any::<bool>())| {
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
        });
    }
}
