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
use std::path::{Path, PathBuf};
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
    let clean = !key.is_empty()
        && key
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.contains('\\'));
    if clean {
        Ok(())
    } else {
        Err(StoreError::BadKey(key.to_owned()))
    }
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
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, key: &str) -> Result<PathBuf, StoreError> {
        check_key(key)?;
        Ok(key.split('/').fold(self.root.clone(), |p, seg| p.join(seg)))
    }

    fn io(path: &Path, source: std::io::Error) -> StoreError {
        if source.kind() == std::io::ErrorKind::NotFound {
            StoreError::NotFound(path.display().to_string())
        } else {
            StoreError::Io {
                path: path.display().to_string(),
                source,
            }
        }
    }

    fn walk(&self, dir: &Path, out: &mut Vec<(String, u64)>) -> Result<(), StoreError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Self::io(dir, e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| Self::io(dir, e))?;
            let path = entry.path();
            let meta = entry.metadata().map_err(|e| Self::io(&path, e))?;
            if meta.is_dir() {
                self.walk(&path, out)?;
            } else if let Ok(rel) = path.strip_prefix(&self.root) {
                let key: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((key.join("/"), meta.len()));
            }
        }
        Ok(())
    }
}

impl ObjectStore for LocalStore {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let path = self.path(key)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Self::io(dir, e))?;
        }
        // Write then rename, so a reader never sees half an object.
        let tmp = path.with_extension("partial");
        std::fs::write(&tmp, bytes).map_err(|e| Self::io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| Self::io(&path, e))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.path(key)?;
        std::fs::read(&path).map_err(|e| Self::io(&path, e))
    }

    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>, StoreError> {
        use std::io::{Read, Seek, SeekFrom};
        let path = self.path(key)?;
        let mut f = std::fs::File::open(&path).map_err(|e| Self::io(&path, e))?;
        f.seek(SeekFrom::Start(offset))
            .map_err(|e| Self::io(&path, e))?;
        let mut buf = vec![0; usize::try_from(len).map_err(|_| StoreError::BadKey(key.into()))?];
        f.read_exact(&mut buf).map_err(|e| Self::io(&path, e))?;
        Ok(buf)
    }

    fn exists(&self, key: &str) -> Result<bool, StoreError> {
        Ok(self.path(key)?.is_file())
    }

    fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        let mut out = Vec::new();
        self.walk(&self.root, &mut out)?;
        out.retain(|(k, _)| k.starts_with(prefix) && !k.ends_with(".partial"));
        out.sort();
        Ok(out)
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let path = self.path(key)?;
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Self::io(&path, e)),
            _ => Ok(()),
        }
    }

    fn describe(&self) -> String {
        format!("directory {}", self.root.display())
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
        self.puts.lock().map_or(0, |p| *p)
    }

    fn objects(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, Vec<u8>>>, StoreError> {
        self.objects
            .lock()
            .map_err(|_| StoreError::Backend("memory store poisoned".into()))
    }
}

impl ObjectStore for MemStore {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        check_key(key)?;
        self.objects()?.insert(key.to_owned(), bytes.to_vec());
        if let Ok(mut p) = self.puts.lock() {
            *p += 1;
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        check_key(key)?;
        self.objects()?
            .get(key)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(key.to_owned()))
    }

    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>, StoreError> {
        let all = self.get(key)?;
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let end = start.saturating_add(usize::try_from(len).unwrap_or(usize::MAX));
        all.get(start..end).map(<[u8]>::to_vec).ok_or_else(|| {
            StoreError::Backend(format!("{key}: range {offset}+{len} out of bounds"))
        })
    }

    fn exists(&self, key: &str) -> Result<bool, StoreError> {
        check_key(key)?;
        Ok(self.objects()?.contains_key(key))
    }

    fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        Ok(self
            .objects()?
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.len() as u64))
            .collect())
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        check_key(key)?;
        self.objects()?.remove(key);
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
        assert_eq!(store.list("manifests/").unwrap().len(), 0);
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
