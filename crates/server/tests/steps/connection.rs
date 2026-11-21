//! Steps for `inspector/connection.feature`: a real server on a real socket.

use std::net::SocketAddr;
use std::thread::JoinHandle;

use cucumber::{given, then, when};
use protocol::{ClientMessage, ServerMessage};
use server::ServerError;

use crate::ServerWorld;
use crate::common::{self, Client};

/// Connection scenario state.
#[derive(Debug, Default)]
pub struct NetState {
    addr: Option<SocketAddr>,
    token: String,
    thread: Option<JoinHandle<Result<(), ServerError>>>,
    client: Option<ClientBox>,
    last_tick: u64,
}

/// The client, boxed so `NetState` can be `Debug`.
pub struct ClientBox(Client);

impl std::fmt::Debug for ClientBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Client")
    }
}

impl NetState {
    fn client(&mut self) -> &mut Client {
        &mut self.client.as_mut().expect("a client is connected").0
    }
}

#[given("an inspector server started on the shipped config with a fresh token")]
fn start(w: &mut ServerWorld) {
    let token = common::token();
    let (addr, thread) = server::spawn(&common::config_dir(), &token).unwrap();
    w.net = NetState {
        addr: Some(addr),
        token,
        thread: Some(thread),
        ..NetState::default()
    };
}

#[then("the server listens on 127.0.0.1 on a port the OS chose")]
fn localhost(w: &mut ServerWorld) {
    let addr = w.net.addr.unwrap();
    assert!(addr.ip().is_loopback(), "{addr}");
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_ne!(addr.port(), 0);
    // Let the waiting server go.
    w.net.client = Some(ClientBox(Client::connect(addr)));
    w.net.client().send(&ClientMessage::Leave);
}

fn hello(w: &mut ServerWorld, protocol: u32, token: String) {
    let mut client = Client::connect(w.net.addr.unwrap());
    client.send(&ClientMessage::Hello { protocol, token });
    w.net.client = Some(ClientBox(client));
}

#[when(expr = "a client connects and says hello with protocol {int} and the token")]
fn hello_right(w: &mut ServerWorld, protocol: u32) {
    let token = w.net.token.clone();
    hello(w, protocol, token);
}

#[when(expr = "a client connects and says hello with protocol {int} and the token {string}")]
fn hello_token(w: &mut ServerWorld, protocol: u32, token: String) {
    hello(w, protocol, token);
}

#[then(expr = "the client receives \"welcome\" with protocol {int}")]
fn welcome(w: &mut ServerWorld, protocol: u32) {
    assert_eq!(
        w.net.client().msg(),
        ServerMessage::Welcome { protocol },
        "welcome"
    );
}

#[then(expr = "the client receives an error naming {string}")]
fn error(w: &mut ServerWorld, text: String) {
    let message = w.net.client().until(1_000, |m| match m {
        ServerMessage::Error { message } => Some(message.clone()),
        _ => None,
    });
    assert!(message.contains(&text), "{message}");
}

#[then("the connection is closed")]
fn closed(w: &mut ServerWorld) {
    assert!(w.net.client().until_closed(100_000), "still open");
}

#[when(expr = "the client sends new_game with seed {int}")]
fn new_game(w: &mut ServerWorld, seed: u64) {
    let c = w.net.client();
    assert!(matches!(c.msg(), ServerMessage::Welcome { .. }));
    c.send(&ClientMessage::NewGame {
        seed,
        economy: None,
        players: protocol::DEFAULT_PLAYERS,
    });
}

#[then(expr = "the client receives a snapshot of {int} lots at tick {int}")]
fn snapshot(w: &mut ServerWorld, lots: usize, tick: u64) {
    let snap = w.net.client().until(100, |m| match m {
        ServerMessage::Snapshot(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(snap.lots.len(), lots);
    assert_eq!(snap.tick, tick);
    w.net.last_tick = snap.tick;
}

#[when(expr = "the client sends set_speed {int}")]
fn set_speed(w: &mut ServerWorld, speed: u8) {
    w.net.client().send(&ClientMessage::SetSpeed { speed });
}

#[then(expr = "the client receives a delta at speed {int} with a later tick")]
fn delta(w: &mut ServerWorld, speed: u8) {
    let after = w.net.last_tick;
    let tick = w.net.client().until(10_000, |m| match m {
        ServerMessage::Delta(d) if d.clock.speed == speed && d.tick > after => Some(d.tick),
        _ => None,
    });
    w.net.last_tick = tick;
}

#[when("the client sends leave")]
fn leave(w: &mut ServerWorld) {
    w.net.client().send(&ClientMessage::Leave);
}

#[then("the server thread ends cleanly")]
fn thread_ends(w: &mut ServerWorld) {
    let result = w.net.thread.take().unwrap().join().unwrap();
    assert!(result.is_ok(), "{result:?}");
}
