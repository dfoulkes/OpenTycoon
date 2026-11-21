//! `config/server.yaml`: how the inspector server paces the sim and what it puts on the wire
//! (spec §10d, R88; `docs/inspector-protocol.md`).
//!
//! It is not sim config: sim-core never reads it, and it cannot change a game's result. It sits
//! in the config directory as one of sim-core's `COMPANION_FILES` and goes through the same
//! pipeline as the nine sim files: parse YAML, validate against the JSON Schema generated from
//! [`ServerConfig`] (committed as `config/schema/server.schema.json`, kept in sync by a test),
//! deserialise, then the cross-field rules in [`ServerConfig::check`]. Every problem names the
//! file and the JSON pointer of the key.

use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sim_core::{ConfigError, ConfigErrors};

/// The file name in the config directory.
pub const FILE_NAME: &str = "server.yaml";

/// The schema file name in `config/schema/`.
pub const SCHEMA_FILE_NAME: &str = "server.schema.json";

/// Speed steps `1`–`4` (`0` is pause).
pub const SPEED_STEPS: usize = 4;

/// `server.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Ticks per real second for speed steps 1, 2, 3 and 4, in that order. Each step must be
    /// faster than the one before.
    #[schemars(inner(range(min = 0.01, max = 100_000)))]
    pub ticks_per_second: [f64; SPEED_STEPS],
    /// The speed step a new game starts at (`0` = paused).
    #[schemars(range(min = 0, max = 4))]
    pub start_speed: u8,
    /// Deltas sent per real second, at most. Each delta carries every tick since the last.
    #[schemars(range(min = 0.1, max = 120))]
    pub deltas_per_second: f64,
    /// If the server falls more than this many real seconds behind its speed (a stall), it
    /// drops the backlog instead of racing to catch up.
    #[schemars(range(min = 0.1, max = 60))]
    pub max_catch_up_seconds: f64,
    /// Walker positions sent per message, at most.
    #[schemars(range(min = 0, max = 100_000))]
    pub max_walkers: u32,
    /// Bid events kept for the snapshot's recent history.
    #[schemars(range(min = 0, max = 10_000))]
    pub recent_bids: u32,
    /// The one-time token rules.
    pub token: TokenRules,
}

/// The single-player token (`hello.token`): the embedded server only talks to the client that
/// started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenRules {
    /// Hex characters in the token. The client makes one of exactly this length; the server
    /// refuses to start with any other.
    #[schemars(range(min = 16, max = 128))]
    pub length: u32,
}

impl TokenRules {
    /// True if `token` is `length` hex characters.
    pub fn accepts(&self, token: &str) -> bool {
        usize::try_from(self.length).is_ok_and(|n| token.len() == n)
            && token.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl ServerConfig {
    /// The JSON Schema for `server.yaml`, generated from the types.
    pub fn schema() -> serde_json::Value {
        schemars::schema_for!(ServerConfig).to_value()
    }

    /// Ticks per real second at `speed` (`0` and unknown steps: none).
    pub fn ticks_per_second_at(&self, speed: u8) -> f64 {
        match usize::from(speed).checked_sub(1) {
            Some(i) => self.ticks_per_second.get(i).copied().unwrap_or(0.0),
            None => 0.0,
        }
    }

    /// The highest speed step.
    pub fn max_speed(&self) -> u8 {
        u8::try_from(SPEED_STEPS).unwrap_or(u8::MAX)
    }

    /// Parses, schema-validates, deserialises and cross-checks `server.yaml` text.
    pub fn from_yaml(text: &str) -> Result<ServerConfig, ConfigErrors> {
        let err = |location: String, message: String| {
            ConfigErrors(vec![ConfigError::new(FILE_NAME, location, message)])
        };
        let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(text).map_err(|e| {
            let location = e
                .location()
                .map(|l| format!("line {} column {}", l.line(), l.column()))
                .unwrap_or_default();
            err(location, format!("YAML syntax: {e}"))
        })?;
        let json = serde_json::to_value(&yaml)
            .map_err(|e| err(String::new(), format!("not representable as JSON: {e}")))?;
        let schema = Self::schema();
        let validator = jsonschema::validator_for(&schema)
            .map_err(|e| err(String::new(), format!("internal: bad schema: {e}")))?;
        let problems: Vec<ConfigError> = validator
            .iter_errors(&json)
            .map(|e| ConfigError::new(FILE_NAME, e.instance_path().to_string(), e.to_string()))
            .collect();
        if !problems.is_empty() {
            return Err(ConfigErrors(problems));
        }
        let config: ServerConfig = serde_path_to_error::deserialize(json).map_err(|e| {
            err(
                format!("/{}", e.path()).replace('.', "/"),
                e.inner().to_string(),
            )
        })?;
        let problems = config.check();
        if problems.is_empty() {
            Ok(config)
        } else {
            Err(ConfigErrors(problems))
        }
    }

    /// Reads `server.yaml` from the config directory.
    pub fn read_dir(dir: &Path) -> Result<ServerConfig, ConfigErrors> {
        let text = std::fs::read_to_string(dir.join(FILE_NAME))
            .map_err(|e| ConfigErrors(vec![ConfigError::new(FILE_NAME, "", e.to_string())]))?;
        Self::from_yaml(&text)
    }

    /// Cross-field rules the schema cannot say: speed steps strictly faster in order.
    pub fn check(&self) -> Vec<ConfigError> {
        self.ticks_per_second
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[1] <= pair[0])
            .map(|(i, pair)| {
                ConfigError::new(
                    FILE_NAME,
                    format!("/ticks_per_second/{}", i + 1),
                    format!(
                        "speed {} ({} ticks/s) must be faster than speed {} ({} ticks/s)",
                        i + 2,
                        pair[1],
                        i + 1,
                        pair[0]
                    ),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_text() -> String {
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/server.yaml"),
        )
        .unwrap()
    }

    #[test]
    fn the_shipped_file_loads() {
        let c = ServerConfig::from_yaml(&shipped_text()).unwrap();
        assert!(c.ticks_per_second_at(1) > 0.0);
        assert!(c.ticks_per_second_at(0).abs() < f64::EPSILON);
        assert!(c.ticks_per_second_at(5).abs() < f64::EPSILON);
    }

    #[test]
    fn an_unknown_key_is_named() {
        let text = shipped_text() + "\nbogus: 1\n";
        let e = ServerConfig::from_yaml(&text).unwrap_err().to_string();
        assert!(e.contains("server.yaml") && e.contains("bogus"), "{e}");
    }

    #[test]
    fn an_out_of_range_value_is_named() {
        let text = shipped_text().replace("max_walkers:", "max_walkers: -5 #");
        let e = ServerConfig::from_yaml(&text).unwrap_err().to_string();
        assert!(e.contains("/max_walkers"), "{e}");
    }

    #[test]
    fn speeds_must_get_faster() {
        let mut c = ServerConfig::from_yaml(&shipped_text()).unwrap();
        c.ticks_per_second = [10.0, 5.0, 20.0, 30.0];
        let e = c.check();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].location, "/ticks_per_second/1");
    }

    #[test]
    fn tokens_are_hex_of_the_configured_length() {
        let rules = TokenRules { length: 16 };
        assert!(rules.accepts("0123456789abcdef"));
        assert!(!rules.accepts("0123456789abcde"));
        assert!(!rules.accepts("0123456789abcdeg"));
    }

    #[test]
    fn the_committed_schema_matches_the_types() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../config/schema")
            .join(SCHEMA_FILE_NAME);
        let committed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_default())
                .unwrap_or(serde_json::Value::Null);
        assert_eq!(
            committed,
            ServerConfig::schema(),
            "stale {SCHEMA_FILE_NAME}: run `cargo run -p server -- --write-schema config/schema`"
        );
    }
}
