//! Config overrides and presets for parameter sweeps (spec §10b, R45).
//!
//! An override is `<file>.<key>.<key>...=<value>`, e.g. `economy.money.starting_cash=9000`: the
//! first segment names the YAML file (`economy` → `economy.yaml`), the rest is the key path, and
//! the value is parsed as YAML (`20000` is a number, `"x"` or `x` a string, `[1, 2]` a list).
//! Overrides edit the YAML *text* and the result goes through the normal loader, so a swept
//! value is schema-checked and cross-checked exactly like a hand-edited file and fails naming
//! the file and key.
//!
//! A preset (`--preset v1`) is shorthand for overriding `game.clock.start_year` and
//! `game.clock.end_year` with the span listed under `game.presets.<name>`. Explicit `--set`
//! overrides are applied after the preset, so they win.

use std::fmt;
use std::str::FromStr;

use serde_yaml_ng::Value;
use sim_core::config::ConfigFile;
use sim_core::{Config, ConfigErrors, ConfigSources};

/// Why an override, preset or the resulting config was rejected.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The override text is not `file.key...=value`.
    #[error("override `{text}`: {reason}")]
    Syntax {
        /// The override as given.
        text: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The first segment names no config file.
    #[error("override `{text}`: no config file `{file}` (expected one of: {known})")]
    UnknownFile {
        /// The override as given.
        text: String,
        /// The file it named.
        file: String,
        /// The files that exist.
        known: String,
    },
    /// A key on the path does not exist.
    #[error("override `{text}`: {file} has no key `{pointer}`")]
    NoSuchKey {
        /// The override as given.
        text: String,
        /// The file.
        file: String,
        /// JSON pointer of the missing key.
        pointer: String,
    },
    /// The named preset is not in `game.yaml`.
    #[error("no preset `{name}` in game.yaml (known: {known})")]
    UnknownPreset {
        /// The preset asked for.
        name: String,
        /// The presets that exist.
        known: String,
    },
    /// The YAML could not be parsed or re-serialised.
    #[error("{file}: {reason}")]
    Yaml {
        /// The file.
        file: String,
        /// What went wrong.
        reason: String,
    },
    /// The config with the overrides applied failed validation; every error names its key.
    #[error("config with overrides is invalid:\n{0}")]
    Invalid(ConfigErrors),
}

/// One `file.key.path=value` override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    text: String,
    file: ConfigFile,
    path: Vec<String>,
    value: String,
}

impl Override {
    /// The YAML file it edits, e.g. `game.yaml`.
    pub fn file_name(&self) -> &'static str {
        self.file.file_name()
    }

    /// JSON pointer of the key inside the file, e.g. `/money/overdraft/start_limit`.
    pub fn pointer(&self) -> String {
        self.path.iter().fold(String::new(), |mut s, p| {
            s.push('/');
            s.push_str(p);
            s
        })
    }

    /// The raw value text (parsed as YAML when applied).
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for Override {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl FromStr for Override {
    type Err = LoadError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let syntax = |reason: &str| LoadError::Syntax {
            text: text.to_owned(),
            reason: reason.to_owned(),
        };
        let (key, value) = text
            .split_once('=')
            .ok_or_else(|| syntax("expected `file.key=value`"))?;
        let mut segments = key.trim().split('.');
        let stem = segments.next().unwrap_or_default();
        let path: Vec<String> = segments.map(str::to_owned).collect();
        if stem.is_empty() || path.is_empty() || path.iter().any(String::is_empty) {
            return Err(syntax(
                "expected `file.key=value`, e.g. `economy.money.overdraft.start_limit=20000`",
            ));
        }
        let file_name = format!("{stem}.yaml");
        let file = ConfigFile::ALL
            .into_iter()
            .find(|f| f.file_name() == file_name)
            .ok_or_else(|| LoadError::UnknownFile {
                text: text.to_owned(),
                file: file_name.clone(),
                known: known_files(),
            })?;
        Ok(Override {
            text: text.to_owned(),
            file,
            path,
            value: value.trim().to_owned(),
        })
    }
}

fn known_files() -> String {
    ConfigFile::ALL
        .iter()
        .map(|f| f.file_name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Applies one override to the YAML text in `sources`.
///
/// Every key on the path must already exist except the last, which may be new (an optional
/// key); the schema then rejects it by name if it is not a real key.
pub fn apply(sources: &mut ConfigSources, ov: &Override) -> Result<(), LoadError> {
    let file = ov.file_name();
    let yaml_err = |reason: String| LoadError::Yaml {
        file: file.to_owned(),
        reason,
    };
    let text = sources
        .get(file)
        .ok_or_else(|| yaml_err("file is missing".into()))?;
    let mut doc: Value = serde_yaml_ng::from_str(text).map_err(|e| yaml_err(e.to_string()))?;
    let new_value: Value = serde_yaml_ng::from_str(&ov.value)
        .map_err(|e| yaml_err(format!("value `{}`: {e}", ov.value)))?;
    let no_key = |upto: usize| LoadError::NoSuchKey {
        text: ov.text.clone(),
        file: file.to_owned(),
        pointer: ov.path[..upto]
            .iter()
            .fold(String::new(), |s, p| s + "/" + p),
    };
    let Some((last, parents)) = ov.path.split_last() else {
        return Err(no_key(0));
    };
    let mut node = &mut doc;
    for (i, part) in parents.iter().enumerate() {
        node = child_mut(node, part).ok_or_else(|| no_key(i + 1))?;
    }
    match node {
        Value::Mapping(map) => {
            let key = map
                .keys()
                .find(|k| key_matches(k, last))
                .cloned()
                .unwrap_or_else(|| Value::String(last.clone()));
            map.insert(key, new_value);
        }
        Value::Sequence(seq) => {
            let slot = last
                .parse::<usize>()
                .ok()
                .and_then(|i| seq.get_mut(i))
                .ok_or_else(|| no_key(ov.path.len()))?;
            *slot = new_value;
        }
        _ => return Err(no_key(ov.path.len())),
    }
    let out = serde_yaml_ng::to_string(&doc).map_err(|e| yaml_err(e.to_string()))?;
    sources.insert(file, out);
    Ok(())
}

/// Year keys (`1930:`) are YAML numbers; match them by their text.
fn key_matches(key: &Value, part: &str) -> bool {
    match key {
        Value::String(s) => s == part,
        Value::Number(n) => n.to_string() == part,
        Value::Bool(b) => b.to_string() == part,
        _ => false,
    }
}

fn child_mut<'a>(node: &'a mut Value, part: &str) -> Option<&'a mut Value> {
    match node {
        Value::Mapping(map) => {
            let key = map.keys().find(|k| key_matches(k, part)).cloned()?;
            map.get_mut(&key)
        }
        Value::Sequence(seq) => seq.get_mut(part.parse::<usize>().ok()?),
        _ => None,
    }
}

/// Loads a config from `sources` with an optional preset and overrides applied in that order,
/// through the normal schema + cross-file validation.
pub fn load(
    sources: &ConfigSources,
    preset: Option<&str>,
    overrides: &[Override],
) -> Result<Config, LoadError> {
    Config::load(&resolve(sources, preset, overrides)?).map_err(LoadError::Invalid)
}

/// The effective config files [`load`] validates: `sources` with the preset and overrides
/// applied. Replays snapshot exactly these (`replay::snapshot`).
pub fn resolve(
    sources: &ConfigSources,
    preset: Option<&str>,
    overrides: &[Override],
) -> Result<ConfigSources, LoadError> {
    let mut edited = sources.clone();
    for ov in overrides {
        apply(&mut edited, ov)?;
    }
    let Some(name) = preset else {
        return Ok(edited);
    };
    // Read the preset table from the overridden config, so a sweep can move a preset too.
    let probe = Config::load(&edited).map_err(LoadError::Invalid)?;
    let span = probe
        .game
        .presets
        .get(name)
        .ok_or_else(|| LoadError::UnknownPreset {
            name: name.to_owned(),
            known: probe
                .game
                .presets
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        })?;
    let mut with_preset = sources.clone();
    let preset_overrides = [
        format!("game.clock.start_year={}", span.start_year),
        format!("game.clock.end_year={}", span.end_year),
    ];
    for text in &preset_overrides {
        apply(&mut with_preset, &text.parse()?)?;
    }
    for ov in overrides {
        apply(&mut with_preset, ov)?;
    }
    Ok(with_preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_path_and_value() {
        let ov: Override = "economy.money.overdraft.start_limit=20000".parse().unwrap();
        assert_eq!(ov.file_name(), "economy.yaml");
        assert_eq!(ov.pointer(), "/money/overdraft/start_limit");
        assert_eq!(ov.value(), "20000");
        assert_eq!(ov.to_string(), "economy.money.overdraft.start_limit=20000");
    }

    #[test]
    fn value_may_contain_equals_and_spaces_are_trimmed() {
        let ov: Override = " ai.personalities.grocer.name = a=b ".parse().unwrap();
        assert_eq!(ov.pointer(), "/personalities/grocer/name");
        assert_eq!(ov.value(), "a=b");
    }

    #[test]
    fn rejects_missing_equals_missing_key_and_empty_segments() {
        for bad in [
            "economy.money.overdraft.start_limit",
            "game=1",
            "=1",
            "game..x=1",
            ".x=1",
        ] {
            assert!(
                matches!(bad.parse::<Override>(), Err(LoadError::Syntax { .. })),
                "{bad} should be a syntax error"
            );
        }
    }

    #[test]
    fn rejects_an_unknown_file_and_lists_the_known_ones() {
        let err = "gaem.clock.start_year=1930"
            .parse::<Override>()
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("gaem.yaml") && msg.contains("game.yaml"),
            "{msg}"
        );
    }

    fn sources() -> ConfigSources {
        ConfigSources::new().with(
            "game.yaml",
            "a:\n  b: 1\n  list: [1, 2]\nyears:\n  1930: 1.5\n",
        )
    }

    fn get(sources: &ConfigSources) -> Value {
        serde_yaml_ng::from_str(sources.get("game.yaml").unwrap()).unwrap()
    }

    #[test]
    fn apply_sets_nested_keys_list_items_and_year_keys() {
        let mut s = sources();
        apply(&mut s, &"game.a.b=7".parse().unwrap()).unwrap();
        apply(&mut s, &"game.a.list.1=9".parse().unwrap()).unwrap();
        apply(&mut s, &"game.years.1930=2.0".parse().unwrap()).unwrap();
        let doc = get(&s);
        assert_eq!(doc["a"]["b"], Value::from(7));
        assert_eq!(doc["a"]["list"][1], Value::from(9));
        assert_eq!(doc["years"][Value::from(1930)], Value::from(2.0));
    }

    #[test]
    fn apply_names_the_missing_parent_key() {
        let mut s = sources();
        let err = apply(&mut s, &"game.nope.b=1".parse().unwrap()).unwrap_err();
        assert!(matches!(&err, LoadError::NoSuchKey { pointer, .. } if pointer == "/nope"));
        let err = apply(&mut s, &"game.a.list.5=1".parse().unwrap()).unwrap_err();
        assert!(matches!(&err, LoadError::NoSuchKey { pointer, .. } if pointer == "/a/list/5"));
    }
}
