//! Native operator controls for the separate AI client. Never called by simulation systems.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// Displayed before explicit native Start actions. Only that action enables the client's bounded
/// per-tool game allowance; direct CLI clients retain their default-off permission choice.
pub const START_PERMISSION_NOTICE: &str =
    "Starting allows the agent to act in this world and use its private Notebook.";

/// The installed CLI supplies its own login and model settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Codex,
    Claude,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

/// Native status accepts only closed diagnostics, including from older or unknown producers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCode {
    Authentication,
    RateLimit,
    ModelUnavailable,
    McpStartup,
    GameToolCancelled,
    GameAccessRevoked,
    GameToolsUnavailable,
    GamePermissionDenied,
    ProviderConfig,
    Network,
    OutputLimit,
    MissingTerminal,
    NonzeroExit,
    Unknown,
}

fn deserialize_failure_code<'de, D>(deserializer: D) -> Result<Option<FailureCode>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Unknown values are discarded immediately rather than retained in UI state or Debug.
    let code = Option::<String>::deserialize(deserializer)?;
    Ok(code.map(|code| match code.as_str() {
        "authentication" => FailureCode::Authentication,
        "rate_limit" => FailureCode::RateLimit,
        "model_unavailable" => FailureCode::ModelUnavailable,
        "mcp_startup" => FailureCode::McpStartup,
        "game_tool_cancelled" => FailureCode::GameToolCancelled,
        "game_access_revoked" => FailureCode::GameAccessRevoked,
        "game_tools_unavailable" => FailureCode::GameToolsUnavailable,
        "game_permission_denied" => FailureCode::GamePermissionDenied,
        "provider_config" => FailureCode::ProviderConfig,
        "network" => FailureCode::Network,
        "output_limit" => FailureCode::OutputLimit,
        "missing_terminal" => FailureCode::MissingTerminal,
        "nonzero_exit" => FailureCode::NonzeroExit,
        _ => FailureCode::Unknown,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionFault {
    Unavailable,
    OwnershipBusy,
    CapacityBusy,
    GameBusy,
    LeaseLost,
    Authentication,
    AssignmentChanged,
    ActorUnavailable,
    Revoked,
    Protocol,
    #[serde(other)]
    Unknown,
}
impl ConnectionFault {
    fn help(self) -> &'static str {
        match self {
            Self::Unavailable => "Waiting for this workspace's game server.",
            Self::OwnershipBusy => {
                "Waiting for the current AI client or legacy turn to release this actor."
            }
            Self::CapacityBusy => "Waiting for an available game AI slot.",
            Self::GameBusy => "Waiting for the game or actor to be ready.",
            Self::LeaseLost => {
                "The previous client lease ended; reconnecting after stopping its AI turn."
            }
            Self::Authentication => {
                "Game credentials were rejected. Reopen the correct world before restarting."
            }
            Self::AssignmentChanged => {
                "Actor assignment changed. Start a new AI session from the game controls."
            }
            Self::ActorUnavailable => {
                "The actor is unavailable. Use the game's recovery controls before restarting."
            }
            Self::Revoked => {
                "Game access was revoked. This client will not reconnect automatically."
            }
            Self::Protocol => {
                "Game and AI client protocols do not match. Use matching builds before restarting."
            }
            Self::Unknown => "Check the game server and AI client before restarting.",
        }
    }
}

/// Only the public, sanitized client status is read by the game.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(from = "StatusRecord")]
pub struct Status {
    pub pid: u32,
    pub provider: String,
    pub state: String,
    pub message: String,
    pub turns_completed: u64,
    pub session_ready: bool,
    pub connected: bool,
    pub actor_controlled: bool,
    pub last_connection_fault: Option<ConnectionFault>,
    pub updated_unix_seconds: u64,
    pub last_failure_code: Option<FailureCode>,
    /// Whether the producer reported phase fields, including explicit false values.
    #[doc(hidden)]
    pub phase_details_reported: bool,
}

/// Preserve absent legacy phase information instead of treating it as a new
/// producer's explicit disconnected/not-established report.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StatusRecord {
    pid: u32,
    provider: String,
    state: String,
    message: String,
    turns_completed: u64,
    session_ready: Option<bool>,
    connected: Option<bool>,
    actor_controlled: Option<bool>,
    last_connection_fault: Option<ConnectionFault>,
    updated_unix_seconds: u64,
    #[serde(default, deserialize_with = "deserialize_failure_code")]
    last_failure_code: Option<FailureCode>,
}
impl From<StatusRecord> for Status {
    fn from(record: StatusRecord) -> Self {
        Self {
            phase_details_reported: record.session_ready.is_some()
                || record.connected.is_some()
                || record.actor_controlled.is_some(),
            pid: record.pid,
            provider: record.provider,
            state: record.state,
            message: record.message,
            turns_completed: record.turns_completed,
            session_ready: record.session_ready.unwrap_or(false),
            connected: record.connected.unwrap_or(false),
            actor_controlled: record.actor_controlled.unwrap_or(false),
            last_connection_fault: record.last_connection_fault,
            updated_unix_seconds: record.updated_unix_seconds,
            last_failure_code: record.last_failure_code,
        }
    }
}
impl Status {
    pub fn is_active(&self) -> bool {
        matches!(
            self.state.as_str(),
            "starting" | "connecting" | "running" | "waiting" | "stopping"
        )
    }
    pub fn line(&self) -> String {
        if self.state.is_empty() {
            return "Ready - choose your logged-in CLI to start playing.".into();
        }
        let mut line = format!(
            "{}: {} | {} completed turn(s)",
            self.provider, self.state, self.turns_completed,
        );
        if self.phase_details_reported
            || self.connected
            || self.session_ready
            || self.actor_controlled
            || self.last_connection_fault.is_some()
        {
            line.push_str(&format!(
                "\nServer: {} | AI session: {} | Actor control: {}",
                if self.connected {
                    "connected"
                } else {
                    "disconnected"
                },
                if self.session_ready {
                    "established"
                } else {
                    "not established"
                },
                if self.actor_controlled {
                    "confirmed"
                } else {
                    "not confirmed"
                },
            ));
        }
        line.push('\n');
        line.push_str(&self.message);
        if let Some(fault) = self.last_connection_fault {
            line.push_str("\nGame connection: ");
            line.push_str(fault.help());
        }
        if let Some(code) = &self.last_failure_code {
            line.push_str("\nLast provider failure: ");
            line.push_str(failure_help(code));
        }
        line
    }
}

fn failure_help(code: &FailureCode) -> &'static str {
    match code {
        FailureCode::Authentication => {
            "Sign in through your selected CLI, then start the agent again."
        }
        FailureCode::RateLimit => {
            "The provider reported a usage limit. Wait for access to reset before restarting."
        }
        FailureCode::ModelUnavailable => {
            "Check the model selected in your CLI and whether your account can use it."
        }
        FailureCode::McpStartup => {
            "The game tools could not start. Check the game is running and npcctl is installed beside the client."
        }
        FailureCode::GameToolCancelled => {
            "Game tool call was cancelled. Review your CLI approval preferences, then start the agent again."
        }
        FailureCode::GameAccessRevoked => {
            "Game access was explicitly revoked. The agent stopped without reconnecting or replaying an action."
        }
        FailureCode::GameToolsUnavailable => {
            "The required npc tools failed to initialize. Check the running game and CLI setup before restarting."
        }
        FailureCode::GamePermissionDenied => {
            "The CLI denied a required npc game tool. Review its game-tool permissions before restarting."
        }
        FailureCode::ProviderConfig => "Check your CLI configuration, then start the agent again.",
        FailureCode::Network => "Check the provider connection, then start the agent again.",
        FailureCode::OutputLimit => {
            "The provider exceeded the client output limit. Check the CLI before restarting."
        }
        FailureCode::MissingTerminal => {
            "The CLI ended without confirming a completed turn. Check provider setup before restarting."
        }
        FailureCode::NonzeroExit => {
            "The CLI exited with an error. Check provider setup before restarting."
        }
        FailureCode::Unknown => {
            "The provider did not complete its turn. Check its CLI setup before restarting."
        }
    }
}

pub fn read_status(workspace: &Path) -> Status {
    let path = workspace.join(".ai-client/status.json");
    if !std::fs::metadata(&path).is_ok_and(|meta| meta.is_file() && meta.len() <= 16_384) {
        return Status::default();
    }
    let mut status: Status = std::fs::read(path)
        .ok()
        .filter(|bytes| bytes.len() <= 16_384)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let age = status_time().saturating_sub(status.updated_unix_seconds);
    if status.pid > 0
        && age > 10
        && !matches!(status.state.as_str(), "stopped" | "error")
        && !super::host::pid_alive(status.pid)
    {
        status.state = "error".into();
        status.connected = false;
        status.actor_controlled = false;
        status.message =
            "AI client exited. Choose Start Codex or Start Claude to resume your session.".into();
    }
    status
}

#[allow(clippy::disallowed_methods)]
fn status_time() -> u64 {
    // determinism: native client liveness display only, never simulation state or schedules.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

pub fn executable() -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|_| "Cannot locate the game executable.")?;
    let path = current.with_file_name(if cfg!(windows) {
        "ai-agent-game-client.exe"
    } else {
        "ai-agent-game-client"
    });
    if !path.is_file() {
        return Err(
            "AI client is not built. Run cargo build --locked -p npcctl --bins, then try again."
                .into(),
        );
    }
    Ok(path)
}

/// Starts a separate client; stopping or handing over the game does not kill an unrelated CLI.
pub fn launch(workspace: &Path, provider: Provider) -> Result<u32, String> {
    let (metrics, logs) = saved_client_folders(&|key| std::env::var(key).ok());
    launch_inner(
        workspace,
        provider,
        false,
        metrics.as_deref(),
        logs.as_deref(),
    )
}

/// The menu's operator preference overrides the client's environment/default destination.
pub fn launch_with_metrics(
    workspace: &Path,
    provider: Provider,
    metrics_dir: Option<&Path>,
) -> Result<u32, String> {
    let (_, logs) = saved_client_folders(&|key| std::env::var(key).ok());
    launch_inner(workspace, provider, false, metrics_dir, logs.as_deref())
}

/// Explicit menu preferences; blank folders preserve the client's inherited environment/defaults.
pub fn launch_with_folders(
    workspace: &Path,
    provider: Provider,
    metrics_dir: Option<&Path>,
    logs_dir: Option<&Path>,
) -> Result<u32, String> {
    launch_inner(workspace, provider, false, metrics_dir, logs_dir)
}

/// Explicit recovery for a lost provider conversation; previous private state is archived.
pub fn launch_fresh(workspace: &Path, provider: Provider) -> Result<u32, String> {
    let (metrics, logs) = saved_client_folders(&|key| std::env::var(key).ok());
    launch_inner(
        workspace,
        provider,
        true,
        metrics.as_deref(),
        logs.as_deref(),
    )
}

fn saved_client_folders(
    env: &dyn Fn(&str) -> Option<String>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    // A window launched by a menu must re-read that menu's exact settings on each Start,
    // including an explicit blank preference. Never substitute unrelated global settings.
    let file = match env("NPC_AI_SETTINGS_FILE") {
        Some(path) => (!path.is_empty()).then(|| PathBuf::from(path)),
        None => super::settings::Settings::file(env),
    };
    file.map(|file| super::settings::Settings::load(&file, Path::new("")))
        .map_or((None, None), |settings| {
            (settings.ai_metrics_dir, settings.ai_logs_dir)
        })
}

fn launch_inner(
    workspace: &Path,
    provider: Provider,
    fresh: bool,
    metrics_dir: Option<&Path>,
    logs_dir: Option<&Path>,
) -> Result<u32, String> {
    let workspace = workspace
        .canonicalize()
        .map_err(|_| "Agent folder is unavailable.")?;
    for (label, directory) in [
        ("AI metrics folder", metrics_dir),
        ("AI logs folder", logs_dir),
    ] {
        if let Some(directory) = directory {
            super::settings::validate_agents_dir(directory)
                .map_err(|error| format!("{label}: {error}"))?;
        }
    }
    let mut command = launch_command(
        &executable()?,
        &workspace,
        provider,
        fresh,
        metrics_dir,
        logs_dir,
    );
    let mut child = command
        .spawn()
        .map_err(|_| "Cannot start the AI client. Check installation and folder permissions.")?;
    let pid = child.id();
    // Reap our launcher handle without tying client lifetime to the current window.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

fn launch_command(
    program: &Path,
    workspace: &Path,
    provider: Provider,
    fresh: bool,
    metrics_dir: Option<&Path>,
    logs_dir: Option<&Path>,
) -> Command {
    let mut command = Command::new(program);
    command
        .arg("run")
        .arg("--workspace")
        .arg(workspace)
        .arg("--provider")
        .arg(provider.name())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if fresh {
        command.arg("--new-session");
    }
    if provider == Provider::Codex {
        command.arg("--approve-game-tools");
    }
    if let Some(metrics) = metrics_dir {
        command.arg("--metrics-dir").arg(metrics);
    }
    if let Some(logs) = logs_dir {
        command.arg("--logs-dir").arg(logs);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// A cooperative stop request. The client cleans up its own provider process and lease.
pub fn request_stop(workspace: &Path) -> Result<(), String> {
    let dir = workspace.join(".ai-client");
    if !dir.is_dir() {
        return Err("No AI client has started for this actor.".into());
    }
    std::fs::write(dir.join("stop"), b"stop\n")
        .map_err(|_| "Cannot request AI stop. Check agent folder permissions.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_starts_reread_the_exact_operator_settings_without_overriding_custom_blanks() {
        let root = std::env::temp_dir().join(format!(
            "npc-client-folders-{}",
            npc_framework::agent_io::registration::new_secret().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let appdata = root.join("global");
        let global_file = appdata.join("npc-framework/settings.json");
        let mut global = super::super::settings::Settings::default_for(&root);
        global.ai_metrics_dir = Some(root.join("global-metrics"));
        global.ai_logs_dir = Some(root.join("global-logs"));
        global.save(&global_file).unwrap();
        let global_env = |key: &str| {
            if key == "APPDATA" {
                Some(appdata.display().to_string())
            } else {
                None
            }
        };
        assert_eq!(
            saved_client_folders(&global_env),
            (global.ai_metrics_dir.clone(), global.ai_logs_dir.clone())
        );
        let custom_file = root.join("custom-settings.json");
        let mut custom = super::super::settings::Settings::default_for(&root);
        custom.save(&custom_file).unwrap();
        let custom_env = |key: &str| {
            if key == "NPC_AI_SETTINGS_FILE" {
                Some(custom_file.display().to_string())
            } else {
                global_env(key)
            }
        };
        let before = std::fs::read(&custom_file).unwrap();
        assert_eq!(
            saved_client_folders(&custom_env),
            (None, None),
            "custom blank preferences must not borrow unrelated global destinations"
        );
        assert_eq!(
            std::fs::read(&custom_file).unwrap(),
            before,
            "looking up a preference never rewrites settings"
        );
        custom.ai_logs_dir = Some(root.join("updated private logs"));
        custom.ai_metrics_dir = Some(root.join("updated metrics"));
        custom.save(&custom_file).unwrap();
        assert_eq!(
            saved_client_folders(&custom_env),
            (custom.ai_metrics_dir.clone(), custom.ai_logs_dir.clone()),
            "a later Start or Fresh must observe changed saved preferences"
        );
        custom.ai_logs_dir = None;
        custom.ai_metrics_dir = None;
        custom.save(&custom_file).unwrap();
        let (metrics, logs) = saved_client_folders(&custom_env);
        let command = launch_command(
            Path::new("client"),
            &root,
            Provider::Codex,
            true,
            metrics.as_deref(),
            logs.as_deref(),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(
            !args
                .iter()
                .any(|value| matches!(value.as_str(), "--logs-dir" | "--metrics-dir")),
            "cleared preferences leave inherited user environment and client defaults in effect"
        );
        assert!(
            command
                .get_envs()
                .all(|(key, _)| key != "NPC_AI_LOGS_DIR" && key != "NPC_AI_METRICS_DIR")
        );
        let empty_marker = |key: &str| {
            if key == "NPC_AI_SETTINGS_FILE" {
                Some(String::new())
            } else {
                global_env(key)
            }
        };
        assert_eq!(saved_client_folders(&empty_marker), (None, None));
        assert_eq!(
            root.parent().unwrap(),
            std::env::temp_dir().canonicalize().unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_native_codex_start_enables_only_the_scoped_game_allowance() {
        let workspace = Path::new("prepared game workspace");
        let metrics = Path::new("private metrics folder");
        let logs = Path::new("private Markdown logs folder");
        for (provider, fresh) in [
            (Provider::Codex, false),
            (Provider::Codex, true),
            (Provider::Claude, false),
            (Provider::Claude, true),
        ] {
            let command = launch_command(
                Path::new("ai-agent-game-client"),
                workspace,
                provider,
                fresh,
                Some(metrics),
                Some(logs),
            );
            let args: Vec<_> = command
                .get_args()
                .map(|value| value.to_string_lossy().into_owned())
                .collect();
            assert_eq!(
                args.iter()
                    .filter(|arg| arg.as_str() == "--approve-game-tools")
                    .count(),
                usize::from(provider == Provider::Codex)
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--provider", provider.name()])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--workspace", "prepared game workspace"])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--metrics-dir", "private metrics folder"])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--logs-dir", "private Markdown logs folder"])
            );
            assert_eq!(args.contains(&"--new-session".into()), fresh);
            assert!(!args.iter().any(|arg| matches!(
                arg.as_str(),
                "--dangerously-bypass-approvals-and-sandbox" | "--full-auto" | "--yolo"
            )));
        }
        assert!(
            START_PERMISSION_NOTICE.contains("act in this world")
                && START_PERMISSION_NOTICE.contains("private Notebook")
        );
    }
    #[test]
    fn game_tool_cancellation_is_a_closed_actionable_native_diagnostic() {
        let status: Status = serde_json::from_str(
            r#"{"provider":"codex","state":"error","last_failure_code":"game_tool_cancelled"}"#,
        )
        .unwrap();
        assert_eq!(
            status.last_failure_code,
            Some(FailureCode::GameToolCancelled)
        );
        assert!(status.line().contains("Game tool call was cancelled."));
        assert!(status.line().contains("CLI approval preferences"));
        let unknown: Status =
            serde_json::from_str(r#"{"last_failure_code":"PRIVATE_NOTEBOOK_TOOL_TOKEN_CANARY"}"#)
                .unwrap();
        assert_eq!(unknown.last_failure_code, Some(FailureCode::Unknown));
        assert!(!format!("{unknown:?}").contains("PRIVATE_NOTEBOOK_TOOL_TOKEN_CANARY"));
        assert!(
            !unknown
                .line()
                .contains("PRIVATE_NOTEBOOK_TOOL_TOKEN_CANARY")
        );
    }

    #[test]
    fn native_status_distinguishes_server_session_and_confirmed_actor_control() {
        let old: Status = serde_json::from_str(
            r#"{"provider":"codex","state":"waiting","connected":true,"session_ready":true}"#,
        )
        .unwrap();
        assert!(!old.actor_controlled);
        let line = old.line();
        assert!(line.contains("Server: connected"));
        assert!(line.contains("AI session: established"));
        assert!(line.contains("Actor control: not confirmed"));
        let active = Status {
            actor_controlled: true,
            ..old
        };
        assert!(active.line().contains("Actor control: confirmed"));
        for (code, expected) in [
            ("game_access_revoked", FailureCode::GameAccessRevoked),
            ("game_tools_unavailable", FailureCode::GameToolsUnavailable),
            ("game_permission_denied", FailureCode::GamePermissionDenied),
        ] {
            let status: Status = serde_json::from_value(
                serde_json::json!({"state":"error","last_failure_code":code}),
            )
            .unwrap();
            assert_eq!(status.last_failure_code, Some(expected));
            assert!(status.line().contains("Last provider failure:"));
        }
        let unknown: Status = serde_json::from_str(
            r#"{"state":"error","last_connection_fault":"PRIVATE_TOKEN_FAULT"}"#,
        )
        .unwrap();
        assert_eq!(
            unknown.last_connection_fault,
            Some(ConnectionFault::Unknown)
        );
        assert!(!format!("{unknown:?}").contains("PRIVATE_"));
        assert!(!unknown.line().contains("PRIVATE_"));
    }
    #[test]
    fn legacy_status_remains_readable_without_a_failure_field() {
        let status: Status = serde_json::from_str(
            r#"{"provider":"codex","state":"stopped","message":"Stopped","turns_completed":1}"#,
        )
        .unwrap();
        assert_eq!(
            status.line(),
            "codex: stopped | 1 completed turn(s)\nStopped"
        );
        assert!(status.last_failure_code.is_none());
    }

    #[test]
    fn absent_legacy_phases_keep_the_original_display_in_every_process_state() {
        for state in [
            "starting",
            "connecting",
            "running",
            "waiting",
            "stopped",
            "error",
        ] {
            let status: Status = serde_json::from_value(serde_json::json!({
                "provider":"codex", "state":state, "message":"Legacy status", "turns_completed":1
            }))
            .unwrap();
            assert!(!status.phase_details_reported);
            assert_eq!(
                status.line(),
                format!("codex: {state} | 1 completed turn(s)\nLegacy status")
            );
        }
    }

    #[test]
    fn explicitly_false_phases_still_display_the_new_producers_report() {
        for state in ["starting", "stopped", "error"] {
            let status: Status = serde_json::from_value(serde_json::json!({
                "provider":"codex", "state":state, "message":"New status", "turns_completed":1,
                "connected":false, "session_ready":false, "actor_controlled":false
            }))
            .unwrap();
            assert!(status.phase_details_reported);
            assert_eq!(
                status.line(),
                format!(
                    "codex: {state} | 1 completed turn(s)\nServer: disconnected | AI session: not established | Actor control: not confirmed\nNew status"
                )
            );
        }
    }

    #[test]
    fn failure_display_is_actionable_and_does_not_echo_unknown_provider_data() {
        let mut status = Status {
            provider: "codex".into(),
            state: "stopped".into(),
            last_failure_code: Some(FailureCode::McpStartup),
            ..Default::default()
        };
        assert!(
            status
                .line()
                .contains("npcctl is installed beside the client")
        );
        status = serde_json::from_str(
            r#"{"state":"stopped","last_failure_code":"PRIVATE_PROVIDER_TOKEN_MARKER"}"#,
        )
        .unwrap();
        assert!(!status.line().contains("PRIVATE_PROVIDER_TOKEN_MARKER"));
        assert!(!format!("{status:?}").contains("PRIVATE_PROVIDER_TOKEN_MARKER"));
        assert_eq!(status.last_failure_code, Some(FailureCode::Unknown));
        assert!(status.line().contains("Check its CLI setup"));
    }
}
