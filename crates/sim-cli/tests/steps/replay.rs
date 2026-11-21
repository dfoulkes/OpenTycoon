//! Steps for `features/replay.feature`: the game recorder, at library level (an in-memory
//! store) and through the `sim-cli` binary (a local-directory store).

use std::sync::Arc;

use cucumber::{given, then, when};
use sim_cli::batch::{BatchRecording, BatchReport, BatchSpec, run_batch_recorded};
use sim_cli::brains::BrainKind;
use sim_cli::game::GameSpec;
use sim_cli::overrides::{self, Override};
use sim_cli::replay::extract::{self, EXTRACTOR_ID};
use sim_cli::replay::fingerprint::GIT_COMMIT;
use sim_cli::replay::format::{Origin, Replay, Seat, SeatKind};
use sim_cli::replay::load;
use sim_cli::replay::manifest::{self, MAX_SEATS, RulesWeight, Weights};
use sim_cli::replay::parquet_table::{self, Value};
use sim_cli::replay::record::{RecordContext, play_recorded, seats_of};
use sim_cli::replay::snapshot;
use sim_cli::replay::store::{LocalStore, MemStore, ObjectStore};
use sim_cli::replay::verify::verify;
use sim_cli::replay::writer::{RunWriter, WriterOptions};
use sim_core::world::Command;
use sim_core::{Config, ConfigSources, Money, World};

use crate::HarnessWorld;

/// Recorder scenario state.
#[derive(Debug, Default)]
pub struct ReplayState {
    /// The rules fingerprint the "build" records and verifies under.
    pub fingerprint: String,
    /// Human multiplier, if a scenario sets one.
    pub human_multiplier: Option<f64>,
    /// Segment size, if a scenario sets one.
    pub segment_bytes: Option<usize>,
    /// The store recordings go into (one per scenario).
    pub store: Option<Arc<MemStore>>,
    /// Runs recorded so far.
    pub runs: u32,
    /// The last recorded batch's results.
    pub report: Option<BatchReport>,
    /// The last recorded run's games, read back from the store.
    pub games: Vec<Replay>,
    /// The last run's manifest file.
    pub manifest: Vec<u8>,
    /// The rules-weight policy.
    pub rules: Option<RulesWeight>,
    /// The last extract: Parquet bytes and rows.
    pub extract: Option<(Vec<u8>, u64)>,
    /// The last verification failure.
    pub message: Option<String>,
}

impl ReplayState {
    fn store(&mut self) -> Arc<MemStore> {
        self.store.get_or_insert_with(Arc::default).clone()
    }

    fn options(&self) -> WriterOptions {
        let mut opts = WriterOptions::default();
        if let Some(m) = self.human_multiplier {
            opts.weights.human_multiplier = m;
        }
        if let Some(b) = self.segment_bytes {
            opts.segment_bytes = b;
        }
        opts
    }

    fn ctx(&self, run_id: &str, config_hash: &str, preset: &str) -> RecordContext {
        RecordContext {
            run_id: run_id.to_owned(),
            rules_fingerprint: self.fingerprint.clone(),
            git_commit: GIT_COMMIT.to_owned(),
            config_hash: config_hash.to_owned(),
            preset: preset.to_owned(),
        }
    }

    /// Reads a run's first manifest part and its games back out of the store.
    fn read_back(&mut self, run_id: &str) {
        let store = self.store();
        let key = format!("manifests/{run_id}/part-00000.parquet");
        self.manifest = store.get(&key).unwrap();
        self.games = load::games(store.as_ref(), &key)
            .unwrap()
            .into_iter()
            .map(|l| l.replay)
            .collect();
    }

    fn config_of(&mut self, replay: &Replay) -> Config {
        load::config(self.store().as_ref(), &replay.header.config_hash).unwrap()
    }
}

/// The effective config files the config steps describe, plus extra overrides.
fn effective(w: &HarnessWorld, extra: &[&str]) -> ConfigSources {
    let sources = w.config.sources.as_ref().expect("no config given");
    let mut ovs: Vec<Override> = w
        .config
        .overrides
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    ovs.extend(extra.iter().map(|s| s.parse::<Override>().unwrap()));
    overrides::resolve(sources, w.config.preset.as_deref(), &ovs).unwrap()
}

#[given(expr = "a build whose rules fingerprint is {string}")]
fn build_fingerprint(w: &mut HarnessWorld, fp: String) {
    w.replay.fingerprint = fp;
}

#[given(expr = "a human multiplier of {float}")]
fn human_multiplier(w: &mut HarnessWorld, m: f64) {
    w.replay.human_multiplier = Some(m);
}

#[given(expr = "segments of {int} KiB")]
fn segments_of(w: &mut HarnessWorld, kib: usize) {
    w.replay.segment_bytes = Some(kib * 1024);
}

fn record_batch(
    w: &mut HarnessWorld,
    games: u64,
    seed: u64,
    brain: &str,
    players: u8,
    extra: &[&str],
) {
    let files = effective(w, extra);
    let config = Config::load(&files).unwrap();
    let snap = snapshot::snapshot(&files);
    w.replay.runs += 1;
    let run_id = format!("test-run-{}", w.replay.runs);
    let preset = w.config.preset.clone().unwrap_or_default();
    let ctx = w.replay.ctx(&run_id, &snap.hash, &preset);
    let store = w.replay.store();
    let opts = w.replay.options();
    let mut writer = RunWriter::new(store.as_ref(), &run_id, opts);
    let spec = BatchSpec {
        games,
        base_seed: seed,
        threads: None,
        game: GameSpec {
            players,
            brain: brain.parse().unwrap(),
            ..GameSpec::default()
        },
    };
    let mut rec = BatchRecording {
        ctx,
        snapshot: snap,
        writer: &mut writer,
        chunk: 4096,
        source: "batch".into(),
    };
    let report = run_batch_recorded(&config, &spec, &mut rec).unwrap();
    w.replay.report = Some(report);
    w.replay.read_back(&run_id);
}

#[when(expr = "I record a batch of {int} games from seed {int} with the {string} brain")]
fn record(w: &mut HarnessWorld, games: u64, seed: u64, brain: String) {
    record_batch(w, games, seed, &brain, 2, &[]);
}

#[when(
    expr = "I record a batch of {int} games from seed {int} with the {string} brain in {int} seats"
)]
fn record_seats(w: &mut HarnessWorld, games: u64, seed: u64, brain: String, players: u8) {
    record_batch(w, games, seed, &brain, players, &[]);
}

#[when(
    expr = "I record a batch of {int} games from seed {int} with the {string} brain into the same store"
)]
fn record_same(w: &mut HarnessWorld, games: u64, seed: u64, brain: String) {
    record_batch(w, games, seed, &brain, 2, &[]);
}

#[when(
    expr = "I record a batch of {int} games from seed {int} with the {string} brain and {string} into the same store"
)]
fn record_same_with(w: &mut HarnessWorld, games: u64, seed: u64, brain: String, ov: String) {
    record_batch(w, games, seed, &brain, 2, &[ov.as_str()]);
}

#[then(expr = "the manifest lists {int} games")]
fn manifest_lists(w: &mut HarnessWorld, games: usize) {
    let rows = manifest::from_parquet(&w.replay.manifest).unwrap();
    assert_eq!(rows.len(), games);
    assert_eq!(w.replay.games.len(), games);
}

#[then("every recorded game re-verifies at every midnight")]
fn every_game_verifies(w: &mut HarnessWorld) {
    let games = w.replay.games.clone();
    for g in &games {
        let config = w.replay.config_of(g);
        let v = verify(g, &config, &w.replay.fingerprint).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(v.checkpoints, g.checkpoints.len());
        assert!(v.checkpoints >= 4, "a v1 game has 4 midnights: {v:?}");
    }
}

#[then("each recorded game ends with the same state hash as the unrecorded one")]
fn same_hashes(w: &mut HarnessWorld) {
    let plain = w.batch.reports.last().expect("no plain batch");
    let recorded = w.replay.report.as_ref().expect("no recorded batch");
    assert_eq!(plain.results.len(), recorded.results.len());
    for ((a, b), g) in plain
        .results
        .iter()
        .zip(&recorded.results)
        .zip(&w.replay.games)
    {
        assert_eq!(a.seed, b.seed);
        assert_eq!(a.state_hash, b.state_hash, "seed {}", a.seed);
        assert_eq!(g.outcome.final_state_hash, a.state_hash, "seed {}", a.seed);
    }
}

#[when(expr = "I tamper with the state hash of midnight {int} of the first game")]
fn tamper_checkpoint(w: &mut HarnessWorld, day: u32) {
    let g = w.replay.games.first_mut().expect("no game");
    let cp = g
        .checkpoints
        .iter_mut()
        .find(|c| c.day == day)
        .expect("no such midnight");
    cp.state_hash ^= 1;
}

#[when(expr = "I raise the opening bid in the first game's first external command by £{int}")]
fn tamper_command(w: &mut HarnessWorld, pounds: i64) {
    let g = w.replay.games.first_mut().expect("no game");
    let rec = g
        .commands
        .iter_mut()
        .find(|c| c.origin == Origin::External)
        .expect("no external command");
    match &mut rec.command {
        Command::OpenLandAuction { bid, .. } => *bid += Money::from_pounds(pounds),
        other => panic!("first external command is {other:?}"),
    }
}

#[when(expr = "the build's rules fingerprint becomes {string}")]
fn fingerprint_becomes(w: &mut HarnessWorld, fp: String) {
    w.replay.fingerprint = fp;
}

#[then(expr = "verifying the first game fails with {string}")]
fn verify_fails(w: &mut HarnessWorld, text: String) {
    let g = w.replay.games.first().expect("no game").clone();
    let config = w.replay.config_of(&g);
    let err = verify(&g, &config, &w.replay.fingerprint).expect_err("verified");
    let msg = err.to_string();
    assert!(msg.contains(&text), "{msg}");
    w.replay.message = Some(msg);
}

#[then(expr = "the message names the replay's rules fingerprint {string}")]
fn message_names(w: &mut HarnessWorld, fp: String) {
    let msg = w.replay.message.as_ref().expect("no message");
    assert!(msg.contains(&fp), "{msg}");
}

#[when("I record one game with a human in seat 0 and one game without")]
fn record_human(w: &mut HarnessWorld) {
    let files = effective(w, &[]);
    let config = Config::load(&files).unwrap();
    let snap = snapshot::snapshot(&files);
    let run_id = "test-human";
    let ctx = w.replay.ctx(run_id, &snap.hash, "v1");
    let spec = GameSpec::default();
    let mut games = Vec::new();
    for (i, human) in [(0u64, true), (1, false)] {
        let world = World::new(config.clone(), i, 2);
        let mut seats = seats_of(&world, BrainKind::Idle);
        if human {
            seats[0] = Seat {
                kind: SeatKind::Human,
                brain: "client".into(),
                brain_version: "test".into(),
                player_id: Some("anon-7f3a".into()),
            };
        }
        let (_, replay) = play_recorded(world, &spec, &ctx, &format!("g{i}"), seats);
        games.push(replay);
    }
    let store = w.replay.store();
    let opts = w.replay.options();
    let mut writer = RunWriter::new(store.as_ref(), run_id, opts);
    writer.put_config(&snap).unwrap();
    writer
        .write_part("part-00000", &games, "test", None)
        .unwrap();
    w.replay.read_back(run_id);
}

fn doc_columns() -> Vec<String> {
    let doc = std::fs::read_to_string(super::repo_root().join("docs/replay.md")).unwrap();
    let section = doc
        .split("## Manifest schema")
        .nth(1)
        .expect("docs/replay.md has no Manifest schema section");
    let section = section.split("\n## ").next().unwrap();
    let mut cols = Vec::new();
    for line in section.lines() {
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let name = rest.split('`').next().unwrap();
        if name.contains("<N>") {
            cols.extend((0..MAX_SEATS).map(|n| name.replace("<N>", &n.to_string())));
        } else {
            cols.push(name.to_owned());
        }
    }
    cols
}

#[then("the manifest has the columns of docs/replay.md")]
fn manifest_columns(w: &mut HarnessWorld) {
    let found = manifest::parquet_columns(&w.replay.manifest).unwrap();
    let mut documented = doc_columns();
    let mut sorted = found.clone();
    sorted.sort();
    documented.sort();
    assert_eq!(sorted, documented);
    assert_eq!(found, manifest::columns());
}

fn row_of(w: &HarnessWorld, game: &str) -> std::collections::BTreeMap<String, Value> {
    parquet_table::read(&w.replay.manifest)
        .unwrap()
        .into_iter()
        .find(|r| r.get("game_id") == Some(&Value::Str(game.into())))
        .unwrap_or_else(|| panic!("no row for {game}"))
}

#[then(expr = "the human game has weight {float} and {int} human seat")]
fn human_weight(w: &mut HarnessWorld, weight: f64, humans: i64) {
    let row = row_of(w, "g0");
    assert_eq!(row["weight"], Value::F64(weight));
    assert_eq!(row["human_seats"], Value::I64(humans));
    assert_eq!(row["seat0_kind"], Value::Str("human".into()));
}

#[then(expr = "the AI-only game has weight {float} and {int} human seats")]
fn ai_weight(w: &mut HarnessWorld, weight: f64, humans: i64) {
    let row = row_of(w, "g1");
    assert_eq!(row["weight"], Value::F64(weight));
    assert_eq!(row["human_seats"], Value::I64(humans));
}

#[then(expr = "the human seat carries the anonymous player id {string} and no name")]
fn anonymous(w: &mut HarnessWorld, id: String) {
    let row = row_of(w, "g0");
    assert_eq!(row["seat0_player_id"], Value::Str(id));
    assert_eq!(row["seat1_player_id"], Value::Null);
    assert!(row.keys().all(|k| !k.contains("name")), "{:?}", row.keys());
}

#[then(expr = "the store holds {int} config snapshot(s)")]
fn config_count(w: &mut HarnessWorld, n: usize) {
    let configs = w.replay.store().list("configs/").unwrap();
    assert_eq!(configs.len(), n, "{configs:?}");
}

#[then("every config snapshot is named by the hash of its content and loads")]
fn configs_load(w: &mut HarnessWorld) {
    let store = w.replay.store();
    for (key, _) in store.list("configs/").unwrap() {
        let hash = key
            .strip_prefix("configs/")
            .and_then(|k| k.strip_suffix(".yaml"))
            .unwrap();
        let yaml = String::from_utf8(store.get(&key).unwrap()).unwrap();
        let files = snapshot::parse(&yaml, hash).unwrap();
        assert_eq!(snapshot::config_hash(&files), hash);
        Config::load(&files).unwrap();
    }
}

#[then("the store holds fewer replay objects than games")]
fn fewer_objects(w: &mut HarnessWorld) {
    let objects = w.replay.store().list("replays/").unwrap();
    let games = w.replay.games.len();
    assert!(
        objects.len() > 1 && objects.len() < games,
        "{} objects for {games} games",
        objects.len()
    );
}

#[then("every manifest row's byte range decodes to that game")]
fn ranges_decode(w: &mut HarnessWorld) {
    let store = w.replay.store();
    let rows = manifest::from_parquet(&w.replay.manifest).unwrap();
    assert_eq!(rows.len(), w.replay.games.len());
    for row in rows {
        let bytes = store
            .get_range(&row.object_key, row.offset, row.size_bytes)
            .unwrap();
        let (replay, used) = sim_cli::replay::decode_one(&bytes).unwrap();
        assert_eq!(used as u64, row.size_bytes);
        assert_eq!(replay.header.game_id, row.game_id);
        assert_eq!(replay.header.seed, row.seed);
    }
}

#[when("I extract the run")]
fn extract_run(w: &mut HarnessWorld) {
    let rules = RulesWeight::new(w.replay.fingerprint.clone());
    let games = w.replay.games.clone();
    let sources: Vec<extract::Source<'_>> = games
        .iter()
        .map(|g| extract::Source {
            replay: g,
            object_key: "test",
            weight: Some(1.0),
        })
        .collect();
    w.replay.extract = Some(extract::extract(&sources, &rules, &Weights::default()).unwrap());
}

#[then("the extract has one row per recorded command")]
fn extract_rows(w: &mut HarnessWorld) {
    let (bytes, rows) = w.replay.extract.as_ref().unwrap();
    let commands: usize = w.replay.games.iter().map(|g| g.commands.len()).sum();
    assert!(commands > 0);
    assert_eq!(*rows, commands as u64);
    assert_eq!(parquet_table::read(bytes).unwrap().len(), commands);
}

#[then("every extract row carries the extractor id, the game id and the rules fingerprint")]
fn extract_lineage(w: &mut HarnessWorld) {
    let (bytes, _) = w.replay.extract.as_ref().unwrap();
    for row in parquet_table::read(bytes).unwrap() {
        assert_eq!(row["extractor_id"], Value::Str(EXTRACTOR_ID.into()));
        assert!(matches!(&row["game_id"], Value::Str(s) if !s.is_empty()));
        assert_eq!(
            row["rules_fingerprint"],
            Value::Str(w.replay.fingerprint.clone())
        );
        assert!(matches!(&row["tick"], Value::I64(t) if *t > 0));
        assert!(matches!(&row["command"], Value::Str(s) if !s.is_empty()));
    }
    let cols = parquet_table::columns(bytes).unwrap();
    assert_eq!(cols, extract::columns());
}

#[given(expr = "rules history {string} with decay {float} and floor {float}")]
fn rules_history(w: &mut HarnessWorld, history: String, decay: f64, floor: f64) {
    let mut rules = RulesWeight::new(w.replay.fingerprint.clone());
    rules.history = history.split(',').map(str::to_owned).collect();
    rules.decay = decay;
    rules.floor = floor;
    w.replay.rules = Some(rules);
}

#[then(expr = "a game recorded under {string} weighs {float}")]
fn weighs(w: &mut HarnessWorld, fp: String, weight: f64) {
    let got = w.replay.rules.as_ref().unwrap().weight(&fp);
    assert!((got - weight).abs() < 1e-12, "{fp}: {got} != {weight}");
}

#[when(expr = "I rewrite the rules fingerprint of run {string} under {string} to {string}")]
fn rewrite_fingerprint(w: &mut HarnessWorld, run: String, dir: String, fp: String) {
    let store = LocalStore::new(w.cli.tmp_dir().join(dir));
    let key = format!("manifests/{run}/part-00000.parquet");
    let mut games: Vec<Replay> = load::games(&store, &key)
        .unwrap()
        .into_iter()
        .map(|l| l.replay)
        .collect();
    for g in &mut games {
        g.header.rules_fingerprint.clone_from(&fp);
    }
    let mut writer = RunWriter::new(&store, &run, WriterOptions::default());
    writer
        .write_part("part-00000", &games, "record", None)
        .unwrap();
}

#[when(
    expr = "I run a batch of {int} games from seed {int} with the {string} brain in {int} seats"
)]
fn plain_batch_seats(w: &mut HarnessWorld, games: u64, seed: u64, brain: String, players: u8) {
    let config = w.config.load_ok();
    let spec = BatchSpec {
        games,
        base_seed: seed,
        threads: None,
        game: GameSpec {
            players,
            brain: brain.parse().unwrap(),
            ..GameSpec::default()
        },
    };
    w.batch
        .reports
        .push(sim_cli::batch::run_batch(&config, &spec).unwrap());
}
