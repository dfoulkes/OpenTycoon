//! # OpenTycoon sim-core
//!
//! The whole game, headless and deterministic: no I/O, no wall clock, no OS randomness, no
//! floats for money. Servers, the balance harness and tests drive it through
//! [`world::World`]. The spec is `docs/spec.md`; rulings are cited as R1–R61.
//!
//! One module per domain, each with a single owner (see docs/architecture.md):
//!
//! | Module | Owns |
//! |---|---|
//! | [`config`] | YAML loading, JSON Schema validation, typed config |
//! | [`money`] | `Money` (integer pence) and `Rate` (basis points) |
//! | [`clock`] | ticks, days, the +5-year step, trading windows |
//! | [`economy`] | ledger, settlement, overdraft, solvency, net worth |
//! | [`land`] | map, blocks, sets, footprints, leases, land rent |
//! | [`auctions`] | the one-at-a-time auction state machine |
//! | [`businesses`] | buildings, types, prices, orders, stock |
//! | [`utilities`] | utility bills and holders |
//! | [`population`] | cohort model and arrivals |
//! | [`customers`] | shoppers, scoring, shop choice |
//! | [`ai`] | AI opponents |
//! | [`events`] | history, random events, elections |
//! | [`world`] | owns all state; the fixed tick order |

// clippy::all (deny) and clippy::pedantic (warn) come from [workspace.lints] in the root
// Cargo.toml so every crate shares them. Floats are banned here except where a module opts out
// with a reason (customer scoring, config decimal conversion).
#![deny(clippy::float_arithmetic)]
#![deny(missing_docs)]

pub mod ai;
pub mod auctions;
pub mod businesses;
pub mod clock;
pub mod config;
pub mod customers;
pub mod economy;
pub mod events;
pub mod hash;
pub mod land;
pub mod money;
pub mod population;
pub mod rng;
pub mod utilities;
pub mod world;

pub use config::{Config, ConfigError, ConfigErrors, ConfigSources};
pub use money::{Money, Rate};
pub use world::World;
