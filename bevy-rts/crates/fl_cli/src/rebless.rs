//! `rebless` (ADR-0021, R146): re-run every golden replay of a directory on this build under a
//! mechanic change request, and write the new replays as `<name>.replay.new` for the lead to
//! bless, with a report.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fl_sim::{ReblessEntry, Replay, rebless_replay};
use serde_json::{Value, json};

use crate::agent::{Game, read_replay};
use crate::report::{CliError, Report, hash_hex};

/// What `rebless` re-runs and where it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReblessArgs {
    /// The MCR id, such as `MCR-001`: `## MCR-001` must head an entry of the changelog.
    pub reason: String,
    /// The directory of the `*.replay` files.
    pub dir: PathBuf,
    /// The MCR changelog.
    pub changelog: PathBuf,
    /// Where the markdown report is written.
    pub report: PathBuf,
}

/// Whether `changelog` has the heading `## <mcr>` (followed by the end of the line or a
/// character that cannot continue the id).
#[must_use]
pub(crate) fn has_mcr_heading(changelog: &str, mcr: &str) -> bool {
    changelog.lines().any(|line| {
        line.strip_prefix("## ")
            .and_then(|rest| rest.strip_prefix(mcr))
            .is_some_and(|rest| {
                rest.chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric() && c != '-' && c != '_')
            })
    })
}

/// One re-blessed replay.
struct Reblessed {
    name: String,
    /// The new bytes, if they differ from the file's.
    new: Option<Vec<u8>>,
    /// The ids of its semantic checks.
    checks: Vec<String>,
    /// What differs between the file and the new replay ([`changes`]).
    changes: Vec<String>,
    entry: ReblessEntry,
}

/// The text of the `- Predicted effect:` line of the `## <mcr>` entry of `changelog`, with its
/// indented continuation lines joined; `None` if the entry has none.
fn predicted_effect(changelog: &str, mcr: &str) -> Option<String> {
    let mut lines = changelog
        .lines()
        .skip_while(|line| !has_mcr_heading(line, mcr))
        .skip(1)
        .take_while(|line| !line.starts_with("## "));
    let first = lines.find_map(|line| line.strip_prefix("- Predicted effect:"))?;
    let mut text = first.trim().to_owned();
    for line in lines.take_while(|line| line.starts_with(' ') && !line.trim().is_empty()) {
        text.push(' ');
        text.push_str(line.trim());
    }
    Some(text)
}

/// What differs between the replay file `old_bytes` (decoded as `old`) and its re-run `new`:
/// the format version, each header field that differs (such as `build_id` after any commit),
/// the hash stream, the inputs (command ids remapped), the journal summary.
fn changes(old_bytes: &[u8], old: &Replay, new: &Replay) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok((format, _)) = postcard::take_from_bytes::<u32>(old_bytes)
        && format != new.version
    {
        out.push(format!("format {format} → {}", new.version));
    }
    let (old_header, new_header) = (json!(old.header), json!(new.header));
    if let (Some(a), Some(b)) = (old_header.as_object(), new_header.as_object()) {
        for (field, value) in a {
            if b.get(field) != Some(value) {
                out.push(format!("header {field}"));
            }
        }
    }
    if old.hashes != new.hashes {
        out.push("hash stream".to_owned());
    }
    if old.inputs != new.inputs {
        out.push("inputs (command ids remapped)".to_owned());
    }
    if old.journal_summary != new.journal_summary {
        out.push("journal summary".to_owned());
    }
    out
}

/// The `*.replay` files of `dir`, by name.
fn replays_in(dir: &Path) -> Result<Vec<PathBuf>, CliError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| CliError::Io(format!("cannot read {}: {e}", dir.display())))?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| CliError::Io(format!("cannot read {}: {e}", dir.display())))?
            .path();
        if path.extension().is_some_and(|ext| ext == "replay") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// `rebless`: refuse (exit 1, writing nothing) when the changelog has no `## <reason>` heading
/// or a replay's semantic check fails on this build; otherwise re-run every `*.replay` of the
/// directory with [`rebless_replay`], write `<name>.replay.new` for every replay whose bytes
/// change, and the report: per replay the first divergent tick, the journal summary diff, the
/// entity count at the start and the end, and the semantic results.
///
/// JSON: `{"reason", "report", "replays": [{"name", "changed", "first_divergence",
/// "journal_diff"}]}`, `first_divergence` null or `{"tick", "expected", "actual"}`, and
/// `journal_diff` a list of `{"kind", "before", "after"}`.
///
/// # Errors
/// [`CliError::Check`] for an unknown MCR, a failed semantic check or a directory without
/// replays; any error reading, re-running or writing.
pub fn rebless(game: &Game, args: &ReblessArgs) -> Result<Report, CliError> {
    let changelog = std::fs::read_to_string(&args.changelog)
        .map_err(|e| CliError::Io(format!("cannot read {}: {e}", args.changelog.display())))?;
    if !has_mcr_heading(&changelog, &args.reason) {
        return Err(CliError::Check(format!(
            "{} has no `## {}` heading: write the MCR entry, with its predicted effect, before \
             re-blessing",
            args.changelog.display(),
            args.reason
        )));
    }
    let paths = replays_in(&args.dir)?;
    if paths.is_empty() {
        return Err(CliError::Check(format!(
            "no *.replay files in {}",
            args.dir.display()
        )));
    }
    let mut done = Vec::new();
    for path in &paths {
        let old_bytes = std::fs::read(path)
            .map_err(|e| CliError::Io(format!("cannot read {}: {e}", path.display())))?;
        let replay = read_replay(path)?;
        let (new, entry) = rebless_replay(&replay, &game.features, Arc::clone(&game.content))
            .map_err(|e| CliError::Setup(format!("{}: {e}", path.display())))?;
        let new_bytes = new.to_bytes();
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let mut changed = changes(&old_bytes, &replay, &new);
        if changed.is_empty() && new_bytes != old_bytes {
            changed.push("encoding".to_owned());
        }
        done.push(Reblessed {
            name,
            new: (new_bytes != old_bytes).then_some(new_bytes),
            checks: replay.semantic.iter().map(|c| c.id().to_owned()).collect(),
            changes: changed,
            entry,
        });
    }
    let failed: Vec<String> = done
        .iter()
        .flat_map(|r| {
            r.entry.semantic.iter().map(|f| {
                format!(
                    "{}: semantic check {} failed at tick {}: {}",
                    r.name, f.id, f.tick.0, f.detail
                )
            })
        })
        .collect();
    if !failed.is_empty() {
        return Err(CliError::Check(format!(
            "refusing to re-bless under {}: semantic checks fail on this build\n{}",
            args.reason,
            failed.join("\n")
        )));
    }

    for r in &done {
        if let Some(bytes) = &r.new {
            let path = args.dir.join(format!("{}.new", r.name));
            std::fs::write(&path, bytes)
                .map_err(|e| CliError::Io(format!("cannot write {}: {e}", path.display())))?;
        }
    }
    let predicted = predicted_effect(&changelog, &args.reason);
    let markdown = report_markdown(&args.reason, predicted.as_deref(), &done);
    if let Some(parent) = args.report.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::Io(format!("cannot create {}: {e}", parent.display())))?;
    }
    std::fs::write(&args.report, &markdown)
        .map_err(|e| CliError::Io(format!("cannot write {}: {e}", args.report.display())))?;

    let replays: Vec<Value> = done.iter().map(replay_json).collect();
    let json = json!({
        "reason": args.reason,
        "predicted_effect": predicted,
        "report": args.report,
        "replays": replays,
    });
    let mut text = String::new();
    for r in &done {
        let _ = writeln!(
            text,
            "{}: {}; first divergent tick: {}",
            r.name,
            if r.new.is_some() {
                format!("changed ({}), wrote .new", r.changes.join("; "))
            } else {
                "unchanged".to_owned()
            },
            r.entry
                .first_divergence
                .as_ref()
                .map_or_else(|| "none".to_owned(), |d| d.tick.0.to_string())
        );
    }
    let _ = writeln!(text, "report: {}", args.report.display());
    Ok(Report::ok(json, text))
}

fn replay_json(r: &Reblessed) -> Value {
    let divergence = r.entry.first_divergence.as_ref().map(|d| {
        json!({
            "tick": d.tick.0,
            "expected": hash_hex(d.expected),
            "actual": hash_hex(d.actual),
        })
    });
    let journal_diff: Vec<Value> = r
        .entry
        .journal_diff
        .iter()
        .map(|(kind, before, after)| json!({"kind": kind, "before": before, "after": after}))
        .collect();
    json!({
        "name": r.name,
        "changed": r.new.is_some(),
        "changes": r.changes,
        "first_divergence": divergence,
        "journal_diff": journal_diff,
    })
}

/// The markdown report of a re-bless under `reason`.
fn report_markdown(reason: &str, predicted: Option<&str>, done: &[Reblessed]) -> String {
    let mut out = format!(
        "# Re-bless report: {reason}\n\nCompare each replay with the predicted effect of the \
         `{reason}` entry; an unpredicted difference is a finding, not a re-bless (ADR-0021).\n\n"
    );
    let _ = writeln!(
        out,
        "Predicted effect ({reason}): {}",
        predicted.unwrap_or("(the entry has no `- Predicted effect:` line)")
    );
    for r in done {
        let e = &r.entry;
        let _ = writeln!(out, "\n## {}\n", r.name);
        let _ = writeln!(
            out,
            "- changed: {}",
            if r.new.is_some() {
                format!("yes ({}), wrote {}.new", r.changes.join("; "), r.name)
            } else {
                "no".to_owned()
            }
        );
        match &e.first_divergence {
            None => {
                let _ = writeln!(out, "- first divergent tick: none");
            }
            Some(d) => {
                let _ = writeln!(
                    out,
                    "- first divergent tick: {} (recorded {}, now {})",
                    d.tick.0,
                    hash_hex(d.expected),
                    hash_hex(d.actual)
                );
            }
        }
        let _ = writeln!(
            out,
            "- entities: {} at the start, {} at the end",
            e.entities.0, e.entities.1
        );
        if r.checks.is_empty() {
            let _ = writeln!(out, "- semantic checks: none");
        } else {
            let _ = writeln!(
                out,
                "- semantic checks: all {} hold ({})",
                r.checks.len(),
                r.checks.join(", ")
            );
        }
        if e.journal_diff.is_empty() {
            let _ = writeln!(out, "- journal summary diff: none");
        } else {
            let _ = writeln!(
                out,
                "- journal summary diff:\n\n  | kind | recorded | now |\n  |---|---:|---:|"
            );
            for (kind, before, after) in &e.journal_diff {
                let _ = writeln!(out, "  | `{kind}` | {before} | {after} |");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{has_mcr_heading, predicted_effect};

    #[test]
    fn predicted_effect_is_quoted_from_its_entry() {
        let log = "## MCR-000 — A (C8)\n\n- Why: x.\n- Predicted effect: replay bytes change;\n  \
                   hash streams identical.\n- SIM_VERSION: unchanged (2).\n\n## MCR-001 — B\n\n\
                   - Predicted effect: poses change.\n";
        assert_eq!(
            predicted_effect(log, "MCR-000").as_deref(),
            Some("replay bytes change; hash streams identical.")
        );
        assert_eq!(
            predicted_effect(log, "MCR-001").as_deref(),
            Some("poses change.")
        );
        assert_eq!(
            predicted_effect("## MCR-002 — C\n\n- Why: x.\n", "MCR-002"),
            None
        );
    }

    #[test]
    fn mcr_headings_match_whole_ids() {
        let log = "# MCRs\n\n## MCR-000 — Replay format 4 (C8, M10)\n### MCR-002 — x\n";
        assert!(has_mcr_heading(log, "MCR-000"));
        assert!(!has_mcr_heading(log, "MCR-00"));
        assert!(!has_mcr_heading(log, "MCR-002"), "a level-3 heading");
        assert!(!has_mcr_heading(log, "MCR-999"));
        assert!(has_mcr_heading("## MCR-007\n", "MCR-007"));
    }
}
