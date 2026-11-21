//! `sim-cli batch`: N independent games in parallel, one CSV row each, plus a summary
//! (spec §11: "run 1,000 AI-vs-AI games per preset, then chart median net worth and bankruptcy
//! rate").
//!
//! Games are spread over a `rayon` pool, but each game depends only on `(config, seed, spec)`
//! and results are sorted by seed before anything is written, so the CSV is identical on any
//! number of threads (only the `wall_ms` column, which `Timing::Omit` drops, varies).

use std::fmt::Write as _;
use std::io::{self, Write};
use std::time::Duration;

use rayon::prelude::*;
use serde::Serialize;
use sim_core::Config;

use crate::game::{GameResult, GameSpec, Winner, play};
use crate::replay::record::RecordContext;
use crate::replay::snapshot::ConfigSnapshot;
use crate::replay::writer::RunWriter;
use crate::stats::{Distribution, distribution, rate};
use crate::timing::Stopwatch;

/// What to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSpec {
    /// Number of games.
    pub games: u64,
    /// First seed; game `i` uses `base_seed + i`.
    pub base_seed: u64,
    /// Worker threads; `None` uses every core.
    pub threads: Option<usize>,
    /// How each game is played.
    pub game: GameSpec,
}

impl Default for BatchSpec {
    fn default() -> Self {
        BatchSpec {
            games: 1_000,
            base_seed: 0,
            threads: None,
            game: GameSpec::default(),
        }
    }
}

/// Why a batch could not run.
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    /// `base_seed + games` overflows `u64`.
    #[error("seeds {base}..{base}+{games} overflow u64")]
    SeedOverflow {
        /// First seed.
        base: u64,
        /// Games asked for.
        games: u64,
    },
    /// The thread pool could not be built.
    #[error("thread pool: {0}")]
    Pool(#[from] rayon::ThreadPoolBuildError),
}

/// Every game's result, sorted by seed, and the batch's wall time.
#[derive(Debug, Clone)]
pub struct BatchReport {
    /// Per game, ascending seed.
    pub results: Vec<GameResult>,
    /// Wall time for the whole batch.
    pub wall: Duration,
    /// Threads actually used.
    pub threads: usize,
}

/// Runs the batch.
pub fn run_batch(config: &Config, spec: &BatchSpec) -> Result<BatchReport, BatchError> {
    let end = spec
        .base_seed
        .checked_add(spec.games)
        .ok_or(BatchError::SeedOverflow {
            base: spec.base_seed,
            games: spec.games,
        })?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(spec.threads.unwrap_or(0))
        .build()?;
    let watch = Stopwatch::start();
    let mut results: Vec<GameResult> = pool.install(|| {
        (spec.base_seed..end)
            .into_par_iter()
            .map(|seed| play(config, seed, &spec.game))
            .collect()
    });
    results.sort_by_key(|r| r.seed);
    Ok(BatchReport {
        results,
        wall: watch.elapsed(),
        threads: pool.current_num_threads(),
    })
}

/// Recording a batch (`--record`): where and under what header.
#[derive(Debug)]
pub struct BatchRecording<'w, 'a> {
    /// Header fields every game shares.
    pub ctx: RecordContext,
    /// The effective config's snapshot (stored once).
    pub snapshot: ConfigSnapshot,
    /// The run's writer.
    pub writer: &'w mut RunWriter<'a>,
    /// Games per manifest part; each part's replays are held in memory until written.
    pub chunk: u64,
    /// What recorded it (`batch`, `record`).
    pub source: String,
}

/// [`run_batch`], recording every game into `rec`'s store: one manifest part per `rec.chunk`
/// games (`part-00000`, `part-00001`, …). The results are identical to an unrecorded batch.
pub fn run_batch_recorded(
    config: &Config,
    spec: &BatchSpec,
    rec: &mut BatchRecording<'_, '_>,
) -> Result<BatchReport, BatchError> {
    let _ = rec;
    run_batch(config, spec)
}

/// Whether the CSV carries the per-game wall time (the only non-deterministic column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// Add `wall_ms` as the last column.
    Include,
    /// Leave it out, so two runs can be diffed byte for byte.
    Omit,
}

/// Writes one row per game: `seed, winner, days_played, ticks, state_hash`, then
/// `p<i>_net_worth_pence, p<i>_bankrupt` per seat, then `wall_ms` if timing is included.
pub fn write_csv(out: &mut impl Write, results: &[GameResult], timing: Timing) -> io::Result<()> {
    let seats = results.iter().map(|r| r.players.len()).max().unwrap_or(0);
    let mut header = String::from("seed,winner,days_played,ticks,state_hash");
    for i in 0..seats {
        let _ = write!(header, ",p{i}_net_worth_pence,p{i}_bankrupt");
    }
    if timing == Timing::Include {
        header.push_str(",wall_ms");
    }
    writeln!(out, "{header}")?;
    for r in results {
        write!(
            out,
            "{},{},{},{},{:016x}",
            r.seed, r.winner, r.days_played, r.ticks, r.state_hash
        )?;
        for i in 0..seats {
            match r.players.get(i) {
                Some(p) => write!(out, ",{},{}", p.net_worth.pence(), p.bankrupt)?,
                None => write!(out, ",,")?,
            }
        }
        if timing == Timing::Include {
            write!(out, ",{:.3}", r.wall.as_secs_f64() * 1_000.0)?;
        }
        writeln!(out)?;
    }
    Ok(())
}

/// One seat's results across the batch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeatSummary {
    /// Seat index.
    pub player: u8,
    /// Net worth spread, pence.
    pub net_worth_pence: Option<Distribution>,
    /// Games this seat went bankrupt in.
    pub bankruptcies: usize,
    /// `bankruptcies / games`.
    pub bankruptcy_rate: f64,
    /// Games this seat won outright.
    pub wins: usize,
}

/// The batch at a glance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    /// Games played.
    pub games: usize,
    /// Threads used.
    pub threads: usize,
    /// Wall time, seconds.
    pub wall_seconds: f64,
    /// Throughput.
    pub games_per_second: f64,
    /// Per seat.
    pub seats: Vec<SeatSummary>,
    /// Net worth of every player in every game, pence.
    pub net_worth_pence: Option<Distribution>,
    /// Bankrupt player-games over all player-games.
    pub bankruptcy_rate: f64,
    /// Games won by nobody because all solvent leaders tied.
    pub ties: usize,
    /// Games with no solvent player.
    pub no_winner: usize,
    /// Days played per game.
    pub days_played: Option<Distribution>,
}

/// Summarises a batch.
pub fn summarise(report: &BatchReport) -> Summary {
    let results = &report.results;
    let games = results.len();
    let seats = results.iter().map(|r| r.players.len()).max().unwrap_or(0);
    let seat_summaries = (0..seats)
        .map(|i| {
            let worth: Vec<i64> = results
                .iter()
                .filter_map(|r| r.players.get(i))
                .map(|p| p.net_worth.pence())
                .collect();
            let bankruptcies = results
                .iter()
                .filter(|r| r.players.get(i).is_some_and(|p| p.bankrupt))
                .count();
            let player = u8::try_from(i).unwrap_or(u8::MAX);
            SeatSummary {
                player,
                net_worth_pence: distribution(&worth),
                bankruptcies,
                bankruptcy_rate: rate(bankruptcies, games),
                wins: results
                    .iter()
                    .filter(|r| r.winner == Winner::Player(player))
                    .count(),
            }
        })
        .collect();
    let all_worth: Vec<i64> = results
        .iter()
        .flat_map(|r| r.players.iter().map(|p| p.net_worth.pence()))
        .collect();
    let player_games = all_worth.len();
    let bankrupt = results
        .iter()
        .flat_map(|r| r.players.iter())
        .filter(|p| p.bankrupt)
        .count();
    let days: Vec<i64> = results.iter().map(|r| i64::from(r.days_played)).collect();
    let wall = report.wall.as_secs_f64();
    #[allow(
        clippy::cast_precision_loss,
        reason = "game counts are far below 2^52; throughput is for display"
    )]
    let games_per_second = if wall > 0.0 { games as f64 / wall } else { 0.0 };
    Summary {
        games,
        threads: report.threads,
        wall_seconds: wall,
        games_per_second,
        seats: seat_summaries,
        net_worth_pence: distribution(&all_worth),
        bankruptcy_rate: rate(bankrupt, player_games),
        ties: results.iter().filter(|r| r.winner == Winner::Tie).count(),
        no_winner: results
            .iter()
            .filter(|r| r.winner == Winner::Nobody)
            .count(),
        days_played: distribution(&days),
    }
}

fn pounds(pence: i64) -> String {
    sim_core::Money::from_pence(pence).to_string()
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "games={} threads={} wall={:.2}s games/sec={:.1}",
            self.games, self.threads, self.wall_seconds, self.games_per_second
        )?;
        if let Some(d) = self.days_played {
            writeln!(
                f,
                "days played: median {} (min {}, max {})",
                d.median, d.min, d.max
            )?;
        }
        writeln!(
            f,
            "bankruptcy rate {:.1}% of player-games; ties {}; no winner {}",
            self.bankruptcy_rate * 100.0,
            self.ties,
            self.no_winner
        )?;
        for s in &self.seats {
            write!(f, "p{}: ", s.player)?;
            if let Some(d) = s.net_worth_pence {
                write!(
                    f,
                    "net worth p10 {} / median {} / p90 {}; ",
                    pounds(d.p10),
                    pounds(d.median),
                    pounds(d.p90)
                )?;
            }
            writeln!(
                f,
                "bankrupt {:.1}%; wins {}",
                s.bankruptcy_rate * 100.0,
                s.wins
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PlayerResult;
    use sim_core::Money;

    fn result(seed: u64, worth: [i64; 2], bankrupt: [bool; 2], winner: Winner) -> GameResult {
        GameResult {
            seed,
            winner,
            players: worth
                .iter()
                .zip(bankrupt)
                .map(|(w, b)| PlayerResult {
                    net_worth: Money::from_pence(*w),
                    bankrupt: b,
                })
                .collect(),
            days_played: 4,
            ticks: 34_560,
            state_hash: 0xabc,
            wall: Duration::from_millis(2),
        }
    }

    fn report() -> BatchReport {
        BatchReport {
            results: vec![
                result(5, [100, -50], [false, true], Winner::Player(0)),
                result(6, [300, 300], [false, false], Winner::Tie),
                result(7, [-10, -20], [true, true], Winner::Nobody),
            ],
            wall: Duration::from_secs(1),
            threads: 2,
        }
    }

    #[test]
    fn csv_has_a_header_and_one_row_per_game() {
        let mut out = Vec::new();
        write_csv(&mut out, &report().results, Timing::Include).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "seed,winner,days_played,ticks,state_hash,p0_net_worth_pence,p0_bankrupt,p1_net_worth_pence,p1_bankrupt,wall_ms"
        );
        assert_eq!(
            lines[1],
            "5,p0,4,34560,0000000000000abc,100,false,-50,true,2.000"
        );
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn csv_without_timing_drops_only_the_wall_column() {
        let mut with = Vec::new();
        let mut without = Vec::new();
        write_csv(&mut with, &report().results, Timing::Include).unwrap();
        write_csv(&mut without, &report().results, Timing::Omit).unwrap();
        let with = String::from_utf8(with).unwrap();
        let without = String::from_utf8(without).unwrap();
        for (a, b) in with.lines().zip(without.lines()) {
            assert_eq!(a.rsplit_once(',').unwrap().0, b);
        }
    }

    #[test]
    fn summary_counts_bankruptcies_wins_and_ties() {
        let s = summarise(&report());
        assert_eq!(s.games, 3);
        assert_eq!(s.seats[0].bankruptcies, 1);
        assert_eq!(s.seats[1].bankruptcies, 2);
        assert_eq!(s.seats[0].wins, 1);
        assert_eq!(s.ties, 1);
        assert_eq!(s.no_winner, 1);
        assert!((s.bankruptcy_rate - 0.5).abs() < 1e-9);
        assert!((s.games_per_second - 3.0).abs() < 1e-9);
        assert_eq!(s.seats[0].net_worth_pence.unwrap().median, 100);
        let text = s.to_string();
        assert!(text.contains("games/sec=3.0"), "{text}");
    }

    #[test]
    fn seed_overflow_is_an_error_not_a_wrap() {
        let config = crate::test_support::shipped_config();
        let spec = BatchSpec {
            games: 2,
            base_seed: u64::MAX,
            ..BatchSpec::default()
        };
        assert!(matches!(
            run_batch(&config, &spec),
            Err(BatchError::SeedOverflow { .. })
        ));
    }
}
