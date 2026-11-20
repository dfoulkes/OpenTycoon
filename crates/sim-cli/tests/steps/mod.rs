//! Step definitions, one module per concern.

pub mod batch;
pub mod cli;
pub mod config;

use std::path::PathBuf;

/// The repo root (the binary runs from here so `config` and `perf/` resolve).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
