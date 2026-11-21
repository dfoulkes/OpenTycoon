//! The fun score's target bands (R79), from the `fun:` block of the study definition, and the
//! score itself.
//!
//! Each metric is scored against its band: 1 inside it, falling off linearly outside it to 0 at
//! `falloff` beyond the edge. The fun score is the weighted mean of the six metric scores. A
//! metric with no value (no decided game, say) scores 0. All in parts per million.

use serde::{Deserialize, Serialize};

use super::{PPM, div_round, to_ppm};
use crate::study::def::StudyError;

/// One metric's target band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    /// Lower edge (inclusive); none for "at most".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Upper edge (inclusive); none for "at least".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// How far outside the band the score falls to 0, in the metric's unit (linear).
    pub falloff: f64,
    /// Weight in the fun score.
    #[serde(default = "one")]
    pub weight: f64,
}

fn one() -> f64 {
    1.0
}

impl Band {
    /// A band from `min` to `max` (either open), falling off over `falloff`, weight 1.
    pub fn new(min: Option<f64>, max: Option<f64>, falloff: f64) -> Self {
        Band {
            min,
            max,
            falloff,
            weight: 1.0,
        }
    }

    /// The score of `value` (parts per million of the metric's unit): [`PPM`] inside the band,
    /// `PPM × (1 − distance / falloff)` outside it, never below 0. No value scores 0.
    pub fn score_ppm(&self, value: Option<i64>) -> i64 {
        // red: not yet implemented
        let _ = value;
        PPM
    }

    fn check(&self, path: &str) -> Result<(), StudyError> {
        for (field, v) in [("min", self.min), ("max", self.max)] {
            if v.is_some_and(|v| !v.is_finite() || v.abs() > 1e9) {
                return Err(StudyError::def(
                    format!("{path}.{field}"),
                    "must be a finite number",
                ));
            }
        }
        if let (Some(lo), Some(hi)) = (self.min, self.max)
            && lo > hi
        {
            return Err(StudyError::def(
                format!("{path}.min"),
                "must not exceed max",
            ));
        }
        if !(self.falloff.is_finite() && self.falloff > 0.0 && self.falloff <= 1e9) {
            return Err(StudyError::def(
                format!("{path}.falloff"),
                "must be above 0",
            ));
        }
        if !(self.weight.is_finite() && (0.0..=1e6).contains(&self.weight)) {
            return Err(StudyError::def(
                format!("{path}.weight"),
                "must be 0 or more",
            ));
        }
        Ok(())
    }
}

/// What a personality's playstyle is measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Playstyle {
    /// The holdings mix at the end of the game, from the world: flats, shops, land, hotels,
    /// other (R79's categories).
    #[default]
    Holdings,
    /// What a recording's checkpoints hold: leases and businesses. The only basis
    /// `study rescore` can measure.
    Checkpoint,
}

/// The `fun:` block of the study definition (R79). Every key is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FunDef {
    /// What playstyle distinctness is measured on.
    pub playstyle: Playstyle,
    /// Upset rate: games not won by the model's strongest AI. R79: 50–75%.
    pub upset: Band,
    /// Mean lead changes per game. R79: at least 2.
    pub lead_changes: Band,
    /// Comeback rate: decided games whose winner was not leading at the halfway midnight.
    /// R79: 20–40%.
    pub comeback: Band,
    /// Median winning margin, `(winner − runner-up) / winner`. R79: under about 25%.
    pub margin: Band,
    /// Playstyle distinctness, 0 (every personality owns the same mix) to 1.
    pub distinctness: Band,
    /// Streak z-score: how much more often than chance one AI wins consecutive games.
    pub streak: Band,
}

impl Default for FunDef {
    fn default() -> Self {
        FunDef {
            playstyle: Playstyle::Holdings,
            upset: Band::new(Some(0.50), Some(0.75), 0.25),
            lead_changes: Band::new(Some(2.0), None, 2.0),
            comeback: Band::new(Some(0.20), Some(0.40), 0.20),
            margin: Band::new(None, Some(0.25), 0.25),
            distinctness: Band::new(Some(0.30), None, 0.30),
            streak: Band::new(None, Some(2.0), 2.0),
        }
    }
}

impl FunDef {
    /// True when it is the default (so a definition without `fun:` serialises as before).
    pub fn is_default(&self) -> bool {
        *self == FunDef::default()
    }

    /// The six bands, by metric name, in column order.
    pub fn bands(&self) -> [(&'static str, &Band); 6] {
        [
            ("upset", &self.upset),
            ("lead_changes", &self.lead_changes),
            ("comeback", &self.comeback),
            ("margin", &self.margin),
            ("distinctness", &self.distinctness),
            ("streak", &self.streak),
        ]
    }

    /// Checks every band; at least one weight must be above 0.
    pub fn validate(&self) -> Result<(), StudyError> {
        for (name, band) in self.bands() {
            band.check(&format!("fun.{name}"))?;
        }
        if self.bands().iter().all(|(_, b)| to_ppm(b.weight) <= 0) {
            return Err(StudyError::def(
                "fun",
                "at least one weight must be above 0",
            ));
        }
        Ok(())
    }
}

/// The fun score: the weighted mean of the metric scores, `Σ wᵢ sᵢ / Σ wᵢ`, in parts per
/// million (weights in parts per million too). 0 if every weight is 0.
pub fn fun_score_ppm(scores: &[(i64, f64)]) -> i64 {
    let (num, den) = scores.iter().fold((0i128, 0i128), |(n, d), (s, w)| {
        let w = i128::from(to_ppm(*w).max(0));
        (n + i128::from(*s) * w, d + w)
    });
    i64::try_from(div_round(num, den)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_scores_one_inside_and_falls_off_linearly_outside() {
        let b = Band::new(Some(0.5), Some(0.75), 0.25);
        assert_eq!(b.score_ppm(Some(500_000)), PPM);
        assert_eq!(b.score_ppm(Some(750_000)), PPM);
        assert_eq!(b.score_ppm(Some(625_000)), PPM);
        assert_eq!(b.score_ppm(Some(375_000)), 500_000);
        assert_eq!(b.score_ppm(Some(875_000)), 500_000);
        assert_eq!(b.score_ppm(Some(250_000)), 0);
        assert_eq!(b.score_ppm(Some(0)), 0);
        assert_eq!(b.score_ppm(None), 0);
        let at_least = Band::new(Some(2.0), None, 2.0);
        assert_eq!(at_least.score_ppm(Some(9_000_000)), PPM);
        assert_eq!(at_least.score_ppm(Some(1_000_000)), 500_000);
    }

    #[test]
    fn the_score_is_the_weighted_mean() {
        assert_eq!(fun_score_ppm(&[(PPM, 1.0), (0, 1.0)]), 500_000);
        assert_eq!(fun_score_ppm(&[(PPM, 3.0), (0, 1.0)]), 750_000);
        assert_eq!(fun_score_ppm(&[(PPM, 0.0)]), 0);
    }

    #[test]
    fn bad_bands_are_refused_naming_the_key() {
        let mut d = FunDef::default();
        d.margin.falloff = 0.0;
        let e = d.validate().unwrap_err().to_string();
        assert!(e.contains("fun.margin.falloff"), "{e}");
        let mut d = FunDef::default();
        d.upset.min = Some(0.9);
        assert!(
            d.validate()
                .unwrap_err()
                .to_string()
                .contains("fun.upset.min")
        );
        let mut d = FunDef::default();
        for b in [
            &mut d.upset,
            &mut d.lead_changes,
            &mut d.comeback,
            &mut d.margin,
            &mut d.distinctness,
            &mut d.streak,
        ] {
            b.weight = 0.0;
        }
        assert!(d.validate().is_err());
        assert!(FunDef::default().validate().is_ok());
    }
}
