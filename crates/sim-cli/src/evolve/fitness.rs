//! Fitness (docs/evolve.md "Fitness"): how well a candidate played its evaluation games.
//!
//! The ranking path is integer arithmetic only. A candidate's counts (games, wins,
//! bankruptcies) and its median margin over idle (pence) are combined with the definition's
//! weights, which are converted to whole parts per million when the definition loads, into a
//! fixed-point fitness in units of 10⁻¹² ([`FITNESS_ONE`]), using `i128` intermediates and
//! division rounded toward −∞. So the ranking is identical on every platform and thread count,
//! and a resumed run (which reads the counts back from `generations.csv`) ranks exactly as an
//! uninterrupted one. Floats appear only when a fitness is printed.
//!
//! R78: a personality whose win share is below the low-win floor is scored more on net worth
//! gained relative to idle, so losers can still improve when one personality dominates.

#![allow(
    clippy::cast_precision_loss,
    reason = "game counts are far below 2^52; shares are for display only"
)]

use std::cmp::Ordering;

use crate::stats::distribution;
use crate::study::{Entry, GameOutcome, Roster};

/// Fixed-point fitness: this many units make a fitness of 1.0 (a resolution of 10⁻¹²).
pub const FITNESS_ONE: i64 = 1_000_000_000_000;

/// Parts per million: the unit of the fitness shares and weights once loaded (0.5 → 500,000).
pub const PPM: i64 = 1_000_000;

/// Fitness units per part per million.
const UNITS_PER_PPM: i128 = 1_000_000; // FITNESS_ONE / PPM

/// The fitness weights in exact integer form (docs/evolve.md "Fitness"), from
/// [`crate::evolve::def::FitnessDef::weights`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitnessWeights {
    /// Bankruptcy penalty per unit of bankruptcy rate, ppm (0.5 → 500,000).
    pub bankruptcy_penalty: i64,
    /// The low-win floor, a win share in ppm: below it net worth is blended in (0 turns the
    /// blend off).
    pub low_win_floor: i64,
    /// Pence of median margin over idle that count as a full net-worth term (positive).
    pub net_worth_scale: i64,
    /// The net-worth weight at a win share of 0, ppm.
    pub max_net_worth_weight: i64,
}

impl Default for FitnessWeights {
    /// The shipped weights: penalty 0.5, floor 0.10, scale £7,000, weight 0.8.
    fn default() -> Self {
        FitnessWeights {
            bankruptcy_penalty: 500_000,
            low_win_floor: 100_000,
            net_worth_scale: 700_000,
            max_net_worth_weight: 800_000,
        }
    }
}

impl FitnessWeights {
    /// The weights with no blend (floor 0): fitness is `win share − penalty × bankruptcy
    /// rate` for every candidate.
    pub fn without_blend(bankruptcy_penalty_ppm: i64) -> Self {
        FitnessWeights {
            bankruptcy_penalty: bankruptcy_penalty_ppm,
            low_win_floor: 0,
            ..FitnessWeights::default()
        }
    }
}

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
    /// Win share: wins / games (0 with no games). For display.
    pub fn win_share(&self) -> f64 {
        share(self.wins, self.games)
    }

    /// Bankruptcy rate: bankruptcies / games (0 with no games). For display.
    pub fn bankrupt_rate(&self) -> f64 {
        share(self.bankruptcies, self.games)
    }

    /// The fitness as a number, for display: [`Evaluation::score`] / [`FITNESS_ONE`].
    pub fn fitness(&self, w: &FitnessWeights) -> f64 {
        self.score(w) as f64 / FITNESS_ONE as f64
    }

    /// The net-worth weight `ω` in fitness units ([`FITNESS_ONE`] = 1): 0 at or above the
    /// low-win floor `f`; below it `ω = max_net_worth_weight × (f − win share) / f`, rising
    /// linearly to `max_net_worth_weight` at a win share of 0. Rounded toward −∞.
    pub fn net_worth_weight(&self, w: &FitnessWeights) -> i64 {
        // Red: the blend is not implemented yet.
        let _ = w;
        0
    }

    /// The fixed-point fitness ([`FITNESS_ONE`] = 1), the ranking key. With `s` the win share,
    /// `b` the bankruptcy rate, `p` the penalty, `f` the low-win floor and `ω` the
    /// [net-worth weight](Evaluation::net_worth_weight):
    ///
    /// - at or above the floor (`ω = 0`): `s − p·b`, in one division, so two candidates with
    ///   the same number of games compare exactly as their rational fitnesses do (up to 10⁶
    ///   games, where distinct values differ by at least one unit);
    /// - below it: `s − p·b + ω·(f·c − s)`, i.e. `(1 − ω)·s + ω·f·c − p·b`, where
    ///   `c = clamp(nw_vs_idle / net_worth_scale, −1, 1)` (0 with no games). The net-worth
    ///   term is scaled by `f`, so a candidate below the floor never scores above one at the
    ///   floor with the same bankruptcies; fitness is continuous at the floor.
    pub fn score(&self, w: &FitnessWeights) -> i64 {
        if self.games == 0 {
            return 0;
        }
        let n = i128::from(self.games);
        let one = i128::from(FITNESS_ONE);
        let wins = i128::from(self.wins);
        let penalty = i128::from(w.bankruptcy_penalty)
            .saturating_mul(i128::from(self.bankruptcies))
            .saturating_mul(UNITS_PER_PPM);
        // (s − p·b) in fitness units, one division.
        let base = (wins.saturating_mul(one) - penalty).div_euclid(n);
        let omega = i128::from(self.net_worth_weight(w));
        if omega == 0 {
            return narrow(base);
        }
        // (f·c − s) in fitness units: (f_ppm·cn·N − 10⁶·S·W) × 10⁶ / (S·N), cn = clamp(m, −S, S).
        let scale = i128::from(w.net_worth_scale.max(1));
        let cn = i128::from(self.nw_vs_idle.unwrap_or(0)).clamp(-scale, scale);
        let floor = i128::from(w.low_win_floor);
        let pull = (floor.saturating_mul(cn).saturating_mul(n)
            - i128::from(PPM).saturating_mul(scale).saturating_mul(wins))
        .saturating_mul(UNITS_PER_PPM)
        .div_euclid(scale.saturating_mul(n));
        narrow(base + omega.saturating_mul(pull).div_euclid(one))
    }
}

fn narrow(v: i128) -> i64 {
    i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX })
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
    let idle = roster.idle();
    let mut e = Evaluation {
        games: games.len() as u64,
        ..Evaluation::default()
    };
    let mut diffs = Vec::with_capacity(games.len());
    for g in games {
        if g.winner == Some(entry) {
            e.wins += 1;
        }
        let seat_of = |want: usize| g.seating.iter().position(|s| *s == want);
        let Some(me) = seat_of(entry) else { continue };
        if g.bankrupt[me] {
            e.bankruptcies += 1;
        }
        let idle_nw = idle.and_then(seat_of).map_or(0, |s| g.net_worth[s]);
        diffs.push(g.net_worth[me].saturating_sub(idle_nw));
    }
    e.nw_vs_idle = distribution(&diffs).map(|d| d.median);
    e
}

/// Compares two candidates, **better first**: a candidate with no games last; then higher
/// fixed-point fitness ([`Evaluation::score`]); then (the tie-break) higher median net worth
/// relative to idle (`None` last); then the lower candidate index. A total order on integers,
/// so a ranking never depends on input order, thread count or platform.
pub fn compare(a: (u32, &Evaluation), b: (u32, &Evaluation), w: &FitnessWeights) -> Ordering {
    (b.1.games > 0)
        .cmp(&(a.1.games > 0))
        .then_with(|| b.1.score(w).cmp(&a.1.score(w)))
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
pub fn rank(candidates: &[(u32, Evaluation)], w: &FitnessWeights) -> Vec<u32> {
    let mut order: Vec<&(u32, Evaluation)> = candidates.iter().collect();
    order.sort_by(|a, b| compare((a.0, &a.1), (b.0, &b.1), w));
    order.into_iter().map(|(i, _)| *i).collect()
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

    fn old(penalty_ppm: i64) -> FitnessWeights {
        FitnessWeights::without_blend(penalty_ppm)
    }

    const W: FitnessWeights = FitnessWeights {
        bankruptcy_penalty: 500_000,
        low_win_floor: 100_000,
        net_worth_scale: 700_000,
        max_net_worth_weight: 800_000,
    };

    #[test]
    fn win_share_first_then_net_worth_then_index() {
        let c = [
            (0, ev(10, 2, 0, 900)),
            (1, ev(10, 5, 0, 100)),
            (2, ev(10, 2, 0, 1000)),
            (3, ev(10, 2, 0, 1000)),
        ];
        assert_eq!(rank(&c, &old(500_000)), vec![1, 2, 3, 0]);
        assert_eq!(rank(&c, &W), vec![1, 2, 3, 0]);
    }

    #[test]
    fn bankruptcy_is_penalised() {
        let c = [(0, ev(10, 3, 4, 0)), (1, ev(10, 2, 0, 0))];
        assert_eq!(rank(&c, &old(500_000)), vec![1, 0]); // 0.3 − 0.2 = 0.1 < 0.2
        assert_eq!(rank(&c, &old(0)), vec![0, 1]);
    }

    #[test]
    fn no_games_ranks_last_on_the_tie_break() {
        let none = Evaluation::default();
        let c = [(0, none), (1, ev(10, 0, 0, -5))];
        assert_eq!(rank(&c, &old(500_000)), vec![1, 0]);
        assert_eq!(rank(&c, &W), vec![1, 0]);
    }

    #[test]
    fn above_the_floor_the_score_is_the_exact_old_fitness() {
        // 7/24 − 0.5 × 3/24 = 11/48 = 0.2291666…, rounded toward −∞ at 10⁻¹².
        assert_eq!(ev(24, 7, 3, 0).score(&W), 229_166_666_666);
        assert_eq!(ev(24, 7, 3, 0).score(&W), ev(24, 7, 3, -9_999).score(&W));
        // −0.5 × 1/24 = −0.0208333…
        assert_eq!(ev(24, 0, 1, 0).score(&old(500_000)), -20_833_333_334);
        assert_eq!(ev(10, 1, 0, 0).score(&W), FITNESS_ONE / 10); // exactly at the floor
    }

    #[test]
    fn above_the_floor_the_order_matches_the_rational_fitness() {
        // Every (wins, bankruptcies) pair over 24 games at or above the floor: the integer
        // ranking equals the order of the exact rationals 2·wins − bankruptcies (penalty ½).
        let mut c = Vec::new();
        for wins in 3..=24 {
            for bk in 0..=(24 - wins) {
                c.push((u32::try_from(c.len()).unwrap(), ev(24, wins, bk, 0)));
            }
        }
        let mut exact = c.clone();
        exact.sort_by(|a, b| {
            let key = |e: &Evaluation| {
                2 * i64::try_from(e.wins).unwrap() - i64::try_from(e.bankruptcies).unwrap()
            };
            key(&b.1).cmp(&key(&a.1)).then(a.0.cmp(&b.0))
        });
        let want: Vec<u32> = exact.iter().map(|x| x.0).collect();
        assert_eq!(rank(&c, &W), want);
        assert_eq!(rank(&c, &old(500_000)), want);
    }

    #[test]
    fn zero_win_genomes_rank_by_net_worth_gained() {
        let (rich, poor) = (ev(24, 0, 0, 300_000), ev(24, 0, 0, 100_000));
        assert!(rich.score(&W) > poor.score(&W));
        assert!(poor.score(&W) > 0);
        // ω = 0.8, f·c = 0.1 × 3/7: 0.8 × 0.0428571… = 0.0342857…
        assert_eq!(rich.score(&W), 34_285_714_285);
        assert_eq!(rank(&[(0, poor), (1, rich)], &W), vec![1, 0]);
    }

    #[test]
    fn net_worth_beats_a_lucky_win_below_the_floor() {
        // One lucky win in 24 (below 0.10) but far below idle, against no wins and well above.
        let lucky = ev(24, 1, 0, -300_000);
        let builder = ev(24, 0, 0, 400_000);
        assert_eq!(rank(&[(0, lucky), (1, builder)], &W), vec![1, 0]);
        assert_eq!(rank(&[(0, lucky), (1, builder)], &old(500_000)), vec![0, 1]);
    }

    #[test]
    fn the_net_worth_term_is_clamped() {
        let at = ev(24, 0, 0, 700_000);
        assert_eq!(at.score(&W), ev(24, 0, 0, 9_000_000).score(&W));
        assert_eq!(at.score(&W), 80_000_000_000); // 0.8 × 0.1 × 1
        assert_eq!(ev(24, 0, 0, -9_000_000).score(&W), -80_000_000_000);
    }

    #[test]
    fn below_the_floor_never_beats_the_floor() {
        let floor = ev(10, 1, 0, -700_000).score(&W);
        for wins in 0..10 {
            let below = ev(100, wins, 0, 700_000);
            assert!(below.score(&W) <= floor, "{wins}");
        }
        let max = FitnessWeights {
            max_net_worth_weight: PPM,
            ..W
        };
        assert_eq!(
            ev(10, 0, 0, 700_000).score(&max),
            ev(10, 1, 0, 0).score(&max)
        );
    }

    #[test]
    fn the_blend_weight_rises_linearly_as_win_share_falls() {
        let weights: Vec<i64> = (0..=4)
            .map(|w| ev(40, w, 0, 0).net_worth_weight(&W))
            .collect();
        // floor 0.10 of 40 games = 4 wins: 0.8, 0.6, 0.4, 0.2, 0.
        assert_eq!(
            weights,
            vec![
                800_000_000_000,
                600_000_000_000,
                400_000_000_000,
                200_000_000_000,
                0
            ]
        );
        assert_eq!(ev(40, 30, 0, 0).net_worth_weight(&W), 0);
        assert_eq!(ev(40, 0, 0, 0).net_worth_weight(&old(500_000)), 0);
        assert_eq!(Evaluation::default().net_worth_weight(&W), 0);
    }

    #[test]
    fn bankruptcy_is_still_penalised_below_the_floor() {
        let solvent = ev(24, 0, 0, 200_000);
        let bust = ev(24, 0, 2, 200_000);
        assert_eq!(
            solvent.score(&W) - bust.score(&W),
            41_666_666_667 // 0.5 × 2/24, up to rounding
        );
        assert_eq!(rank(&[(0, bust), (1, solvent)], &W), vec![1, 0]);
    }

    #[test]
    fn extreme_counts_do_not_overflow() {
        let w = FitnessWeights {
            bankruptcy_penalty: 1_000 * PPM,
            low_win_floor: PPM,
            net_worth_scale: i64::MAX,
            max_net_worth_weight: PPM,
        };
        let e = Evaluation {
            games: 1_000_000,
            wins: 0,
            bankruptcies: 1_000_000,
            nw_vs_idle: Some(i64::MIN),
        };
        assert!(e.score(&w) < 0);
        assert!(ev(1_000_000, 999_999, 0, i64::MAX).score(&w) > 0);
    }
}
