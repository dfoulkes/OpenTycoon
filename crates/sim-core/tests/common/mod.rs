//! Shared test helpers: the shipped config, and YAML overrides by JSON pointer.
#![allow(dead_code, reason = "each test target uses a different subset")]

use std::path::PathBuf;
use std::sync::OnceLock;

use sim_core::config::ConfigFile;
use sim_core::{Config, ConfigSources};

/// The repo's `config/` directory.
pub fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config")
}

/// The repo's root.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The shipped config as sources (tests may do I/O; sim-core may not).
///
/// `config/` is read from disk once per test binary and every later call gets a copy, so a
/// file that changes while the tests run (an edit, a branch switch in the same checkout) can't
/// change the config half way through. Property tests load the config before their first case,
/// so a broken file fails the test once, with the read error, instead of failing every case
/// and being reported as a shrunk counterexample (`steps = []`).
pub fn shipped_sources() -> ConfigSources {
    static SHIPPED: OnceLock<Result<ConfigSources, String>> = OnceLock::new();
    match SHIPPED.get_or_init(read_shipped_sources) {
        Ok(sources) => sources.clone(),
        Err(e) => panic!("{e}"),
    }
}

/// Reads `config/` and checks it loads, before a property test's first case. Call it at the
/// top of the test: a missing or broken file then fails the test once, with its error, and the
/// cases get the copy already read.
pub fn preload_shipped_config() {
    let _ = shipped_config();
}

fn read_shipped_sources() -> Result<ConfigSources, String> {
    let dir = config_dir();
    let mut sources = ConfigSources::new();
    for file in ConfigFile::ALL {
        let path = dir.join(file.file_name());
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading shipped config {}: {e}", path.display()))?;
        sources.insert(file.file_name(), text);
    }
    Ok(sources)
}

/// The shipped config, loaded. Panics with every error if it is invalid.
pub fn shipped_config() -> Config {
    Config::load(&shipped_sources()).unwrap_or_else(|e| panic!("{e}"))
}

/// The shipped config on the grey-box 3×3 map (`map: v1`), for tests about it.
pub fn shipped_config_v1() -> Config {
    let mut sources = shipped_sources();
    set_key(&mut sources, "map.yaml", "/map", "v1");
    Config::load(&sources).unwrap_or_else(|e| panic!("{e}"))
}

/// Sets the value at a JSON-pointer-style path (`/clock/start_year`) in one file, creating the
/// final key if absent (that is how tests add unknown keys). `value` is parsed as YAML, so
/// `"20"` is a number and `"\"twenty\""` or `"twenty"` a string.
pub fn set_key(sources: &mut ConfigSources, file: &str, pointer: &str, value: &str) {
    let text = sources.get(file).unwrap_or_else(|| panic!("no {file}"));
    let mut doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).unwrap();
    let new_value: serde_yaml_ng::Value = serde_yaml_ng::from_str(value).unwrap();
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    let (last, path) = parts.split_last().unwrap();
    let mut node = &mut doc;
    for part in path {
        node = step(node, part);
    }
    match node {
        serde_yaml_ng::Value::Mapping(map) => {
            map.insert(serde_yaml_ng::Value::String((*last).to_owned()), new_value);
        }
        serde_yaml_ng::Value::Sequence(seq) => {
            seq[last.parse::<usize>().unwrap()] = new_value;
        }
        other => panic!("cannot set {pointer}: parent is {other:?}"),
    }
    sources.insert(file, serde_yaml_ng::to_string(&doc).unwrap());
}

fn step<'a>(node: &'a mut serde_yaml_ng::Value, part: &str) -> &'a mut serde_yaml_ng::Value {
    match node {
        serde_yaml_ng::Value::Mapping(map) => {
            let key = map
                .keys()
                .find(|k| match k {
                    serde_yaml_ng::Value::String(s) => s == part,
                    serde_yaml_ng::Value::Number(n) => n.to_string() == part,
                    _ => false,
                })
                .cloned()
                .unwrap_or_else(|| panic!("no key {part}"));
            map.get_mut(&key).unwrap()
        }
        serde_yaml_ng::Value::Sequence(seq) => &mut seq[part.parse::<usize>().unwrap()],
        other => panic!("cannot step into {other:?} at {part}"),
    }
}

/// Removes a file from the sources.
pub fn without(sources: &ConfigSources, file: &str) -> ConfigSources {
    let mut out = ConfigSources::new();
    for name in sources.file_names() {
        if name != file {
            out.insert(name, sources.get(name).unwrap_or_default());
        }
    }
    out
}
