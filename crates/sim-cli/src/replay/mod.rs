//! The game recorder: every game's inputs, recorded so a learning AI can be trained from them
//! later (docs/replay.md).
//!
//! A replay is a header (build, rules fingerprint, config hash, seed, seats), the command log,
//! a checkpoint at every midnight and the outcome. Replay files never go in the repository:
//! they live in an object store (S3-compatible, or a local directory with the same layout),
//! indexed by Parquet manifests.
//!
//! - [`format`]: the `.otr` envelope and payload.
//! - [`record`]: recording a harness game through `World::tick_with`.
//! - [`fingerprint`]: the build's rules fingerprint and git commit.
//! - [`snapshot`]: config snapshots, stored once per distinct config.
//! - [`store`]: the object store trait, the local-directory backend and an in-memory fake.
//! - [`s3`]: the S3 backend (endpoint, bucket and keys from the environment).
//! - [`manifest`]: the Parquet manifest, game weights and the rules-weight policy.
//! - [`writer`]: packs a run's replays into segment objects and writes its manifest parts.
//! - [`verify`]: re-simulates a replay and compares every midnight.
//! - [`extract`]: the stub feature extractor.
//! - [`load`]: reads replays back out of a store, by file or by manifest.
//!
//! Recording never changes a game: the recorder only reads the world.

pub mod extract;
pub mod fingerprint;
pub mod format;
pub mod load;
pub mod manifest;
pub mod parquet_table;
pub mod record;
pub mod s3;
pub mod snapshot;
pub mod store;
pub mod verify;
pub mod writer;

pub use format::{
    Checkpoint, CommandRecord, FORMAT_VERSION, Header, Origin, Outcome, Replay, Seat,
    SeatCheckpoint, SeatKind, SeatOutcome, Winner, decode_all, decode_one, encode,
};
pub use manifest::{ManifestRow, RulesWeight, Weights};
pub use record::{RecordContext, play_recorded, seats_of};
pub use store::{LocalStore, MemStore, ObjectStore, StoreError};
pub use verify::{VerifyError, verify};
pub use writer::{RunWriter, WriteStats, WriterOptions};
