//! Writing a run to an object store. Replays are packed into **segments**: per part (a batch
//! chunk or a study model), games are concatenated in game order into objects of about
//! [`WriterOptions::segment_bytes`], so a 600,000-game study is a few thousand objects, not
//! 600,000 tiny requests. The manifest row of each game records its segment, offset and length,
//! so one game is one ranged GET. Config snapshots are written once per distinct hash.

use std::collections::BTreeSet;

use super::format::{Replay, encode};
use super::manifest::{ManifestError, ManifestRow, Weights, row_for, to_parquet};
use super::snapshot::{self, ConfigSnapshot};
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

/// A segment being packed: its envelopes and, per game, the offset and length.
struct Segment<'g> {
    fingerprint: &'g str,
    bytes: Vec<u8>,
    games: Vec<(&'g Replay, u64, u64)>,
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
        if self.configs.contains(&snap.hash) {
            return Ok(false);
        }
        let key = snapshot::key(&snap.hash);
        let written = if self.store.exists(&key)? {
            self.stats.configs_present += 1;
            false
        } else {
            self.put(&key, snap.yaml.as_bytes())?;
            self.stats.configs_written += 1;
            true
        };
        self.configs.insert(snap.hash.clone());
        Ok(written)
    }

    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<(), WriteError> {
        self.store.put(key, bytes)?;
        self.stats.objects += 1;
        self.stats.bytes += bytes.len() as u64;
        Ok(())
    }

    /// Writes one part: `games` (in game order) packed into segments, and the manifest
    /// `manifests/<run_id>/<part>.parquet`. Writing the same part again replaces it.
    pub fn write_part(
        &mut self,
        part: &str,
        games: &[Replay],
        source: &str,
        model_id: Option<u32>,
    ) -> Result<(), WriteError> {
        // Pack: a segment closes at the size target or when the rules fingerprint changes
        // (the fingerprint is part of its key).
        let mut segments: Vec<Segment<'_>> = Vec::new();
        for game in games {
            let fp = game.header.rules_fingerprint.as_str();
            let bytes = encode(game)?;
            let open = segments
                .last()
                .is_some_and(|s| s.fingerprint == fp && s.bytes.len() < self.opts.segment_bytes);
            if !open {
                segments.push(Segment {
                    fingerprint: fp,
                    bytes: Vec::new(),
                    games: Vec::new(),
                });
            }
            if let Some(seg) = segments.last_mut() {
                seg.games
                    .push((game, seg.bytes.len() as u64, bytes.len() as u64));
                seg.bytes.extend_from_slice(&bytes);
            }
            self.stats.games += 1;
            self.stats.replay_bytes += bytes.len() as u64;
        }
        let mut rows: Vec<ManifestRow> = Vec::with_capacity(games.len());
        for (n, seg) in segments.iter().enumerate() {
            let key = format!(
                "replays/{}/{}/{part}-s{n:03}.otr",
                seg.fingerprint, self.run_id
            );
            self.put(&key, &seg.bytes)?;
            self.stats.segments += 1;
            rows.extend(seg.games.iter().map(|(game, offset, len)| {
                row_for(
                    game,
                    &self.opts.weights,
                    source,
                    model_id,
                    &key,
                    *offset,
                    *len,
                )
            }));
        }
        let manifest = to_parquet(&rows)?;
        self.put(
            &format!("manifests/{}/{part}.parquet", self.run_id),
            &manifest,
        )?;
        self.stats.manifest_parts += 1;
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
