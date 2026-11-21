//! Genomes and the operators that make them: the first population, Gaussian mutation, uniform
//! crossover and parent selection (docs/evolve.md "The algorithm").
//!
//! A genome is one value per gene of one personality, in definition order. Every value is kept
//! **canonical**: clamped to the gene's range and rounded the way the gene asks (whole numbers,
//! `decimals` places, or a category index), so `parse(label(v)) == v` exactly and a resumed run
//! breeds from bit-identical genomes. Every draw comes from sim-core's portable ChaCha8 helpers
//! and transcendentals go through `libm`, so the same seed breeds the same genomes everywhere.

#![allow(
    clippy::cast_precision_loss,
    reason = "indices and counts are far below 2^52"
)]

use std::cmp::Ordering;

use sim_core::ConfigSources;
use sim_core::hash::stable_hash;
use sim_core::rng::{SimRng, below, seeded, unit_f64};

use super::def::{EvolveDef, SelectionMethod};
use crate::overrides::{LoadError, Override};
use crate::study::def::{Distribution, Kind, Parameter};
use crate::study::lhs::{round_to, unit_samples, value_at};
use crate::study::models::{parameter_baselines, parameter_overrides};

/// One genome: a value per gene of its personality.
#[derive(Debug, Clone, PartialEq)]
pub struct Genome(pub Vec<f64>);

/// Where a candidate came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The base config's settings (candidate 0 of generation 0).
    Base,
    /// A Latin hypercube sample (the rest of generation 0).
    Sample,
    /// Carried over unchanged: the previous generation's candidate with this index.
    Elite(u32),
    /// Bred from the previous generation's candidates with these indices (one, or two with
    /// crossover), then mutated.
    Child(Vec<u32>),
}

impl Origin {
    /// Its `origin` column: `base`, `sample`, `elite` or `child`.
    pub fn kind(&self) -> &'static str {
        match self {
            Origin::Base => "base",
            Origin::Sample => "sample",
            Origin::Elite(_) => "elite",
            Origin::Child(_) => "child",
        }
    }

    /// Its `parents` column: the previous generation's candidate indices, `+`-joined.
    pub fn parents(&self) -> String {
        match self {
            Origin::Base | Origin::Sample => String::new(),
            Origin::Elite(p) => p.to_string(),
            Origin::Child(ps) => ps.iter().map(u32::to_string).collect::<Vec<_>>().join("+"),
        }
    }

    /// Reads the `origin` and `parents` columns back.
    pub fn parse(kind: &str, parents: &str) -> Option<Self> {
        let list = || -> Option<Vec<u32>> { parents.split('+').map(|p| p.parse().ok()).collect() };
        match kind {
            "base" => Some(Origin::Base),
            "sample" => Some(Origin::Sample),
            "elite" => Some(Origin::Elite(parents.parse().ok()?)),
            "child" => Some(Origin::Child(list()?)),
            _ => None,
        }
    }
}

/// `v` made canonical for gene `p`: a category index in `0..values`, or a number clamped to
/// `[min, max]` and rounded to the gene's places (whole for an integer gene).
pub fn canon(p: &Parameter, v: f64) -> f64 {
    if p.is_categorical() {
        let top = p.values.len().saturating_sub(1) as f64;
        return v.floor().clamp(0.0, top);
    }
    let (lo, hi) = p.range();
    let r = match p.kind {
        Kind::Integer => v.round(),
        Kind::Decimal => round_to(v, p.places()),
    };
    // Rounding can step just outside a bound that is not on the grid; clamp after it.
    r.clamp(lo, hi)
}

/// A genome as text: `name=value` per gene, `;`-joined (the `genome` column).
pub fn label(genes: &[&Parameter], g: &Genome) -> String {
    genes
        .iter()
        .zip(&g.0)
        .map(|(p, v)| format!("{}={}", p.name, p.label(*v)))
        .collect::<Vec<_>>()
        .join(";")
}

/// Reads [`label`] back. Fails unless it names exactly these genes, in order.
pub fn parse(genes: &[&Parameter], text: &str) -> Result<Genome, String> {
    let parts: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.split(';').collect()
    };
    if parts.len() != genes.len() {
        return Err(format!("genome {text:?}: expected {} genes", genes.len()));
    }
    let mut values = Vec::with_capacity(genes.len());
    for (p, part) in genes.iter().zip(parts) {
        let (name, value) = part
            .split_once('=')
            .ok_or_else(|| format!("genome {text:?}: {part:?} is not name=value"))?;
        if name != p.name {
            return Err(format!(
                "genome {text:?}: expected gene {}, found {name}",
                p.name
            ));
        }
        let v = if p.is_categorical() {
            let i =
                p.values.iter().position(|c| c == value).ok_or_else(|| {
                    format!("genome {text:?}: {value:?} is not a value of {name}")
                })?;
            i as f64
        } else {
            value
                .parse::<f64>()
                .map_err(|e| format!("genome {text:?}: {name}: {e}"))?
        };
        values.push(canon(p, v));
    }
    Ok(Genome(values))
}

/// The `--set` overrides that put genome `g` into a config, gene by gene.
pub fn overrides(
    genes: &[&Parameter],
    base: &ConfigSources,
    g: &Genome,
) -> Result<Vec<Override>, LoadError> {
    let mut out = Vec::new();
    for (p, v) in genes.iter().zip(&g.0) {
        out.extend(parameter_overrides(p, base, *v)?);
    }
    Ok(out)
}

/// The base config's settings as a genome: each gene's value in `base` (1 for a scale gene,
/// the base category's index), made canonical. A gene the base config holds no single number
/// for (or a category it does not list) starts at the middle of its range (its first value).
pub fn base_genome(genes: &[&Parameter], base: &ConfigSources) -> Genome {
    let owned: Vec<Parameter> = genes.iter().map(|p| (*p).clone()).collect();
    Genome(
        genes
            .iter()
            .zip(parameter_baselines(&owned, base))
            .map(|(p, v)| {
                let (lo, hi) = p.range();
                canon(p, v.unwrap_or(f64::midpoint(lo, hi)))
            })
            .collect(),
    )
}

/// A seed for one personality's stream of draws: the run seed mixed with a purpose, the
/// personality id and the generation, so streams never overlap.
pub fn stream_seed(seed: u64, purpose: &str, personality: &str, generation: u32) -> u64 {
    seed ^ stable_hash(&(purpose, personality, generation))
}

/// Generation 0 of one personality: the base genome, then `population − 1` Latin hypercube
/// samples over the genes (one per stratum of every gene).
pub fn first_population(
    genes: &[&Parameter],
    base: &ConfigSources,
    population: u32,
    seed: u64,
    personality: &str,
) -> Vec<(Genome, Origin)> {
    let mut out = vec![(base_genome(genes, base), Origin::Base)];
    let n = usize::try_from(population.saturating_sub(1)).unwrap_or(0);
    let units = unit_samples(genes.len(), n, stream_seed(seed, "first", personality, 0));
    for u in units {
        let values = genes
            .iter()
            .zip(&u)
            .map(|(p, x)| {
                if p.is_categorical() {
                    canon(p, *x * p.values.len() as f64)
                } else {
                    canon(p, value_at(p, *x))
                }
            })
            .collect();
        out.push((Genome(values), Origin::Sample));
    }
    out
}

/// One standard normal draw (Box–Muller, through `libm`): `√(−2 ln(1 − u₁)) · cos(2π u₂)`.
pub fn normal(rng: &mut SimRng) -> f64 {
    let (u1, u2) = (unit_f64(rng), unit_f64(rng));
    (-2.0 * libm::log(1.0 - u1)).sqrt() * libm::cos(std::f64::consts::TAU * u2)
}

/// Gene `p` at `v` after one Gaussian step `z` (a standard normal draw) of `sigma` × its range:
///
/// - uniform: `v + z · σ · (max − min)`;
/// - log-uniform: `exp(ln v + z · σ · (ln max − ln min))`;
/// - an integer gene moves at least one whole step in the direction of `z`;
///
/// then made canonical, so the result is always within `[min, max]`.
pub fn mutate_number(p: &Parameter, v: f64, z: f64, sigma: f64) -> f64 {
    let (lo, hi) = p.range();
    let raw = match p.distribution {
        Distribution::Uniform => v + z * sigma * (hi - lo),
        Distribution::LogUniform => {
            let span = libm::log(hi) - libm::log(lo);
            libm::exp(libm::log(v.max(lo)) + z * sigma * span)
        }
    };
    let mut out = canon(p, raw);
    if p.kind == Kind::Integer && out.total_cmp(&v) == Ordering::Equal && z != 0.0 {
        out = canon(p, v + z.signum());
    }
    out
}

/// Mutates gene `j` of `g` in place, drawing from `rng`. A categorical gene takes a different
/// value, picked uniformly; a numeric one takes a Gaussian step ([`mutate_number`]).
fn mutate_gene(p: &Parameter, g: &mut Genome, j: usize, sigma: f64, rng: &mut SimRng) {
    if p.is_categorical() {
        let k = p.values.len() as u64;
        if k > 1 {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a canonical category index is a small whole number"
            )]
            let cur = g.0[j] as u64;
            let mut pick = below(rng, k - 1);
            if pick >= cur {
                pick += 1;
            }
            g.0[j] = pick as f64;
        }
    } else {
        g.0[j] = mutate_number(p, g.0[j], normal(rng), sigma);
    }
}

/// Mutation as the definition asks: each gene with chance `rate`; if none came up, one gene
/// picked uniformly. One `unit_f64` draw per gene first, so the stream never depends on values.
pub fn mutate(genes: &[&Parameter], g: &mut Genome, def: &EvolveDef, rng: &mut SimRng) {
    let picked: Vec<bool> = genes
        .iter()
        .map(|_| unit_f64(rng) < def.mutation.rate)
        .collect();
    let mut any = false;
    for (j, p) in genes.iter().enumerate() {
        if picked[j] {
            mutate_gene(p, g, j, def.mutation.sigma, rng);
            any = true;
        }
    }
    if !any && !genes.is_empty() {
        let j = usize::try_from(below(rng, genes.len() as u64)).unwrap_or(0);
        mutate_gene(genes[j], g, j, def.mutation.sigma, rng);
    }
}

/// Uniform crossover: each gene from `a` or `b` with even odds.
pub fn crossover(a: &Genome, b: &Genome, rng: &mut SimRng) -> Genome {
    Genome(
        a.0.iter()
            .zip(&b.0)
            .map(|(x, y)| if unit_f64(rng) < 0.5 { *x } else { *y })
            .collect(),
    )
}

/// Picks a parent from `n` candidates ranked best first (position 0 is the best): the best of
/// `tournament_size` uniform draws, or a linear-rank draw. Returns a position in the ranking.
pub fn select(n: usize, def: &EvolveDef, rng: &mut SimRng) -> usize {
    let n64 = n.max(1) as u64;
    match def.selection.method {
        SelectionMethod::Tournament => (0..def.selection.tournament_size.max(1))
            .map(|_| usize::try_from(below(rng, n64)).unwrap_or(0))
            .min()
            .unwrap_or(0),
        SelectionMethod::Rank => {
            // Weights n, n−1, …, 1: total n(n+1)/2.
            let mut r = below(rng, n64 * (n64 + 1) / 2);
            for i in 0..n64 {
                let w = n64 - i;
                if r < w {
                    return usize::try_from(i).unwrap_or(0);
                }
                r -= w;
            }
            n.saturating_sub(1)
        }
    }
}

/// The next generation of one personality from the previous one's **valid** candidates ranked
/// best first (`ranked[i] = (previous candidate index, genome)`): the top `elitism` unchanged,
/// then children (a selected parent, crossed with a second one with chance `crossover.rate`,
/// then mutated) until the population is full. With no valid candidate at all, every child is
/// bred from `fallback` (the base genome). Draws come from one stream per personality and
/// generation, in child order.
pub fn breed(
    genes: &[&Parameter],
    ranked: &[(u32, Genome)],
    fallback: &Genome,
    def: &EvolveDef,
    personality: &str,
    generation: u32,
) -> Vec<(Genome, Origin)> {
    let mut rng = seeded(stream_seed(def.seed, "breed", personality, generation));
    let pop = usize::try_from(def.population).unwrap_or(0);
    let elites = usize::try_from(def.elitism).unwrap_or(0).min(ranked.len());
    let mut out: Vec<(Genome, Origin)> = ranked[..elites]
        .iter()
        .map(|(i, g)| (g.clone(), Origin::Elite(*i)))
        .collect();
    let fallback_pool = [(0, fallback.clone())];
    let pool: &[(u32, Genome)] = if ranked.is_empty() {
        &fallback_pool
    } else {
        ranked
    };
    while out.len() < pop {
        let a = &pool[select(pool.len(), def, &mut rng)];
        let mut parents = vec![a.0];
        let mut child = if unit_f64(&mut rng) < def.crossover.rate {
            let b = &pool[select(pool.len(), def, &mut rng)];
            parents.push(b.0);
            crossover(&a.1, &b.1, &mut rng)
        } else {
            a.1.clone()
        };
        mutate(genes, &mut child, def, &mut rng);
        if ranked.is_empty() {
            parents.clear();
        }
        out.push((child, Origin::Child(parents)));
    }
    out
}

/// Total order on genome values for tests and dedup: by bits.
pub fn same(a: &Genome, b: &Genome) -> bool {
    a.0.len() == b.0.len()
        && a.0
            .iter()
            .zip(&b.0)
            .all(|(x, y)| x.total_cmp(y) == Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn gene(min: f64, max: f64, dist: Distribution, kind: Kind) -> Parameter {
        Parameter::set("g", "ai.toff.aggression", min, max, dist, kind)
    }

    #[test]
    fn labels_round_trip_exactly() {
        let a = gene(0.1, 0.9, Distribution::Uniform, Kind::Decimal);
        let b = Parameter::set(
            "b",
            "ai.toff.cash_buffer",
            100.0,
            5000.0,
            Distribution::LogUniform,
            Kind::Integer,
        );
        let c = Parameter::categorical("c", "ai.toff.fit_out", &["low", "medium", "high"]);
        let genes = [&a, &b, &c];
        let g = Genome(vec![canon(&a, 0.333_333), canon(&b, 777.4), 2.0]);
        let text = label(&genes, &g);
        assert_eq!(text, "g=0.33;b=777;c=high");
        assert!(same(&parse(&genes, &text).unwrap(), &g));
        assert!(parse(&genes, "g=0.3;b=1").is_err());
        assert!(parse(&genes, "x=0.3;b=1;c=low").is_err());
    }

    #[test]
    fn origins_round_trip() {
        for o in [
            Origin::Base,
            Origin::Sample,
            Origin::Elite(3),
            Origin::Child(vec![1]),
            Origin::Child(vec![4, 0]),
        ] {
            assert_eq!(Origin::parse(o.kind(), &o.parents()), Some(o));
        }
    }

    #[test]
    fn an_integer_gene_always_moves_a_whole_step() {
        let p = gene(1.0, 100.0, Distribution::Uniform, Kind::Integer);
        assert!((mutate_number(&p, 50.0, 0.01, 0.01) - 51.0).abs() < 1e-12);
        assert!((mutate_number(&p, 50.0, -0.01, 0.01) - 49.0).abs() < 1e-12);
        assert!((mutate_number(&p, 100.0, 0.01, 0.01) - 100.0).abs() < 1e-12);
    }

    #[test]
    fn tournament_of_the_whole_population_tends_to_the_best() {
        let def = EvolveDef {
            population: 8,
            ..EvolveDef::default()
        };
        let mut rng = seeded(1);
        let picks: Vec<usize> = (0..200).map(|_| select(8, &def, &mut rng)).collect();
        let best = picks.iter().filter(|p| **p == 0).count();
        let worst = picks.iter().filter(|p| **p == 7).count();
        assert!(best > worst, "{best} vs {worst}");
    }

    proptest! {
        #[test]
        fn mutation_stays_in_range(
            min in -1000i32..1000, span in 0i32..1000, start in 0.0f64..1.0,
            z in -50.0f64..50.0, sigma in 0.01f64..1.0, int in any::<bool>(), log in any::<bool>(),
        ) {
            let (lo, hi) = (f64::from(min), f64::from(min + span));
            let (lo, hi) = if log { (lo.abs() + 1.0, lo.abs() + 1.0 + (hi - lo)) } else { (lo, hi) };
            let kind = if int { Kind::Integer } else { Kind::Decimal };
            let dist = if log { Distribution::LogUniform } else { Distribution::Uniform };
            let p = gene(lo, hi, dist, kind);
            let v = canon(&p, lo + start * (hi - lo));
            let m = mutate_number(&p, v, z, sigma);
            prop_assert!((lo..=hi).contains(&m), "{m} outside [{lo}, {hi}]");
            prop_assert!(m.total_cmp(&canon(&p, m)) == Ordering::Equal, "{m} not canonical");
        }

        #[test]
        fn canonical_values_survive_their_label(v in -1.0e6f64..1.0e6, places in 0u32..6) {
            let mut p = gene(-1.0e6, 1.0e6, Distribution::Uniform, Kind::Decimal);
            p.decimals = Some(places);
            let c = canon(&p, v);
            let genes = [&p];
            let back = parse(&genes, &label(&genes, &Genome(vec![c]))).unwrap();
            prop_assert_eq!(back.0[0].to_bits(), c.to_bits());
        }
    }
}
