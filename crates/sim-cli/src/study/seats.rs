//! The roster and the seat rotation (R65: "seats rotated").
//!
//! A roster of `n` entries fills `n` seats. Games are planned in blocks of `n`: block `b` uses
//! seed `base_seed + b` and plays it once under each rotation `r = 0..n`, where seat `s` holds
//! roster entry `(s + r) mod n`. So every entry sits in every seat equally often, and every
//! seed is played by every seating: seat order and seed luck cannot bias the win shares. Games
//! per model round up to a whole number of blocks. Every model uses the same seeds (common
//! random numbers), so models differ only by their parameters.

use std::fmt;

use sim_core::Config;
use sim_core::economy::PlayerId;

use super::def::{IDLE, StudyDef, StudyError};

/// One roster entry: an AI personality or the idle control seat.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Entry {
    /// A personality id from `ai.yaml`, played by sim-core's AI.
    Personality(String),
    /// The idle control seat: a harness brain that never acts.
    Idle,
}

impl Entry {
    /// `idle` is the control seat; anything else is a personality id.
    pub fn from_name(name: &str) -> Self {
        if name == IDLE {
            Entry::Idle
        } else {
            Entry::Personality(name.to_owned())
        }
    }

    /// Its name in the outputs.
    pub fn name(&self) -> &str {
        match self {
            Entry::Personality(p) => p,
            Entry::Idle => IDLE,
        }
    }

    /// True for an AI personality.
    pub fn is_ai(&self) -> bool {
        matches!(self, Entry::Personality(_))
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The entries that fill the seats, in roster order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster(Vec<Entry>);

impl Roster {
    /// A roster of these entries.
    pub fn new(entries: Vec<Entry>) -> Self {
        Roster(entries)
    }

    /// Seats per game.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries.
    pub fn entries(&self) -> &[Entry] {
        &self.0
    }

    /// Entry names, in roster order.
    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(|e| e.name().to_owned()).collect()
    }

    /// Roster index of the idle seat, if any.
    pub fn idle(&self) -> Option<usize> {
        self.0.iter().position(|e| *e == Entry::Idle)
    }

    /// The AI seats of one game, highest seat first (the order `World::new` runs its
    /// opponents in), for `World::seated`.
    pub fn ai_seats(&self, seating: &[usize]) -> Vec<(PlayerId, String)> {
        let mut seats: Vec<(PlayerId, String)> = seating
            .iter()
            .enumerate()
            .filter_map(|(seat, entry)| match &self.0[*entry] {
                Entry::Personality(p) => u8::try_from(seat).ok().map(|s| (PlayerId(s), p.clone())),
                Entry::Idle => None,
            })
            .collect();
        seats.sort_by_key(|s| std::cmp::Reverse(s.0));
        seats
    }
}

/// The definition's roster, checked against the config: every personality exists in `ai.yaml`,
/// no entry is repeated, at most one idle seat, and the seat count is within `game.players`.
/// Default: every personality in `ai.yaml` (id order), then one `idle` seat.
pub fn resolve_roster(def: &StudyDef, config: &Config) -> Result<Roster, StudyError> {
    let names: Vec<String> = match &def.roster {
        Some(names) => names.clone(),
        None => config
            .ai
            .personalities
            .keys()
            .cloned()
            .chain([IDLE.to_owned()])
            .collect(),
    };
    for (i, n) in names.iter().enumerate() {
        if n != IDLE && !config.ai.personalities.contains_key(n) {
            return Err(StudyError::def(
                format!("roster[{i}]"),
                format!(
                    "{n:?} is not a personality in ai.yaml (known: {}) or `idle`",
                    config
                        .ai
                        .personalities
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        if names[..i].contains(n) {
            return Err(StudyError::def(
                format!("roster[{i}]"),
                format!("{n:?} is listed twice"),
            ));
        }
    }
    let (min, max) = (config.game.players.min, config.game.players.max);
    let seats = u32::try_from(names.len()).unwrap_or(u32::MAX);
    if seats < min || seats > max {
        return Err(StudyError::def(
            "roster",
            format!("{seats} seats; game.players allows {min} to {max}"),
        ));
    }
    if !names.iter().any(|n| n != IDLE) {
        return Err(StudyError::def(
            "roster",
            "needs at least one AI personality",
        ));
    }
    Ok(Roster(names.iter().map(|n| Entry::from_name(n)).collect()))
}

/// `games` rounded up to a whole number of rotations of `seats`.
pub fn effective_games(games: u32, seats: usize) -> u32 {
    let n = u32::try_from(seats.max(1)).unwrap_or(u32::MAX);
    games.div_ceil(n).saturating_mul(n)
}

/// One planned game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePlan {
    /// Index within the model, from 0.
    pub index: u32,
    /// Game seed: `base_seed + block`.
    pub seed: u64,
    /// Rotation `r`: seat `s` holds roster entry `(s + r) mod seats`.
    pub rotation: u32,
    /// `seating[seat]` is the roster index playing that seat.
    pub seating: Vec<usize>,
}

/// The seating of one rotation.
pub fn seating(seats: usize, rotation: usize) -> Vec<usize> {
    (0..seats).map(|s| (s + rotation) % seats).collect()
}

/// The games of one model: `games` rounded up to whole blocks of `seats`, block by block.
pub fn plan(games: u32, seats: usize, base_seed: u64) -> Vec<GamePlan> {
    let total = effective_games(games, seats);
    let n = u32::try_from(seats.max(1)).unwrap_or(u32::MAX);
    (0..total)
        .map(|index| {
            let rotation = index % n;
            GamePlan {
                index,
                seed: base_seed.wrapping_add(u64::from(index / n)),
                rotation,
                seating: seating(seats, rotation as usize),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_up_to_whole_rotations() {
        assert_eq!(effective_games(500, 6), 504);
        assert_eq!(effective_games(500, 2), 500);
        assert_eq!(effective_games(1, 3), 3);
    }

    #[test]
    fn ai_seats_skip_idle_and_run_highest_seat_first() {
        let r = Roster::new(vec![
            Entry::from_name("grocer"),
            Entry::Idle,
            Entry::from_name("toff"),
        ]);
        // rotation 1: seat 0 → idle, seat 1 → toff, seat 2 → grocer
        assert_eq!(
            r.ai_seats(&seating(3, 1)),
            vec![
                (PlayerId(2), "grocer".to_owned()),
                (PlayerId(1), "toff".to_owned())
            ]
        );
    }
}
