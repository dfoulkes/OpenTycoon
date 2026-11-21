//! Config snapshots: the effective config of a game (every file, after preset and overrides)
//! as one YAML document, stored once per distinct config at `configs/<hash>.yaml`.

use sim_core::ConfigSources;

/// One config snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSnapshot {
    /// 16 hex digits: the stable hash of the files' names and text.
    pub hash: String,
    /// The YAML document: a mapping from file name to that file's text.
    pub yaml: String,
}

/// Why a snapshot could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SnapshotError {
    /// Not a mapping of file name to text.
    #[error("config snapshot: {0}")]
    Yaml(String),
    /// The content does not hash to the name it was stored under.
    #[error("config snapshot {expected}: content hashes to {found}")]
    Hash {
        /// The hash it was looked up by.
        expected: String,
        /// The hash of what it holds.
        found: String,
    },
}

/// The snapshot of `sources`.
pub fn snapshot(sources: &ConfigSources) -> ConfigSnapshot {
    let _ = sources;
    ConfigSnapshot {
        hash: String::new(),
        yaml: String::new(),
    }
}

/// The hash of `sources`, as [`snapshot`] names it.
pub fn config_hash(sources: &ConfigSources) -> String {
    snapshot(sources).hash
}

/// Reads a snapshot back, checking it hashes to `expected`.
pub fn parse(yaml: &str, expected: &str) -> Result<ConfigSources, SnapshotError> {
    let _ = yaml;
    Err(SnapshotError::Hash {
        expected: expected.to_owned(),
        found: String::new(),
    })
}

/// The object key of a snapshot.
pub fn key(hash: &str) -> String {
    format!("configs/{hash}.yaml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> ConfigSources {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        ConfigSources::read_dir(&dir).unwrap()
    }

    #[test]
    fn a_snapshot_round_trips_to_the_same_files_and_config() {
        let files = sources();
        let snap = snapshot(&files);
        assert_eq!(snap.hash.len(), 16);
        let back = parse(&snap.yaml, &snap.hash).unwrap();
        assert_eq!(back, files);
        assert_eq!(
            sim_core::Config::load(&back).unwrap(),
            sim_core::Config::load(&files).unwrap()
        );
    }

    #[test]
    fn equal_configs_share_a_hash_and_any_edit_changes_it() {
        let files = sources();
        assert_eq!(snapshot(&files), snapshot(&files.clone()));
        let mut edited = files.clone();
        let game = edited
            .get("game.yaml")
            .unwrap()
            .replace("start_limit: 10000", "start_limit: 10001");
        edited.insert("game.yaml", game);
        assert_ne!(config_hash(&edited), config_hash(&files));
    }

    #[test]
    fn a_snapshot_under_the_wrong_name_is_refused() {
        let snap = snapshot(&sources());
        let err = parse(&snap.yaml, "0000000000000000").unwrap_err();
        assert!(matches!(err, SnapshotError::Hash { .. }), "{err}");
        assert!(matches!(
            parse("- a list", &snap.hash).unwrap_err(),
            SnapshotError::Yaml(_)
        ));
    }

    #[test]
    fn the_key_follows_the_layout() {
        assert_eq!(key("00ff"), "configs/00ff.yaml");
    }
}
