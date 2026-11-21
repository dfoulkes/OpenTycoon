//! Reading replays back: from a segment (`.otr`) or through a manifest (`.parquet`), out of any
//! store, with their config snapshots.

use sim_core::Config;

use super::format::Replay;
use super::manifest::ManifestRow;
use super::store::ObjectStore;

/// One loaded game.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The replay.
    pub replay: Replay,
    /// The object it came from.
    pub object_key: String,
    /// Its manifest row, when loaded through a manifest.
    pub row: Option<ManifestRow>,
}

/// Every game in `key`: a manifest (`*.parquet`) or a segment (`*.otr`).
pub fn games(store: &dyn ObjectStore, key: &str) -> Result<Vec<Loaded>, String> {
    let _ = (store, key);
    Err("not implemented".into())
}

/// The config a replay was recorded with, from its snapshot in `store`.
pub fn config(store: &dyn ObjectStore, config_hash: &str) -> Result<Config, String> {
    let _ = (store, config_hash);
    Err("not implemented".into())
}

/// Splits a local path into a store root and a key, by finding the `replays/` or `manifests/`
/// directory of the layout in it.
pub fn split_local(path: &std::path::Path) -> Option<(std::path::PathBuf, String)> {
    let _ = path;
    None
}
