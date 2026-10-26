//! `journal`: filter a run's `journal.jsonl`, so an agent never reads the whole file.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use fl_protocol::{PlayerId, SimId, Tick};
use fl_view::JournalEntry;
use serde_json::json;

use crate::report::{CliError, Report};

/// Which journal entries to keep; every set field must match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JournalFilter {
    /// The record kind, exactly (`"core.unit_spawned"`).
    pub kind: Option<String>,
    pub sim_id: Option<u32>,
    pub player: Option<u8>,
    /// The first tick, inclusive.
    pub from: Option<u32>,
    /// The last tick, inclusive.
    pub to: Option<u32>,
    /// At most this many entries, the earliest.
    pub limit: Option<usize>,
}

impl JournalFilter {
    fn matches(&self, entry: &JournalEntry) -> bool {
        self.kind.as_ref().is_none_or(|kind| entry.kind == *kind)
            && self.sim_id.is_none_or(|id| entry.sim_id == Some(SimId(id)))
            && self
                .player
                .is_none_or(|p| entry.player == Some(PlayerId(p)))
            && self.from.is_none_or(|from| entry.tick >= Tick(from))
            && self.to.is_none_or(|to| entry.tick <= Tick(to))
    }
}

/// The entries of `path` (a `journal.jsonl`, or a run directory holding one) that match
/// `filter`, in tick order (stable: a tick's entries keep the journal's order), cut to the
/// filter's limit; and how many matched before the cut. The file is read line by line.
///
/// # Errors
/// [`CliError::Io`] if the file cannot be read, [`CliError::Setup`] naming the line of an
/// entry that does not parse.
pub fn read_journal(
    path: &Path,
    filter: &JournalFilter,
) -> Result<(Vec<JournalEntry>, usize), CliError> {
    let path = if path.is_dir() {
        path.join("journal.jsonl")
    } else {
        path.to_owned()
    };
    let io = |e: std::io::Error| CliError::Io(format!("{}: {e}", path.display()));
    let reader = BufReader::new(File::open(&path).map_err(io)?);
    let mut entries = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(io)?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: JournalEntry = serde_json::from_str(&line).map_err(|e| {
            CliError::Setup(format!(
                "{}:{}: not a journal entry: {e}",
                path.display(),
                index + 1
            ))
        })?;
        if filter.matches(&entry) {
            entries.push(entry);
        }
    }
    entries.sort_by_key(|entry| entry.tick);
    let matched = entries.len();
    if let Some(limit) = filter.limit {
        entries.truncate(limit);
    }
    Ok((entries, matched))
}

/// `journal`: [`read_journal`]. JSON: `{"entries": [...], "matched": n}`, where `matched`
/// counts the matches before `--limit`; the text is one JSON line per entry.
///
/// # Errors
/// As [`read_journal`].
pub fn journal(path: &Path, filter: &JournalFilter) -> Result<Report, CliError> {
    let (entries, matched) = read_journal(path, filter)?;
    let mut text = String::new();
    for entry in &entries {
        let line = serde_json::to_string(entry).map_err(|e| CliError::Io(e.to_string()))?;
        text.push_str(&line);
        text.push('\n');
    }
    Ok(Report::ok(
        json!({ "entries": entries, "matched": matched }),
        text,
    ))
}
