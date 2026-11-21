//! One model's fun metrics (R79), from its games in plan order (seed by seed, each seed's
//! rotations in turn). Definitions are in docs/study.md "Fun (R79)"; all integer.
//!
//! - **Strongest AI**: the AI personality with the most outright wins in the model (the lower
//!   roster index on a tie); none if no AI ever wins.
//! - **Upset rate**: games whose outright winner is not the strongest AI (a tie, nobody solvent
//!   or the idle seat winning included) / games.
//! - **Mean lead changes**: total lead changes / games.
//! - **Comeback rate**: comebacks / decided games (an outright winner and a halfway midnight).
//! - **Median margin**: the lower median (nearest rank) of the defined winning margins.
//! - **Distinctness**: each AI personality's holdings are pooled over all its games into one
//!   mix (the share of each category); a personality that never held anything has a mix of its
//!   own ("nothing"). The distance between two mixes is the total variation distance,
//!   `½ Σᵢ |aᵢ − bᵢ|`, in `[0, 1]`: 0 for the same mix, 1 for mixes with no category in
//!   common. Distinctness is the mean over every pair of personalities, so it is 0 when every
//!   personality owns the same mix and 1 when no two share a category; 1 with fewer than two
//!   personalities (nothing to tell apart).
//! - **Streak**: the winner sequence in plan order; `A` counts adjacent games won by the same
//!   AI personality (a tie, nobody or the idle seat never matches). Under the hypothesis that
//!   the order is random (every arrangement of the same winners equally likely) its mean and
//!   variance are exact functions of the win counts `nₖ` and games `N` (below), and the streak
//!   statistic is the z-score `(A − E[A]) / √Var[A]`: about 0 for a random-looking sequence,
//!   large when one AI wins runs of games more often than chance predicts. The longest run of
//!   one AI is reported alongside.
//!
//! With `(x)ᵣ` the falling factorial, `S₂ = Σ (nₖ)₂`, `S₃ = Σ (nₖ)₃`, `S₄ = Σ (nₖ)₄`,
//! `S₂₂ = S₂² − Σ (nₖ)₂²` and `D = (N − 2)(N − 3) / 2` (pairs of adjacent positions that share
//! no game):
//!
//! `E[A] = (N − 1) S₂ / (N)₂`,
//! `E[A²] = E[A] + 2 (N − 2) S₃ / (N)₃ + 2 D (S₄ + S₂₂) / (N)₄`,
//! `Var[A] = E[A²] − E[A]²`.
//!
//! These are evaluated in fixed point (10⁻¹² of a game) with integer arithmetic and an integer
//! square root, so the z-score is the same on every machine.

use super::game::FunGame;
use super::score::Playstyle;
use super::{PPM, div_round, ratio_ppm};
use crate::study::metrics::GameOutcome;
use crate::study::seats::{Entry, Roster};

/// One model's fun metrics. Rates, means, margins, distances and z-scores are in parts per
/// million.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunMetrics {
    /// Roster index of the strongest AI.
    pub strongest: Option<usize>,
    /// Upset rate.
    pub upset_ppm: i64,
    /// Mean lead changes per game.
    pub lead_changes_ppm: i64,
    /// Decided games (an outright winner and a halfway midnight).
    pub decided: u64,
    /// Comeback rate among decided games.
    pub comeback_ppm: Option<i64>,
    /// Median winning margin.
    pub margin_median_ppm: Option<i64>,
    /// Distinctness on the full holdings mix; `None` if some game's holdings are not fully
    /// known (a recording).
    pub distinctness_ppm: Option<i64>,
    /// Distinctness on what a checkpoint holds (leases, businesses).
    pub distinctness_checkpoint_ppm: Option<i64>,
    /// The streak z-score; `None` if it would overflow.
    pub streak_z_ppm: Option<i64>,
    /// The longest run of games won by one AI.
    pub streak_longest: u32,
}

/// The upset rate: games whose winner is not the strongest AI / games. Also returns the
/// strongest AI (most wins among `ai`, the lower index on a tie; none if no AI won).
pub fn upset_rate_ppm(winners: &[Option<usize>], ai: &[bool]) -> (i64, Option<usize>) {
    let mut wins = vec![0u64; ai.len()];
    for w in winners.iter().flatten() {
        if let Some(c) = wins.get_mut(*w) {
            *c += 1;
        }
    }
    let strongest = (0..ai.len())
        .filter(|i| ai[*i] && wins[*i] > 0)
        .max_by(|a, b| wins[*a].cmp(&wins[*b]).then(b.cmp(a)));
    let games = winners.len() as u64;
    let upsets = winners
        .iter()
        .filter(|w| strongest.is_none() || **w != strongest)
        .count() as u64;
    (ratio_ppm(upsets, games), strongest)
}

/// Mean pairwise total variation distance between the mixes (counts per category), in parts
/// per million (see the module docs). [`PPM`] with fewer than two mixes.
pub fn distinctness_ppm(mixes: &[Vec<u64>]) -> i64 {
    if mixes.len() < 2 {
        return PPM;
    }
    // A mix with nothing in it is its own category, "nothing".
    let shares: Vec<(Vec<i128>, i128)> = mixes
        .iter()
        .map(|m| {
            let total: u64 = m.iter().sum();
            let mut v: Vec<i128> = m.iter().map(|c| i128::from(*c)).collect();
            v.push(i128::from(total == 0));
            (v, i128::from(total.max(1)))
        })
        .collect();
    let mut sum = 0i128;
    let mut pairs = 0i128;
    for (i, (a, ta)) in shares.iter().enumerate() {
        for (b, tb) in &shares[i + 1..] {
            let len = a.len().max(b.len());
            let at = |v: &Vec<i128>, k: usize| v.get(k).copied().unwrap_or(0);
            let l1: i128 = (0..len)
                .map(|k| (at(a, k) * tb - at(b, k) * ta).abs())
                .sum();
            sum += div_round(l1 * i128::from(PPM), 2 * ta * tb);
            pairs += 1;
        }
    }
    i64::try_from(div_round(sum, pairs)).unwrap_or(PPM)
}

/// The streak statistic of a winner sequence (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Streak {
    /// Adjacent games won by the same AI.
    pub repeats: u64,
    /// The z-score of `repeats` under a random order, parts per million; `None` on overflow.
    pub z_ppm: Option<i64>,
    /// The longest run of games won by one AI.
    pub longest: u32,
}

/// Fixed-point scale of the streak moments: 10⁻¹² of a game.
const SCALE: i128 = 1_000_000_000_000;

fn falling(n: i128, r: i128) -> i128 {
    (0..r).map(|i| (n - i).max(0)).product()
}

/// `S` (a sum of falling factorials) over `(N)ᵣ`, scaled; 0 when `(N)ᵣ` is 0.
fn prob(s: i128, n: i128, r: i128) -> Option<i128> {
    let d = falling(n, r);
    if d == 0 {
        return Some(0);
    }
    Some(div_round(s.checked_mul(SCALE)?, d))
}

fn moments(counts: &[u64], n: u64) -> Option<(i128, i128)> {
    let n = i128::from(n);
    let f = |r: i128| -> Option<i128> {
        counts
            .iter()
            .try_fold(0i128, |acc, c| acc.checked_add(falling(i128::from(*c), r)))
    };
    let (s2, s3, s4) = (f(2)?, f(3)?, f(4)?);
    let sq: i128 = counts.iter().try_fold(0i128, |acc, c| {
        acc.checked_add(falling(i128::from(*c), 2).checked_pow(2)?)
    })?;
    let s22 = s2.checked_pow(2)?.checked_sub(sq)?;
    let mean = (n - 1).max(0).checked_mul(prob(s2, n, 2)?)?;
    let adjacent = 2 * (n - 2).max(0) * prob(s3, n, 3)?;
    let disjoint_pairs = (n - 2).max(0) * (n - 3).max(0) / 2;
    let disjoint = (2 * disjoint_pairs).checked_mul(prob(s4.checked_add(s22)?, n, 4)?)?;
    let second = mean.checked_add(adjacent)?.checked_add(disjoint)?;
    let var = second.checked_sub(div_round(mean.checked_mul(mean)?, SCALE))?;
    Some((mean, var))
}

/// The streak statistic of `winners` (roster index of each game's outright winner, in plan
/// order); `ai[i]` says whether roster entry `i` is an AI personality.
pub fn streak(winners: &[Option<usize>], ai: &[bool]) -> Streak {
    let is_ai = |w: &Option<usize>| w.is_some_and(|i| ai.get(i).copied().unwrap_or(false));
    let mut repeats = 0u64;
    let (mut longest, mut run) = (0u32, 0u32);
    for (i, w) in winners.iter().enumerate() {
        if !is_ai(w) {
            run = 0;
            continue;
        }
        if i > 0 && winners[i - 1] == *w {
            repeats += 1;
            run += 1;
        } else {
            run = 1;
        }
        longest = longest.max(run);
    }
    let mut counts = vec![0u64; ai.len()];
    for w in winners.iter().filter(|w| is_ai(w)).flatten() {
        counts[*w] += 1;
    }
    let z_ppm = moments(&counts, winners.len() as u64).and_then(|(mean, var)| {
        if var <= 0 {
            return Some(0);
        }
        let sd = u128::try_from(var.checked_mul(SCALE)?).ok()?.isqrt();
        let dev = i128::from(repeats).checked_mul(SCALE)?.checked_sub(mean)?;
        i64::try_from(div_round(
            dev.checked_mul(i128::from(PPM))?,
            i128::try_from(sd).ok()?,
        ))
        .ok()
    });
    Streak {
        repeats,
        z_ppm,
        longest,
    }
}

/// One model's fun metrics from its games, in plan order.
pub fn fun_metrics(roster: &Roster, games: &[GameOutcome]) -> FunMetrics {
    let ai: Vec<bool> = roster.entries().iter().map(Entry::is_ai).collect();
    let winners: Vec<Option<usize>> = games.iter().map(|g| g.winner).collect();
    let (upset_ppm, strongest) = upset_rate_ppm(&winners, &ai);
    let n = games.len() as u64;
    let fun: Vec<&FunGame> = games.iter().map(|g| &g.fun).collect();
    let lead: u64 = fun.iter().map(|f| u64::from(f.lead_changes)).sum();
    let decided = fun.iter().filter(|f| f.comeback.is_some()).count() as u64;
    let comebacks = fun.iter().filter(|f| f.comeback == Some(true)).count() as u64;
    let mut margins: Vec<i64> = fun.iter().filter_map(|f| f.margin_ppm).collect();
    margins.sort_unstable();
    let pooled = |basis: Playstyle| -> Vec<Vec<u64>> {
        roster
            .entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_ai())
            .map(|(entry, _)| {
                let mut sum: Vec<u64> = Vec::new();
                for g in games {
                    for (seat, e) in g.seating.iter().enumerate() {
                        if *e != entry {
                            continue;
                        }
                        let mix = g.fun.holdings.get(seat).map(|h| h.mix(basis));
                        for (k, c) in mix.unwrap_or_default().into_iter().enumerate() {
                            if sum.len() <= k {
                                sum.resize(k + 1, 0);
                            }
                            sum[k] += c;
                        }
                    }
                }
                sum
            })
            .collect()
    };
    let full = games.iter().all(|g| g.fun.full_holdings);
    let s = streak(&winners, &ai);
    FunMetrics {
        strongest,
        upset_ppm,
        lead_changes_ppm: ratio_ppm(lead, n),
        decided,
        comeback_ppm: (decided > 0).then(|| ratio_ppm(comebacks, decided)),
        margin_median_ppm: margins.get(margins.len().saturating_sub(1) / 2).copied(),
        distinctness_ppm: full.then(|| distinctness_ppm(&pooled(Playstyle::Holdings))),
        distinctness_checkpoint_ppm: Some(distinctness_ppm(&pooled(Playstyle::Checkpoint))),
        streak_z_ppm: s.z_ppm,
        streak_longest: s.longest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(
        clippy::cast_precision_loss,
        reason = "a brute-force float check of small exact numbers"
    )]
    fn the_moments_match_brute_force_on_a_small_sequence() {
        // Winners a a b c (c not an AI): every arrangement of {a, a, b, c} equally likely.
        let seq = [0usize, 0, 1, 2];
        let mut perms = Vec::new();
        let mut idx = [0usize, 1, 2, 3];
        permute(&mut idx, 0, &mut perms);
        let (mut sum, mut sum2, mut k) = (0f64, 0f64, 0f64);
        for p in &perms {
            let a = (0..3)
                .filter(|i| seq[p[*i]] == seq[p[i + 1]] && seq[p[*i]] < 2)
                .count() as f64;
            sum += a;
            sum2 += a * a;
            k += 1.0;
        }
        let (mean, var) = moments(&[2, 1], 4).unwrap();
        let exact_mean = sum / k;
        let exact_var = sum2 / k - exact_mean * exact_mean;
        assert!((mean as f64 / SCALE as f64 - exact_mean).abs() < 1e-9);
        assert!((var as f64 / SCALE as f64 - exact_var).abs() < 1e-9);
    }

    fn permute(v: &mut [usize; 4], k: usize, out: &mut Vec<[usize; 4]>) {
        if k == v.len() {
            out.push(*v);
            return;
        }
        for i in k..v.len() {
            v.swap(k, i);
            permute(v, k + 1, out);
            v.swap(k, i);
        }
    }

    #[test]
    fn the_streak_does_not_overflow_on_a_large_study() {
        let winners: Vec<Option<usize>> = (0..20_004).map(|i| Some(i % 5)).collect();
        let s = streak(&winners, &[true; 5]);
        assert!(s.z_ppm.is_some());
        assert!(s.z_ppm.unwrap() < 0, "alternating winners are anti-streaky");
    }

    #[test]
    fn upsets_count_every_game_the_strongest_did_not_win() {
        let w = [Some(0), Some(0), Some(1), None, Some(2)];
        let (rate, strongest) = upset_rate_ppm(&w, &[true, true, false]);
        assert_eq!(strongest, Some(0));
        assert_eq!(rate, 600_000);
        assert_eq!(
            upset_rate_ppm(&[None, Some(2)], &[true, true, false]),
            (PPM, None)
        );
    }

    #[test]
    fn distinctness_spans_zero_to_one() {
        assert_eq!(distinctness_ppm(&[vec![1, 1], vec![5, 5]]), 0);
        assert_eq!(distinctness_ppm(&[vec![1, 0], vec![0, 1]]), PPM);
        assert_eq!(distinctness_ppm(&[vec![0, 0], vec![0, 3]]), PPM);
        assert_eq!(distinctness_ppm(&[vec![2, 2]]), PPM);
    }
}
