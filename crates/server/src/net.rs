//! The WebSocket server: one client, on `127.0.0.1` only (R44), plain `tungstenite` on a std
//! thread (no async runtime). It answers `hello`, `new_game`, `set_speed` and `leave`, steps the
//! sim at the current speed's rate from `server.yaml`, and sends a delta at most
//! `deltas_per_second` times a second (`docs/inspector-protocol.md`).
//!
//! The wall clock is read here and only here: it decides how many ticks to run, never what a
//! tick does, so a game played through the server is the same game as one run headless.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use sim_core::{Config, ConfigErrors, ConfigSources};
use tungstenite::{Message, WebSocket};

use crate::config::ServerConfig;
use crate::session::Session;

/// Why the server stopped or could not start.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// `server.yaml` is missing or invalid.
    #[error("refusing to start: {0}")]
    Config(ConfigErrors),
    /// The token does not follow `server.yaml` `token`.
    #[error("refusing to start: the token must be {0} hex characters")]
    Token(u32),
    /// A socket error.
    #[error("socket: {0}")]
    Io(#[from] std::io::Error),
    /// A WebSocket protocol error.
    #[error("websocket: {0}")]
    Ws(#[from] tungstenite::Error),
}

/// The wall clock. The sim never reads it (clippy bans `Instant::now` workspace-wide); the
/// server reads it only to pace ticks and deltas.
#[allow(
    clippy::disallowed_methods,
    reason = "real-time pacing of the server loop; never reaches the sim"
)]
fn now() -> Instant {
    Instant::now()
}

/// A bound server, not yet serving.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    config_dir: PathBuf,
    settings: ServerConfig,
    token: String,
}

impl Server {
    /// Loads `server.yaml` from `config_dir`, checks `token` against its rules and binds
    /// `127.0.0.1` on a port the OS chooses. The sim config is loaded per game, at `new_game`.
    pub fn bind(config_dir: &Path, token: &str) -> Result<Server, ServerError> {
        let settings = ServerConfig::read_dir(config_dir).map_err(ServerError::Config)?;
        if !settings.token.accepts(token) {
            return Err(ServerError::Token(settings.token.length));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        Ok(Server {
            listener,
            config_dir: config_dir.to_owned(),
            settings,
            token: token.to_owned(),
        })
    }

    /// Where it listens.
    pub fn local_addr(&self) -> Result<SocketAddr, ServerError> {
        Ok(self.listener.local_addr()?)
    }

    /// Accepts one client and serves it until it leaves or disconnects.
    pub fn serve_one(self) -> Result<(), ServerError> {
        let (stream, _) = self.listener.accept()?;
        stream.set_nodelay(true)?;
        let ws = tungstenite::accept(stream).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(e) => ServerError::Ws(e),
            tungstenite::HandshakeError::Interrupted(_) => {
                ServerError::Io(std::io::Error::from(ErrorKind::WouldBlock))
            }
        })?;
        ws.get_ref().set_nonblocking(true)?;
        Connection {
            ws,
            config_dir: self.config_dir,
            settings: self.settings,
            token: self.token,
            greeted: false,
            session: None,
            pacer: Pacer::default(),
            last_delta: now(),
            dirty: false,
        }
        .run()
    }
}

/// Binds a server (see [`Server::bind`]) and serves one client on a new thread. Returns the
/// address to connect to and the thread.
pub fn spawn(
    config_dir: &Path,
    token: &str,
) -> Result<(SocketAddr, JoinHandle<Result<(), ServerError>>), ServerError> {
    let server = Server::bind(config_dir, token)?;
    let addr = server.local_addr()?;
    let handle = std::thread::Builder::new()
        .name("opentycoon-server".into())
        .spawn(move || server.serve_one())?;
    Ok((addr, handle))
}

/// How many ticks are due at the current speed.
#[derive(Debug, Clone, Copy, Default)]
struct Pacer {
    since: Option<Instant>,
    done: u64,
    rate: f64,
}

impl Pacer {
    /// Starts counting at `rate` ticks per second from `at`.
    fn start(&mut self, rate: f64, at: Instant) {
        *self = Pacer {
            since: (rate > 0.0).then_some(at),
            done: 0,
            rate,
        };
    }

    /// Ticks to run now. More than `max_behind` seconds behind, the backlog is dropped.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a non-negative, floored tick count far below 2^52"
    )]
    fn due(&mut self, at: Instant, max_behind: f64) -> u64 {
        let Some(since) = self.since else {
            return 0;
        };
        let want = (at.duration_since(since).as_secs_f64() * self.rate).floor() as u64;
        let due = want.saturating_sub(self.done);
        if due as f64 > self.rate * max_behind {
            self.start(self.rate, at);
            return 0;
        }
        self.done += due;
        due
    }
}

/// One client connection.
struct Connection {
    ws: WebSocket<TcpStream>,
    config_dir: PathBuf,
    settings: ServerConfig,
    token: String,
    greeted: bool,
    session: Option<Session>,
    pacer: Pacer,
    last_delta: Instant,
    dirty: bool,
}

/// What to do after handling a message.
enum Next {
    Continue,
    Close,
}

impl Connection {
    fn run(mut self) -> Result<(), ServerError> {
        let interval = Duration::from_secs_f64(1.0 / self.settings.deltas_per_second);
        // Poll a few times per delta so commands are answered promptly.
        let nap = interval / 4;
        loop {
            loop {
                match self.ws.read() {
                    Ok(Message::Text(text)) => {
                        if let Next::Close = self.handle(text.as_str())? {
                            return self.close();
                        }
                    }
                    Ok(Message::Close(_)) => {
                        self.drain();
                        return Ok(());
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(
                        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
                    ) => {
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            let at = now();
            if let Some(session) = self.session.as_mut() {
                let due = self.pacer.due(at, self.settings.max_catch_up_seconds);
                for _ in 0..due {
                    if session.is_over() {
                        break;
                    }
                    session.step();
                    self.dirty = true;
                }
                if self.dirty && at.duration_since(self.last_delta) >= interval {
                    let delta = ServerMessage::Delta(session.delta());
                    self.last_delta = at;
                    self.dirty = false;
                    send(&mut self.ws, &delta)?;
                }
                if let Some(over) = session.game_over() {
                    send(&mut self.ws, &ServerMessage::GameOver(over))?;
                }
            }
            flush(&mut self.ws)?;
            std::thread::sleep(nap);
        }
    }

    fn handle(&mut self, text: &str) -> Result<Next, ServerError> {
        let msg: ClientMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                self.error(&format!("bad message: {e}"))?;
                return Ok(Next::Continue);
            }
        };
        if !self.greeted {
            return self.hello(msg);
        }
        match msg {
            ClientMessage::Hello { .. } => self.error("already said hello")?,
            ClientMessage::NewGame {
                seed,
                economy,
                players,
            } => self.new_game(seed, economy.as_deref(), players)?,
            ClientMessage::SetSpeed { speed } => self.set_speed(speed)?,
            ClientMessage::Leave => return Ok(Next::Close),
        }
        Ok(Next::Continue)
    }

    fn hello(&mut self, msg: ClientMessage) -> Result<Next, ServerError> {
        match msg {
            ClientMessage::Hello { protocol, .. } if protocol != PROTOCOL_VERSION => {
                self.error(&format!(
                    "protocol {protocol} is not supported; this server speaks {PROTOCOL_VERSION}"
                ))?;
                Ok(Next::Close)
            }
            ClientMessage::Hello { token, .. } if token != self.token => {
                self.error("bad token")?;
                Ok(Next::Close)
            }
            ClientMessage::Hello { .. } => {
                self.greeted = true;
                send(
                    &mut self.ws,
                    &ServerMessage::Welcome {
                        protocol: PROTOCOL_VERSION,
                    },
                )?;
                Ok(Next::Continue)
            }
            _ => {
                self.error("say hello first")?;
                Ok(Next::Close)
            }
        }
    }

    fn new_game(
        &mut self,
        seed: u64,
        economy: Option<&str>,
        players: u8,
    ) -> Result<(), ServerError> {
        let config = match load_config(&self.config_dir, economy) {
            Ok(c) => c,
            Err(message) => return self.error(&message),
        };
        let mut session = match Session::new(config, seed, players, &self.settings) {
            Ok(s) => s,
            Err(e) => return self.error(&e.to_string()),
        };
        let snapshot = ServerMessage::Snapshot(session.snapshot());
        send(&mut self.ws, &snapshot)?;
        let at = now();
        self.pacer
            .start(self.settings.ticks_per_second_at(session.speed()), at);
        self.last_delta = at;
        self.dirty = false;
        self.session = Some(session);
        Ok(())
    }

    fn set_speed(&mut self, speed: u8) -> Result<(), ServerError> {
        if speed > self.settings.max_speed() {
            return self.error(&format!(
                "speed must be 0 to {}, got {speed}",
                self.settings.max_speed()
            ));
        }
        let Some(session) = self.session.as_mut() else {
            return self.error("no game: send new_game first");
        };
        session.set_speed(speed);
        self.pacer
            .start(self.settings.ticks_per_second_at(speed), now());
        // The client hears about the new speed in the next delta, even when paused.
        self.dirty = true;
        Ok(())
    }

    fn error(&mut self, message: &str) -> Result<(), ServerError> {
        send(
            &mut self.ws,
            &ServerMessage::Error {
                message: message.to_owned(),
            },
        )
    }

    /// Starts the close handshake and waits (briefly) for the client's reply.
    fn close(mut self) -> Result<(), ServerError> {
        match self.ws.close(None) {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
        self.drain();
        Ok(())
    }

    /// Reads until the connection is closed, for at most a second.
    fn drain(mut self) {
        let deadline = now() + Duration::from_secs(1);
        while now() < deadline {
            match self.ws.read() {
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => {
                    let _ = self.ws.flush();
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return,
            }
        }
    }
}

/// Queues a message and tries to flush it; a full socket buffer is not an error (the rest goes
/// out on a later flush).
fn send(ws: &mut WebSocket<TcpStream>, msg: &ServerMessage) -> Result<(), ServerError> {
    let text = serde_json::to_string(msg).map_err(std::io::Error::other)?;
    match ws.send(Message::text(text)) {
        Ok(()) => Ok(()),
        Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn flush(ws: &mut WebSocket<TcpStream>) -> Result<(), ServerError> {
    match ws.flush() {
        Ok(()) => Ok(()),
        Err(tungstenite::Error::Io(e)) if e.kind() == ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// The sim config from `config_dir`, with `economy` (a path to an `economy.yaml`) in place of
/// the shipped one if given. Every problem is in the message.
pub fn load_config(config_dir: &Path, economy: Option<&str>) -> Result<Config, String> {
    let mut sources = ConfigSources::read_dir(config_dir).map_err(|e| e.to_string())?;
    if let Some(path) = economy.filter(|p| !p.is_empty()) {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("economy file {path}: {e}"))?;
        sources.insert("economy.yaml", text);
    }
    Config::load(&sources).map_err(|e| e.to_string())
}
