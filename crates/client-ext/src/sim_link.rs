//! `SimLink`: the Godot client's only connection to the game (spec §10d, R88; the "Godot side"
//! section of `docs/inspector-protocol.md`).
//!
//! `start_single_player` starts the real server in-process on a thread bound to `127.0.0.1:0`
//! (R44: single player is multiplayer on localhost), connects to it over WebSocket, and sends
//! `hello` then `new_game`. Every frame `_process` reads whatever has arrived without blocking,
//! parses each JSON message into a `Dictionary` and emits it as a signal. Scenes never see the
//! socket: they get dictionaries (and are tested with them, against `fake_sim_link.gd`).
//!
//! JSON integers arrive as `int` (money is integer pence), other numbers as `float`, arrays as
//! `Array`, objects as `Dictionary`, `null` as `null`.

use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::thread::JoinHandle;

use godot::classes::{INode, Node, ProjectSettings};
use godot::prelude::*;
use protocol::{ClientMessage, DEFAULT_PLAYERS, PROTOCOL_VERSION};
use server::{ServerConfig, ServerError};
use tungstenite::{Message, WebSocket};

/// The live link: the socket and the in-process server thread.
struct Link {
    ws: WebSocket<TcpStream>,
    server: Option<ServerThread>,
}

/// The network node a game scene owns. See the module docs.
#[derive(GodotClass)]
#[class(base = Node, init)]
pub struct SimLink {
    base: Base<Node>,
    link: Option<Link>,
}

#[godot_api]
impl SimLink {
    /// The parsed `snapshot`.
    #[signal]
    fn snapshot_received(data: VarDictionary);

    /// A parsed `delta`.
    #[signal]
    fn delta_received(data: VarDictionary);

    /// The parsed `game_over`.
    #[signal]
    fn game_over(data: VarDictionary);

    /// The server refused something, or the link failed.
    #[signal]
    fn link_error(message: GString);

    /// The connection closed.
    #[signal]
    fn disconnected();

    /// Starts the embedded server on `config_dir` (a filesystem path, or `res://`/`user://`),
    /// connects, and sends `hello` then `new_game` with `seed` and the economy at
    /// `economy_path` ("" = the shipped one). Failures are reported through `link_error`.
    #[func]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Godot-callable methods take their arguments by value"
    )]
    fn start_single_player(&mut self, config_dir: GString, economy_path: GString, seed: i64) {
        self.close_link();
        let dir = globalize(&config_dir);
        let economy = economy_path.to_string();
        let economy = (!economy.is_empty()).then(|| globalize(&economy_path).display().to_string());
        let seed = u64::from_ne_bytes(seed.to_ne_bytes());
        match connect(&dir) {
            Ok((ws, server, token)) => {
                self.link = Some(Link {
                    ws,
                    server: Some(server),
                });
                self.send(&ClientMessage::Hello {
                    protocol: PROTOCOL_VERSION,
                    token,
                });
                self.send(&ClientMessage::NewGame {
                    seed,
                    economy,
                    players: DEFAULT_PLAYERS,
                });
            }
            Err(message) => self.emit_error(&message),
        }
    }

    /// `0` pauses; `1`–`4` are the speed steps in `config/server.yaml`.
    #[func]
    fn set_speed(&mut self, speed: i64) {
        match u8::try_from(speed) {
            Ok(speed) => self.send(&ClientMessage::SetSpeed { speed }),
            Err(_) => self.emit_error(&format!("speed must be 0 to 4, got {speed}")),
        }
    }

    /// Ends the game; `disconnected` follows when the server has closed the connection.
    #[func]
    fn leave(&mut self) {
        self.send(&ClientMessage::Leave);
    }

    /// True while connected.
    #[func]
    fn is_connected_to_server(&self) -> bool {
        self.link.is_some()
    }
}

#[godot_api]
impl INode for SimLink {
    fn process(&mut self, _delta: f64) {
        let (messages, closed, error) = self.poll();
        for text in messages {
            self.dispatch(&text);
        }
        if let Some(e) = error {
            self.emit_error(&e);
        }
        if closed {
            self.close_link();
            self.base_mut().emit_signal("disconnected", &[]);
        }
    }

    fn exit_tree(&mut self) {
        if self.link.is_some() {
            self.send(&ClientMessage::Leave);
        }
        self.close_link();
    }
}

impl SimLink {
    /// Reads everything waiting on the socket: the text frames, whether it closed, and an
    /// error if the link failed.
    fn poll(&mut self) -> (Vec<String>, bool, Option<String>) {
        let mut out = Vec::new();
        let Some(link) = self.link.as_mut() else {
            return (out, false, None);
        };
        let _ = link.ws.flush();
        loop {
            match link.ws.read() {
                Ok(Message::Text(t)) => out.push(t.as_str().to_owned()),
                Ok(Message::Close(_)) => {
                    let _ = link.ws.flush();
                    return (out, true, None);
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {
                    return (out, false, None);
                }
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return (out, true, None);
                }
                Err(e) => return (out, true, Some(format!("connection lost: {e}"))),
            }
        }
    }

    /// Emits the signal for one server message.
    fn dispatch(&mut self, text: &str) {
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => return self.emit_error(&format!("bad message from server: {e}")),
        };
        let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let signal = match kind {
            "snapshot" => "snapshot_received",
            "delta" => "delta_received",
            "game_over" => "game_over",
            "error" => {
                let message = value
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("error")
                    .to_owned();
                return self.emit_error(&message);
            }
            // `welcome` needs no action.
            _ => return,
        };
        let data = to_variant(&value);
        self.base_mut().emit_signal(signal, &[data]);
    }

    fn emit_error(&mut self, message: &str) {
        let message = GString::from(message).to_variant();
        self.base_mut().emit_signal("link_error", &[message]);
    }

    fn send(&mut self, msg: &ClientMessage) {
        let Some(link) = self.link.as_mut() else {
            return self.emit_error("not connected: call start_single_player first");
        };
        let text = match serde_json::to_string(msg) {
            Ok(t) => t,
            Err(e) => return self.emit_error(&e.to_string()),
        };
        match link.ws.send(Message::text(text)) {
            Ok(()) => {}
            // Queued; flushed on the next frame.
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => self.emit_error(&format!("send failed: {e}")),
        }
    }

    /// Drops the socket and lets the server thread finish on its own (it ends when the
    /// connection does).
    fn close_link(&mut self) {
        if let Some(mut link) = self.link.take() {
            let _ = link.ws.close(None);
            let _ = link.ws.flush();
            drop(link.server.take());
        }
    }
}

/// A filesystem path for a Godot path (`res://`, `user://`) or a plain one.
fn globalize(path: &GString) -> PathBuf {
    let text = path.to_string();
    if text.starts_with("res://") || text.starts_with("user://") {
        PathBuf::from(
            ProjectSettings::singleton()
                .globalize_path(path)
                .to_string(),
        )
    } else {
        PathBuf::from(text)
    }
}

/// A fresh one-time token of `length` hex characters, from the OS-seeded hasher keys.
fn make_token(length: u32) -> String {
    let mut out = String::new();
    let mut n = 0u64;
    while out.len() < usize::try_from(length).unwrap_or(0) {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(n);
        let _ = write!(out, "{:016x}", h.finish());
        n += 1;
    }
    out.truncate(usize::try_from(length).unwrap_or(0));
    out
}

/// The in-process server's thread.
type ServerThread = JoinHandle<Result<(), ServerError>>;

/// Starts the in-process server and connects to it: the socket, the thread and the token.
fn connect(dir: &std::path::Path) -> Result<(WebSocket<TcpStream>, ServerThread, String), String> {
    let rules = ServerConfig::read_dir(dir)
        .map_err(|e| e.to_string())?
        .token;
    let token = make_token(rules.length);
    let (addr, server): (SocketAddr, _) = server::spawn(dir, &token).map_err(|e| e.to_string())?;
    let stream = TcpStream::connect(addr).map_err(|e| format!("connecting to {addr}: {e}"))?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    let (ws, _) = tungstenite::client(format!("ws://{addr}/"), stream)
        .map_err(|e| format!("websocket handshake: {e}"))?;
    ws.get_ref()
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    Ok((ws, server, token))
}

/// JSON to a Godot value: integers stay `int`, objects become `Dictionary`.
fn to_variant(value: &serde_json::Value) -> Variant {
    use serde_json::Value as J;
    match value {
        J::Null => Variant::nil(),
        J::Bool(b) => b.to_variant(),
        J::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => i.to_variant(),
            (None, Some(f)) => f.to_variant(),
            (None, None) => Variant::nil(),
        },
        J::String(s) => GString::from(s.as_str()).to_variant(),
        J::Array(items) => {
            let mut array = VarArray::new();
            for item in items {
                array.push(&to_variant(item));
            }
            array.to_variant()
        }
        J::Object(map) => {
            let mut dict = VarDictionary::new();
            for (k, v) in map {
                dict.set(&GString::from(k.as_str()).to_variant(), &to_variant(v));
            }
            dict.to_variant()
        }
    }
}
