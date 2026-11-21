//! Config snapshots: the effective config of a game (every file, after preset and overrides)
//! as one YAML document, stored once per distinct config at `configs/<hash>.yaml`.

use std::collections::BTreeMap;

use sim_core::ConfigSources;
use sim_core::hash::stable_hash;

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

fn files(sources: &ConfigSources) -> BTreeMap<String, String> {
    sources
        .file_names()
        .map(|f| (f.to_owned(), sources.get(f).unwrap_or_default().to_owned()))
        .collect()
}

fn hash_of(files: &BTreeMap<String, String>) -> String {
    let pairs: Vec<(&str, &str)> = files
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    format!("{:016x}", stable_hash(&pairs))
}

/// The snapshot of `sources`.
pub fn snapshot(sources: &ConfigSources) -> ConfigSnapshot {
    let files = files(sources);
    let hash = hash_of(&files);
    let body = serde_yaml_ng::to_string(&files).unwrap_or_default();
    ConfigSnapshot {
        yaml: format!("# OpenTycoon config snapshot {hash}: file name -> file text\n{body}"),
        hash,
    }
}

/// The hash of `sources`, as [`snapshot`] names it.
pub fn config_hash(sources: &ConfigSources) -> String {
    hash_of(&files(sources))
}

/// Reads a snapshot back, checking it hashes to `expected`.
pub fn parse(yaml: &str, expected: &str) -> Result<ConfigSources, SnapshotError> {
    let files: BTreeMap<String, String> =
        serde_yaml_ng::from_str(yaml).map_err(|e| SnapshotError::Yaml(e.to_string()))?;
    let found = hash_of(&files);
    if found != expected {
        return Err(SnapshotError::Hash {
            expected: expected.to_owned(),
            found,
        });
    }
    let mut sources = ConfigSources::new();
    for (f, t) in files {
        sources.insert(f, t);
    }
    Ok(sources)
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
        for name in files.file_names() {
            let mut edited = files.clone();
            edited.insert(name, format!("{}\n# edited\n", files.get(name).unwrap()));
            assert_ne!(config_hash(&edited), config_hash(&files), "{name}");
        }
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
    fn a_snapshot_holds_every_config_file() {
        let snap = snapshot(&sources());
        let files: BTreeMap<String, String> = serde_yaml_ng::from_str(&snap.yaml).unwrap();
        let names: Vec<&str> = files.keys().map(String::as_str).collect();
        let mut all: Vec<&str> = sim_core::config::ConfigFile::ALL
            .iter()
            .map(|f| f.file_name())
            .collect();
        all.sort_unstable();
        assert_eq!(names, all);
    }

    #[test]
    fn the_key_follows_the_layout() {
        assert_eq!(key("00ff"), "configs/00ff.yaml");
    }
}
