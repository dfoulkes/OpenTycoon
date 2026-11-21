//! OpenTycoon's authoritative game server (R40, R44) for the game inspector (spec §10d, R88).
//!
//! - [`config`]: `config/server.yaml` (pacing, wire limits, token rules).
//! - [`view`]: pure functions from a `World` to the protocol's shapes.
//! - [`session`]: one game, every seat an AI, as a snapshot then deltas.
//! - [`net`]: the WebSocket server on `127.0.0.1` (one client, std threads).
//!
//! The wire contract is `docs/inspector-protocol.md` and the `protocol` crate.

pub mod config;
pub mod net;
pub mod session;
pub mod view;

pub use config::ServerConfig;
pub use net::{Server, ServerError, load_config, spawn};
pub use session::{Session, SessionError};
