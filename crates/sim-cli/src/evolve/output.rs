//! Evolution outputs: `generations.csv` (one row per candidate), each generation's champions
//! (`champions/<gen>/ai.yaml` and `lineup.json`) and `summary.json`. The columns are a contract
//! documented in docs/evolve.md: add columns at the end, never rename or reorder one.
//!
//! A row holds everything a resumed run needs to breed the next generation exactly as an
//! uninterrupted run would: the genome (canonical values, so its text reads back bit for bit)
//! and the evaluation as integer counts.

use serde_json::{Value as Json, json};

use super::fitness::{Evaluation, FitnessWeights};
use super::genome::{self, Genome, Origin};
use crate::study::Parameter;
use crate::study::output::{csv_field, split_csv_line};

/// The columns of `generations.csv`, in order.
pub const COLUMNS: [&str; 19] = [
    "gen",
    "personality",
    "candidate",
    "origin",
    "parents",
    "genome_id",
    "status",
    "error",
    "genome",
    "games",
    "wins",
    "win_share",
    "bankruptcies",
    "bankrupt_rate",
    "nw_vs_idle_median",
    "fitness",
    "rank",
    "champion",
    "games_digest",
];

/// One evaluated candidate: a row of `generations.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Generation, from 0.
    pub generation: u32,
    /// Personality id.
    pub personality: String,
    /// Index in its personality's population, from 0.
    pub index: u32,
    /// Where it came from.
    pub origin: Origin,
    /// Its genes' values.
    pub genome: Genome,
    /// The brain version its seat is recorded under (`replay::record::brain_version`): the hash
    /// of the personality's settings with this genome in place. Empty if the config is invalid.
    pub genome_id: String,
    /// Its evaluation, or why its config is invalid (the loader's message, naming the key).
    pub result: Result<Evaluation, String>,
    /// Stable hash of its games' final state hashes, in game order (16 hex digits; empty if
    /// invalid): two runs that played the same games agree on it.
    pub games_digest: String,
    /// Rank among its personality's valid candidates this generation (1 = best).
    pub rank: Option<u32>,
    /// True for the personality's champion of this generation (rank 1).
    pub champion: bool,
}

fn f6(v: f64) -> String {
    format!("{v:.6}")
}

impl Candidate {
    /// Its row, in [`COLUMNS`] order.
    pub fn row(&self, genes: &[&Parameter], weights: &FitnessWeights) -> Vec<String> {
        let mut row = vec![
            self.generation.to_string(),
            self.personality.clone(),
            self.index.to_string(),
            self.origin.kind().to_owned(),
            self.origin.parents(),
            self.genome_id.clone(),
        ];
        match &self.result {
            Ok(_) => row.extend(["ok".to_owned(), String::new()]),
            Err(e) => row.extend(["invalid".to_owned(), csv_field(e)]),
        }
        row.push(csv_field(&genome::label(genes, &self.genome)));
        match &self.result {
            Ok(e) => row.extend([
                e.games.to_string(),
                e.wins.to_string(),
                f6(e.win_share()),
                e.bankruptcies.to_string(),
                f6(e.bankrupt_rate()),
                e.nw_vs_idle.map(|v| v.to_string()).unwrap_or_default(),
                f6(e.fitness(weights)),
            ]),
            Err(_) => row.extend(std::iter::repeat_n(String::new(), 7)),
        }
        row.extend([
            self.rank.map(|r| r.to_string()).unwrap_or_default(),
            self.champion.to_string(),
            self.games_digest.clone(),
        ]);
        row
    }

    /// Reads a row back; `genes_of` gives a personality's genes. `None` for a malformed row
    /// (a line cut off mid-write).
    pub fn parse<'a>(
        line: &str,
        genes_of: impl Fn(&str) -> Option<Vec<&'a Parameter>>,
    ) -> Option<Candidate> {
        let f = split_csv_line(line);
        if f.len() != COLUMNS.len() {
            return None;
        }
        let genes = genes_of(&f[1])?;
        let num = |i: usize| f[i].parse::<u64>().ok();
        let result = match f[6].as_str() {
            "ok" => Ok(Evaluation {
                games: num(9)?,
                wins: num(10)?,
                bankruptcies: num(12)?,
                nw_vs_idle: if f[14].is_empty() {
                    None
                } else {
                    Some(f[14].parse().ok()?)
                },
            }),
            "invalid" => Err(f[7].clone()),
            _ => return None,
        };
        Some(Candidate {
            generation: f[0].parse().ok()?,
            personality: f[1].clone(),
            index: f[2].parse().ok()?,
            origin: Origin::parse(&f[3], &f[4])?,
            genome_id: f[5].clone(),
            genome: genome::parse(&genes, &f[8]).ok()?,
            result,
            rank: if f[16].is_empty() {
                None
            } else {
                Some(f[16].parse().ok()?)
            },
            champion: f[17] == "true",
            games_digest: f[18].clone(),
        })
    }
}

/// `generations.csv` text: the header and these rows.
pub fn csv(rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut text = COLUMNS.join(",") + "\n";
    for r in rows {
        text.push_str(&r.join(","));
        text.push('\n');
    }
    text
}

/// One personality's fitness curve over the finished generations, for `summary.json`.
pub fn curve(
    candidates: &[Candidate],
    personality: &str,
    generations: u32,
    weights: &FitnessWeights,
) -> Json {
    let mut best = Vec::new();
    let mut mean = Vec::new();
    let mut champion = Vec::new();
    let mut champion_win_share = Vec::new();
    let mut invalid = Vec::new();
    for g in 0..generations {
        let mine: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| c.generation == g && c.personality == personality)
            .collect();
        let fit: Vec<f64> = mine
            .iter()
            .filter_map(|c| c.result.as_ref().ok().map(|e| e.fitness(weights)))
            .collect();
        #[allow(clippy::cast_precision_loss, reason = "populations are small")]
        let avg = if fit.is_empty() {
            Json::Null
        } else {
            json!(fit.iter().sum::<f64>() / fit.len() as f64)
        };
        best.push(
            fit.iter()
                .copied()
                .reduce(f64::max)
                .map_or(Json::Null, |v| json!(v)),
        );
        mean.push(avg);
        let champ = mine.iter().find(|c| c.champion);
        champion.push(champ.map_or(Json::Null, |c| json!(c.genome_id)));
        champion_win_share.push(
            champ
                .and_then(|c| c.result.as_ref().ok())
                .map_or(Json::Null, |e| json!(e.win_share())),
        );
        invalid.push(mine.iter().filter(|c| c.result.is_err()).count());
    }
    json!({
        "best_fitness": best,
        "mean_fitness": mean,
        "champion": champion,
        "champion_win_share": champion_win_share,
        "invalid": invalid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::{Distribution, Kind};

    #[test]
    fn a_row_reads_back() {
        let p = Parameter::set(
            "agg",
            "ai.toff.aggression",
            0.1,
            0.9,
            Distribution::Uniform,
            Kind::Decimal,
        );
        let genes = vec![&p];
        let c = Candidate {
            generation: 2,
            personality: "toff".into(),
            index: 5,
            origin: Origin::Child(vec![1, 3]),
            genome: Genome(vec![0.42]),
            genome_id: "00ff00ff00ff00ff".into(),
            result: Ok(Evaluation {
                games: 12,
                wins: 5,
                bankruptcies: 1,
                nw_vs_idle: Some(-1234),
            }),
            games_digest: "0123456789abcdef".into(),
            rank: Some(1),
            champion: true,
        };
        let line = c.row(&genes, &FitnessWeights::default()).join(",");
        assert_eq!(
            Candidate::parse(&line, |_| Some(genes.clone())),
            Some(c.clone())
        );
        let bad = Candidate {
            result: Err("ai.yaml: /toff/cut_at: must be below raise_at, really".into()),
            rank: None,
            champion: false,
            genome_id: String::new(),
            games_digest: String::new(),
            ..c
        };
        let line = bad.row(&genes, &FitnessWeights::default()).join(",");
        assert_eq!(Candidate::parse(&line, |_| Some(genes.clone())), Some(bad));
        assert_eq!(Candidate::parse("2,toff,5", |_| Some(genes.clone())), None);
    }
}
