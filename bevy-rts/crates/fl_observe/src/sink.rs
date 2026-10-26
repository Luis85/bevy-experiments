//! The journal sink: `journal.jsonl` in the run directory (spec §15 B).

use std::fs::File;
use std::io::{self, BufWriter, Write};

use fl_view::JournalEntry;

use crate::logging::RunDir;

/// Appends journal records to `journal.jsonl`, one JSON object per line.
#[derive(Debug)]
pub struct JournalSink {
    out: BufWriter<File>,
}

impl JournalSink {
    /// Creates (or truncates) `journal.jsonl` in `run`.
    ///
    /// # Errors
    /// Any I/O error creating the file.
    pub fn create(run: &RunDir) -> io::Result<Self> {
        let file = File::create(run.path.join("journal.jsonl"))?;
        Ok(Self {
            out: BufWriter::new(file),
        })
    }

    /// Writes `entries`, one line each, and flushes.
    ///
    /// # Errors
    /// Any I/O or serialization error.
    pub fn write(&mut self, entries: &[JournalEntry]) -> io::Result<()> {
        for entry in entries {
            serde_json::to_writer(&mut self.out, entry)?;
            self.out.write_all(b"\n")?;
        }
        self.out.flush()
    }
}
