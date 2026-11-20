//! The study definition file (`study/study.yaml`, docs/study.md): which config values to vary
//! and how, how many models and games, the preset, the roster and the R65 criteria.
//!
//! Parsing is strict: unknown keys and wrong types are rejected naming the key path, and
//! [`StudyDef::validate`] checks every range before any game runs.

use serde::{Deserialize, Serialize};

/// Why a study could not be defined or run. Models whose config is invalid are *not* errors:
/// they are reported in `models.csv` (`status = invalid`).
#[derive(Debug, thiserror::Error)]
pub enum StudyError {
    /// The definition file does not parse or breaks a rule; names the key path.
    #[error("study definition: {path}: {reason}")]
    Definition {
        /// Key path inside the definition, e.g. `parameters[2].min`.
        path: String,
        /// What is wrong.
        reason: String,
    },
    /// The base config (shipped + `--set`) does not load: nothing can run.
    #[error("base config: {0}")]
    BaseConfig(String),
    /// The output directory holds a different study.
    #[error(
        "{dir} holds a different study (fingerprint {found}, this one {expected}); \
         use --fresh to start over or choose another --out"
    )]
    Mismatch {
        /// Output directory.
        dir: String,
        /// Fingerprint found there.
        found: String,
        /// This study's fingerprint.
        expected: String,
    },
    /// Reading or writing outputs failed.
    #[error("{path}: {source}")]
    Io {
        /// File or directory.
        path: String,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The thread pool could not be built.
    #[error("thread pool: {0}")]
    Pool(#[from] rayon::ThreadPoolBuildError),
}

impl StudyError {
    pub(crate) fn def(path: impl Into<String>, reason: impl Into<String>) -> Self {
        StudyError::Definition {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        StudyError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

/// How a parameter's unit sample maps onto its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    /// Even over `[min, max]`.
    #[default]
    Uniform,
    /// Even over `[ln min, ln max]`: each decade equally likely. Needs `min > 0`.
    LogUniform,
}

/// Whether a parameter takes whole numbers or decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Whole numbers: every integer in `[min, max]` is equally likely (uniform).
    Integer,
    /// Decimals, rounded to `decimals` places so the config loader accepts them exactly.
    #[default]
    Decimal,
}

/// What a parameter's value does to its keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Replace each key's value with the sampled value.
    #[default]
    Set,
    /// Multiply every number under each key (a scalar, or a whole map such as
    /// `customers.births_per_woman`) by the sampled factor. A whole-number leaf stays whole.
    Scale,
}

/// One varied parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// Column name in `models.csv` (`param_<name>`): letters, digits and `_`.
    pub name: String,
    /// One config key, in `--set` form: `file.key.path` (e.g. `game.leases.term_years`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Several keys that all take the same value (or factor).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Lower bound (inclusive). Required for a numeric parameter, absent for a categorical one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Upper bound (inclusive). Required for a numeric parameter, absent for a categorical one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The values of a **categorical** parameter (e.g. `[book_decay, market]`): each model sets
    /// the key to one of them, and the hypercube gives each an equal share of the models.
    /// Empty for a numeric parameter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// `uniform` (default) or `log_uniform`.
    #[serde(default)]
    pub distribution: Distribution,
    /// `integer` or `decimal` (default).
    #[serde(default)]
    pub kind: Kind,
    /// Decimal places for `decimal` values (default 2: money is pence, ratios basis points).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u32>,
    /// `set` (default) or `scale`.
    #[serde(default)]
    pub mode: Mode,
    /// Skip this parameter, with a warning, if its key is missing from the config (for keys
    /// that only exist once some content lands). Default false: a missing key is an error.
    #[serde(default)]
    pub optional: bool,
}

/// Default decimal places.
pub const DEFAULT_DECIMALS: u32 = 2;

impl Parameter {
    /// A `set` parameter on one key.
    pub fn set(
        name: &str,
        key: &str,
        min: f64,
        max: f64,
        distribution: Distribution,
        kind: Kind,
    ) -> Self {
        Parameter {
            name: name.to_owned(),
            key: Some(key.to_owned()),
            keys: Vec::new(),
            min: Some(min),
            max: Some(max),
            values: Vec::new(),
            distribution,
            kind,
            decimals: None,
            mode: Mode::Set,
            optional: false,
        }
    }

    /// A `scale` parameter on one key: a uniform decimal factor.
    pub fn scale(name: &str, key: &str, min: f64, max: f64) -> Self {
        Parameter {
            mode: Mode::Scale,
            ..Parameter::set(name, key, min, max, Distribution::Uniform, Kind::Decimal)
        }
    }

    /// A categorical parameter on one key: each model sets it to one of `values`.
    pub fn categorical(name: &str, key: &str, values: &[&str]) -> Self {
        Parameter {
            min: None,
            max: None,
            values: values.iter().map(|v| (*v).to_owned()).collect(),
            ..Parameter::set(name, key, 0.0, 0.0, Distribution::Uniform, Kind::Integer)
        }
    }

    /// True for a categorical parameter (it lists `values`).
    pub fn is_categorical(&self) -> bool {
        !self.values.is_empty()
    }

    /// The numeric range `[min, max]` (`(0, 0)` for a categorical parameter, which has none).
    pub fn range(&self) -> (f64, f64) {
        (self.min.unwrap_or(0.0), self.max.unwrap_or(0.0))
    }

    /// A model's value as written in the config and `models.csv`: the category name for a
    /// categorical parameter (the value is its index), otherwise the number to its places.
    pub fn label(&self, value: f64) -> String {
        if self.is_categorical() {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a category index is a small whole number"
            )]
            let i = value.max(0.0) as usize;
            self.values.get(i).cloned().unwrap_or_default()
        } else {
            super::models::value_text(value, self.places())
        }
    }

    /// Every key it edits, `key` first.
    pub fn all_keys(&self) -> Vec<&str> {
        self.key
            .iter()
            .map(String::as_str)
            .chain(self.keys.iter().map(String::as_str))
            .collect()
    }

    /// Decimal places for its values (0 for integers).
    pub fn places(&self) -> u32 {
        match self.kind {
            Kind::Integer => 0,
            Kind::Decimal => self.decimals.unwrap_or(DEFAULT_DECIMALS),
        }
    }
}

/// The balance criteria (R65). Defaults are the ruling's numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Criteria {
    /// Every AI personality's win share is at least this (R65: 10%).
    pub min_win_share: f64,
    /// ... and at most this (R65: 40%).
    pub max_win_share: f64,
    /// AI bankruptcies per AI player-game must be *below* this (R65: 15%).
    pub max_bankruptcy_rate: f64,
    /// The idle seat finishes below every AI still solvent in at least this share of games
    /// (R65: every game, 1.0). Bankrupt AIs are left to `max_bankruptcy_rate`.
    pub min_idle_below_solvent_rate: f64,
}

impl Default for Criteria {
    fn default() -> Self {
        Criteria {
            min_win_share: 0.10,
            max_win_share: 0.40,
            max_bankruptcy_rate: 0.15,
            min_idle_below_solvent_rate: 1.0,
        }
    }
}

/// A whole study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StudyDef {
    /// Models, including model 0 (the shipped defaults). Default 100 (R64).
    pub models: u32,
    /// Games per model, rounded up to a whole number of seat rotations. Default 500 (R64).
    pub games_per_model: u32,
    /// Preset from `game.yaml`; if missing, the longest preset is used. Default `full`.
    pub preset: String,
    /// Base seed: drives the Latin hypercube and the game seeds. Default 0.
    pub seed: u64,
    /// Seats: personality ids from `ai.yaml` and `idle`. Default: every personality, then one
    /// `idle` seat.
    pub roster: Option<Vec<String>>,
    /// How many of the best balanced models get a ready-to-copy overlay. Default 10.
    pub top_n: u32,
    /// The R65 thresholds.
    pub criteria: Criteria,
    /// The varied parameters.
    pub parameters: Vec<Parameter>,
}

impl Default for StudyDef {
    fn default() -> Self {
        StudyDef {
            models: 100,
            games_per_model: 500,
            preset: "full".to_owned(),
            seed: 0,
            roster: None,
            top_n: 10,
            criteria: Criteria::default(),
            parameters: Vec::new(),
        }
    }
}

/// The roster entry name for the idle control seat.
pub const IDLE: &str = "idle";

impl StudyDef {
    /// Parses and validates a definition.
    pub fn from_yaml(text: &str) -> Result<Self, StudyError> {
        let de = serde_yaml_ng::Deserializer::from_str(text);
        let def: StudyDef = serde_path_to_error::deserialize(de).map_err(|e| {
            let path = e.path().to_string();
            StudyError::def(
                if path == "." { "(root)".into() } else { path },
                e.into_inner().to_string(),
            )
        })?;
        def.validate()?;
        Ok(def)
    }

    /// Checks counts, names, ranges and criteria. Config keys are checked against the base
    /// config when the study starts.
    pub fn validate(&self) -> Result<(), StudyError> {
        if self.models == 0 {
            return Err(StudyError::def("models", "must be at least 1"));
        }
        if self.games_per_model == 0 {
            return Err(StudyError::def("games_per_model", "must be at least 1"));
        }
        if self.preset.is_empty() {
            return Err(StudyError::def("preset", "must not be empty"));
        }
        if let Some(roster) = &self.roster {
            for (i, a) in roster.iter().enumerate() {
                if roster[..i].contains(a) {
                    return Err(StudyError::def(
                        format!("roster[{i}]"),
                        format!("{a:?} is listed twice"),
                    ));
                }
            }
        }
        let c = &self.criteria;
        for (name, v) in [
            ("criteria.min_win_share", c.min_win_share),
            ("criteria.max_win_share", c.max_win_share),
            ("criteria.max_bankruptcy_rate", c.max_bankruptcy_rate),
            (
                "criteria.min_idle_below_solvent_rate",
                c.min_idle_below_solvent_rate,
            ),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(StudyError::def(name, "must be between 0 and 1"));
            }
        }
        if c.min_win_share > c.max_win_share {
            return Err(StudyError::def(
                "criteria.min_win_share",
                "must not exceed max_win_share",
            ));
        }
        for (i, p) in self.parameters.iter().enumerate() {
            let at = |field: &str| format!("parameters[{i}].{field}");
            if p.name.is_empty()
                || !p
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(StudyError::def(
                    at("name"),
                    format!("{:?}: use letters, digits and _", p.name),
                ));
            }
            if self.parameters[..i].iter().any(|q| q.name == p.name) {
                return Err(StudyError::def(
                    at("name"),
                    format!("{:?} is used twice", p.name),
                ));
            }
            if p.all_keys().is_empty() {
                return Err(StudyError::def(at("key"), "give `key` or `keys`"));
            }
            for k in p.all_keys() {
                if format!("{k}=0")
                    .parse::<crate::overrides::Override>()
                    .is_err()
                {
                    return Err(StudyError::def(
                        at("key"),
                        format!("{k:?} is not `file.key.path`"),
                    ));
                }
            }
            if p.is_categorical() {
                check_categorical(p, &at)?;
                continue;
            }
            let (Some(min), Some(max)) = (p.min, p.max) else {
                return Err(StudyError::def(
                    at("min"),
                    "give min and max (or `values` for a categorical parameter)",
                ));
            };
            if !min.is_finite() || !max.is_finite() {
                return Err(StudyError::def(at("min"), "min and max must be numbers"));
            }
            if min > max {
                return Err(StudyError::def(at("min"), "must not exceed max"));
            }
            if p.distribution == Distribution::LogUniform && min <= 0.0 {
                return Err(StudyError::def(at("min"), "log_uniform needs min > 0"));
            }
            if p.kind == Kind::Integer && (min.fract() != 0.0 || max.fract() != 0.0) {
                return Err(StudyError::def(
                    at("min"),
                    "integer parameters need whole min and max",
                ));
            }
            if p.mode == Mode::Scale && p.kind == Kind::Integer {
                return Err(StudyError::def(at("kind"), "a scale factor is a decimal"));
            }
            if p.places() > 6 {
                return Err(StudyError::def(at("decimals"), "at most 6"));
            }
        }
        Ok(())
    }
}

/// A categorical parameter: plain distinct values, no range, set mode, nothing numeric.
fn check_categorical(p: &Parameter, at: &dyn Fn(&str) -> String) -> Result<(), StudyError> {
    if p.min.is_some() || p.max.is_some() {
        return Err(StudyError::def(
            at("min"),
            "a categorical parameter has `values`, not min and max",
        ));
    }
    if p.mode == Mode::Scale {
        return Err(StudyError::def(
            at("mode"),
            "a categorical parameter sets its key",
        ));
    }
    if p.distribution != Distribution::Uniform || p.decimals.is_some() {
        return Err(StudyError::def(
            at("values"),
            "a categorical parameter takes no distribution or decimals",
        ));
    }
    for (j, v) in p.values.iter().enumerate() {
        let plain = v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if v.is_empty() || !plain {
            return Err(StudyError::def(
                at(&format!("values[{j}]")),
                format!("{v:?}: use letters, digits, _ and -"),
            ));
        }
        if p.values[..j].contains(v) {
            return Err(StudyError::def(
                at(&format!("values[{j}]")),
                format!("{v:?} is listed twice"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_rulings() {
        let d = StudyDef::from_yaml("{}").unwrap();
        assert_eq!((d.models, d.games_per_model), (100, 500));
        assert_eq!(d.preset, "full");
        assert_eq!(d.criteria, Criteria::default());
        assert!((d.criteria.max_bankruptcy_rate - 0.15).abs() < 1e-12);
    }

    #[test]
    fn parses_a_parameter() {
        let d = StudyDef::from_yaml(
            "parameters:\n  - { name: lease, key: game.leases.term_years, min: 10, max: 60, kind: integer }\n  - { name: k, keys: [customers.price_sensitivity.working], min: 0.5, max: 2, distribution: log_uniform, decimals: 3, mode: scale }\n",
        )
        .unwrap();
        assert_eq!(d.parameters[0].kind, Kind::Integer);
        assert_eq!(d.parameters[0].places(), 0);
        assert_eq!(d.parameters[1].mode, Mode::Scale);
        assert_eq!(d.parameters[1].places(), 3);
        assert_eq!(
            d.parameters[1].all_keys(),
            vec!["customers.price_sensitivity.working"]
        );
    }

    #[test]
    fn parses_a_categorical_parameter() {
        let d = StudyDef::from_yaml(
            "parameters:\n  - { name: land, key: game.net_worth.land_value, values: [book_decay, market] }\n",
        )
        .unwrap();
        let p = &d.parameters[0];
        assert!(p.is_categorical());
        assert_eq!((p.min, p.max), (None, None));
        assert_eq!(
            (p.label(0.0), p.label(1.0)),
            ("book_decay".into(), "market".into())
        );
    }

    #[test]
    fn rejects_bad_definitions_naming_the_key() {
        for (yaml, path) in [
            ("modles: 3", "unknown field `modles`"),
            ("models: -1", "models"),
            ("models: 0", "models"),
            ("roster: [grocer, grocer]", "roster[1]"),
            ("criteria: { min_win_share: 2 }", "criteria.min_win_share"),
            (
                "parameters: [{ name: x, key: game.a, min: 2, max: 1 }]",
                "parameters[0].min",
            ),
            (
                "parameters: [{ name: x, key: game.a, min: 0, max: 1, distribution: log_uniform }]",
                "parameters[0].min",
            ),
            (
                "parameters: [{ name: x, key: game.a, min: 0.5, max: 1, kind: integer }]",
                "parameters[0].min",
            ),
            (
                "parameters: [{ name: x, min: 0, max: 1 }]",
                "parameters[0].key",
            ),
            (
                "parameters: [{ name: x, key: nofile.a, min: 0, max: 1 }]",
                "parameters[0].key",
            ),
            (
                "parameters: [{ name: 'a b', key: game.a, min: 0, max: 1 }]",
                "parameters[0].name",
            ),
            (
                "parameters: [{ name: x, key: game.a, min: 0, max: 1, colour: red }]",
                "parameters[0]",
            ),
            (
                "parameters: [{ name: x, key: game.a, max: 1 }]",
                "parameters[0].min",
            ),
            (
                "parameters: [{ name: x, key: game.a, values: [a, b], min: 0, max: 1 }]",
                "parameters[0].min",
            ),
            (
                "parameters: [{ name: x, key: game.a, values: [a, b], mode: scale }]",
                "parameters[0].mode",
            ),
            (
                "parameters: [{ name: x, key: game.a, values: [a, a] }]",
                "parameters[0].values[1]",
            ),
            (
                "parameters: [{ name: x, key: game.a, values: [a, 'b c'] }]",
                "parameters[0].values[1]",
            ),
            (
                "parameters: [{ name: x, key: game.a, values: [a, b], distribution: log_uniform }]",
                "parameters[0].values",
            ),
        ] {
            let err = StudyDef::from_yaml(yaml).unwrap_err().to_string();
            assert!(err.contains(path), "{yaml}: {err}");
        }
    }
}
