//! Manifests: Parquet files that index replays (docs/replay.md "Manifest schema"), one row per
//! game; plus the training weights: the per-game weight written into the manifest (human games
//! count more) and the rules-weight policy applied at extract time.

use super::format::{Replay, Seat, SeatOutcome};

/// Seat column groups in every manifest (`seat0_*` … `seat7_*`); unused seats are null.
pub const MAX_SEATS: usize = 8;

/// Per-game weights written into the manifest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// Weight of a game with at least one human seat; AI-only games weigh 1.0.
    pub human_multiplier: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            human_multiplier: 5.0,
        }
    }
}

impl Weights {
    /// The weight of a game with these seats.
    pub fn game_weight(&self, seats: &[Seat]) -> f64 {
        let _ = seats;
        0.0
    }
}

/// One manifest row: one game.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestRow {
    /// Unique within the run.
    pub game_id: String,
    /// The run.
    pub run_id: String,
    /// Rules fingerprint the game was recorded under.
    pub rules_fingerprint: String,
    /// The sim build's git commit.
    pub git_commit: String,
    /// Config snapshot hash.
    pub config_hash: String,
    /// Seed.
    pub seed: u64,
    /// Length preset.
    pub preset: String,
    /// First year.
    pub start_year: i32,
    /// Last year.
    pub end_year: i32,
    /// `.otr` format version.
    pub format_version: u16,
    /// What recorded it: `batch`, `study`, `record`.
    pub source: String,
    /// Study model id, for study games.
    pub model_id: Option<u32>,
    /// Seats.
    pub seats: Vec<Seat>,
    /// Per seat at the whistle.
    pub seat_outcomes: Vec<SeatOutcome>,
    /// Human seats.
    pub human_seats: u32,
    /// Training weight (1.0, or the human multiplier).
    pub weight: f64,
    /// `p<n>`, `tie` or `none`.
    pub winner: String,
    /// Midnights passed.
    pub days_played: u32,
    /// Ticks run.
    pub ticks: u64,
    /// Final state hash, 16 hex digits.
    pub final_state_hash: String,
    /// Seats that went bankrupt.
    pub bankruptcies: u32,
    /// Commands in the log.
    pub commands: u64,
    /// The segment object holding the replay.
    pub object_key: String,
    /// Byte offset of the replay's envelope in the object.
    pub offset: u64,
    /// Length of the envelope in bytes.
    pub size_bytes: u64,
}

/// Why a manifest could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// More seats than the manifest has columns for.
    #[error("game {game}: {seats} seats; manifests hold at most {MAX_SEATS}")]
    TooManySeats {
        /// The game.
        game: String,
        /// Its seats.
        seats: usize,
    },
    /// Parquet failed.
    #[error("parquet: {0}")]
    Parquet(String),
    /// A row is missing a required column or has a bad value.
    #[error("manifest row {row}: {reason}")]
    Row {
        /// Row index.
        row: usize,
        /// What is wrong.
        reason: String,
    },
}

/// The manifest row for `replay`, stored at `object_key[offset..offset + size_bytes]`.
pub fn row_for(
    replay: &Replay,
    weights: &Weights,
    source: &str,
    model_id: Option<u32>,
    object_key: &str,
    offset: u64,
    size_bytes: u64,
) -> ManifestRow {
    let _ = (
        replay, weights, source, model_id, object_key, offset, size_bytes,
    );
    ManifestRow {
        game_id: String::new(),
        run_id: String::new(),
        rules_fingerprint: String::new(),
        git_commit: String::new(),
        config_hash: String::new(),
        seed: 0,
        preset: String::new(),
        start_year: 0,
        end_year: 0,
        format_version: 0,
        source: String::new(),
        model_id: None,
        seats: Vec::new(),
        seat_outcomes: Vec::new(),
        human_seats: 0,
        weight: 0.0,
        winner: String::new(),
        days_played: 0,
        ticks: 0,
        final_state_hash: String::new(),
        bankruptcies: 0,
        commands: 0,
        object_key: String::new(),
        offset: 0,
        size_bytes: 0,
    }
}

/// The manifest's column names, in order.
pub fn columns() -> Vec<String> {
    Vec::new()
}

/// Writes rows as a Parquet file.
pub fn to_parquet(rows: &[ManifestRow]) -> Result<Vec<u8>, ManifestError> {
    let _ = rows;
    Err(ManifestError::Parquet("not implemented".into()))
}

/// Reads a Parquet manifest.
pub fn from_parquet(bytes: &[u8]) -> Result<Vec<ManifestRow>, ManifestError> {
    let _ = bytes;
    Err(ManifestError::Parquet("not implemented".into()))
}

/// The column names of a Parquet file, in order.
pub fn parquet_columns(bytes: &[u8]) -> Result<Vec<String>, ManifestError> {
    let _ = bytes;
    Err(ManifestError::Parquet("not implemented".into()))
}

/// How much a game counts for training by the rules it was recorded under, applied at extract
/// time (never baked into a manifest): 1.0 for `current`; `decay^(i + 1)` for `history[i]`
/// (newest first); `decay` for any other fingerprint; never below `floor`.
#[derive(Debug, Clone, PartialEq)]
pub struct RulesWeight {
    /// The fingerprint training is for (normally this build's).
    pub current: String,
    /// Older fingerprints, newest first.
    pub history: Vec<String>,
    /// Weight multiplier per generation of age.
    pub decay: f64,
    /// Lowest weight.
    pub floor: f64,
}

impl RulesWeight {
    /// Full weight for `current`, the default decay (0.5) and floor (0.0).
    pub fn new(current: impl Into<String>) -> Self {
        RulesWeight {
            current: current.into(),
            history: Vec::new(),
            decay: 0.5,
            floor: 0.0,
        }
    }

    /// The weight for games recorded under `fingerprint`.
    pub fn weight(&self, fingerprint: &str) -> f64 {
        let _ = fingerprint;
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::format::{SeatKind, tests::replay};
    use proptest::prelude::*;
    use proptest::strategy::ValueTree;

    fn seat(kind: SeatKind) -> Seat {
        Seat {
            kind,
            brain: "x".into(),
            brain_version: "1".into(),
            player_id: None,
        }
    }

    #[test]
    fn human_games_weigh_the_multiplier_and_ai_games_one() {
        let w = Weights {
            human_multiplier: 5.0,
        };
        assert!((w.game_weight(&[seat(SeatKind::Ai), seat(SeatKind::Ai)]) - 1.0).abs() < 1e-12);
        assert!((w.game_weight(&[seat(SeatKind::Human), seat(SeatKind::Ai)]) - 5.0).abs() < 1e-12);
        assert!(
            (w.game_weight(&[seat(SeatKind::Human), seat(SeatKind::Human)]) - 5.0).abs() < 1e-12
        );
    }

    #[test]
    fn rules_weight_decays_by_generation_with_a_floor() {
        let mut r = RulesWeight::new("now");
        r.history = vec!["old".into(), "older".into(), "oldest".into()];
        r.decay = 0.5;
        r.floor = 0.2;
        assert!((r.weight("now") - 1.0).abs() < 1e-12);
        assert!((r.weight("old") - 0.5).abs() < 1e-12);
        assert!((r.weight("older") - 0.25).abs() < 1e-12);
        assert!((r.weight("oldest") - 0.2).abs() < 1e-12);
        assert!((r.weight("unknown") - 0.5).abs() < 1e-12);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(16))]

        /// A manifest written and read back gives the same rows.
        #[test]
        fn manifests_round_trip(r in replay().prop_filter("seats fit", |r| {
            r.header.seats.len() <= MAX_SEATS && r.outcome.seats.len() <= r.header.seats.len()
        }), offset in any::<u32>(), size in any::<u32>()) {
            let row = row_for(&r, &Weights::default(), "batch", Some(3), "replays/k.otr",
                              u64::from(offset), u64::from(size));
            let bytes = to_parquet(std::slice::from_ref(&row)).unwrap();
            let back = from_parquet(&bytes).unwrap();
            prop_assert_eq!(back, vec![row]);
            prop_assert_eq!(parquet_columns(&bytes).unwrap(), columns());
        }
    }

    #[test]
    fn too_many_seats_is_an_error() {
        let mut runner = proptest::test_runner::TestRunner::deterministic();
        let mut r = replay().new_tree(&mut runner).unwrap().current();
        r.header.seats = vec![seat(SeatKind::Ai); MAX_SEATS + 1];
        let row = row_for(&r, &Weights::default(), "batch", None, "k", 0, 0);
        assert!(matches!(
            to_parquet(&[row]),
            Err(ManifestError::TooManySeats { .. })
        ));
    }
}
