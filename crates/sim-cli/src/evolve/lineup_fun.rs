//! `lineup_fun.json`: each generation's champion line-up scored by the study's fun metrics
//! (R79) over its lineup games, so the shipped line-up can be chosen by fun while the strongest
//! evolved settings become a harder difficulty tier. Output only: it reads the lineup games
//! the generation already played and never touches fitness, selection or breeding.
//!
//! One entry per finished generation, in order. It is written just before the generation's
//! `lineup.json` (which marks the generation finished), dropping any entry of that generation
//! or later, so a resumed run ends with the same file as a run in one go. Scored with the
//! default bands (docs/study.md "Fun (R79)"); the raw metrics are kept so other bands can be
//! applied afterwards.

use std::path::Path;

use serde_json::{Value as Json, json};

use super::def::EvolveError;
use crate::study::def::Criteria;
use crate::study::fun::{FunDef, FunMetrics, Playstyle, fmt_ppm, fun_metrics, fun_score_ppm};
use crate::study::metrics::{GameOutcome, Tally, metrics};
use crate::study::seats::Roster;

/// The file name, in the evolution's output directory.
pub const FILE: &str = "lineup_fun.json";

fn num(v: i64) -> Json {
    serde_json::from_str(&fmt_ppm(v)).unwrap_or(Json::Null)
}

fn opt(v: Option<i64>) -> Json {
    v.map_or(Json::Null, num)
}

/// One generation's entry: the lineup games' R65 verdict, fun metrics, band scores and fun
/// score.
pub fn entry(roster: &Roster, generation: u32, lineup: &[GameOutcome]) -> Json {
    let mut tally = Tally::new(roster.len());
    for g in lineup {
        tally.add(roster, g);
    }
    let m = metrics(roster, &tally, &Criteria::default());
    let f: FunMetrics = fun_metrics(roster, lineup);
    let def = FunDef::default();
    let raw = [
        f.upset_ppm.into(),
        f.lead_changes_ppm.into(),
        f.comeback_ppm,
        f.margin_median_ppm,
        match def.playstyle {
            Playstyle::Holdings => f.distinctness_ppm,
            Playstyle::Checkpoint => f.distinctness_checkpoint_ppm,
        },
        f.streak_z_ppm,
    ];
    let mut scores = serde_json::Map::new();
    let mut weighted = Vec::new();
    for ((name, band), value) in def.bands().iter().zip(raw) {
        let s = band.score_ppm(value);
        scores.insert((*name).to_owned(), num(s));
        weighted.push((s, band.weight));
    }
    json!({
        "generation": generation,
        "games": lineup.len(),
        "r65": {
            "balanced": m.balanced,
            "max_win_share": m.max_win_share,
            "min_win_share": m.min_win_share,
            "ai_bankruptcy_rate": m.ai_bankruptcy_rate,
            "idle_below_solvent_rate": m.idle_below_solvent_rate,
        },
        "fun": {
            "strongest": f.strongest.and_then(|i| roster.entries().get(i)).map(|e| e.name().to_owned()),
            "upset_rate": num(f.upset_ppm),
            "lead_changes_mean": num(f.lead_changes_ppm),
            "decided": f.decided,
            "comeback_rate": opt(f.comeback_ppm),
            "margin_median": opt(f.margin_median_ppm),
            "distinctness": opt(f.distinctness_ppm),
            "distinctness_checkpoint": opt(f.distinctness_checkpoint_ppm),
            "streak_z": opt(f.streak_z_ppm),
            "streak_longest": f.streak_longest,
        },
        "scores": scores,
        "fun_score": num(fun_score_ppm(&weighted)),
    })
}

/// Adds generation `generation`'s entry to `dir/lineup_fun.json`, replacing that generation
/// and any later one.
pub fn write(
    dir: &Path,
    roster: &Roster,
    generation: u32,
    lineup: &[GameOutcome],
) -> Result<(), EvolveError> {
    // red: not yet implemented
    let _ = (dir, roster, generation, lineup);
    Ok(())
}
