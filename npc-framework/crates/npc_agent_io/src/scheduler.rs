//! Host-only idle and event supervision for external AI CLI sessions. Never run this from NpcSimTick.
//! Policy and process state are not hashed, snapshotted or read by simulation systems.
use crate::{
    api,
    control::{ControlMode, ExternalController},
    stats::{AgentStats, wall_now},
};
use bevy_ecs::prelude::*;
use npc_kernel::{SimControl, SimId, SimPos};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};
#[path = "client_host.rs"]
pub mod client_host;
#[path = "scheduler_events.rs"]
mod notifications;
use notifications::{EventFeed, EventInbox, Receipts};
pub use notifications::{
    MAX_PENDING_EVENTS, SchedulerEventClassifier, SchedulerEventKind, SchedulerEventSummary,
    SchedulerNotification, acknowledge,
};

pub const BINDINGS_FILE: &str = "bindings.json";
const NUDGE: &str = "Continue playing the game in your existing assigned role. Your actor has been idle and your previous turn has ended. Use your existing game connection instructions: observe your actor, check needs and current work, then continue your previous goal with useful game actions. Respect permanent death, saves, ownership and the game's rules. Report a concrete blocker if you cannot proceed. Do not change game or scheduler configuration to avoid playing.";

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchedulerPolicy {
    pub enabled: bool,
    #[serde(skip_serializing_if = "is_true")]
    pub events_enabled: bool,
    pub idle_seconds: u64,
    pub cooldown_seconds: u64,
    pub poll_seconds: u64,
    pub max_nudges_per_hour: u32,
    pub runner_timeout_seconds: u64,
    pub max_concurrent: u32,
    pub skip_paused: bool,
}
impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            events_enabled: true,
            idle_seconds: 120,
            cooldown_seconds: 180,
            poll_seconds: 1,
            max_nudges_per_hour: 6,
            runner_timeout_seconds: 900,
            max_concurrent: 1,
            skip_paused: true,
        }
    }
}
fn is_true(value: &bool) -> bool {
    *value
}
impl SchedulerPolicy {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("idle_seconds", self.idle_seconds, 5, 86400),
            ("cooldown_seconds", self.cooldown_seconds, 5, 86400),
            ("poll_seconds", self.poll_seconds, 1, 30),
            (
                "runner_timeout_seconds",
                self.runner_timeout_seconds,
                10,
                3600,
            ),
        ] {
            if !(min..=max).contains(&value) {
                return Err(format!("scheduler {name} must be {min}..={max}"));
            }
        }
        if !(1..=60).contains(&self.max_nudges_per_hour) || !(1..=4).contains(&self.max_concurrent)
        {
            return Err("scheduler hourly cap must be 1..=60 and concurrency 1..=4".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerProvider {
    Codex,
    Claude,
    Custom,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerBinding {
    pub controller: String,
    pub unit: u32,
    pub provider: RunnerProvider,
    pub session_id: String,
    pub cwd: PathBuf,
    #[serde(default)]
    pub program: Option<PathBuf>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub client_pid: Option<u32>,
    #[serde(default)]
    pub revision: u64,
}
impl fmt::Debug for RunnerBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunnerBinding")
            .field("controller", &self.controller)
            .field("unit", &self.unit)
            .field("provider", &self.provider)
            .field("session_id", &"[configured]")
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}
impl RunnerBinding {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_stored()?;
        if !self.cwd.is_dir() {
            return Err("runner working directory must be an existing absolute directory; restore the workspace or rebind with --cwd".into());
        }
        Ok(())
    }
    fn validate_stored(&self) -> Result<(), String> {
        if self.controller.is_empty()
            || self.controller.len() > 200
            || self.controller.contains(['\n', '\r'])
            || self.unit == 0
        {
            return Err("a binding needs a controller and assigned actor id".into());
        }
        if self.client_pid == Some(0) {
            return Err("original client PID must be positive".into());
        }
        let id = self.session_id.as_bytes();
        if id.len() != 36
            || id.iter().enumerate().any(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    *b != b'-'
                } else {
                    !b.is_ascii_hexdigit()
                }
            })
        {
            return Err("use the exact CLI session UUID, not --last or a conversation name".into());
        }
        if !self.cwd.is_absolute() {
            return Err("runner working directory must be an existing absolute directory; restore the workspace or rebind with --cwd".into());
        }
        if self.provider == RunnerProvider::Custom
            && (self.program.is_none() || self.args.len() > 64)
        {
            return Err("a custom runner needs an executable and at most 64 argv entries".into());
        }
        if self.args.iter().any(|s| s.len() > 4096 || s.contains('\0')) {
            return Err("runner arguments are invalid or too long".into());
        }
        if self.provider != RunnerProvider::Custom && !self.args.is_empty() {
            return Err("use Custom for a user-supplied argument vector".into());
        }
        Ok(())
    }
    pub fn invocation(&self) -> (PathBuf, Vec<String>) {
        let default = match self.provider {
            RunnerProvider::Codex => {
                if cfg!(windows) {
                    "codex.cmd"
                } else {
                    "codex"
                }
            }
            RunnerProvider::Claude => "claude",
            RunnerProvider::Custom => "",
        };
        let args = match self.provider {
            RunnerProvider::Codex => vec![
                "exec".into(),
                "--skip-git-repo-check".into(),
                "resume".into(),
                self.session_id.clone(),
                "-".into(),
            ],
            RunnerProvider::Claude => {
                vec!["--print".into(), "--resume".into(), self.session_id.clone()]
            }
            RunnerProvider::Custom => self
                .args
                .iter()
                .map(|a| a.replace("{session_id}", &self.session_id))
                .collect(),
        };
        (self.program.clone().unwrap_or_else(|| default.into()), args)
    }
    fn target_key(&self) -> String {
        // UUID validation makes this a single safe filename component. No transcript/credential is read.
        format!(
            "{:?}-{}",
            self.provider,
            self.session_id.to_ascii_lowercase()
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RunnerBindings {
    pub bindings: Vec<RunnerBinding>,
}
/// Local configuration checks only: no CLI invocation, authentication request or lock mutation.
#[derive(Clone, Debug, Serialize)]
pub struct RunnerPreflight {
    pub linked: bool,
    pub configuration_ready: bool,
    pub issues: Vec<String>,
    pub authentication: &'static str,
}
pub fn check_binding(path: &Path, controller: &str) -> Result<RunnerPreflight, String> {
    if path.exists() && !path.is_file() {
        return Err(
            "scheduler bindings path is not a file; select the operator-owned bindings JSON".into(),
        );
    }
    let bindings = read_bindings(path)?;
    let binding = bindings
        .bindings
        .iter()
        .find(|b| b.controller == controller);
    let mut issues = Vec::new();
    if let Some(binding) = binding {
        if !binding.cwd.is_dir() {
            issues.push(
                "runner workspace is unavailable; restore the workspace or rebind with --cwd"
                    .into(),
            );
        }
        if resolve_executable(&binding.invocation().0).is_none() {
            issues.push("configured CLI executable is unavailable; install the selected CLI or rebind with --program to its executable".into());
        }
        if let Some(parent) = path.parent() {
            if parent.join(".bindings-write.lock").exists() {
                issues.push("bindings edit lock exists; confirm no operator edit is running before inspecting a stale edit lock".into());
            }
            if parent
                .join("locks")
                .join(format!("{}.lock", binding.target_key()))
                .exists()
            {
                issues.push("runner lock exists; do not overlap turns. After a crash confirm the previous turn finished, then rebind with --turn-finished; an unverifiable lock needs operator inspection".into());
            }
        }
    } else {
        issues.push("unlinked; connect the AI, take control, then use scheduler bind with the exact conversation UUID and a finished-turn declaration or original one-shot --client-pid".into());
    }
    Ok(RunnerPreflight {
        linked: binding.is_some(),
        configuration_ready: issues.is_empty(),
        issues,
        authentication: "not checked; use the selected provider CLI's login/status instructions outside the game. If a turn fails, check login, usage availability and local runner configuration before retrying",
    })
}
pub fn read_bindings(path: &Path) -> Result<RunnerBindings, String> {
    if !path.is_file() {
        return Ok(RunnerBindings::default());
    }
    if fs::metadata(path)
        .map_err(|_| "cannot inspect scheduler bindings")?
        .len()
        > 262_144
    {
        return Err("scheduler bindings file exceeds 256 KiB".into());
    }
    let bytes = fs::read(path).map_err(|_| "cannot read scheduler bindings")?;
    let value: RunnerBindings =
        serde_json::from_slice(&bytes).map_err(|_| "scheduler bindings JSON is invalid")?;
    if value.bindings.len() > 128 {
        return Err("at most 128 agent bindings are supported".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut targets = std::collections::BTreeSet::new();
    for b in &value.bindings {
        // Stored links must remain editable/unlinkable if a workspace was moved or deleted.
        // Existence is checked at preflight and again before a launch.
        b.validate_stored()?;
        if !ids.insert(&b.controller) {
            return Err("duplicate controller in scheduler bindings".into());
        }
        if !targets.insert(b.target_key()) {
            return Err(
                "a CLI conversation can be linked to only one controller in this file".into(),
            );
        }
    }
    Ok(value)
}
pub fn write_binding(path: &Path, mut binding: RunnerBinding) -> Result<(), String> {
    binding.validate()?;
    let _edit = BindingsEdit::acquire(path)?;
    let expected = fs::read(path).ok();
    let mut all = read_bindings(path)?;
    if all
        .bindings
        .iter()
        .any(|b| b.controller != binding.controller && b.target_key() == binding.target_key())
    {
        return Err("this CLI conversation is already linked to another actor".into());
    }
    binding.revision = all
        .bindings
        .iter()
        .find(|b| b.controller == binding.controller)
        .map_or(1, |b| b.revision.saturating_add(1));
    all.bindings.retain(|b| b.controller != binding.controller);
    all.bindings.push(binding);
    all.bindings.sort_by(|a, b| a.controller.cmp(&b.controller));
    write_file(path, &all, expected)
}
pub fn remove_binding(path: &Path, controller: &str) -> Result<(), String> {
    let _edit = BindingsEdit::acquire(path)?;
    let expected = fs::read(path).ok();
    let mut all = read_bindings(path)?;
    all.bindings.retain(|b| b.controller != controller);
    write_file(path, &all, expected)
}
struct BindingsEdit(PathBuf);
impl BindingsEdit {
    fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path.parent().ok_or("bindings need a parent directory")?;
        fs::create_dir_all(parent).map_err(|_| "cannot create scheduler directory")?;
        let lock = parent.join(".bindings-write.lock");
        let mut file=fs::OpenOptions::new().write(true).create_new(true).open(&lock).map_err(|_|"scheduler bindings are being edited (or a stale edit lock needs operator cleanup)")?;
        if write!(file, "{}", std::process::id()).is_err() {
            let _ = fs::remove_file(&lock);
            return Err("cannot claim binding edit lock".into());
        }
        Ok(Self(lock))
    }
}
impl Drop for BindingsEdit {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn write_file(
    path: &Path,
    value: &RunnerBindings,
    expected: Option<Vec<u8>>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("bindings need a parent directory")?;
    fs::create_dir_all(parent).map_err(|_| "cannot create scheduler directory")?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "cannot encode scheduler bindings")?;
    let tmp = parent.join(format!(".bindings-{}.tmp", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options
        .open(&tmp)
        .map_err(|_| "cannot write scheduler bindings")?;
    f.write_all(&bytes)
        .map_err(|_| "cannot write scheduler bindings")?;
    f.sync_all()
        .map_err(|_| "cannot flush scheduler bindings")?;
    drop(f);
    if fs::read(path).ok() != expected {
        let _ = fs::remove_file(&tmp);
        return Err(
            "scheduler bindings changed during this edit; retry with the current file".into(),
        );
    }
    fs::rename(tmp, path).map_err(|_| "cannot replace scheduler bindings".into())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentPulse {
    pub controller: String,
    pub unit: u32,
    pub activity: (u64, u64),
    pub position: Option<(i32, i32)>,
    pub busy: Option<String>,
    pub blocked: Option<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SchedulerAgentStatus {
    pub controller: String,
    pub unit: u32,
    pub state: String,
    pub idle_seconds: u64,
    pub nudges_last_hour: usize,
    pub last_result: Option<String>,
    pub pending_events: usize,
    pub event_overflow: u64,
    pub trigger: Option<String>,
    pub last_delivery_seq: Option<u64>,
}
#[derive(Resource, Clone, Debug, Default, Serialize)]
pub struct SchedulerStatus {
    pub enabled: bool,
    pub events_enabled: bool,
    pub agents: Vec<SchedulerAgentStatus>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default)]
struct Track {
    unit: u32,
    activity: (u64, u64),
    position: Option<(i32, i32)>,
    idle_since: u64,
    last_nudge: Option<u64>,
    attempts: VecDeque<u64>,
    failures: u32,
    last_result: Option<String>,
    binding: Option<RunnerBinding>,
    events: EventInbox,
    last_trigger: Option<String>,
}
impl Track {
    fn update(&mut self, p: &AgentPulse, now: u64) {
        if self.unit != p.unit
            || self.activity != p.activity
            || self.position != p.position
            || p.busy.is_some()
            || p.blocked.is_some()
        {
            self.idle_since = now;
        }
        self.unit = p.unit;
        self.activity = p.activity;
        self.position = p.position;
        while self
            .attempts
            .front()
            .is_some_and(|t| now.saturating_sub(*t) >= 3_600_000)
        {
            self.attempts.pop_front();
        }
    }
    fn state(
        &self,
        p: &AgentPulse,
        policy: &SchedulerPolicy,
        binding: Option<&RunnerBinding>,
        now: u64,
        running: bool,
        capacity: bool,
    ) -> String {
        if running {
            return "running".into();
        }
        if !policy.enabled {
            return "disabled".into();
        }
        if let Some(reason) = &p.blocked {
            return reason.clone();
        }
        if let Some(reason) = &p.busy {
            return format!("busy: {reason}");
        }
        let Some(b) = binding else {
            return "unlinked".into();
        };
        if b.unit != p.unit {
            return "actor changed; link the new actor's CLI session".into();
        }
        if self.attempts.len() >= policy.max_nudges_per_hour as usize {
            return "hourly limit".into();
        }
        let backoff = policy
            .cooldown_seconds
            .saturating_mul(1_u64 << self.failures.min(4))
            .min(3600)
            .saturating_mul(1000);
        if self
            .last_nudge
            .is_some_and(|t| now.saturating_sub(t) < backoff)
        {
            return "cooldown".into();
        }
        if !(policy.events_enabled && !self.events.pending.is_empty())
            && now.saturating_sub(self.idle_since) < policy.idle_seconds * 1000
        {
            return "idle timer".into();
        }
        if !capacity {
            return "waiting for runner slot".into();
        }
        "ready".into()
    }
}
struct OwnedRun {
    controller: String,
    unit: u32,
    child: Child,
    started: u64,
    lock: PathBuf,
    released: bool,
    binding: RunnerBinding,
}
impl OwnedRun {
    fn stop(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        if self.child.try_wait().ok().flatten().is_none() {
            #[cfg(windows)]
            {
                let mut c = Command::new("taskkill.exe");
                c.args(["/PID", &self.child.id().to_string(), "/T", "/F"]);
                quiet(&mut c);
                let _ = c.status();
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_file(&self.lock);
    }
}
impl Drop for OwnedRun {
    fn drop(&mut self) {
        self.stop();
    }
}
#[derive(Resource)]
pub struct SchedulerRuntime {
    pub bindings_path: PathBuf,
    start: Instant,
    last_poll: Option<u64>,
    tracks: BTreeMap<String, Track>,
    runs: Vec<OwnedRun>,
    external_slots: usize,
    last_status: SchedulerStatus,
}
impl SchedulerRuntime {
    pub fn new(bindings_path: PathBuf) -> Self {
        Self {
            bindings_path,
            start: wall_now(),
            last_poll: None,
            tracks: BTreeMap::new(),
            runs: Vec::new(),
            external_slots: 0,
            last_status: SchedulerStatus::default(),
        }
    }
    pub fn poll(&mut self, policy: &SchedulerPolicy, pulses: Vec<AgentPulse>) -> SchedulerStatus {
        let now =
            u64::try_from(wall_now().duration_since(self.start).as_millis()).unwrap_or(u64::MAX);
        self.poll_at(policy, pulses, now)
    }
    pub fn poll_at(
        &mut self,
        policy: &SchedulerPolicy,
        pulses: Vec<AgentPulse>,
        now: u64,
    ) -> SchedulerStatus {
        self.poll_at_with_events(policy, pulses, now, None)
    }
    fn poll_at_with_events(
        &mut self,
        policy: &SchedulerPolicy,
        pulses: Vec<AgentPulse>,
        now: u64,
        feed: Option<&EventFeed<'_>>,
    ) -> SchedulerStatus {
        let mut status = SchedulerStatus {
            enabled: policy.enabled,
            events_enabled: policy.events_enabled,
            ..Default::default()
        };
        if let Err(e) = policy.validate() {
            self.runs.clear();
            status.error = Some(e);
            return status;
        }
        if self
            .last_poll
            .is_some_and(|last| now.saturating_sub(last) < policy.poll_seconds * 1000)
            && self.last_status.enabled == policy.enabled
            && self.last_status.events_enabled == policy.events_enabled
        {
            return self.last_status.clone();
        }
        let bindings = match read_bindings(&self.bindings_path) {
            Ok(b) => b,
            Err(e) => {
                status.error = Some(e);
                RunnerBindings::default()
            }
        };
        // Reap only processes this scheduler created, never a configured original client PID.
        let mut i = 0;
        while i < self.runs.len() {
            let timed_out =
                now.saturating_sub(self.runs[i].started) >= policy.runner_timeout_seconds * 1000;
            let result = self.runs[i].child.try_wait();
            let valid_target = pulses.iter().any(|p| {
                p.controller == self.runs[i].controller
                    && p.unit == self.runs[i].unit
                    && p.blocked
                        .as_deref()
                        .is_none_or(|reason| reason == "game paused")
            });
            let valid_binding = bindings.bindings.iter().any(|b| *b == self.runs[i].binding);
            let cancelled = !valid_target || !valid_binding;
            if timed_out || cancelled || !matches!(result, Ok(None)) {
                let mut run = self.runs.remove(i);
                let t = self.tracks.entry(run.controller.clone()).or_default();
                let success =
                    matches!(result,Ok(Some(s)) if s.success()) && !timed_out && !cancelled;
                t.last_nudge = Some(now);
                t.idle_since = now;
                t.failures = if success {
                    0
                } else {
                    t.failures.saturating_add(1)
                };
                t.last_result = Some(if cancelled {
                    "stopped: actor, control or session link changed".into()
                } else if timed_out {
                    "runner timed out".into()
                } else if success {
                    "turn finished".into()
                } else {
                    "runner failed; check provider login and local runner configuration".into()
                });
                run.stop();
            } else {
                i += 1;
            }
        }
        let can_dispatch = self
            .last_poll
            .is_none_or(|last| now.saturating_sub(last) >= policy.poll_seconds * 1000);
        if can_dispatch {
            self.last_poll = Some(now);
        }
        let linked_active: std::collections::BTreeSet<_> = pulses
            .iter()
            .filter(|p| {
                bindings
                    .bindings
                    .iter()
                    .any(|b| b.controller == p.controller && b.unit == p.unit)
            })
            .map(|p| p.controller.clone())
            .collect();
        // Release/reclaim must not reset hourly caps or failure backoff. Keep rate history for
        // operator-bound controllers while discarding their private event backlog when absent.
        self.tracks.retain(|controller, track| {
            if pulses.iter().any(|pulse| &pulse.controller == controller) {
                true
            } else {
                track.events = EventInbox::default();
                track.last_trigger = None;
                bindings
                    .bindings
                    .iter()
                    .any(|binding| &binding.controller == controller)
            }
        });
        for pulse in pulses {
            let binding = bindings
                .bindings
                .iter()
                .find(|b| b.controller == pulse.controller);
            if linked_active.contains(&pulse.controller)
                && binding.is_some_and(|b| b.unit != pulse.unit)
            {
                continue;
            }
            let running = self
                .runs
                .iter()
                .any(|r| r.controller == pulse.controller && r.unit == pulse.unit);
            let t = self.tracks.entry(pulse.controller.clone()).or_default();
            let changed_actor = t.unit != pulse.unit;
            t.update(&pulse, now);
            let changed_binding = t.binding.as_ref() != binding;
            if changed_binding {
                t.binding = binding.cloned();
                t.failures = 0;
                t.last_nudge = None;
                t.last_result = None;
                t.last_trigger = None;
            }
            if let Some(feed) = feed {
                let clear_events = changed_actor
                    || changed_binding
                    || !policy.events_enabled
                    || binding.is_none_or(|b| b.unit != pulse.unit)
                    || pulse
                        .blocked
                        .as_deref()
                        .is_some_and(|reason| reason != "game paused");
                t.events
                    .ingest(feed, &pulse.controller, SimId(pulse.unit), clear_events);
            } else if changed_actor || changed_binding || !policy.events_enabled {
                t.events = EventInbox::default();
            }
            let mut state = t.state(
                &pulse,
                policy,
                binding,
                now,
                running,
                self.runs.len().saturating_add(self.external_slots)
                    < policy.max_concurrent as usize,
            );
            if state == "ready"
                && can_dispatch
                && let Some(binding) = binding
            {
                if binding.client_pid.is_some_and(process_alive) {
                    state = "original CLI client is still running".into();
                } else {
                    let trigger = if policy.events_enabled && !t.events.pending.is_empty() {
                        "events"
                    } else {
                        "idle"
                    };
                    let prompt = notifications::prompt(&t.events.pending, t.events.overflow);
                    match launch(binding, &self.bindings_path, now, &prompt) {
                        Ok(run) => {
                            t.last_trigger = Some(trigger.into());
                            if trigger == "events" {
                                t.events.delivered();
                            }
                            t.last_nudge = Some(now);
                            t.attempts.push_back(now);
                            self.runs.push(run);
                            state = "running".into();
                        }
                        Err(e) => {
                            t.last_nudge = Some(now);
                            t.attempts.push_back(now);
                            t.failures = t.failures.saturating_add(1);
                            t.last_result = Some(e.clone());
                            state = e;
                        }
                    }
                }
            }
            status.agents.push(SchedulerAgentStatus {
                controller: pulse.controller,
                unit: pulse.unit,
                state,
                idle_seconds: now.saturating_sub(t.idle_since) / 1000,
                nudges_last_hour: t.attempts.len(),
                last_result: t.last_result.clone(),
                pending_events: t.events.pending.len(),
                event_overflow: t.events.overflow,
                trigger: if !t.events.pending.is_empty() {
                    Some("events".into())
                } else {
                    t.last_trigger.clone()
                },
                last_delivery_seq: t.events.last_delivery_seq,
            });
        }
        self.last_status = status.clone();
        status
    }
}
fn quiet(command: &mut Command) {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
}
pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("tasklist.exe");
        command.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
        quiet(&mut command);
        command.stdout(Stdio::piped());
        command.output().map_or(true, |out| {
            String::from_utf8_lossy(&out.stdout).lines().any(|line| {
                line.split(',')
                    .nth(1)
                    .is_some_and(|v| v.trim_matches('"') == pid.to_string())
            })
        })
    }
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .output()
            .map_or(true, |o| {
                String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .any(|s| s == pid.to_string())
            })
    }
}
fn launch(
    binding: &RunnerBinding,
    path: &Path,
    now: u64,
    prompt: &str,
) -> Result<OwnedRun, String> {
    binding.validate()?;
    let locks = path
        .parent()
        .ok_or("scheduler directory missing")?
        .join("locks");
    fs::create_dir_all(&locks).map_err(|_| "cannot create runner lock directory")?;
    let lock = locks.join(format!("{}.lock", binding.target_key()));
    if lock.exists() {
        return Err("this CLI session has a runner lock; after a crash confirm the previous turn has finished before clearing it".into());
    }
    let mut owned = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|_| "CLI session runner is already claimed")?;
    if write!(owned, "{}", std::process::id()).is_err() {
        let _ = fs::remove_file(&lock);
        return Err("cannot write runner lock".into());
    }
    drop(owned);
    let (program, args) = binding.invocation();
    let Some(program) = resolve_executable(&program) else {
        let _ = fs::remove_file(&lock);
        return Err("cannot start the configured CLI executable".into());
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&binding.cwd)
        .stdin(Stdio::piped());
    quiet(&mut command);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => {
            let _ = fs::remove_file(lock);
            return Err("cannot start the configured CLI executable".into());
        }
    };
    let input = child
        .stdin
        .take()
        .ok_or("runner stdin is unavailable")
        .and_then(|mut pipe| {
            pipe.write_all(prompt.as_bytes())
                .map_err(|_| "cannot deliver the continuation prompt")
        });
    if let Err(e) = input {
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(lock);
        return Err(e.into());
    }
    Ok(OwnedRun {
        controller: binding.controller.clone(),
        unit: binding.unit,
        child,
        started: now,
        lock,
        released: false,
        binding: binding.clone(),
    })
}

fn resolve_executable(program: &Path) -> Option<PathBuf> {
    if program.is_absolute() {
        return program.is_file().then(|| program.to_path_buf());
    }
    // Search the host's PATH explicitly, never the AI-controlled child working directory.
    if program.components().count() != 1 {
        return None;
    }
    resolve_from_path(program, &std::env::var_os("PATH")?)
}
fn resolve_from_path(program: &Path, search: &std::ffi::OsStr) -> Option<PathBuf> {
    for dir in std::env::split_paths(search) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            // Resolve relative PATH entries against the host now, before current_dir is set
            // to the agent workspace when the child starts.
            return if candidate.is_absolute() {
                Some(candidate)
            } else {
                candidate.canonicalize().ok()
            };
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            for ext in ["exe", "cmd"] {
                let p = candidate.with_extension(ext);
                if p.is_file() {
                    return if p.is_absolute() {
                        Some(p)
                    } else {
                        p.canonicalize().ok()
                    };
                }
            }
        }
    }
    None
}

/// An explicit operator confirmation can clear a crashed supervisor's lock, never a live owner.
pub fn clear_stale_runner_lock(path: &Path, binding: &RunnerBinding) -> Result<(), String> {
    let lock = path
        .parent()
        .ok_or("scheduler directory missing")?
        .join("locks")
        .join(format!("{}.lock", binding.target_key()));
    if !lock.exists() {
        return Ok(());
    }
    let owner = fs::read_to_string(&lock)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|p| *p > 0)
        .ok_or("runner lock owner cannot be verified; inspect it manually")?;
    if process_alive(owner) {
        return Ok(());
    }
    fs::remove_file(lock).map_err(|_| "cannot clear the confirmed stale runner lock".into())
}
/// Optional host eligibility reasons, e.g. permanent death. Never a simulation resource.
#[derive(Resource, Clone, Debug, Default)]
pub struct SchedulerExclusions(pub BTreeMap<SimId, String>);
/// Current activity/work snapshot. Calling this changes no sim state and no activity counters.
pub fn pulses(world: &mut World, policy: &SchedulerPolicy) -> Vec<AgentPulse> {
    let paused = policy.skip_paused
        && world
            .get_resource::<SimControl>()
            .is_some_and(|s| s.is_stopped());
    let mut actors: Vec<_> = world
        .query::<(&SimId, &ExternalController, Option<&SimPos>)>()
        .iter(world)
        .map(|(id, c, p)| (*id, c.clone(), p.map(|p| (p.x, p.z))))
        .collect();
    actors.sort_by_key(|(id, _, _)| *id);
    actors
        .into_iter()
        .map(|(id, c, pos)| {
            let activity = world
                .get_resource::<AgentStats>()
                .and_then(|s| s.controllers.get(&c.id))
                .map_or((0, 0), |s| (s.observes.saturating_add(s.requests), s.acts));
            let authorized = crate::registration::controller_session_active(world, &c.id, id);
            let excluded = world
                .get_resource::<SchedulerExclusions>()
                .and_then(|s| s.0.get(&id).cloned());
            let blocked = if let Some(reason) = excluded {
                Some(reason)
            } else if !authorized {
                Some("registration revoked or disconnected".into())
            } else if paused {
                Some("game paused".into())
            } else if c.mode == ControlMode::Observer {
                Some("observer control".into())
            } else if !crate::registration::required(world) && activity == (0, 0) {
                Some("waiting for first agent request".into())
            } else {
                None
            };
            AgentPulse {
                controller: c.id,
                unit: id.0,
                activity,
                position: pos,
                busy: api::busy_reason(world, id),
                blocked,
            }
        })
        .collect()
}
/// Host frame hook, deliberately not an ECS system in the simulation schedule.
pub fn frame(world: &mut World) {
    let policy = world
        .get_resource::<SchedulerPolicy>()
        .cloned()
        .unwrap_or_default();
    if let Some(runtime) = world.get_resource::<SchedulerRuntime>() {
        let now =
            u64::try_from(wall_now().duration_since(runtime.start).as_millis()).unwrap_or(u64::MAX);
        if runtime
            .last_poll
            .is_some_and(|last| now.saturating_sub(last) < policy.poll_seconds * 1000)
            && runtime.last_status.enabled == policy.enabled
            && runtime.last_status.events_enabled == policy.events_enabled
        {
            return;
        }
    }
    let mut input = pulses(world, &policy);
    client_host::suppress_legacy(world, &mut input);
    if !world.contains_resource::<SchedulerRuntime>() {
        world.insert_resource(SchedulerStatus {
            enabled: policy.enabled,
            events_enabled: policy.events_enabled,
            agents: input
                .into_iter()
                .map(|p| SchedulerAgentStatus {
                    controller: p.controller,
                    unit: p.unit,
                    state: if policy.enabled {
                        "unlinked"
                    } else {
                        "disabled"
                    }
                    .into(),
                    idle_seconds: 0,
                    nudges_last_hour: 0,
                    last_result: None,
                    pending_events: 0,
                    event_overflow: 0,
                    trigger: None,
                    last_delivery_seq: None,
                })
                .collect(),
            error: None,
        });
        return;
    }
    crate::events::harvest(world);
    let external_slots = client_host::running_slots(world);
    let status = world.resource_scope(|world, mut runtime: Mut<SchedulerRuntime>| {
        runtime.external_slots = external_slots;
        let feed = world
            .get_resource::<crate::events::AgentEvents>()
            .map(|events| EventFeed {
                epoch: api::epoch(world),
                events,
                receipts: world.get_resource::<Receipts>(),
                classifier: world.get_resource::<SchedulerEventClassifier>().copied(),
            });
        let now =
            u64::try_from(wall_now().duration_since(runtime.start).as_millis()).unwrap_or(u64::MAX);
        runtime.poll_at_with_events(&policy, input, now, feed.as_ref())
    });
    world.insert_resource(status);
}

/// An operator-owned host path, deliberately outside any scene/replay descriptor.
pub fn default_bindings_path() -> PathBuf {
    std::env::var_os("NPC_AI_SCHEDULER_BINDINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".npc-ai-scheduler")
                .join(BINDINGS_FILE)
        })
}
pub fn configure_host(world: &mut World, path: PathBuf) {
    if world
        .get_resource::<SchedulerRuntime>()
        .is_none_or(|r| r.bindings_path != path)
    {
        world.insert_resource(SchedulerRuntime::new(path));
    }
}

/// Read-only status/discovery: querying this endpoint must never dispatch a continuation.
pub fn read_status(world: &mut World) -> SchedulerStatus {
    let policy = world
        .get_resource::<SchedulerPolicy>()
        .cloned()
        .unwrap_or_default();
    let mut input = pulses(world, &policy);
    client_host::suppress_legacy(world, &mut input);
    let mut status = world
        .get_resource::<SchedulerStatus>()
        .cloned()
        .unwrap_or_default();
    status.enabled = policy.enabled;
    status.events_enabled = policy.events_enabled;
    status.agents.retain(|a| {
        input
            .iter()
            .any(|p| p.controller == a.controller && p.unit == a.unit)
    });
    for p in input {
        let current_reason = if !policy.enabled {
            Some("disabled".into())
        } else {
            p.blocked
                .clone()
                .or_else(|| p.busy.as_ref().map(|reason| format!("busy: {reason}")))
        };
        if let Some(cached) = status
            .agents
            .iter_mut()
            .find(|a| a.controller == p.controller && a.unit == p.unit)
        {
            // Cached timings are useful, but must not disguise current eligibility.
            // A still-owned running turn remains running until the host reaps it.
            if cached.state != "running" {
                if let Some(reason) = current_reason {
                    cached.state = reason;
                } else if matches!(
                    cached.state.as_str(),
                    "disabled"
                        | "game paused"
                        | "observer control"
                        | "registration revoked or disconnected"
                        | "waiting for first agent request"
                ) || cached.state.starts_with("busy:")
                {
                    cached.state = "waiting for host poll".into();
                }
            }
            continue;
        }
        if !status
            .agents
            .iter()
            .any(|a| a.controller == p.controller && a.unit == p.unit)
        {
            status.agents.push(SchedulerAgentStatus {
                controller: p.controller,
                unit: p.unit,
                state: current_reason.unwrap_or_else(|| {
                    if policy.enabled {
                        "waiting for host poll".into()
                    } else {
                        "disabled".into()
                    }
                }),
                idle_seconds: 0,
                nudges_last_hour: 0,
                last_result: None,
                pending_events: 0,
                event_overflow: 0,
                trigger: None,
                last_delivery_seq: None,
            });
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pulse() -> AgentPulse {
        AgentPulse {
            controller: "a".into(),
            unit: 1,
            activity: (1, 1),
            position: Some((0, 0)),
            busy: None,
            blocked: None,
        }
    }
    fn binding() -> RunnerBinding {
        RunnerBinding {
            controller: "a".into(),
            unit: 1,
            provider: RunnerProvider::Codex,
            session_id: "00000000-0000-4000-8000-000000000001".into(),
            cwd: std::env::current_dir().unwrap(),
            program: None,
            args: vec![],
            client_pid: None,
            revision: 1,
        }
    }
    #[test]
    fn named_resume_and_stdin_never_select_an_unrelated_latest_session() {
        let b = binding();
        b.validate().unwrap();
        let (_, args) = b.invocation();
        assert_eq!(
            args,
            vec![
                "exec",
                "--skip-git-repo-check",
                "resume",
                &b.session_id,
                "-"
            ]
        );
        assert!(!format!("{b:?}").contains(&b.session_id));
        let mut bad = b.clone();
        bad.session_id = "--last".into();
        assert!(bad.validate().is_err());
        let mut claude = b;
        claude.provider = RunnerProvider::Claude;
        assert_eq!(
            claude.invocation().1,
            vec!["--print", "--resume", &claude.session_id]
        );
    }
    #[test]
    fn relative_path_entry_is_resolved_before_entering_an_agent_workspace() {
        // File discovery alone is enough here; no executable is launched or PATH changed.
        let program = Path::new("Cargo.toml");
        let resolved = resolve_from_path(program, std::ffi::OsStr::new(".")).unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(
            resolved,
            std::env::current_dir()
                .unwrap()
                .join(program)
                .canonicalize()
                .unwrap()
        );
    }
    #[test]
    fn idle_busy_pause_activity_cooldown_and_actor_replacement_are_distinct() {
        let policy = SchedulerPolicy {
            enabled: true,
            idle_seconds: 10,
            cooldown_seconds: 20,
            ..Default::default()
        };
        let b = binding();
        let mut p = pulse();
        let mut t = Track::default();
        t.update(&p, 0);
        assert_eq!(
            t.state(&p, &policy, Some(&b), 9000, false, true),
            "idle timer"
        );
        assert_eq!(t.state(&p, &policy, Some(&b), 10000, false, true), "ready");
        p.busy = Some("crafting".into());
        t.update(&p, 12000);
        assert!(
            t.state(&p, &policy, Some(&b), 30000, false, true)
                .starts_with("busy")
        );
        p.busy = None;
        p.activity.0 += 1;
        t.update(&p, 30000);
        assert_eq!(
            t.state(&p, &policy, Some(&b), 31000, false, true),
            "idle timer"
        );
        t.last_nudge = Some(40000);
        assert_eq!(
            t.state(&p, &policy, Some(&b), 41000, false, true),
            "cooldown"
        );
        p.blocked = Some("game paused".into());
        assert_eq!(
            t.state(&p, &policy, Some(&b), 70000, false, true),
            "game paused"
        );
        p.blocked = None;
        p.unit = 2;
        t.update(&p, 70000);
        assert!(
            t.state(&p, &policy, Some(&b), 90000, false, true)
                .starts_with("actor changed")
        );
    }

    fn fixture() -> (PathBuf, RunnerBinding, SchedulerPolicy) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "npc-scheduler-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".scheduler-fake-permit"), "owned test fixture").unwrap();
        let mut b = binding();
        b.provider = RunnerProvider::Custom;
        b.program = Some(std::env::current_exe().unwrap());
        b.cwd = dir.clone();
        b.args = vec![
            "--exact".into(),
            "scheduler::tests::fake_cli".into(),
            "--nocapture".into(),
        ];
        let policy = SchedulerPolicy {
            enabled: true,
            idle_seconds: 5,
            cooldown_seconds: 5,
            runner_timeout_seconds: 10,
            ..Default::default()
        };
        (dir, b, policy)
    }
    fn event_push(events: &mut crate::events::AgentEvents, kind: npc_kernel::journal::JournalKind) {
        let seq = events.next_seq;
        events.next_seq += 1;
        events.events.push_back(crate::events::AgentEvent {
            seq,
            tick: seq,
            unit: Some(SimId(1)),
            kind: kind.name().into(),
            event: kind,
        });
    }
    fn poll_events(
        runtime: &mut SchedulerRuntime,
        policy: &SchedulerPolicy,
        pulse: AgentPulse,
        now: u64,
        events: &crate::events::AgentEvents,
    ) -> SchedulerStatus {
        runtime.poll_at_with_events(
            policy,
            vec![pulse],
            now,
            Some(&EventFeed {
                epoch: 1,
                events,
                receipts: None,
                classifier: None,
            }),
        )
    }
    #[test]
    fn event_policy_is_additive_to_legacy_presets_without_rewriting_their_serialization() {
        let policy: SchedulerPolicy =
            serde_json::from_value(serde_json::json!({"enabled":true})).unwrap();
        assert!(policy.events_enabled);
        assert!(
            serde_json::to_value(&policy)
                .unwrap()
                .get("events_enabled")
                .is_none()
        );
        let disabled = SchedulerPolicy {
            events_enabled: false,
            ..policy
        };
        assert_eq!(
            serde_json::to_value(disabled).unwrap()["events_enabled"],
            false
        );
    }
    #[test]
    fn events_wait_through_busy_pause_running_cooldown_and_caps_then_deliver_historical_stdin() {
        use npc_kernel::journal::JournalKind;
        let (dir, b, mut policy) = fixture();
        fs::write(dir.join("sleep"), "").unwrap();
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file);
        let mut events = crate::events::AgentEvents::default();
        let mut p = pulse();
        poll_events(&mut runtime, &policy, p.clone(), 0, &events);
        event_push(
            &mut events,
            JournalKind::OrderDone {
                order: "private-order-name".into(),
            },
        );
        p.busy = Some("crafting".into());
        let busy = poll_events(&mut runtime, &policy, p.clone(), 1000, &events);
        assert_eq!(busy.agents[0].pending_events, 1);
        assert!(runtime.runs.is_empty());
        p.busy = None;
        p.blocked = Some("game paused".into());
        let paused = poll_events(&mut runtime, &policy, p.clone(), 2000, &events);
        assert_eq!(paused.agents[0].state, "game paused");
        assert_eq!(paused.agents[0].pending_events, 1);
        p.blocked = None;
        let running = poll_events(&mut runtime, &policy, p.clone(), 3000, &events);
        assert_eq!(
            running.agents[0].state, "running",
            "pending events bypass only the idle timer"
        );
        assert_eq!(running.agents[0].trigger.as_deref(), Some("events"));
        assert_eq!(running.agents[0].last_delivery_seq, Some(1));
        assert_eq!(running.agents[0].pending_events, 0);
        wait_file(&dir.join("received-prompt.txt"));
        let prompt = fs::read_to_string(dir.join("received-prompt.txt")).unwrap();
        assert!(prompt.contains("Historical events") && prompt.contains("Observe your actor"));
        assert!(prompt.contains("OrderDone") && prompt.contains("order_completed"));
        assert!(!prompt.contains("private-order-name") && !prompt.contains(&b.session_id));
        event_push(
            &mut events,
            JournalKind::NeedLevel {
                need: "private-need-name".into(),
                from: npc_kernel::journal::NeedLevel::Ok,
                to: npc_kernel::journal::NeedLevel::Urgent,
            },
        );
        let active = poll_events(&mut runtime, &policy, p.clone(), 4000, &events);
        assert_eq!(active.agents[0].state, "running");
        assert_eq!(active.agents[0].pending_events, 1);
        assert_eq!(runtime.runs.len(), 1);
        policy.max_nudges_per_hour = 1;
        let capped = poll_events(&mut runtime, &policy, p.clone(), 14000, &events);
        assert_eq!(capped.agents[0].state, "hourly limit");
        assert_eq!(
            capped.agents[0].last_result.as_deref(),
            Some("runner timed out")
        );
        assert_eq!(capped.agents[0].pending_events, 1);
        policy.max_nudges_per_hour = 60;
        let cooldown = poll_events(&mut runtime, &policy, p, 15000, &events);
        assert_eq!(cooldown.agents[0].state, "cooldown");
        assert_eq!(cooldown.agents[0].pending_events, 1);
        drop(runtime);
        cleanup(dir);
    }
    #[test]
    fn releasing_and_reclaiming_control_preserves_hourly_cap_and_failure_cooldown() {
        use npc_kernel::journal::JournalKind;
        let (dir, binding, mut policy) = fixture();
        fs::write(dir.join("sleep"), "").unwrap();
        let file = dir.join("bindings.json");
        write_binding(&file, binding).unwrap();
        policy.max_nudges_per_hour = 1;
        let mut runtime = SchedulerRuntime::new(file);
        let mut events = crate::events::AgentEvents::default();
        poll_events(&mut runtime, &policy, pulse(), 0, &events);
        event_push(
            &mut events,
            JournalKind::OrderDone {
                order: "Move".into(),
            },
        );
        assert_eq!(
            poll_events(&mut runtime, &policy, pulse(), 1000, &events).agents[0].state,
            "running"
        );
        wait_file(&dir.join("received-prompt.txt"));
        // The actor releases control, cancelling only the scheduler-owned run.
        runtime.poll_at_with_events(
            &policy,
            Vec::new(),
            2000,
            Some(&EventFeed {
                epoch: 1,
                events: &events,
                receipts: None,
                classifier: None,
            }),
        );
        assert!(runtime.runs.is_empty());
        assert_eq!(runtime.tracks["a"].attempts.len(), 1);
        assert!(runtime.tracks["a"].events.pending.is_empty());
        let reclaimed = poll_events(&mut runtime, &policy, pulse(), 3000, &events);
        assert_eq!(reclaimed.agents[0].state, "hourly limit");
        event_push(
            &mut events,
            JournalKind::OrderDone {
                order: "Move".into(),
            },
        );
        let capped = poll_events(&mut runtime, &policy, pulse(), 4000, &events);
        assert_eq!(capped.agents[0].state, "hourly limit");
        assert_eq!(capped.agents[0].pending_events, 1);
        policy.max_nudges_per_hour = 60;
        let cooldown = poll_events(&mut runtime, &policy, pulse(), 5000, &events);
        assert_eq!(cooldown.agents[0].state, "cooldown");
        assert_eq!(cooldown.agents[0].pending_events, 1);
        assert!(runtime.runs.is_empty());
        drop(runtime);
        cleanup(dir);
    }
    #[test]
    fn failed_event_launch_retains_notifications_and_original_clients_and_policy_gates_are_respected()
     {
        use npc_kernel::journal::JournalKind;
        let (dir, mut b, mut policy) = fixture();
        b.program = Some(dir.join("missing-cli"));
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file.clone());
        let mut events = crate::events::AgentEvents::default();
        let mut p = pulse();
        poll_events(&mut runtime, &policy, p.clone(), 0, &events);
        event_push(
            &mut events,
            JournalKind::OrderDone {
                order: "Move".into(),
            },
        );
        let failed = poll_events(&mut runtime, &policy, p.clone(), 1000, &events);
        assert!(failed.agents[0].state.contains("cannot start"));
        assert_eq!(failed.agents[0].pending_events, 1);
        assert_eq!(failed.agents[0].last_delivery_seq, None);
        assert_eq!(
            poll_events(&mut runtime, &policy, p.clone(), 2000, &events).agents[0].state,
            "cooldown"
        );
        policy.enabled = false;
        let disabled = poll_events(&mut runtime, &policy, p.clone(), 3000, &events);
        assert_eq!(disabled.agents[0].state, "disabled");
        assert_eq!(
            disabled.agents[0].pending_events, 1,
            "master pause retains bounded historical events"
        );
        policy.events_enabled = false;
        assert_eq!(
            poll_events(&mut runtime, &policy, p.clone(), 4000, &events).agents[0].pending_events,
            0
        );
        policy.enabled = true;
        policy.events_enabled = true;
        b.client_pid = Some(std::process::id());
        write_binding(&file, b).unwrap();
        assert_eq!(
            poll_events(&mut runtime, &policy, p.clone(), 5000, &events).agents[0].pending_events,
            0,
            "new links rebaseline"
        );
        event_push(
            &mut events,
            JournalKind::OrderDone {
                order: "Move".into(),
            },
        );
        let original = poll_events(&mut runtime, &policy, p.clone(), 6000, &events);
        assert_eq!(
            original.agents[0].state,
            "original CLI client is still running"
        );
        assert_eq!(original.agents[0].pending_events, 1);
        assert!(runtime.runs.is_empty());
        p.blocked = Some("registration revoked or disconnected".into());
        assert_eq!(
            poll_events(&mut runtime, &policy, p, 7000, &events).agents[0].pending_events,
            0
        );
        assert!(process_alive(std::process::id()));
        drop(runtime);
        cleanup(dir);
    }
    fn wait_file(path: &Path) {
        let start = wall_now();
        while !path.is_file() && start.elapsed() < std::time::Duration::from_secs(30) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(path.is_file(), "fake CLI did not receive its prompt");
    }
    fn cleanup(dir: PathBuf) {
        let path = dir.canonicalize().unwrap();
        assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn fake_cli() {
        if !Path::new(".scheduler-fake-permit").is_file() {
            return;
        }
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        // Publish readiness only after the full file is written and closed. `fs::write` creates
        // its destination before writing, so waiting on that destination alone races an empty
        // or partial read in the parent, especially when the scheduler test suite runs in parallel.
        fs::write("received-prompt.partial", input).unwrap();
        fs::rename("received-prompt.partial", "received-prompt.txt").unwrap();
        if Path::new("sleep").is_file() {
            std::thread::sleep(std::time::Duration::from_secs(120));
        }
        assert!(
            !Path::new("fail").is_file(),
            "intentional fake runner failure"
        );
    }
    #[test]
    fn owned_background_runner_receives_stdin_and_cannot_overlap_or_outlive_timeout() {
        let (dir, b, policy) = fixture();
        fs::write(dir.join("sleep"), "").unwrap();
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file.clone());
        let p = pulse();
        assert_eq!(
            runtime.poll_at(&policy, vec![p.clone()], 0).agents[0].state,
            "idle timer"
        );
        assert_eq!(
            runtime.poll_at(&policy, vec![p.clone()], 5000).agents[0].state,
            "running"
        );
        wait_file(&dir.join("received-prompt.txt"));
        let prompt = fs::read_to_string(dir.join("received-prompt.txt")).unwrap();
        assert!(prompt.contains("Continue playing"));
        assert_eq!(prompt, NUDGE, "fake CLI must receive the complete prompt");
        assert!(!prompt.contains(&b.session_id));
        let same = runtime.poll_at(&policy, vec![p.clone()], 6000);
        assert_eq!(same.agents[0].nudges_last_hour, 1);
        assert_eq!(same.agents[0].state, "running");
        assert_eq!(runtime.runs.len(), 1);
        let mut other = SchedulerRuntime::new(file);
        other.poll_at(&policy, vec![p.clone()], 0);
        let conflict = other.poll_at(&policy, vec![p.clone()], 5000);
        assert!(conflict.agents[0].state.contains("lock"));
        assert!(other.runs.is_empty());
        drop(other);
        let ended = runtime.poll_at(&policy, vec![p.clone()], 16000);
        assert_eq!(
            ended.agents[0].last_result.as_deref(),
            Some("runner timed out")
        );
        assert!(runtime.runs.is_empty());
        assert_eq!(
            runtime.poll_at(&policy, vec![p], 17000).agents[0].state,
            "cooldown"
        );
        drop(runtime);
        cleanup(dir);
    }
    #[test]
    fn changed_actor_or_binding_stops_only_the_owned_nudge_process() {
        let (dir, b, policy) = fixture();
        fs::write(dir.join("sleep"), "").unwrap();
        let file = dir.join("bindings.json");
        write_binding(&file, b).unwrap();
        let mut runtime = SchedulerRuntime::new(file);
        let mut p = pulse();
        runtime.poll_at(&policy, vec![p.clone()], 0);
        runtime.poll_at(&policy, vec![p.clone()], 5000);
        wait_file(&dir.join("received-prompt.txt"));
        p.unit = 2;
        let status = runtime.poll_at(&policy, vec![p], 6000);
        assert!(runtime.runs.is_empty());
        assert!(status.agents[0].state.starts_with("actor changed"));
        drop(runtime);
        cleanup(dir);
    }
    #[test]
    fn failed_launch_is_bounded_and_state_never_leaks_session_identity() {
        let (dir, mut b, policy) = fixture();
        b.program = Some(dir.join("missing-executable"));
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file);
        let p = pulse();
        runtime.poll_at(&policy, vec![p.clone()], 0);
        let failed = runtime.poll_at(&policy, vec![p.clone()], 5000);
        assert_eq!(failed.agents[0].nudges_last_hour, 1);
        assert!(failed.agents[0].state.contains("cannot start"));
        let next = runtime.poll_at(&policy, vec![p], 6000);
        assert_eq!(next.agents[0].state, "cooldown");
        assert!(
            !serde_json::to_string(&next)
                .unwrap()
                .contains(&b.session_id)
        );
        drop(runtime);
        cleanup(dir);
    }
    #[test]
    fn read_only_preflight_finds_missing_cli_link_workspace_and_locks_without_running_or_writing() {
        let (dir, mut b, _) = fixture();
        let file = dir.join("bindings.json");
        let absent = check_binding(&file, "a").unwrap();
        assert!(!absent.linked && !absent.configuration_ready);
        assert!(absent.issues[0].contains("exact conversation UUID"));
        assert!(!file.exists());

        b.program = Some(dir.join("missing-cli"));
        write_binding(&file, b.clone()).unwrap();
        let bytes = fs::read(&file).unwrap();
        let missing_cli = check_binding(&file, "a").unwrap();
        assert!(missing_cli.linked && !missing_cli.configuration_ready);
        assert!(missing_cli.issues[0].contains("--program"));
        assert!(missing_cli.authentication.contains("not checked"));
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert!(!dir.join("locks").exists());

        b.program = Some(std::env::current_exe().unwrap());
        write_binding(&file, b.clone()).unwrap();
        assert!(check_binding(&file, "a").unwrap().configuration_ready);
        assert!(!dir.join("received-prompt.txt").exists());
        fs::create_dir(dir.join("locks")).unwrap();
        let lock = dir.join("locks").join(format!("{}.lock", b.target_key()));
        fs::write(&lock, "unverifiable lock fixture").unwrap();
        let locked = check_binding(&file, "a").unwrap();
        assert!(!locked.configuration_ready);
        assert!(locked.issues[0].contains("--turn-finished"));
        assert_eq!(
            fs::read_to_string(&lock).unwrap(),
            "unverifiable lock fixture"
        );
        assert!(
            !serde_json::to_string(&locked)
                .unwrap()
                .contains(&b.session_id)
        );
        assert!(!dir.join("received-prompt.txt").exists());

        let workspace = dir.join("disposable-workspace");
        fs::create_dir(&workspace).unwrap();
        b.cwd = workspace.clone();
        write_binding(&file, b.clone()).unwrap();
        fs::remove_dir(&workspace).unwrap();
        let unavailable = check_binding(&file, "a").unwrap();
        assert!(!unavailable.configuration_ready);
        assert!(
            unavailable
                .issues
                .iter()
                .any(|e| e.contains("restore the workspace or rebind with --cwd"))
        );
        b.cwd = dir.clone();
        write_binding(&file, b.clone()).unwrap();
        assert_eq!(read_bindings(&file).unwrap().bindings[0].cwd, dir);
        b.cwd = workspace.clone();
        fs::create_dir(&workspace).unwrap();
        write_binding(&file, b).unwrap();
        fs::remove_dir(&workspace).unwrap();
        remove_binding(&file, "a").unwrap();
        assert!(!check_binding(&file, "a").unwrap().linked);
        cleanup(dir);
    }
    #[test]
    fn failure_backoff_restart_and_unlink_preserve_owned_runner_lifecycle() {
        let (dir, b, policy) = fixture();
        fs::write(dir.join("fail"), "").unwrap();
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file.clone());
        runtime.poll_at(&policy, vec![pulse()], 0);
        runtime.poll_at(&policy, vec![pulse()], 5000);
        wait_file(&dir.join("received-prompt.txt"));
        assert!(!runtime.runs[0].child.wait().unwrap().success());
        let failed = runtime.poll_at(&policy, vec![pulse()], 6000);
        assert!(
            failed.agents[0]
                .last_result
                .as_deref()
                .unwrap()
                .contains("check provider login")
        );
        assert_eq!(
            runtime.poll_at(&policy, vec![pulse()], 15000).agents[0].state,
            "cooldown"
        );
        assert!(runtime.runs.is_empty());
        assert!(check_binding(&file, "a").unwrap().configuration_ready);
        drop(runtime);

        // Restart begins a fresh idle interval. Unlinking then stops only the owned continuation.
        fs::remove_file(dir.join("fail")).unwrap();
        fs::remove_file(dir.join("received-prompt.txt")).unwrap();
        fs::write(dir.join("sleep"), "").unwrap();
        let mut restarted = SchedulerRuntime::new(file.clone());
        assert_eq!(
            restarted.poll_at(&policy, vec![pulse()], 0).agents[0].state,
            "idle timer"
        );
        assert_eq!(
            restarted.poll_at(&policy, vec![pulse()], 5000).agents[0].state,
            "running"
        );
        wait_file(&dir.join("received-prompt.txt"));
        remove_binding(&file, "a").unwrap();
        let stopped = restarted.poll_at(&policy, vec![pulse()], 6000);
        assert_eq!(stopped.agents[0].state, "unlinked");
        assert!(restarted.runs.is_empty());
        assert!(fs::read_dir(dir.join("locks")).unwrap().next().is_none());
        assert!(!check_binding(&file, "a").unwrap().linked);
        drop(restarted);
        cleanup(dir);
    }
    #[test]
    fn hourly_limit_and_live_original_client_prevent_repeated_dispatch() {
        let (dir, mut b, mut policy) = fixture();
        b.client_pid = Some(std::process::id());
        let file = dir.join("bindings.json");
        write_binding(&file, b.clone()).unwrap();
        let mut runtime = SchedulerRuntime::new(file);
        runtime.poll_at(&policy, vec![pulse()], 0);
        let guarded = runtime.poll_at(&policy, vec![pulse()], 5000);
        assert!(guarded.agents[0].state.contains("original CLI"));
        assert!(runtime.runs.is_empty());
        policy.max_nudges_per_hour = 1;
        let mut t = Track::default();
        let p = pulse();
        t.update(&p, 0);
        t.attempts.push_back(0);
        assert_eq!(
            t.state(&p, &policy, Some(&b), 6000, false, true),
            "hourly limit"
        );
        t.update(&p, 3_600_000);
        assert_eq!(t.attempts.len(), 0);
        drop(runtime);
        cleanup(dir);
    }
}
