//! Steps for `features/study.feature` (library level, plus one binary scenario through
//! `steps::cli`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use cucumber::{given, then, when};
use sim_cli::study::{
    Distribution, Entry, GamePlan, Kind, ModelMetrics, ModelSpec, Parameter, Roster, RunOptions,
    StudyDef, StudyReport, Tally, generate, gini, metrics, model_config, normalised_entropy,
    overrides_for, plan, resolve_roster, run_study, wilson,
};
use sim_core::ConfigSources;

use crate::HarnessWorld;

/// Scenario state for studies.
#[derive(Debug, Default)]
pub struct StudyState {
    /// The study being built.
    pub def: Option<StudyDef>,
    /// Generated models (last generation).
    pub models: Vec<ModelSpec>,
    /// A second generation, for determinism.
    pub again: Vec<ModelSpec>,
    /// The roster under test.
    pub roster: Option<Roster>,
    /// Planned games.
    pub plan: Vec<GamePlan>,
    /// A Wilson interval.
    pub interval: Option<(f64, f64)>,
    /// Win counts for evenness.
    pub counts: Vec<u64>,
    /// A synthetic tally.
    pub tally: Option<Tally>,
    /// Metrics of the last model computed.
    pub metrics: Option<ModelMetrics>,
    /// Output directories of study runs, in order.
    pub outs: Vec<PathBuf>,
    /// Reports of study runs, in order.
    pub reports: Vec<StudyReport>,
}

impl StudyState {
    fn def(&mut self) -> &mut StudyDef {
        self.def.as_mut().expect("no study given")
    }
}

fn scratch() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "sim-cli-study-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn base(w: &HarnessWorld) -> ConfigSources {
    w.config
        .sources
        .clone()
        .unwrap_or_else(|| ConfigSources::read_dir(&super::repo_root().join("config")).unwrap())
}

fn parse_distribution(s: &str) -> Distribution {
    match s {
        "uniform" => Distribution::Uniform,
        "log-uniform" => Distribution::LogUniform,
        other => panic!("unknown distribution {other}"),
    }
}

fn parse_kind(s: &str) -> Kind {
    match s {
        "integer" => Kind::Integer,
        "decimal" => Kind::Decimal,
        other => panic!("unknown kind {other}"),
    }
}

fn param_index(w: &HarnessWorld, name: &str) -> usize {
    w.study
        .def
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == name)
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn model(w: &HarnessWorld, id: u32) -> &ModelSpec {
    w.study.models.iter().find(|m| m.id == id).unwrap()
}

// ---- Model generation ------------------------------------------------------------------------

#[given(expr = "a study of {int} models from seed {int}")]
fn a_study(w: &mut HarnessWorld, models: u32, seed: u64) {
    w.study.def = Some(StudyDef {
        models,
        seed,
        ..StudyDef::default()
    });
}

#[given(
    expr = "the parameter {string} on {string} from {float} to {float}, {word}, {word}"
)]
fn a_parameter(
    w: &mut HarnessWorld,
    name: String,
    key: String,
    min: f64,
    max: f64,
    distribution: String,
    kind: String,
) {
    let p = Parameter::set(
        &name,
        &key,
        min,
        max,
        parse_distribution(&distribution),
        parse_kind(&kind),
    );
    w.study.def().parameters.push(p);
}

#[given(expr = "the scale parameter {string} on {string} from {float} to {float}")]
fn a_scale_parameter(w: &mut HarnessWorld, name: String, key: String, min: f64, max: f64) {
    w.study
        .def()
        .parameters
        .push(Parameter::scale(&name, &key, min, max));
}

#[when("I generate the models twice")]
fn generate_twice(w: &mut HarnessWorld) {
    let def = w.study.def.clone().unwrap();
    w.study.models = generate(&def).unwrap();
    w.study.again = generate(&def).unwrap();
}

#[when("I generate the models")]
fn generate_once(w: &mut HarnessWorld) {
    let def = w.study.def.clone().unwrap();
    w.study.models = generate(&def).unwrap();
}

#[then("both generations are identical")]
fn identical_generations(w: &mut HarnessWorld) {
    assert_eq!(w.study.models.len(), 10);
    assert_eq!(w.study.models, w.study.again);
}

#[then(expr = "generating from seed {int} gives different models")]
fn different_seed(w: &mut HarnessWorld, seed: u64) {
    let mut def = w.study.def.clone().unwrap();
    def.seed = seed;
    let other = generate(&def).unwrap();
    assert_eq!(other[0], w.study.models[0], "model 0 is always the baseline");
    assert_ne!(other[1..], w.study.models[1..]);
}

#[then(expr = "every value of {string} is between {float} and {float}")]
fn values_in_range(w: &mut HarnessWorld, name: String, min: f64, max: f64) {
    let i = param_index(w, &name);
    let sampled: Vec<f64> = w
        .study
        .models
        .iter()
        .filter_map(|m| m.values.as_ref().map(|v| v[i]))
        .collect();
    assert_eq!(sampled.len(), w.study.models.len() - 1);
    for v in sampled {
        assert!((min..=max).contains(&v), "{v} outside {min}..={max}");
    }
}

#[then(expr = "the {int} sampled models of {string} fall one in each of {int} equal strata")]
fn one_per_stratum(w: &mut HarnessWorld, n: usize, name: String, strata: usize) {
    let i = param_index(w, &name);
    let mut hits = vec![0usize; strata];
    for m in w.study.models.iter().filter(|m| m.id != 0) {
        let u = m.units.as_ref().unwrap()[i];
        assert!((0.0..1.0).contains(&u), "unit sample {u}");
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let stratum = (u * strata as f64) as usize;
        hits[stratum] += 1;
    }
    assert_eq!(hits.iter().sum::<usize>(), n);
    assert!(hits.iter().all(|h| *h == 1), "strata hit: {hits:?}");
}

#[then("model 0 has no overrides")]
fn model0_no_overrides(w: &mut HarnessWorld) {
    let def = w.study.def.clone().unwrap();
    let m = model(w, 0).clone();
    assert_eq!(m.values, None);
    assert!(overrides_for(&def, &base(w), &m).unwrap().is_empty());
}

#[then("model 0's config equals the shipped config")]
fn model0_is_shipped(w: &mut HarnessWorld) {
    let def = w.study.def.clone().unwrap();
    let m = model(w, 0).clone();
    let sources = base(w);
    let ours = model_config(&def, &sources, &m, "full").unwrap();
    let shipped = sim_cli::overrides::load(&sources, Some("full"), &[]).unwrap();
    assert_eq!(ours, shipped);
}

#[then(expr = "model 0's {string} value is the shipped {float}")]
fn model0_value(w: &mut HarnessWorld, name: String, value: f64) {
    let def = w.study.def.clone().unwrap();
    let i = param_index(w, &name);
    let shipped = sim_cli::study::baseline_values(&def, &base(w));
    assert_eq!(shipped[i], Some(value));
}

#[then(expr = "model {int} overrides {string}")]
fn model_overrides(w: &mut HarnessWorld, id: u32, key: String) {
    let def = w.study.def.clone().unwrap();
    let m = model(w, id).clone();
    let ovs = overrides_for(&def, &base(w), &m).unwrap();
    assert_eq!(ovs.len(), 1, "{ovs:?}");
    assert!(ovs[0].to_string().starts_with(&format!("{key}=")), "{ovs:?}");
    let config = model_config(&def, &base(w), &m, "v1").unwrap();
    let value = m.values.as_ref().unwrap()[0];
    assert!((f64::from(config.game.leases.term_years) - value).abs() < 1e-9);
}

#[then(expr = "model {int} overrides every year under {string} by its {string} factor")]
fn model_scales(w: &mut HarnessWorld, id: u32, key: String, name: String) {
    let def = w.study.def.clone().unwrap();
    let i = param_index(w, &name);
    let m = model(w, id).clone();
    let factor = m.values.as_ref().unwrap()[i];
    let sources = base(w);
    let ovs = overrides_for(&def, &sources, &m).unwrap();
    let shipped = sim_cli::overrides::load(&sources, None, &[]).unwrap();
    let years = shipped.customers.births_per_woman.len();
    assert_eq!(ovs.len(), years, "one override per year: {ovs:?}");
    for ov in &ovs {
        let text = ov.to_string();
        assert!(text.starts_with(&format!("{key}.")), "{text}");
        let (k, v) = text.split_once('=').unwrap();
        let year = k.rsplit('.').next().unwrap();
        let before = shipped
            .customers
            .births_per_woman
            .iter()
            .find(|(y, _)| y.to_string() == year)
            .map(|(_, b)| serde_json::to_value(b).unwrap().as_f64().unwrap())
            .unwrap();
        let after: f64 = v.parse().unwrap();
        assert!(
            (after - before * factor).abs() < 0.005 + 1e-9,
            "{year}: {before} × {factor} ≠ {after}"
        );
    }
    model_config(&def, &sources, &m, "v1").unwrap();
}

// ---- Seats -----------------------------------------------------------------------------------

#[given(expr = "the roster {string}")]
fn a_roster(w: &mut HarnessWorld, names: String) {
    let entries = names.split(',').map(|n| Entry::from_name(n.trim())).collect();
    w.study.roster = Some(Roster::new(entries));
}

#[when(expr = "I plan {int} games")]
fn plan_games(w: &mut HarnessWorld, games: u32) {
    let seats = w.study.roster.as_ref().unwrap().len();
    w.study.plan = plan(games, seats, 100);
}

#[then(expr = "each of the {int} entries sits in each of the {int} seats {int} times")]
fn equal_seats(w: &mut HarnessWorld, entries: usize, seats: usize, times: usize) {
    let mut count = BTreeMap::<(usize, usize), usize>::new();
    for g in &w.study.plan {
        assert_eq!(g.seating.len(), seats);
        for (seat, entry) in g.seating.iter().enumerate() {
            *count.entry((*entry, seat)).or_default() += 1;
        }
    }
    assert_eq!(count.len(), entries * seats, "{count:?}");
    assert!(count.values().all(|c| *c == times), "{count:?}");
}

#[then("every rotation of a seed is played once")]
fn rotations_per_seed(w: &mut HarnessWorld) {
    let mut by_seed = BTreeMap::<u64, Vec<Vec<usize>>>::new();
    for g in &w.study.plan {
        by_seed.entry(g.seed).or_default().push(g.seating.clone());
    }
    let seats = w.study.roster.as_ref().unwrap().len();
    for (seed, seatings) in by_seed {
        let mut sorted = seatings.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), seats, "seed {seed}: {seatings:?}");
    }
}

#[then(expr = "{int} games are planned")]
fn planned(w: &mut HarnessWorld, games: usize) {
    assert_eq!(w.study.plan.len(), games);
}

#[when("I resolve the roster")]
fn resolve(w: &mut HarnessWorld) {
    let def = w.study.def.clone().unwrap();
    let config = sim_cli::overrides::load(&base(w), None, &[]).unwrap();
    w.study.roster = Some(resolve_roster(&def, &config).unwrap());
}

#[then(expr = "the roster is every personality in ai.yaml followed by {string}")]
fn default_roster(w: &mut HarnessWorld, last: String) {
    let config = sim_cli::overrides::load(&base(w), None, &[]).unwrap();
    let mut want: Vec<String> = config.ai.personalities.keys().cloned().collect();
    want.push(last);
    assert_eq!(w.study.roster.as_ref().unwrap().names(), want);
}

// ---- Metrics ---------------------------------------------------------------------------------

#[when(expr = "I take the Wilson interval of {int} wins in {int} games")]
fn take_wilson(w: &mut HarnessWorld, wins: u64, games: u64) {
    w.study.interval = Some(wilson(wins, games));
}

#[then(expr = "it is {float} to {float} to 4 places")]
fn interval_is(w: &mut HarnessWorld, low: f64, high: f64) {
    let (lo, hi) = w.study.interval.unwrap();
    assert!((lo - low).abs() < 5e-5, "low {lo} ≠ {low}");
    assert!((hi - high).abs() < 5e-5, "high {hi} ≠ {high}");
}

#[when(expr = "the win counts are {string}")]
fn win_counts(w: &mut HarnessWorld, counts: String) {
    w.study.counts = counts.split(',').map(|c| c.trim().parse().unwrap()).collect();
}

#[then(expr = "the normalised entropy is {float} and the Gini index is {float}")]
fn evenness(w: &mut HarnessWorld, entropy: f64, g: f64) {
    let h = normalised_entropy(&w.study.counts);
    let gi = gini(&w.study.counts);
    assert!((h - entropy).abs() < 5e-5, "entropy {h} ≠ {entropy}");
    assert!((gi - g).abs() < 5e-5, "gini {gi} ≠ {g}");
}

#[given(
    expr = "{int} synthetic games where grocer wins {int}, landlord {int}, toff {int}, idle {int}"
)]
fn synthetic(
    w: &mut HarnessWorld,
    games: u64,
    grocer: u64,
    landlord: u64,
    toff: u64,
    idle: u64,
) {
    let roster = w.study.roster.as_ref().unwrap();
    let mut t = Tally::new(roster.len());
    t.games = games;
    t.wins = vec![grocer, landlord, toff, idle];
    t.idle_rank_sum = 4 * games;
    w.study.tally = Some(t);
}

#[given(expr = "{int} AI player-games end bankrupt")]
fn synthetic_bankrupt(w: &mut HarnessWorld, n: u64) {
    let t = w.study.tally.as_mut().unwrap();
    t.bankrupt[0] = n / 2;
    t.bankrupt[1] = n - n / 2;
}

#[given(expr = "the idle seat finishes strictly last in {int} games")]
fn synthetic_idle_last(w: &mut HarnessWorld, n: u64) {
    w.study.tally.as_mut().unwrap().idle_last = n;
}

#[when("I compute the model's metrics")]
fn compute(w: &mut HarnessWorld) {
    let roster = w.study.roster.clone().unwrap();
    let criteria = StudyDef::default().criteria;
    w.study.metrics = Some(metrics(&roster, w.study.tally.as_ref().unwrap(), &criteria));
}

#[then(regex = r"^the model is (balanced|not balanced)$")]
fn verdict(w: &mut HarnessWorld, expected: String) {
    let m = w.study.metrics.as_ref().unwrap();
    assert_eq!(m.balanced, expected == "balanced", "{m:?}");
}

// ---- Running ---------------------------------------------------------------------------------

#[given(
    expr = "a study of {int} models of {int} games from seed {int} with the roster {string} on preset {string}"
)]
fn a_small_study(
    w: &mut HarnessWorld,
    models: u32,
    games: u32,
    seed: u64,
    roster: String,
    preset: String,
) {
    w.study.def = Some(StudyDef {
        models,
        games_per_model: games,
        seed,
        preset,
        roster: Some(roster.split(',').map(|s| s.trim().to_owned()).collect()),
        ..StudyDef::default()
    });
}

fn run(w: &mut HarnessWorld, out: PathBuf, threads: Option<usize>, stop_after: Option<u32>) {
    let def = w.study.def.clone().unwrap();
    let opts = RunOptions {
        out: out.clone(),
        threads,
        games_csv: true,
        wall_time: false,
        stop_after,
        ..RunOptions::default()
    };
    let report = run_study(&def, &base(w), &opts).unwrap();
    w.study.outs.push(out);
    w.study.reports.push(report);
}

#[when("I run the study")]
fn run_once(w: &mut HarnessWorld) {
    run(w, scratch(), None, None);
}

#[when(expr = "I run the study, stopping after {int} models")]
fn run_stopping(w: &mut HarnessWorld, n: u32) {
    run(w, scratch(), None, Some(n));
}

#[when("I run the study again")]
fn run_again(w: &mut HarnessWorld) {
    let out = w.study.outs.last().unwrap().clone();
    run(w, out, None, None);
}

#[when(expr = "I run the study on {int} thread(s) without wall time")]
fn run_threads(w: &mut HarnessWorld, threads: usize) {
    run(w, scratch(), Some(threads), None);
}

/// models.csv of the last run, as (header → value) rows.
fn rows(w: &HarnessWorld) -> Vec<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(w.study.outs.last().unwrap().join("models.csv")).unwrap();
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    lines
        .map(|l| {
            header
                .iter()
                .zip(sim_cli::study::split_csv_line(l))
                .map(|(h, v)| ((*h).to_owned(), v))
                .collect()
        })
        .collect()
}

#[then(expr = "the study finishes with {int} models")]
fn finishes_with(w: &mut HarnessWorld, n: usize) {
    assert!(w.study.reports.last().unwrap().complete);
    assert_eq!(rows(w).len(), n);
}

#[then(expr = "model {int} is {string}")]
fn model_status(w: &mut HarnessWorld, id: u32, status: String) {
    let rows = rows(w);
    let row = rows.iter().find(|r| r["model_id"] == id.to_string()).unwrap();
    assert_eq!(row["status"], status, "{row:?}");
}

#[then(expr = "models {int} and {int} are {string} naming {string}")]
fn models_invalid(w: &mut HarnessWorld, a: u32, b: u32, status: String, key: String) {
    let rows = rows(w);
    for id in [a, b] {
        let row = rows.iter().find(|r| r["model_id"] == id.to_string()).unwrap();
        assert_eq!(row["status"], status, "{row:?}");
        assert!(row["error"].contains(&key), "{row:?}");
    }
    assert_eq!(w.study.reports.last().unwrap().invalid, 2);
}

#[then(expr = "models.csv has {int} models")]
fn csv_models(w: &mut HarnessWorld, n: usize) {
    assert_eq!(rows(w).len(), n);
}

#[then(expr = "{int} models were skipped and {int} were run")]
fn skipped_and_run(w: &mut HarnessWorld, skipped: u32, ran: u32) {
    let r = w.study.reports.last().unwrap();
    assert_eq!((r.skipped, r.ran), (skipped, ran), "{r:?}");
}

#[then("models.csv equals that of the same study run in one go")]
fn equals_one_go(w: &mut HarnessWorld) {
    let resumed = std::fs::read(w.study.outs.last().unwrap().join("models.csv")).unwrap();
    let resumed_games = std::fs::read(w.study.outs.last().unwrap().join("games.csv")).unwrap();
    run(w, scratch(), None, None);
    let fresh = std::fs::read(w.study.outs.last().unwrap().join("models.csv")).unwrap();
    let fresh_games = std::fs::read(w.study.outs.last().unwrap().join("games.csv")).unwrap();
    assert_eq!(
        String::from_utf8(resumed).unwrap(),
        String::from_utf8(fresh).unwrap()
    );
    assert_eq!(resumed_games, fresh_games);
}

#[then("the two runs wrote identical models.csv and games.csv")]
fn identical_runs(w: &mut HarnessWorld) {
    let [a, b] = &w.study.outs[..] else {
        panic!("expected two runs")
    };
    for file in ["models.csv", "games.csv"] {
        let x = std::fs::read_to_string(a.join(file)).unwrap();
        let y = std::fs::read_to_string(b.join(file)).unwrap();
        assert!(x.lines().count() > 1, "{file} is empty");
        assert_eq!(x, y, "{file} differs");
    }
}

#[then(expr = "{string} lists {int} models ranked")]
fn summary_ranked(w: &mut HarnessWorld, file: String, n: usize) {
    let path = w.cli.tmp_dir().join(file);
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ranking = v["ranking"].as_array().unwrap();
    assert_eq!(ranking.len(), n, "{v}");
    let ranks: Vec<u64> = ranking.iter().map(|r| r["rank"].as_u64().unwrap()).collect();
    assert_eq!(ranks, (1..=n as u64).collect::<Vec<_>>());
}

#[then("the overlay of the top model loads when copied over the shipped config")]
fn overlay_loads(w: &mut HarnessWorld) {
    let out = w.cli.tmp_dir().join("out");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("summary.json")).unwrap())
            .unwrap();
    let top = v["top"][0].as_u64().expect("a top model");
    let overlay = out.join("models").join(top.to_string());
    let overrides = std::fs::read_to_string(overlay.join("overrides.txt")).unwrap();
    // Copy the shipped config, then the overlay's YAML files over it.
    let target = scratch();
    std::fs::create_dir_all(&target).unwrap();
    let shipped = super::repo_root().join("config");
    for entry in std::fs::read_dir(&shipped).unwrap().flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
        }
    }
    for entry in std::fs::read_dir(&overlay).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".yaml") {
            std::fs::copy(entry.path(), target.join(&name)).unwrap();
        }
    }
    let adopted =
        sim_core::Config::load(&ConfigSources::read_dir(&target).unwrap()).unwrap();
    // The same config as the shipped one with the model's overrides.
    let parsed: Vec<sim_cli::overrides::Override> = overrides
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| l.trim().trim_start_matches("--set ").parse().unwrap())
        .collect();
    let expected = sim_cli::overrides::load(
        &ConfigSources::read_dir(&shipped).unwrap(),
        None,
        &parsed,
    )
    .unwrap();
    assert_eq!(adopted, expected);
}
