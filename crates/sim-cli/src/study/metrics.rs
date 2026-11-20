//! Per-model metrics (R65).

use super::def::Criteria;
use super::seats::Roster;

/// Wilson 95% interval for `successes` of `n`.
pub fn wilson(successes: u64, n: u64) -> (f64, f64) {
    let _ = (successes, n);
    todo!("wilson")
}

/// Normalised Shannon entropy of the counts.
pub fn normalised_entropy(counts: &[u64]) -> f64 {
    let _ = counts;
    todo!("entropy")
}

/// Gini index of the counts.
pub fn gini(counts: &[u64]) -> f64 {
    let _ = counts;
    todo!("gini")
}

/// Raw counts for one model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tally {
    /// Games.
    pub games: u64,
    /// Wins per roster entry.
    pub wins: Vec<u64>,
    /// Bankrupt games per roster entry.
    pub bankrupt: Vec<u64>,
    /// Games the idle seat finished strictly last.
    pub idle_last: u64,
    /// Sum of the idle seat's ranks.
    pub idle_rank_sum: u64,
    /// Final net worths per roster entry, pence.
    pub net_worth: Vec<Vec<i64>>,
}

impl Tally {
    /// An empty tally for `entries` roster entries.
    pub fn new(entries: usize) -> Self {
        Tally {
            wins: vec![0; entries],
            bankrupt: vec![0; entries],
            net_worth: vec![Vec::new(); entries],
            ..Tally::default()
        }
    }
}

/// One roster entry's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryMetrics {
    /// Name.
    pub name: String,
}

/// One model's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetrics {
    /// R65 verdict.
    pub balanced: bool,
}

/// The metrics of a tally.
pub fn metrics(roster: &Roster, tally: &Tally, criteria: &Criteria) -> ModelMetrics {
    let _ = (roster, tally, criteria);
    todo!("metrics")
}
