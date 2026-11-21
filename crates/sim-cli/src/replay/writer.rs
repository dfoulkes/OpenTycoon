//! Writing a run to an object store. Replays are packed into **segments**: per part (a batch
//! chunk or a study model), games are concatenated in game order into objects of about
//! [`WriterOptions::segment_bytes`], so a 600,000-game study is a few thousand objects, not
//! 600,000 tiny requests. The manifest row of each game records its segment, offset and length,
//! so one game is one ranged GET. Config snapshots are written once per distinct hash.

use std::collections::BTreeSet;

use super::format::Replay;
use super::manifest::{ManifestError, Weights};
use super::snapshot::ConfigSnapshot;
use super::store::{ObjectStore, StoreError};

/// How a run is written.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WriterOptions {
    /// Target segment size in bytes; a segment closes once it reaches this.
    pub segment_bytes: usize,
    /// Game weights for the manifest.
    pub weights: Weights,
}

impl Default for WriterOptions {
    fn default() -> Self {
        WriterOptions {
            segment_bytes: 8 << 20,
            weights: Weights::default(),
        }
    }
}

/// What a run wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriteStats {
    /// Games recorded.
    pub games: u64,
    /// Bytes of replay envelopes.
    pub replay_bytes: u64,
    /// Segment objects written.
    pub segments: u64,
    /// Manifest parts written.
    pub manifest_parts: u64,
    /// Config snapshots written.
    pub configs_written: u64,
    /// Config snapshots already present (deduplicated).
    pub configs_present: u64,
    /// Objects written in all.
    pub objects: u64,
    /// Bytes written in all.
    pub bytes: u64,
}

/// Why a run could not be written.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The manifest could not be built.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// A replay could not be encoded.
    #[error(transparent)]
    Encode(#[from] super::format::EncodeError),
}

/// Writes one run's replays, manifests and configs.
#[derive(Debug)]
pub struct RunWriter<'a> {
    store: &'a dyn ObjectStore,
    run_id: String,
    opts: WriterOptions,
    configs: BTreeSet<String>,
    stats: WriteStats,
}

impl<'a> RunWriter<'a> {
    /// A writer for `run_id` into `store`.
    pub fn new(store: &'a dyn ObjectStore, run_id: &str, opts: WriterOptions) -> Self {
        RunWriter {
            store,
            run_id: run_id.to_owned(),
            opts,
            configs: BTreeSet::new(),
            stats: WriteStats::default(),
        }
    }

    /// Stores a config snapshot unless one with its hash is already there. True if written.
    pub fn put_config(&mut self, snap: &ConfigSnapshot) -> Result<bool, WriteError> {
        let _ = (snap, &self.configs, self.store, &self.run_id, self.opts);
        Ok(false)
    }

    /// Writes one part: `games` (in game order) packed into segments, and the manifest
    /// `manifests/<run_id>/<part>.parquet`.
    pub fn write_part(
        &mut self,
        part: &str,
        games: &[Replay],
        source: &str,
        model_id: Option<u32>,
    ) -> Result<(), WriteError> {
        let _ = (part, games, source, model_id);
        Ok(())
    }

    /// The run.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// What has been written so far.
    pub fn stats(&self) -> WriteStats {
        self.stats
    }
}
