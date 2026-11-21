//! Shared test support: the shipped config and a scripted WebSocket client.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support; not every test binary uses every helper"
)]

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use protocol::{ClientMessage, ServerMessage};
use server::ServerConfig;
use sim_core::{Config, ConfigSources};
use tungstenite::{Message, WebSocket};

/// The repo's `config/`.
pub fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config")
}

/// The shipped sim config.
pub fn shipped_config() -> Config {
    Config::load(&ConfigSources::read_dir(&config_dir()).unwrap()).unwrap()
}

/// The shipped `server.yaml`.
pub fn shipped_server() -> ServerConfig {
    ServerConfig::read_dir(&config_dir()).unwrap()
}

/// A token of the shipped length.
pub fn token() -> String {
    "0123456789abcdef".repeat(2)
}

/// A blocking client with a read timeout, so a broken server fails a test instead of hanging.
pub struct Client {
    ws: WebSocket<TcpStream>,
}

/// What a read got.
#[derive(Debug)]
pub enum Got {
    /// A server message.
    Msg(Box<ServerMessage>, usize),
    /// The connection closed.
    Closed,
}

impl Client {
    /// Connects to `addr`.
    pub fn connect(addr: SocketAddr) -> Client {
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let (ws, _) = tungstenite::client(format!("ws://{addr}/"), stream).unwrap();
        Client { ws }
    }

    /// Sends a message.
    pub fn send(&mut self, msg: &ClientMessage) {
        self.ws
            .send(Message::text(serde_json::to_string(msg).unwrap()))
            .unwrap();
    }

    /// The next server message and its size in bytes, or `Closed`.
    pub fn next(&mut self) -> Got {
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    let msg: ServerMessage = serde_json::from_str(t.as_str()).unwrap();
                    return Got::Msg(Box::new(msg), t.len());
                }
                Ok(Message::Close(_)) => {
                    // Answer the close handshake, then report it.
                    let _ = self.ws.flush();
                    return Got::Closed;
                }
                Ok(_) => {}
                Err(
                    tungstenite::Error::ConnectionClosed
                    | tungstenite::Error::AlreadyClosed
                    | tungstenite::Error::Protocol(_),
                ) => return Got::Closed,
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    return Got::Closed;
                }
                Err(e) => panic!("read failed: {e}"),
            }
        }
    }

    /// The next server message; panics if the connection closed.
    pub fn msg(&mut self) -> ServerMessage {
        match self.next() {
            Got::Msg(m, _) => *m,
            Got::Closed => panic!("connection closed"),
        }
    }

    /// Reads until `pick` returns something, skipping other messages (at most `limit`).
    pub fn until<T>(
        &mut self,
        limit: usize,
        mut pick: impl FnMut(&ServerMessage) -> Option<T>,
    ) -> T {
        for _ in 0..limit {
            let m = self.msg();
            if let Some(t) = pick(&m) {
                return t;
            }
        }
        panic!("no matching message in {limit}");
    }

    /// Reads until the connection closes (at most `limit` messages).
    pub fn until_closed(&mut self, limit: usize) -> bool {
        for _ in 0..limit {
            if let Got::Closed = self.next() {
                return true;
            }
        }
        false
    }
}
