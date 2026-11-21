//! Configuration: every tunable lives in YAML (R17, R45; §10b).
//!
//! # Pipeline
//!
//! For each of the nine files in [`ConfigFile::ALL`]:
//!
//! 1. parse YAML (syntax errors name the file and line);
//! 2. validate against the file's JSON Schema, generated from the Rust types in this module
//!    (unknown keys, wrong types and out-of-range values name the file and the JSON pointer);
//! 3. deserialise into typed structs (precision errors, e.g. fractional pence, name the path);
//!
//! then, once every file has loaded, run the cross-field and cross-file rules in
//! [`validate`] (e.g. a business type's `max_height` may not exceed `world.max_height`, R58).
//!
//! Every error is collected, not just the first, and the whole load fails if there is any:
//! nothing fails silently. The committed `config/schema/*.schema.json` files are generated from
//! these types (`cargo schema`) and a test keeps them in sync.
//!
//! # I/O
//!
//! sim-core does no I/O. [`Config::load`] takes [`ConfigSources`] (file name → YAML text).
//! The `fs` feature adds [`ConfigSources::read_dir`] for binaries.

pub mod ai;
pub mod auctions;
pub mod businesses;
pub mod customers;
pub mod economy;
pub mod events;
pub mod game;
pub mod map;
pub mod map_checks;
pub mod types;
pub mod utilities;
pub mod validate;

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use ai::AiConfig;
pub use auctions::AuctionsConfig;
pub use businesses::BusinessesConfig;
pub use customers::CustomersConfig;
pub use economy::EconomyConfig;
pub use events::EventsConfig;
pub use game::GameConfig;
pub use map::{MapChoice, MapConfig, MapFile};
pub use types::{ClockTime, Pounds, Ratio};
pub use utilities::UtilitiesConfig;

/// One of the nine config files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConfigFile {
    /// `game.yaml`
    Game,
    /// `map.yaml`
    Map,
    /// `auctions.yaml`
    Auctions,
    /// `utilities.yaml`
    Utilities,
    /// `businesses.yaml`
    Businesses,
    /// `customers.yaml`
    Customers,
    /// `events.yaml`
    Events,
    /// `ai.yaml`
    Ai,
    /// `economy.yaml`: every economy weight (R71).
    Economy,
}

impl ConfigFile {
    /// Every config file, in load order.
    pub const ALL: [ConfigFile; 9] = [
        ConfigFile::Game,
        ConfigFile::Map,
        ConfigFile::Auctions,
        ConfigFile::Utilities,
        ConfigFile::Businesses,
        ConfigFile::Customers,
        ConfigFile::Events,
        ConfigFile::Ai,
        ConfigFile::Economy,
    ];

    /// The file name, e.g. `game.yaml`.
    pub const fn file_name(self) -> &'static str {
        match self {
            ConfigFile::Game => "game.yaml",
            ConfigFile::Map => "map.yaml",
            ConfigFile::Auctions => "auctions.yaml",
            ConfigFile::Utilities => "utilities.yaml",
            ConfigFile::Businesses => "businesses.yaml",
            ConfigFile::Customers => "customers.yaml",
            ConfigFile::Events => "events.yaml",
            ConfigFile::Ai => "ai.yaml",
            ConfigFile::Economy => "economy.yaml",
        }
    }

    /// The schema file name, e.g. `game.schema.json`.
    pub fn schema_file_name(self) -> String {
        self.file_name().replace(".yaml", ".schema.json")
    }

    /// The JSON Schema for this file, generated from its Rust type.
    pub fn schema(self) -> serde_json::Value {
        let schema = match self {
            ConfigFile::Game => schemars::schema_for!(GameConfig),
            ConfigFile::Map => schemars::schema_for!(MapFile),
            ConfigFile::Auctions => schemars::schema_for!(AuctionsConfig),
            ConfigFile::Utilities => schemars::schema_for!(UtilitiesConfig),
            ConfigFile::Businesses => schemars::schema_for!(BusinessesConfig),
            ConfigFile::Customers => schemars::schema_for!(CustomersConfig),
            ConfigFile::Events => schemars::schema_for!(EventsConfig),
            ConfigFile::Ai => schemars::schema_for!(AiConfig),
            ConfigFile::Economy => schemars::schema_for!(EconomyConfig),
        };
        schema.to_value()
    }
}

impl fmt::Display for ConfigFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

/// One config problem: which file, where in it (a JSON pointer such as
/// `/types/bakery/max_height`, or a line for syntax errors), and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// File name, e.g. `businesses.yaml`.
    pub file: String,
    /// JSON pointer or `line N column M`; empty for the whole file.
    pub location: String,
    /// What is wrong, naming the offending key or value.
    pub message: String,
}

impl ConfigError {
    /// Builds an error.
    pub fn new(
        file: impl Into<String>,
        location: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        ConfigError {
            file: file.into(),
            location: location.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let location = if self.location.is_empty() {
            "(root)"
        } else {
            &self.location
        };
        write!(f, "{}: {}: {}", self.file, location, self.message)
    }
}

/// Every problem found in a config load. Never empty.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ConfigErrors(pub Vec<ConfigError>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "invalid config ({} problem(s)):", self.0.len())?;
        for e in &self.0 {
            writeln!(f, "  - {e}")?;
        }
        Ok(())
    }
}

/// Raw YAML text per file name. The sim never reads files itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigSources {
    files: BTreeMap<String, String>,
}

impl ConfigSources {
    /// No files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces a file's text.
    pub fn insert(&mut self, file_name: impl Into<String>, yaml: impl Into<String>) {
        self.files.insert(file_name.into(), yaml.into());
    }

    /// Builder form of [`ConfigSources::insert`].
    #[must_use]
    pub fn with(mut self, file_name: impl Into<String>, yaml: impl Into<String>) -> Self {
        self.insert(file_name, yaml);
        self
    }

    /// A file's text, if present.
    pub fn get(&self, file_name: &str) -> Option<&str> {
        self.files.get(file_name).map(String::as_str)
    }

    /// File names present.
    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// Reads the nine config files from `dir`. A missing file, or any other `*.yaml`/`*.yml`
    /// in the directory (a likely typo), is an error.
    #[cfg(feature = "fs")]
    pub fn read_dir(dir: &std::path::Path) -> Result<Self, ConfigErrors> {
        let mut sources = Self::new();
        let mut errors = Vec::new();
        let dir_label = dir.display().to_string();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_yaml = std::path::Path::new(&name).extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml")
                    });
                    if is_yaml && !ConfigFile::ALL.iter().any(|f| f.file_name() == name) {
                        errors.push(ConfigError::new(
                            name,
                            "",
                            format!("unknown config file in {dir_label}; expected one of the nine files in §10b"),
                        ));
                    }
                }
            }
            Err(e) => errors.push(ConfigError::new(dir_label.clone(), "", e.to_string())),
        }
        for file in ConfigFile::ALL {
            match std::fs::read_to_string(dir.join(file.file_name())) {
                Ok(text) => sources.insert(file.file_name(), text),
                Err(e) => errors.push(ConfigError::new(file.file_name(), "", e.to_string())),
            }
        }
        if errors.is_empty() {
            Ok(sources)
        } else {
            Err(ConfigErrors(errors))
        }
    }
}

/// The whole validated config. A game stores the config it started with (§10b).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// `game.yaml`
    pub game: GameConfig,
    /// `map.yaml`: the selected board, resolved ([`MapFile::board`]).
    pub map: MapConfig,
    /// `auctions.yaml`
    pub auctions: AuctionsConfig,
    /// `utilities.yaml`
    pub utilities: UtilitiesConfig,
    /// `businesses.yaml`
    pub businesses: BusinessesConfig,
    /// `customers.yaml`
    pub customers: CustomersConfig,
    /// `events.yaml`
    pub events: EventsConfig,
    /// `ai.yaml`
    pub ai: AiConfig,
    /// `economy.yaml`: every economy weight (R71).
    pub economy: EconomyConfig,
}

impl Config {
    /// Parses, schema-validates, deserialises and cross-checks every file.
    ///
    /// Fails with every problem found, each naming its file and key.
    pub fn load(sources: &ConfigSources) -> Result<Config, ConfigErrors> {
        let mut errors = Vec::new();
        for name in sources.file_names() {
            if !ConfigFile::ALL.iter().any(|f| f.file_name() == name) {
                errors.push(ConfigError::new(name, "", "unknown config file"));
            }
        }
        let game = load_file::<GameConfig>(sources, ConfigFile::Game, &mut errors);
        let map_file = load_file::<MapFile>(sources, ConfigFile::Map, &mut errors);
        let map = map_file.as_ref().and_then(|file| {
            let board = file.board(file.map);
            if board.is_none() {
                errors.push(ConfigError::new(
                    ConfigFile::Map.file_name(),
                    "/map",
                    format!(
                        "selects board {:?} but /boards/{} is missing",
                        file.map.key(),
                        file.map
                    ),
                ));
            }
            board
        });
        let auctions = load_file::<AuctionsConfig>(sources, ConfigFile::Auctions, &mut errors);
        let utilities = load_file::<UtilitiesConfig>(sources, ConfigFile::Utilities, &mut errors);
        let businesses =
            load_file::<BusinessesConfig>(sources, ConfigFile::Businesses, &mut errors);
        let customers = load_file::<CustomersConfig>(sources, ConfigFile::Customers, &mut errors);
        let events = load_file::<EventsConfig>(sources, ConfigFile::Events, &mut errors);
        let ai = load_file::<AiConfig>(sources, ConfigFile::Ai, &mut errors);
        let economy = load_file::<EconomyConfig>(sources, ConfigFile::Economy, &mut errors);

        let (
            Some(game),
            Some(map),
            Some(auctions),
            Some(utilities),
            Some(businesses),
            Some(customers),
            Some(events),
            Some(ai),
            Some(economy),
        ) = (
            game, map, auctions, utilities, businesses, customers, events, ai, economy,
        )
        else {
            return Err(ConfigErrors(errors));
        };
        let config = Config {
            game,
            map,
            auctions,
            utilities,
            businesses,
            customers,
            events,
            ai,
            economy,
        };
        errors.extend(validate::check(&config));
        // The boards not selected are checked too, so a switch of `map` never meets a stale
        // board (R26).
        if let Some(file) = &map_file {
            for choice in MapChoice::ALL.into_iter().filter(|c| *c != file.map) {
                if let Some(board) = file.board(choice) {
                    errors.extend(validate::check_board(&config, &board));
                }
            }
        }
        if errors.is_empty() {
            Ok(config)
        } else {
            Err(ConfigErrors(errors))
        }
    }
}

/// Loads one file through parse → schema → typed deserialise, pushing any errors.
fn load_file<T: DeserializeOwned + JsonSchema>(
    sources: &ConfigSources,
    file: ConfigFile,
    errors: &mut Vec<ConfigError>,
) -> Option<T> {
    let name = file.file_name();
    let Some(text) = sources.get(name) else {
        errors.push(ConfigError::new(name, "", "file is missing"));
        return None;
    };
    let yaml: serde_yaml_ng::Value = match serde_yaml_ng::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            let location = e
                .location()
                .map(|l| format!("line {} column {}", l.line(), l.column()))
                .unwrap_or_default();
            errors.push(ConfigError::new(
                name,
                location,
                format!("YAML syntax: {e}"),
            ));
            return None;
        }
    };
    let json = match yaml_to_json(&yaml, "") {
        Ok(v) => v,
        Err((pointer, msg)) => {
            errors.push(ConfigError::new(name, pointer, msg));
            return None;
        }
    };
    let schema = file.schema();
    let validator = match jsonschema::validator_for(&schema) {
        Ok(v) => v,
        Err(e) => {
            errors.push(ConfigError::new(
                name,
                "",
                format!("internal: bad schema: {e}"),
            ));
            return None;
        }
    };
    let before = errors.len();
    for e in validator.iter_errors(&json) {
        errors.push(ConfigError::new(
            name,
            e.instance_path().to_string(),
            e.to_string(),
        ));
    }
    if errors.len() > before {
        return None;
    }
    match serde_path_to_error::deserialize::<_, T>(json) {
        Ok(v) => Some(v),
        Err(e) => {
            let pointer = format!("/{}", e.path()).replace('.', "/");
            errors.push(ConfigError::new(name, pointer, e.inner().to_string()));
            None
        }
    }
}

/// Converts parsed YAML to JSON for schema validation. Map keys that are numbers or booleans
/// (e.g. year keys `1930:`) become strings; anything else as a key is an error.
fn yaml_to_json(
    value: &serde_yaml_ng::Value,
    pointer: &str,
) -> Result<serde_json::Value, (String, String)> {
    use serde_yaml_ng::Value as Y;
    Ok(match value {
        Y::Null => serde_json::Value::Null,
        Y::Bool(b) => serde_json::Value::Bool(*b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::from(i)
            } else if let Some(u) = n.as_u64() {
                serde_json::Value::from(u)
            } else {
                let f = n.as_f64().unwrap_or(f64::NAN);
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| (pointer.to_owned(), format!("{f} is not a finite number")))?
            }
        }
        Y::String(s) => serde_json::Value::String(s.clone()),
        Y::Sequence(items) => serde_json::Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| yaml_to_json(v, &format!("{pointer}/{i}")))
                .collect::<Result<_, _>>()?,
        ),
        Y::Mapping(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    Y::String(s) => s.clone(),
                    Y::Number(n) => n.to_string(),
                    Y::Bool(b) => b.to_string(),
                    other => {
                        return Err((
                            pointer.to_owned(),
                            format!("map key {other:?} must be a string or number"),
                        ));
                    }
                };
                let child = yaml_to_json(v, &format!("{pointer}/{key}"))?;
                out.insert(key, child);
            }
            serde_json::Value::Object(out)
        }
        Y::Tagged(tagged) => {
            return Err((
                pointer.to_owned(),
                format!("YAML tags are not supported ({})", tagged.tag),
            ));
        }
    })
}
