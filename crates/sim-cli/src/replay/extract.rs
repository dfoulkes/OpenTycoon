//! `sim-cli extract`: the stub feature extractor. One Parquet row per recorded command:
//! (decision tick, seat, command), with a versioned extractor id, lineage columns (which
//! replay, build, rules and config it came from) and the training weight: the manifest's game
//! weight times the rules weight, applied here and never baked into a manifest. The real
//! feature extractor replaces it later.

use super::format::{FORMAT_VERSION, Origin, Replay};
use super::manifest::{ManifestError, RulesWeight, Weights};
use super::parquet_table::{self, Column, Kind, Value};

/// The extractor's id.
pub const EXTRACTOR_ID: &str = "cmdlog-stub";
/// The extractor's version; bump when its output changes.
pub const EXTRACTOR_VERSION: u32 = 1;

/// One replay to extract, with where it came from and its manifest weight if known.
#[derive(Debug, Clone)]
pub struct Source<'a> {
    /// The replay.
    pub replay: &'a Replay,
    /// Object key (or file) it was read from.
    pub object_key: &'a str,
    /// The manifest's game weight, if it came through a manifest.
    pub weight: Option<f64>,
}

const COLUMNS: [(&str, Kind); 23] = [
    ("extractor_id", Kind::Str),
    ("extractor_version", Kind::I64),
    ("game_id", Kind::Str),
    ("run_id", Kind::Str),
    ("rules_fingerprint", Kind::Str),
    ("git_commit", Kind::Str),
    ("config_hash", Kind::Str),
    ("format_version", Kind::I64),
    ("source_object", Kind::Str),
    ("seed", Kind::I64),
    ("decision_tick", Kind::I64),
    ("tick", Kind::I64),
    ("seat", Kind::I64),
    ("seat_kind", Kind::Str),
    ("brain", Kind::Str),
    ("origin", Kind::Str),
    ("accepted", Kind::Bool),
    ("command_kind", Kind::Str),
    ("command", Kind::Str),
    ("game_weight", Kind::F64),
    ("rules_weight", Kind::F64),
    ("sample_weight", Kind::F64),
    ("index", Kind::I64),
];

/// The extract's column names, in order.
pub fn columns() -> Vec<String> {
    COLUMNS.iter().map(|(n, _)| (*n).to_owned()).collect()
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "u64 → i64 by bits, as in the manifest"
)]
fn bits(x: u64) -> Value {
    Value::I64(x as i64)
}

/// Extracts `sources` into one Parquet table; returns it and its row count.
pub fn extract(
    sources: &[Source<'_>],
    rules: &RulesWeight,
    weights: &Weights,
) -> Result<(Vec<u8>, u64), ManifestError> {
    let mut cols: Vec<Column> = COLUMNS
        .iter()
        .map(|(n, k)| Column::new(*n, *k, false))
        .collect();
    let mut rows = 0u64;
    for src in sources {
        let h = &src.replay.header;
        let game_weight = src.weight.unwrap_or_else(|| weights.game_weight(&h.seats));
        let rules_weight = rules.weight(&h.rules_fingerprint);
        for (i, c) in src.replay.commands.iter().enumerate() {
            let seat = h.seats.get(usize::from(c.seat));
            let json = serde_json::to_value(&c.command)
                .map_err(|e| ManifestError::Parquet(e.to_string()))?;
            let kind = match &json {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
                _ => String::new(),
            };
            let values = [
                Value::Str(EXTRACTOR_ID.into()),
                Value::I64(i64::from(EXTRACTOR_VERSION)),
                Value::Str(h.game_id.clone()),
                Value::Str(h.run_id.clone()),
                Value::Str(h.rules_fingerprint.clone()),
                Value::Str(h.git_commit.clone()),
                Value::Str(h.config_hash.clone()),
                Value::I64(i64::from(FORMAT_VERSION)),
                Value::Str(src.object_key.to_owned()),
                bits(h.seed),
                // Commands are decided one tick before they apply (the AI phase, or the
                // harness between ticks).
                bits(c.tick.saturating_sub(1)),
                bits(c.tick),
                Value::I64(i64::from(c.seat)),
                Value::Str(seat.map_or("", |s| s.kind.name()).to_owned()),
                Value::Str(seat.map_or("", |s| s.brain.as_str()).to_owned()),
                Value::Str(
                    match c.origin {
                        Origin::External => "external",
                        Origin::Core => "core",
                    }
                    .into(),
                ),
                Value::Bool(c.accepted),
                Value::Str(kind),
                Value::Str(json.to_string()),
                Value::F64(game_weight),
                Value::F64(rules_weight),
                Value::F64(game_weight * rules_weight),
                bits(i as u64),
            ];
            for (col, v) in cols.iter_mut().zip(values) {
                col.values.push(v);
            }
            rows += 1;
        }
    }
    let bytes = parquet_table::write(&cols).map_err(ManifestError::Parquet)?;
    Ok((bytes, rows))
}
