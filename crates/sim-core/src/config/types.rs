//! Value types used across config files.
//!
//! YAML is written for humans: money as decimal pounds, ratios as decimals, times as `"HH:MM"`.
//! These types convert to the sim's integer representations once, at load time, and reject any
//! value that would lose precision. Nothing downstream ever sees a float for money.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize, Serializer};

use crate::money::{BASIS_POINTS_PER_UNIT, Money, PENCE_PER_POUND, Rate};

/// Converts a decimal to an integer number of `scale` units, refusing anything that is not an
/// exact multiple of `1/scale` (to within float noise) or does not fit in `i64`.
#[allow(
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "config load is the one place decimals become fixed-point; checked for exactness"
)]
fn decimal_to_fixed(value: f64, scale: i64) -> Result<i64, String> {
    if !value.is_finite() {
        return Err(format!("{value} is not a finite number"));
    }
    let scaled = value * scale as f64;
    let rounded = scaled.round();
    if (scaled - rounded).abs() > 1e-6 {
        return Err(format!(
            "{value} has more precision than 1/{scale} allows; round it"
        ));
    }
    if rounded >= i64::MAX as f64 || rounded <= i64::MIN as f64 {
        return Err(format!("{value} is out of range"));
    }
    Ok(rounded as i64)
}

/// An amount of money written in YAML as decimal pounds (`12.50`), held as [`Money`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Pounds(pub Money);

impl Pounds {
    /// The amount as [`Money`].
    pub const fn money(self) -> Money {
        self.0
    }
}

impl<'de> Deserialize<'de> for Pounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        decimal_to_fixed(value, PENCE_PER_POUND)
            .map(|p| Pounds(Money::from_pence(p)))
            .map_err(|e| de::Error::custom(format!("money (pounds): {e}")))
    }
}

impl Serialize for Pounds {
    #[allow(
        clippy::float_arithmetic,
        clippy::cast_precision_loss,
        reason = "serialises back to the decimal-pounds form it was read from"
    )]
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0.pence() as f64 / PENCE_PER_POUND as f64)
    }
}

impl JsonSchema for Pounds {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> Cow<'static, str> {
        "Pounds".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "number",
            "description": "Money in decimal pounds, at most 2 decimal places (held as integer pence)."
        })
    }
}

/// A decimal ratio or multiplier (`0.5`, `1.8`), held as a fixed-point [`Rate`] in basis points.
/// At most 4 decimal places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ratio(pub Rate);

impl Ratio {
    /// The ratio as a [`Rate`].
    pub const fn rate(self) -> Rate {
        self.0
    }
}

impl<'de> Deserialize<'de> for Ratio {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        decimal_to_fixed(value, BASIS_POINTS_PER_UNIT)
            .map(|bp| Ratio(Rate::from_bp(bp)))
            .map_err(|e| de::Error::custom(format!("ratio: {e}")))
    }
}

impl Serialize for Ratio {
    #[allow(
        clippy::float_arithmetic,
        clippy::cast_precision_loss,
        reason = "serialises back to the decimal form it was read from"
    )]
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.0.bp() as f64 / BASIS_POINTS_PER_UNIT as f64)
    }
}

impl JsonSchema for Ratio {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> Cow<'static, str> {
        "Ratio".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "number",
            "description": "A decimal ratio or multiplier, at most 4 decimal places (held as basis points)."
        })
    }
}

/// A time of day written `"HH:MM"`, held as seconds since midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ClockTime {
    seconds: u32,
}

impl ClockTime {
    /// Builds a time from hours and minutes. Returns `None` if out of range.
    pub const fn from_hm(hour: u32, minute: u32) -> Option<Self> {
        if hour < 24 && minute < 60 {
            Some(ClockTime {
                seconds: hour * 3600 + minute * 60,
            })
        } else {
            None
        }
    }

    /// Seconds since midnight.
    pub const fn seconds_since_midnight(self) -> u32 {
        self.seconds
    }

    /// Parses `"HH:MM"` (24-hour).
    pub fn parse(text: &str) -> Option<Self> {
        let (h, m) = text.split_once(':')?;
        if h.len() != 2 || m.len() != 2 {
            return None;
        }
        Self::from_hm(h.parse().ok()?, m.parse().ok()?)
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}",
            self.seconds / 3600,
            (self.seconds % 3600) / 60
        )
    }
}

impl<'de> Deserialize<'de> for ClockTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = ClockTime;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a time of day as \"HH:MM\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<ClockTime, E> {
                ClockTime::parse(v).ok_or_else(|| E::custom(format!("{v:?} is not \"HH:MM\"")))
            }
        }
        deserializer.deserialize_str(V)
    }
}

impl Serialize for ClockTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl JsonSchema for ClockTime {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> Cow<'static, str> {
        "ClockTime".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$",
            "description": "Time of day, 24-hour \"HH:MM\"."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pounds_convert_exactly() {
        let p: Pounds = serde_json::from_str("12.5").unwrap();
        assert_eq!(p.money(), Money::from_pence(1250));
        let p: Pounds = serde_json::from_str("0.05").unwrap();
        assert_eq!(p.money(), Money::from_pence(5));
    }

    #[test]
    fn pounds_reject_fractional_pence() {
        let err = serde_json::from_str::<Pounds>("0.005").unwrap_err();
        assert!(err.to_string().contains("precision"), "{err}");
    }

    #[test]
    fn ratio_converts_to_basis_points() {
        let r: Ratio = serde_json::from_str("0.6").unwrap();
        assert_eq!(r.rate().bp(), 6_000);
        assert!(serde_json::from_str::<Ratio>("0.00001").is_err());
    }

    #[test]
    fn money_and_ratios_round_trip() {
        for text in ["0.05", "12.5", "10000", "-3.99"] {
            let p: Pounds = serde_json::from_str(text).unwrap();
            let back: Pounds = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
            assert_eq!(p, back);
        }
        let r: Ratio = serde_json::from_str("0.0001").unwrap();
        let back: Ratio = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn clock_time_parses_and_rejects() {
        assert_eq!(
            ClockTime::parse("23:50").unwrap().seconds_since_midnight(),
            85_800
        );
        assert!(ClockTime::parse("24:00").is_none());
        assert!(ClockTime::parse("7:00").is_none());
        assert_eq!(ClockTime::parse("07:05").unwrap().to_string(), "07:05");
    }
}
