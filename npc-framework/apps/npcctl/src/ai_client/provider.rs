use super::Provider;
use super::codex_model::SelectionIntent;
use super::diagnostics::{self, FailureCode};
use super::metrics::{UsageSample, UsageScope};
#[path = "provider_usage.rs"]
pub(super) mod usage;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 256 * 1024;
const MAX_OUTPUT: usize = 16 * 1024 * 1024;

pub fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

pub fn resolve(provider: Provider, explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        if !path.is_absolute() || !path.is_file() {
            return Err("--program must name an existing absolute native executable".into());
        }
        if cfg!(windows)
            && !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            return Err("--program must be a native .exe; shell wrappers are not accepted".into());
        }
        return path
            .canonicalize()
            .map_err(|_| "cannot resolve provider executable".into());
    }
    let name = match provider {
        Provider::Codex => "codex",
        Provider::Claude => "claude",
    };
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        // Never search a relative PATH entry in the provider-controlled workspace.
        if !dir.is_absolute() {
            continue;
        }
        let executable = dir.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        if executable.is_file() {
            return Ok(executable);
        }
        if cfg!(windows) && provider == Provider::Codex && dir.join("codex.cmd").is_file() {
            let arch = if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x64"
            };
            let triple = if cfg!(target_arch = "aarch64") {
                "aarch64-pc-windows-msvc"
            } else {
                "x86_64-pc-windows-msvc"
            };
            let root = dir.join("node_modules/@openai/codex");
            for vendor in [
                root.join(format!("node_modules/@openai/codex-win32-{arch}/vendor")),
                root.join("vendor"),
            ] {
                let native = vendor.join(triple).join("bin/codex.exe");
                if native.is_file() {
                    return Ok(native);
                }
            }
        }
    }
    Err(format!(
        "{} CLI was not found; install it and sign in, or choose an absolute native executable with --program",
        name
    ))
}

pub fn quiet(command: &mut Command) {
    command.stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

pub(super) fn subscription_environment(command: &mut Command) {
    for name in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
    ] {
        command.env_remove(name);
    }
}

pub(super) fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        // The direct provider may have exited while its MCP children still hold pipes open.
        // The group was allocated to this owned child at spawn, and may still exist after it exits.
        let mut command = Command::new("kill");
        command
            .args(["-TERM", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null());
        quiet(&mut command);
        let _ = command.status();
    }
    if child.try_wait().ok().flatten().is_none() {
        #[cfg(windows)]
        {
            let mut command = Command::new("taskkill.exe");
            command
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null());
            quiet(&mut command);
            let _ = command.status();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn probe(program: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    quiet(&mut command);
    subscription_environment(&mut command);
    command.stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| "cannot start provider CLI")?;
    let _job = match super::job::Job::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            terminate(&mut child);
            return Err(error);
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or("provider stdout is unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("provider diagnostic pipe is unavailable")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.take((MAX_LINE + 1) as u64).read_to_end(&mut bytes);
        let _ = sender.send((result.is_ok(), bytes));
    });
    let (error_sender, error_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr.take((MAX_LINE + 1) as u64).read_to_end(&mut bytes);
        let _ = error_sender.send((result.is_ok(), bytes));
    });
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "cannot inspect provider CLI")?
        {
            let (ok, mut bytes) = receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| "provider probe did not close its output")?;
            let (error_ok, errors) = error_receiver
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| "provider probe did not close its diagnostics")?;
            if !status.success() || !ok || !error_ok || bytes.len() + errors.len() > MAX_LINE {
                return Err("provider CLI is unavailable or signed out; sign in using its own login command".into());
            }
            if bytes.is_empty() {
                bytes = errors;
            }
            return Ok(bytes);
        }
        if start.elapsed() > Duration::from_secs(5) {
            terminate(&mut child);
            return Err("provider CLI check timed out".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub fn check(provider: Provider, explicit: Option<&Path>) -> Result<Value, String> {
    let program = resolve(provider, explicit)?;
    let version_output = probe(&program, &["--version"])?;
    let version = usage::version(provider, &version_output);
    let scope = usage::scope(provider, version.as_deref());
    let logged_in = match provider {
        Provider::Codex => probe(&program, &["login", "status"])
            .ok()
            .is_some_and(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .to_ascii_lowercase()
                    .contains("chatgpt")
            }),
        Provider::Claude => probe(&program, &["auth", "status", "--json"])
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| {
                value["loggedIn"] == true
                    && matches!(
                        value["authMethod"].as_str(),
                        Some("claude.ai" | "oauth_token")
                    )
            }),
    };
    Ok(
        json!({"provider":provider,"installed":true,"logged_in":logged_in,"ready":logged_in,"cli_version":version,"usage_scope":scope.map_or("unknown",|scope|match scope{UsageScope::Turn=>"turn",UsageScope::SessionCumulative=>"session_cumulative"}),"message":if logged_in { "CLI login is ready; model access is checked when a turn starts" } else { match provider { Provider::Codex => "Sign in with `codex login`, then start the AI client", Provider::Claude => "Sign in with `claude auth login`, then start the AI client" } }}),
    )
}

pub fn usage_scope(readiness: &Value, _provider: Provider) -> Option<UsageScope> {
    match readiness["usage_scope"].as_str() {
        Some("turn") => Some(UsageScope::Turn),
        Some("session_cumulative") => Some(UsageScope::SessionCumulative),
        _ => None,
    }
}

#[derive(Default)]
pub struct Outcome {
    pub session: Option<String>,
    pub completed: bool,
    pub failed: bool,
    pub failure: Option<FailureCode>,
    pub usage: UsageSample,
    pub(crate) claude_game_frames: bool,
    pub(crate) game_tool_uses: BTreeMap<String, String>,
}
pub enum Progress {
    Session(String),
    Usage(UsageSample),
    GameToolCancelled,
    GameFailure(FailureCode),
    End(Outcome),
}

const IDENTITY_ERROR: &str =
    "provider returned a different session identity; stopped to protect conversation ownership";

fn absorb_progress(
    progress: Progress,
    session: &mut Option<String>,
    identity_rejected: &mut bool,
    usage_sample: &mut UsageSample,
    failure: &mut Option<FailureCode>,
    terminal: &mut Option<Outcome>,
) -> Result<(), String> {
    if *identity_rejected {
        return Err(IDENTITY_ERROR.into());
    }
    let incoming_session = match &progress {
        Progress::Session(id) => Some(id.as_str()),
        Progress::End(outcome) => {
            if let Some(code) = outcome.failure.filter(|code| code.requires_attention())
                && !failure.is_some_and(FailureCode::requires_attention)
            {
                *failure = Some(code);
            }
            outcome.session.as_deref()
        }
        _ => None,
    };
    if let Some(id) = incoming_session {
        if session.as_deref().is_some_and(|old| old != id) {
            *identity_rejected = true;
            if !failure.is_some_and(FailureCode::requires_attention) {
                *failure = Some(FailureCode::Unknown);
            }
            return Err(IDENTITY_ERROR.into());
        }
        *session = Some(id.to_owned());
    }
    match progress {
        Progress::Session(_) => {}
        Progress::End(outcome) => {
            usage::merge(usage_sample, &outcome.usage);
            *terminal = Some(outcome);
        }
        Progress::Usage(sample) => usage::merge(usage_sample, &sample),
        Progress::GameToolCancelled => {
            if !failure.is_some_and(FailureCode::requires_attention) {
                *failure = Some(FailureCode::GameToolCancelled);
            }
        }
        Progress::GameFailure(code) => {
            if !failure.is_some_and(FailureCode::requires_attention) {
                *failure = Some(code);
            }
        }
    }
    Ok(())
}

fn parse(provider: Provider, value: &Value, outcome: &mut Outcome, scope: Option<UsageScope>) {
    usage::event(provider, value, &mut outcome.usage, scope);
    let game_failure = if provider == Provider::Codex {
        if diagnostics::codex_game_access_revoked(value) {
            Some(FailureCode::GameAccessRevoked)
        } else if diagnostics::codex_game_tool_cancelled(value) {
            Some(FailureCode::GameToolCancelled)
        } else {
            None
        }
    } else {
        claude_game_failure(value, outcome)
    };
    if let Some(code) = game_failure {
        outcome.failed = true;
        if !outcome.failure.is_some_and(FailureCode::requires_attention) {
            outcome.failure = Some(code);
        }
    }
    match provider {
        Provider::Codex => match value["type"].as_str() {
            Some("thread.started") => {
                if let Some(id) = value["thread_id"].as_str().filter(|id| uuid(id)) {
                    outcome.session = Some(id.to_owned());
                }
            }
            Some("turn.completed") => outcome.completed = true,
            Some("turn.failed" | "error") => {
                outcome.failed = true;
                if !outcome.failure.is_some_and(FailureCode::requires_attention) {
                    outcome.failure = diagnostics::structured(value)
                        .or(outcome.failure)
                        .or(Some(FailureCode::Unknown));
                }
            }
            _ => {}
        },
        Provider::Claude => {
            if let Some(id) = value["session_id"].as_str().filter(|id| uuid(id)) {
                outcome.session = Some(id.to_owned());
            }
            if value["type"] == "result" {
                if value["is_error"] == false
                    && value["subtype"].as_str().is_none_or(|s| s == "success")
                {
                    outcome.completed = true;
                } else {
                    outcome.failed = true;
                    if !outcome.failure.is_some_and(FailureCode::requires_attention) {
                        outcome.failure =
                            diagnostics::structured(value).or(Some(FailureCode::Unknown));
                    }
                }
            }
        }
    }
}

fn claude_game_failure(value: &Value, outcome: &mut Outcome) -> Option<FailureCode> {
    if !outcome.claude_game_frames {
        return None;
    }
    if let Some(code) = diagnostics::claude_game_setup_failure(value, true) {
        return Some(code);
    }
    if value["type"] == "assistant" {
        if let Some(blocks) = value["message"]["content"]
            .as_array()
            .filter(|blocks| blocks.len() <= 64)
        {
            for block in blocks {
                if block["type"] != "tool_use"
                    || !block["name"]
                        .as_str()
                        .is_some_and(diagnostics::claude_game_tool)
                {
                    continue;
                }
                if let Some(id) = block["id"].as_str().filter(|id| {
                    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
                }) {
                    if outcome.game_tool_uses.len() >= 64 {
                        outcome.game_tool_uses.pop_first();
                    }
                    outcome.game_tool_uses.insert(
                        id.to_owned(),
                        block["name"]
                            .as_str()
                            .expect("checked tool name")
                            .to_owned(),
                    );
                }
            }
        }
    } else if value["type"] == "user"
        && let Some(blocks) = value["message"]["content"]
            .as_array()
            .filter(|blocks| blocks.len() <= 64)
    {
        for block in blocks {
            if block["type"] != "tool_result" || block["is_error"] == false {
                continue;
            }
            let Some(id) = block["tool_use_id"].as_str() else {
                continue;
            };
            if outcome.game_tool_uses.remove(id).is_some()
                && diagnostics::game_access_revoked(&block["content"])
            {
                return Some(FailureCode::GameAccessRevoked);
            }
        }
    }
    None
}

fn read_events(
    provider: Provider,
    stdout: impl Read,
    sender: mpsc::SyncSender<Progress>,
    scope: Option<UsageScope>,
    claude_game_frames: bool,
) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    let mut total = 0;
    let mut outcome = Outcome {
        claude_game_frames,
        ..Default::default()
    };
    let mut announced = None;
    let mut attention_announced = false;
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) => chunk,
            Err(_) => {
                outcome.failed = true;
                if !outcome.failure.is_some_and(FailureCode::requires_attention) {
                    outcome.failure = Some(FailureCode::Unknown);
                }
                break;
            }
        };
        if chunk.is_empty() {
            break;
        }
        let count = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |i| i + 1);
        total += count;
        if total > MAX_OUTPUT || line.len() + count > MAX_LINE {
            outcome.failed = true;
            if !outcome.failure.is_some_and(FailureCode::requires_attention) {
                outcome.failure = Some(FailureCode::OutputLimit);
            }
            break;
        }
        line.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if line.last() == Some(&b'\n') {
            if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                let previous = outcome.usage.clone();
                parse(provider, &value, &mut outcome, scope);
                // A Claude result can report a session and usage together.
                // Validate identity before accounting from that same frame.
                if outcome.session != announced {
                    if let Some(id) = &outcome.session
                        && sender.send(Progress::Session(id.clone())).is_err()
                    {
                        return;
                    }
                    announced = outcome.session.clone();
                }
                if let Some(code) = outcome
                    .failure
                    .filter(|code| code.requires_attention() && !attention_announced)
                {
                    let progress = if code == FailureCode::GameToolCancelled {
                        Progress::GameToolCancelled
                    } else {
                        Progress::GameFailure(code)
                    };
                    if sender.send(progress).is_err() {
                        return;
                    }
                    attention_announced = true;
                }
                if outcome.usage != previous {
                    let _ = sender.try_send(Progress::Usage(outcome.usage.clone()));
                }
            }
            line.clear();
        }
    }
    if !line.is_empty()
        && let Ok(value) = serde_json::from_slice::<Value>(&line)
    {
        parse(provider, &value, &mut outcome, scope);
    }
    let _ = sender.send(Progress::End(outcome));
}

pub struct Turn {
    log: Option<super::session_log::SessionLog>,
    logged_outcome: bool,
    readers: Vec<thread::JoinHandle<()>>,
    pub child: Child,
    pub progress: Receiver<Progress>,
    pub started: Instant,
    outcome: Option<Outcome>,
    _job: super::job::Job,
    exit_seen: Option<Instant>,
    stderr_diagnostic: Receiver<FailureCode>,
    pub failure: Option<FailureCode>,
    pub usage: UsageSample,
    stderr_metadata: Receiver<UsageSample>,
    identity_rejected: bool,
}

pub struct TurnSettings<'a> {
    pub log: Option<&'a super::session_log::SessionLog>,
    pub usage_scope: Option<UsageScope>,
    pub selection: Option<&'a SelectionIntent>,
    pub approve_game_tools: bool,
    pub cli_version: Option<&'a str>,
}

fn configure_selection(
    command: &mut Command,
    selection: Option<&SelectionIntent>,
) -> Result<(), String> {
    let selection = selection.ok_or("Codex model selection was not verified before the AI turn")?;
    let model = selection.requested_model.as_str();
    let effort = selection
        .requested_effort
        .as_deref()
        .ok_or("Codex effort selection was not verified before the AI turn")?;
    if usage::model(model).as_deref() != Some(model)
        || usage::effort(effort).as_deref() != Some(effort)
    {
        return Err("Codex model or effort selection is invalid".into());
    }
    command.arg("--model").arg(model).arg("-c").arg(format!(
        "model_reasoning_effort={}",
        serde_json::to_string(effort).map_err(|_| "Cannot encode Codex effort selection")?
    ));
    Ok(())
}

impl Turn {
    #[cfg(test)]
    pub(super) fn queued_for_test(
        log: super::session_log::SessionLog,
        progress: Receiver<Progress>,
    ) -> Self {
        // Run this test harness's listing mode; no provider is involved.
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--list")
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        quiet(&mut command);
        let child = command.spawn().unwrap();
        let job = super::job::Job::attach(&child).unwrap();
        Self {
            log: Some(log),
            logged_outcome: false,
            readers: Vec::new(),
            child,
            progress,
            started: Instant::now(),
            outcome: None,
            _job: job,
            exit_seen: None,
            stderr_diagnostic: mpsc::sync_channel(1).1,
            failure: None,
            usage: UsageSample::default(),
            stderr_metadata: mpsc::sync_channel(1).1,
            identity_rejected: false,
        }
    }

    pub fn start(
        program: &Path,
        provider: Provider,
        workspace: &Path,
        session: Option<&str>,
        prompt: &str,
        settings: TurnSettings<'_>,
    ) -> Result<Self, String> {
        if let Some(log) = settings.log {
            log.check()?;
        }
        let mut command = Command::new(program);
        match provider {
            Provider::Codex => {
                command.args(["exec", "--skip-git-repo-check", "--json"]);
                if let Some(id) = session {
                    command.args(["resume", id]);
                }
                configure_selection(&mut command, settings.selection)?;
                command.arg("-");
            }
            Provider::Claude => {
                command.args(["--print", "--output-format", "stream-json", "--verbose"]);
                if let Some(id) = session {
                    command.args(["--resume", id]);
                }
            }
        }
        command
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        quiet(&mut command);
        command.stderr(Stdio::piped());
        subscription_environment(&mut command);
        super::mcp_config::configure(
            &mut command,
            provider,
            workspace,
            settings.approve_game_tools,
        )?;
        let mut child = command
            .spawn()
            .map_err(|_| "cannot start provider CLI; check its installation")?;
        let job = match super::job::Job::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                terminate(&mut child);
                return Err(error);
            }
        };
        let stdout = child
            .stdout
            .take()
            .ok_or("provider stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("provider diagnostic pipe is unavailable")?;
        let stdout: Box<dyn Read + Send> = if let Some(log) = settings.log {
            Box::new(log.reader(stdout, "CLI stdout", MAX_OUTPUT))
        } else {
            Box::new(stdout)
        };
        let stderr: Box<dyn Read + Send> = if let Some(log) = settings.log {
            Box::new(log.reader(stderr, "CLI stderr", 256 * 1024))
        } else {
            Box::new(stderr)
        };
        let (diagnostic_sender, stderr_diagnostic) = mpsc::sync_channel(1);
        let (metadata_sender, stderr_metadata) = mpsc::sync_channel(1);
        let codex_metadata = provider == Provider::Codex;
        let diagnostics_reader = thread::spawn(move || {
            diagnostics::drain_stderr_with_metadata(
                stderr,
                diagnostic_sender,
                codex_metadata.then_some(metadata_sender),
            )
        });
        let (sender, progress) = mpsc::sync_channel(8);
        let claude_game_frames =
            provider == Provider::Claude && settings.cli_version == Some("2.1.281");
        let events_reader = thread::spawn(move || {
            read_events(
                provider,
                stdout,
                sender,
                settings.usage_scope,
                claude_game_frames,
            )
        });
        let Some(mut stdin) = child.stdin.take() else {
            terminate(&mut child);
            return Err("provider stdin is unavailable".into());
        };
        let bytes = prompt.as_bytes().to_vec();
        // The main loop remains responsive if an unresponsive child does not consume stdin.
        thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
        Ok(Self {
            log: settings.log.cloned(),
            logged_outcome: false,
            readers: vec![diagnostics_reader, events_reader],
            child,
            progress,
            started: Instant::now(),
            outcome: None,
            _job: job,
            exit_seen: None,
            stderr_diagnostic,
            failure: None,
            usage: UsageSample::default(),
            stderr_metadata,
            identity_rejected: false,
        })
    }
    fn record_failure(&mut self, fallback: FailureCode) {
        let code = if matches!(
            fallback,
            FailureCode::Unknown | FailureCode::MissingTerminal | FailureCode::NonzeroExit
        ) {
            self.stderr_diagnostic
                .recv_timeout(Duration::from_millis(50))
                .unwrap_or(fallback)
        } else {
            fallback
        };
        self.failure = Some(code);
        self.collect_metadata(true);
    }
    fn collect_metadata(&mut self, wait: bool) {
        if wait && let Ok(sample) = self.stderr_metadata.recv_timeout(Duration::from_millis(50)) {
            usage::merge(&mut self.usage, &sample);
        }
        while let Ok(sample) = self.stderr_metadata.try_recv() {
            usage::merge(&mut self.usage, &sample);
        }
    }

    fn absorb(&mut self, progress: Progress, session: &mut Option<String>) -> Result<(), String> {
        absorb_progress(
            progress,
            session,
            &mut self.identity_rejected,
            &mut self.usage,
            &mut self.failure,
            &mut self.outcome,
        )
    }

    fn drain_cancelled(&mut self, session: &mut Option<String>) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(200);
        while self.outcome.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let Ok(progress) = self.progress.recv_timeout(remaining) else {
                break;
            };
            self.absorb(progress, session)?;
        }
        Ok(())
    }

    pub fn poll(&mut self, session: &mut Option<String>) -> Result<Option<bool>, String> {
        let logging_error = self.log.as_ref().and_then(|log| log.check().err());
        let result = self.poll_inner(session);
        if let Some(error) = logging_error {
            // A transcript failure is fatal, but must not hide already emitted
            // conversation identity or accounting from the independent ledger.
            terminate(&mut self.child);
            if !self.identity_rejected {
                self.drain_cancelled(session)?;
                self.collect_metadata(true);
            }
            return Err(error);
        }
        if !self.logged_outcome && !matches!(result, Ok(None)) {
            self.logged_outcome = true;
            if let Some(log) = &self.log {
                log.entry(
                    "Turn finished",
                    &json!({
                        "outcome": if result == Ok(Some(true)) { "completed" } else { "failed" },
                        "failure_code":self.failure, "reported_usage":self.usage,
                        "error":result.as_ref().err(),
                    })
                    .to_string(),
                )?;
            }
        }
        result
    }

    fn poll_inner(&mut self, session: &mut Option<String>) -> Result<Option<bool>, String> {
        // Cleanup may poll again after an error. Never consume remaining usage
        // or metadata once its provider conversation identity was rejected.
        if self.identity_rejected {
            return Err(IDENTITY_ERROR.into());
        }
        self.collect_metadata(false);
        while let Ok(progress) = self.progress.try_recv() {
            self.absorb(progress, session)?;
        }
        if let Some(code) = self.failure.filter(|code| code.requires_attention()) {
            // A cancellation is actionable even when the CLI later reports a
            // successful turn. Briefly drain accounting already following it;
            // no new prompt, registration or mutation is sent during cleanup.
            self.drain_cancelled(session)?;
            terminate(&mut self.child);
            self.drain_cancelled(session)?;
            self.record_failure(code);
            return Ok(Some(false));
        }
        if let Some(outcome) = &self.outcome
            && (outcome.failed || !outcome.completed)
        {
            let fallback = if outcome.failed {
                FailureCode::Unknown
            } else {
                let exit = self
                    .child
                    .try_wait()
                    .map_err(|_| "cannot inspect owned provider process")?;
                if exit.is_none()
                    && self.exit_seen.get_or_insert_with(Instant::now).elapsed()
                        < Duration::from_secs(2)
                {
                    return Ok(None);
                }
                if exit.is_some_and(|status| !status.success()) {
                    FailureCode::NonzeroExit
                } else {
                    FailureCode::MissingTerminal
                }
            };
            let failure = outcome.failure.unwrap_or(fallback);
            terminate(&mut self.child);
            self.record_failure(failure);
            return Ok(Some(false));
        }
        let Some(exit) = self
            .child
            .try_wait()
            .map_err(|_| "cannot inspect owned provider process")?
        else {
            return Ok(None);
        };
        let exited = *self.exit_seen.get_or_insert_with(Instant::now);
        let Some(outcome) = &self.outcome else {
            if exited.elapsed() >= Duration::from_secs(2) {
                self.record_failure(FailureCode::MissingTerminal);
                return Ok(Some(false));
            }
            return Ok(None);
        };
        let success = exit.success() && outcome.completed && !outcome.failed && session.is_some();
        if !success {
            self.record_failure(if !exit.success() {
                FailureCode::NonzeroExit
            } else {
                FailureCode::MissingTerminal
            });
        } else {
            self.collect_metadata(true);
        }
        Ok(Some(success))
    }
}
impl Drop for Turn {
    fn drop(&mut self) {
        terminate(&mut self.child);
        // Release a reader waiting to send progress during interruption. Bound
        // the wait: inherited pipes must never hold client shutdown hostage.
        self.progress = mpsc::sync_channel(1).1;
        let deadline = Instant::now() + Duration::from_millis(500);
        while self.readers.iter().any(|reader| !reader.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        for reader in self.readers.drain(..) {
            if reader.is_finished() {
                let _ = reader.join();
            } else if let Some(log) = &self.log {
                let _ = log.entry("Capture incomplete", "Owned CLI output pipe did not close before the shutdown deadline; trailing output may be unavailable.");
            }
        }
        if !self.logged_outcome
            && let Some(log) = &self.log
        {
            let _ = log.entry(
                "Turn interrupted",
                &json!({"reported_usage":self.usage,"failure_code":self.failure}).to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::codex_model::SelectionSource;
    use super::*;

    #[test]
    fn revoked_game_access_is_sticky_and_retains_later_owned_terminal_accounting() {
        let values = [
            json!({"type":"thread.started","thread_id":"11111111-1111-4111-8111-111111111111"}),
            json!({"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"npc","tool":"act_and_wait","status":"completed","arguments":{"token":"PRIVATE_ARGUMENT"},"result":{"content":[],"structured_content":{"error":{"code":-32602,"reason":"unknown_session","data":{"revoked":true},"message":"PRIVATE_RESULT"}}}}}),
            json!({"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30}}),
        ];
        let input = values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let (sender, receiver) = mpsc::sync_channel(8);
        read_events(
            Provider::Codex,
            input.as_bytes(),
            sender,
            Some(UsageScope::SessionCumulative),
            false,
        );
        assert!(matches!(receiver.recv().unwrap(), Progress::Session(_)));
        assert!(matches!(
            receiver.recv().unwrap(),
            Progress::GameFailure(FailureCode::GameAccessRevoked)
        ));
        assert!(matches!(receiver.recv().unwrap(), Progress::Usage(_)));
        let Progress::End(outcome) = receiver.recv().unwrap() else {
            panic!("terminal accounting missing")
        };
        assert!(outcome.completed && outcome.failed);
        assert_eq!(outcome.failure, Some(FailureCode::GameAccessRevoked));
        assert_eq!(outcome.usage.input_tokens, Some(100));
        assert_eq!(outcome.usage.output_tokens, Some(30));
        let (mut session, mut rejected, mut usage, mut failure, mut terminal) =
            (None, false, UsageSample::default(), None, None);
        absorb_progress(
            Progress::End(outcome),
            &mut session,
            &mut rejected,
            &mut usage,
            &mut failure,
            &mut terminal,
        )
        .unwrap();
        assert_eq!(failure, Some(FailureCode::GameAccessRevoked));
        assert_eq!(
            session.as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        assert!(
            absorb_progress(
                Progress::Session("22222222-2222-4222-8222-222222222222".into()),
                &mut session,
                &mut rejected,
                &mut usage,
                &mut failure,
                &mut terminal
            )
            .is_err()
        );
        assert!(
            absorb_progress(
                Progress::Usage(UsageSample {
                    input_tokens: Some(9_000_001),
                    ..Default::default()
                }),
                &mut session,
                &mut rejected,
                &mut usage,
                &mut failure,
                &mut terminal
            )
            .is_err()
        );
        assert_eq!(usage.input_tokens, Some(100));
        assert_eq!(failure, Some(FailureCode::GameAccessRevoked));
    }

    #[test]
    fn claude_game_results_require_the_announced_game_tool_id_and_verified_version() {
        let payload = json!({"error":{"code":-32602,"reason":"unknown_session","data":{"revoked":true},"message":"PRIVATE_RESULT"}});
        for (verified, name, expected) in [
            (
                true,
                "mcp__npc__observe",
                Some(FailureCode::GameAccessRevoked),
            ),
            (false, "mcp__npc__observe", None),
            (true, "mcp__other__observe", None),
        ] {
            let mut outcome = Outcome {
                claude_game_frames: verified,
                ..Default::default()
            };
            parse(
                Provider::Claude,
                &json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool_1","name":name,"input":{"token":"PRIVATE_ARGUMENT"}}]}}),
                &mut outcome,
                Some(UsageScope::SessionCumulative),
            );
            parse(
                Provider::Claude,
                &json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"unannounced","is_error":true,"content":payload.to_string()}]}}),
                &mut outcome,
                Some(UsageScope::SessionCumulative),
            );
            assert_eq!(outcome.failure, None);
            parse(
                Provider::Claude,
                &json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool_1","is_error":true,"content":[{"type":"text","text":payload.to_string()}]}]}}),
                &mut outcome,
                Some(UsageScope::SessionCumulative),
            );
            parse(
                Provider::Claude,
                &json!({"type":"result","subtype":"success","is_error":false,"session_id":"11111111-1111-4111-8111-111111111111","total_cost_usd":0.1}),
                &mut outcome,
                Some(UsageScope::SessionCumulative),
            );
            assert_eq!(outcome.failure, expected);
            assert_eq!(outcome.failed, expected.is_some());
            assert!(outcome.completed);
            assert_eq!(outcome.usage.cost_microusd, Some(100_000));
        }
        for initial in [
            json!({"type":"system","subtype":"init","mcp_servers":[{"name":"npc","status":"failed"}]}),
            json!({"type":"result","subtype":"success","is_error":false,"permission_denials":[{"tool_name":"mcp__npc__register","tool_use_id":"tool_1","tool_input":{"token":"PRIVATE_ARGUMENT"}}]}),
        ] {
            let mut outcome = Outcome {
                claude_game_frames: true,
                ..Default::default()
            };
            parse(Provider::Claude, &initial, &mut outcome, None);
            let code = outcome.failure;
            assert!(code.is_some_and(FailureCode::requires_attention));
            parse(
                Provider::Claude,
                &json!({"type":"result","subtype":"success","is_error":false}),
                &mut outcome,
                None,
            );
            assert!(outcome.completed && outcome.failed);
            assert_eq!(outcome.failure, code);
        }
    }

    #[test]
    fn failed_logging_still_drains_owned_identity_and_usage_without_accepting_a_foreign_session() {
        for foreign in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "npc-log-accounting-{}",
                npc_framework::agent_io::registration::new_secret().unwrap()
            ));
            std::fs::create_dir(&root).unwrap();
            let read_only = root.join("read-only-file");
            std::fs::write(&read_only, "owned fixture").unwrap();
            let log = super::super::session_log::SessionLog::open(
                &root,
                None,
                Provider::Codex,
                "private-token",
            )
            .unwrap();
            log.fail_writes_for_test(&read_only);
            // This is the test harness itself listing tests, never a provider.
            let child = Command::new(std::env::current_exe().unwrap())
                .arg("--list")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let job = super::super::job::Job::attach(&child).unwrap();
            let (sender, progress) = mpsc::sync_channel(8);
            const A: &str = "11111111-1111-4111-8111-111111111111";
            const B: &str = "22222222-2222-4222-8222-222222222222";
            let usage = UsageSample {
                input_tokens: Some(19),
                output_tokens: Some(7),
                ..Default::default()
            };
            sender.send(Progress::Session(A.into())).unwrap();
            sender.send(Progress::Usage(usage.clone())).unwrap();
            if foreign {
                sender.send(Progress::Session(B.into())).unwrap();
                sender
                    .send(Progress::Usage(UsageSample {
                        input_tokens: Some(9999),
                        ..Default::default()
                    }))
                    .unwrap();
            }
            sender
                .send(Progress::End(Outcome {
                    session: Some(A.into()),
                    completed: true,
                    usage: usage.clone(),
                    ..Default::default()
                }))
                .unwrap();
            drop(sender);
            let mut turn = Turn {
                log: Some(log.clone()),
                logged_outcome: false,
                readers: Vec::new(),
                child,
                progress,
                started: Instant::now(),
                outcome: None,
                _job: job,
                exit_seen: None,
                stderr_diagnostic: mpsc::sync_channel(1).1,
                failure: None,
                usage: UsageSample::default(),
                stderr_metadata: mpsc::sync_channel(1).1,
                identity_rejected: false,
            };
            let mut session = None;
            assert!(turn.poll(&mut session).is_err());
            assert_eq!(session.as_deref(), Some(A));
            assert_eq!(turn.usage, usage);
            assert!(
                turn.poll(&mut session).is_err(),
                "cleanup repoll must remain fatal"
            );
            assert_eq!(turn.usage, usage);
            assert!(turn.child.try_wait().unwrap().is_some());
            drop(turn);
            drop(log);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn conflicting_identity_latches_before_terminal_usage_and_every_later_progress() {
        const A: &str = "11111111-1111-4111-8111-111111111111";
        const B: &str = "22222222-2222-4222-8222-222222222222";
        for cancelled in [false, true] {
            for conflict_in_terminal in [false, true] {
                let mut session = Some(A.to_owned());
                let mut rejected = false;
                let mut usage = UsageSample {
                    input_tokens: Some(100),
                    output_tokens: Some(30),
                    ..Default::default()
                };
                let mut failure = cancelled.then_some(FailureCode::GameToolCancelled);
                let mut terminal = None;
                let other_usage = UsageSample {
                    input_tokens: Some(9_000_001),
                    output_tokens: Some(7_000_003),
                    ..Default::default()
                };
                let conflict = if conflict_in_terminal {
                    Progress::End(Outcome {
                        session: Some(B.into()),
                        completed: true,
                        usage: other_usage.clone(),
                        ..Default::default()
                    })
                } else {
                    Progress::Session(B.into())
                };
                assert_eq!(
                    absorb_progress(
                        conflict,
                        &mut session,
                        &mut rejected,
                        &mut usage,
                        &mut failure,
                        &mut terminal
                    )
                    .unwrap_err(),
                    IDENTITY_ERROR
                );
                assert!(rejected);
                // A cleanup repoll must not accept queued numbers even if a
                // later terminal switches its claimed identity back to A.
                for later in [
                    Progress::Usage(other_usage.clone()),
                    Progress::End(Outcome {
                        session: Some(A.into()),
                        completed: true,
                        usage: other_usage,
                        ..Default::default()
                    }),
                ] {
                    assert_eq!(
                        absorb_progress(
                            later,
                            &mut session,
                            &mut rejected,
                            &mut usage,
                            &mut failure,
                            &mut terminal
                        )
                        .unwrap_err(),
                        IDENTITY_ERROR
                    );
                }
                assert_eq!(session.as_deref(), Some(A));
                assert_eq!(usage.input_tokens, Some(100));
                assert_eq!(usage.output_tokens, Some(30));
                assert!(terminal.is_none());
                assert_eq!(
                    failure,
                    Some(if cancelled {
                        FailureCode::GameToolCancelled
                    } else {
                        FailureCode::Unknown
                    })
                );
            }
        }
    }

    #[test]
    fn same_frame_claude_identity_is_announced_before_its_usage() {
        let input = b"{\"type\":\"result\",\"session_id\":\"22222222-2222-4222-8222-222222222222\",\"subtype\":\"success\",\"is_error\":false,\"total_cost_usd\":0.1}\n";
        let (sender, receiver) = mpsc::sync_channel(8);
        read_events(
            Provider::Claude,
            &input[..],
            sender,
            Some(UsageScope::Turn),
            false,
        );
        assert!(
            matches!(receiver.recv().unwrap(), Progress::Session(id) if id == "22222222-2222-4222-8222-222222222222")
        );
        assert!(
            matches!(receiver.recv().unwrap(), Progress::Usage(sample) if sample.cost_microusd == Some(100_000))
        );
        assert!(matches!(receiver.recv().unwrap(), Progress::End(_)));
    }

    #[test]
    fn cancelled_game_tool_latches_failure_but_later_terminal_usage_and_session_survive() {
        let input = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"11111111-1111-4111-8111-111111111111\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"mcp_tool_call\",\"server\":\"npc\",\"tool\":\"register\",\"status\":\"failed\",\"error\":{\"message\":\"user cancelled MCP tool call\"},\"arguments\":{\"token\":\"private-token-marker\"},\"result\":{\"content\":[\"private-result-marker\"]}}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":100,\"cached_input_tokens\":20,\"output_tokens\":30}}\n"
        );
        let (sender, receiver) = mpsc::sync_channel(8);
        read_events(
            Provider::Codex,
            input.as_bytes(),
            sender,
            Some(UsageScope::Turn),
            false,
        );
        assert!(matches!(receiver.recv().unwrap(), Progress::Session(_)));
        assert!(matches!(
            receiver.recv().unwrap(),
            Progress::GameToolCancelled
        ));
        assert!(matches!(receiver.recv().unwrap(), Progress::Usage(_)));
        let Progress::End(mut outcome) = receiver.recv().unwrap() else {
            panic!("terminal accounting is required after the cancellation")
        };
        assert!(outcome.completed && outcome.failed);
        assert_eq!(outcome.failure, Some(FailureCode::GameToolCancelled));
        assert!(outcome.session.is_some());
        assert_eq!(outcome.usage.input_tokens, Some(100));
        assert_eq!(outcome.usage.output_tokens, Some(30));
        parse(
            Provider::Codex,
            &json!({"type":"turn.failed","error":{"code":"authentication_error"}}),
            &mut outcome,
            Some(UsageScope::Turn),
        );
        assert_eq!(outcome.failure, Some(FailureCode::GameToolCancelled));
        let public = serde_json::to_string(&outcome.failure).unwrap();
        assert!(
            !public.contains("private-token-marker") && !public.contains("private-result-marker")
        );
    }

    #[test]
    fn unrelated_mcp_errors_or_claude_lookalikes_do_not_latch_a_game_cancellation() {
        let value = json!({"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"npc","tool":"register","status":"failed","error":{"message":"user cancelled MCP tool call"}}});
        let mut claude = Outcome::default();
        parse(Provider::Claude, &value, &mut claude, None);
        assert!(!claude.failed && claude.failure.is_none());
        for (field, replacement) in [
            ("server", json!("another_server")),
            ("tool", json!("unknown")),
            ("status", json!("completed")),
            ("error", json!({"message":"ordinary rejection"})),
        ] {
            let mut value = value.clone();
            value["item"][field] = replacement;
            let mut outcome = Outcome::default();
            parse(Provider::Codex, &value, &mut outcome, None);
            assert!(!outcome.failed && outcome.failure.is_none());
        }
    }

    #[test]
    fn verified_model_and_effort_are_process_arguments_without_policy_or_global_writes() {
        let selection = SelectionIntent {
            requested_model: "gpt-fixture-selected".into(),
            requested_effort: Some("high".into()),
            model_source: SelectionSource::AdvertisedDefault,
            effort_source: SelectionSource::CliConfig,
        };
        let mut command = Command::new("not-launched");
        configure_selection(&mut command, Some(&selection)).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--model",
                "gpt-fixture-selected",
                "-c",
                "model_reasoning_effort=\"high\""
            ]
        );
        assert_eq!(command.get_envs().count(), 0);
        for selection in [
            None,
            Some(SelectionIntent {
                requested_model: "private-secret-marker".into(),
                ..selection.clone()
            }),
            Some(SelectionIntent {
                requested_effort: None,
                ..selection.clone()
            }),
        ] {
            let mut command = Command::new("not-launched");
            let error = configure_selection(&mut command, selection.as_ref()).unwrap_err();
            assert!(!error.contains("private-secret-marker"));
            assert_eq!(command.get_args().count(), 0);
        }
    }

    #[test]
    fn structured_completion_requires_a_terminal_success_and_exact_identity() {
        let mut outcome = Outcome::default();
        parse(
            Provider::Codex,
            &json!({"type":"thread.started","thread_id":"11111111-1111-4111-8111-111111111111"}),
            &mut outcome,
            None,
        );
        assert!(outcome.session.is_some());
        assert!(!outcome.completed);
        parse(
            Provider::Codex,
            &json!({"type":"turn.completed"}),
            &mut outcome,
            None,
        );
        parse(
            Provider::Codex,
            &json!({"type":"turn.failed","error":{"message":"private-secret"}}),
            &mut outcome,
            None,
        );
        assert!(outcome.completed && outcome.failed);
        assert!(!uuid("--last"));
    }
    #[test]
    fn unbounded_output_is_rejected_without_retaining_the_payload() {
        let (sender, receiver) = mpsc::sync_channel(8);
        read_events(
            Provider::Codex,
            &vec![b'x'; MAX_LINE + 1][..],
            sender,
            None,
            false,
        );
        assert!(matches!(
            receiver.recv().unwrap(),
            Progress::End(Outcome { failed: true, .. })
        ));
    }
    #[test]
    fn claude_failed_result_cannot_be_mistaken_for_a_completed_turn() {
        for result in [
            json!({"type":"result","is_error":true,"subtype":"error_during_execution"}),
            json!({"type":"result","is_error":false,"subtype":"error_max_turns"}),
        ] {
            let mut outcome = Outcome::default();
            parse(Provider::Claude, &result, &mut outcome, None);
            assert!(outcome.failed);
            assert!(!outcome.completed);
        }
        let mut outcome = Outcome::default();
        parse(
            Provider::Claude,
            &json!({"type":"result","is_error":false,"subtype":"success","session_id":"11111111-1111-4111-8111-111111111111","result":"private tool transcript"}),
            &mut outcome,
            None,
        );
        assert!(outcome.completed && !outcome.failed);
        assert!(outcome.session.is_some());
    }
    #[test]
    fn lifecycle_reader_ignores_tool_payloads_and_recovers_a_final_unterminated_json_line() {
        let input=b"{\"type\":\"item.completed\",\"item\":{\"text\":\"private-token\"}}\n{\"type\":\"thread.started\",\"thread_id\":\"11111111-1111-4111-8111-111111111111\"}\n{\"type\":\"turn.completed\"}";
        let (sender, receiver) = mpsc::sync_channel(8);
        read_events(Provider::Codex, &input[..], sender, None, false);
        assert!(matches!(receiver.recv().unwrap(), Progress::Session(_)));
        assert!(matches!(
            receiver.recv().unwrap(),
            Progress::End(Outcome {
                completed: true,
                failed: false,
                session: Some(_),
                ..
            })
        ));
    }
}
