//! OpenTycoon Godot extension (godot-rust / gdext, R47).
//!
//! Loaded by `client/opentycoon.gdextension`. Will hold the network client and walker
//! interpolation (M3/M4). For now: one `Hello` node that proves the bridge works.
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
        reason = "Godot-callable functions take arguments by value"
    )]
    fn greet(name: GString) -> GString {
        GString::from(format!("Hello, {name}, from OpenTycoon Rust").as_str())
    }
}
