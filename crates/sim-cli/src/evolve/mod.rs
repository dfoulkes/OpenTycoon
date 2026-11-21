//! `sim-cli evolve`: evolving each AI personality's settings over generations of games
//! (docs/evolve.md).
//!
//! Per personality, a population of genomes (values for its `ai.yaml` tunables) is evaluated
//! by seating each candidate against the other personalities' current champions and an idle
//! seat, seats rotated, over whole games; the fittest are kept and bred. Like the rest of the
//! harness it implements no game rule and never changes a brain's code: a genome is a config
//! overlay validated exactly like `--set`, and the brains still act only through public
//! commands.
//!
//! - [`def`]: the definition file.
//! - [`genome`]: genomes, the first population, mutation, crossover and selection.
//! - [`fitness`]: what a candidate's games are worth, and the ranking.
//! - [`output`]: `generations.csv` rows and the fitness curves.
//! - [`run`]: playing generations, in parallel, resumable, optionally recorded.

#![allow(
    dead_code,
    unused_imports,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    reason = "red commit: the API is stubbed; the green commit implements it"
)]

pub mod def;
pub mod fitness;
pub mod genome;
pub mod output;
pub mod run;

pub use def::{EvolveDef, EvolveError, SelectionMethod};
pub use fitness::{Evaluation, compare, evaluate, rank};
pub use genome::{Genome, Origin};
pub use output::Candidate;
pub use run::{
    EvolveOptions, EvolveRecording, EvolveReport, base_sources, run_evolution,
    run_evolution_recorded,
};
