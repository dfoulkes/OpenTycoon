//! Study outputs: `models.csv`, `games.csv`, `summary.json` and the overlays of the best
//! balanced models. Column names and meanings are a contract documented in docs/study.md;
//! the charts are drawn from these files, so add columns at the end of their group and never
//! rename one.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Value as Json, json};
use sim_core::ConfigSources;
use sim_core::config::ConfigFile;

use super::def::{StudyDef, StudyError};
use super::fun::{FunDef, FunMetrics, Playstyle, fmt_ppm, fun_score_ppm, parse_ppm};
use super::metrics::{GameOutcome, ModelMetrics};
use super::models::{ModelSpec, model_sources};
use super::seats::Roster;

/// Splits one CSV line into fields (RFC 4180 quoting: `"a,b"`, `""` for a quote).
pub fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => fields.push(std::mem::take(&mut field)),
            (_, c) => field.push(c),
        }
    }
    fields.push(field);
    fields
}

/// One CSV field, quoted if it holds a comma or a quote; newlines become `; `.
pub fn csv_field(s: &str) -> String {
    let s = s.replace("\r\n", "; ").replace(['\n', '\r'], "; ");
    if s.contains([',', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn f6(v: f64) -> String {
    format!("{v:.6}")
}

fn opt6(v: Option<f64>) -> String {
    v.map(f6).unwrap_or_default()
}

/// The fixed metric columns of `models.csv`, after `model_id, status, error` and the
/// `param_<name>` columns.
pub const MODEL_METRICS: [&str; 15] = [
    "games",
    "no_winner",
    "max_win_share",
    "min_win_share",
    "evenness_entropy",
    "evenness_gini",
    "bankruptcy_rate",
    "ai_bankruptcy_rate",
    "idle_mean_rank",
    "idle_last_rate",
    "balanced",
    "score",
    "rank",
    "games_per_sec",
    "idle_below_solvent_rate",
];

/// Per roster entry `<name>_<suffix>` columns of `models.csv`.
pub const ENTRY_METRICS: [&str; 8] = [
    "wins",
    "win_share",
    "win_low",
    "win_high",
    "bankrupt_rate",
    "nw_p10",
    "nw_median",
    "nw_p90",
];

/// The fun columns (R79) at the end of `models.csv`: the raw metrics, written when the model
/// finishes, then each metric's band score, the fun score and the evenness-only rank, filled
/// when the file is finalised from the raw metrics and the definition's `fun:` bands (so
/// retuned bands re-rank without a game).
pub const FUN_COLUMNS: [&str; 18] = [
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
    "fun_upset_score",
    "fun_lead_changes_score",
    "fun_comeback_score",
    "fun_margin_score",
    "fun_distinctness_score",
    "fun_streak_score",
    "fun_score",
    "evenness_rank",
];

/// The raw fun metric columns, in [`FunDef::bands`] order, and their score columns.
const FUN_SCORED: [(&str, &str); 6] = [
    ("fun_upset_rate", "fun_upset_score"),
    ("fun_lead_changes_mean", "fun_lead_changes_score"),
    ("fun_comeback_rate", "fun_comeback_score"),
    ("fun_margin_median", "fun_margin_score"),
    ("fun_distinctness", "fun_distinctness_score"),
    ("fun_streak_z", "fun_streak_score"),
];

/// The header of `models.csv`. `games_per_sec` is left out without wall time, so two runs
/// diff byte for byte.
pub fn models_header(def: &StudyDef, roster: &Roster, wall: bool) -> Vec<String> {
    let mut h: Vec<String> = ["model_id", "status", "error"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    h.extend(def.parameters.iter().map(|p| format!("param_{}", p.name)));
    h.extend(
        MODEL_METRICS
            .iter()
            .filter(|m| wall || **m != "games_per_sec")
            .map(|s| (*s).to_owned()),
    );
    for name in roster.names() {
        h.extend(ENTRY_METRICS.iter().map(|m| format!("{name}_{m}")));
    }
    h.extend(FUN_COLUMNS.iter().map(|s| (*s).to_owned()));
    h
}

/// The raw fun columns of a model (the score columns and `evenness_rank` empty).
pub fn fun_fields(roster: &Roster, f: &FunMetrics) -> Vec<String> {
    let opt = |v: Option<i64>| v.map(fmt_ppm).unwrap_or_default();
    let mut row = vec![
        f.strongest
            .and_then(|i| roster.entries().get(i))
            .map(|e| e.name().to_owned())
            .unwrap_or_default(),
        fmt_ppm(f.upset_ppm),
        fmt_ppm(f.lead_changes_ppm),
        f.decided.to_string(),
        opt(f.comeback_ppm),
        opt(f.margin_median_ppm),
        opt(f.distinctness_ppm),
        opt(f.distinctness_checkpoint_ppm),
        opt(f.streak_z_ppm),
        f.streak_longest.to_string(),
    ];
    row.extend(std::iter::repeat_n(
        String::new(),
        FUN_COLUMNS.len() - row.len(),
    ));
    row
}

/// Fills every ok row's band scores and fun score from its raw fun metrics and `fun` (R79).
/// Distinctness is read from `fun_distinctness` or `fun_distinctness_checkpoint`, as
/// `fun.playstyle` says. A row with no raw metrics (written before the fun columns existed)
/// keeps them empty.
pub fn apply_fun_scores(cols: &Columns, rows: &mut [Row], fun: &FunDef) {
    let Some(score_col) = cols.index("fun_score") else {
        return;
    };
    for r in rows.iter_mut() {
        if !cols.is_ok(r) || cols.get(r, "fun_upset_rate").is_empty() {
            continue;
        }
        let mut scores = Vec::new();
        for ((name, band), (raw, out)) in fun.bands().iter().zip(FUN_SCORED) {
            let raw = if *name == "distinctness" && fun.playstyle == Playstyle::Checkpoint {
                "fun_distinctness_checkpoint"
            } else {
                raw
            };
            let s = band.score_ppm(parse_ppm(cols.get(r, raw)));
            if let Some(i) = cols.index(out) {
                r.fields[i] = fmt_ppm(s);
            }
            scores.push((s, band.weight));
        }
        r.fields[score_col] = fmt_ppm(fun_score_ppm(&scores));
    }
}

/// A finished model's row: what it is and, if it ran, its metrics.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Ran: its metrics, its fun metrics (R79) and its games per second (wall time).
    Ok(ModelMetrics, Box<FunMetrics>, f64),
    /// Its config failed validation; the error names the key.
    Invalid(String),
}

/// One `models.csv` row, in header order.
pub fn model_row(
    def: &StudyDef,
    roster: &Roster,
    model: &ModelSpec,
    baseline: &[Option<String>],
    outcome: &Outcome,
    wall: bool,
) -> Vec<String> {
    let mut row = vec![model.id.to_string()];
    match outcome {
        Outcome::Ok(..) => row.extend(["ok".to_owned(), String::new()]),
        Outcome::Invalid(e) => row.extend(["invalid".to_owned(), csv_field(e)]),
    }
    for (i, p) in def.parameters.iter().enumerate() {
        let v = match &model.values {
            Some(v) => Some(p.label(v[i])),
            None => baseline.get(i).cloned().flatten(),
        };
        row.push(v.unwrap_or_default());
    }
    let metric_cols = MODEL_METRICS.len() - usize::from(!wall);
    let entry_cols = ENTRY_METRICS.len() * roster.len();
    match outcome {
        Outcome::Invalid(_) => {
            row.extend(std::iter::repeat_n(
                String::new(),
                metric_cols + entry_cols + FUN_COLUMNS.len(),
            ));
        }
        Outcome::Ok(m, fun, gps) => {
            row.extend([
                m.games.to_string(),
                m.no_winner.to_string(),
                f6(m.max_win_share),
                f6(m.min_win_share),
                f6(m.evenness_entropy),
                f6(m.evenness_gini),
                f6(m.bankruptcy_rate),
                f6(m.ai_bankruptcy_rate),
                opt6(m.idle_mean_rank),
                opt6(m.idle_last_rate),
                m.balanced.to_string(),
                f6(m.score),
                String::new(), // rank: filled when the file is finalised
            ]);
            if wall {
                row.push(format!("{gps:.1}"));
            }
            row.push(opt6(m.idle_below_solvent_rate));
            for e in &m.entries {
                let nw = |f: fn(&crate::stats::Distribution) -> i64| {
                    e.net_worth
                        .as_ref()
                        .map(|d| f(d).to_string())
                        .unwrap_or_default()
                };
                row.extend([
                    e.wins.to_string(),
                    f6(e.win_share),
                    f6(e.win_low),
                    f6(e.win_high),
                    f6(e.bankrupt_rate),
                    nw(|d| d.p10),
                    nw(|d| d.median),
                    nw(|d| d.p90),
                ]);
            }
            row.extend(fun_fields(roster, fun));
        }
    }
    row
}

/// The header of `games.csv` for `seats` seats.
pub fn games_header(seats: usize, wall: bool) -> Vec<String> {
    let mut h: Vec<String> = [
        "model_id",
        "game",
        "seed",
        "rotation",
        "winner",
        "days",
        "ticks",
        "state_hash",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for s in 0..seats {
        h.extend([
            format!("seat{s}_entry"),
            format!("seat{s}_net_worth_pence"),
            format!("seat{s}_bankrupt"),
        ]);
    }
    if wall {
        h.push("wall_ms".to_owned());
    }
    h.extend(
        [
            "lead_changes",
            "halfway_day",
            "halfway_leader",
            "comeback",
            "margin",
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    for s in 0..seats {
        h.extend(GAME_HOLDINGS.iter().map(|m| format!("seat{s}_{m}")));
    }
    h
}

/// Per seat holdings columns of `games.csv` (R79), after the per-game fun columns.
pub const GAME_HOLDINGS: [&str; 7] = [
    "flats",
    "shops",
    "land",
    "hotels",
    "other",
    "leases",
    "businesses",
];

/// One `games.csv` row.
pub fn game_row(model: u32, roster: &Roster, g: &GameOutcome, wall_ms: Option<f64>) -> Vec<String> {
    let names = roster.names();
    let winner = match (g.winner, g.tie) {
        (Some(w), _) => names[w].clone(),
        (None, true) => "tie".to_owned(),
        (None, false) => "none".to_owned(),
    };
    let mut row = vec![
        model.to_string(),
        g.index.to_string(),
        g.seed.to_string(),
        g.rotation.to_string(),
        winner,
        g.days.to_string(),
        g.ticks.to_string(),
        format!("{:016x}", g.state_hash),
    ];
    for (seat, entry) in g.seating.iter().enumerate() {
        row.extend([
            names[*entry].clone(),
            g.net_worth[seat].to_string(),
            g.bankrupt[seat].to_string(),
        ]);
    }
    if let Some(ms) = wall_ms {
        row.push(format!("{ms:.3}"));
    }
    let f = &g.fun;
    let leaders: Vec<String> = f
        .halfway_leaders
        .iter()
        .filter_map(|s| g.seating.get(usize::from(*s)).map(|e| names[*e].clone()))
        .collect();
    row.extend([
        f.lead_changes.to_string(),
        f.halfway_day.to_string(),
        leaders.join("+"),
        f.comeback.map(|c| c.to_string()).unwrap_or_default(),
        f.margin_ppm.map(fmt_ppm).unwrap_or_default(),
    ]);
    for seat in 0..g.seating.len() {
        match f.holdings.get(seat) {
            Some(h) => {
                let full = |v: u32| {
                    if f.full_holdings {
                        v.to_string()
                    } else {
                        String::new()
                    }
                };
                row.extend([
                    full(h.flats),
                    full(h.shops),
                    full(h.land),
                    full(h.hotels),
                    full(h.other),
                    h.leases.to_string(),
                    h.businesses.to_string(),
                ]);
            }
            None => row.extend(std::iter::repeat_n(String::new(), GAME_HOLDINGS.len())),
        }
    }
    row
}

/// A `models.csv` row read back: its id and fields in header order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Model id.
    pub id: u32,
    /// Fields.
    pub fields: Vec<String>,
}

/// Column lookups on `models.csv` rows.
#[derive(Debug, Clone)]
pub struct Columns {
    header: Vec<String>,
}

impl Columns {
    /// For this header.
    pub fn new(header: Vec<String>) -> Self {
        Columns { header }
    }

    /// The header.
    pub fn header(&self) -> &[String] {
        &self.header
    }

    /// Index of a column.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }

    /// A field of a row.
    pub fn get<'a>(&self, row: &'a Row, name: &str) -> &'a str {
        self.index(name)
            .and_then(|i| row.fields.get(i))
            .map_or("", String::as_str)
    }

    fn num(&self, row: &Row, name: &str) -> f64 {
        self.get(row, name).parse().unwrap_or(f64::NAN)
    }

    /// True if the row ran (status `ok`).
    pub fn is_ok(&self, row: &Row) -> bool {
        self.get(row, "status") == "ok"
    }

    /// True if the row is balanced.
    pub fn is_balanced(&self, row: &Row) -> bool {
        self.get(row, "balanced") == "true"
    }

    /// The row as a JSON object (numbers where they parse).
    pub fn json(&self, row: &Row) -> Json {
        let mut obj = serde_json::Map::new();
        for (h, v) in self.header.iter().zip(&row.fields) {
            let value = if v.is_empty() {
                Json::Null
            } else if v == "true" || v == "false" {
                Json::Bool(v == "true")
            } else if let Ok(i) = v.parse::<i64>() {
                json!(i)
            } else if let Some(f) = v.parse::<f64>().ok().filter(|f| f.is_finite()) {
                json!(f)
            } else {
                Json::String(v.clone())
            };
            obj.insert(h.clone(), value);
        }
        Json::Object(obj)
    }
}

/// Reads `models.csv` rows. Lines with the wrong number of fields (a run cut off mid-write)
/// and repeated ids are dropped. `Err(header)` if the header differs from `expected`.
pub fn read_models(text: &str, expected: &[String]) -> Result<Vec<Row>, Vec<String>> {
    let mut lines = text.lines();
    let Some(head) = lines.next() else {
        return Ok(Vec::new());
    };
    let header = split_csv_line(head);
    if header != expected {
        return Err(header);
    }
    let mut rows: Vec<Row> = Vec::new();
    for line in lines {
        let fields = split_csv_line(line);
        if fields.len() != expected.len() {
            continue;
        }
        let Ok(id) = fields[0].parse::<u32>() else {
            continue;
        };
        if rows.iter().any(|r| r.id == id) {
            continue;
        }
        rows.push(Row { id, fields });
    }
    Ok(rows)
}

/// Sorts rows into the ranking (R65, R79): models that pass R65 (`balanced`) first, then the
/// higher fun score (an exact fixed-point comparison; a row without one counts as 0), then the
/// evenness tie-breaks: higher score, lower max win share, lower AI bankruptcy rate, lower id.
/// Invalid models are not ranked. Returns the ids in rank order (ok models only), fills each ok
/// row's `rank`, and fills `evenness_rank` with the ranking R65 alone gives (no fun score).
pub fn rank(cols: &Columns, rows: &mut [Row]) -> Vec<u32> {
    let fun = |r: &Row| parse_ppm(cols.get(r, "fun_score")).unwrap_or(0);
    let evenness = |a: &Row, b: &Row| {
        (-cols.num(a, "score"))
            .total_cmp(&-cols.num(b, "score"))
            .then(
                cols.num(a, "max_win_share")
                    .total_cmp(&cols.num(b, "max_win_share")),
            )
            .then(
                cols.num(a, "ai_bankruptcy_rate")
                    .total_cmp(&cols.num(b, "ai_bankruptcy_rate")),
            )
            .then(a.id.cmp(&b.id))
    };
    let ok: Vec<&Row> = rows.iter().filter(|r| cols.is_ok(r)).collect();
    let mut by_fun = ok.clone();
    by_fun.sort_by(|a, b| {
        (!cols.is_balanced(a))
            .cmp(&!cols.is_balanced(b))
            .then(fun(b).cmp(&fun(a)))
            .then(evenness(a, b))
    });
    let mut by_evenness = ok;
    by_evenness.sort_by(|a, b| {
        (!cols.is_balanced(a))
            .cmp(&!cols.is_balanced(b))
            .then(evenness(a, b))
    });
    let order: Vec<u32> = by_fun.iter().map(|r| r.id).collect();
    let even: Vec<u32> = by_evenness.iter().map(|r| r.id).collect();
    for (col, ids) in [("rank", &order), ("evenness_rank", &even)] {
        if let Some(ci) = cols.index(col) {
            for r in rows.iter_mut() {
                if let Some(pos) = ids.iter().position(|id| *id == r.id) {
                    r.fields[ci] = (pos + 1).to_string();
                }
            }
        }
    }
    order
}

/// `summary.json`'s `ranking`: every ok model in rank order.
pub fn ranking_json(cols: &Columns, rows: &[Row], order: &[u32]) -> Vec<Json> {
    order
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = rows.iter().find(|r| r.id == *id)?;
            let num = |c: &str| cols.json(r)[c].clone();
            Some(json!({
                "rank": i + 1,
                "model_id": id,
                "balanced": cols.is_balanced(r),
                "score": num("score"),
                "max_win_share": num("max_win_share"),
                "ai_bankruptcy_rate": num("ai_bankruptcy_rate"),
                "idle_last_rate": num("idle_last_rate"),
                "idle_below_solvent_rate": num("idle_below_solvent_rate"),
                "fun_score": num("fun_score"),
                "evenness_rank": num("evenness_rank"),
            }))
        })
        .collect()
}

/// A CSV file's text.
pub fn csv_text(header: &[String], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut s = header.join(",");
    s.push('\n');
    for r in rows {
        s.push_str(&r.join(","));
        s.push('\n');
    }
    s
}

fn write(path: &Path, text: &str) -> Result<(), StudyError> {
    std::fs::write(path, text).map_err(|e| StudyError::io(path, e))
}

/// Writes the overlay of each model in `ids` under `dir/models/<id>/`: the model's whole
/// `economy.yaml` (R71: adopting the model is copying that one file over `config/`), any other
/// config file the model changes (only a study that also varies `ai.yaml`), `overrides.txt`
/// (the same change as `--set` lines) and `model.json` (its parameters and metrics). Replaces
/// any older overlays.
pub fn write_overlays(
    dir: &Path,
    def: &StudyDef,
    base: &ConfigSources,
    models: &[ModelSpec],
    ids: &[u32],
    cols: &Columns,
    rows: &[Row],
) -> Result<(), StudyError> {
    let root = dir.join("models");
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| StudyError::io(&root, e))?;
    }
    for id in ids {
        let Some(model) = models.iter().find(|m| m.id == *id) else {
            continue;
        };
        let out = root.join(id.to_string());
        std::fs::create_dir_all(&out).map_err(|e| StudyError::io(&out, e))?;
        let (edited, ovs) =
            model_sources(def, base, model).map_err(|e| StudyError::BaseConfig(e.to_string()))?;
        for file in ConfigFile::ALL {
            let name = file.file_name();
            let (Some(old), Some(new)) = (base.get(name), edited.get(name)) else {
                continue;
            };
            if old != new || file == ConfigFile::Economy {
                let schema = file.schema_file_name();
                let text = format!(
                    "# yaml-language-server: $schema=schema/{schema}\n# sim-cli study model {id}: \
                     {name} with this model's values (see overrides.txt). Copy over config/ to \
                     adopt.\n{new}"
                );
                write(&out.join(name), &text)?;
            }
        }
        let mut txt = format!(
            "# sim-cli study model {id}: the same change as --set overrides on the base config.\n"
        );
        if ovs.is_empty() {
            txt.push_str("# (none: model 0 is the base config)\n");
        }
        for ov in &ovs {
            let _ = writeln!(txt, "{ov}");
        }
        write(&out.join("overrides.txt"), &txt)?;
        let row = rows.iter().find(|r| r.id == *id).map(|r| cols.json(r));
        let doc = json!({ "model_id": id, "overrides": ovs.iter().map(ToString::to_string).collect::<Vec<_>>(), "row": row });
        write(
            &out.join("model.json"),
            &(serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_fields_round_trip() {
        for s in ["plain", "a,b", "say \"hi\"", ""] {
            let line = format!("1,{},x", csv_field(s));
            assert_eq!(split_csv_line(&line), vec!["1", s, "x"]);
        }
        assert_eq!(csv_field("a\nb"), "a; b");
    }

    #[test]
    fn a_cut_off_line_and_a_repeat_are_dropped() {
        let header: Vec<String> = ["model_id", "status"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let rows = read_models("model_id,status\n0,ok\n0,ok\n1", &header).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(read_models("model_id,other\n", &header).is_err());
    }
}
