//! Manifests: Parquet files that index replays (docs/replay.md "Manifest schema"), one row per
//! game; plus the training weights: the per-game weight written into the manifest (human games
//! count more) and the rules-weight policy applied at extract time.

use std::collections::BTreeMap;

use super::format::{FORMAT_VERSION, Replay, Seat, SeatKind, SeatOutcome};
use super::parquet_table::{self, Column, Kind, Value};

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
        if seats.iter().any(|s| s.kind == SeatKind::Human) {
            self.human_multiplier
        } else {
            1.0
        }
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
    let h = &replay.header;
    let o = &replay.outcome;
    ManifestRow {
        game_id: h.game_id.clone(),
        run_id: h.run_id.clone(),
        rules_fingerprint: h.rules_fingerprint.clone(),
        git_commit: h.git_commit.clone(),
        config_hash: h.config_hash.clone(),
        seed: h.seed,
        preset: h.preset.clone(),
        start_year: h.start_year,
        end_year: h.end_year,
        format_version: FORMAT_VERSION,
        source: source.to_owned(),
        model_id,
        seats: h.seats.clone(),
        seat_outcomes: o.seats.clone(),
        human_seats: count(h.seats.iter().filter(|s| s.kind == SeatKind::Human)),
        weight: weights.game_weight(&h.seats),
        winner: o.winner.to_string(),
        days_played: o.days_played,
        ticks: o.ticks,
        final_state_hash: format!("{:016x}", o.final_state_hash),
        bankruptcies: count(o.seats.iter().filter(|s| s.bankrupt)),
        commands: replay.commands.len() as u64,
        object_key: object_key.to_owned(),
        offset,
        size_bytes,
    }
}

fn count<T>(it: impl Iterator<Item = T>) -> u32 {
    u32::try_from(it.count()).unwrap_or(u32::MAX)
}

/// The fixed columns, then [`SEAT_COLUMNS`] for each of [`MAX_SEATS`] seats.
const FIXED: [(&str, Kind, bool); 24] = [
    ("game_id", Kind::Str, false),
    ("run_id", Kind::Str, false),
    ("rules_fingerprint", Kind::Str, false),
    ("git_commit", Kind::Str, false),
    ("config_hash", Kind::Str, false),
    ("seed", Kind::I64, false),
    ("preset", Kind::Str, false),
    ("start_year", Kind::I64, false),
    ("end_year", Kind::I64, false),
    ("format_version", Kind::I64, false),
    ("source", Kind::Str, false),
    ("model_id", Kind::I64, true),
    ("n_seats", Kind::I64, false),
    ("human_seats", Kind::I64, false),
    ("weight", Kind::F64, false),
    ("winner", Kind::Str, false),
    ("days_played", Kind::I64, false),
    ("ticks", Kind::I64, false),
    ("final_state_hash", Kind::Str, false),
    ("bankruptcies", Kind::I64, false),
    ("commands", Kind::I64, false),
    ("object_key", Kind::Str, false),
    ("offset", Kind::I64, false),
    ("size_bytes", Kind::I64, false),
];

/// Per-seat columns, `seat<N>_<name>`; all nullable (null where the game has fewer seats).
const SEAT_COLUMNS: [(&str, Kind); 8] = [
    ("kind", Kind::Str),
    ("brain", Kind::Str),
    ("brain_version", Kind::Str),
    ("player_id", Kind::Str),
    ("rank", Kind::I64),
    ("net_worth", Kind::I64),
    ("bankrupt", Kind::Bool),
    ("weight", Kind::F64),
];

fn all_columns() -> Vec<Column> {
    let mut cols: Vec<Column> = FIXED
        .iter()
        .map(|(n, k, o)| Column::new(*n, *k, *o))
        .collect();
    for i in 0..MAX_SEATS {
        cols.extend(
            SEAT_COLUMNS
                .iter()
                .map(|(n, k)| Column::new(format!("seat{i}_{n}"), *k, true)),
        );
    }
    cols
}

/// The manifest's column names, in order.
pub fn columns() -> Vec<String> {
    all_columns().into_iter().map(|c| c.name).collect()
}

/// u64 → i64 by bits (Parquet has no unsigned 64-bit physical type); read back the same way.
#[allow(
    clippy::cast_possible_wrap,
    reason = "documented bit-cast: seeds and counts above i64::MAX round-trip"
)]
fn bits(x: u64) -> Value {
    Value::I64(x as i64)
}

fn s(x: &str) -> Value {
    Value::Str(x.to_owned())
}

fn row_values(r: &ManifestRow) -> Result<Vec<Value>, ManifestError> {
    if r.seats.len() > MAX_SEATS {
        return Err(ManifestError::TooManySeats {
            game: r.game_id.clone(),
            seats: r.seats.len(),
        });
    }
    let mut v = vec![
        s(&r.game_id),
        s(&r.run_id),
        s(&r.rules_fingerprint),
        s(&r.git_commit),
        s(&r.config_hash),
        bits(r.seed),
        s(&r.preset),
        Value::I64(i64::from(r.start_year)),
        Value::I64(i64::from(r.end_year)),
        Value::I64(i64::from(r.format_version)),
        s(&r.source),
        r.model_id.map_or(Value::Null, |m| Value::I64(i64::from(m))),
        Value::I64(i64::try_from(r.seats.len()).unwrap_or(i64::MAX)),
        Value::I64(i64::from(r.human_seats)),
        Value::F64(r.weight),
        s(&r.winner),
        Value::I64(i64::from(r.days_played)),
        bits(r.ticks),
        s(&r.final_state_hash),
        Value::I64(i64::from(r.bankruptcies)),
        bits(r.commands),
        s(&r.object_key),
        bits(r.offset),
        bits(r.size_bytes),
    ];
    for i in 0..MAX_SEATS {
        match r.seats.get(i) {
            Some(seat) => {
                v.push(s(seat.kind.name()));
                v.push(s(&seat.brain));
                v.push(s(&seat.brain_version));
                v.push(seat.player_id.as_deref().map_or(Value::Null, s));
            }
            None => v.extend([Value::Null, Value::Null, Value::Null, Value::Null]),
        }
        match r.seat_outcomes.get(i).filter(|_| i < r.seats.len()) {
            Some(o) => {
                v.push(Value::I64(i64::from(o.rank)));
                v.push(Value::I64(o.net_worth));
                v.push(Value::Bool(o.bankrupt));
            }
            None => v.extend([Value::Null, Value::Null, Value::Null]),
        }
        v.push(Value::Null); // seat<N>_weight: reserved for per-player weights
    }
    Ok(v)
}

/// Writes rows as a Parquet file.
pub fn to_parquet(rows: &[ManifestRow]) -> Result<Vec<u8>, ManifestError> {
    let mut cols = all_columns();
    for r in rows {
        for (c, v) in cols.iter_mut().zip(row_values(r)?) {
            c.values.push(v);
        }
    }
    parquet_table::write(&cols).map_err(ManifestError::Parquet)
}

struct Cells<'a> {
    row: usize,
    map: &'a BTreeMap<String, Value>,
}

impl Cells<'_> {
    fn bad(&self, col: &str, what: &str) -> ManifestError {
        ManifestError::Row {
            row: self.row,
            reason: format!("{col}: {what}"),
        }
    }

    fn get(&self, col: &str) -> &Value {
        self.map.get(col).unwrap_or(&Value::Null)
    }

    fn opt_str(&self, col: &str) -> Result<Option<String>, ManifestError> {
        match self.get(col) {
            Value::Null => Ok(None),
            Value::Str(x) => Ok(Some(x.clone())),
            _ => Err(self.bad(col, "not a string")),
        }
    }

    fn str(&self, col: &str) -> Result<String, ManifestError> {
        self.opt_str(col)?.ok_or_else(|| self.bad(col, "missing"))
    }

    fn opt_i64(&self, col: &str) -> Result<Option<i64>, ManifestError> {
        match self.get(col) {
            Value::Null => Ok(None),
            Value::I64(x) => Ok(Some(*x)),
            _ => Err(self.bad(col, "not an integer")),
        }
    }

    fn i64(&self, col: &str) -> Result<i64, ManifestError> {
        self.opt_i64(col)?.ok_or_else(|| self.bad(col, "missing"))
    }

    fn int<T: TryFrom<i64>>(&self, col: &str) -> Result<T, ManifestError> {
        T::try_from(self.i64(col)?).map_err(|_| self.bad(col, "out of range"))
    }

    #[allow(
        clippy::cast_sign_loss,
        reason = "the documented bit-cast back from i64"
    )]
    fn bits(&self, col: &str) -> Result<u64, ManifestError> {
        Ok(self.i64(col)? as u64)
    }
}

fn read_row(row: usize, map: &BTreeMap<String, Value>) -> Result<ManifestRow, ManifestError> {
    let c = Cells { row, map };
    let n: usize = c.int("n_seats")?;
    if n > MAX_SEATS {
        return Err(c.bad("n_seats", "too many seats"));
    }
    let mut seats = Vec::with_capacity(n);
    let mut seat_outcomes = Vec::new();
    for i in 0..n {
        let col = |name: &str| format!("seat{i}_{name}");
        let kind = match c.str(&col("kind"))?.as_str() {
            "human" => SeatKind::Human,
            "ai" => SeatKind::Ai,
            _ => return Err(c.bad(&col("kind"), "not human or ai")),
        };
        seats.push(Seat {
            kind,
            brain: c.str(&col("brain"))?,
            brain_version: c.str(&col("brain_version"))?,
            player_id: c.opt_str(&col("player_id"))?,
        });
        if let Some(rank) = c.opt_i64(&col("rank"))? {
            seat_outcomes.push(SeatOutcome {
                rank: u8::try_from(rank).map_err(|_| c.bad(&col("rank"), "out of range"))?,
                net_worth: c.i64(&col("net_worth"))?,
                bankrupt: match c.get(&col("bankrupt")) {
                    Value::Bool(b) => *b,
                    _ => return Err(c.bad(&col("bankrupt"), "not a boolean")),
                },
            });
        }
    }
    Ok(ManifestRow {
        game_id: c.str("game_id")?,
        run_id: c.str("run_id")?,
        rules_fingerprint: c.str("rules_fingerprint")?,
        git_commit: c.str("git_commit")?,
        config_hash: c.str("config_hash")?,
        seed: c.bits("seed")?,
        preset: c.str("preset")?,
        start_year: c.int("start_year")?,
        end_year: c.int("end_year")?,
        format_version: c.int("format_version")?,
        source: c.str("source")?,
        model_id: c
            .opt_i64("model_id")?
            .map(|m| u32::try_from(m).map_err(|_| c.bad("model_id", "out of range")))
            .transpose()?,
        seats,
        seat_outcomes,
        human_seats: c.int("human_seats")?,
        weight: match c.get("weight") {
            Value::F64(w) => *w,
            _ => return Err(c.bad("weight", "not a float")),
        },
        winner: c.str("winner")?,
        days_played: c.int("days_played")?,
        ticks: c.bits("ticks")?,
        final_state_hash: c.str("final_state_hash")?,
        bankruptcies: c.int("bankruptcies")?,
        commands: c.bits("commands")?,
        object_key: c.str("object_key")?,
        offset: c.bits("offset")?,
        size_bytes: c.bits("size_bytes")?,
    })
}

/// Reads a Parquet manifest.
pub fn from_parquet(bytes: &[u8]) -> Result<Vec<ManifestRow>, ManifestError> {
    parquet_table::read(bytes)
        .map_err(ManifestError::Parquet)?
        .iter()
        .enumerate()
        .map(|(i, m)| read_row(i, m))
        .collect()
}

/// The column names of a Parquet file, in order.
pub fn parquet_columns(bytes: &[u8]) -> Result<Vec<String>, ManifestError> {
    parquet_table::columns(bytes).map_err(ManifestError::Parquet)
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
        if fingerprint == self.current {
            return 1.0;
        }
        let age = self
            .history
            .iter()
            .position(|f| f == fingerprint)
            .map_or(1, |i| i + 1);
        let w = (0..age).fold(1.0, |w, _| w * self.decay);
        w.max(self.floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::format::tests::replay;
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
