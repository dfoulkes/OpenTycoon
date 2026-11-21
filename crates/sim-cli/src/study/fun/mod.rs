//! The fun score (R79): a model that passes R65 is ranked by how much fun its games are, not by
//! how efficient its AIs are. Definitions are in docs/study.md "Fun (R79)".
//!
//! - [`game`]: what one game contributes, measured from the midnight boards while it plays
//!   (or from a recording's checkpoints): lead changes, the halfway leader, the comeback, the
//!   winning margin and each seat's end-of-game holdings. It only reads the world, so the game
//!   is the same game (same state hash).
//! - [`model`]: one model's fun metrics: upset rate, mean lead changes, comeback rate, median
//!   margin, playstyle distinctness and the streak statistic.
//! - [`score`]: the target bands, falloffs and weights (the `fun:` block of the study
//!   definition) and the fun score.
//! - [`rescore`]: the fun metrics of a recorded study, from its manifests and replays, without
//!   playing a game (`sim-cli study rescore`).
//!
//! **Exactness.** Everything that feeds the ranking is integer: rates, means, margins,
//! distances, z-scores and scores are fixed point in parts per million ([`PPM`]), computed from
//! counts and pence with exact integer rounding, so every machine ranks identically.

pub mod game;
pub mod model;
pub mod rescore;
pub mod score;

pub use game::{Board, FunGame, FunTicker, Holdings, board, fun_game, holdings_of};
pub use model::{FunMetrics, Streak, distinctness_ppm, fun_metrics, streak, upset_rate_ppm};
pub use rescore::{RescoreOptions, RescoreReport, find_study_run, rescore};
pub use score::{Band, FunDef, Playstyle, fun_score_ppm};

/// One, in fixed point: parts per million.
pub const PPM: i64 = 1_000_000;

/// `n / d` rounded half away from zero; 0 when `d` is 0.
pub fn div_round(n: i128, d: i128) -> i128 {
    if d == 0 {
        return 0;
    }
    let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
    let q = n / d;
    let r = n % d;
    if 2 * r.abs() >= d { q + n.signum() } else { q }
}

/// `part / whole` in parts per million, rounded half away from zero (0 when `whole` is 0).
pub fn ratio_ppm(part: u64, whole: u64) -> i64 {
    i64::try_from(div_round(
        i128::from(part) * i128::from(PPM),
        i128::from(whole),
    ))
    .unwrap_or(i64::MAX)
}

/// A decimal from the definition file in parts per million, rounded to the nearest. The
/// parse and the rounding are IEEE-exact, so every machine gets the same integer.
#[allow(
    clippy::cast_possible_truncation,
    reason = "definition values are small decimals, far inside i64 once scaled"
)]
pub fn to_ppm(v: f64) -> i64 {
    (v * 1_000_000.0).round() as i64
}

/// A fixed-point value as a decimal with six places (`-0.250000`, `2.000000`).
pub fn fmt_ppm(v: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    let a = v.unsigned_abs();
    format!("{sign}{}.{:06}", a / 1_000_000, a % 1_000_000)
}

/// Reads back what [`fmt_ppm`] wrote (or any decimal with at most six places), exactly.
pub fn parse_ppm(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    if int.is_empty() || frac.len() > 6 || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let int: i64 = int.parse().ok()?;
    let frac_val: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<6}").parse().ok()?
    };
    let v = int.checked_mul(PPM)?.checked_add(frac_val)?;
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(div_round(5, 2), 3);
        assert_eq!(div_round(-5, 2), -3);
        assert_eq!(div_round(4, 3), 1);
        assert_eq!(div_round(-4, 3), -1);
        assert_eq!(div_round(1, 0), 0);
        assert_eq!(ratio_ppm(1, 3), 333_333);
        assert_eq!(ratio_ppm(2, 3), 666_667);
    }

    #[test]
    fn fixed_point_text_round_trips() {
        for v in [
            0,
            1,
            -1,
            250_000,
            -250_000,
            2_000_000,
            123_456_789,
            -999_999,
        ] {
            assert_eq!(parse_ppm(&fmt_ppm(v)), Some(v), "{v}");
        }
        assert_eq!(parse_ppm("0.5"), Some(500_000));
        assert_eq!(parse_ppm("2"), Some(2_000_000));
        assert_eq!(parse_ppm(""), None);
        assert_eq!(parse_ppm("x"), None);
        assert_eq!(to_ppm(0.25), 250_000);
        assert_eq!(to_ppm(2.0), 2_000_000);
    }
}
