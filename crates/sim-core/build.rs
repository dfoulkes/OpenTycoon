//! Warns at build time when `docs/spec.md` is behind the canonical spec in Dan's vault.
//!
//! Only where the vault exists (Dan's Mac); silent elsewhere, including CI. It never fails the
//! build: the fix is `cargo spec-sync`. The comparison skips the snapshot's header line and the
//! blank line after it (see `crates/sim-cli/src/spec.rs`).

use std::path::{Path, PathBuf};

const HEADER_PREFIX: &str = "> Snapshot of the canonical spec in Dan's vault";

fn vault_spec() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENTYCOON_VAULT_SPEC") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var("HOME").ok()?;
    Some(Path::new(&home).join("Vaults/Homelab/Projects/opentycoon/spec.md"))
}

fn body(snapshot: &str) -> &str {
    match snapshot.strip_prefix(HEADER_PREFIX) {
        Some(rest) => rest
            .split_once('\n')
            .map_or("", |(_, after)| after.strip_prefix('\n').unwrap_or(after)),
        None => snapshot,
    }
}

fn main() {
    println!("cargo::rerun-if-env-changed=OPENTYCOON_VAULT_SPEC");
    let repo_spec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec.md");
    println!("cargo::rerun-if-changed={}", repo_spec.display());
    let Some(vault) = vault_spec().filter(|p| p.is_file()) else {
        return;
    };
    // Only watch the vault file when it exists, or cargo would rerun this on every build.
    println!("cargo::rerun-if-changed={}", vault.display());
    let (Ok(vault_text), Ok(repo_text)) = (
        std::fs::read_to_string(&vault),
        std::fs::read_to_string(&repo_spec),
    ) else {
        return;
    };
    if body(&repo_text) != vault_text {
        println!(
            "cargo::warning=docs/spec.md is behind the vault spec ({}). Agents and tests read the \
             repo copy: run `cargo spec-sync`, then `cargo trace`.",
            vault.display()
        );
    }
}
