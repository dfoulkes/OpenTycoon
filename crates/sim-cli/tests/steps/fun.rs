//! Steps for `features/fun.feature` (R79): synthetic midnight boards and winner sequences at
//! library level, plus the command line for rescore, confirm and evolve through `steps::cli`.

use std::collections::BTreeMap;

use cucumber::{given, then, when};
use sim_cli::game::{GameSpec, play_from};
use sim_cli::study::fun::{
    Band, Board, FunGame, FunMetrics, Holdings, fmt_ppm, fun_game, fun_metrics, parse_ppm,
};
use sim_cli::study::output::{Columns, Row, rank};
use sim_cli::study::run::play_one;
use sim_cli::study::{Entry, GameOutcome, GamePlan, Roster, seats};
use sim_core::World;

use crate::HarnessWorld;

/// Scenario state for the fun score.
#[derive(Debug, Default)]
pub struct FunState {
    /// Midnight boards of a synthetic game.
    pub boards: Vec<Board>,
    /// Its outright winner's seat.
    pub winner: Option<u8>,
    /// The measured game.
    pub game: Option<FunGame>,
    /// Synthetic games of one model.
    pub games: Vec<GameOutcome>,
    /// The model's fun metrics.
    pub metrics: Option<FunMetrics>,
    /// A band under test and its last score.
    pub band: Option<Band>,
    /// The last score.
    pub score: Option<i64>,
    /// Ranked rows: header and rows.
    pub ranked: Option<(Columns, Vec<Row>, Vec<u32>)>,
    /// State hashes of a game played measuring fun and plainly.
    pub hashes: Option<(u64, u64)>,
}

fn parse_boards(s: &str) -> Vec<Board> {
    s.split(',')
        .map(|m| {
            m.trim()
                .split('/')
                .map(|v| {
                    let v = v.trim();
                    match v.strip_suffix('x') {
                        Some(n) => (n.parse().unwrap(), true),
                        None => (v.parse().unwrap(), false),
                    }
                })
                .collect()
        })
        .collect()
}

fn roster(w: &HarnessWorld) -> Roster {
    w.study.roster.clone().expect("no roster given")
}

fn game(index: u32, seating: Vec<usize>, winner: Option<usize>, tie: bool) -> GameOutcome {
    let n = seating.len();
    GameOutcome {
        index,
        seed: u64::from(index) / n as u64,
        rotation: u32::try_from(seating[0]).unwrap(),
        seating,
        winner,
        tie,
        net_worth: vec![0; n],
        bankrupt: vec![false; n],
        days: 0,
        ticks: 0,
        state_hash: 0,
        fun: FunGame::default(),
    }
}

// ---- One game --------------------------------------------------------------------------------

#[given(expr = "a game whose midnight net worths are {string}")]
fn boards(w: &mut HarnessWorld, s: String) {
    w.fun.boards = parse_boards(&s);
}

#[given(regex = r"^seat (\d+|none) wins outright$")]
fn winner(w: &mut HarnessWorld, seat: String) {
    w.fun.winner = seat.parse().ok();
}

#[when("I measure the game's fun")]
fn measure(w: &mut HarnessWorld) {
    let seats = w.fun.boards.first().map_or(0, Vec::len);
    w.fun.game = Some(fun_game(
        &w.fun.boards,
        w.fun.winner,
        vec![Holdings::default(); seats],
        true,
    ));
}

fn measured(w: &HarnessWorld) -> &FunGame {
    w.fun.game.as_ref().expect("game not measured")
}

#[then(expr = "it has {int} lead changes")]
fn lead_changes(w: &mut HarnessWorld, n: u32) {
    assert_eq!(measured(w).lead_changes, n, "{:?}", measured(w));
}

#[then(expr = "the halfway midnight is day {int}")]
fn halfway(w: &mut HarnessWorld, day: u32) {
    assert_eq!(measured(w).halfway_day, day);
}

#[then(regex = r"^the game (is|is not|is undecided as) a comeback$")]
fn comeback(w: &mut HarnessWorld, verdict: String) {
    let expected = match verdict.as_str() {
        "is" => Some(true),
        "is not" => Some(false),
        _ => None,
    };
    assert_eq!(measured(w).comeback, expected, "{:?}", measured(w));
}

#[then(expr = "the winning margin is {word}")]
fn margin(w: &mut HarnessWorld, m: String) {
    assert_eq!(measured(w).margin_ppm.map(fmt_ppm), Some(m));
}

// ---- One model -------------------------------------------------------------------------------

#[given(expr = "the winners in seed order are {string}")]
fn winners(w: &mut HarnessWorld, s: String) {
    let roster = roster(w);
    let n = roster.len();
    let names: Vec<String> = s
        .split(',')
        .flat_map(|item| {
            let item = item.trim();
            match item.split_once('*') {
                Some((name, k)) => vec![name.to_owned(); k.parse().unwrap()],
                None => vec![item.to_owned()],
            }
        })
        .collect();
    w.fun.games = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let i = u32::try_from(i).unwrap();
            let seating = seats::seating(n, i as usize % n);
            let winner = roster.entries().iter().position(|e| e.name() == name);
            game(i, seating, winner, name == "tie")
        })
        .collect();
}

#[given(
    expr = "over {int} games grocer holds {string}, landlord holds {string} and speculator holds {string}"
)]
fn holdings(w: &mut HarnessWorld, games: u32, g: String, l: String, s: String) {
    let roster = roster(w);
    let n = roster.len();
    let parse = |s: &str| {
        let v: Vec<u32> = s.split('/').map(|x| x.parse().unwrap()).collect();
        Holdings {
            flats: v[0],
            shops: v[1],
            land: v[2],
            hotels: v[3],
            other: v[4],
            leases: v[1] + v[2],
            businesses: v[0] + v[1] + v[3] + v[4],
        }
    };
    let by_name: BTreeMap<&str, Holdings> = [
        ("grocer", parse(&g)),
        ("landlord", parse(&l)),
        ("speculator", parse(&s)),
    ]
    .into_iter()
    .collect();
    w.fun.games = (0..games)
        .map(|i| {
            let seating = seats::seating(n, i as usize % n);
            let mut g = game(i, seating.clone(), Some(0), false);
            g.fun.full_holdings = true;
            g.fun.holdings = seating
                .iter()
                .map(|e| {
                    by_name
                        .get(roster.entries()[*e].name())
                        .copied()
                        .unwrap_or_default()
                })
                .collect();
            g
        })
        .collect();
}

#[when("I compute the model's fun")]
fn compute(w: &mut HarnessWorld) {
    w.fun.metrics = Some(fun_metrics(&roster(w), &w.fun.games));
}

fn metrics(w: &HarnessWorld) -> &FunMetrics {
    w.fun.metrics.as_ref().expect("fun not computed")
}

#[then(expr = "the strongest AI is {string}")]
fn strongest(w: &mut HarnessWorld, name: String) {
    let i = metrics(w).strongest.expect("no strongest AI");
    assert_eq!(roster(w).entries()[i].name(), name);
}

#[then(expr = "the upset rate is {word}")]
fn upset(w: &mut HarnessWorld, rate: String) {
    assert_eq!(fmt_ppm(metrics(w).upset_ppm), rate);
}

fn z(w: &HarnessWorld) -> i64 {
    metrics(w).streak_z_ppm.expect("no streak statistic")
}

#[then(expr = "the streak statistic is above {float}")]
fn streak_above(w: &mut HarnessWorld, v: f64) {
    assert!(
        z(w) > sim_cli::study::fun::to_ppm(v),
        "z = {}",
        fmt_ppm(z(w))
    );
}

#[then(expr = "the streak statistic is between {float} and {float}")]
fn streak_between(w: &mut HarnessWorld, lo: f64, hi: f64) {
    let (lo, hi) = (
        sim_cli::study::fun::to_ppm(lo),
        sim_cli::study::fun::to_ppm(hi),
    );
    assert!((lo..=hi).contains(&z(w)), "z = {}", fmt_ppm(z(w)));
}

#[then(expr = "the longest winning streak is {int}")]
fn longest(w: &mut HarnessWorld, n: u32) {
    assert_eq!(metrics(w).streak_longest, n);
}

#[then(expr = "the streak scores {word}")]
fn streak_score(w: &mut HarnessWorld, score: String) {
    let band = sim_cli::study::fun::FunDef::default().streak;
    assert_eq!(fmt_ppm(band.score_ppm(Some(z(w)))), score);
}

#[then(expr = "the playstyle distinctness is {word}")]
fn distinctness(w: &mut HarnessWorld, d: String) {
    assert_eq!(metrics(w).distinctness_ppm.map(fmt_ppm), Some(d));
}

// ---- Scoring and ranking ---------------------------------------------------------------------

#[given(expr = "the band from {word} to {word} with a falloff of {float}")]
fn band(w: &mut HarnessWorld, min: String, max: String, falloff: f64) {
    let edge = |s: &str| (s != "-").then(|| s.parse::<f64>().unwrap());
    w.fun.band = Some(Band::new(edge(&min), edge(&max), falloff));
}

#[when(expr = "I score {word} against it")]
fn score(w: &mut HarnessWorld, value: String) {
    let b = w.fun.band.as_ref().unwrap();
    w.fun.score = Some(b.score_ppm(parse_ppm(&value)));
}

#[then(expr = "the score is {word}")]
fn score_is(w: &mut HarnessWorld, s: String) {
    assert_eq!(w.fun.score.map(fmt_ppm), Some(s));
}

#[given("these finished models")]
fn finished_models(w: &mut HarnessWorld, step: &cucumber::gherkin::Step) {
    let header: Vec<String> = [
        "model_id",
        "status",
        "balanced",
        "score",
        "max_win_share",
        "ai_bankruptcy_rate",
        "fun_score",
        "rank",
        "evenness_rank",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let table = step.table.as_ref().unwrap();
    let rows: Vec<Row> = table.rows[1..]
        .iter()
        .map(|r| Row {
            id: r[0].parse().unwrap(),
            fields: vec![
                r[0].clone(),
                "ok".into(),
                r[1].clone(),
                r[3].clone(),
                "0.300000".into(),
                "0.000000".into(),
                r[2].clone(),
                String::new(),
                String::new(),
            ],
        })
        .collect();
    w.fun.ranked = Some((Columns::new(header), rows, Vec::new()));
}

#[when("I rank the models")]
fn rank_models(w: &mut HarnessWorld) {
    let (cols, rows, order) = w.fun.ranked.as_mut().unwrap();
    *order = rank(cols, rows);
}

fn ids(s: &str) -> Vec<u32> {
    s.split(',').map(|x| x.trim().parse().unwrap()).collect()
}

#[then(expr = "the ranking is {string}")]
fn ranking(w: &mut HarnessWorld, expected: String) {
    let (_, _, order) = w.fun.ranked.as_ref().unwrap();
    assert_eq!(*order, ids(&expected));
}

#[then(expr = "the evenness-only ranking is {string}")]
fn evenness_ranking(w: &mut HarnessWorld, expected: String) {
    let (cols, rows, _) = w.fun.ranked.as_ref().unwrap();
    let mut by: Vec<(u32, u32)> = rows
        .iter()
        .map(|r| (cols.get(r, "evenness_rank").parse().unwrap(), r.id))
        .collect();
    by.sort_unstable();
    let order: Vec<u32> = by.into_iter().map(|(_, id)| id).collect();
    assert_eq!(order, ids(&expected));
}

// ---- Running ---------------------------------------------------------------------------------

#[when(
    expr = "I play seed {int} of the roster {string} at rotation {int} on preset {string} measuring fun, and again plainly"
)]
fn play_both(w: &mut HarnessWorld, seed: u64, names: String, rotation: usize, preset: String) {
    let roster = Roster::new(
        names
            .split(',')
            .map(|n| Entry::from_name(n.trim()))
            .collect(),
    );
    let sources = w.config.sources.clone().unwrap_or_else(|| {
        sim_core::ConfigSources::read_dir(&super::repo_root().join("config")).unwrap()
    });
    let config = sim_cli::overrides::load(&sources, Some(&preset), &[]).unwrap();
    let plan = GamePlan {
        index: 0,
        seed,
        rotation: u32::try_from(rotation).unwrap(),
        seating: seats::seating(roster.len(), rotation),
    };
    let (measured, _) = play_one(&config, &roster, &plan);
    let seats_n = u8::try_from(roster.len()).unwrap();
    let world = World::seated(config, seed, seats_n, &roster.ai_seats(&plan.seating));
    let plain = play_from(
        world,
        &GameSpec {
            players: seats_n,
            ..GameSpec::default()
        },
    );
    w.fun.hashes = Some((measured.state_hash, plain.state_hash));
    w.fun.game = Some(measured.fun);
}

#[then("both games end with the same state hash")]
fn same_hash(w: &mut HarnessWorld) {
    let (a, b) = w.fun.hashes.unwrap();
    assert_eq!(format!("{a:016x}"), format!("{b:016x}"));
}

#[then(expr = "the measured game has fun data for {int} midnights and {int} seats")]
fn fun_data(w: &mut HarnessWorld, midnights: u32, seats_n: usize) {
    let g = measured(w);
    assert_eq!(g.midnights, midnights);
    assert_eq!(g.holdings.len(), seats_n);
    assert!(g.full_holdings);
    assert!(
        g.holdings.iter().any(|h| h.leases + h.businesses > 0),
        "someone holds something: {g:?}"
    );
}

fn csv(path: &std::path::Path) -> (Vec<String>, Vec<BTreeMap<String, String>>) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    let header = sim_cli::study::split_csv_line(lines.next().unwrap());
    let rows = lines
        .map(|l| {
            header
                .iter()
                .cloned()
                .zip(sim_cli::study::split_csv_line(l))
                .collect()
        })
        .collect();
    (header, rows)
}

#[then("every ok model has a fun score")]
fn every_fun_score(w: &mut HarnessWorld) {
    let (_, rows) = csv(&w.study.outs.last().unwrap().join("models.csv"));
    for r in rows.iter().filter(|r| r["status"] == "ok") {
        assert!(parse_ppm(&r["fun_score"]).is_some(), "{r:?}");
        assert!(!r["evenness_rank"].is_empty(), "{r:?}");
    }
}

fn by_id(rows: &[BTreeMap<String, String>]) -> BTreeMap<String, &BTreeMap<String, String>> {
    rows.iter().map(|r| (r["model_id"].clone(), r)).collect()
}

#[then(expr = "every fun column but {string}, and the ranks, of {string} equal those of {string}")]
fn fun_equal(w: &mut HarnessWorld, except: String, a: String, b: String) {
    let tmp = w.cli.tmp_dir();
    let (ha, ra) = csv(&tmp.join(a));
    let (_, rb) = csv(&tmp.join(b));
    let (ra, rb) = (by_id(&ra), by_id(&rb));
    assert_eq!(ra.keys().collect::<Vec<_>>(), rb.keys().collect::<Vec<_>>());
    let compared: Vec<&String> = ha
        .iter()
        .filter(|h| {
            (h.starts_with("fun_") || *h == "rank" || *h == "evenness_rank") && **h != except
        })
        .collect();
    assert_eq!(compared.len(), 18, "{compared:?}");
    for (id, r) in &ra {
        for c in &compared {
            assert_eq!(r[*c], rb[id][*c], "model {id} column {c}");
        }
    }
}

#[then(expr = "the raw fun columns of {string} equal those of {string} except {string}")]
fn raw_equal(w: &mut HarnessWorld, a: String, b: String, except: String) {
    let tmp = w.cli.tmp_dir();
    let (_, ra) = csv(&tmp.join(a));
    let (_, rb) = csv(&tmp.join(b));
    let (ra, rb) = (by_id(&ra), by_id(&rb));
    let raw = [
        "fun_strongest",
        "fun_upset_rate",
        "fun_lead_changes_mean",
        "fun_decided",
        "fun_comeback_rate",
        "fun_margin_median",
        "fun_distinctness",
        "fun_distinctness_checkpoint",
        "fun_streak_z",
        "fun_streak_longest",
    ];
    assert!(!ra.is_empty());
    for (id, r) in &ra {
        for c in raw.iter().filter(|c| **c != except) {
            assert_eq!(r[*c], rb[id][*c], "model {id} column {c}");
        }
        assert!(
            !r["fun_upset_rate"].is_empty(),
            "model {id} has no fun columns"
        );
    }
}

fn json(w: &mut HarnessWorld, path: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(w.cli.tmp_dir().join(path)).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[then(expr = "{string} says the playstyle was measured on {string}")]
fn playstyle_used(w: &mut HarnessWorld, path: String, basis: String) {
    assert_eq!(json(w, &path)["fun"]["playstyle_used"], basis);
}

#[then(expr = "{string} played seeds {string}")]
fn played_seeds(w: &mut HarnessWorld, path: String, seeds: String) {
    assert_eq!(json(w, &path)["study"]["seeds"], seeds);
}

#[then(
    expr = "{string} scores generations {int} and {int}, each with a fun score and an R65 verdict"
)]
fn lineup_fun(w: &mut HarnessWorld, path: String, a: u64, b: u64) {
    let doc = json(w, &path);
    let gens = doc["generations"].as_array().unwrap();
    let got: Vec<u64> = gens
        .iter()
        .map(|g| g["generation"].as_u64().unwrap())
        .collect();
    assert_eq!(got, vec![a, b]);
    for g in gens {
        assert!(g["fun_score"].is_number(), "{g}");
        assert!(g["r65"]["balanced"].is_boolean(), "{g}");
        assert!(g["fun"]["lead_changes_mean"].is_number(), "{g}");
    }
}
