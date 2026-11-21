//! Wire-size budgets (`perf/budgets.yaml` `wire`, docs/perf.md "Wire sizes"): the snapshot and
//! the largest delta on the full board, seed 42, four AI seats. Sizes are deterministic (the
//! same game gives the same bytes on any machine), so unlike the timing budgets they run in
//! `cargo test`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    missing_docs,
    reason = "test code"
)]

mod common;

use protocol::ServerMessage;
use server::Session;

/// Bytes of a message as sent.
fn bytes(msg: &ServerMessage) -> usize {
    serde_json::to_string(msg).unwrap().len()
}

/// `wire.<name>.limit` from `perf/budgets.yaml`.
fn limit(name: &str) -> f64 {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../perf/budgets.yaml");
    let yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    yaml["wire"][name]["limit"]
        .as_f64()
        .unwrap_or_else(|| panic!("perf/budgets.yaml has no wire.{name}.limit"))
}

/// The game the budgets measure: three game days of seed 42, a snapshot at the start and at
/// the end (when the board is busiest), and a delta at the fastest speed's cadence.
fn measure() -> (usize, usize) {
    let server = common::shipped_server();
    let mut s = Session::new(common::shipped_config(), 42, 4, &server).unwrap();
    let mut snapshot = bytes(&ServerMessage::Snapshot(s.snapshot()));
    let fastest = server.ticks_per_second[server.ticks_per_second.len() - 1];
    let every = (fastest / server.deltas_per_second).ceil().max(1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive tick count"
    )]
    let every = every as u64;
    let days = 3 * u64::from(s.world().clock().rules().ticks_per_day());
    let mut delta = 0;
    for t in 1..=days {
        s.step();
        if t % every == 0 {
            delta = delta.max(bytes(&ServerMessage::Delta(s.delta())));
        }
    }
    snapshot = snapshot.max(bytes(&ServerMessage::Snapshot(s.snapshot())));
    (snapshot, delta)
}

#[test]
fn snapshot_and_delta_fit_their_wire_budgets() {
    let (snapshot, delta) = measure();
    println!("snapshot_bytes={snapshot} delta_bytes={delta}");
    assert!(
        snapshot as f64 <= limit("snapshot_bytes"),
        "snapshot {snapshot} bytes is over budget"
    );
    assert!(
        delta as f64 <= limit("delta_bytes"),
        "largest delta {delta} bytes is over budget"
    );
}
