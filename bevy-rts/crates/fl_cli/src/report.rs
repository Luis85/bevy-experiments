//! What every command returns: a [`Report`] or a [`CliError`], each with its exit code.

use serde_json::Value;

/// Why a CLI command failed.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Content or the sim could not be built.
    #[error("{0}")]
    Build(String),
    /// The scenario could not be set up.
    #[error("{0}")]
    Setup(String),
    /// A file could not be read or written.
    #[error("{0}")]
    Io(String),
    /// Bad arguments.
    #[error("{0}")]
    Usage(String),
    /// The command ran and found a problem: a bench baseline without a size in common, an
    /// entity that does not exist, ...
    #[error("{0}")]
    Check(String),
}

impl CliError {
    /// The process exit code: 2 for [`CliError::Usage`] (as for clap's own usage errors),
    /// 1 otherwise.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            CliError::Usage(_) => 2,
            _ => 1,
        }
    }
}

/// What a command prints and how it exits.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The one JSON document printed with `--json`.
    pub json: Value,
    /// The human-readable output, printed without `--json`.
    pub text: String,
    /// The process exit code.
    pub code: u8,
}

impl Report {
    /// A successful report (exit 0).
    #[must_use]
    pub fn ok(json: Value, text: String) -> Report {
        Report {
            json,
            text,
            code: 0,
        }
    }
}

/// `0x` and 16 hex digits: how every command prints a hash.
#[must_use]
pub fn hash_hex(hash: u64) -> String {
    format!("0x{hash:016x}")
}
