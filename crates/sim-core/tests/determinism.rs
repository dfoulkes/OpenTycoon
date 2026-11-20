//! Determinism: same seed + config ⇒ identical state hash after N ticks (docs/engineering.md).
//! Cross-platform: CI runs `sim-cli run` on Linux x86_64 and macOS aarch64 and compares the
//! printed hashes (.github/workflows/ci.yml), so no constant here needs pinning per feature.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use proptest::prelude::*;
use sim_core::World;

fn run(seed: u64, ticks: u64) -> u64 {
    let mut world = World::new(common::shipped_config(), seed, 2);
    for _ in 0..ticks {
        world.tick();
    }
    world.state_hash()
}

#[test]
fn same_seed_and_config_give_the_same_hash_after_a_full_day() {
    assert_eq!(run(42, 8_640), run(42, 8_640));
}

#[test]
fn different_seeds_give_different_hashes() {
    assert_ne!(run(1, 100), run(2, 100));
}

#[test]
fn hash_changes_as_time_passes() {
    assert_ne!(run(42, 1), run(42, 2));
}

#[test]
fn a_cloned_world_stays_in_lockstep() {
    let mut a = World::new(common::shipped_config(), 7, 2);
    for _ in 0..500 {
        a.tick();
    }
    let mut b = a.clone();
    for _ in 0..5_000 {
        a.tick();
        b.tick();
    }
    assert_eq!(a.state_hash(), b.state_hash());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn any_seed_replays_identically(seed in any::<u64>(), ticks in 0u64..2_000) {
        prop_assert_eq!(run(seed, ticks), run(seed, ticks));
    }
}
