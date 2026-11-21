//! Steps for `features/evolve.feature`: `sim-cli evolve` at library level, plus the binary
//! through `steps::cli`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use sim_cli::evolve::genome::{self, Genome, breed, canon};
use sim_cli::evolve::{
    Candidate, Evaluation, EvolveDef, EvolveOptions, EvolveRecording, evaluate, rank,
    run_evolution, run_evolution_recorded,
};
use sim_cli::overrides::Override;
use sim_cli::replay::load;
use sim_cli::replay::record::RecordContext;
use sim_cli::replay::store::{LocalStore, ObjectStore};
use sim_cli::replay::verify::verify;
use sim_cli::replay::writer::{RunWriter, WriterOptions};
use sim_cli::study::{
    Distribution, Entry, GameOutcome, Kind, Parameter, Roster, RunOptions, StudyDef, run_study,
};
use sim_core::{Config, ConfigSources};

use crate::HarnessWorld;

/// The rules fingerprint recorded games are stamped with in these scenarios.
const FINGERPRINT: &str = "00c0ffee00c0ffee";

/// Scenario state for evolutions.
#[derive(Debug, Default)]
pub struct EvolveState {
    /// The definition under test.
    pub def: Option<EvolveDef>,
    /// Output directories of runs, in order.
    pub outs: Vec<PathBuf>,
    /// The error of the last run that refused to start.
    pub refused: Option<String>,
    /// A gene for the breeding scenarios.
    pub gene: Option<Parameter>,
    /// Children bred in the mutation scenario.
    pub children: Vec<f64>,
    /// The recording store of the last recorded run.
    pub store: Option<PathBuf>,
    /// A copied config directory (champion adoption).
    pub adopted: Option<PathBuf>,
    /// Synthetic results for the fitness scenario.
    pub synthetic: Option<Synthetic>,
}

/// Two genomes' synthetic results: names, wins, games, and how far each finishes above idle.
#[derive(Debug, Clone, Default)]
pub struct Synthetic {
    names: (String, String),
    wins: (usize, usize),
    games: usize,
    above: (i64, i64),
}

impl EvolveState {
    fn def(&mut self) -> &mut EvolveDef {
        self.def.as_mut().expect("no evolution definition given")
    }

    fn out(&self) -> &Path {
        self.outs.last().expect("no evolution run")
    }
}

fn scratch(tag: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "sim-cli-evolve-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn base(w: &HarnessWorld) -> ConfigSources {
    let mut sources =
        w.config.sources.clone().unwrap_or_else(|| {
            ConfigSources::read_dir(&super::repo_root().join("config")).unwrap()
        });
    for o in &w.config.overrides {
        let ov: Override = o.parse().unwrap();
        sim_cli::overrides::apply(&mut sources, &ov).unwrap();
    }
    sources
}

fn options(out: PathBuf, threads: Option<usize>, stop_after: Option<u32>) -> EvolveOptions {
    EvolveOptions {
        out,
        threads,
        wall_time: false,
        fresh: false,
        stop_after,
        progress: false,
    }
}

fn run(w: &mut HarnessWorld, out: PathBuf, threads: Option<usize>, stop_after: Option<u32>) {
    let def = w.evolve.def().clone();
    let report = run_evolution(&def, &base(w), &options(out.clone(), threads, stop_after))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.preset, def.preset);
    w.evolve.outs.push(out);
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every candidate of a run, read back from its generations.csv.
fn candidates(w: &HarnessWorld) -> Vec<Candidate> {
    let def = w.evolve.def.as_ref().unwrap();
    let text = read(&w.evolve.out().join("generations.csv"));
    text.lines()
        .skip(1)
        .map(|l| {
            Candidate::parse(l, |p| Some(genes_of(def, p))).unwrap_or_else(|| panic!("bad row {l}"))
        })
        .collect()
}

fn genes_of<'a>(def: &'a EvolveDef, p: &str) -> Vec<&'a Parameter> {
    def.genes
        .iter()
        .filter(|g| g.all_keys()[0].split('.').nth(1) == Some(p))
        .collect()
}

fn of<'a>(cs: &'a [Candidate], g: u32, p: &str) -> Vec<&'a Candidate> {
    cs.iter()
        .filter(|c| c.generation == g && c.personality == p)
        .collect()
}

/// The text of every output that must match between two runs.
fn outputs(dir: &Path) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for name in ["generations.csv", "summary.json"] {
        files.insert(name.to_owned(), read(&dir.join(name)));
    }
    let champs = dir.join("champions");
    for g in std::fs::read_dir(&champs).unwrap().flatten() {
        for f in std::fs::read_dir(g.path()).unwrap().flatten() {
            let key = format!(
                "champions/{}/{}",
                g.file_name().to_string_lossy(),
                f.file_name().to_string_lossy()
            );
            files.insert(key, read(&f.path()));
        }
    }
    files
}

// ---- Setup -----------------------------------------------------------------------------------

#[given("the evolution definition:")]
fn the_definition(w: &mut HarnessWorld, step: &Step) {
    let text = step.docstring.as_ref().expect("a definition docstring");
    w.evolve.def = Some(EvolveDef::from_yaml(text).unwrap_or_else(|e| panic!("{e}")));
}

#[given(expr = "only {string} evolves")]
fn only_evolves(w: &mut HarnessWorld, p: String) {
    w.evolve.def().evolve = Some(vec![p]);
}

#[given(expr = "the evolution runs {int} generation(s)")]
fn runs_generations(w: &mut HarnessWorld, n: u32) {
    w.evolve.def().generations = n;
}

#[given(expr = "the elitism is {int}")]
fn elitism_is(w: &mut HarnessWorld, n: u32) {
    w.evolve.def().elitism = n;
}

#[given(expr = "the population is {int}")]
fn population_is(w: &mut HarnessWorld, n: u32) {
    w.evolve.def().population = n;
}

#[given(expr = "the gene {string} on {string} from {float} to {float}")]
fn a_gene(w: &mut HarnessWorld, name: String, key: String, min: f64, max: f64) {
    let p = Parameter::set(&name, &key, min, max, Distribution::Uniform, Kind::Decimal);
    w.evolve.def().genes.push(p);
}

#[given(expr = "the gene {string} on {string} with values {string}")]
fn a_categorical_gene(w: &mut HarnessWorld, name: String, key: String, values: String) {
    let values: Vec<&str> = values.split(',').map(str::trim).collect();
    let p = Parameter::categorical(&name, &key, &values);
    w.evolve.def().genes.push(p);
}

// ---- Running ---------------------------------------------------------------------------------

/// Plain runs of the same definition and base are deterministic, so scenarios share them:
/// the debug build plays games slowly.
static RUNS: std::sync::Mutex<BTreeMap<String, PathBuf>> = std::sync::Mutex::new(BTreeMap::new());

#[when("I evolve")]
fn evolve_once(w: &mut HarnessWorld) {
    let key = serde_json::to_string(&(w.evolve.def(), &w.config.overrides)).unwrap();
    let cached = RUNS.lock().unwrap().get(&key).cloned();
    if let Some(out) = cached {
        w.evolve.outs.push(out);
        return;
    }
    run(w, scratch("run"), None, None);
    RUNS.lock().unwrap().insert(key, w.evolve.out().to_owned());
}

#[when("I evolve again into a new directory")]
fn evolve_again(w: &mut HarnessWorld) {
    run(w, scratch("again"), None, None);
}

#[when(expr = "I evolve from seed {int}")]
fn evolve_from_seed(w: &mut HarnessWorld, seed: u64) {
    w.evolve.def().seed = seed;
    run(w, scratch("seed"), None, None);
}

#[when(expr = "I evolve on {int} thread(s)")]
fn evolve_threads(w: &mut HarnessWorld, threads: usize) {
    run(w, scratch("threads"), Some(threads), None);
}

#[when(expr = "I evolve into a new directory, stopping after {int} generation(s)")]
fn evolve_stopping(w: &mut HarnessWorld, n: u32) {
    run(w, scratch("resume"), None, Some(n));
}

#[when("I evolve that directory again")]
fn evolve_resume(w: &mut HarnessWorld) {
    let out = w.evolve.out().to_owned();
    w.evolve.outs.pop();
    run(w, out, None, None);
}

#[when(expr = "generation {int} is cut off mid-write in that directory")]
fn cut_off(w: &mut HarnessWorld, g: u32) {
    // A crash part-way through: some of generation g's rows, then half a line; no lineup.json.
    let path = w.evolve.out().join("generations.csv");
    let text = read(&path);
    let row = text
        .lines()
        .nth(1)
        .unwrap()
        .replacen("0,", &format!("{g},"), 1);
    std::fs::write(&path, format!("{text}{row}\n{g},toff,1,chi")).unwrap();
}

#[when("I evolve, recording every game")]
fn evolve_recorded(w: &mut HarnessWorld) {
    let def = w.evolve.def().clone();
    let store_dir = scratch("store");
    let store = LocalStore::new(&store_dir);
    let mut writer = RunWriter::new(&store, "test-evolve", WriterOptions::default());
    let mut rec = EvolveRecording {
        ctx: RecordContext {
            run_id: "test-evolve".into(),
            rules_fingerprint: FINGERPRINT.into(),
            git_commit: "test".into(),
            ..RecordContext::default()
        },
        writer: &mut writer,
    };
    let out = scratch("recorded");
    run_evolution_recorded(
        &def,
        &base(w),
        &options(out.clone(), None, None),
        Some(&mut rec),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    w.evolve.outs.push(out);
    w.evolve.store = Some(store_dir);
}

// ---- Determinism and resume -----------------------------------------------------------------

#[then("both evolutions wrote identical generations.csv, champions and summary.json")]
fn identical(w: &mut HarnessWorld) {
    let n = w.evolve.outs.len();
    let (a, b) = (
        outputs(&w.evolve.outs[n - 2]),
        outputs(&w.evolve.outs[n - 1]),
    );
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (k, v) in &a {
        assert_eq!(v, &b[k], "{k} differs");
    }
}

#[then("its generations.csv differs from the first")]
fn differs(w: &mut HarnessWorld) {
    let first = read(&w.evolve.outs[0].join("generations.csv"));
    let last = read(&w.evolve.out().join("generations.csv"));
    assert_ne!(first, last);
}

// ---- The algorithm --------------------------------------------------------------------------

#[then(expr = "candidate 0 of generation 0 of {string} has the shipped genome {string}")]
fn base_genome(w: &mut HarnessWorld, p: String, genome_text: String) {
    let cs = candidates(w);
    let c = of(&cs, 0, &p)[0];
    let def = w.evolve.def.as_ref().unwrap();
    assert_eq!(genome::label(&genes_of(def, &p), &c.genome), genome_text);
    assert_eq!(c.origin.kind(), "base");
}

#[then(expr = "every candidate plays {int} games")]
fn every_plays(w: &mut HarnessWorld, n: u64) {
    for c in candidates(w) {
        assert_eq!(c.result.unwrap().games, n);
    }
}

#[then("each personality's champion of generation 0 is in generation 1 unchanged, as an elite")]
fn champion_kept(w: &mut HarnessWorld) {
    let cs = candidates(w);
    for p in ["grocer", "toff"] {
        let champ = of(&cs, 0, p).into_iter().find(|c| c.champion).unwrap();
        let kept = of(&cs, 1, p)
            .into_iter()
            .find(|c| genome::same(&c.genome, &champ.genome))
            .unwrap_or_else(|| panic!("{p}: champion {:?} lost", champ.genome));
        assert_eq!(kept.origin.kind(), "elite");
        assert_eq!(kept.origin.parents(), champ.index.to_string());
    }
}

#[then(expr = "generation 1's elites are generation 0's top {int} in rank order")]
fn elites_in_order(w: &mut HarnessWorld, k: u32) {
    let cs = candidates(w);
    for p in ["grocer", "toff"] {
        let mut top: Vec<&Candidate> = of(&cs, 0, p)
            .into_iter()
            .filter(|c| c.rank.is_some_and(|r| r <= k))
            .collect();
        top.sort_by_key(|c| c.rank);
        let next = of(&cs, 1, p);
        for (i, t) in top.iter().enumerate() {
            assert_eq!(next[i].origin.kind(), "elite", "{p} candidate {i}");
            assert!(genome::same(&next[i].genome, &t.genome), "{p} elite {i}");
        }
        assert_eq!(next[top.len()].origin.kind(), "child");
    }
}

#[given(expr = "a {word} {word} gene from {float} to {float}")]
fn a_test_gene(w: &mut HarnessWorld, dist: String, kind: String, min: f64, max: f64) {
    let dist = match dist.as_str() {
        "uniform" => Distribution::Uniform,
        "log-uniform" => Distribution::LogUniform,
        other => panic!("{other}"),
    };
    let kind = match kind.as_str() {
        "integer" => Kind::Integer,
        "decimal" => Kind::Decimal,
        other => panic!("{other}"),
    };
    w.evolve.gene = Some(Parameter::set(
        "x",
        "ai.toff.aggression",
        min,
        max,
        dist,
        kind,
    ));
}

#[given(expr = "mutation at rate {float} with sigma {float}")]
fn mutation_at(w: &mut HarnessWorld, rate: f64, sigma: f64) {
    let def = w.evolve.def();
    def.mutation.rate = rate;
    def.mutation.sigma = sigma;
}

#[when(expr = "I breed {int} children from parents at the edges of the range")]
fn breed_children(w: &mut HarnessWorld, n: u32) {
    let gene = w.evolve.gene.clone().unwrap();
    let (lo, hi) = gene.range();
    let genes = [&gene];
    let mut def = w.evolve.def().clone();
    def.population = n + 2;
    def.elitism = 2;
    let ranked = vec![
        (0, Genome(vec![canon(&gene, hi)])),
        (1, Genome(vec![canon(&gene, lo)])),
    ];
    let kids = breed(&genes, &ranked, &ranked[0].1, &def, "toff", 1);
    assert_eq!(kids.len(), (n + 2) as usize);
    w.evolve.children = kids[2..].iter().map(|(g, _)| g.0[0]).collect();
}

#[then(expr = "every child's gene is between {float} and {float}")]
fn children_in_range(w: &mut HarnessWorld, min: f64, max: f64) {
    assert_ne!(w.evolve.children.len(), 0);
    for v in &w.evolve.children {
        assert!((min..=max).contains(v), "{v} outside [{min}, {max}]");
    }
    // A step of sigma = the whole range reaches both ends.
    assert!(w.evolve.children.contains(&min) && w.evolve.children.contains(&max));
}

#[then("every child's gene is a value the gene can take")]
fn children_canonical(w: &mut HarnessWorld) {
    let gene = w.evolve.gene.clone().unwrap();
    for v in &w.evolve.children {
        assert_eq!(v.to_bits(), canon(&gene, *v).to_bits(), "{v}");
    }
}

/// Synthetic games: roster A, B, idle, seats rotated.
fn synthetic(
    a_wins: usize,
    b_wins: usize,
    games: usize,
    a_above: i64,
    b_above: i64,
) -> Vec<GameOutcome> {
    (0..games)
        .map(|i| {
            let winner = if i < a_wins {
                Some(0)
            } else if i < a_wins + b_wins {
                Some(1)
            } else {
                None
            };
            GameOutcome {
                index: u32::try_from(i).unwrap(),
                seed: 0,
                rotation: 0,
                seating: vec![0, 1, 2],
                winner,
                tie: winner.is_none(),
                net_worth: vec![1_000 + a_above, 1_000 + b_above, 1_000],
                bankrupt: vec![false, false, false],
                days: 4,
                ticks: 0,
                state_hash: 0,
            }
        })
        .collect()
}

fn synthetic_roster() -> Roster {
    Roster::new(vec![
        Entry::from_name("a"),
        Entry::from_name("b"),
        Entry::Idle,
    ])
}

#[given(
    expr = "synthetic games where genome {string} wins {int} of {int} and genome {string} wins {int} of {int}"
)]
fn synthetic_games(
    w: &mut HarnessWorld,
    first: String,
    first_wins: usize,
    games: usize,
    second: String,
    second_wins: usize,
    games_again: usize,
) {
    assert_eq!(games, games_again);
    w.evolve.synthetic = Some(Synthetic {
        names: (first, second),
        wins: (first_wins, second_wins),
        games,
        above: (0, 0),
    });
}

#[given(expr = "{string} finishes further above the idle seat than {string} in every game")]
fn further_above(w: &mut HarnessWorld, first: String, second: String) {
    let syn = w.evolve.synthetic.as_mut().unwrap();
    assert_eq!(syn.names, (first, second));
    syn.above = (5_000, 2_000);
}

fn ranked(first: Evaluation, second: Evaluation) -> Vec<u32> {
    rank(&[(1, second), (0, first)], 0.5)
}

#[then(expr = "{string} ranks above {string}")]
fn ranks_above(w: &mut HarnessWorld, first: String, second: String) {
    let syn = w.evolve.synthetic.clone().unwrap();
    assert_eq!(syn.names, (first, second));
    let games = synthetic(syn.wins.0, syn.wins.1, syn.games, syn.above.0, syn.above.1);
    let roster = synthetic_roster();
    let (one, two) = (evaluate(&roster, 0, &games), evaluate(&roster, 1, &games));
    assert_eq!((one.wins, two.wins), (syn.wins.0 as u64, syn.wins.1 as u64));
    assert_eq!(
        (one.nw_vs_idle, two.nw_vs_idle),
        (Some(syn.above.0), Some(syn.above.1))
    );
    assert_eq!(ranked(one, two), vec![0, 1]);
}

#[then("with equal wins, the genome further above idle ranks higher")]
fn tie_break(_w: &mut HarnessWorld) {
    let roster = synthetic_roster();
    let games = synthetic(3, 3, 12, 900, 4_000);
    let (a, b) = (evaluate(&roster, 0, &games), evaluate(&roster, 1, &games));
    assert_eq!(ranked(a, b), vec![1, 0]);
}

#[then("with equal wins and net worth, the genome that went bankrupt ranks lower")]
fn bankrupt_lower(_w: &mut HarnessWorld) {
    let roster = synthetic_roster();
    let mut games = synthetic(3, 3, 12, 1_000, 1_000);
    games[11].bankrupt[0] = true;
    let (a, b) = (evaluate(&roster, 0, &games), evaluate(&roster, 1, &games));
    assert_eq!(a.bankruptcies, 1);
    assert_eq!(ranked(a, b), vec![1, 0]);
}

#[then(expr = "the evolution finishes {int} generations")]
fn finishes(w: &mut HarnessWorld, n: u32) {
    let s: serde_json::Value =
        serde_json::from_str(&read(&w.evolve.out().join("summary.json"))).unwrap();
    assert_eq!(s["generations"]["finished"], n);
    assert_eq!(s["generations"]["complete"], true);
}

#[then(expr = "some candidates are invalid, naming {string}")]
fn some_invalid(w: &mut HarnessWorld, key: String) {
    let cs = candidates(w);
    let bad: Vec<&Candidate> = cs.iter().filter(|c| c.result.is_err()).collect();
    assert!(!bad.is_empty(), "no invalid candidate");
    assert!(cs.iter().any(|c| c.result.is_ok()));
    for c in bad {
        let e = c.result.as_ref().unwrap_err();
        assert!(e.contains(&key), "{e}");
    }
}

#[then("no invalid candidate is ranked, a champion or an elite")]
fn invalid_never_kept(w: &mut HarnessWorld) {
    let cs = candidates(w);
    for c in cs.iter().filter(|c| c.result.is_err()) {
        assert_eq!((c.rank, c.champion), (None, false));
        let next = of(&cs, c.generation + 1, &c.personality);
        assert!(
            !next
                .iter()
                .any(|n| n.origin.kind() == "elite" && n.origin.parents() == c.index.to_string()),
            "invalid {} carried over",
            c.index
        );
    }
}

// ---- Outputs ---------------------------------------------------------------------------------

#[when("I copy champions/0/ai.yaml over a copy of the config")]
fn adopt_champion(w: &mut HarnessWorld) {
    let dir = scratch("adopted");
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(super::repo_root().join("config"))
        .unwrap()
        .flatten()
    {
        if e.path().is_file() {
            std::fs::copy(e.path(), dir.join(e.file_name())).unwrap();
        }
    }
    std::fs::copy(
        w.evolve.out().join("champions/0/ai.yaml"),
        dir.join("ai.yaml"),
    )
    .unwrap();
    w.evolve.adopted = Some(dir);
}

fn adopted(w: &HarnessWorld) -> ConfigSources {
    ConfigSources::read_dir(w.evolve.adopted.as_ref().unwrap()).unwrap()
}

#[then("the copied config loads")]
fn adopted_loads(w: &mut HarnessWorld) {
    Config::load(&adopted(w)).unwrap_or_else(|e| panic!("{e}"));
}

fn lineup(w: &HarnessWorld, g: u32) -> serde_json::Value {
    serde_json::from_str(&read(
        &w.evolve.out().join(format!("champions/{g}/lineup.json")),
    ))
    .unwrap()
}

#[then("playing generation 0's games with it gives the lineup's state hashes")]
fn reproduces(w: &mut HarnessWorld) {
    // An ordinary study of the adopted config: its model 0 plays the same seeds and seatings.
    let def = w.evolve.def.clone().unwrap();
    let study = StudyDef {
        models: 1,
        games_per_model: def.games_per_eval,
        preset: def.preset.clone(),
        seed: def.seed,
        roster: def.roster.clone(),
        ..StudyDef::default()
    };
    let out = scratch("replay-study");
    let opts = RunOptions {
        out: out.clone(),
        games_csv: true,
        ..RunOptions::default()
    };
    run_study(&study, &adopted(w), &opts).unwrap_or_else(|e| panic!("{e}"));
    let played: Vec<String> = read(&out.join("games.csv"))
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(7).unwrap().to_owned())
        .collect();
    let recorded: Vec<String> = lineup(w, 0)["games"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["state_hash"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(played.len(), 3);
    assert_eq!(played, recorded);
}

#[then(expr = "they are the games the {string} champion was evaluated in")]
fn champion_games(w: &mut HarnessWorld, p: String) {
    let cs = candidates(w);
    let champ = of(&cs, 0, &p).into_iter().find(|c| c.champion).unwrap();
    assert_eq!(lineup(w, 0)["games_digest"], champ.games_digest.as_str());
}

#[then(
    expr = "summary.json has a best and mean fitness curve of {int} points for {string} and {string}"
)]
fn curves(w: &mut HarnessWorld, points: usize, first: String, second: String) {
    let summary: serde_json::Value =
        serde_json::from_str(&read(&w.evolve.out().join("summary.json"))).unwrap();
    let cs = candidates(w);
    for p in [first, second] {
        let best = summary["personalities"][&p]["best_fitness"]
            .as_array()
            .unwrap();
        let mean = summary["personalities"][&p]["mean_fitness"]
            .as_array()
            .unwrap();
        assert_eq!((best.len(), mean.len()), (points, points), "{p}");
        for (gen_index, value) in best.iter().enumerate() {
            let champ = of(&cs, u32::try_from(gen_index).unwrap(), &p)
                .into_iter()
                .find(|c| c.champion)
                .unwrap();
            let fitness = champ.result.as_ref().unwrap().fitness(0.5);
            assert!(
                (value.as_f64().unwrap() - fitness).abs() < 1e-9,
                "{p} gen {gen_index}"
            );
            assert!(mean[gen_index].as_f64().unwrap() <= fitness + 1e-9);
        }
    }
}

#[then("summary.json names each generation's champion genome")]
fn names_champions(w: &mut HarnessWorld) {
    let s: serde_json::Value =
        serde_json::from_str(&read(&w.evolve.out().join("summary.json"))).unwrap();
    let cs = candidates(w);
    for p in ["grocer", "toff"] {
        let ids = s["personalities"][p]["champion"].as_array().unwrap();
        for (g, id) in ids.iter().enumerate() {
            let champ = of(&cs, u32::try_from(g).unwrap(), p)
                .into_iter()
                .find(|c| c.champion)
                .unwrap();
            assert_eq!(id.as_str().unwrap(), champ.genome_id);
            assert_eq!(champ.genome_id.len(), 16);
        }
    }
}

fn store(w: &HarnessWorld) -> LocalStore {
    LocalStore::new(w.evolve.store.as_ref().unwrap())
}

/// Every manifest part of the recorded run: part name → its games.
fn recorded(w: &HarnessWorld) -> BTreeMap<String, Vec<load::Loaded>> {
    let s = store(w);
    s.list("manifests/test-evolve/")
        .unwrap()
        .into_iter()
        .map(|(key, _)| {
            let part = key
                .rsplit('/')
                .next()
                .unwrap()
                .trim_end_matches(".parquet")
                .to_owned();
            (part, load::games(&s, &key).unwrap())
        })
        .collect()
}

#[then(
    expr = "every candidate's games are recorded, {int} each, plus {int} lineup games per generation"
)]
fn all_recorded(w: &mut HarnessWorld, per: usize, lineup_games: usize) {
    let parts = recorded(w);
    let cs = candidates(w);
    for c in &cs {
        let part = format!(
            "part-g{:04}-{}-c{:03}",
            c.generation, c.personality, c.index
        );
        assert_eq!(parts.get(&part).map(Vec::len), Some(per), "{part}");
    }
    let gens = cs.iter().map(|c| c.generation).max().unwrap() + 1;
    for g in 0..gens {
        let part = format!("part-g{g:04}-lineup");
        assert_eq!(parts.get(&part).map(Vec::len), Some(lineup_games), "{part}");
    }
    assert_eq!(parts.len(), cs.len() + gens as usize);
}

#[then(
    "each recorded seat of an evolving personality carries that candidate's or champion's genome id"
)]
fn genome_ids(w: &mut HarnessWorld) {
    let parts = recorded(w);
    let cs = candidates(w);
    let champion = |g: Option<u32>, p: &str| -> String {
        match g {
            Some(g) => of(&cs, g, p)
                .into_iter()
                .find(|c| c.champion)
                .unwrap()
                .genome_id
                .clone(),
            None => of(&cs, 0, p)
                .into_iter()
                .find(|c| c.index == 0)
                .unwrap()
                .genome_id
                .clone(),
        }
    };
    for c in &cs {
        let part = format!(
            "part-g{:04}-{}-c{:03}",
            c.generation, c.personality, c.index
        );
        for game in &parts[&part] {
            for seat in &game.replay.header.seats {
                let Some(p) = seat.core_personality() else {
                    continue;
                };
                let want = if p == c.personality {
                    c.genome_id.clone()
                } else {
                    // The other evolving personality plays its previous champion (base in gen 0).
                    champion(c.generation.checked_sub(1), p)
                };
                assert_eq!(seat.brain_version, want, "{part}: seat {p}");
            }
            assert_eq!(game.row.as_ref().unwrap().source, "evolve");
        }
    }
    let gens = cs.iter().map(|c| c.generation).max().unwrap() + 1;
    for g in 0..gens {
        for game in &parts[&format!("part-g{g:04}-lineup")] {
            for seat in &game.replay.header.seats {
                if let Some(p) = seat.core_personality() {
                    assert_eq!(seat.brain_version, champion(Some(g), p), "lineup {g}: {p}");
                }
            }
        }
    }
}

#[then("the recorded games re-verify at every midnight")]
fn re_verify(w: &mut HarnessWorld) {
    let s = store(w);
    for (_, games) in recorded(w) {
        for g in games {
            let config = load::config(&s, &g.replay.header.config_hash).unwrap();
            verify(&g.replay, &config, FINGERPRINT).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}

#[given(expr = "the definition is saved in the scratch directory as {string}")]
fn save_definition(w: &mut HarnessWorld, name: String) {
    let text = serde_yaml_ng::to_string(w.evolve.def.as_ref().unwrap()).unwrap();
    std::fs::write(w.cli.tmp_dir().join(name), text).unwrap();
}

#[then(
    expr = "the evolution in {string} has generations.csv, champions\\/0\\/ai.yaml, champions\\/1\\/ai.yaml and summary.json"
)]
fn cli_outputs(w: &mut HarnessWorld, dir: String) {
    let dir = PathBuf::from(dir.replace("{tmp}", &w.cli.tmp_dir().display().to_string()));
    for f in [
        "generations.csv",
        "champions/0/ai.yaml",
        "champions/1/ai.yaml",
        "summary.json",
    ] {
        assert!(dir.join(f).is_file(), "{f} missing");
    }
}

// ---- Guard rails ----------------------------------------------------------------------------

#[then(expr = "the evolution refuses to start, naming {string} and {string}")]
fn refuses_naming_both(w: &mut HarnessWorld, a: String, b: String) {
    let e = refusal(w);
    assert!(e.contains(&a) && e.contains(&b), "{e}");
}

#[then(expr = "the evolution refuses to start, naming {string}")]
fn refuses_naming(w: &mut HarnessWorld, a: String) {
    let e = refusal(w);
    assert!(e.contains(&a), "{e}");
}

/// Validates the definition and starts the run; returns the refusal.
fn refusal(w: &mut HarnessWorld) -> String {
    let def = w.evolve.def().clone();
    let out = scratch("refused");
    let err = def
        .validate()
        .err()
        .map(|e| e.to_string())
        .or_else(|| {
            run_evolution(&def, &base(w), &options(out.clone(), None, None))
                .err()
                .map(|e| e.to_string())
        })
        .expect("the evolution started");
    assert!(
        !out.join("generations.csv").exists()
            || read(&out.join("generations.csv")).lines().count() <= 1
    );
    w.evolve.refused = Some(err.clone());
    err
}

#[then("every champions ai.yaml loads with the Grocer's opportunism off")]
fn grocer_pure(w: &mut HarnessWorld) {
    let mut n = 0;
    for g in std::fs::read_dir(w.evolve.out().join("champions"))
        .unwrap()
        .flatten()
    {
        let ai = read(&g.path().join("ai.yaml"));
        let sources = base(w).with("ai.yaml", ai);
        let config = Config::load(&sources).unwrap_or_else(|e| panic!("{e}"));
        assert!(!config.ai.grocer.opportunism.enabled);
        n += 1;
    }
    assert_eq!(n, 2);
}
