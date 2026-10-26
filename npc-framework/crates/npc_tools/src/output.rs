//! Where JSON goes (stdout or `--out FILE`) and how failures are reported.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

/// Exit code: everything fine.
pub const EXIT_OK: u8 = 0;
/// Exit code: a check failed (replay mismatch, divergence, bench regression).
pub const EXIT_CHECK_FAILED: u8 = 1;
/// Exit code: usage or input error (clap errors, unreadable files, bad
/// commands, unknown units).
pub const EXIT_USAGE: u8 = 2;

/// A failed invocation: exit `code`, with `body` (always an object with an
/// `"error"` string) printed as JSON on stdout and the message on stderr.
#[derive(Debug)]
pub struct Failure {
    pub code: u8,
    pub body: Value,
}

impl Failure {
    /// A usage / input error (exit 2) with message `msg`.
    pub fn usage(msg: impl Into<String>) -> Self {
        Self {
            code: EXIT_USAGE,
            body: json!({ "error": msg.into() }),
        }
    }

    /// Adds `key: value` to the JSON body.
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        if let Value::Object(map) = &mut self.body {
            map.insert(
                key.to_owned(),
                serde_json::to_value(value).unwrap_or(Value::Null),
            );
        }
        self
    }

    /// The human-readable message.
    pub fn message(&self) -> &str {
        self.body["error"].as_str().unwrap_or("error")
    }
}

/// Reads and parses a JSON file, failing with exit 2 and a message naming it.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T, Failure> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        Failure::usage(format!("cannot read {what} {}: {e}", path.display()))
            .with("path", path.display().to_string())
    })?;
    serde_json::from_str(&text).map_err(|e| {
        Failure::usage(format!("{what} {} is not valid: {e}", path.display()))
            .with("path", path.display().to_string())
    })
}

/// Writes `value` as JSON to `path` (side outputs like `--record`).
pub fn write_json_file(path: &Path, value: &impl Serialize, pretty: bool) -> Result<(), Failure> {
    let result = File::create(path).and_then(|f| {
        let mut w = BufWriter::new(f);
        if pretty {
            serde_json::to_writer_pretty(&mut w, value)?;
        } else {
            serde_json::to_writer(&mut w, value)?;
        }
        w.write_all(b"\n")?;
        w.flush()
    });
    result.map_err(|e| Failure::usage(format!("cannot write {}: {e}", path.display())))
}

/// The main output: stdout, or `--out FILE` (created on the first write so a
/// failing command leaves no empty file behind).
pub struct Out {
    path: Option<PathBuf>,
    pretty: bool,
    writer: Option<Box<dyn Write>>,
}

impl Out {
    pub fn new(path: Option<PathBuf>, pretty: bool) -> Self {
        Self {
            path,
            pretty,
            writer: None,
        }
    }

    fn writer(&mut self) -> Result<&mut Box<dyn Write>, Failure> {
        if self.writer.is_none() {
            let w: Box<dyn Write> = match &self.path {
                Some(p) => Box::new(BufWriter::new(File::create(p).map_err(|e| {
                    Failure::usage(format!("cannot create {}: {e}", p.display()))
                })?)),
                None => Box::new(BufWriter::new(io::stdout())),
            };
            self.writer = Some(w);
        }
        Ok(self.writer.as_mut().expect("just set"))
    }

    /// One JSON document (indented with `--pretty`), newline-terminated.
    pub fn json(&mut self, value: &impl Serialize) -> Result<(), Failure> {
        let pretty = self.pretty;
        let w = self.writer()?;
        let result = if pretty {
            serde_json::to_writer_pretty(&mut *w, value)
        } else {
            serde_json::to_writer(&mut *w, value)
        }
        .map_err(io::Error::from)
        .and_then(|()| w.write_all(b"\n"));
        io_result(result)
    }

    /// One JSON Lines record (never indented).
    pub fn line(&mut self, value: &impl Serialize) -> Result<(), Failure> {
        let w = self.writer()?;
        let result = serde_json::to_writer(&mut *w, value)
            .map_err(io::Error::from)
            .and_then(|()| w.write_all(b"\n"));
        io_result(result)
    }

    pub fn finish(&mut self) -> Result<(), Failure> {
        match &mut self.writer {
            Some(w) => io_result(w.flush()),
            None => Ok(()),
        }
    }
}

/// A closed stdout pipe (`npcctl journal .. | head`) is not an error.
fn io_result(result: io::Result<()>) -> Result<(), Failure> {
    match result {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Failure::usage(format!("cannot write output: {e}"))),
    }
}
