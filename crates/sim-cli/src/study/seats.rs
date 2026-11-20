//! The roster and the seat rotation (R65: "seats rotated").

use sim_core::Config;

use super::def::{StudyDef, StudyError};

/// One roster entry: an AI personality or the idle control seat.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Entry {
    /// A personality id from `ai.yaml`.
    Personality(String),
    /// The idle control seat.
    Idle,
}

impl Entry {
    /// `idle` or a personality id.
    pub fn from_name(name: &str) -> Self {
        let _ = name;
        todo!("from_name")
    }
}

/// The seats of every game, in roster order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster(Vec<Entry>);

impl Roster {
    /// A roster of these entries.
    pub fn new(entries: Vec<Entry>) -> Self {
        Roster(entries)
    }

    /// Seats.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Entry names.
    pub fn names(&self) -> Vec<String> {
        todo!("names")
    }
}

/// The definition's roster, checked against the config.
pub fn resolve_roster(def: &StudyDef, config: &Config) -> Result<Roster, StudyError> {
    let _ = (def, config);
    todo!("resolve_roster")
}

/// One planned game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePlan {
    /// Index within the model.
    pub index: u32,
    /// Game seed.
    pub seed: u64,
    /// Rotation.
    pub rotation: u32,
    /// `seating[seat]` is the roster index playing that seat.
    pub seating: Vec<usize>,
}

/// The games of one model.
pub fn plan(games: u32, seats: usize, base_seed: u64) -> Vec<GamePlan> {
    let _ = (games, seats, base_seed);
    todo!("plan")
}
