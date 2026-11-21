//! The evolution definition file (`study/evolve.yaml`, docs/evolve.md): which personalities
//! evolve, their genes, the population, the operators, the economy and the run size.
//!
//! Parsing is strict, like the study's: unknown keys and wrong types are rejected naming the
//! key path, and [`EvolveDef::validate`] checks every count, rate and gene before any game.
//! Genes use the study's parameter format ([`Parameter`]), restricted to `ai.<personality>.*`.

use serde::{Deserialize, Serialize};

use super::fitness::FitnessWeights;
use crate::study::Parameter;
use crate::study::def::check_parameters;

/// Why an evolution could not be defined or run. Genomes whose config is invalid are *not*
/// errors: they are reported in `generations.csv` (`status = invalid`) and never bred from.
#[derive(Debug, thiserror::Error)]
pub enum EvolveError {
    /// The definition file does not parse or breaks a rule; names the key path.
    #[error("evolution definition: {path}: {reason}")]
    Definition {
        /// Key path inside the definition, e.g. `genes[2].key`.
        path: String,
        /// What is wrong.
        reason: String,
    },
    /// The base config (shipped + economy + `--set`) does not load, or breaks a hard rule.
    #[error("base config: {0}")]
    BaseConfig(String),
    /// The output directory holds a different evolution.
    #[error(
        "{dir} holds a different evolution (fingerprint {found}, this one {expected}); \
         use --fresh to start over or choose another --out"
    )]
    Mismatch {
        /// Output directory.
        dir: String,
        /// Fingerprint found there.
        found: String,
        /// This evolution's fingerprint.
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
    /// Recording the games failed (`--record`).
    #[error("recording: {0}")]
    Record(String),
}

impl EvolveError {
    pub(crate) fn def(path: impl Into<String>, reason: impl Into<String>) -> Self {
        EvolveError::Definition {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        EvolveError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

impl From<crate::study::StudyError> for EvolveError {
    fn from(e: crate::study::StudyError) -> Self {
        use crate::study::StudyError as S;
        match e {
            S::Definition { path, reason } => EvolveError::Definition { path, reason },
            S::BaseConfig(m) => EvolveError::BaseConfig(m),
            S::Mismatch {
                dir,
                found,
                expected,
            } => EvolveError::Mismatch {
                dir,
                found,
                expected,
            },
            S::Io { path, source } => EvolveError::Io { path, source },
            S::Pool(p) => EvolveError::Pool(p),
            S::Record(m) => EvolveError::Record(m),
        }
    }
}

/// How parents are picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMethod {
    /// Best of `tournament_size` candidates drawn uniformly, with replacement.
    #[default]
    Tournament,
    /// Linear rank selection: the candidate ranked `r` of `n` (1 = best) is drawn with weight
    /// `n − r + 1`.
    Rank,
}

/// Parent selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Selection {
    /// `tournament` (default) or `rank`.
    pub method: SelectionMethod,
    /// Candidates per tournament (default 3). Ignored by `rank`.
    pub tournament_size: u32,
}

impl Default for Selection {
    fn default() -> Self {
        Selection {
            method: SelectionMethod::Tournament,
            tournament_size: 3,
        }
    }
}

/// Gaussian mutation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Mutation {
    /// Chance that each gene of a child mutates (default 0.25). A child none of whose genes
    /// came up mutates one gene picked uniformly, so no child is a plain copy.
    pub rate: f64,
    /// Standard deviation of a mutation as a share of the gene's range (default 0.1); for a
    /// log-uniform gene, of its range in log space.
    pub sigma: f64,
}

impl Default for Mutation {
    fn default() -> Self {
        Mutation {
            rate: 0.25,
            sigma: 0.1,
        }
    }
}

/// Uniform crossover.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Crossover {
    /// Chance that a child has two parents (default 0.5; 0 turns crossover off). Each gene then
    /// comes from either parent with even odds.
    pub rate: f64,
}

impl Default for Crossover {
    fn default() -> Self {
        Crossover { rate: 0.5 }
    }
}

/// The fitness weights (docs/evolve.md "Fitness"). At or above `low_win_floor`, fitness =
/// win share − `bankruptcy_penalty` × bankruptcy rate; below it, net worth gained relative to
/// idle is blended in (R78). Shares and weights are converted to whole parts per million when
/// the definition loads ([`FitnessDef::weights`]), so the ranking uses integers only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FitnessDef {
    /// Penalty per unit of bankruptcy rate (default 0.5; 0 to 1,000, at most six decimals).
    pub bankruptcy_penalty: f64,
    /// The low-win floor, a win share (default 0.10; 0 to 1, at most six decimals; 0 turns the
    /// blend off). Below it, the net-worth term is blended in.
    pub low_win_floor: f64,
    /// Pence of median margin over idle (`nw_vs_idle_median`) that make a full net-worth term:
    /// the term is that margin over this scale, clamped to −1..1 (default 700,000: £7,000, the
    /// starting cash, R62). Must be positive.
    pub net_worth_scale: i64,
    /// The net-worth weight at a win share of 0; it falls linearly to 0 at the floor (default
    /// 0.8; 0 to 1, at most six decimals).
    pub max_net_worth_weight: f64,
}

impl Default for FitnessDef {
    fn default() -> Self {
        FitnessDef {
            bankruptcy_penalty: 0.5,
            low_win_floor: 0.10,
            net_worth_scale: 700_000,
            max_net_worth_weight: 0.8,
        }
    }
}

/// `v` in whole parts per million, if it is finite, within `0..=max` and has at most six
/// decimal places; otherwise a definition error naming `path`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "checked: finite, within 0..=max (at most 1,000), and whole after scaling"
)]
fn ppm(path: &str, v: f64, max: f64) -> Result<i64, EvolveError> {
    if !v.is_finite() || !(0.0..=max).contains(&v) {
        return Err(EvolveError::def(
            path,
            format!("must be between 0 and {max}"),
        ));
    }
    let scaled = v * 1e6;
    let whole = scaled.round();
    if (scaled - whole).abs() > 1e-3 {
        return Err(EvolveError::def(
            path,
            "must have at most six decimal places",
        ));
    }
    Ok(whole as i64)
}

impl FitnessDef {
    /// The weights in exact integer form, or a definition error naming the bad key.
    pub fn weights(&self) -> Result<FitnessWeights, EvolveError> {
        // Red: only the penalty is checked; the blend's keys are not validated yet.
        let bankruptcy_penalty = ppm(
            "fitness.bankruptcy_penalty",
            self.bankruptcy_penalty,
            1000.0,
        )?;
        Ok(FitnessWeights {
            bankruptcy_penalty,
            low_win_floor: 0,
            net_worth_scale: self.net_worth_scale,
            max_net_worth_weight: 0,
        })
    }
}

/// A whole evolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct EvolveDef {
    /// Personalities that evolve. Default: every personality in the roster.
    pub evolve: Option<Vec<String>>,
    /// Genes, `ai.<personality>.<key>`, in the study's parameter format.
    pub genes: Vec<Parameter>,
    /// Genomes per personality per generation (default 16).
    pub population: u32,
    /// Generations (default 20).
    pub generations: u32,
    /// Games per candidate evaluation, rounded up to whole seat rotations (default 120).
    pub games_per_eval: u32,
    /// Best genomes per personality carried into the next generation unchanged (default 2).
    pub elitism: u32,
    /// Parent selection.
    pub selection: Selection,
    /// Gaussian mutation.
    pub mutation: Mutation,
    /// Uniform crossover.
    pub crossover: Crossover,
    /// Fitness weights.
    pub fitness: FitnessDef,
    /// An adopted `economy.yaml` to evolve under, instead of the config directory's. A path
    /// relative to the working directory.
    pub economy: Option<String>,
    /// Preset from `game.yaml`; if missing, the longest preset is used. Default `full`.
    pub preset: String,
    /// Seed: drives the first population, the breeding and the game seeds. Default 0.
    pub seed: u64,
    /// Seats: personality ids and at most one `idle`. Default: every personality in `ai.yaml`,
    /// then one `idle` seat.
    pub roster: Option<Vec<String>>,
    /// Record every game here (`s3` or a local directory); `--record` overrides it.
    pub record: Option<String>,
}

impl Default for EvolveDef {
    fn default() -> Self {
        EvolveDef {
            evolve: None,
            genes: Vec::new(),
            population: 16,
            generations: 20,
            games_per_eval: 120,
            elitism: 2,
            selection: Selection::default(),
            mutation: Mutation::default(),
            crossover: Crossover::default(),
            fitness: FitnessDef::default(),
            economy: None,
            preset: "full".to_owned(),
            seed: 0,
            roster: None,
            record: None,
        }
    }
}

/// The `ai.yaml` keys under each personality's id that are not its tunables.
const NOT_A_PERSONALITY: [&str; 3] = ["personalities", "opponents", "decision_interval_ticks"];

/// The personality a gene key belongs to: `ai.<id>.<field>...` gives `<id>`.
pub fn gene_personality(key: &str) -> Option<&str> {
    let mut parts = key.split('.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("ai"), Some(id), Some(field)) if !id.is_empty() && !field.is_empty() => Some(id),
        _ => None,
    }
}

/// True if `key` is (or is under) the Grocer's opportunism, which R66 keeps off: "the Grocer
/// stays pure retail".
pub fn is_grocer_opportunism(key: &str) -> bool {
    let mut parts = key.split('.');
    parts.next() == Some("ai")
        && parts.next() == Some("grocer")
        && parts.next() == Some("opportunism")
}

impl EvolveDef {
    /// Parses and validates a definition.
    pub fn from_yaml(text: &str) -> Result<Self, EvolveError> {
        let de = serde_yaml_ng::Deserializer::from_str(text);
        let def: EvolveDef = serde_path_to_error::deserialize(de).map_err(|e| {
            let path = e.path().to_string();
            EvolveError::def(
                if path == "." { "(root)".into() } else { path },
                e.into_inner().to_string(),
            )
        })?;
        def.validate()?;
        Ok(def)
    }

    /// Children bred per personality per generation: `population − elitism`.
    pub fn children(&self) -> u32 {
        self.population.saturating_sub(self.elitism)
    }

    /// Checks counts, rates and genes. Personalities and keys are checked against the base
    /// config when the run starts.
    pub fn validate(&self) -> Result<(), EvolveError> {
        if self.population < 2 {
            return Err(EvolveError::def("population", "must be at least 2"));
        }
        if self.generations == 0 {
            return Err(EvolveError::def("generations", "must be at least 1"));
        }
        if self.games_per_eval == 0 {
            return Err(EvolveError::def("games_per_eval", "must be at least 1"));
        }
        if self.elitism >= self.population {
            return Err(EvolveError::def(
                "elitism",
                "must be below population, or nothing new is ever tried",
            ));
        }
        if self.preset.is_empty() {
            return Err(EvolveError::def("preset", "must not be empty"));
        }
        let t = self.selection.tournament_size;
        if self.selection.method == SelectionMethod::Tournament && (t == 0 || t > self.population) {
            return Err(EvolveError::def(
                "selection.tournament_size",
                "must be between 1 and population",
            ));
        }
        for (name, v) in [
            ("mutation.rate", self.mutation.rate),
            ("crossover.rate", self.crossover.rate),
        ] {
            if !(0.0..=1.0).contains(&v) {
                return Err(EvolveError::def(name, "must be between 0 and 1"));
            }
        }
        if !(self.mutation.sigma > 0.0 && self.mutation.sigma <= 1.0) {
            return Err(EvolveError::def(
                "mutation.sigma",
                "must be above 0 and at most 1 (a share of the gene's range)",
            ));
        }
        self.fitness.weights()?;
        for (field, list) in [("evolve", &self.evolve), ("roster", &self.roster)] {
            if let Some(list) = list {
                for (i, a) in list.iter().enumerate() {
                    if list[..i].contains(a) {
                        return Err(EvolveError::def(
                            format!("{field}[{i}]"),
                            format!("{a:?} is listed twice"),
                        ));
                    }
                }
            }
        }
        if self.evolve.as_ref().is_some_and(Vec::is_empty) {
            return Err(EvolveError::def("evolve", "name at least one personality"));
        }
        if self.genes.is_empty() {
            return Err(EvolveError::def("genes", "give at least one gene"));
        }
        check_parameters(&self.genes, "genes")?;
        self.check_gene_keys()
    }

    /// Every gene edits one personality's tunables, `ai.<id>.<key>`, and never the Grocer's
    /// opportunism (R66).
    fn check_gene_keys(&self) -> Result<(), EvolveError> {
        for (i, g) in self.genes.iter().enumerate() {
            let mut owner: Option<&str> = None;
            for k in g.all_keys() {
                if is_grocer_opportunism(k) {
                    return Err(EvolveError::def(
                        format!("genes[{i}].key"),
                        format!(
                            "{k}: the Grocer stays pure retail (R66), so its opportunism is off \
                             and cannot be a gene"
                        ),
                    ));
                }
                let id = gene_personality(k)
                    .filter(|id| !NOT_A_PERSONALITY.contains(id))
                    .ok_or_else(|| {
                        EvolveError::def(
                            format!("genes[{i}].key"),
                            format!(
                                "{k:?}: a gene is one personality's setting, `ai.<personality>.<key>`"
                            ),
                        )
                    })?;
                if owner.is_some_and(|o| o != id) {
                    return Err(EvolveError::def(
                        format!("genes[{i}].keys"),
                        "every key of a gene belongs to the same personality",
                    ));
                }
                owner = Some(id);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENE: &str = "genes: [{ name: t, key: ai.toff.aggression, min: 0.1, max: 0.9 }]\n";

    #[test]
    fn defaults() {
        let d = EvolveDef::from_yaml(GENE).unwrap();
        assert_eq!((d.population, d.generations, d.elitism), (16, 20, 2));
        assert_eq!(d.games_per_eval, 120);
        assert_eq!(d.selection.method, SelectionMethod::Tournament);
        assert_eq!(d.preset, "full");
        assert_eq!(d.children(), 14);
        assert_eq!(
            d.fitness.weights().unwrap(),
            FitnessWeights {
                bankruptcy_penalty: 500_000,
                low_win_floor: 100_000,
                net_worth_scale: 700_000,
                max_net_worth_weight: 800_000,
            }
        );
        assert_eq!(d.fitness.weights().unwrap(), FitnessWeights::default());
    }

    #[test]
    fn fitness_weights_accept_their_bounds() {
        let w = EvolveDef::from_yaml(&format!(
            "fitness: {{ bankruptcy_penalty: 0, low_win_floor: 1, net_worth_scale: 1, \
             max_net_worth_weight: 1 }}\n{GENE}"
        ))
        .unwrap()
        .fitness
        .weights()
        .unwrap();
        assert_eq!(
            (
                w.bankruptcy_penalty,
                w.low_win_floor,
                w.max_net_worth_weight
            ),
            (0, 1_000_000, 1_000_000)
        );
        let off =
            EvolveDef::from_yaml(&format!("fitness: {{ low_win_floor: 0 }}\n{GENE}")).unwrap();
        assert_eq!(off.fitness.weights().unwrap().low_win_floor, 0);
        let fine = EvolveDef::from_yaml(&format!("fitness: {{ low_win_floor: 0.123456 }}\n{GENE}"))
            .unwrap();
        assert_eq!(fine.fitness.weights().unwrap().low_win_floor, 123_456);
    }

    #[test]
    fn gene_keys_name_their_personality() {
        assert_eq!(gene_personality("ai.toff.aggression"), Some("toff"));
        assert_eq!(
            gene_personality("ai.toff.opportunism.threshold"),
            Some("toff")
        );
        assert_eq!(gene_personality("ai.toff"), None);
        assert_eq!(gene_personality("economy.land.base_rate"), None);
        assert!(is_grocer_opportunism("ai.grocer.opportunism.enabled"));
        assert!(is_grocer_opportunism("ai.grocer.opportunism"));
        assert!(!is_grocer_opportunism("ai.toff.opportunism.enabled"));
        assert!(!is_grocer_opportunism("ai.grocer.max_bid_share"));
    }

    #[test]
    fn rejects_bad_definitions_naming_the_key() {
        for (yaml, path) in [
            ("genes: []", "genes"),
            ("population: 1\n", "population"),
            ("elitism: 16\n", "elitism"),
            ("generations: 0\n", "generations"),
            ("games_per_eval: 0\n", "games_per_eval"),
            ("mutation: { rate: 2 }\n", "mutation.rate"),
            ("mutation: { sigma: 0 }\n", "mutation.sigma"),
            ("crossover: { rate: -0.1 }\n", "crossover.rate"),
            (
                "selection: { tournament_size: 0 }\n",
                "selection.tournament_size",
            ),
            (
                "fitness: { bankruptcy_penalty: -1 }\n",
                "fitness.bankruptcy_penalty",
            ),
            (
                "fitness: { bankruptcy_penalty: 0.1234567 }\n",
                "fitness.bankruptcy_penalty: must have at most six decimal places",
            ),
            (
                "fitness: { low_win_floor: -0.01 }\n",
                "fitness.low_win_floor: must be between 0 and 1",
            ),
            (
                "fitness: { low_win_floor: 1.5 }\n",
                "fitness.low_win_floor: must be between 0 and 1",
            ),
            (
                "fitness: { low_win_floor: .nan }\n",
                "fitness.low_win_floor",
            ),
            (
                "fitness: { net_worth_scale: 0 }\n",
                "fitness.net_worth_scale: must be a positive",
            ),
            (
                "fitness: { net_worth_scale: -700000 }\n",
                "fitness.net_worth_scale: must be a positive",
            ),
            (
                "fitness: { net_worth_scale: 1.5 }\n",
                "fitness.net_worth_scale",
            ),
            (
                "fitness: { max_net_worth_weight: 1.01 }\n",
                "fitness.max_net_worth_weight: must be between 0 and 1",
            ),
            (
                "fitness: { max_net_worth_weight: -0.2 }\n",
                "fitness.max_net_worth_weight: must be between 0 and 1",
            ),
            (
                "fitness: { low_win_floor: 0.1, typo: 1 }\n",
                "unknown field `typo`",
            ),
            ("evolve: []\n", "evolve"),
            ("evolve: [toff, toff]\n", "evolve[1]"),
            ("populaton: 3\n", "unknown field `populaton`"),
        ] {
            let text = if yaml.starts_with("genes") {
                yaml.to_owned()
            } else {
                format!("{yaml}{GENE}")
            };
            let err = EvolveDef::from_yaml(&text).unwrap_err().to_string();
            assert!(err.contains(path), "{yaml}: {err}");
        }
        for (gene, says) in [
            (
                "{ name: x, key: economy.land.base_rate, min: 0, max: 1 }",
                "genes[0].key",
            ),
            (
                "{ name: x, key: ai.decision_interval_ticks.a, min: 1, max: 9 }",
                "genes[0].key",
            ),
            ("{ name: x, key: ai.toff, min: 0, max: 1 }", "genes[0].key"),
            (
                "{ name: x, keys: [ai.toff.aggression, ai.landlord.aggression], min: 0, max: 1 }",
                "genes[0].keys",
            ),
            (
                "{ name: x, key: ai.grocer.opportunism.enabled, values: [true, false] }",
                "R66",
            ),
            (
                "{ name: x, key: ai.grocer.opportunism.threshold, min: 0, max: 1 }",
                "R66",
            ),
            (
                "{ name: x, key: ai.toff.aggression, min: 2, max: 1 }",
                "genes[0].min",
            ),
        ] {
            let err = EvolveDef::from_yaml(&format!("genes: [{gene}]"))
                .unwrap_err()
                .to_string();
            assert!(err.contains(says), "{gene}: {err}");
        }
    }
}
