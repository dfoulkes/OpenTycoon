//! `sim-cli extract`: the stub feature extractor. One Parquet row per recorded command:
//! (decision tick, seat, command), with a versioned extractor id, lineage columns (which
//! replay, build, rules and config it came from) and the training weight (the manifest's game
//! weight times the rules weight). The real feature extractor replaces it later.

use super::format::Replay;
use super::manifest::{ManifestError, RulesWeight, Weights};

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

/// The extract's column names, in order.
pub fn columns() -> Vec<String> {
    Vec::new()
}

/// Extracts `sources` into one Parquet table.
pub fn extract(
    sources: &[Source<'_>],
    rules: &RulesWeight,
    weights: &Weights,
) -> Result<(Vec<u8>, u64), ManifestError> {
    let _ = (sources, rules, weights);
    Err(ManifestError::Parquet("not implemented".into()))
}
