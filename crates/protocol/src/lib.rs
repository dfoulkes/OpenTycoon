//! OpenTycoon wire protocol: messages shared by the server and the Godot client (§10a).
//!
//! Placeholder until M2. The shape is fixed now so both sides can depend on it: clients send
//! [`ClientMessage`]s (commands only; the server is authoritative, R40), the server sends
//! [`ServerMessage`]s (snapshots and per-tick deltas at two levels of detail).

use serde::{Deserialize, Serialize};

/// Protocol version; bump on any breaking change.
pub const PROTOCOL_VERSION: u32 = 0;

/// Client → server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message on a connection.
    Hello {
        /// Client's protocol version.
        protocol: u32,
        /// One-time token (single-player) or invite code (friends).
        token: String,
    },
}

/// Server → client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Welcome {
        /// Server's protocol version.
        protocol: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips_as_tagged_json() {
        let msg = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            token: "t".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"hello","protocol":0,"token":"t"}"#);
        assert_eq!(serde_json::from_str::<ClientMessage>(&json).unwrap(), msg);
    }
}
