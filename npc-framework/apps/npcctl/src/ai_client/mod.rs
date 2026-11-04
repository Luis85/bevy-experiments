//! Local AI session supervision, outside the simulation and provider credential stores.
mod codex_model;
mod diagnostics;
mod job;
mod mcp_config;
mod metrics;
mod provider;
mod runtime;
mod session_log;
mod storage;

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::ExitCode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Codex,
    Claude,
}

#[derive(Parser)]
#[command(
    name = "ai-agent-game-client",
    version,
    about = "Manage a game-playing Codex or Claude CLI session using your existing provider login."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start or continue the exact AI session for a running explorer workspace.
    Run(runtime::RunArgs),
    /// Check the selected CLI and its existing login without starting an AI turn.
    Check {
        #[arg(long, value_enum)]
        provider: Provider,
        /// Absolute native executable; useful for installations outside PATH.
        #[arg(long)]
        program: Option<PathBuf>,
        /// Also verify Codex model selection in this game directory without starting an AI turn.
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Read sanitized client status. Does not contact or change the game.
    Status {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Ask this workspace's client to stop. Leaves the game running.
    Stop {
        #[arg(long)]
        workspace: PathBuf,
    },
}

pub fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Run(args) => runtime::run(args),
        Command::Check {
            provider,
            program,
            workspace,
        } => check(provider, program.as_deref(), workspace.as_deref()),
        Command::Status { workspace } => storage::status(&workspace),
        Command::Stop { workspace } => storage::stop(&workspace),
    };
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", serde_json::json!({"error":error}));
            ExitCode::from(2)
        }
    }
}

fn check(
    provider: Provider,
    program: Option<&std::path::Path>,
    workspace: Option<&std::path::Path>,
) -> Result<serde_json::Value, String> {
    let workspace = workspace
        .map(|path| {
            path.canonicalize()
                .ok()
                .filter(|path| path.is_dir())
                .ok_or_else(|| "--workspace must name an existing game directory".to_owned())
        })
        .transpose()?;
    let mut report = provider::check(provider, program)?;
    if provider == Provider::Codex
        && let Some(workspace) = workspace
    {
        report["selection_intent"] = serde_json::Value::Null;
        report["model_selection_ready"] = false.into();
        if report["ready"] == true {
            let executable = provider::resolve(provider, program)?;
            match codex_model::prepare(&executable, &workspace, false) {
                Ok(selection) => {
                    report["selection_intent"] = serde_json::to_value(selection)
                        .map_err(|_| "Cannot encode verified Codex selection")?;
                    report["model_selection_ready"] = true.into();
                    report["message"] = "CLI login and model selection are ready; actual access is checked when a turn starts".into();
                }
                Err(error) => {
                    report["ready"] = false.into();
                    report["message"] = error.into();
                }
            }
        }
    }
    Ok(report)
}
