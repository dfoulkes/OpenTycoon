//! Steps that run the built `sim-cli` binary (exit codes, files, JSON on stdout).

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use cucumber::{given, then, when};

use crate::HarnessWorld;

/// The last command-line run and the scenario's scratch directory.
#[derive(Debug, Default)]
pub struct CliState {
    /// Last run.
    pub output: Option<Output>,
    /// Scratch directory for `{tmp}` in arguments.
    pub tmp: Option<PathBuf>,
}

impl CliState {
    fn tmp(&mut self) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        self.tmp
            .get_or_insert_with(|| {
                let dir = std::env::temp_dir().join(format!(
                    "sim-cli-bdd-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&dir).unwrap();
                dir
            })
            .clone()
    }

    /// The scenario's scratch directory (created on first use).
    pub fn tmp_dir(&mut self) -> PathBuf {
        self.tmp()
    }

    fn output(&self) -> &Output {
        self.output.as_ref().expect("sim-cli was not run")
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output().stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output().stderr).into_owned()
    }
}

#[when(expr = "I run sim-cli {string}")]
fn run_cli(w: &mut HarnessWorld, args: String) {
    let tmp = w.cli.tmp();
    let args: Vec<String> = args
        .split_whitespace()
        .map(|a| a.replace("{tmp}", &tmp.display().to_string()))
        .collect();
    let output = Command::new(env!("CARGO_BIN_EXE_sim-cli"))
        .args(&args)
        .current_dir(super::repo_root())
        .output()
        .unwrap();
    w.cli.output = Some(output);
}

#[then("sim-cli succeeds")]
fn succeeds(w: &mut HarnessWorld) {
    let out = w.cli.output();
    assert!(
        out.status.success(),
        "sim-cli failed: {:?}\nstderr:\n{}",
        out.status,
        w.cli.stderr()
    );
}

#[then("sim-cli fails")]
fn fails(w: &mut HarnessWorld) {
    let out = w.cli.output();
    assert!(
        !out.status.success(),
        "sim-cli unexpectedly succeeded\nstdout:\n{}",
        w.cli.stdout()
    );
}

#[then(expr = "its output contains {string}")]
fn output_contains(w: &mut HarnessWorld, text: String) {
    let all = w.cli.stdout() + &w.cli.stderr();
    assert!(all.contains(&text), "output lacks {text:?}:\n{all}");
}

#[then(expr = "its error output contains {string}")]
fn stderr_contains(w: &mut HarnessWorld, text: String) {
    let err = w.cli.stderr();
    assert!(err.contains(&text), "stderr lacks {text:?}:\n{err}");
}

#[then(expr = "the file {string} has {int} lines")]
fn file_lines(w: &mut HarnessWorld, name: String, lines: usize) {
    let text = std::fs::read_to_string(w.cli.tmp().join(name)).unwrap();
    assert_eq!(text.lines().count(), lines, "{text}");
}

#[then(expr = "the files {string} and {string} are identical")]
fn files_identical(w: &mut HarnessWorld, a: String, b: String) {
    let tmp = w.cli.tmp();
    let a = std::fs::read(tmp.join(a)).unwrap();
    let b = std::fs::read(tmp.join(b)).unwrap();
    assert_ne!(a.len(), 0);
    assert_eq!(a, b);
}

fn json(w: &HarnessWorld) -> serde_json::Value {
    serde_json::from_str(&w.cli.stdout())
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", w.cli.stdout()))
}

#[then(expr = "its output is JSON with {string}, {string}, {string} and {string}")]
fn json_keys(w: &mut HarnessWorld, k1: String, k2: String, k3: String, k4: String) {
    let value = json(w);
    for key in [k1, k2, k3, k4] {
        assert!(value.get(&key).is_some(), "JSON lacks {key}: {value}");
    }
}

#[then(expr = "the JSON says scenario {string} with {int} ticks")]
fn json_ticks(w: &mut HarnessWorld, scenario: String, ticks: u64) {
    let v = json(w);
    assert_eq!(v["scenario"], scenario);
    assert_eq!(v["ticks"], ticks);
}

#[then(expr = "the JSON says scenario {string} with {int} agents")]
fn json_agents(w: &mut HarnessWorld, scenario: String, agents: u64) {
    let v = json(w);
    assert_eq!(v["scenario"], scenario);
    assert_eq!(v["agents"], agents);
}

/// Copies the shipped `config/` into the scenario's scratch directory as `name`, leaving out
/// `skip` if given, and returns the copy's path.
fn copy_shipped(w: &mut HarnessWorld, name: &str, skip: Option<&str>) -> PathBuf {
    let target = w.cli.tmp().join(name);
    std::fs::create_dir_all(&target).unwrap();
    for entry in std::fs::read_dir(super::repo_root().join("config"))
        .unwrap()
        .flatten()
    {
        let file = entry.file_name();
        if entry.path().is_file() && skip.is_none_or(|s| file != s) {
            std::fs::copy(entry.path(), target.join(&file)).unwrap();
        }
    }
    target
}

#[given(expr = "a copy of the shipped config without {string} in {string}")]
fn copy_without(w: &mut HarnessWorld, skip: String, name: String) {
    copy_shipped(w, &name, Some(&skip));
}

#[given(
    expr = "a copy of the shipped config in {string} with {string} key {string} set to {string}"
)]
fn copy_with(w: &mut HarnessWorld, name: String, file: String, key: String, value: String) {
    let dir = copy_shipped(w, &name, None);
    let mut sources = sim_core::ConfigSources::read_dir(&dir).unwrap();
    let stem = file.trim_end_matches(".yaml");
    let ov: sim_cli::overrides::Override = format!("{stem}.{key}={value}").parse().unwrap();
    sim_cli::overrides::apply(&mut sources, &ov).unwrap();
    std::fs::write(dir.join(&file), sources.get(&file).unwrap()).unwrap();
}
