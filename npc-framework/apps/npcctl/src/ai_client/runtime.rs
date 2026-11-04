use super::{
    Provider,
    diagnostics::{ConnectionFault, FailureCode},
    metrics::{Metrics, TurnOutcome},
    provider::{self, Turn},
    storage::{self, Lock, State, Status},
};
use clap::Args;
use npc_framework::agent_io::{
    http::{ClientError, RpcClient},
    registration::new_secret,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use testbed_content::explorer::{
    home::{self, AgentDir},
    host::{self, RunFile},
};

#[derive(Args)]
pub struct RunArgs {
    #[arg(long)]
    workspace: PathBuf,
    #[arg(long, value_enum)]
    provider: Provider,
    /// Absolute native CLI executable. Provider authentication remains provider-owned.
    #[arg(long)]
    program: Option<PathBuf>,
    /// Allow the 16 registered game tools for this owned Codex process only.
    #[arg(long)]
    approve_game_tools: bool,
    /// Explicitly archive the previous private session and start a new AI conversation.
    #[arg(long)]
    new_session: bool,
    /// Folder for atomic per-session JSON metrics (else NPC_AI_METRICS_DIR, then workspace/.ai-client/metrics).
    #[arg(long)]
    metrics_dir: Option<PathBuf>,
    /// Folder for private per-run Markdown transcripts (else NPC_AI_LOGS_DIR, then workspace/.ai-client/logs).
    #[arg(long)]
    logs_dir: Option<PathBuf>,
    /// Stop after N successful turns (0: continue until stopped).
    #[arg(long, default_value_t = 0)]
    max_turns: u64,
    /// Stop after this duration (0: continue until stopped).
    #[arg(long, default_value_t = 0)]
    max_runtime_seconds: u64,
    #[arg(long, default_value_t=1000, value_parser=clap::value_parser!(u64).range(50..=5000))]
    poll_millis: u64,
    /// Upper bound per provider turn, also bounded by the world's policy.
    #[arg(long, default_value_t=900, value_parser=clap::value_parser!(u64).range(1..=3600))]
    turn_timeout_seconds: u64,
}

#[derive(Deserialize)]
struct Policy {
    #[serde(default = "yes")]
    events_enabled: bool,
    idle_seconds: u64,
    cooldown_seconds: u64,
    max_nudges_per_hour: u32,
    runner_timeout_seconds: u64,
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct Pulse {
    activity: [u64; 2],
    #[serde(default)]
    position: Option<[i32; 2]>,
    #[serde(default)]
    busy: Option<String>,
    #[serde(default)]
    blocked: Option<String>,
}
#[derive(Deserialize)]
struct Poll {
    protocol: u32,
    unit: u32,
    epoch: u64,
    last_seq: u64,
    #[serde(default)]
    acknowledged_seq: u64,
    #[serde(default)]
    reset: bool,
    attached: bool,
    pulse: Pulse,
    policy: Policy,
    #[serde(default)]
    notifications: Vec<Value>,
}

struct Connection {
    client: RpcClient,
    identity: (u32, u16, String),
    owner: String,
    controller: String,
}
const CONTROL_BUDGET: Duration = Duration::from_secs(2);
const CONTROL_RESPONSE_LIMIT: usize = 64 * 1024;
fn connection_fault(error: ClientError) -> ConnectionFault {
    let ClientError::Rpc(error) = error else {
        return ConnectionFault::Unavailable;
    };
    match error.data.as_ref().and_then(|data| data["reason"].as_str()) {
        Some("client_auth_required") => ConnectionFault::Authentication,
        Some("client_owned" | "legacy_running") => ConnectionFault::OwnershipBusy,
        Some("client_capacity") => ConnectionFault::CapacityBusy,
        Some("turn_blocked") => ConnectionFault::GameBusy,
        Some("client_lease_lost") => ConnectionFault::LeaseLost,
        Some("actor_changed" | "actor_owned" | "unknown_controller") => {
            ConnectionFault::AssignmentChanged
        }
        Some("actor_missing") => ConnectionFault::ActorUnavailable,
        Some("client_revoked") => ConnectionFault::Revoked,
        Some("client_protocol_mismatch" | "unknown_client_method" | "invalid_client_request") => {
            ConnectionFault::Protocol
        }
        _ if error.code == -23502 => ConnectionFault::Unavailable,
        _ => ConnectionFault::Protocol,
    }
}
impl Connection {
    fn open(
        dir: &AgentDir,
        controller: &str,
        token: &str,
        owner: &str,
    ) -> Result<(Self, u32), ConnectionFault> {
        let run = RunFile::read_live(dir).ok_or(ConnectionFault::Unavailable)?;
        let mut client = RpcClient::new(&format!("http://127.0.0.1:{}", run.port))
            .map_err(|_| ConnectionFault::Protocol)?;
        client.timeout = CONTROL_BUDGET;
        let claim = client
            .call_with_budget(
                "client/claim",
                json!({"controller":controller,"owner":owner,"token":token}),
                CONTROL_BUDGET,
                CONTROL_RESPONSE_LIMIT,
            )
            .map_err(connection_fault)?;
        let unit = claim["unit"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or(ConnectionFault::Protocol)?;
        if claim["protocol"] != 1 || claim["claimed"] != true {
            return Err(ConnectionFault::Protocol);
        }
        Ok((
            Self {
                client,
                identity: (run.pid, run.port, run.started),
                owner: owner.into(),
                controller: controller.into(),
            },
            unit,
        ))
    }
    fn poll(&self, epoch: Option<u64>, cursor: u64) -> Result<Poll, ConnectionFault> {
        let value=self.client.call_with_budget("client/poll",json!({"controller":self.controller,"owner":self.owner,"epoch":epoch,"since_seq":cursor}),CONTROL_BUDGET,CONTROL_RESPONSE_LIMIT).map_err(connection_fault)?;
        serde_json::from_value(value).map_err(|_| ConnectionFault::Protocol)
    }
    fn unchanged(&self, dir: &AgentDir) -> bool {
        RunFile::read_live(dir).is_some_and(|run| (run.pid, run.port, run.started) == self.identity)
    }
    fn turn_slot(&self, running: bool) -> Result<bool, ConnectionFault> {
        let value = self
            .client
            .call_with_budget(
                "client/turn",
                json!({"controller":self.controller,"owner":self.owner,"running":running}),
                CONTROL_BUDGET,
                CONTROL_RESPONSE_LIMIT,
            )
            .map_err(connection_fault)?;
        match (value["running"].as_bool(), value["acquired"].as_bool()) {
            (Some(false), Some(false)) => Ok(!running),
            (Some(true), Some(true)) if running => Ok(true),
            _ => Err(ConnectionFault::Protocol),
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.client.call_with_budget(
            "client/release",
            json!({"controller":self.controller,"owner":self.owner}),
            CONTROL_BUDGET,
            CONTROL_RESPONSE_LIMIT,
        );
    }
}

const CONTINUE: &str = "Continue playing your assigned character in the existing game. Your previous AI turn has finished. This turn opens a fresh npc MCP connection even though this AI conversation continues. Read AGENTS.md and privately register once on this fresh connection using its existing instructions, call enter_world, and take_control if needed. Then observe current state and needs before choosing useful actions toward your goal. If any response reports revoked:true or access denied, stop and report the blocker; do not reconnect, register again, or replay a mutation to recover revoked access. Historical notifications below are signals, not current truth. Respect ownership, permanent death and game rules. Use only the game tools and your prepared workspace. Never reveal credentials, registration session keys or this conversation's identity. If blocked, describe the blocker. Never modify game/client configuration to bypass a blocker.";

fn running_message(session_ready: bool, actor_controlled: bool) -> &'static str {
    if actor_controlled {
        "AI has actor control; using the registered game tools"
    } else if session_ready {
        "AI session established; waiting for game registration and actor control"
    } else {
        "AI turn started; waiting for its session and game registration"
    }
}

fn notification(value: &Value) -> Option<Value> {
    let summary = value["summary"].as_str()?;
    if !matches!(
        summary,
        "action_completed"
            | "action_failed"
            | "action_cancelled"
            | "action_rejected"
            | "interaction_completed"
            | "interaction_failed"
            | "need_low"
            | "need_urgent"
            | "need_recovered"
            | "craft_completed"
            | "craft_failed"
            | "order_completed"
            | "order_failed"
            | "path_failed"
            | "day_started"
            | "night_started"
            | "work_completed"
            | "inventory_changed"
            | "needs_restored"
            | "trade_completed"
            | "rest_started"
            | "life_changed"
            | "incoming_speech"
    ) {
        return None;
    }
    Some(
        json!({"summary":summary,"seq":value["seq"].as_u64()?,"tick":value["tick"].as_u64()?,"count":value["count"].as_u64()?.min(64)}),
    )
}

fn refresh_notifications(
    pending: &mut VecDeque<Value>,
    in_flight: &mut VecDeque<Value>,
    enabled: bool,
    acknowledged: u64,
    notifications: &[Value],
) {
    if !enabled {
        pending.clear();
        in_flight.clear();
        return;
    }
    pending.retain(|event| event["seq"].as_u64().is_some_and(|seq| seq > acknowledged));
    in_flight.retain(|event| event["seq"].as_u64().is_some_and(|seq| seq > acknowledged));
    for event in notifications.iter().take(16).filter_map(notification) {
        if event["seq"].as_u64().is_some_and(|seq| seq > acknowledged)
            && !pending.iter().any(|old| old["seq"] == event["seq"])
        {
            if pending.len() == 16 {
                pending.pop_front();
            }
            pending.push_back(event);
        }
    }
}

fn initial_prompt(dir: &AgentDir) -> Result<String, String> {
    let current = dir.dir.join("prompts/current.md");
    let path = if current.is_file() {
        current
    } else {
        host::latest_prompt(dir).ok_or(
            "Generated game prompt is missing; reopen or resume the game to prepare the workspace",
        )?
    };
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(|_| "cannot read generated game prompt")?
        .take(128 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| "cannot read generated game prompt")?;
    if text.len() > 128 * 1024 {
        return Err("generated game prompt exceeds 128 KiB".into());
    }
    let text = host::prompt_text(&text);
    Ok(format!(
        "Read AGENTS.md in this prepared game workspace, connect to the registered npc MCP server, and follow this game briefing.\n\n{text}"
    ))
}

fn archive_state(path: &Path) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let archive = path.with_file_name(format!("state.previous.{stamp}.json"));
    fs::rename(path, archive).map_err(|_| "cannot preserve the previous private AI session".into())
}

pub fn run(args: RunArgs) -> Result<Value, String> {
    if args.approve_game_tools && args.provider != Provider::Codex {
        return Err("--approve-game-tools applies only to Codex".into());
    }
    let workspace = args
        .workspace
        .canonicalize()
        .map_err(|_| "workspace must be an existing prepared game directory")?;
    let dir = AgentDir {
        dir: workspace.clone(),
    };
    let (meta, _) = home::credential_snapshot(&dir)
        .map_err(|_| "workspace has no valid agent identity; start its explorer game first")?;
    let private = workspace.join(storage::DIR);
    if fs::symlink_metadata(&private).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("private client directory must not be a symbolic link".into());
    }
    let _lock = Lock::acquire(&private)?;
    let _ = fs::remove_file(private.join("stop"));
    let mut status = Status {
        log: None,
        version: 1,
        pid: std::process::id(),
        provider: args.provider,
        state: "starting".into(),
        message: "Preparing AI client".into(),
        turns_completed: 0,
        failures: 0,
        last_failure_code: None,
        session_ready: false,
        connected: false,
        actor_controlled: false,
        last_connection_fault: None,
        updated_unix_seconds: storage::now(),
    };
    status.set(&private, "starting", "Preparing AI client")?;
    let log = match super::session_log::SessionLog::open(
        &private,
        args.logs_dir.as_deref(),
        args.provider,
        &meta.token,
    ) {
        Ok(log) => log,
        Err(error) => {
            let _ = status.set(&private, "error", &error);
            return Err(error);
        }
    };
    status.log = Some(log.clone());
    // A usable durable transcript is required even before subscription/login or
    // metadata probes. Opening the asynchronous writer alone is not readiness.
    let result = log.flush(Duration::from_secs(2)).and_then(|()| {
        run_owned(
            &args,
            &dir,
            &meta.agent_id,
            &meta.token,
            (&meta.agent, &meta.world),
            &private,
            &mut status,
        )
    });
    if let Err(error) = &result {
        status.connected = false;
        status.actor_controlled = false;
        let _ = status.set(&private, "error", error);
    }
    let logged = log.entry(
        "Client finished",
        &json!({
            "ok":result.is_ok(), "state":status.state, "message":status.message,
            "turns_completed":status.turns_completed, "failures":status.failures,
            "last_failure_code":status.last_failure_code,
        })
        .to_string(),
    );
    let closed = log.shutdown(Duration::from_secs(1));
    let logging = logged.and(closed);
    if result.is_ok()
        && let Err(error) = &logging
    {
        status.connected = false;
        status.actor_controlled = false;
        let _ = status.set(&private, "error", error);
    }
    result.and_then(|value| logging.map(|()| value))
}

fn run_owned(
    args: &RunArgs,
    dir: &AgentDir,
    controller: &str,
    token: &str,
    (agent_label, world_label): (&str, &str),
    private: &Path,
    status: &mut Status,
) -> Result<Value, String> {
    let program = provider::resolve(args.provider, args.program.as_deref())?;
    let readiness = provider::check(args.provider, Some(&program))?;
    if readiness["ready"] != true {
        status.last_failure_code = Some(FailureCode::Authentication);
        return Err(match args.provider {Provider::Codex=>"Sign in with `codex login` using your ChatGPT subscription, then restart the AI client",Provider::Claude=>"Sign in with `claude auth login` using your Claude subscription, then restart the AI client"}.into());
    }
    let usage_scope = provider::usage_scope(&readiness, args.provider);
    let cli_version = readiness["cli_version"].as_str();
    if let Some(log) = &status.log {
        log.entry("CLI readiness", &readiness.to_string())?;
    }
    let selection = if args.provider == Provider::Codex {
        status.set(
            private,
            "starting",
            "Checking the CLI's available gameplay model and effort",
        )?;
        match super::codex_model::prepare(&program, &dir.dir, args.approve_game_tools) {
            Ok(selection) => Some(selection),
            Err(error) => {
                status.last_failure_code =
                    Some(if error == super::codex_model::GAME_TOOL_POLICY_ERROR {
                        FailureCode::ProviderConfig
                    } else {
                        FailureCode::ModelUnavailable
                    });
                return Err(error);
            }
        }
    } else {
        None
    };
    let state_path = private.join("state.json");
    if args.new_session && state_path.exists() {
        archive_state(&state_path)?;
    }
    let mut state = if state_path.exists() {
        let old: State = storage::read_json(&state_path)?;
        if old.version != 1 {
            return Err(
                "unsupported private client state; preserve it and use a compatible client".into(),
            );
        }
        if old.provider == args.provider && old.controller == controller {
            old
        } else {
            archive_state(&state_path)?;
            State {
                version: 1,
                provider: args.provider,
                controller: controller.into(),
                unit: 0,
                session: None,
                metrics_id: None,
            }
        }
    } else {
        State {
            version: 1,
            provider: args.provider,
            controller: controller.into(),
            unit: 0,
            session: None,
            metrics_id: None,
        }
    };
    if state
        .session
        .as_deref()
        .is_some_and(|id| !provider::uuid(id))
    {
        return Err("private client session identity is invalid".into());
    }
    let metrics_directory = Metrics::directory(private, args.metrics_dir.as_deref())?;
    let mut metrics = Metrics::open(
        private,
        &metrics_directory,
        args.provider,
        state.metrics_id.as_deref(),
        state.session.is_none(),
    )?;
    metrics.set_labels(Some(agent_label), Some(world_label))?;
    metrics.set_selection(selection.as_ref())?;
    metrics.set_provider_context(cli_version, usage_scope)?;
    state.metrics_id = Some(metrics.id().to_owned());
    storage::write_json(&state_path, &state)?;
    if let Some(log) = &status.log {
        log.link_metrics(&metrics_directory, metrics.id())?;
    }
    let owner = new_secret().map_err(|_| "cannot create private client owner identity")?;
    let stopping = Arc::new(AtomicBool::new(false));
    let signal = stopping.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .map_err(|_| "cannot install AI client stop handler")?;
    let start = Instant::now();
    let mut connection: Option<Connection> = None;
    let mut turn: Option<Turn> = None;
    let mut epoch = None;
    let mut cursor = 0;
    let mut pending: VecDeque<Value> = VecDeque::new();
    let mut in_flight: VecDeque<Value> = VecDeque::new();
    let mut last_activity = None;
    let mut idle_since = Instant::now();
    let mut last_turn = None;
    let mut attempts: VecDeque<Instant> = VecDeque::new();
    let mut retry_at = Instant::now();
    let mut bootstrap = true;
    let mut consecutive_failures = 0u32;
    let mut assignment_seen = false;
    status.session_ready = state.session.is_some();
    let result = (|| -> Result<Value, String> {
        loop {
            if stopping.load(Ordering::Relaxed)
                || private.join("stop").exists()
                || (args.max_runtime_seconds > 0
                    && start.elapsed().as_secs() >= args.max_runtime_seconds)
                || (args.max_turns > 0 && status.turns_completed >= args.max_turns)
            {
                status.set(
                    private,
                    "stopping",
                    "Stopping owned AI turn; the game continues running",
                )?;
                stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path)?;
                drop(connection.take());
                status.connected = false;
                status.actor_controlled = false;
                status.set(
                    private,
                    "stopped",
                    "AI client stopped; the game continues running",
                )?;
                let _ = fs::remove_file(private.join("stop"));
                return serde_json::to_value(&status)
                    .map_err(|_| "cannot encode final AI client status".into());
            }
            if connection.as_ref().is_some_and(|c| !c.unchanged(dir)) {
                stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path)?;
                drop(connection.take());
                epoch = None;
                pending.clear();
                in_flight.clear();
                bootstrap = true;
                status.connected = false;
                status.actor_controlled = false;
                status.last_connection_fault = Some(ConnectionFault::Unavailable);
                status.set(
                    private,
                    "connecting",
                    "Game stopped or changed; waiting for its connection",
                )?;
            }
            if connection.is_none() {
                match Connection::open(dir, controller, token, &owner) {
                    Ok((connected, unit)) => {
                        if state.unit != 0 && state.unit != unit {
                            if assignment_seen {
                                status.last_connection_fault =
                                    Some(ConnectionFault::AssignmentChanged);
                                return Err(ConnectionFault::AssignmentChanged.message().into());
                            }
                            archive_state(&state_path)?;
                            state.session = None;
                            metrics = Metrics::open(
                                private,
                                &metrics_directory,
                                args.provider,
                                None,
                                true,
                            )?;
                            metrics.set_labels(Some(agent_label), Some(world_label))?;
                            metrics.set_selection(selection.as_ref())?;
                            metrics.set_provider_context(cli_version, usage_scope)?;
                            state.metrics_id = Some(metrics.id().to_owned());
                            if let Some(log) = &status.log {
                                log.link_metrics(&metrics_directory, metrics.id())?;
                            }
                            status.session_ready = false;
                        }
                        state.unit = unit;
                        assignment_seen = true;
                        storage::write_json(&state_path, &state)?;
                        connection = Some(connected);
                        status.connected = true;
                        status.actor_controlled = false;
                    }
                    Err(fault) => {
                        status.connected = false;
                        status.actor_controlled = false;
                        status.last_connection_fault = Some(fault);
                        if !fault.retryable() {
                            return Err(fault.message().into());
                        }
                        status.set(private, "connecting", fault.message())?;
                        thread::sleep(Duration::from_millis(args.poll_millis.max(500)));
                        continue;
                    }
                }
            }
            let polled = connection
                .as_ref()
                .expect("connected above")
                .poll(epoch, cursor);
            let poll = match polled {
                Ok(poll) => {
                    status.last_connection_fault = None;
                    poll
                }
                Err(fault) => {
                    status.last_connection_fault = Some(fault);
                    stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path)?;
                    drop(connection.take());
                    pending.clear();
                    in_flight.clear();
                    epoch = None;
                    bootstrap = true;
                    status.connected = false;
                    status.actor_controlled = false;
                    if !fault.retryable() {
                        return Err(fault.message().into());
                    }
                    status.set(private, "connecting", fault.message())?;
                    thread::sleep(Duration::from_millis(args.poll_millis.max(500)));
                    continue;
                }
            };
            if poll.protocol != 1 || poll.unit != state.unit {
                let fault = if poll.protocol != 1 {
                    ConnectionFault::Protocol
                } else {
                    ConnectionFault::AssignmentChanged
                };
                status.last_connection_fault = Some(fault);
                return Err(fault.message().into());
            }
            if epoch.is_some_and(|old| old != poll.epoch) {
                stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path)?;
                if let Some(connection) = &connection {
                    let _ = connection.turn_slot(false);
                }
                pending.clear();
                in_flight.clear();
                bootstrap = true;
            }
            if poll.reset {
                pending.clear();
                in_flight.clear();
            }
            if let Some(log) = &status.log {
                let notifications: Vec<_> = poll
                    .notifications
                    .iter()
                    .filter(|event| {
                        event["seq"]
                            .as_u64()
                            .is_some_and(|seq| poll.reset || seq > cursor)
                    })
                    .filter_map(notification)
                    .collect();
                if !notifications.is_empty() {
                    log.entry("Game notifications", &json!(notifications).to_string())?;
                }
            }
            epoch = Some(poll.epoch);
            cursor = poll.last_seq;
            refresh_notifications(
                &mut pending,
                &mut in_flight,
                poll.policy.events_enabled,
                poll.acknowledged_seq,
                &poll.notifications,
            );
            let activity = (poll.pulse.activity, poll.pulse.position);
            if last_activity != Some(activity) || poll.pulse.busy.is_some() {
                idle_since = Instant::now();
                last_activity = Some(activity);
            }
            let blocked = poll.pulse.blocked.as_deref();
            let fatal = blocked.is_some_and(|reason| {
                !matches!(
                    reason,
                    "game paused" | "waiting for agent registration" | "waiting for agent control"
                )
            });
            let actor_controlled = poll.attached && !fatal;
            let actor_phase_changed = status.actor_controlled != actor_controlled;
            status.actor_controlled = actor_controlled;
            if fatal {
                status.last_connection_fault =
                    Some(if blocked == Some("registration revoked or disconnected") {
                        ConnectionFault::Revoked
                    } else {
                        ConnectionFault::ActorUnavailable
                    });
                return Err(
                "Agent control was revoked, the actor changed or its life ended; AI client stopped"
                    .into(),
            );
            }
            if turn.is_some()
                && blocked.is_some_and(|reason| {
                    reason != "waiting for agent registration"
                        && reason != "waiting for agent control"
                        && reason != "game paused"
                })
            {
                return Err("Agent ownership became unavailable; AI client stopped".into());
            }
            if let Some(active) = &mut turn {
                let old_session = state.session.clone();
                let completed = active.poll(&mut state.session);
                let failure = active.failure;
                if state.session != old_session {
                    // The exact owned conversation must survive an export-only
                    // failure even while the provider process is still active.
                    storage::write_json(&state_path, &state)?;
                    status.session_ready = true;
                }
                metrics.record(&active.usage)?;
                let completed = completed?;
                if state.session != old_session || actor_phase_changed {
                    status.set(
                        private,
                        "running",
                        running_message(status.session_ready, status.actor_controlled),
                    )?;
                }
                let timeout = args
                    .turn_timeout_seconds
                    .min(poll.policy.runner_timeout_seconds.clamp(1, 3600));
                if completed.is_none() && active.started.elapsed().as_secs() >= timeout {
                    status.last_failure_code = Some(FailureCode::Unknown);
                    stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path)?;
                    return Err("AI turn timed out and was stopped; check provider login and game tools before restarting".into());
                }
                if let Some(success) = completed {
                    metrics.finish(if success {
                        TurnOutcome::Completed
                    } else {
                        TurnOutcome::Failed
                    })?;
                    drop(turn.take());
                    if let Some(connection) = &connection {
                        let _ = connection.turn_slot(false);
                    }
                    last_turn = Some(Instant::now());
                    if success {
                        in_flight.clear();
                        status.turns_completed += 1;
                        consecutive_failures = 0;
                        bootstrap = false;
                        status.set(
                            private,
                            "waiting",
                            "AI turn finished; waiting for game events or inactivity",
                        )?;
                    } else {
                        let code = failure.unwrap_or(FailureCode::Unknown);
                        status.last_failure_code = Some(code);
                        while let Some(event) = in_flight.pop_back() {
                            if pending.len() == 16 {
                                pending.pop_back();
                            }
                            pending.push_front(event);
                        }
                        consecutive_failures += 1;
                        status.failures = status.failures.saturating_add(1);
                        if code.requires_attention() {
                            return Err(code.message().into());
                        }
                        if consecutive_failures >= 3 {
                            return Err(code.message().into());
                        }
                        retry_at = Instant::now()
                            + Duration::from_secs(
                                5u64.saturating_mul(1u64 << consecutive_failures.min(5)),
                            );
                        status.set(private, "waiting", code.message())?;
                    }
                }
            } else {
                while attempts
                    .front()
                    .is_some_and(|at| at.elapsed().as_secs() >= 3600)
                {
                    attempts.pop_front();
                }
                let cooldown = last_turn.is_none_or(|at: Instant| {
                    at.elapsed().as_secs() >= poll.policy.cooldown_seconds.clamp(1, 86400)
                });
                let rate = attempts.len() < (poll.policy.max_nudges_per_hour.clamp(1, 60) as usize);
                let due = bootstrap
                    || (poll.policy.events_enabled && !pending.is_empty())
                    || idle_since.elapsed().as_secs() >= poll.policy.idle_seconds.clamp(1, 86400);
                let eligible = poll.pulse.busy.is_none()
                    && (blocked.is_none()
                        || (bootstrap
                            && matches!(
                                blocked,
                                Some(
                                    "waiting for agent registration" | "waiting for agent control"
                                )
                            )))
                    && (poll.attached || bootstrap);
                if due && eligible && cooldown && rate && Instant::now() >= retry_at {
                    let slot = connection
                        .as_ref()
                        .expect("connected above")
                        .turn_slot(true);
                    if let Err(fault) = slot {
                        status.last_connection_fault = Some(fault);
                        status.actor_controlled = false;
                        status.connected = false;
                        drop(connection.take());
                        epoch = None;
                        bootstrap = true;
                        if !fault.retryable() {
                            return Err(fault.message().into());
                        }
                        status.set(private, "connecting", fault.message())?;
                        thread::sleep(Duration::from_millis(args.poll_millis));
                        continue;
                    }
                    if slot == Ok(false) {
                        status.set(
                            private,
                            "waiting",
                            "Waiting for an available AI turn slot or game action",
                        )?;
                        thread::sleep(Duration::from_millis(args.poll_millis));
                        continue;
                    }
                    let mut prompt = if bootstrap {
                        initial_prompt(dir)?
                    } else {
                        CONTINUE.into()
                    };
                    if !pending.is_empty() && poll.policy.events_enabled {
                        prompt.push_str("\n\nHistorical game notifications:\n");
                        for event in &pending {
                            prompt.push_str(&event.to_string());
                            prompt.push('\n');
                        }
                    }
                    // A failed initial process without an established provider session
                    // cannot be merged with the next newly created conversation.
                    if state.session.is_none() && metrics.has_started() {
                        metrics =
                            Metrics::open(private, &metrics_directory, args.provider, None, true)?;
                        metrics.set_labels(Some(agent_label), Some(world_label))?;
                        metrics.set_selection(selection.as_ref())?;
                        metrics.set_provider_context(cli_version, usage_scope)?;
                        state.metrics_id = Some(metrics.id().to_owned());
                        storage::write_json(&state_path, &state)?;
                        if let Some(log) = &status.log {
                            log.link_metrics(&metrics_directory, metrics.id())?;
                        }
                    }
                    if let Some(log) = &status.log {
                        log.entry(
                            "Turn prompt",
                            &json!({
                                "metrics_id":metrics.id(), "resuming":state.session.is_some(),
                                "selection_intent":selection, "prompt":prompt,
                            })
                            .to_string(),
                        )?;
                        log.flush(Duration::from_secs(2))?;
                    }
                    metrics.begin()?;
                    let launched = Turn::start(
                        &program,
                        args.provider,
                        &dir.dir,
                        state.session.as_deref(),
                        &prompt,
                        provider::TurnSettings {
                            usage_scope,
                            selection: selection.as_ref(),
                            approve_game_tools: args.approve_game_tools,
                            log: status.log.as_ref(),
                            cli_version,
                        },
                    );
                    attempts.push_back(Instant::now());
                    match launched {
                        Ok(active) => {
                            turn = Some(active);
                            in_flight = std::mem::take(&mut pending);
                            status.set(
                                private,
                                "running",
                                running_message(status.session_ready, status.actor_controlled),
                            )?;
                        }
                        Err(error) => {
                            metrics.finish(TurnOutcome::Failed)?;
                            let _ = connection
                                .as_ref()
                                .expect("connected above")
                                .turn_slot(false);
                            return Err(error);
                        }
                    }
                } else {
                    let message = if blocked == Some("game paused") {
                        "Game paused; resume the game to let the AI play"
                    } else if poll.pulse.busy.is_some() {
                        "Actor is working; waiting for the game action to finish"
                    } else if !rate {
                        "Hourly AI turn limit reached; waiting for the next allowance"
                    } else if !cooldown {
                        "AI turn finished; respecting the game's continuation cooldown"
                    } else {
                        "Waiting for game events or actor inactivity"
                    };
                    status.set(private, "waiting", message)?;
                }
            }
            thread::sleep(Duration::from_millis(args.poll_millis));
        }
    })();
    // Every Result exit, including revocation, protocol mismatch and storage
    // errors, drains already queued provider accounting before cleanup. This
    // does not send another prompt, reconnect an actor or replay a mutation.
    let cleanup = stop_tracked_turn(&mut turn, &mut metrics, status, &mut state, &state_path);
    drop(connection.take());
    // Cleanup has already attempted every independent write. Preserve the
    // original failure while still returning cleanup errors on a normal stop.
    result.and_then(|value| cleanup.map(|()| value))
}

fn stop_tracked_turn(
    turn: &mut Option<Turn>,
    metrics: &mut Metrics,
    status: &mut Status,
    state: &mut State,
    state_path: &Path,
) -> Result<(), String> {
    if let Some(active) = turn.as_mut() {
        let completion = active.poll(&mut state.session);
        let completion_error = completion.as_ref().err().cloned();
        let failure = active.failure;
        // The provider validates every ID before assigning it and rejects all
        // later progress after an identity conflict. Keep the first accepted
        // identity even if this cleanup poll first sees A and then rejects B.
        let outcome =
            if failure.is_some_and(FailureCode::requires_attention) || completion_error.is_some() {
                TurnOutcome::Failed
            } else {
                match completion {
                    Ok(Some(true)) => TurnOutcome::Completed,
                    Ok(Some(false)) => TurnOutcome::Failed,
                    _ => TurnOutcome::Interrupted,
                }
            };
        let usage = active.usage.clone();
        // Cleanup cannot depend on disk availability. Stop the owned process
        // before a failed state/metrics write can release its host lease.
        drop(turn.take());
        persist_stopped_accounting(metrics, status, state, state_path, &usage, outcome, failure)?;
        if let Some(code) = failure.filter(|code| code.requires_attention()) {
            return Err(code.message().into());
        }
        if let Some(error) = completion_error {
            return Err(error);
        }
    }
    // Reconnect, changed-world and epoch cleanup must propagate the same fatal
    // cancellation as ordinary polling. Accounting and owned-process shutdown
    // above finish first; no caller may then dispatch another provider turn.
    if let Some(code) = status
        .last_failure_code
        .filter(|code| code.requires_attention())
    {
        return Err(code.message().into());
    }
    Ok(())
}

fn persist_stopped_accounting(
    metrics: &mut Metrics,
    status: &mut Status,
    state: &State,
    state_path: &Path,
    usage: &super::metrics::UsageSample,
    outcome: TurnOutcome,
    failure: Option<FailureCode>,
) -> Result<(), String> {
    // These are independent destinations: an unavailable custom export
    // must not discard the private identity or skip canonical terminal
    // accounting. Evaluate all writes before choosing the first error.
    let identity_write = storage::write_json(state_path, state);
    let usage_write = metrics.record(usage);
    let outcome_write = metrics.finish(outcome);
    status.session_ready = state.session.is_some();
    match outcome {
        TurnOutcome::Completed => status.turns_completed = status.turns_completed.saturating_add(1),
        TurnOutcome::Failed => {
            status.failures = status.failures.saturating_add(1);
            status.last_failure_code = Some(failure.unwrap_or(FailureCode::Unknown));
        }
        TurnOutcome::Interrupted => {}
    }
    identity_write.and(usage_write).and(outcome_write)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_first_poll_keeps_fresh_verified_identity_and_usage_before_foreign_rejection() {
        for fatal in [None, Some(FailureCode::GameAccessRevoked)] {
            let base = std::env::temp_dir().canonicalize().unwrap();
            let root = base.join(format!(
                "npc-fresh-identity-cleanup-{}",
                new_secret().unwrap()
            ));
            fs::create_dir(&root).unwrap();
            let private = root.join(".ai-client");
            let export = root.join("exports");
            fs::create_dir(&export).unwrap();
            let mut metrics =
                Metrics::open(&private, &export, Provider::Codex, None, true).unwrap();
            metrics
                .set_provider_context(
                    Some("0.145.0"),
                    Some(super::super::metrics::UsageScope::SessionCumulative),
                )
                .unwrap();
            metrics.begin().unwrap();
            let log = super::super::session_log::SessionLog::open(
                &private,
                None,
                Provider::Codex,
                "private-token",
            )
            .unwrap();
            log.flush(Duration::from_secs(2)).unwrap();
            const A: &str = "11111111-1111-4111-8111-111111111111";
            const B: &str = "22222222-2222-4222-8222-222222222222";
            let a = super::super::metrics::UsageSample {
                input_tokens: Some(19),
                output_tokens: Some(7),
                scope: super::super::metrics::UsageScope::SessionCumulative,
                ..Default::default()
            };
            let b = super::super::metrics::UsageSample {
                input_tokens: Some(9_000_001),
                output_tokens: Some(8_000_002),
                scope: super::super::metrics::UsageScope::SessionCumulative,
                ..Default::default()
            };
            let (sender, progress) = std::sync::mpsc::sync_channel(8);
            sender.send(provider::Progress::Session(A.into())).unwrap();
            sender.send(provider::Progress::Usage(a)).unwrap();
            if let Some(code) = fatal {
                sender.send(provider::Progress::GameFailure(code)).unwrap();
            }
            sender.send(provider::Progress::Session(B.into())).unwrap();
            sender.send(provider::Progress::Usage(b.clone())).unwrap();
            sender
                .send(provider::Progress::End(provider::Outcome {
                    session: Some(B.into()),
                    completed: true,
                    usage: b,
                    ..Default::default()
                }))
                .unwrap();
            drop(sender);
            let mut turn = Some(Turn::queued_for_test(log.clone(), progress));
            let mut state = State {
                version: 1,
                provider: Provider::Codex,
                controller: "fixture".into(),
                unit: 1,
                session: None,
                metrics_id: Some(metrics.id().into()),
            };
            let mut status:Status=serde_json::from_value(json!({"version":1,"pid":std::process::id(),"provider":"codex","state":"running","message":"fixture","turns_completed":0,"failures":0,"session_ready":false,"connected":true,"updated_unix_seconds":0})).unwrap();
            let state_path = private.join("state.json");
            assert!(
                stop_tracked_turn(
                    &mut turn,
                    &mut metrics,
                    &mut status,
                    &mut state,
                    &state_path
                )
                .is_err()
            );
            assert!(
                turn.is_none(),
                "identity rejection terminates the owned turn before returning a fatal result"
            );
            let saved: State = storage::read_json(&state_path).unwrap();
            assert_eq!(saved.session.as_deref(), Some(A));
            assert_eq!(state.session.as_deref(), Some(A));
            assert_eq!(
                status.last_failure_code,
                Some(fatal.unwrap_or(FailureCode::Unknown))
            );
            assert_eq!(status.failures, 1);
            assert_eq!(status.turns_completed, 0);
            assert!(status.session_ready);
            let ledger: Value = storage::read_json(
                &private
                    .join("metrics-ledger")
                    .join(format!("{}.json", metrics.id())),
            )
            .unwrap();
            assert_eq!(ledger["turns_started"], 1);
            assert_eq!(ledger["turns_failed"], 1);
            assert_eq!(ledger["turns_interrupted"], 0);
            assert_eq!(ledger["totals"]["input_tokens"], 19);
            assert_eq!(ledger["totals"]["output_tokens"], 7);
            assert!(ledger["active_turn"].is_null());
            assert!(!serde_json::to_string(&ledger).unwrap().contains(B));
            log.shutdown(Duration::from_secs(1)).unwrap();
            drop(log);
            drop(metrics);
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(&base) && resolved != base);
            fs::remove_dir_all(resolved).unwrap();
        }
    }
    #[test]
    fn game_connection_errors_are_closed_and_distinguish_waits_from_operator_failures() {
        use npc_framework::agent_io::api::RpcError;
        for (reason, expected, retryable) in [
            (
                "client_auth_required",
                ConnectionFault::Authentication,
                false,
            ),
            ("actor_changed", ConnectionFault::AssignmentChanged, false),
            (
                "unknown_controller",
                ConnectionFault::AssignmentChanged,
                false,
            ),
            ("client_protocol_mismatch", ConnectionFault::Protocol, false),
            ("client_revoked", ConnectionFault::Revoked, false),
            ("actor_missing", ConnectionFault::ActorUnavailable, false),
            ("client_owned", ConnectionFault::OwnershipBusy, true),
            ("legacy_running", ConnectionFault::OwnershipBusy, true),
            ("client_capacity", ConnectionFault::CapacityBusy, true),
            ("client_lease_lost", ConnectionFault::LeaseLost, true),
            ("turn_blocked", ConnectionFault::GameBusy, true),
            ("PRIVATE_TOKEN", ConnectionFault::Protocol, false),
        ] {
            let mut error = RpcError::invalid("PRIVATE_RPC_MESSAGE");
            error.data = Some(json!({"reason":reason,"token":"PRIVATE_TOKEN"}));
            let fault = connection_fault(ClientError::Rpc(error));
            assert_eq!(fault, expected);
            assert_eq!(fault.retryable(), retryable);
            assert!(!fault.message().contains("PRIVATE_"));
            assert!(!serde_json::to_string(&fault).unwrap().contains("PRIVATE_"));
        }
        assert_eq!(
            connection_fault(ClientError::Transport("PRIVATE_URL".into())),
            ConnectionFault::Unavailable
        );
    }
    #[test]
    fn cleanup_retains_first_verified_session_and_usage_when_markdown_writes_fail() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!("npc-ai-log-cleanup-{}", new_secret().unwrap()));
        fs::create_dir(&root).unwrap();
        let private = root.join(".ai-client");
        let export = root.join("exports");
        fs::create_dir(&export).unwrap();
        let mut metrics = Metrics::open(&private, &export, Provider::Codex, None, true).unwrap();
        metrics.begin().unwrap();
        let log = super::super::session_log::SessionLog::open(
            &private,
            None,
            Provider::Codex,
            "private-token",
        )
        .unwrap();
        let read_only = root.join("read-only-file");
        fs::write(&read_only, "owned fixture").unwrap();
        log.fail_writes_for_test(&read_only);
        const SESSION: &str = "11111111-1111-4111-8111-111111111111";
        let usage = super::super::metrics::UsageSample {
            input_tokens: Some(19),
            output_tokens: Some(7),
            ..Default::default()
        };
        let (sender, progress) = std::sync::mpsc::sync_channel(8);
        sender
            .send(provider::Progress::Session(SESSION.into()))
            .unwrap();
        sender
            .send(provider::Progress::Usage(usage.clone()))
            .unwrap();
        sender
            .send(provider::Progress::End(provider::Outcome {
                session: Some(SESSION.into()),
                completed: true,
                usage,
                ..Default::default()
            }))
            .unwrap();
        drop(sender);
        let mut turn = Some(Turn::queued_for_test(log.clone(), progress));
        let mut state = State {
            version: 1,
            provider: Provider::Codex,
            controller: "fixture".into(),
            unit: 1,
            session: None,
            metrics_id: Some(metrics.id().into()),
        };
        let mut status:Status = serde_json::from_value(json!({"version":1,"pid":std::process::id(),"provider":"codex","state":"running","message":"fixture","turns_completed":0,"failures":0,"session_ready":false,"connected":true,"updated_unix_seconds":0})).unwrap();
        status.log = Some(log.clone());
        let state_path = private.join("state.json");
        assert!(
            stop_tracked_turn(
                &mut turn,
                &mut metrics,
                &mut status,
                &mut state,
                &state_path
            )
            .is_err()
        );
        assert!(
            turn.is_none(),
            "owned process must be stopped on log failure"
        );
        let saved: State = storage::read_json(&state_path).unwrap();
        assert_eq!(
            saved.session.as_deref(),
            Some(SESSION),
            "cleanup must retain the first verified conversation even when logging fails"
        );
        assert!(status.session_ready);
        let canonical: Value = storage::read_json(
            &private
                .join("metrics-ledger")
                .join(format!("{}.json", metrics.id())),
        )
        .unwrap();
        assert_eq!(canonical["turns_failed"], 1);
        assert_eq!(canonical["turns_interrupted"], 0);
        assert_eq!(canonical["totals"]["input_tokens"], 19);
        assert_eq!(canonical["totals"]["output_tokens"], 7);
        drop(status);
        drop(log);
        drop(metrics);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn cancelled_cleanup_is_fatal_after_accounting_even_without_an_active_turn() {
        for code in [
            FailureCode::GameToolCancelled,
            FailureCode::GameAccessRevoked,
            FailureCode::GameToolsUnavailable,
            FailureCode::GamePermissionDenied,
        ] {
            let base = std::env::temp_dir().canonicalize().unwrap();
            let root = base.join(format!("npc-ai-cancel-cleanup-{}", new_secret().unwrap()));
            fs::create_dir(&root).unwrap();
            let private = root.join(".ai-client");
            let export = root.join("exports");
            fs::create_dir(&export).unwrap();
            let mut metrics =
                Metrics::open(&private, &export, Provider::Codex, None, true).unwrap();
            metrics.begin().unwrap();
            let mut state = State {
                version: 1,
                provider: Provider::Codex,
                controller: "fixture-controller".into(),
                unit: 1,
                session: Some("11111111-1111-4111-8111-111111111111".into()),
                metrics_id: Some(metrics.id().to_owned()),
            };
            let mut status: Status = serde_json::from_value(json!({
                "version":1,"pid":std::process::id(),"provider":"codex",
                "state":"running","message":"fixture","turns_completed":0,"failures":0,
                "session_ready":false,"connected":true,"updated_unix_seconds":0
            }))
            .unwrap();
            let state_path = private.join("state.json");
            let usage = super::super::metrics::UsageSample {
                input_tokens: Some(19),
                output_tokens: Some(7),
                ..Default::default()
            };
            persist_stopped_accounting(
                &mut metrics,
                &mut status,
                &state,
                &state_path,
                &usage,
                TurnOutcome::Failed,
                Some(code),
            )
            .unwrap();
            let result = stop_tracked_turn(
                &mut None,
                &mut metrics,
                &mut status,
                &mut state,
                &state_path,
            );
            assert_eq!(result.unwrap_err(), code.message());
            assert_eq!(status.failures, 1);
            assert_eq!(status.turns_completed, 0);
            assert!(status.session_ready);
            let saved: State = storage::read_json(&state_path).unwrap();
            assert_eq!(saved.session, state.session);
            let canonical: Value = storage::read_json(
                &private
                    .join("metrics-ledger")
                    .join(format!("{}.json", metrics.id())),
            )
            .unwrap();
            assert_eq!(canonical["turns_failed"], 1);
            assert_eq!(canonical["turns_interrupted"], 0);
            assert_eq!(canonical["totals"]["input_tokens"], 19);
            assert!(canonical["active_turn"].is_null());
            drop(metrics);
            let resolved = root.canonicalize().unwrap();
            assert!(resolved.starts_with(&base) && resolved != base);
            fs::remove_dir_all(resolved).unwrap();
        }
    }
    #[test]
    fn export_failure_keeps_identity_and_known_terminal_accounting() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!("npc-ai-final-accounting-{}", new_secret().unwrap()));
        fs::create_dir(&root).unwrap();
        let private = root.join(".ai-client");
        let export = root.join("exports");
        fs::create_dir_all(&export).unwrap();
        let mut metrics = Metrics::open(&private, &export, Provider::Codex, None, true).unwrap();
        let id = metrics.id().to_owned();
        metrics.begin().unwrap();
        let export_file = export.join(format!("{id}.json"));
        fs::remove_file(&export_file).unwrap();
        fs::create_dir(&export_file).unwrap();
        let state = State {
            version: 1,
            provider: Provider::Codex,
            controller: "fixture-controller".into(),
            unit: 1,
            session: Some("11111111-1111-4111-8111-111111111111".into()),
            metrics_id: Some(id.clone()),
        };
        let mut status = Status {
            log: None,
            version: 1,
            pid: std::process::id(),
            provider: Provider::Codex,
            state: "stopping".into(),
            message: "fixture".into(),
            turns_completed: 0,
            failures: 0,
            last_failure_code: None,
            session_ready: false,
            connected: true,
            actor_controlled: false,
            last_connection_fault: None,
            updated_unix_seconds: 0,
        };
        let usage = super::super::metrics::UsageSample {
            input_tokens: Some(19),
            output_tokens: Some(7),
            ..Default::default()
        };
        assert!(
            persist_stopped_accounting(
                &mut metrics,
                &mut status,
                &state,
                &private.join("state.json"),
                &usage,
                TurnOutcome::Completed,
                None
            )
            .is_err()
        );
        let saved: State = storage::read_json(&private.join("state.json")).unwrap();
        assert_eq!(saved.session, state.session);
        let canonical: Value =
            storage::read_json(&private.join("metrics-ledger").join(format!("{id}.json"))).unwrap();
        assert_eq!(canonical["totals"]["input_tokens"], 19);
        assert_eq!(canonical["turns_completed"], 1);
        assert_eq!(canonical["turns_interrupted"], 0);
        assert!(canonical["active_turn"].is_null());
        assert_eq!(status.turns_completed, 1);
        assert!(status.session_ready);
        drop(metrics);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn only_closed_event_summaries_enter_prompts() {
        assert!(
            notification(&json!({"summary":"secret instruction","seq":1,"tick":2,"count":1}))
                .is_none()
        );
        let safe=notification(&json!({"summary":"action_failed","seq":1,"tick":2,"count":999,"payload":"private-token"})).unwrap();
        assert_eq!(safe["count"], 64);
        assert!(!safe.to_string().contains("private-token"));
    }
    #[test]
    fn continuation_reestablishes_only_transport_registration_before_observing() {
        let handshake = [
            "Read AGENTS.md",
            "privately register once",
            "enter_world",
            "take_control",
            "Then observe",
        ]
        .map(|step| {
            CONTINUE
                .find(step)
                .expect("fresh MCP connection requires its handshake")
        });
        assert!(handshake.windows(2).all(|steps| steps[0] < steps[1]));
        assert!(CONTINUE.contains("revoked:true or access denied, stop"));
        assert!(CONTINUE.contains("do not reconnect, register again, or replay a mutation"));
        assert!(CONTINUE.contains("Never reveal credentials"));
        assert!(
            !CONTINUE.contains("--token")
                && !CONTINUE.contains("{TOKEN}")
                && !CONTINUE.contains("session_id=")
        );
    }
    #[test]
    fn disabling_notifications_discards_backlog_and_failed_turn_retries() {
        let event = |seq| json!({"summary":"action_failed","seq":seq,"tick":10,"count":1});
        let mut pending = VecDeque::from([event(1)]);
        let mut in_flight = VecDeque::from([event(2)]);
        refresh_notifications(&mut pending, &mut in_flight, false, 0, &[event(3)]);
        assert!(pending.is_empty() && in_flight.is_empty());
        refresh_notifications(&mut pending, &mut in_flight, true, 0, &[]);
        assert!(
            pending.is_empty() && in_flight.is_empty(),
            "reenabling must not resurrect disabled-period work"
        );
        refresh_notifications(&mut pending, &mut in_flight, true, 3, &[event(3), event(4)]);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0]["seq"], 4);
    }
    #[test]
    fn canonical_current_mission_wins_over_newer_generated_alternatives() {
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "npc-ai-current-{}-{}",
            std::process::id(),
            new_secret().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("prompts")).unwrap();
        fs::write(
            root.join("prompts/current.md"),
            "---\ngenerated: true\n---\nCorrect current mission",
        )
        .unwrap();
        fs::write(
            root.join("prompts/resume_rebuilt.md"),
            "Wrong alternate mission",
        )
        .unwrap();
        let prompt = initial_prompt(&AgentDir { dir: root.clone() }).unwrap();
        assert!(prompt.contains("Correct current mission"));
        assert!(!prompt.contains("Wrong alternate"));
        assert!(!prompt.contains("generated: true"));
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(&base) && resolved != base);
        fs::remove_dir_all(resolved).unwrap();
    }
}
