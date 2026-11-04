//! The recording file format and its writer. A recording is JSONL: one
//! `header` line, then `command` and `input` lines stamped with the runner's
//! simulated-tick counter, then an `end` line. `replay` reads the same types.
use horde_core::{
    command::Command,
    sim::{Game, Input, RULESET_VERSION, SAVE_VERSION},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Bumped when the line format changes incompatibly.
pub const FORMAT: u32 = 1;

/// First line: what the replay must start from and be compatible with.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub format: u32,
    pub seed: u64,
    /// Save-envelope version of the writing build (`SAVE_VERSION`).
    pub version: u32,
    pub ruleset: u32,
    /// `catalog::content_fingerprint()` as 16 hex digits.
    pub content_fingerprint: String,
    /// Simulated-tick counter when recording began; replay starts counting here.
    pub start_tick: u64,
    /// The whole game at `start_tick` (the save payload), so a recording that
    /// begins mid-session replays exactly.
    pub initial: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Line {
    Header(Header),
    /// Applied through `Game::apply` before tick `tick` runs.
    Command {
        tick: u64,
        command: Command,
    },
    /// The input for tick `tick` and every later tick until the next input line.
    Input {
        tick: u64,
        input: Input,
    },
    /// `tick` is the counter after the last recorded tick; `digest` is the
    /// state hash at that point.
    End {
        tick: u64,
        digest: String,
        reason: String,
    },
}

pub fn hex(value: u64) -> String {
    format!("{value:016x}")
}

pub struct Recorder {
    path: PathBuf,
    out: BufWriter<File>,
    last_input: Option<String>,
    /// First write failure, reported by `flush` (writes themselves never fail the frame).
    error: Option<String>,
}
impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
impl Recorder {
    /// Creates `path` (and its folder) and writes the header for `game` at `start_tick`.
    pub fn create(path: &Path, game: &Game, start_tick: u64) -> Result<Self, String> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut recorder = Self {
            path: path.to_owned(),
            out: BufWriter::new(file),
            last_input: None,
            error: None,
        };
        let initial = serde_json::to_value(game).map_err(|e| e.to_string())?;
        recorder.write(&Line::Header(Header {
            format: FORMAT,
            seed: game.seed(),
            version: SAVE_VERSION,
            ruleset: RULESET_VERSION,
            content_fingerprint: hex(horde_core::catalog::content_fingerprint()),
            start_tick,
            initial,
        }));
        recorder.flush()?;
        Ok(recorder)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    fn write(&mut self, line: &Line) {
        let result = serde_json::to_writer(&mut self.out, line)
            .map_err(|e| e.to_string())
            .and_then(|()| self.out.write_all(b"\n").map_err(|e| e.to_string()));
        if let Err(error) = result {
            self.error.get_or_insert(error);
        }
    }
    pub fn command(&mut self, tick: u64, command: &Command) {
        self.write(&Line::Command {
            tick,
            command: command.clone(),
        });
    }
    /// Writes a line only when the input differs from the previous tick's.
    pub fn input(&mut self, tick: u64, input: &Input) {
        let Ok(encoded) = serde_json::to_string(input) else {
            return;
        };
        if self.last_input.as_deref() == Some(encoded.as_str()) {
            return;
        }
        self.last_input = Some(encoded);
        self.write(&Line::Input {
            tick,
            input: input.clone(),
        });
    }
    /// Pushes the buffer to the file. Called once per frame; never syncs.
    pub fn flush(&mut self) -> Result<(), String> {
        if let Some(error) = self.error.take() {
            return Err(format!("{}: {error}", self.path.display()));
        }
        self.out
            .flush()
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }
    /// Writes the `end` line (with the digest of `game`) and flushes.
    pub fn finish(mut self, tick: u64, game: &Game, reason: &str) {
        self.write(&Line::End {
            tick,
            digest: hex(game.state_digest()),
            reason: reason.into(),
        });
        if let Err(error) = self.flush() {
            bevy::log::warn!("Recording end line not written: {error}");
        }
    }
}
