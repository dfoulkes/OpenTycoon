//! Spec traceability: which rulings (R1–R61, `docs/spec.md` §12) have a tagged Gherkin scenario.
//!
//! - Every ruling is in scope unless listed in `docs/traceability-out-of-scope.txt`.
//! - An in-scope ruling with no `@Rn` tag must be listed in `docs/traceability-pending.txt`.
//!
//! Always prints a report (`cargo trace`). Fails only on things that are definitely wrong:
//! a tag citing a ruling that doesn't exist, an in-scope ruling that is neither tagged nor
//! pending, or an id listed in both files. A pending entry that is now tagged (and not marked
//! `partial:`) is reported as stale; set `TRACEABILITY_STRICT=1` to make that fail too.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs,
    reason = "tests"
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Ruling ids from the "Ruled" table in §12 of the spec.
fn ruled_ids(spec: &str) -> BTreeSet<u32> {
    let section = spec
        .split("## 12. Decisions")
        .nth(1)
        .expect("spec has a §12 Decisions section");
    let ruled = section.split("### Open").next().unwrap_or(section);
    ruled
        .lines()
        .filter_map(|line| {
            let cell = line.trim().strip_prefix('|')?.split('|').next()?.trim();
            cell.strip_prefix('R')?.parse().ok()
        })
        .collect()
}

/// `R-id -> note` from an allowlist file (`R12  # reason`).
fn listed_ids(path: &Path) -> BTreeMap<u32, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (id, note) = line.split_once('#').unwrap_or((line, ""));
            let id = id.trim().strip_prefix('R')?.parse().ok()?;
            Some((id, note.trim().to_owned()))
        })
        .collect()
}

/// `R-id -> feature files citing it`, from `@Rn` tags on tag lines.
fn tagged_ids(dir: &Path, out: &mut BTreeMap<u32, BTreeSet<String>>) {
    for entry in std::fs::read_dir(dir).expect("features dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            tagged_ids(&path, out);
        } else if path.extension().is_some_and(|e| e == "feature") {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            for line in text.lines().map(str::trim).filter(|l| l.starts_with('@')) {
                for tag in line.split_whitespace() {
                    if let Some(id) = tag.strip_prefix("@R").and_then(|n| n.parse().ok()) {
                        out.entry(id).or_default().insert(name.clone());
                    }
                }
            }
        }
    }
}

fn fmt_ids<'a>(ids: impl IntoIterator<Item = &'a u32>) -> String {
    let v: Vec<String> = ids.into_iter().map(|i| format!("R{i}")).collect();
    if v.is_empty() {
        "(none)".into()
    } else {
        v.join(" ")
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one linear report; splitting it hides the flow"
)]
fn traceability_report() {
    let root = common::repo_root();
    let spec = std::fs::read_to_string(root.join("docs/spec.md")).expect("docs/spec.md");
    let ruled = ruled_ids(&spec);
    assert!(
        ruled.len() >= 60,
        "parsed only {} rulings from §12",
        ruled.len()
    );

    let out_of_scope = listed_ids(&root.join("docs/traceability-out-of-scope.txt"));
    let pending = listed_ids(&root.join("docs/traceability-pending.txt"));
    let mut tagged = BTreeMap::new();
    tagged_ids(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("features"),
        &mut tagged,
    );

    let in_scope: BTreeSet<u32> = ruled
        .iter()
        .copied()
        .filter(|id| !out_of_scope.contains_key(id))
        .collect();
    let covered: BTreeSet<u32> = in_scope
        .iter()
        .copied()
        .filter(|id| tagged.contains_key(id))
        .collect();
    let partial: BTreeSet<u32> = covered
        .iter()
        .copied()
        .filter(|id| pending.get(id).is_some_and(|n| n.starts_with("partial:")))
        .collect();
    let stale: BTreeSet<u32> = covered
        .iter()
        .copied()
        .filter(|id| pending.contains_key(id) && !partial.contains(id))
        .collect();
    let untracked: BTreeSet<u32> = in_scope
        .iter()
        .copied()
        .filter(|id| !tagged.contains_key(id) && !pending.contains_key(id))
        .collect();
    let unknown_tags: BTreeSet<u32> = tagged
        .keys()
        .copied()
        .filter(|id| !ruled.contains(id))
        .collect();
    let unknown_listed: BTreeSet<u32> = pending
        .keys()
        .chain(out_of_scope.keys())
        .copied()
        .filter(|id| !ruled.contains(id))
        .collect();
    let both: BTreeSet<u32> = pending
        .keys()
        .copied()
        .filter(|id| out_of_scope.contains_key(id))
        .collect();
    let bonus: BTreeSet<u32> = tagged
        .keys()
        .copied()
        .filter(|id| out_of_scope.contains_key(id))
        .collect();

    println!("\n=== Spec traceability (docs/spec.md §12) ===");
    println!(
        "rulings: {}  in v1 scope: {}  out of scope: {}",
        ruled.len(),
        in_scope.len(),
        out_of_scope.len()
    );
    println!(
        "covered: {}/{} in scope ({} partial)",
        covered.len() - partial.len(),
        in_scope.len(),
        partial.len()
    );
    println!(
        "  fully covered:  {}",
        fmt_ids(covered.difference(&partial))
    );
    println!("  partial:        {}", fmt_ids(&partial));
    println!(
        "  pending:        {}",
        fmt_ids(pending.keys().filter(|i| !covered.contains(i)))
    );
    println!("  tagged, out of scope: {}", fmt_ids(&bonus));
    for (id, files) in &tagged {
        println!(
            "    R{id}: {}",
            files.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    if !stale.is_empty() {
        println!(
            "  STALE pending (now tagged; delete from traceability-pending.txt): {}",
            fmt_ids(&stale)
        );
    }

    let mut problems = Vec::new();
    if !untracked.is_empty() {
        problems.push(format!(
            "in-scope rulings with no scenario and not in traceability-pending.txt: {}",
            fmt_ids(&untracked)
        ));
    }
    if !unknown_tags.is_empty() {
        problems.push(format!(
            "tags cite rulings not in §12: {}",
            fmt_ids(&unknown_tags)
        ));
    }
    if !unknown_listed.is_empty() {
        problems.push(format!(
            "allowlists name rulings not in §12: {}",
            fmt_ids(&unknown_listed)
        ));
    }
    if !both.is_empty() {
        problems.push(format!(
            "listed as both pending and out of scope: {}",
            fmt_ids(&both)
        ));
    }
    if std::env::var("TRACEABILITY_STRICT").is_ok_and(|v| v == "1") && !stale.is_empty() {
        problems.push(format!("stale pending entries: {}", fmt_ids(&stale)));
    }
    assert!(
        problems.is_empty(),
        "traceability problems:\n  {}",
        problems.join("\n  ")
    );
}
