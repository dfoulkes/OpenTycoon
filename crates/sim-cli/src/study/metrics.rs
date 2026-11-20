//! Per-model metrics (R65): win shares with Wilson intervals, evenness, bankruptcy, the idle
//! control seat (below every solvent AI), net-worth spread, the `balanced` flag and the ranking score. Definitions are
//! in docs/study.md; every number is computed from integer counts, so it is identical on any
//! thread count and platform.

#![allow(
    clippy::cast_precision_loss,
    reason = "game and win counts are far below 2^52; shares and rates are for display and ranking"
)]

use std::cmp::Ordering;

use super::def::Criteria;
use super::seats::{Entry, Roster};
use crate::stats::{Distribution, distribution};

/// The 97.5th percentile of the standard normal: the z of a two-sided 95% interval.
pub const Z95: f64 = 1.959_963_984_540_054;

/// Wilson score 95% interval for `successes` of `n` trials, clamped to `[0, 1]`:
/// centre `(p̂ + z²/2n) / (1 + z²/n)`, half-width `z / (1 + z²/n) × √(p̂(1 − p̂)/n + z²/4n²)`.
/// `(0, 1)` when `n` is 0.
pub fn wilson(successes: u64, n: u64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 1.0);
    }
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = Z95 * Z95;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = Z95 / denom * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

/// Normalised Shannon entropy of `counts`: `−Σ pᵢ ln pᵢ / ln k` over `k` categories, 1 when
/// every category has the same count, 0 when one has everything. 0 if every count is 0; 1 for
/// a single category with any count (nothing to be uneven against).
pub fn normalised_entropy(counts: &[u64]) -> f64 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 0.0;
    }
    if counts.len() < 2 {
        return 1.0;
    }
    let t = total as f64;
    let h: f64 = counts
        .iter()
        .filter(|c| **c > 0)
        .map(|c| {
            let p = *c as f64 / t;
            -p * libm::log(p)
        })
        .sum();
    h / libm::log(counts.len() as f64)
}

/// Gini index of `counts`: `Σᵢ Σⱼ |xᵢ − xⱼ| / (2 k² x̄)`. 0 when equal, `(k − 1)/k` when one has
/// everything. 0 if every count is 0 or there are fewer than two.
pub fn gini(counts: &[u64]) -> f64 {
    let k = counts.len();
    let total: u64 = counts.iter().sum();
    if k < 2 || total == 0 {
        return 0.0;
    }
    let diff: u64 = counts
        .iter()
        .flat_map(|a| counts.iter().map(move |b| a.abs_diff(*b)))
        .sum();
    // x̄ = total / k, so 2 k² x̄ = 2 k total.
    diff as f64 / (2.0 * k as f64 * total as f64)
}

/// One game, as the study records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOutcome {
    /// Index within the model.
    pub index: u32,
    /// Seed.
    pub seed: u64,
    /// Rotation.
    pub rotation: u32,
    /// Roster index per seat.
    pub seating: Vec<usize>,
    /// The winning roster entry; `None` for a tie or no solvent player.
    pub winner: Option<usize>,
    /// True if several solvent players shared the top (else `winner` is `None` only when
    /// nobody was solvent).
    pub tie: bool,
    /// Final net worth per seat, pence.
    pub net_worth: Vec<i64>,
    /// Bankrupt per seat.
    pub bankrupt: Vec<bool>,
    /// Midnights passed.
    pub days: u32,
    /// Ticks run.
    pub ticks: u64,
    /// Final state hash.
    pub state_hash: u64,
}

impl GameOutcome {
    /// Finishing position of `seat`, from 1: one more than the number of players strictly
    /// ahead of it. Solvent players are ahead of bankrupt ones; then higher net worth is ahead.
    pub fn rank(&self, seat: usize) -> u32 {
        let key = |s: usize| (!self.bankrupt[s], self.net_worth[s]);
        let me = key(seat);
        let ahead = (0..self.net_worth.len())
            .filter(|s| *s != seat && key(*s).cmp(&me) == Ordering::Greater)
            .count();
        u32::try_from(ahead).unwrap_or(u32::MAX).saturating_add(1)
    }

    /// True if every other player finished strictly ahead of `seat`.
    pub fn strictly_last(&self, seat: usize) -> bool {
        let key = |s: usize| (!self.bankrupt[s], self.net_worth[s]);
        self.net_worth.len() > 1
            && (0..self.net_worth.len()).all(|s| s == seat || key(s) > key(seat))
    }

    /// R65's idle rule: `seat` finished strictly below every AI that is still solvent (a tie
    /// does not count). Bankrupt AIs are ignored; they are covered by the bankruptcy limit.
    /// False if no AI is solvent: the idle seat then beat everyone.
    pub fn idle_below_solvent_ais(&self, roster: &Roster, seat: usize) -> bool {
        let key = |s: usize| (!self.bankrupt[s], self.net_worth[s]);
        let mut solvent_ais = (0..self.net_worth.len())
            .filter(|s| {
                *s != seat
                    && !self.bankrupt[*s]
                    && roster
                        .entries()
                        .get(self.seating[*s])
                        .is_some_and(Entry::is_ai)
            })
            .peekable();
        solvent_ais.peek().is_some() && solvent_ais.all(|s| key(s) > key(seat))
    }
}

/// Raw counts for one model; [`metrics`] turns them into shares and the verdict.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tally {
    /// Games played.
    pub games: u64,
    /// Wins per roster entry.
    pub wins: Vec<u64>,
    /// Games each roster entry ended bankrupt.
    pub bankrupt: Vec<u64>,
    /// Games the idle seat finished strictly last.
    pub idle_last: u64,
    /// Games the idle seat finished below every AI still solvent (R65).
    pub idle_below_solvent: u64,
    /// Sum of the idle seat's finishing positions.
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

    /// Counts one game.
    pub fn add(&mut self, roster: &Roster, game: &GameOutcome) {
        self.games += 1;
        if let Some(w) = game.winner {
            self.wins[w] += 1;
        }
        for (seat, entry) in game.seating.iter().enumerate() {
            if game.bankrupt[seat] {
                self.bankrupt[*entry] += 1;
            }
            self.net_worth[*entry].push(game.net_worth[seat]);
            if roster.entries()[*entry] == Entry::Idle {
                self.idle_rank_sum += u64::from(game.rank(seat));
                if game.strictly_last(seat) {
                    self.idle_last += 1;
                }
                if game.idle_below_solvent_ais(roster, seat) {
                    self.idle_below_solvent += 1;
                }
            }
        }
    }
}

/// One roster entry's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryMetrics {
    /// Personality id or `idle`.
    pub name: String,
    /// An AI personality (not the idle seat).
    pub is_ai: bool,
    /// Games won outright.
    pub wins: u64,
    /// `wins / games`.
    pub win_share: f64,
    /// Wilson 95% interval of the win share.
    pub win_low: f64,
    /// Upper end.
    pub win_high: f64,
    /// Games it ended bankrupt / games.
    pub bankrupt_rate: f64,
    /// Final net worth spread (pence, nearest rank), `None` if no games.
    pub net_worth: Option<Distribution>,
}

/// One model's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetrics {
    /// Games played.
    pub games: u64,
    /// Games with no outright winner (a tie at the top, or nobody solvent).
    pub no_winner: u64,
    /// Per roster entry, in roster order.
    pub entries: Vec<EntryMetrics>,
    /// The largest AI win share.
    pub max_win_share: f64,
    /// The smallest AI win share.
    pub min_win_share: f64,
    /// Normalised entropy of the AI win counts (1 = perfectly even).
    pub evenness_entropy: f64,
    /// Gini index of the AI win counts (0 = perfectly even).
    pub evenness_gini: f64,
    /// Bankrupt player-games / player-games, every seat.
    pub bankruptcy_rate: f64,
    /// Bankrupt AI player-games / AI player-games (the R65 criterion).
    pub ai_bankruptcy_rate: f64,
    /// The idle seat's mean finishing position (1 = first), `None` without an idle seat.
    pub idle_mean_rank: Option<f64>,
    /// Share of games the idle seat finished strictly last, `None` without an idle seat.
    pub idle_last_rate: Option<f64>,
    /// Share of games the idle seat finished below every AI still solvent, `None` without an
    /// idle seat.
    pub idle_below_solvent_rate: Option<f64>,
    /// R65: every AI win share within the bounds, AI bankruptcy below the limit, the idle
    /// seat below every solvent AI often enough. Never true without an idle seat or games.
    pub balanced: bool,
    /// Ranking score: `evenness_entropy` (docs/study.md).
    pub score: f64,
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// Floating-point slack for comparing shares with the criteria (`10/100` is `0.1` exactly, but
/// other counts may round).
const SLACK: f64 = 1e-12;

/// The metrics of a tally.
pub fn metrics(roster: &Roster, tally: &Tally, criteria: &Criteria) -> ModelMetrics {
    let games = tally.games;
    let entries: Vec<EntryMetrics> = roster
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let wins = tally.wins.get(i).copied().unwrap_or(0);
            let (win_low, win_high) = wilson(wins, games);
            EntryMetrics {
                name: e.name().to_owned(),
                is_ai: e.is_ai(),
                wins,
                win_share: share(wins, games),
                win_low,
                win_high,
                bankrupt_rate: share(tally.bankrupt.get(i).copied().unwrap_or(0), games),
                net_worth: tally.net_worth.get(i).and_then(|v| distribution(v)),
            }
        })
        .collect();
    let ai: Vec<&EntryMetrics> = entries.iter().filter(|e| e.is_ai).collect();
    let ai_wins: Vec<u64> = ai.iter().map(|e| e.wins).collect();
    let max_win_share = ai.iter().map(|e| e.win_share).fold(0.0, f64::max);
    let min_win_share = ai
        .iter()
        .map(|e| e.win_share)
        .fold(f64::INFINITY, f64::min)
        .min(max_win_share);
    let seats = roster.len() as u64;
    let ai_seats = ai.len() as u64;
    let all_bankrupt: u64 = tally.bankrupt.iter().sum();
    let ai_bankrupt: u64 = roster
        .entries()
        .iter()
        .zip(&tally.bankrupt)
        .filter(|(e, _)| e.is_ai())
        .map(|(_, b)| *b)
        .sum();
    let has_idle = roster.idle().is_some();
    let idle_mean_rank = (has_idle && games > 0).then(|| share(tally.idle_rank_sum, games));
    let idle_last_rate = (has_idle && games > 0).then(|| share(tally.idle_last, games));
    let idle_below_solvent_rate =
        (has_idle && games > 0).then(|| share(tally.idle_below_solvent, games));
    let ai_bankruptcy_rate = share(ai_bankrupt, games * ai_seats);
    let evenness_entropy = normalised_entropy(&ai_wins);
    let balanced = games > 0
        && !ai.is_empty()
        && ai.iter().all(|e| {
            e.win_share + SLACK >= criteria.min_win_share
                && e.win_share - SLACK <= criteria.max_win_share
        })
        && ai_bankruptcy_rate < criteria.max_bankruptcy_rate
        && idle_below_solvent_rate
            .is_some_and(|r| r + SLACK >= criteria.min_idle_below_solvent_rate);
    ModelMetrics {
        games,
        no_winner: games.saturating_sub(tally.wins.iter().sum()),
        max_win_share,
        min_win_share: if ai.is_empty() { 0.0 } else { min_win_share },
        evenness_entropy,
        evenness_gini: gini(&ai_wins),
        bankruptcy_rate: share(all_bankrupt, games * seats),
        ai_bankruptcy_rate,
        idle_mean_rank,
        idle_last_rate,
        idle_below_solvent_rate,
        balanced,
        score: evenness_entropy,
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn game(net_worth: Vec<i64>, bankrupt: Vec<bool>) -> GameOutcome {
        GameOutcome {
            index: 0,
            seed: 0,
            rotation: 0,
            seating: (0..net_worth.len()).collect(),
            winner: None,
            tie: false,
            net_worth,
            bankrupt,
            days: 0,
            ticks: 0,
            state_hash: 0,
        }
    }

    #[test]
    fn bankrupt_players_rank_below_solvent_ones() {
        let g = game(vec![500, 700, 900], vec![false, false, true]);
        assert_eq!((g.rank(0), g.rank(1), g.rank(2)), (2, 1, 3));
        assert!(g.strictly_last(2));
        assert!(!g.strictly_last(0));
    }

    #[test]
    fn a_tie_at_the_bottom_is_not_strictly_last() {
        let g = game(vec![500, 500, 900], vec![false; 3]);
        assert_eq!((g.rank(0), g.rank(1)), (2, 2));
        assert!(!g.strictly_last(0));
    }

    #[test]
    fn the_idle_rule_ignores_bankrupt_ais_but_needs_a_solvent_one() {
        let roster = Roster::new(vec![
            Entry::from_name("a"),
            Entry::from_name("b"),
            Entry::Idle,
        ]);
        let above_a_bankrupt = game(vec![900, -50, 500], vec![false, true, false]);
        assert!(above_a_bankrupt.idle_below_solvent_ais(&roster, 2));
        assert!(!above_a_bankrupt.strictly_last(2));
        let above_a_solvent = game(vec![900, 400, 500], vec![false; 3]);
        assert!(!above_a_solvent.idle_below_solvent_ais(&roster, 2));
        let all_bankrupt = game(vec![-10, -50, 500], vec![true, true, false]);
        assert!(!all_bankrupt.idle_below_solvent_ais(&roster, 2));
    }

    #[test]
    fn tally_counts_wins_and_the_idle_seat() {
        let roster = Roster::new(vec![Entry::from_name("grocer"), Entry::Idle]);
        let mut t = Tally::new(2);
        let mut g = game(vec![900, 700], vec![false; 2]);
        g.winner = Some(0);
        t.add(&roster, &g);
        g.seating = vec![1, 0]; // idle in seat 0 with 900: idle wins
        g.winner = Some(1);
        t.add(&roster, &g);
        assert_eq!(t.wins, vec![1, 1]);
        assert_eq!(t.idle_last, 1);
        assert_eq!(t.idle_below_solvent, 1);
        assert_eq!(t.idle_rank_sum, 3);
        let m = metrics(&roster, &t, &Criteria::default());
        assert_eq!(m.idle_last_rate, Some(0.5));
        assert_eq!(m.idle_below_solvent_rate, Some(0.5));
        assert!(!m.balanced);
        assert!((m.score - 1.0).abs() < 1e-12, "one AI is trivially even");
    }

    #[test]
    fn no_idle_seat_is_never_balanced() {
        let roster = Roster::new(vec![Entry::from_name("a"), Entry::from_name("b")]);
        let mut t = Tally::new(2);
        t.games = 10;
        t.wins = vec![4, 4];
        let loose = Criteria {
            min_win_share: 0.0,
            max_win_share: 1.0,
            max_bankruptcy_rate: 1.0,
            min_idle_below_solvent_rate: 0.0,
        };
        assert!(!metrics(&roster, &t, &loose).balanced);
    }

    proptest! {
        #[test]
        fn wilson_contains_the_estimate(n in 1u64..10_000, share in 0u32..=100) {
            let x = n * u64::from(share) / 100;
            let (lo, hi) = wilson(x, n);
            let p = x as f64 / n as f64;
            prop_assert!(0.0 <= lo && lo <= p + 1e-12 && p <= hi + 1e-12 && hi <= 1.0);
        }

        #[test]
        fn evenness_is_bounded(counts in prop::collection::vec(0u64..1000, 1..8)) {
            let h = normalised_entropy(&counts);
            let g = gini(&counts);
            prop_assert!((0.0..=1.0 + 1e-12).contains(&h));
            prop_assert!((0.0..1.0).contains(&g));
        }
    }
}
