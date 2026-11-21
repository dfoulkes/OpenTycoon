//! OpenTycoon Godot extension (godot-rust / gdext, R47).
//!
//! Loaded by `client/opentycoon.gdextension`. Holds `SimLink`, the network client (which runs
//! the server in-process for single player, R44), and will hold walker
//! interpolation (M3/M4). `Hello` proves the bridge works (the GUT smoke test).
// gdext's entry point is an `unsafe impl`; this crate is the only one allowed unsafe code.
#![allow(
    unsafe_code,
    reason = "gdext's ExtensionLibrary entry point is an unsafe impl"
)]
#![allow(
    clippy::redundant_field_names,
    reason = "fires inside the GodotClass derive expansion, not our code"
)]

use godot::prelude::*;

mod sim_link;

pub use sim_link::SimLink;

struct OpenTycoonExtension;

// SAFETY: gdext requires exactly one ExtensionLibrary impl per cdylib; this is it.
#[gdextension]
unsafe impl ExtensionLibrary for OpenTycoonExtension {}

/// A node that greets from Rust, so GDScript tests can prove the extension loaded.
#[derive(GodotClass)]
#[class(base = Node, init)]
pub struct Hello {
    base: Base<Node>,
}

#[godot_api]
impl Hello {
    /// Returns a greeting naming the extension.
    #[func]
    #[allow(
        clippy::needless_pass_by_value,
        clippy::unused_self,
        reason = "Godot-callable methods take arguments by value; an instance method keeps the \
                  GDScript call simple (ClassDB.instantiate + call)"
    )]
    fn greet(&self, name: GString) -> GString {
        GString::from(format!("Hello, {name}, from OpenTycoon Rust").as_str())
    }
}
