//! Models: model 0 is the shipped defaults (the baseline), models 1.. are Latin hypercube
//! samples. A model is a list of `--set` overrides on the base config, so it is validated
//! exactly like a hand-made sweep (R45) and can be adopted by copying its overlay.

use serde_yaml_ng::Value;
use sim_core::{Config, ConfigSources};

use super::def::{Mode, Parameter, StudyDef, StudyError};
use super::lhs::{category_at, round_to, unit_samples, value_at};
use crate::overrides::{self, LoadError, Override};

/// One model of a study.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpec {
    /// 0 is the baseline (the base config, untouched).
    pub id: u32,
    /// The hypercube's unit sample per parameter, in `[0, 1)` (`None` for the baseline).
    pub units: Option<Vec<f64>>,
    /// The value per parameter, rounded (`None` for the baseline): the value set, or the scale
    /// factor.
    pub values: Option<Vec<f64>>,
}

/// Every model of the study, in id order: the baseline, then `models − 1` hypercube samples.
pub fn generate(def: &StudyDef) -> Result<Vec<ModelSpec>, StudyError> {
    def.validate()?;
    let sampled = usize::try_from(def.models.saturating_sub(1))
        .map_err(|_| StudyError::def("models", "too many"))?;
    let units = unit_samples(def.parameters.len(), sampled, def.seed);
    let mut models = vec![ModelSpec {
        id: 0,
        units: None,
        values: None,
    }];
    for (id, u) in (1..).zip(units) {
        let values = def
            .parameters
            .iter()
            .zip(&u)
            .map(|(p, x)| {
                if p.is_categorical() {
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "a category index is far below 2^52"
                    )]
                    let i = category_at(*x, sampled, p.values.len()) as f64;
                    i
                } else {
                    value_at(p, *x)
                }
            })
            .collect();
        models.push(ModelSpec {
            id,
            units: Some(u),
            values: Some(values),
        });
    }
    Ok(models)
}

/// The value as YAML text: whole numbers without a point, decimals with their places.
pub fn value_text(v: f64, places: u32) -> String {
    if places == 0 {
        format!("{v:.0}")
    } else {
        format!("{v:.prec$}", prec = places as usize)
    }
}

/// The YAML node at `file.key.path` in `base`, if it exists.
pub fn node_at(base: &ConfigSources, key: &str) -> Option<Value> {
    let ov: Override = format!("{key}=0").parse().ok()?;
    let text = base.get(ov.file_name())?;
    let doc: Value = serde_yaml_ng::from_str(text).ok()?;
    let mut node = &doc;
    for part in key.split('.').skip(1) {
        node = match node {
            Value::Mapping(m) => m
                .iter()
                .find(|(k, _)| key_text(k).as_deref() == Some(part))
                .map(|(_, v)| v)?,
            Value::Sequence(s) => s.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(node.clone())
}

fn key_text(k: &Value) -> Option<String> {
    match k {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Every number under `node`, as (`.`-joined path suffix, value, is whole).
fn numeric_leaves(node: &Value, prefix: &str, out: &mut Vec<(String, f64, bool)>) {
    match node {
        Value::Number(n) => {
            if let Some(v) = n.as_f64() {
                out.push((prefix.to_owned(), v, n.is_i64() || n.is_u64()));
            }
        }
        Value::Mapping(m) => {
            for (k, v) in m {
                if let Some(k) = key_text(k) {
                    numeric_leaves(v, &format!("{prefix}.{k}"), out);
                }
            }
        }
        Value::Sequence(s) => {
            for (i, v) in s.iter().enumerate() {
                numeric_leaves(v, &format!("{prefix}.{i}"), out);
            }
        }
        _ => {}
    }
}

fn parse(text: &str) -> Result<Override, LoadError> {
    text.parse()
}

/// The overrides one parameter makes at `value`.
pub(crate) fn parameter_overrides(
    p: &Parameter,
    base: &ConfigSources,
    value: f64,
) -> Result<Vec<Override>, LoadError> {
    let mut out = Vec::new();
    for key in p.all_keys() {
        if p.optional && node_at(base, key).is_none() {
            continue;
        }
        match p.mode {
            Mode::Set => out.push(parse(&format!("{key}={}", p.label(value)))?),
            Mode::Scale => {
                let Some(node) = node_at(base, key) else {
                    // Let the normal path name the missing key.
                    out.push(parse(&format!("{key}={}", p.label(value)))?);
                    continue;
                };
                let mut leaves = Vec::new();
                numeric_leaves(&node, key, &mut leaves);
                for (path, v, whole) in leaves {
                    let scaled = v * value;
                    let text = if whole {
                        value_text(scaled.round(), 0)
                    } else {
                        value_text(round_to(scaled, p.places()), p.places())
                    };
                    out.push(parse(&format!("{path}={text}"))?);
                }
            }
        }
    }
    Ok(out)
}

/// The `--set` overrides that make `model` from the base config, in parameter then key
/// order. Empty for the baseline. An `optional` parameter whose key is missing from `base`
/// contributes nothing.
pub fn overrides_for(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
) -> Result<Vec<Override>, LoadError> {
    let Some(values) = &model.values else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (p, v) in def.parameters.iter().zip(values) {
        if p.optional && p.all_keys().iter().all(|k| node_at(base, k).is_none()) {
            continue;
        }
        out.extend(parameter_overrides(p, base, *v)?);
    }
    Ok(out)
}

/// Each parameter's value in the base config: the number at its first key for `set`, 1 for a
/// `scale` factor, the index of the base value among a categorical parameter's `values`;
/// `None` if the key holds no single number (or is missing, or a category not listed).
pub fn baseline_values(def: &StudyDef, base: &ConfigSources) -> Vec<Option<f64>> {
    parameter_baselines(&def.parameters, base)
}

/// [`baseline_values`] for any list of parameters (the evolution's genes too).
pub(crate) fn parameter_baselines(params: &[Parameter], base: &ConfigSources) -> Vec<Option<f64>> {
    params
        .iter()
        .map(|p| match p.mode {
            _ if p.is_categorical() => {
                let node = p.all_keys().first().and_then(|k| node_at(base, k))?;
                let text = node.as_str()?;
                let i = p.values.iter().position(|v| v == text)?;
                u32::try_from(i).ok().map(f64::from)
            }
            Mode::Scale => Some(1.0),
            Mode::Set => p
                .all_keys()
                .first()
                .and_then(|k| node_at(base, k))
                .and_then(|n| n.as_f64()),
        })
        .collect()
}

/// Each parameter's base-config value as `models.csv` shows it for model 0: the base value of
/// a categorical parameter's key as written (listed among its `values` or not), otherwise
/// [`baseline_values`] to the parameter's places.
pub fn baseline_labels(def: &StudyDef, base: &ConfigSources) -> Vec<Option<String>> {
    def.parameters
        .iter()
        .zip(baseline_values(def, base))
        .map(|(p, v)| {
            if p.is_categorical() {
                let node = p.all_keys().first().and_then(|k| node_at(base, k))?;
                node.as_str().map(str::to_owned)
            } else {
                v.map(|v| p.label(v))
            }
        })
        .collect()
}

/// The validated config a model's games run with: the base config, the preset, then the
/// model's overrides (the same order and checks as `sim-cli batch --preset P --set ...`).
pub fn model_config(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
    preset: &str,
) -> Result<Config, LoadError> {
    let ovs = overrides_for(def, base, model)?;
    overrides::load(base, Some(preset), &ovs)
}

/// The effective config files of a model under `preset`: what [`model_config`] validates, and
/// what a recorded game's config snapshot holds.
pub fn model_files(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
    preset: &str,
) -> Result<ConfigSources, LoadError> {
    let ovs = overrides_for(def, base, model)?;
    overrides::resolve(base, Some(preset), &ovs)
}

/// The base sources with a model's overrides applied, *without* the preset: the files a
/// winner's overlay holds, ready to copy over `config/`.
pub fn model_sources(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
) -> Result<(ConfigSources, Vec<Override>), LoadError> {
    let ovs = overrides_for(def, base, model)?;
    let mut edited = base.clone();
    for ov in &ovs {
        overrides::apply(&mut edited, ov)?;
    }
    Ok((edited, ovs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_sources() -> ConfigSources {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        ConfigSources::read_dir(&dir).unwrap()
    }

    /// The shipped study must parse, name only keys that exist, and mostly sample valid
    /// configs: a content change that breaks it shows up here, not in a long run.
    #[test]
    fn the_shipped_study_definition_is_usable() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../study/study.yaml");
        let def = StudyDef::from_yaml(&std::fs::read_to_string(path).unwrap()).unwrap();
        let base = shipped_sources();
        for p in def.parameters.iter().filter(|p| !p.optional) {
            for k in p.all_keys() {
                assert!(node_at(&base, k).is_some(), "{}: no key {k}", p.name);
            }
        }
        super::super::run::check_expectations(&def, &base).unwrap();
        let models = generate(&def).unwrap();
        assert_eq!(models.len(), def.models as usize);
        assert_eq!(
            model_config(&def, &base, &models[0], "full").unwrap().game,
            {
                crate::overrides::load(&base, Some("full"), &[])
                    .unwrap()
                    .game
            }
        );
        // A sample of 30 keeps the debug-build test quick.
        let errors: Vec<String> = models[1..31]
            .iter()
            .filter_map(|m| model_config(&def, &base, m, "full").err())
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.len() <= 3,
            "{} of 30 models fail: {errors:#?}",
            errors.len()
        );
    }

    #[test]
    fn an_optional_parameter_with_a_missing_key_changes_nothing() {
        let mut p = super::super::def::Parameter::set(
            "x",
            "game.no_such_key",
            1.0,
            2.0,
            super::super::def::Distribution::Uniform,
            super::super::def::Kind::Integer,
        );
        p.optional = true;
        let def = StudyDef {
            models: 3,
            parameters: vec![p],
            ..StudyDef::default()
        };
        let base = shipped_sources();
        for m in generate(&def).unwrap() {
            assert_eq!(overrides_for(&def, &base, &m).unwrap(), Vec::new());
        }
    }
}
