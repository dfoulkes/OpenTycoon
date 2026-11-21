//! Steps for `features/config/`.

use cucumber::{given, then, when};
use sim_core::{Config, ConfigErrors, ConfigSources};

use crate::SimWorld;
use crate::common;

/// Config sources and the last load result.
#[derive(Debug, Default)]
pub struct ConfigState {
    /// Sources being edited by Given steps.
    pub sources: Option<ConfigSources>,
    /// Result of the last load.
    pub result: Option<Result<Config, ConfigErrors>>,
}

impl ConfigState {
    /// Sources, starting from the shipped config if none yet.
    pub fn sources_mut(&mut self) -> &mut ConfigSources {
        self.sources.get_or_insert_with(common::shipped_sources)
    }

    /// Loads the current sources and returns the config, panicking with every error.
    pub fn load_ok(&mut self) -> Config {
        Config::load(self.sources_mut()).unwrap_or_else(|e| panic!("{e}"))
    }
}

#[given("the shipped config")]
fn shipped(w: &mut SimWorld) {
    w.config.sources = Some(common::shipped_sources());
    w.clock.reset();
}

#[given(expr = "the shipped config with {string} key {string} set to {string}")]
fn shipped_with(w: &mut SimWorld, file: String, pointer: String, value: String) {
    common::set_key(w.config.sources_mut(), &file, &pointer, &value);
    w.clock.reset();
}

/// R70 off for scenarios about something else: one window per trade, its whole opening hours,
/// for everyone and every good. Shoppers then set out as trade opens, as before R70.
#[given("shoppers shop at any hour of trade")]
fn any_hour(w: &mut SimWorld) {
    common::set_key(
        w.config.sources_mut(),
        "customers.yaml",
        "/shopping_windows/rules",
        "[{match: {trade: day}, windows: {'*': {from: '07:00', to: '17:00'}}}, \
         {match: {trade: night}, windows: {'*': {from: '19:00', to: '23:50'}}}]",
    );
    w.clock.reset();
}

#[given(expr = "the shipped config without {string}")]
fn shipped_without(w: &mut SimWorld, file: String) {
    let sources = common::without(w.config.sources_mut(), &file);
    w.config.sources = Some(sources);
}

#[given(expr = "the shipped config plus a file {string}")]
fn shipped_plus(w: &mut SimWorld, file: String) {
    w.config.sources_mut().insert(file, "{}\n");
}

#[when("the config is loaded")]
fn load(w: &mut SimWorld) {
    let result = Config::load(w.config.sources_mut());
    w.config.result = Some(result);
}

#[then("loading succeeds")]
fn succeeds(w: &mut SimWorld) {
    match w.config.result.as_ref().expect("no load attempted") {
        Ok(_) => {}
        Err(e) => panic!("expected success, got:\n{e}"),
    }
}

#[then("loading fails")]
fn fails(w: &mut SimWorld) {
    assert!(
        w.config
            .result
            .as_ref()
            .expect("no load attempted")
            .is_err(),
        "expected the load to fail, but it succeeded"
    );
}

#[then(expr = "an error names {string} and {string}")]
fn error_names(w: &mut SimWorld, file: String, text: String) {
    let Some(Err(errors)) = w.config.result.as_ref() else {
        panic!("expected a failed load");
    };
    let hit = errors
        .0
        .iter()
        .any(|e| e.file == file && e.to_string().contains(&text));
    assert!(hit, "no error names {file:?} and {text:?}; got:\n{errors}");
}
