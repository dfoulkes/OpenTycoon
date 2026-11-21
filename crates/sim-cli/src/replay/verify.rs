//! `sim-cli replay --verify`: re-simulate a replay and compare every midnight.
//!
//! The replay's world is rebuilt from its config snapshot, seed and seats; the external
//! commands are re-submitted at their ticks; sim-core's AI regenerates its own, and every
//! applied command is compared with the log. At every midnight the state hash and each seat's
//! net worth and holdings must equal the checkpoint. A replay recorded under different rules
//! (another rules fingerprint) is refused before anything runs.

use std::fmt::Write as _;

use sim_core::economy::PlayerId;
use sim_core::{Config, Money, World};

use super::format::{Checkpoint, CommandRecord, Origin, Replay};
use super::record::{checkpoint, tick_logged};

/// Why a replay did not verify.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    /// Recorded under other rules: re-simulating would prove nothing.
    #[error(
        "game {game}: recorded under rules fingerprint {replay} (commit {commit}); this build's \
         rules fingerprint is {build}. Refusing to verify: check out that commit to re-simulate it"
    )]
    RulesMismatch {
        /// The game.
        game: String,
        /// The replay's fingerprint.
        replay: String,
        /// This build's.
        build: String,
        /// The commit that recorded it.
        commit: String,
    },
    /// A midnight differs.
    #[error(
        "game {game}: diverges at midnight {day} (tick {tick}): {what}{}",
        first_command.as_ref().map(|c| format!("; first command difference: {c}")).unwrap_or_default()
    )]
    Diverged {
        /// The game.
        game: String,
        /// The first diverging midnight.
        day: u32,
        /// Its tick.
        tick: u64,
        /// What differs.
        what: String,
        /// The first command that differed before it, if any.
        first_command: Option<String>,
    },
    /// Every midnight matched but something else did not (command log, outcome).
    #[error("game {game}: {what}")]
    Mismatch {
        /// The game.
        game: String,
        /// What differs.
        what: String,
    },
    /// The world could not be rebuilt.
    #[error("game {game}: {what}")]
    Setup {
        /// The game.
        game: String,
        /// What went wrong.
        what: String,
    },
}

/// A verified game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The game.
    pub game_id: String,
    /// Midnights compared.
    pub checkpoints: usize,
    /// Commands compared.
    pub commands: usize,
    /// Ticks re-simulated.
    pub ticks: u64,
}

/// The world a replay starts from.
pub fn start(replay: &Replay, config: &Config) -> Result<World, String> {
    let h = &replay.header;
    let seats = u8::try_from(h.seats.len()).map_err(|_| "too many seats".to_owned())?;
    let mut ai = Vec::with_capacity(h.ai_order.len());
    for seat in &h.ai_order {
        let personality = h
            .seats
            .get(usize::from(*seat))
            .and_then(|s| s.core_personality())
            .ok_or_else(|| format!("AI seat {seat} names no sim-core personality"))?;
        if !config.ai.personalities.contains_key(personality) {
            return Err(format!(
                "personality {personality} is not in the config snapshot"
            ));
        }
        ai.push((PlayerId(*seat), personality.to_owned()));
    }
    let clock = &config.game.clock;
    if (clock.start_year, clock.end_year) != (h.start_year, h.end_year) {
        return Err(format!(
            "the config snapshot runs {}-{}, the replay {}-{}",
            clock.start_year, clock.end_year, h.start_year, h.end_year
        ));
    }
    Ok(World::seated(config.clone(), h.seed, seats, &ai))
}

fn describe(c: &CommandRecord) -> String {
    format!(
        "tick {} seat {} {:?} {:?} ({})",
        c.tick,
        c.seat,
        c.origin,
        c.command,
        if c.accepted { "accepted" } else { "refused" }
    )
}

fn compare_commands(replayed: &[CommandRecord], recorded: &[CommandRecord]) -> Option<String> {
    for (i, r) in recorded.iter().enumerate() {
        match replayed.get(i) {
            Some(x) if x == r => {}
            Some(x) => {
                return Some(format!(
                    "recorded {}, replayed {}",
                    describe(r),
                    describe(x)
                ));
            }
            None => return Some(format!("recorded {}, replayed nothing", describe(r))),
        }
    }
    replayed
        .get(recorded.len())
        .map(|x| format!("replayed {} that was not recorded", describe(x)))
}

fn pounds(pence: i64) -> String {
    Money::from_pence(pence).to_string()
}

fn compare_checkpoint(replayed: &Checkpoint, recorded: &Checkpoint) -> Option<String> {
    if replayed == recorded {
        return None;
    }
    let mut what = if replayed.state_hash == recorded.state_hash {
        "state hash matches".to_owned()
    } else {
        format!(
            "state hash {:016x}, recorded {:016x}",
            replayed.state_hash, recorded.state_hash
        )
    };
    for (seat, (a, b)) in replayed.seats.iter().zip(&recorded.seats).enumerate() {
        if a.net_worth != b.net_worth {
            let _ = write!(
                what,
                "; seat {seat} net worth {}, recorded {}",
                pounds(a.net_worth),
                pounds(b.net_worth)
            );
        } else if a != b {
            let _ = write!(what, "; seat {seat} holdings {a:?}, recorded {b:?}");
        }
    }
    if replayed.seats.len() != recorded.seats.len() || replayed.tick != recorded.tick {
        let _ = write!(
            what,
            "; {} seats at tick {}, recorded {} at tick {}",
            replayed.seats.len(),
            replayed.tick,
            recorded.seats.len(),
            recorded.tick
        );
    }
    Some(what)
}

/// Re-simulates `replay` under `config` (its snapshot) and compares every midnight. Refuses a
/// replay whose rules fingerprint is not `build_fingerprint`.
pub fn verify(
    replay: &Replay,
    config: &Config,
    build_fingerprint: &str,
) -> Result<Verified, VerifyError> {
    let h = &replay.header;
    let game = h.game_id.clone();
    if h.rules_fingerprint != build_fingerprint {
        return Err(VerifyError::RulesMismatch {
            game,
            replay: h.rules_fingerprint.clone(),
            build: build_fingerprint.to_owned(),
            commit: h.git_commit.clone(),
        });
    }
    let mut world = start(replay, config).map_err(|what| VerifyError::Setup {
        game: game.clone(),
        what,
    })?;
    let recorded = &replay.commands;
    let mut at = 0; // next recorded command
    let mut midnights = replay.checkpoints.iter();
    let mut checked = 0;
    let mut first_command: Option<String> = None;
    let mut log = Vec::new();
    let mut ticks = 0u64;
    while !world.clock().is_over() && ticks < replay.outcome.ticks {
        let tick = world.clock().tick() + 1;
        let end = at + recorded[at..].iter().take_while(|c| c.tick == tick).count();
        let this_tick = &recorded[at..end];
        let mut external = 0;
        for c in this_tick.iter().filter(|c| c.origin == Origin::External) {
            world.submit(PlayerId(c.seat), c.command.clone());
            external += 1;
        }
        log.clear();
        let fired = tick_logged(&mut world, external, &mut log);
        if first_command.is_none() {
            first_command = compare_commands(&log, this_tick);
        }
        at = end;
        ticks += 1;
        if fired.midnight {
            let now = checkpoint(&world);
            let diverged = match midnights.next() {
                Some(rec) => compare_checkpoint(&now, rec),
                None => Some("the replay has no checkpoint for it".to_owned()),
            };
            if let Some(what) = diverged {
                return Err(VerifyError::Diverged {
                    game,
                    day: now.day,
                    tick: now.tick,
                    what,
                    first_command,
                });
            }
            checked += 1;
        }
    }
    let mismatch = |what: String| VerifyError::Mismatch {
        game: game.clone(),
        what,
    };
    if let Some(c) = first_command {
        return Err(mismatch(format!("command log differs: {c}")));
    }
    if at != recorded.len() {
        return Err(mismatch(format!(
            "{} recorded commands after the last re-simulated tick",
            recorded.len() - at
        )));
    }
    if let Some(rec) = midnights.next() {
        return Err(mismatch(format!(
            "re-simulation ended before recorded midnight {}",
            rec.day
        )));
    }
    let o = &replay.outcome;
    if (ticks, world.clock().day(), world.state_hash())
        != (o.ticks, o.days_played, o.final_state_hash)
    {
        return Err(mismatch(format!(
            "ends after {ticks} ticks, day {}, hash {:016x}; recorded {} ticks, day {}, hash {:016x}",
            world.clock().day(),
            world.state_hash(),
            o.ticks,
            o.days_played,
            o.final_state_hash
        )));
    }
    Ok(Verified {
        game_id: game,
        checkpoints: checked,
        commands: recorded.len(),
        ticks,
    })
}
