//! Keeping `docs/spec.md` in step with the canonical spec in Dan's vault.
//!
//! `docs/spec.md` is a header line, a blank line, then the vault file byte for byte. Agents
//! and the traceability test read the repo copy, so a vault edit is invisible until it is
//! synced. `sim-cli spec-sync` copies it across and reports which rulings were added, removed
//! or reworded, so the scenarios tagged with a reworded ruling get re-checked.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Start of the header line written above the vault text.
pub const HEADER_PREFIX: &str = "> Snapshot of the canonical spec in Dan's vault";

/// The vault spec: `$OPENTYCOON_VAULT_SPEC`, else `~/Vaults/Homelab/Projects/opentycoon/spec.md`.
pub fn default_vault_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OPENTYCOON_VAULT_SPEC") {
        return Some(PathBuf::from(path));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join("Vaults/Homelab/Projects/opentycoon/spec.md"))
}

/// The vault text inside a repo snapshot (everything after the header and its blank line).
/// A file without the header is returned whole.
pub fn body(snapshot: &str) -> &str {
    match snapshot.strip_prefix(HEADER_PREFIX) {
        Some(rest) => rest
            .split_once('\n')
            .map_or("", |(_, after)| after.strip_prefix('\n').unwrap_or(after)),
        None => snapshot,
    }
}

/// The date a snapshot was taken, from its header.
pub fn taken(snapshot: &str) -> Option<&str> {
    let first = snapshot.lines().next()?;
    let rest = first.split_once("taken ")?.1;
    rest.split_once('.').map(|(date, _)| date)
}

/// Builds a snapshot: header naming the date and the ruling range, a blank line, the vault text.
pub fn snapshot(vault: &str, date: &str) -> String {
    let max = rulings(vault).keys().next_back().copied().unwrap_or(0);
    format!(
        "{HEADER_PREFIX} (Projects/opentycoon/spec.md), taken {date}. Rulings are cited by ID (R1–R{max}).\n\n{vault}"
    )
}

/// `R-id -> decision text` from the "Ruled" table in §12.
pub fn rulings(spec: &str) -> BTreeMap<u32, String> {
    let Some(section) = spec.split("## 12. Decisions").nth(1) else {
        return BTreeMap::new();
    };
    let ruled = section.split("### Open").next().unwrap_or(section);
    ruled
        .lines()
        .filter_map(|line| {
            let mut cells = line.trim().strip_prefix('|')?.split('|');
            let id = cells.next()?.trim().strip_prefix('R')?.parse().ok()?;
            let text = cells.next()?.trim().to_owned();
            Some((id, text))
        })
        .collect()
}

/// How the rulings changed between two spec texts.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RulingChanges {
    /// New rulings.
    pub added: BTreeSet<u32>,
    /// Rulings that disappeared.
    pub removed: BTreeSet<u32>,
    /// Rulings whose text changed.
    pub reworded: BTreeSet<u32>,
}

impl RulingChanges {
    /// True if no ruling changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.reworded.is_empty()
    }
}

/// Compares the rulings in two spec texts.
pub fn ruling_changes(old: &str, new: &str) -> RulingChanges {
    let (old, new) = (rulings(old), rulings(new));
    let mut changes = RulingChanges::default();
    for (id, text) in &new {
        match old.get(id) {
            None => {
                changes.added.insert(*id);
            }
            Some(before) if before != text => {
                changes.reworded.insert(*id);
            }
            Some(_) => {}
        }
    }
    changes.removed = old
        .keys()
        .filter(|id| !new.contains_key(id))
        .copied()
        .collect();
    changes
}

/// Feature files (relative to `features_dir`) tagging each of `ids`.
pub fn features_tagging(features_dir: &Path, ids: &BTreeSet<u32>) -> BTreeMap<u32, Vec<String>> {
    let mut out: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut stack = vec![features_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "feature") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let name = path
                .strip_prefix(features_dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            let tagged: BTreeSet<u32> = text
                .lines()
                .map(str::trim)
                .filter(|l| l.starts_with('@'))
                .flat_map(str::split_whitespace)
                .filter_map(|t| t.strip_prefix("@R")?.parse().ok())
                .collect();
            for id in tagged.intersection(ids) {
                out.entry(*id).or_default().push(name.clone());
            }
        }
    }
    for files in out.values_mut() {
        files.sort();
    }
    out
}

/// Today's UTC date as `YYYY-MM-DD`. The CLI may read the wall clock; sim-core may not.
#[allow(
    clippy::disallowed_methods,
    reason = "sim-cli is a tool, not the sim; it dates the snapshot"
)]
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    civil_date(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// `YYYY-MM-DD` for a count of days since 1970-01-01 (Howard Hinnant's `civil_from_days`).
pub fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = "# Spec\n\n## 12. Decisions\n\n### Ruled\n\n| # | Decision |\n|---|---|\n| R1 | Day trade 07:00–17:00 |\n| R2 | No restock |\n\n### Open\n\n| R99 | not a ruling |\n";

    #[test]
    fn snapshot_round_trips_to_the_vault_text() {
        let snap = snapshot(SPEC, "2026-11-20");
        assert!(snap.starts_with(HEADER_PREFIX));
        assert!(snap.contains("taken 2026-11-20. Rulings are cited by ID (R1–R2)."));
        assert_eq!(body(&snap), SPEC);
        assert_eq!(taken(&snap), Some("2026-11-20"));
    }

    #[test]
    fn body_of_an_unheaded_file_is_the_whole_file() {
        assert_eq!(body(SPEC), SPEC);
    }

    #[test]
    fn rulings_come_only_from_the_ruled_table() {
        let r = rulings(SPEC);
        assert_eq!(r.keys().copied().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(r[&2], "No restock");
    }

    #[test]
    fn changes_name_added_removed_and_reworded_rulings() {
        let new = SPEC
            .replace(
                "| R1 | Day trade 07:00–17:00 |",
                "| R1 | Day trade 08:00–17:00 |",
            )
            .replace("| R2 | No restock |\n", "| R3 | New rule |\n");
        let c = ruling_changes(SPEC, &new);
        assert_eq!(c.reworded, BTreeSet::from([1]));
        assert_eq!(c.removed, BTreeSet::from([2]));
        assert_eq!(c.added, BTreeSet::from([3]));
        assert!(ruling_changes(SPEC, SPEC).is_empty());
    }

    #[test]
    fn civil_date_matches_known_days() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_411), "2025-11-19");
        assert_eq!(civil_date(11_016), "2000-02-29");
    }
}
