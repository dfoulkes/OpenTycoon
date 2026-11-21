//! Playing one generation: every candidate of every evolving personality against the previous
//! generation's champions, in one parallel batch; then the ranking, the new champions and their
//! lineup; then `champions/<gen>/`.
//!
//! Games are collected in plan order, so the results never depend on the thread count. When
//! recording, replays are written after the games finish, candidate by candidate, so the stored
//! objects are byte-identical whatever the thread count.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;
use serde_json::{Value as Json, json};
use sim_core::Config;
use sim_core::config::ConfigFile;

use super::def::EvolveError;
use super::evolution::Evolution;
use super::fitness::{Evaluation, entry_of, evaluate, rank};
use super::genome::{self, Genome, Origin};
use super::output::Candidate;
use super::run::{EvolveRecording, digest};
use crate::overrides;
use crate::replay::format::Replay;
use crate::replay::record::{RecordContext, brain_version};
use crate::replay::snapshot::snapshot;
use crate::study::run::{play_one, play_one_recorded, write};
use crate::study::seats::GamePlan;
use crate::study::{GameOutcome, Roster};

/// A finished generation.
pub(super) struct Generation {
    /// Its candidates, by personality (roster order) then index.
    pub(super) candidates: Vec<Candidate>,
    /// Each personality's champion after it.
    pub(super) champions: BTreeMap<String, Genome>,
    /// The champions' lineup games, in plan order.
    pub(super) lineup: Vec<GameOutcome>,
    /// Games played.
    pub(super) games: u64,
}

/// A config ready to play: the validated config, the evolving seat's genome id and the config
/// snapshot's hash.
struct Setup {
    config: Config,
    genome_id: String,
    config_hash: String,
}

/// One candidate to evaluate.
struct Slot {
    personality: String,
    index: u32,
    origin: Origin,
    genome: Genome,
    setup: Result<Setup, String>,
}

/// One game to play: a config, a planned seating and seed, and its recorded game id.
struct Job<'s> {
    slot: usize,
    config: &'s Config,
    config_hash: &'s str,
    plan: &'s GamePlan,
    game_id: String,
}

/// Progress lines for a large generation.
struct Progress {
    generation: u32,
    total: u64,
    done: AtomicU64,
    on: bool,
}

impl Progress {
    fn tick(&self) {
        let n = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        // Only a large generation gets lines of its own; a small one is quick.
        if self.on && self.total >= 1_000 && n % (self.total / 10).max(1) == 0 && n < self.total {
            eprintln!(
                "evolve: generation {}: {n}/{} games",
                self.generation, self.total
            );
        }
    }
}

/// Plays `jobs` on `pool`, recording them when `ctx` is given; results in job order.
fn play(
    pool: &rayon::ThreadPool,
    roster: &Roster,
    jobs: &[Job<'_>],
    ctx: Option<&RecordContext>,
    progress: &Progress,
) -> Vec<(GameOutcome, Option<Replay>)> {
    pool.install(|| {
        jobs.par_iter()
            .map(|job| {
                let out = match ctx {
                    Some(ctx) => {
                        let ctx = RecordContext {
                            config_hash: job.config_hash.to_owned(),
                            ..ctx.clone()
                        };
                        let (o, _, r) =
                            play_one_recorded(job.config, roster, job.plan, &ctx, &job.game_id);
                        (o, Some(r))
                    }
                    None => (play_one(job.config, roster, job.plan).0, None),
                };
                progress.tick();
                out
            })
            .collect()
    })
}

fn record_err(e: impl std::fmt::Display) -> EvolveError {
    EvolveError::Record(e.to_string())
}

impl Evolution<'_> {
    /// Builds and validates every candidate's config (its genome among `champs`), storing each
    /// config snapshot once when recording.
    fn slots(
        &self,
        pops: &BTreeMap<String, Vec<(Genome, Origin)>>,
        champs: &BTreeMap<String, Genome>,
        mut rec: Option<&mut EvolveRecording<'_, '_>>,
    ) -> Result<Vec<Slot>, EvolveError> {
        let mut slots = Vec::new();
        for p in &self.personalities {
            for (i, (genome, origin)) in pops[p].iter().enumerate() {
                let mut genomes = champs.clone();
                genomes.insert(p.clone(), genome.clone());
                let setup = match self.lineup_config(&genomes) {
                    Err(e) => Err(e),
                    Ok((files, config)) => {
                        let snap = snapshot(&files);
                        if let Some(rec) = rec.as_deref_mut() {
                            rec.writer.put_config(&snap).map_err(record_err)?;
                        }
                        Ok(Setup {
                            genome_id: brain_version(&config, p),
                            config,
                            config_hash: snap.hash,
                        })
                    }
                };
                slots.push(Slot {
                    personality: p.clone(),
                    index: u32::try_from(i).unwrap_or(u32::MAX),
                    origin: origin.clone(),
                    genome: genome.clone(),
                    setup,
                });
            }
        }
        Ok(slots)
    }

    /// Each slot's evaluation, ranked within its personality; and the new champions (the
    /// previous ones where a personality had no valid candidate).
    fn score(
        &self,
        g: u32,
        slots: &[Slot],
        games: &[Vec<GameOutcome>],
        champs: &BTreeMap<String, Genome>,
    ) -> (Vec<Candidate>, BTreeMap<String, Genome>) {
        let mut candidates: Vec<Candidate> = slots
            .iter()
            .zip(games)
            .map(|(s, games)| {
                let (result, genome_id, games_digest) = match &s.setup {
                    Err(e) => (Err(e.clone()), String::new(), String::new()),
                    Ok(setup) => {
                        let entry = entry_of(&self.roster, &s.personality).unwrap_or(0);
                        let hashes: Vec<u64> = games.iter().map(|o| o.state_hash).collect();
                        (
                            Ok(evaluate(&self.roster, entry, games)),
                            setup.genome_id.clone(),
                            digest(&hashes),
                        )
                    }
                };
                Candidate {
                    generation: g,
                    personality: s.personality.clone(),
                    index: s.index,
                    origin: s.origin.clone(),
                    genome: s.genome.clone(),
                    genome_id,
                    result,
                    games_digest,
                    rank: None,
                    champion: false,
                }
            })
            .collect();
        let mut new_champs = champs.clone();
        for p in &self.personalities {
            let valid: Vec<(u32, Evaluation)> = candidates
                .iter()
                .filter(|c| &c.personality == p)
                .filter_map(|c| c.result.as_ref().ok().map(|e| (c.index, *e)))
                .collect();
            for (r, idx) in rank(&valid, self.penalty()).into_iter().enumerate() {
                let Some(c) = candidates
                    .iter_mut()
                    .find(|c| &c.personality == p && c.index == idx)
                else {
                    continue;
                };
                c.rank = Some(u32::try_from(r + 1).unwrap_or(u32::MAX));
                c.champion = r == 0;
                if r == 0 {
                    new_champs.insert(p.clone(), c.genome.clone());
                }
            }
        }
        (candidates, new_champs)
    }

    /// Plays generation `g`: every candidate of `pops` against `champs`, then the new
    /// champions' lineup on the same seeds.
    pub(super) fn run_generation(
        &self,
        pool: &rayon::ThreadPool,
        g: u32,
        pops: &BTreeMap<String, Vec<(Genome, Origin)>>,
        champs: &BTreeMap<String, Genome>,
        mut rec: Option<&mut EvolveRecording<'_, '_>>,
    ) -> Result<Generation, EvolveError> {
        let slots = self.slots(pops, champs, rec.as_deref_mut())?;
        let plans = self.plans(g);
        let jobs: Vec<Job<'_>> = slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.setup.as_ref().ok().map(|setup| (i, s, setup)))
            .flat_map(|(i, s, setup)| {
                plans.iter().map(move |plan| Job {
                    slot: i,
                    config: &setup.config,
                    config_hash: &setup.config_hash,
                    plan,
                    game_id: format!(
                        "g{g:04}-{}-c{:03}-n{:05}",
                        s.personality, s.index, plan.index
                    ),
                })
            })
            .collect();
        let progress = Progress {
            generation: g,
            total: (jobs.len() + plans.len()) as u64,
            done: AtomicU64::new(0),
            on: self.opts.progress,
        };
        let ctx = rec.as_ref().map(|r| RecordContext {
            preset: self.preset.clone(),
            ..r.ctx.clone()
        });
        let played = play(pool, &self.roster, &jobs, ctx.as_ref(), &progress);
        let mut games: Vec<Vec<GameOutcome>> = vec![Vec::new(); slots.len()];
        let mut replays: Vec<Vec<Replay>> = vec![Vec::new(); slots.len()];
        for (job, (o, r)) in jobs.iter().zip(played) {
            games[job.slot].push(o);
            replays[job.slot].extend(r);
        }
        if let Some(rec) = rec.as_deref_mut() {
            for (s, r) in slots.iter().zip(&replays).filter(|(_, r)| !r.is_empty()) {
                let part = format!("part-g{g:04}-{}-c{:03}", s.personality, s.index);
                rec.writer
                    .write_part(&part, r, "evolve", None)
                    .map_err(record_err)?;
            }
        }
        let (candidates, champions) = self.score(g, &slots, &games, champs);
        // The champions' lineup, on the same seeds.
        let (files, config) = self.lineup_config(&champions).map_err(|e| {
            EvolveError::BaseConfig(format!(
                "generation {g}: the champion lineup is invalid: {e}"
            ))
        })?;
        let snap = snapshot(&files);
        if let Some(rec) = rec.as_deref_mut() {
            rec.writer.put_config(&snap).map_err(record_err)?;
        }
        let lineup_jobs: Vec<Job<'_>> = plans
            .iter()
            .map(|plan| Job {
                slot: 0,
                config: &config,
                config_hash: &snap.hash,
                plan,
                game_id: format!("g{g:04}-lineup-n{:05}", plan.index),
            })
            .collect();
        let lineup = play(pool, &self.roster, &lineup_jobs, ctx.as_ref(), &progress);
        if let Some(rec) = rec {
            let r: Vec<Replay> = lineup.iter().filter_map(|(_, r)| r.clone()).collect();
            rec.writer
                .write_part(&format!("part-g{g:04}-lineup"), &r, "evolve", None)
                .map_err(record_err)?;
        }
        Ok(Generation {
            candidates,
            champions,
            lineup: lineup.into_iter().map(|(o, _)| o).collect(),
            games: progress.total,
        })
    }

    /// Writes `champions/<g>/ai.yaml` (the whole `ai.yaml` with every champion in place,
    /// ready to copy over `config/`) and then `lineup.json`, which marks the generation done.
    pub(super) fn write_champions(&self, g: u32, done: &Generation) -> Result<(), EvolveError> {
        let dir = self.champion_dir(g);
        std::fs::create_dir_all(&dir).map_err(|e| EvolveError::io(&dir, e))?;
        let ovs = self
            .lineup_overrides(&done.champions)
            .map_err(EvolveError::BaseConfig)?;
        let mut edited = self.base.clone();
        for ov in &ovs {
            overrides::apply(&mut edited, ov)
                .map_err(|e| EvolveError::BaseConfig(e.to_string()))?;
        }
        let ai = edited.get(ConfigFile::Ai.file_name()).unwrap_or_default();
        write(&dir.join("ai.yaml"), ai)?;
        write(
            &dir.join("lineup.json"),
            &(serde_json::to_string_pretty(&self.lineup_json(g, done)).unwrap_or_default() + "\n"),
        )?;
        Ok(())
    }

    fn lineup_json(&self, g: u32, done: &Generation) -> Json {
        let names = self.roster.names();
        let entries: serde_json::Map<String, Json> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let e = evaluate(&self.roster, i, &done.lineup);
                (
                    name.clone(),
                    json!({
                        "wins": e.wins,
                        "win_share": e.win_share(),
                        "bankruptcies": e.bankruptcies,
                        "bankrupt_rate": e.bankrupt_rate(),
                        "nw_vs_idle_median": e.nw_vs_idle,
                    }),
                )
            })
            .collect();
        let champions: serde_json::Map<String, Json> = self
            .personalities
            .iter()
            .map(|p| {
                let row = done
                    .candidates
                    .iter()
                    .find(|c| &c.personality == p && c.champion);
                let fitness = row
                    .and_then(|c| c.result.as_ref().ok())
                    .map(|e| e.fitness(self.penalty()));
                (
                    p.clone(),
                    json!({
                        "candidate": row.map(|c| c.index),
                        "genome_id": row.map(|c| c.genome_id.clone()),
                        "genome": genome::label(&self.genes[p], &done.champions[p]),
                        "fitness": fitness,
                    }),
                )
            })
            .collect();
        let hashes: Vec<u64> = done.lineup.iter().map(|o| o.state_hash).collect();
        let games: Vec<Json> = done
            .lineup
            .iter()
            .map(|o| {
                json!({
                    "game": o.index,
                    "seed": o.seed,
                    "rotation": o.rotation,
                    "winner": o.winner.map(|w| names[w].clone()),
                    "state_hash": format!("{:016x}", o.state_hash),
                })
            })
            .collect();
        json!({
            "generation": g,
            "preset": self.preset,
            "roster": names,
            "champions": champions,
            "entries": entries,
            "no_winner": done.lineup.iter().filter(|o| o.winner.is_none()).count(),
            "games_digest": digest(&hashes),
            "games": games,
        })
    }
}
