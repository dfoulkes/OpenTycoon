//! Records the sim build's git commit for replay headers (`replay::fingerprint::GIT_COMMIT`).
//! `OPENTYCOON_GIT_COMMIT` in the environment overrides it (e.g. for builds from a tarball);
//! outside a git checkout it is `unknown`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-env-changed=OPENTYCOON_GIT_COMMIT");
    let commit = std::env::var("OPENTYCOON_GIT_COMMIT")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .filter(|c| c.len() == 40 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=OPENTYCOON_GIT_COMMIT={commit}");
    // Rebuild when HEAD moves: HEAD itself, the branch it points at, and packed refs.
    for path in [
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["symbolic-ref", "-q", "HEAD"])
            .and_then(|r| git(&["rev-parse", "--git-path", r.as_str()])),
        git(&["rev-parse", "--git-path", "packed-refs"]),
    ]
    .into_iter()
    .flatten()
    {
        println!("cargo:rerun-if-changed={path}");
    }
}
