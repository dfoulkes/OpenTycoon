//! Steps for `features/batch.feature` and `features/determinism.feature` (library level).

use cucumber::{given, then, when};
use sim_cli::batch::{BatchReport, BatchSpec, Timing, run_batch, summarise, write_csv};
use sim_cli::brains::BrainKind;
use sim_cli::game::GameSpec;
use sim_core::clock::TradingWindow;
use sim_core::economy::PlayerId;
use sim_core::rng::seeded;
use sim_core::world::Command;
use sim_core::{Money, World};

use crate::HarnessWorld;

/// Batches run in this scenario, and a hand-driven game for brain scenarios.
#[derive(Debug, Default)]
pub struct BatchState {
    /// Every batch run, in order.
    pub reports: Vec<BatchReport>,
    /// A game for brain scenarios.
    pub world: Option<World>,
    /// Commands a brain issued, with whether day trade was open at the time.
    pub issued: Vec<(Command, bool)>,
    /// CSV written for a hand-built game.
    pub csv: Option<String>,
}

impl BatchState {
    fn last(&self) -> &BatchReport {
        self.reports.last().expect("no batch run")
    }
}

fn csv(report: &BatchReport, timing: Timing) -> String {
    let mut out = Vec::new();
    write_csv(&mut out, &report.results, timing).unwrap();
    String::from_utf8(out).unwrap()
}

fn batch(w: &mut HarnessWorld, games: u64, seed: u64, brain: &str, threads: Option<usize>) {
    let config = w.config.load_ok();
    let spec = BatchSpec {
        games,
        base_seed: seed,
        threads,
        game: GameSpec {
            brain: brain.parse().unwrap(),
            ..GameSpec::default()
        },
    };
    w.batch.reports.push(run_batch(&config, &spec).unwrap());
}

#[when(expr = "I run a batch of {int} games from seed {int} with the {string} brain")]
fn run_a_batch(w: &mut HarnessWorld, games: u64, seed: u64, brain: String) {
    batch(w, games, seed, &brain, None);
}

#[when(
    expr = "I run a batch of {int} games from seed {int} with the {string} brain on {int} threads"
)]
fn run_a_batch_on(w: &mut HarnessWorld, games: u64, seed: u64, brain: String, threads: usize) {
    batch(w, games, seed, &brain, Some(threads));
    assert_eq!(w.batch.last().threads, threads);
}

#[then(expr = "the CSV has {int} games with seeds {int} to {int} in order")]
fn csv_seeds(w: &mut HarnessWorld, games: usize, first: u64, last: u64) {
    let text = csv(w.batch.last(), Timing::Include);
    let seeds: Vec<u64> = text
        .lines()
        .skip(1)
        .map(|l| l.split(',').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(seeds.len(), games);
    assert_eq!(seeds, (first..=last).collect::<Vec<_>>());
}

#[then(expr = "every game played {int} days")]
fn days_played(w: &mut HarnessWorld, days: u32) {
    for r in &w.batch.last().results {
        assert_eq!(r.days_played, days, "seed {}", r.seed);
    }
}

#[then(expr = "every row has a net worth and a bankrupt flag for {int} players")]
fn row_columns(w: &mut HarnessWorld, players: usize) {
    let text = csv(w.batch.last(), Timing::Include);
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    for p in 0..players {
        assert!(header.contains(&format!("p{p}_net_worth_pence").as_str()));
        assert!(header.contains(&format!("p{p}_bankrupt").as_str()));
    }
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        assert_eq!(cells.len(), header.len(), "{line}");
        for p in 0..players {
            let at = |name: String| header.iter().position(|h| *h == name).unwrap();
            cells[at(format!("p{p}_net_worth_pence"))]
                .parse::<i64>()
                .unwrap();
            cells[at(format!("p{p}_bankrupt"))].parse::<bool>().unwrap();
        }
    }
}

#[then(expr = "the summary has p10, median and p90 net worth for each of {int} seats")]
fn summary_spread(w: &mut HarnessWorld, seats: usize) {
    let s = summarise(w.batch.last());
    assert_eq!(s.seats.len(), seats);
    for seat in &s.seats {
        let d = seat.net_worth_pence.expect("no net worth spread");
        assert!(d.p10 <= d.median && d.median <= d.p90);
    }
    let text = s.to_string();
    assert!(text.contains("p10") && text.contains("median") && text.contains("p90"));
}

#[then("the summary's bankruptcy rate is between 0 and 1")]
fn summary_bankruptcy(w: &mut HarnessWorld) {
    let s = summarise(w.batch.last());
    assert!((0.0..=1.0).contains(&s.bankruptcy_rate));
    assert!(s.to_string().contains("bankruptcy rate"));
}

#[then("the summary reports games per second")]
fn summary_throughput(w: &mut HarnessWorld) {
    let s = summarise(w.batch.last());
    assert!(s.games_per_second > 0.0);
    assert!(s.to_string().contains("games/sec="));
}

#[then("the two CSVs are identical without the wall time column")]
fn two_csvs_identical(w: &mut HarnessWorld) {
    let [a, b] = &w.batch.reports[..] else {
        panic!("expected two batches")
    };
    assert_eq!(csv(a, Timing::Omit), csv(b, Timing::Omit));
}

#[then("each game's state hash equals a plain run of that seed for the same ticks")]
fn hash_matches_run(w: &mut HarnessWorld) {
    let config = w.config.load_ok();
    for r in &w.batch.last().results {
        let mut world = World::new(config.clone(), r.seed, 2);
        for _ in 0..r.ticks {
            world.tick();
        }
        assert_eq!(r.state_hash, world.state_hash(), "seed {}", r.seed);
    }
}

#[given(expr = "a new game of the {string} preset")]
fn a_new_game(w: &mut HarnessWorld, preset: String) {
    let dir = super::repo_root().join("config");
    w.config.sources = Some(sim_core::ConfigSources::read_dir(&dir).unwrap());
    w.config.preset = Some(preset);
    w.batch.world = Some(World::new(w.config.load_ok(), 7, 2));
}

#[given(expr = "a new game of the {string} preset with no AI opponents")]
fn a_new_game_without_ai(w: &mut HarnessWorld, preset: String) {
    let dir = super::repo_root().join("config");
    let mut sources = sim_core::ConfigSources::read_dir(&dir).unwrap();
    sources.insert(
        "ai.yaml",
        without_opponents(sources.get("ai.yaml").unwrap()),
    );
    w.config.sources = Some(sources);
    w.config.preset = Some(preset);
    w.batch.world = Some(World::new(w.config.load_ok(), 7, 2));
}

/// `ai.yaml` with `opponents: []`, so every seat is free for a harness brain.
fn without_opponents(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.starts_with("opponents:") {
                "opponents: []"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[when(expr = "the {string} brain plays seat {int} for one day")]
fn brain_plays(w: &mut HarnessWorld, brain: String, seat: u8) {
    let kind: BrainKind = brain.parse().unwrap();
    let mut brain = kind.build();
    let mut rng = seeded(1);
    let world = w.batch.world.as_mut().unwrap();
    let me = PlayerId(seat);
    for _ in 0..world.clock().rules().ticks_per_day() {
        let day_open = world.clock().is_open(TradingWindow::Day);
        for command in brain.decide(world, me, &mut rng) {
            w.batch.issued.push((command.clone(), day_open));
            world.submit(me, command);
        }
        world.tick();
    }
}

#[then(expr = "it issued {int} commands")]
fn issued(w: &mut HarnessWorld, n: usize) {
    assert_eq!(w.batch.issued.len(), n, "{:?}", w.batch.issued);
}

#[then(expr = "its first command opens an auction on {string} at £{int}")]
fn first_command(w: &mut HarnessWorld, name: String, pounds: i64) {
    let world = w.batch.world.as_ref().unwrap();
    match &w.batch.issued[0].0 {
        Command::OpenLandAuction { block, bid } => {
            assert_eq!(world.map().block(*block).unwrap().name, name);
            assert_eq!(*bid, Money::from_pounds(pounds));
        }
        other => panic!("expected an opening bid, got {other:?}"),
    }
}

#[then(expr = "its second command builds a {string} on {string}")]
fn second_command(w: &mut HarnessWorld, type_id: String, name: String) {
    let world = w.batch.world.as_ref().unwrap();
    match &w.batch.issued[1].0 {
        Command::Build {
            block, type_id: t, ..
        } => {
            assert_eq!(world.map().block(*block).unwrap().name, name);
            assert_eq!(*t, type_id);
        }
        other => panic!("expected a build, got {other:?}"),
    }
}

fn player_shops(
    world: &World,
    player: u8,
) -> Vec<(
    sim_core::businesses::BusinessId,
    &sim_core::businesses::Business,
)> {
    world
        .businesses()
        .owned_by(sim_core::economy::AccountId::Player(PlayerId(player)))
        .collect()
}

#[then(expr = "player {int} owns {int} shop on {string}, touching the lot edge")]
fn owns_one_shop(w: &mut HarnessWorld, player: u8, n: usize, name: String) {
    let world = w.batch.world.as_ref().unwrap();
    let shops = player_shops(world, player);
    assert_eq!(shops.len(), n);
    let [lw, ld] = world.config().game.world.lot_size;
    for (_, b) in shops {
        assert_eq!(world.map().block(b.block).unwrap().name, name);
        assert!(b.footprint.touches_edge(lw, ld));
        assert!(b.height <= world.config().businesses.types[&b.type_id].max_height);
    }
}

#[then(expr = "player {int}'s shop has a standing order for its full capacity")]
fn full_order(w: &mut HarnessWorld, player: u8) {
    let world = w.batch.world.as_ref().unwrap();
    let (_, shop) = player_shops(world, player)[0];
    assert!(shop.capacity_per_good > 0);
    for units in shop.standing_order.values() {
        assert_eq!(*units, shop.capacity_per_good);
    }
}

#[then("it bid during day trade")]
fn bid_in_day_trade(w: &mut HarnessWorld) {
    assert!(w.batch.issued[0].1);
}

#[given(expr = "player {int} holds the lease on {string}, bought in {int} for £{int}")]
fn holds_lease(w: &mut HarnessWorld, player: u8, name: String, year: i32, pounds: i64) {
    let world = w.batch.world.as_mut().unwrap();
    let term_years = world.config().game.leases.term_years;
    let block = world
        .map_mut()
        .blocks
        .iter_mut()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no block {name}"));
    block.lease = Some(sim_core::land::Lease {
        holder: sim_core::economy::AccountId::Player(PlayerId(player)),
        start_year: year,
        term_years,
        last_price: Money::from_pounds(pounds),
    });
}

#[when("the harness plays it to the whistle and writes the CSV")]
fn play_to_whistle(w: &mut HarnessWorld) {
    let world = w.batch.world.take().unwrap();
    let result = sim_cli::game::play_from(world, &GameSpec::default());
    let mut out = Vec::new();
    write_csv(&mut out, &[result], Timing::Omit).unwrap();
    w.batch.csv = Some(String::from_utf8(out).unwrap());
}

fn csv_cell(w: &HarnessWorld, column: &str) -> String {
    let text = w.batch.csv.as_ref().expect("no CSV");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    let row: Vec<&str> = lines.next().unwrap().split(',').collect();
    let at = header.iter().position(|h| *h == column).unwrap();
    row[at].to_owned()
}

#[then(expr = "player {int}'s net worth in the CSV is higher than player {int}'s")]
fn richer(w: &mut HarnessWorld, a: u8, b: u8) {
    let worth = |p: u8| -> i64 {
        csv_cell(w, &format!("p{p}_net_worth_pence"))
            .parse()
            .unwrap()
    };
    assert!(
        worth(a) > worth(b),
        "p{a} {} vs p{b} {}\n{}",
        worth(a),
        worth(b),
        w.batch.csv.as_ref().unwrap()
    );
}

#[then(expr = "the CSV names player {int} the winner")]
fn csv_winner(w: &mut HarnessWorld, p: u8) {
    assert_eq!(csv_cell(w, "winner"), format!("p{p}"));
}
