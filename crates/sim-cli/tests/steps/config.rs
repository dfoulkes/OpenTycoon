//! Steps for `features/overrides.feature`.

use cucumber::{given, then, when};
use sim_cli::overrides::{self, Override};
use sim_core::{Config, ConfigSources, Money};

use crate::HarnessWorld;

/// Config inputs and the last load result.
#[derive(Debug, Default)]
pub struct ConfigState {
    /// Shipped sources.
    pub sources: Option<ConfigSources>,
    /// `--preset`.
    pub preset: Option<String>,
    /// `--set` texts, in order.
    pub overrides: Vec<String>,
    /// Last load: the config, or the error text.
    pub result: Option<Result<Config, String>>,
}

impl ConfigState {
    /// Loads sources + preset + overrides the way `sim-cli` does.
    pub fn load(&self) -> Result<Config, String> {
        let sources = self.sources.as_ref().expect("no config given");
        let parsed: Vec<Override> = self
            .overrides
            .iter()
            .map(|s| s.parse::<Override>())
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        overrides::load(sources, self.preset.as_deref(), &parsed).map_err(|e| e.to_string())
    }

    /// Loads, panicking with the errors.
    pub fn load_ok(&self) -> Config {
        self.load().unwrap_or_else(|e| panic!("{e}"))
    }
}

#[given("the shipped config")]
fn shipped(w: &mut HarnessWorld) {
    let dir = super::repo_root().join("config");
    w.config.sources = Some(ConfigSources::read_dir(&dir).unwrap());
}

#[given(expr = "the override {string}")]
fn an_override(w: &mut HarnessWorld, text: String) {
    w.config.overrides.push(text);
}

#[given(expr = "the preset {string}")]
fn a_preset(w: &mut HarnessWorld, name: String) {
    w.config.preset = Some(name);
}

#[when("the harness loads the config")]
fn loads(w: &mut HarnessWorld) {
    w.config.result = Some(w.config.load());
}

#[then("the config loads")]
fn config_loads(w: &mut HarnessWorld) {
    if let Some(Err(e)) = &w.config.result {
        panic!("expected the config to load, got:\n{e}");
    }
}

#[then(expr = "loading fails naming {string}")]
fn fails_naming(w: &mut HarnessWorld, named: String) {
    match w.config.result.as_ref().expect("not loaded") {
        Ok(_) => panic!("expected loading to fail naming {named}, but it loaded"),
        Err(e) => assert!(e.contains(&named), "error does not name {named}:\n{e}"),
    }
}

#[then(expr = "the overdraft start limit is £{int}")]
fn start_limit(w: &mut HarnessWorld, pounds: i64) {
    let config = w.config.result.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(
        config.game.overdraft.start_limit.money(),
        Money::from_pounds(pounds)
    );
}

#[then(expr = "the game runs from {int} to {int}")]
fn runs_from(w: &mut HarnessWorld, start: i32, end: i32) {
    let config = w.config.result.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(
        (config.game.clock.start_year, config.game.clock.end_year),
        (start, end)
    );
}
