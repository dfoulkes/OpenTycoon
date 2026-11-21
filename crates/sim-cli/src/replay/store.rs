//! Object stores: where replays, config snapshots and manifests go. The layout
//! (docs/replay.md "Storage layout") is the same in every backend:
//!
//! - `replays/<rules_fingerprint>/<run_id>/<part>-s<NNN>.otr`: replay segments;
//! - `configs/<config_hash>.yaml`: config snapshots;
//! - `manifests/<run_id>/<part>.parquet`: manifests.
//!
//! Backends: [`LocalStore`] (a directory: tests and a local cache), [`MemStore`] (an in-memory
//! fake for tests) and [`super::s3::S3Store`].

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No such object.
    #[error("no object {0}")]
    NotFound(String),
    /// The key is not a clean relative path.
    #[error("bad object key {0:?}")]
    BadKey(String),
    /// Local I/O.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: String,
        /// The error.
        source: std::io::Error,
    },
    /// The backend refused or failed.
    #[error("{0}")]
    Backend(String),
    /// Configuration (environment) missing or invalid.
    #[error("{0}")]
    Config(String),
}

/// A flat key → bytes store.
pub trait ObjectStore: Send + Sync + std::fmt::Debug {
    /// Writes an object, replacing any with that key.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError>;
    /// Reads a whole object.
    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError>;
    /// Reads `len` bytes from `offset`.
    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>, StoreError>;
    /// True if the object exists.
    fn exists(&self, key: &str) -> Result<bool, StoreError>;
    /// Every key under `prefix`, with its size, in key order.
    fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, StoreError>;
    /// Deletes an object (no error if it is already gone).
    fn delete(&self, key: &str) -> Result<(), StoreError>;
    /// Where it is, for messages (never includes credentials).
    fn describe(&self) -> String;
}

/// Checks a key is a clean relative path: non-empty `/`-separated segments, none `.` or `..`.
pub fn check_key(key: &str) -> Result<(), StoreError> {
    let _ = key;
    Ok(())
}

/// A directory with the store layout under it.
#[derive(Debug, Clone)]
pub struct LocalStore {
    root: PathBuf,
}

impl LocalStore {
    /// A store rooted at `root` (created on first write).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalStore { root: root.into() }
    }

    /// The root directory.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
}

impl ObjectStore for LocalStore {
    fn put(&self, key: &str, _bytes: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Backend(format!("not implemented: {key}")))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn get_range(&self, key: &str, _offset: u64, _len: u64) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn exists(&self, _key: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn list(&self, _prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        Ok(Vec::new())
    }
    fn delete(&self, _key: &str) -> Result<(), StoreError> {
        Ok(())
    }
    fn describe(&self) -> String {
        self.root.display().to_string()
    }
}

/// An in-memory store that counts writes: the test fake.
#[derive(Debug, Default)]
pub struct MemStore {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    puts: Mutex<u64>,
}

impl MemStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes so far.
    pub fn puts(&self) -> u64 {
        self.puts.lock().map(|p| *p).unwrap_or(0)
    }
}

impl ObjectStore for MemStore {
    fn put(&self, key: &str, _bytes: &[u8]) -> Result<(), StoreError> {
        let _ = &self.objects;
        Err(StoreError::Backend(format!("not implemented: {key}")))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn get_range(&self, key: &str, _offset: u64, _len: u64) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn exists(&self, _key: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn list(&self, _prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        Ok(Vec::new())
    }
    fn delete(&self, _key: &str) -> Result<(), StoreError> {
        Ok(())
    }
    fn describe(&self) -> String {
        "memory".into()
    }
}

/// Opens a destination: `s3` is the S3 store configured by the environment
/// (`OPENTYCOON_S3_*`); anything else is a local directory.
pub fn open(dest: &str) -> Result<Box<dyn ObjectStore>, StoreError> {
    if dest == "s3" {
        return Ok(Box::new(super::s3::S3Store::from_env()?));
    }
    Ok(Box::new(LocalStore::new(dest)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The behaviour every backend must have.
    pub(crate) fn conformance(store: &dyn ObjectStore) {
        assert!(!store.exists("configs/a.yaml").unwrap());
        assert!(matches!(
            store.get("configs/a.yaml"),
            Err(StoreError::NotFound(_))
        ));
        store.put("configs/a.yaml", b"hello").unwrap();
        store.put("replays/f/r/p-s000.otr", b"0123456789").unwrap();
        store.put("replays/f/r/p-s001.otr", b"").unwrap();
        assert!(store.exists("configs/a.yaml").unwrap());
        assert_eq!(store.get("configs/a.yaml").unwrap(), b"hello");
        assert_eq!(
            store.get_range("replays/f/r/p-s000.otr", 3, 4).unwrap(),
            b"3456"
        );
        store.put("configs/a.yaml", b"bye").unwrap();
        assert_eq!(store.get("configs/a.yaml").unwrap(), b"bye");
        assert_eq!(
            store.list("replays/").unwrap(),
            vec![
                ("replays/f/r/p-s000.otr".to_owned(), 10),
                ("replays/f/r/p-s001.otr".to_owned(), 0)
            ]
        );
        assert_eq!(store.list("configs/").unwrap().len(), 1);
        assert!(store.list("manifests/").unwrap().is_empty());
        store.delete("configs/a.yaml").unwrap();
        store.delete("configs/a.yaml").unwrap();
        assert!(!store.exists("configs/a.yaml").unwrap());
        for bad in ["", "/abs", "a/../b", "a//b", "./a", "a/"] {
            assert!(
                matches!(store.put(bad, b"x"), Err(StoreError::BadKey(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_local_store_conforms() {
        let dir = std::env::temp_dir().join(format!("otr-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        conformance(&LocalStore::new(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_memory_store_conforms_and_counts_writes() {
        let store = MemStore::new();
        conformance(&store);
        assert_eq!(store.puts(), 4);
    }

    #[test]
    fn s3_without_its_environment_names_the_missing_variables() {
        if std::env::var_os("OPENTYCOON_S3_ENDPOINT").is_some() {
            return; // configured on this machine; nothing to prove
        }
        let err = open("s3").unwrap_err().to_string();
        assert!(err.contains("OPENTYCOON_S3_ENDPOINT"), "{err}");
    }
}
