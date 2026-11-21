//! The server starts on the shipped config and refuses a config directory without
//! `economy.yaml` (R71, §10b), naming the file.
#![allow(clippy::unwrap_used, reason = "test code")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_config() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config")
}

fn run(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_opentycoon-server"))
        .arg("--config")
        .arg(dir)
        .output()
        .unwrap()
}

#[test]
fn starts_on_the_shipped_config() {
    let out = run(&repo_config());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
}

#[test]
fn refuses_to_start_without_economy_yaml() {
    let dir = std::env::temp_dir().join(format!("server-no-economy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(repo_config()).unwrap().flatten() {
        let name = entry.file_name();
        if entry.path().is_file() && name != "economy.yaml" {
            std::fs::copy(entry.path(), dir.join(name)).unwrap();
        }
    }
    let out = run(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("refusing to start") && err.contains("economy.yaml"),
        "{err}"
    );
}
