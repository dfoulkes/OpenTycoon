//! Reading replays back: from a segment (`.otr`) or through a manifest (`.parquet`), out of any
//! store, with their config snapshots.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use sim_core::Config;

use super::format::{Replay, decode_all, decode_one};
use super::manifest::{ManifestRow, from_parquet};
use super::snapshot;
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

/// Every game in `key`: a manifest (`*.parquet`, games in row order, each segment fetched
/// once) or a segment (`*.otr`, every envelope in it).
pub fn games(store: &dyn ObjectStore, key: &str) -> Result<Vec<Loaded>, String> {
    let bytes = store.get(key).map_err(|e| e.to_string())?;
    if !key.ends_with(".parquet") {
        return Ok(decode_all(&bytes)
            .map_err(|e| format!("{key}: {e}"))?
            .into_iter()
            .map(|(replay, _, _)| Loaded {
                replay,
                object_key: key.to_owned(),
                row: None,
            })
            .collect());
    }
    let rows = from_parquet(&bytes).map_err(|e| format!("{key}: {e}"))?;
    let mut segments: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if !segments.contains_key(&row.object_key) {
            let seg = store.get(&row.object_key).map_err(|e| e.to_string())?;
            segments.insert(row.object_key.clone(), seg);
        }
        let seg = &segments[&row.object_key];
        let at = usize::try_from(row.offset).unwrap_or(usize::MAX);
        let end = at.saturating_add(usize::try_from(row.size_bytes).unwrap_or(usize::MAX));
        let slice = seg.get(at..end).ok_or_else(|| {
            format!(
                "{}: game {} at {}+{} is past the end ({} bytes)",
                row.object_key,
                row.game_id,
                row.offset,
                row.size_bytes,
                seg.len()
            )
        })?;
        let (replay, used) = decode_one(slice)
            .map_err(|e| format!("{} game {}: {e}", row.object_key, row.game_id))?;
        if used != slice.len() || replay.header.game_id != row.game_id {
            return Err(format!(
                "{}: the manifest's range for game {} holds {}",
                row.object_key, row.game_id, replay.header.game_id
            ));
        }
        out.push(Loaded {
            replay,
            object_key: row.object_key.clone(),
            row: Some(row),
        });
    }
    Ok(out)
}

/// The config a replay was recorded with, from its snapshot in `store`.
pub fn config(store: &dyn ObjectStore, config_hash: &str) -> Result<Config, String> {
    let key = snapshot::key(config_hash);
    let bytes = store
        .get(&key)
        .map_err(|e| format!("config snapshot: {e}"))?;
    let yaml = String::from_utf8(bytes).map_err(|e| format!("{key}: {e}"))?;
    let files = snapshot::parse(&yaml, config_hash).map_err(|e| e.to_string())?;
    Config::load(&files).map_err(|e| format!("{key} does not load:\n{e}"))
}

/// Splits a local path into a store root and a key, by finding the `replays/` or `manifests/`
/// directory of the layout in it.
pub fn split_local(path: &Path) -> Option<(PathBuf, String)> {
    let parts: Vec<&std::ffi::OsStr> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    let at = parts
        .iter()
        .rposition(|p| *p == "replays" || *p == "manifests")?;
    let key: Vec<String> = parts[at..]
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut root = PathBuf::new();
    let mut seen = 0;
    for c in path.components() {
        if let Component::Normal(_) = c {
            if seen == at {
                break;
            }
            seen += 1;
        }
        root.push(c);
    }
    Some((root, key.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_paths_split_at_the_layout() {
        let (root, key) =
            split_local(Path::new("/data/rec/manifests/run-1/part-00000.parquet")).unwrap();
        assert_eq!(root, PathBuf::from("/data/rec"));
        assert_eq!(key, "manifests/run-1/part-00000.parquet");
        let (root, key) = split_local(Path::new("rec/replays/ab/run/p-s000.otr")).unwrap();
        assert_eq!(root, PathBuf::from("rec"));
        assert_eq!(key, "replays/ab/run/p-s000.otr");
        assert!(split_local(Path::new("/tmp/game.otr")).is_none());
    }
}
