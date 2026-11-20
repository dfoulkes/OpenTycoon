//! Thread-count independence of `sim-cli batch` (docs/engineering.md §3): the same batch on 1
//! thread and on many threads must produce byte-identical CSVs (wall time aside), because each
//! game depends only on the config, its seed and the brains.
#![allow(clippy::unwrap_used, missing_docs, reason = "tests")]

use sim_cli::batch::{BatchSpec, Timing, run_batch, write_csv};
use sim_cli::brains::BrainKind;
use sim_cli::game::GameSpec;
use sim_cli::overrides;
use sim_core::ConfigSources;

fn csv(threads: usize, brain: BrainKind) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
    let sources = ConfigSources::read_dir(&dir).unwrap();
    let config = overrides::load(&sources, Some("v1"), &[]).unwrap();
    let spec = BatchSpec {
        games: 32,
        base_seed: 1_000,
        threads: Some(threads),
        game: GameSpec {
            brain,
            ..GameSpec::default()
        },
    };
    let report = run_batch(&config, &spec).unwrap();
    assert_eq!(report.threads, threads);
    let mut out = Vec::new();
    write_csv(&mut out, &report.results, Timing::Omit).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn one_thread_and_eight_threads_give_identical_csvs() {
    for brain in BrainKind::ALL {
        let one = csv(1, brain);
        assert_eq!(one.lines().count(), 33);
        assert_eq!(one, csv(8, brain), "brain {brain}");
    }
}
