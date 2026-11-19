//! The committed `config/schema/*.schema.json` files must match the schemas generated from the
//! Rust config types. Regenerate with `cargo schema`.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

mod common;

use sim_core::config::ConfigFile;

#[test]
fn committed_schemas_match_the_config_types() {
    let dir = common::config_dir().join("schema");
    let mut stale = Vec::new();
    for file in ConfigFile::ALL {
        let path = dir.join(file.schema_file_name());
        let committed: serde_json::Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::Value::Null);
        if committed != file.schema() {
            stale.push(file.schema_file_name());
        }
    }
    assert!(
        stale.is_empty(),
        "stale schemas {stale:?}: run `cargo schema` and commit the result"
    );
}
