//! An evolution's fixed context, resolved before the first game: the preset, the roster, the
//! evolving personalities and their genes, the base genomes and the seed plan; and the configs,
//! populations and champions derived from it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sim_core::{Config, ConfigSources};

use super::def::{EvolveDef, EvolveError, gene_personality};
use super::fitness::FitnessWeights;
use super::genome::{self, Genome, Origin, breed, first_population};
use super::output::Candidate;
use super::run::{EvolveOptions, generation_seed};
use crate::overrides::{self, Override};
use crate::study::models::node_at;
use crate::study::run::{check_expectations, resolve_preset};
use crate::study::seats::{GamePlan, effective_games, plan};
use crate::study::{Parameter, Roster, StudyDef, resolve_roster};

/// Everything resolved before the first game.
pub(super) struct Evolution<'a> {
    pub(super) def: &'a EvolveDef,
    pub(super) base: &'a ConfigSources,
    pub(super) opts: &'a EvolveOptions,
    pub(super) preset: String,
    pub(super) roster: Roster,
    /// The evolving personalities, in roster order.
    pub(super) personalities: Vec<String>,
    /// Each evolving personality's genes, in definition order.
    pub(super) genes: BTreeMap<String, Vec<&'a Parameter>>,
    /// Each evolving personality's base genome: the champion before generation 0.
    pub(super) base_genomes: BTreeMap<String, Genome>,
    pub(super) seats: usize,
    /// Seed blocks per evaluation: games per evaluation / seats.
    pub(super) blocks: u32,
    /// The fitness weights (checked when the definition was validated).
    pub(super) weights: FitnessWeights,
}

/// The personalities that evolve: the definition's list (each must sit in the roster), in
/// roster order, or every personality in the roster.
fn evolving(def: &EvolveDef, roster: &Roster) -> Result<Vec<String>, EvolveError> {
    let in_roster: Vec<String> = roster
        .entries()
        .iter()
        .filter(|e| e.is_ai())
        .map(|e| e.name().to_owned())
        .collect();
    let Some(list) = &def.evolve else {
        return Ok(in_roster);
    };
    for (i, p) in list.iter().enumerate() {
        if !in_roster.contains(p) {
            return Err(EvolveError::def(
                format!("evolve[{i}]"),
                format!(
                    "{p:?} is not a personality in the roster ({})",
                    in_roster.join(", ")
                ),
            ));
        }
    }
    // Roster order, so the outputs do not depend on how the list is written.
    Ok(in_roster.into_iter().filter(|p| list.contains(p)).collect())
}

/// Each evolving personality's genes. Every gene's personality must exist in `ai.yaml` and its
/// keys in the base config (unless `optional`); genes of a personality that does not evolve are
/// skipped (with a note when `notes`), and every evolving personality needs at least one.
fn genes_by_personality<'a>(
    def: &'a EvolveDef,
    base: &ConfigSources,
    config: &Config,
    personalities: &[String],
    notes: bool,
) -> Result<BTreeMap<String, Vec<&'a Parameter>>, EvolveError> {
    let mut genes: BTreeMap<String, Vec<&'a Parameter>> = BTreeMap::new();
    for (i, g) in def.genes.iter().enumerate() {
        let owner = g
            .all_keys()
            .first()
            .and_then(|k| gene_personality(k))
            .unwrap_or_default()
            .to_owned();
        if !config.ai.personalities.contains_key(&owner) {
            return Err(EvolveError::def(
                format!("genes[{i}].key"),
                format!("{owner:?} is not a personality in ai.yaml"),
            ));
        }
        for k in g.all_keys() {
            if node_at(base, k).is_none() && !g.optional {
                return Err(EvolveError::def(
                    format!("genes[{i}].key"),
                    format!("no key {k} in the base config"),
                ));
            }
        }
        if personalities.contains(&owner) {
            genes.entry(owner).or_default().push(g);
        } else if notes {
            eprintln!("evolve: gene {} skipped: {owner} does not evolve", g.name);
        }
    }
    for p in personalities {
        if !genes.contains_key(p) {
            return Err(EvolveError::def(
                "genes",
                format!("{p} evolves but has no genes"),
            ));
        }
    }
    Ok(genes)
}

/// R66: the Grocer stays pure retail. Checked on the base config and on every config a game
/// is played with.
pub(super) fn grocer_stays_retail(config: &Config) -> Result<(), String> {
    if config.ai.grocer.opportunism.enabled {
        return Err(
            "ai.yaml: grocer.opportunism.enabled is true, but the Grocer stays pure retail (R66)"
                .into(),
        );
    }
    Ok(())
}

impl<'a> Evolution<'a> {
    /// Validates the definition against the base config and resolves everything fixed.
    pub(super) fn prepare(
        def: &'a EvolveDef,
        base: &'a ConfigSources,
        opts: &'a EvolveOptions,
    ) -> Result<Self, EvolveError> {
        def.validate()?;
        let base_config =
            overrides::load(base, None, &[]).map_err(|e| EvolveError::BaseConfig(e.to_string()))?;
        grocer_stays_retail(&base_config)
            .map_err(|e| EvolveError::BaseConfig(format!("{e}; an evolution refuses to start")))?;
        let preset = resolve_preset(&base_config, &def.preset)
            .ok_or_else(|| EvolveError::BaseConfig("game.yaml has no presets".into()))?;
        if preset != def.preset && opts.progress {
            eprintln!(
                "evolve: preset {:?} not in game.yaml; using the longest, {preset:?}",
                def.preset
            );
        }
        // The study's checks, on the genes as its parameters.
        let shim = StudyDef {
            roster: def.roster.clone(),
            parameters: def.genes.clone(),
            ..StudyDef::default()
        };
        let roster = resolve_roster(&shim, &base_config)?;
        let personalities = evolving(def, &roster)?;
        let genes = genes_by_personality(def, base, &base_config, &personalities, opts.progress)?;
        check_expectations(&shim, base)?;
        let base_genomes = genes
            .iter()
            .map(|(p, g)| (p.clone(), genome::base_genome(g, base)))
            .collect();
        let seats = roster.len();
        let blocks =
            effective_games(def.games_per_eval, seats) / u32::try_from(seats.max(1)).unwrap_or(1);
        Ok(Evolution {
            def,
            base,
            opts,
            preset,
            roster,
            personalities,
            genes,
            base_genomes,
            seats,
            blocks,
            weights: def.fitness.weights()?,
        })
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.opts.out.join(name)
    }

    pub(super) fn champion_dir(&self, g: u32) -> PathBuf {
        self.opts.out.join("champions").join(g.to_string())
    }

    pub(super) fn genes_of(&self, p: &str) -> Option<Vec<&'a Parameter>> {
        self.genes.get(p).cloned()
    }

    /// The fitness weights, in exact integer form.
    pub(super) fn weights(&self) -> &FitnessWeights {
        &self.weights
    }

    pub(super) fn games_per_eval(&self) -> u32 {
        self.blocks * u32::try_from(self.seats).unwrap_or(1)
    }

    /// Generation `g`'s games: its seed blocks, every rotation of each.
    pub(super) fn plans(&self, g: u32) -> Vec<GamePlan> {
        plan(
            self.games_per_eval(),
            self.seats,
            generation_seed(self.def.seed, g, self.blocks),
        )
    }

    /// The overrides of a lineup: each evolving personality's genome, in personality order.
    pub(super) fn lineup_overrides(
        &self,
        genomes: &BTreeMap<String, Genome>,
    ) -> Result<Vec<Override>, String> {
        let mut out = Vec::new();
        for p in &self.personalities {
            if let (Some(genes), Some(g)) = (self.genes.get(p), genomes.get(p)) {
                out.extend(genome::overrides(genes, self.base, g).map_err(|e| e.to_string())?);
            }
        }
        Ok(out)
    }

    /// The config files (with the preset) and the validated config a lineup plays with; an
    /// error (the loader's message, naming the key) if a genome makes it invalid.
    pub(super) fn lineup_config(
        &self,
        genomes: &BTreeMap<String, Genome>,
    ) -> Result<(ConfigSources, Config), String> {
        let ovs = self.lineup_overrides(genomes)?;
        let files =
            overrides::resolve(self.base, Some(&self.preset), &ovs).map_err(|e| e.to_string())?;
        let config =
            Config::load(&files).map_err(|e| overrides::LoadError::Invalid(e).to_string())?;
        grocer_stays_retail(&config)?;
        Ok((files, config))
    }

    /// Each personality's champion after generation `g` (the base genome before generation
    /// 0): its latest rank-1 candidate, or the base genome if it never had a valid one.
    pub(super) fn champions_after(
        &self,
        rows: &[Candidate],
        g: Option<u32>,
    ) -> BTreeMap<String, Genome> {
        let mut champs = self.base_genomes.clone();
        let Some(last) = g else { return champs };
        for done in 0..=last {
            for c in rows.iter().filter(|c| c.generation == done && c.champion) {
                champs.insert(c.personality.clone(), c.genome.clone());
            }
        }
        champs
    }

    /// The population of generation `g`, per personality: the first population, or bred from
    /// generation `g − 1`'s valid candidates in rank order.
    pub(super) fn population(
        &self,
        rows: &[Candidate],
        g: u32,
    ) -> BTreeMap<String, Vec<(Genome, Origin)>> {
        self.personalities
            .iter()
            .map(|p| {
                let genes = &self.genes[p];
                let pop = match g.checked_sub(1) {
                    None => {
                        first_population(genes, self.base, self.def.population, self.def.seed, p)
                    }
                    Some(prev) => {
                        let mut ranked: Vec<(u32, u32, Genome)> = rows
                            .iter()
                            .filter(|c| c.generation == prev && &c.personality == p)
                            .filter_map(|c| c.rank.map(|r| (r, c.index, c.genome.clone())))
                            .collect();
                        ranked.sort_by_key(|(r, i, _)| (*r, *i));
                        let ranked: Vec<(u32, Genome)> =
                            ranked.into_iter().map(|(_, i, g)| (i, g)).collect();
                        breed(genes, &ranked, &self.base_genomes[p], self.def, p, g)
                    }
                };
                (p.clone(), pop)
            })
            .collect()
    }
}
