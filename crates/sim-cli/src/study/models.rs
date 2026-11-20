//! Models: model 0 is the shipped defaults, models 1.. are Latin hypercube samples.

use sim_core::{Config, ConfigSources};

use super::def::{StudyDef, StudyError};
use crate::overrides::{LoadError, Override};

/// One model of a study.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpec {
    /// 0 is the baseline.
    pub id: u32,
    /// Unit samples per parameter (`None` for the baseline).
    pub units: Option<Vec<f64>>,
    /// Values per parameter (`None` for the baseline).
    pub values: Option<Vec<f64>>,
}

/// Every model of the study, in id order.
pub fn generate(def: &StudyDef) -> Result<Vec<ModelSpec>, StudyError> {
    let _ = def;
    todo!("generate")
}

/// The `--set` overrides that make `model` from the base config.
pub fn overrides_for(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
) -> Result<Vec<Override>, LoadError> {
    let _ = (def, base, model);
    todo!("overrides_for")
}

/// Each parameter's value in the base config (its first key; 1 for a scale factor).
pub fn baseline_values(def: &StudyDef, base: &ConfigSources) -> Vec<Option<f64>> {
    let _ = (def, base);
    todo!("baseline_values")
}

/// The validated config a model's games run with.
pub fn model_config(
    def: &StudyDef,
    base: &ConfigSources,
    model: &ModelSpec,
    preset: &str,
) -> Result<Config, LoadError> {
    let _ = (def, base, model, preset);
    todo!("model_config")
}
