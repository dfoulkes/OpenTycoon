//! Fitness (docs/evolve.md "Fitness"): how well a candidate played its evaluation games.
//!
//! Everything is computed from integer counts and pence, so it is exact and identical on any
//! thread count and platform, and a resumed run (which reads the counts back from
//! `generations.csv`) ranks exactly as an uninterrupted one.

#![allow(
    clippy::cast_precision_loss,
    reason = "game counts are far below 2^52; shares are for ranking and display"
)]

use std::cmp::Ordering;

use crate::stats::distribution;
use crate::study::{Entry, GameOutcome, Roster};

/// A candidate's evaluation, as counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Evaluation {
    /// Games played.
    pub games: u64,
    /// Games its seat won outright (a tie at the top, or nobody solvent, is nobody's win).
    pub wins: u64,
    /// Games its seat ended bankrupt.
    pub bankruptcies: u64,
    /// Median over its games of (its final net worth − the idle seat's), pence (nearest rank,
    /// the lower middle for an even count). Its own median net worth if the roster has no idle
    /// seat; `None` with no games.
    pub nw_vs_idle: Option<i64>,
}

impl Evaluation {
    /// Win share: wins / games (0 with no games).
    pub fn win_share(&self) -> f64 {
        share(self.wins, self.games)
    }

    /// Bankruptcy rate: bankruptcies / games (0 with no games).
    pub fn bankrupt_rate(&self) -> f64 {
        share(self.bankruptcies, self.games)
    }

    /// Fitness: `win share − penalty × bankruptcy rate`.
    pub fn fitness(&self, penalty: f64) -> f64 {
        self.win_share() - penalty * self.bankrupt_rate()
    }
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// The evaluation of roster entry `entry` over `games`.
pub fn evaluate(roster: &Roster, entry: usize, games: &[GameOutcome]) -> Evaluation {
    let _ = (roster, entry, games);
    Evaluation::default() // red: not yet
}

/// Compares two candidates, **better first**: higher fitness, then (the tie-break) higher
/// median net worth relative to idle (`None` last), then the lower candidate index. A total
/// order, so a ranking never depends on input order.
pub fn compare(a: (u32, &Evaluation), b: (u32, &Evaluation), penalty: f64) -> Ordering {
    b.1.fitness(penalty)
        .total_cmp(&a.1.fitness(penalty))
        .then_with(|| match (a.1.nw_vs_idle, b.1.nw_vs_idle) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| a.0.cmp(&b.0))
}

/// Ranks candidates best first: `(index, evaluation)` for every valid one; invalid ones are
/// left out by the caller. Returns the indices in rank order.
pub fn rank(candidates: &[(u32, Evaluation)], penalty: f64) -> Vec<u32> {
    let _ = penalty;
    candidates.iter().map(|(i, _)| *i).collect() // red: not yet
}

/// The roster index of personality `id`.
pub fn entry_of(roster: &Roster, id: &str) -> Option<usize> {
    roster
        .entries()
        .iter()
        .position(|e| *e == Entry::Personality(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(games: u64, wins: u64, bankruptcies: u64, nw: i64) -> Evaluation {
        Evaluation {
            games,
            wins,
            bankruptcies,
            nw_vs_idle: Some(nw),
        }
    }

    #[test]
    fn win_share_first_then_net_worth_then_index() {
        let c = [
            (0, ev(10, 2, 0, 900)),
            (1, ev(10, 5, 0, 100)),
            (2, ev(10, 2, 0, 1000)),
            (3, ev(10, 2, 0, 1000)),
        ];
        assert_eq!(rank(&c, 0.5), vec![1, 2, 3, 0]);
    }

    #[test]
    fn bankruptcy_is_penalised() {
        let c = [(0, ev(10, 3, 4, 0)), (1, ev(10, 2, 0, 0))];
        assert_eq!(rank(&c, 0.5), vec![1, 0]); // 0.3 − 0.2 = 0.1 < 0.2
        assert_eq!(rank(&c, 0.0), vec![0, 1]);
    }

    #[test]
    fn no_games_ranks_last_on_the_tie_break() {
        let none = Evaluation::default();
        let c = [(0, none), (1, ev(10, 0, 0, -5))];
        assert_eq!(rank(&c, 0.5), vec![1, 0]);
    }
}
