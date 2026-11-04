//! Dedicated client rehearsals: real explorer HTTP/MCP, native fake providers, no model turns.
use npc_framework::agent_io::http::RpcClient;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const FOREIGN_SESSION: &str = "22222222-2222-4222-8222-222222222222";
const CONFIG_CANARY: &str = "private-config-canary-never-export";

struct Disposable(PathBuf, PathBuf);
impl Disposable {
    fn new(tag: &str) -> Self {
        let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        std::fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = base.join(format!("ai-client-{tag}-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).expect("fresh disposable AI client home");
        Self(dir.canonicalize().unwrap(), base)
    }
}
impl Drop for Disposable {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize()
            && path.is_absolute()
            && path.starts_with(&self.1)
            && path != self.1
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

struct OwnedChild(Option<Child>);
impl OwnedChild {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }
    fn child(&mut self) -> &mut Child {
        self.0.as_mut().unwrap()
    }
    fn finish(mut self, timeout: Duration) -> Output {
        wait_until(
            timeout,
            || self.child().try_wait().unwrap().is_some(),
            "owned child did not exit",
        );
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            #[cfg(windows)]
            if child.try_wait().ok().flatten().is_none() {
                let mut command = Command::new("taskkill.exe");
                command
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                quiet(&mut command);
                let _ = command.status();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn quiet(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool, failure: &str) {
    let deadline = Instant::now() + timeout;
    while !predicate() {
        assert!(Instant::now() < deadline, "{failure}");
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn fixture_program() -> &'static Path {
    static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAM
        .get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("ai-client-provider-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let program = dir.join(format!(
                "ai-client-provider{}",
                std::env::consts::EXE_SUFFIX
            ));
            let mut command = Command::new("rustc");
            command
                .arg("--edition=2024")
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/ai_client_provider.rs"),
                )
                .arg("-o")
                .arg(&program);
            quiet(&mut command);
            let output = command
                .output()
                .expect("compile owned native provider fixture");
            assert!(
                output.status.success(),
                "fixture compilation: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            program.canonicalize().unwrap()
        })
        .as_path()
}

fn metrics_records(directory: &Path, host: &Host) -> Vec<Value> {
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .expect("configured metrics folder must exist")
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths.into_iter().map(|path|{
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        for secret in [&host.token,SESSION,FOREIGN_SESSION,CONFIG_CANARY,"private-provider-transcript-marker","private-provider-stderr-marker","fixture_unknown_verb","Owned fixture checking its game tools."] {
            assert!(!text.contains(secret),"operator metrics must not contain credentials, provider identity, transcripts or tool payloads");
        }
        let record:Value = serde_json::from_slice(&bytes).expect("metrics are readable JSON");
        let id = record["metrics_id"].as_str().expect("local metrics id");
        assert_eq!(id.len(),32,"metrics must use a local opaque id rather than provider conversation identity");
        assert!(id.bytes().all(|byte|byte.is_ascii_hexdigit()));
        assert_eq!(path.file_stem().and_then(|stem|stem.to_str()),Some(id));
        record
    }).collect()
}

fn assert_metrics_totals(record: &Value, provider: &str, turns: u64) {
    assert_eq!(record["turns_started"], turns);
    assert_eq!(record["turns_completed"], turns);
    assert_eq!(record["turns_failed"], 0);
    assert_eq!(
        record["totals"]["input_tokens"],
        if provider == "codex" {
            100 * turns
        } else {
            125 * turns
        }
    );
    assert_eq!(record["totals"]["output_tokens"], 30 * turns);
    assert_eq!(record["totals"]["cached_input_tokens"], 20 * turns);
    assert_eq!(record["totals"]["reported_input_turns"], turns);
    assert_eq!(record["totals"]["reported_output_turns"], turns);
    if provider == "codex" {
        assert!(
            record["totals"]["cost_microusd"].is_null(),
            "Codex subscription usage must not imply invented dollar cost"
        );
        assert_eq!(record["cost_available"], false);
        assert_eq!(record["cost_basis"], "unknown");
        assert_eq!(record["totals"]["reported_cost_turns"], 0);
    } else {
        assert_eq!(record["totals"]["cache_creation_input_tokens"], 5 * turns);
        assert_eq!(
            record["totals"]["cost_microusd"],
            1000 * turns,
            "Claude cumulative session cost must not be summed again after resume"
        );
        assert_eq!(record["cost_available"], true);
        assert_eq!(record["cost_basis"], "api_equivalent");
        assert_eq!(record["totals"]["reported_cost_turns"], turns);
    }
}

fn assert_codex_reported_usage(log: &Path, index: usize, conversation_turns: u64) {
    let raw: Value = serde_json::from_slice(
        &std::fs::read(log.join(format!("turn-{index}.reported-usage.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        raw,
        json!({
            "input_tokens":100*conversation_turns,
            "cached_input_tokens":20*conversation_turns,
            "output_tokens":30*conversation_turns
        }),
        "pinned Codex fixture must emit whole-conversation counters, not per-call deltas"
    );
}

struct Host {
    child: OwnedChild,
    rpc: RpcClient,
    workspace: PathBuf,
    url: String,
    agent: String,
    token: String,
    _lines: mpsc::Receiver<String>,
}
impl Host {
    fn start(base: &Path) -> Self {
        let worlds = base.join("worlds");
        std::fs::create_dir(&worlds).unwrap();
        let mut preset: Value = serde_json::from_str(include_str!(
            "../../../content/testbed_content/worlds/island_village.json"
        ))
        .unwrap();
        preset["ai_scheduler"] = json!({"enabled":true,"events_enabled":true,"idle_seconds":5,"cooldown_seconds":5,"poll_seconds":1,"max_nudges_per_hour":60,"runner_timeout_seconds":10,"max_concurrent":1,"skip_paused":true});
        std::fs::write(
            worlds.join("fast.json"),
            serde_json::to_vec(&preset).unwrap(),
        )
        .unwrap();
        let home = base.join("agents");
        let mut command = Command::new(env!("CARGO_BIN_EXE_npcctl"));
        command
            .args(["serve", "--explorer-home"])
            .arg(&home)
            .arg("--actors-dir")
            .arg(base.join("actors"))
            .arg("--worlds-dir")
            .arg(&worlds)
            .args([
                "--agent",
                "Wren",
                "--world",
                "Island",
                "--world-preset",
                "fast",
                "--preset",
                "human",
                "--port",
                "0",
            ]);
        quiet(&mut command);
        let mut child = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("owned explorer host"),
        );
        let stdout = child.child().stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let ready: Value = serde_json::from_str(
            &rx.recv_timeout(Duration::from_secs(180))
                .expect("owned explorer readiness"),
        )
        .unwrap();
        let url = ready["listening"].as_str().expect("host URL").to_owned();
        let workspace = home.join("wren").canonicalize().unwrap();
        let meta: Value =
            serde_json::from_slice(&std::fs::read(workspace.join("agent.json")).unwrap()).unwrap();
        Self {
            child,
            rpc: RpcClient::new(&url).unwrap(),
            workspace,
            url,
            agent: meta["agent_id"].as_str().unwrap().into(),
            token: meta["token"].as_str().unwrap().into(),
            _lines: rx,
        }
    }
    fn stop(self) {
        assert_eq!(
            self.rpc
                .call("session/stop", json!({}))
                .expect("stop owned explorer")["stopped"],
            true
        );
        assert!(self.child.finish(Duration::from_secs(30)).status.success());
    }
    fn command(&self, provider: &str, mode: &str, log: &Path) -> Command {
        self.command_with_poll(provider, mode, log, "100")
    }
    fn command_with_poll(
        &self,
        provider: &str,
        mode: &str,
        log: &Path,
        poll_millis: &str,
    ) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
        command
            .args(["run", "--workspace"])
            .arg(&self.workspace)
            .args(["--provider", provider, "--program"])
            .arg(fixture_program())
            .args(["--poll-millis", poll_millis, "--turn-timeout-seconds", "10"]);
        command
            .env_remove("NPC_AI_METRICS_DIR")
            .env_remove("NPC_AI_LOGS_DIR")
            .env_remove("NPC_AI_FIXTURE_MODEL_CASE")
            .env("NPC_AI_FIXTURE_PROVIDER", provider)
            .env("NPC_AI_FIXTURE_MODE", mode)
            .env("NPC_AI_FIXTURE_ROOT", log)
            .env("NPC_AI_FIXTURE_URL", &self.url)
            .env("NPC_AI_FIXTURE_NPCCTL", env!("CARGO_BIN_EXE_npcctl"))
            .env("NPC_AI_FIXTURE_AGENT", &self.agent)
            .env("NPC_AI_FIXTURE_TOKEN", &self.token);
        quiet(&mut command);
        command
    }
    fn status(&self) -> Value {
        std::fs::read(self.workspace.join(".ai-client/status.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null)
    }
    fn control(&self, verb: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
        command.args([verb, "--workspace"]).arg(&self.workspace);
        quiet(&mut command);
        command.output().expect("client control command")
    }
    fn private_output(&self, output: &Output) {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for private in [
            &self.token,
            SESSION,
            FOREIGN_SESSION,
            "private-provider-transcript-marker",
            "private-provider-stderr-marker",
            CONFIG_CANARY,
        ] {
            assert!(
                !combined.contains(private),
                "public output leaked private provider data"
            );
        }
        let status = self.status().to_string();
        assert!(
            !status.contains(&self.token)
                && !status.contains(SESSION)
                && !status.contains(FOREIGN_SESSION),
            "status leaked private identity"
        );
        assert!(
            !status.contains("private-provider-transcript-marker")
                && !status.contains("private-provider-stderr-marker")
                && !status.contains(CONFIG_CANARY),
            "public status retained raw provider output"
        );
    }
}

fn assert_codex_selection(argv: &str, model: &str, effort: &str) {
    let args: Vec<_> = argv.lines().collect();
    let model_position = args
        .windows(2)
        .position(|pair| pair == ["--model", model])
        .expect("each owned Codex turn must receive the selected model explicitly");
    let effort_position = args
        .windows(2)
        .position(|pair| {
            pair[0] == "-c"
                && pair[1]
                    .strip_prefix("model_reasoning_effort=")
                    .is_some_and(|value| value.trim_matches('"') == effort)
        })
        .expect(
            "each owned Codex turn must receive the selected effort as a process-only override",
        );
    let operation_position = if let Some(resume) = args.iter().position(|arg| *arg == "resume") {
        assert_eq!(
            args.get(resume + 1),
            Some(&SESSION),
            "resume must name the exact owned session before its process options"
        );
        resume + 1
    } else {
        args.iter()
            .position(|arg| *arg == "exec")
            .expect("initial Codex turn uses exec")
    };
    let stdin_position = args
        .iter()
        .rposition(|arg| *arg == "-")
        .expect("prompt comes from stdin");
    assert!(
        operation_position < model_position && model_position + 1 < stdin_position,
        "model selection must follow exec or resume UUID and precede the final stdin prompt marker"
    );
    assert!(
        operation_position < effort_position && effort_position + 1 < stdin_position,
        "effort selection must follow exec or resume UUID and precede the final stdin prompt marker"
    );
}

fn assert_game_approvals(argv: &str, enabled: bool) {
    let args: Vec<_> = argv.lines().collect();
    let mut approval_options: Vec<_> = args
        .windows(2)
        .filter(|pair| {
            pair[0] == "-c"
                && pair[1]
                    .split('=')
                    .next()
                    .is_some_and(|key| key.contains("approval"))
        })
        .map(|pair| pair[1].to_owned())
        .collect();
    approval_options.sort();
    let mut expected: Vec<_> = if enabled {
        [
            "register",
            "enter_world",
            "take_control",
            "release",
            "observe",
            "act",
            "act_and_wait",
            "talk",
            "cancel",
            "events",
            "notes_list",
            "notes_add",
            "notes_update",
            "map_list",
            "map_add",
            "map_update",
        ]
        .iter()
        .map(|tool| format!("mcp_servers.npc.tools.{tool}.approval_mode=\"approve\""))
        .collect()
    } else {
        Vec::new()
    };
    expected.sort();
    assert_eq!(
        approval_options, expected,
        "approval intent must be confined to exactly the sixteen registered game tools, and absent by default"
    );
    for forbidden in [
        "approval_policy",
        "sandbox_mode",
        "trust_level",
        "--dangerously-bypass-approvals-and-sandbox",
        "--full-auto",
        "default_tools_approval_mode",
    ] {
        assert!(
            !argv.contains(forbidden),
            "game approval intent must not alter shell, sandbox, project trust, server defaults or global approval policy"
        );
    }
}

#[test]
fn explicit_game_approval_is_scoped_to_registered_tools_on_new_and_exact_resumed_codex_turns() {
    let fixture = Disposable::new("game-tool-approval");
    let host = Host::start(&fixture.0);
    let workspace_config = host.workspace.join(".codex/config.toml");
    let workspace_before = std::fs::read(&workspace_config).unwrap();
    let owned_global = fixture.0.join("owned fake codex home");
    std::fs::create_dir(&owned_global).unwrap();
    let global_config = owned_global.join("config.toml");
    let global_before = format!("# {CONFIG_CANARY}\napproval_policy = \"on-request\"\n");
    std::fs::write(&global_config, &global_before).unwrap();
    let mut command = host.command("codex", "success", &fixture.0);
    command.env("CODEX_HOME", &owned_global).args([
        "--approve-game-tools",
        "--max-turns",
        "2",
        "--max-runtime-seconds",
        "40",
    ]);
    let output = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(50));
    host.private_output(&output);
    assert!(output.status.success());
    assert_eq!(host.status()["turns_completed"], 2);
    for turn in 1..=2 {
        let argv = std::fs::read_to_string(fixture.0.join(format!("turn-{turn}.argv"))).unwrap();
        assert_codex_selection(&argv, "gpt-fixture", "medium");
        assert_game_approvals(&argv, true);
        assert_eq!(
            argv.contains(SESSION),
            turn == 2,
            "approval intent must not change exact conversation selection"
        );
    }
    assert_eq!(std::fs::read(&workspace_config).unwrap(), workspace_before);
    assert_eq!(
        std::fs::read(&global_config).unwrap(),
        global_before.as_bytes()
    );
    let claude_log = fixture.0.join("claude-refused");
    std::fs::create_dir(&claude_log).unwrap();
    let mut claude = host.command("claude", "success", &claude_log);
    claude.env(
        "NPC_AI_FIXTURE_PROCESS_MARKER",
        claude_log.join("provider-started"),
    );
    claude.args(["--approve-game-tools", "--max-runtime-seconds", "10"]);
    let output = OwnedChild::new(
        claude
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(20));
    host.private_output(&output);
    assert!(
        !output.status.success(),
        "Codex-specific approval intent is rejected for Claude"
    );
    for marker in [
        "provider-started",
        "turns",
        "turn-1.pid",
        "metadata-1.requests",
        "mcp-ok",
    ] {
        assert!(
            !claude_log.join(marker).exists(),
            "unsupported approval intent must be rejected before provider dispatch"
        );
    }
    host.stop();
}

#[test]
fn explicit_restricted_game_policy_is_not_overridden_by_opt_in() {
    let fixture = Disposable::new("restricted-game-policy");
    let host = Host::start(&fixture.0);
    let workspace_config = host.workspace.join(".codex/config.toml");
    let workspace_before = std::fs::read(&workspace_config).unwrap();
    let owned_global = fixture.0.join("owned fake codex home");
    std::fs::create_dir(&owned_global).unwrap();
    let global_config = owned_global.join("config.toml");
    for (case, approve, restriction) in [
        (
            "policy-server-prompt",
            true,
            "[mcp_servers.npc]\ndefault_tools_approval_mode = \"prompt\"\n",
        ),
        (
            "policy-tool-writes",
            true,
            "[mcp_servers.npc.tools.register]\napproval_mode = \"writes\"\n",
        ),
        (
            "policy-server-disabled",
            true,
            "[mcp_servers.npc]\nenabled = false\n",
        ),
        (
            "policy-server-disabled",
            false,
            "[mcp_servers.npc]\nenabled = false\n",
        ),
    ] {
        let log = fixture.0.join(format!("{case}-{approve}"));
        std::fs::create_dir(&log).unwrap();
        let global_before = format!("# {CONFIG_CANARY}\n{restriction}");
        std::fs::write(&global_config, &global_before).unwrap();
        let mut command = host.command("codex", "success", &log);
        command
            .env("CODEX_HOME", &owned_global)
            .env("NPC_AI_FIXTURE_MODEL_CASE", case)
            .args(["--max-turns", "1", "--max-runtime-seconds", "10"]);
        if approve {
            command.arg("--approve-game-tools");
        }
        let mut child = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let pid = child.child().id();
        let output = child.finish(Duration::from_secs(20));
        host.private_output(&output);
        assert!(
            !output.status.success(),
            "explicit operator restrictions must stop the client before a turn receives approval grants"
        );
        let public = host.status();
        assert_eq!(
            public["pid"], pid,
            "refusal status belongs to this client attempt"
        );
        assert_eq!(public["last_failure_code"], "provider_config");
        assert_eq!(public["turns_completed"], 0);
        assert!(
            log.join("metadata-1.requests").is_file(),
            "guard must inspect effective CLI metadata"
        );
        for marker in ["turns", "turn-1.pid", "turn-1.argv", "mcp-ok"] {
            assert!(
                !log.join(marker).exists(),
                "restricted game policy must not reach an approval-granted turn or game mutation"
            );
        }
        assert_eq!(
            std::fs::read(&global_config).unwrap(),
            global_before.as_bytes(),
            "operator policy bytes are preserved"
        );
        assert_eq!(
            std::fs::read(&workspace_config).unwrap(),
            workspace_before,
            "workspace configuration bytes are preserved"
        );
        assert!(
            host.rpc.call("session/info", json!({})).is_ok(),
            "game remains available after policy refusal"
        );
    }
    host.stop();
}

#[test]
fn queued_game_tool_cancellation_is_fatal_during_host_identity_reconnect_cleanup() {
    let fixture = Disposable::new("cancelled-reconnect");
    let host = Host::start(&fixture.0);
    let mut command =
        host.command_with_poll("codex", "game-tool-cancelled-barrier", &fixture.0, "5000");
    command.args(["--max-turns", "2", "--max-runtime-seconds", "25"]);
    let mut child = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_until(
        Duration::from_secs(15),
        || {
            assert!(
                child.child().try_wait().unwrap().is_none(),
                "client exited before the owned cancellation barrier"
            );
            fixture.0.join("cancel-barrier-ready").is_file() && host.status()["state"] == "running"
        },
        "owned provider did not reach its cancellation barrier",
    );
    let status = host.status();
    assert_eq!(status["turns_completed"], 0);
    assert_eq!(status["failures"], 0);
    assert_eq!(
        status["session_ready"], false,
        "session notification must still be queued before any ordinary provider poll"
    );
    let dir = testbed_content::explorer::home::AgentDir {
        dir: host.workspace.clone(),
    };
    let original = testbed_content::explorer::host::RunFile::read_any(&dir).unwrap();
    let mut changed = original.clone();
    changed.started = if original.started == "2000-01-01T00:00:00Z" {
        "2001-01-01T00:00:00Z"
    } else {
        "2000-01-01T00:00:00Z"
    }
    .into();
    changed.write(&dir).unwrap();
    assert_eq!(
        testbed_content::explorer::host::RunFile::read_any(&dir).unwrap(),
        changed,
        "same live game remains reconnectable with a different run identity"
    );
    assert!(host.rpc.call("session/info", json!({})).is_ok());
    // Change identity before permitting output: the next loop must enter reconnect cleanup,
    // which appears before its normal provider poll, and discover the queued fatal outcome there.
    std::fs::write(fixture.0.join("release-cancel"), "emit queued cancellation").unwrap();
    wait_until(
        Duration::from_secs(3),
        || {
            fixture.0.join("turn-1.done").is_file()
                && fixture.0.join("cancel-held-after-terminal").is_file()
        },
        "provider did not emit its queued cancellation and accounting",
    );
    assert_eq!(
        host.status()["failures"],
        0,
        "ordinary polling must not discover the cancellation before the reconnect cleanup regression"
    );
    wait_until(
        Duration::from_secs(10),
        || {
            assert!(
                !fixture.0.join("turn-2.pid").exists(),
                "reconnect cleanup must not launch another paid provider turn after a queued cancellation"
            );
            child.child().try_wait().unwrap().is_some()
        },
        "fatal reconnect cleanup did not stop the owned client",
    );
    let output = child.finish(Duration::from_secs(5));
    host.private_output(&output);
    assert!(!output.status.success());
    let status = host.status();
    assert_eq!(status["last_failure_code"], "game_tool_cancelled");
    assert_eq!(status["turns_completed"], 0);
    assert_eq!(status["failures"], 1);
    assert_eq!(status["session_ready"], true);
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("turns"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!fixture.0.join("turn-2.argv").exists());
    let provider_pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(!npc_framework::agent_io::scheduler::process_alive(
        provider_pid
    ));
    let saved: Value = serde_json::from_slice(
        &std::fs::read(host.workspace.join(".ai-client/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["session"], SESSION,
        "cleanup must retain the exact privately learned session before propagating fatal cancellation"
    );
    let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["turns_started"], 1);
    assert_eq!(records[0]["turns_completed"], 0);
    assert_eq!(records[0]["turns_failed"], 1);
    assert_eq!(records[0]["turns_interrupted"], 0);
    assert_eq!(records[0]["totals"]["input_tokens"], 100);
    assert_eq!(records[0]["totals"]["output_tokens"], 30);
    assert_eq!(records[0]["totals"]["cached_input_tokens"], 20);
    assert_eq!(records[0]["history"][0]["outcome"], "failed");
    let owner = "cancelled-reconnect-successor-0123456789";
    assert_eq!(
        host.rpc
            .call(
                "client/claim",
                json!({"controller":host.agent,"token":host.token,"owner":owner})
            )
            .unwrap()["claimed"],
        true,
        "fatal reconnect cleanup must release host ownership"
    );
    host.rpc
        .call(
            "client/release",
            json!({"controller":host.agent,"owner":owner}),
        )
        .unwrap();
    original.write(&dir).unwrap();
    host.stop();
}

#[test]
fn queued_foreign_session_is_fatal_during_reconnect_cleanup_without_misattributing_its_usage() {
    for (mode, expected) in [
        (
            "game-tool-cancelled-identity-barrier",
            "game_tool_cancelled",
        ),
        ("identity-conflict-barrier", "unknown"),
    ] {
        let fixture = Disposable::new(mode);
        let host = Host::start(&fixture.0);
        // Establish A and its real cumulative baseline through the ordinary fake-provider
        // flow. Never invent a private ledger schema to turn adopted history into zero.
        let mut baseline = host.command("codex", "success", &fixture.0);
        baseline.args(["--max-turns", "1", "--max-runtime-seconds", "20"]);
        let output = OwnedChild::new(
            baseline
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(30));
        host.private_output(&output);
        assert!(output.status.success());
        let private = host.workspace.join(".ai-client");
        let original_state: Value =
            serde_json::from_slice(&std::fs::read(private.join("state.json")).unwrap()).unwrap();
        assert_eq!(original_state["session"], SESSION);
        let unit = original_state["unit"].as_u64().unwrap();
        let before_fault = metrics_records(&private.join("metrics"), &host);
        assert_eq!(before_fault.len(), 1);
        assert_metrics_totals(&before_fault[0], "codex", 1);
        assert_codex_reported_usage(&fixture.0, 1, 1);
        let mut command = host.command_with_poll("codex", mode, &fixture.0, "5000");
        command.args(["--max-turns", "2", "--max-runtime-seconds", "25"]);
        let mut child = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        wait_until(
            Duration::from_secs(15),
            || {
                assert!(
                    child.child().try_wait().unwrap().is_none(),
                    "client exited before the synchronized identity fault"
                );
                fixture.0.join("cancel-barrier-ready").is_file()
                    && host.status()["state"] == "running"
            },
            "owned provider did not reach its identity-fault barrier",
        );
        let first_argv = std::fs::read_to_string(fixture.0.join("turn-2.argv")).unwrap();
        assert!(
            first_argv.contains(SESSION) && !first_argv.contains(FOREIGN_SESSION),
            "the single authorized fault attempt resumes original conversation A"
        );
        assert_eq!(host.status()["turns_completed"], 0);
        assert_eq!(host.status()["failures"], 0);
        let dir = testbed_content::explorer::home::AgentDir {
            dir: host.workspace.clone(),
        };
        let original = testbed_content::explorer::host::RunFile::read_any(&dir).unwrap();
        let mut changed = original.clone();
        changed.started = if original.started == "2000-01-01T00:00:00Z" {
            "2001-01-01T00:00:00Z"
        } else {
            "2000-01-01T00:00:00Z"
        }
        .into();
        changed.write(&dir).unwrap();
        assert_eq!(
            testbed_content::explorer::host::RunFile::read_any(&dir).unwrap(),
            changed
        );
        assert!(host.rpc.call("session/info", json!({})).is_ok());
        // The client checks run identity before normal polling. Release B only after
        // that identity differs, so stop_tracked_turn must discover the queued fault.
        std::fs::write(
            fixture.0.join("release-cancel"),
            "emit foreign identity and usage",
        )
        .unwrap();
        wait_until(
            Duration::from_secs(3),
            || {
                fixture.0.join("turn-2.done").is_file()
                    && fixture.0.join("cancel-held-after-terminal").is_file()
            },
            "foreign identity fixture did not queue its terminal and hold for cleanup",
        );
        assert_eq!(
            host.status()["failures"],
            0,
            "the fault must remain queued until reconnect cleanup"
        );
        wait_until(
            Duration::from_secs(10),
            || {
                assert!(
                    !fixture.0.join("turn-3.pid").exists(),
                    "identity-fault cleanup must not automatically dispatch another provider turn"
                );
                child.child().try_wait().unwrap().is_some()
            },
            "fatal identity cleanup did not stop the client",
        );
        let output = child.finish(Duration::from_secs(5));
        host.private_output(&output);
        assert!(
            !output.status.success(),
            "provider identity faults require explicit operator recovery"
        );
        let public = host.status();
        assert_eq!(public["last_failure_code"], expected);
        assert_eq!(public["turns_completed"], 0);
        assert_eq!(public["failures"], 1);
        assert_eq!(public["session_ready"], true);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("turns"))
                .unwrap()
                .lines()
                .count(),
            2
        );
        assert!(!fixture.0.join("turn-3.argv").exists());
        let provider_pid: u32 = std::fs::read_to_string(fixture.0.join("turn-2.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            !npc_framework::agent_io::scheduler::process_alive(provider_pid),
            "owned provider held after terminal must be reaped"
        );
        let saved: Value =
            serde_json::from_slice(&std::fs::read(private.join("state.json")).unwrap()).unwrap();
        assert_eq!(
            saved["session"], SESSION,
            "foreign identity B must never replace private conversation A"
        );
        assert_eq!(saved["unit"], unit);
        let records = metrics_records(&private.join("metrics"), &host);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record["metrics_id"], before_fault[0]["metrics_id"]);
        assert_eq!(record["turns_started"], 2);
        assert_eq!(record["turns_completed"], 1);
        assert_eq!(record["turns_failed"], 1);
        assert_eq!(
            record["turns_interrupted"], 0,
            "an identity fault is failed even when discovered during cleanup"
        );
        assert_eq!(record["history"][0]["outcome"], "completed");
        assert_eq!(record["history"][1]["outcome"], "failed");
        assert_codex_reported_usage(&fixture.0, 2, 2);
        assert_eq!(
            record["totals"]["input_tokens"], 200,
            "retain only A usage, never B's distinct 9000001 input tokens"
        );
        assert_eq!(
            record["totals"]["cached_input_tokens"], 40,
            "foreign cached usage must not be attributed to A"
        );
        assert_eq!(
            record["totals"]["output_tokens"], 60,
            "foreign output usage must not be attributed to A"
        );
        assert_eq!(record["totals"]["reported_input_turns"], 2);
        assert!(record["totals"]["cost_microusd"].is_null());
        let successor = "identity-fault-successor-owner-0123456789";
        assert_eq!(
            host.rpc
                .call(
                    "client/claim",
                    json!({"controller":host.agent,"token":host.token,"owner":successor})
                )
                .unwrap()["claimed"],
            true,
            "fatal identity fault releases its host lease"
        );
        host.rpc
            .call(
                "client/release",
                json!({"controller":host.agent,"owner":successor}),
            )
            .unwrap();
        original.write(&dir).unwrap();
        host.stop();
    }
}

#[test]
fn replacement_registration_between_polls_cannot_hide_an_owned_mcp_revocation() {
    for provider in ["codex", "claude"] {
        let fixture = Disposable::new(&format!("revocation-replacement-{provider}"));
        let host = Host::start(&fixture.0);
        let mut command = host.command(provider, "revoked-between-polls", &fixture.0);
        command.args(["--max-turns", "2", "--max-runtime-seconds", "20"]);
        let client = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        wait_until(
            Duration::from_secs(15),
            || {
                fixture.0.join("registered-ready").is_file()
                    && host.status()["actor_controlled"] == true
            },
            "owned MCP registration did not acquire real actor control",
        );
        // Register atomically revokes A and installs B, so no host poll can see
        // an unauthenticated gap. A's existing MCP connection must still fail.
        let replacement = host
            .rpc
            .call(
                "agent/register",
                json!({"agent_id":host.agent,"token":host.token}),
            )
            .unwrap();
        assert!(
            replacement["session"]
                .as_str()
                .is_some_and(|key| !key.is_empty())
        );
        std::fs::write(
            fixture.0.join("use-revoked-session"),
            "release owned fixture",
        )
        .unwrap();
        let output = client.finish(Duration::from_secs(15));
        host.private_output(&output);
        assert!(
            !output.status.success(),
            "an actual revoked game tool must remain fatal after provider success"
        );
        let status = host.status();
        assert_eq!(status["last_failure_code"], "game_access_revoked");
        assert_eq!(status["turns_completed"], 0);
        assert_eq!(status["failures"], 1);
        assert_eq!(status["session_ready"], true);
        assert_eq!(status["connected"], false);
        assert_eq!(status["actor_controlled"], false);
        assert!(
            !fixture.0.join("turn-2.pid").exists(),
            "revocation must not trigger another CLI turn"
        );
        let saved: Value = serde_json::from_slice(
            &std::fs::read(host.workspace.join(".ai-client/state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["session"], SESSION);
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["turns_failed"], 1);
        assert_eq!(records[0]["turns_completed"], 0);
        assert_eq!(
            records[0]["totals"]["input_tokens"],
            if provider == "codex" { 100 } else { 125 }
        );
        assert_eq!(records[0]["totals"]["output_tokens"], 30);
        let pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(!npc_framework::agent_io::scheduler::process_alive(pid));
        // The replacement remained authorized; only A was revoked.
        assert!(
            host.rpc
                .call(
                    "agent/enter_world",
                    json!({"session":replacement["session"]})
                )
                .is_ok()
        );
        host.stop();
    }
}

#[test]
fn connected_game_and_saved_provider_session_do_not_claim_actor_control_before_attachment() {
    let fixture = Disposable::new("actor-control-phases");
    let host = Host::start(&fixture.0);
    let mut command = host.command("codex", "attachment-barrier", &fixture.0);
    command.args(["--max-runtime-seconds", "20"]);
    let client = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_until(
        Duration::from_secs(15),
        || fixture.0.join("registration-wait").is_file() && host.status()["session_ready"] == true,
        "provider session was not saved before game registration",
    );
    let before = host.status();
    assert_eq!(before["connected"], true);
    assert_eq!(before["session_ready"], true);
    assert_eq!(before["actor_controlled"], false);
    std::fs::write(
        fixture.0.join("allow-registration"),
        "release owned fixture",
    )
    .unwrap();
    wait_until(
        Duration::from_secs(10),
        || fixture.0.join("mcp-ok").is_file() && host.status()["actor_controlled"] == true,
        "actual registered control was not reflected in status",
    );
    assert_eq!(host.status()["connected"], true);
    std::fs::write(
        host.workspace.join(".ai-client/stop"),
        "cooperative owned stop",
    )
    .unwrap();
    let output = client.finish(Duration::from_secs(15));
    host.private_output(&output);
    assert!(output.status.success());
    assert_eq!(host.status()["actor_controlled"], false);
    assert_eq!(host.status()["connected"], false);
    assert_eq!(host.status()["session_ready"], true);
    host.stop();
}

#[test]
fn claude_required_game_setup_and_permissions_fail_closed_despite_success_result() {
    for (mode, expected) in [
        ("game-tools-unavailable", "game_tools_unavailable"),
        ("game-permission-denied", "game_permission_denied"),
    ] {
        let fixture = Disposable::new(mode);
        let host = Host::start(&fixture.0);
        let mut command = host.command("claude", mode, &fixture.0);
        command.args(["--max-turns", "2", "--max-runtime-seconds", "20"]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(30));
        host.private_output(&output);
        assert!(!output.status.success());
        let status = host.status();
        assert_eq!(status["last_failure_code"], expected);
        assert_eq!(status["turns_completed"], 0);
        assert_eq!(status["failures"], 1);
        assert_eq!(status["session_ready"], true);
        assert_eq!(status["actor_controlled"], false);
        assert!(!fixture.0.join("turn-2.pid").exists());
        assert!(!fixture.0.join("mcp-ok").exists());
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["turns_failed"], 1);
        assert_eq!(records[0]["totals"]["input_tokens"], 125);
        assert_eq!(records[0]["totals"]["output_tokens"], 30);
        host.stop();
    }
}

#[test]
fn cancelled_registered_game_tool_stops_without_retry_or_false_success_and_keeps_usage_private() {
    let fixture = Disposable::new("cancelled-game-tool");
    let host = Host::start(&fixture.0);
    let mut command = host.command("codex", "game-tool-cancelled", &fixture.0);
    command.args(["--max-turns", "2", "--max-runtime-seconds", "20"]);
    let output = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(30));
    host.private_output(&output);
    assert!(
        !output.status.success(),
        "cancelled game bootstrap must stop for explicit operator recovery"
    );
    let status = host.status();
    assert_eq!(status["last_failure_code"], "game_tool_cancelled");
    assert_eq!(
        status["turns_completed"], 0,
        "later provider turn.completed cannot turn a cancelled game tool into success"
    );
    assert_eq!(status["failures"], 1);
    assert_eq!(
        status["session_ready"], true,
        "provider identity remains privately resumable"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("turns"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(
        !fixture.0.join("turn-2.pid").exists(),
        "a cancelled tool must never trigger an automatic retry"
    );
    assert!(
        !fixture.0.join("mcp-ok").exists(),
        "cancelled bootstrap did not execute game mutations"
    );
    let provider_pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        !npc_framework::agent_io::scheduler::process_alive(provider_pid),
        "owned provider is stopped before returning the failure"
    );
    let saved: Value = serde_json::from_slice(
        &std::fs::read(host.workspace.join(".ai-client/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["session"], SESSION);
    let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["turns_started"], 1);
    assert_eq!(records[0]["turns_completed"], 0);
    assert_eq!(records[0]["turns_failed"], 1);
    assert_eq!(records[0]["totals"]["input_tokens"], 100);
    assert_eq!(records[0]["totals"]["output_tokens"], 30);
    assert_eq!(records[0]["totals"]["cached_input_tokens"], 20);
    assert_eq!(records[0]["history"][0]["outcome"], "failed");
    assert!(records[0]["totals"]["cost_microusd"].is_null());
    assert!(host.rpc.call("session/info", json!({})).is_ok());
    let owner = "cancelled-game-tool-successor-0123456789";
    assert_eq!(
        host.rpc
            .call(
                "client/claim",
                json!({"controller":host.agent,"token":host.token,"owner":owner})
            )
            .unwrap()["claimed"],
        true,
        "fatal cancellation must release the host lease"
    );
    host.rpc
        .call(
            "client/release",
            json!({"controller":host.agent,"owner":owner}),
        )
        .unwrap();
    host.stop();
}

fn assert_metadata_probe(log: &Path, workspace: &Path) {
    let requests: Vec<_> = std::fs::read_dir(log)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "requests")
        })
        .collect();
    assert!(
        !requests.is_empty(),
        "model selection must precede a provider turn"
    );
    for path in requests {
        let calls: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let methods: Vec<_> = calls
            .iter()
            .map(|call| call["method"].as_str().unwrap())
            .collect();
        assert_eq!(
            methods,
            ["initialize", "initialized", "config/read", "model/list"]
        );
        assert_eq!(calls[2]["params"]["includeLayers"], false);
        assert_eq!(
            PathBuf::from(calls[2]["params"]["cwd"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            workspace,
            "effective config must be read for the same absolute game workspace"
        );
        assert_eq!(
            calls[3]["params"],
            json!({"limit":100,"includeHidden":false})
        );
        assert_eq!(
            PathBuf::from(std::fs::read_to_string(path.with_extension("cwd")).unwrap())
                .canonicalize()
                .unwrap(),
            workspace,
            "metadata process itself must run in the game workspace"
        );
        assert_eq!(
            std::fs::read_to_string(path.with_extension("environment")).unwrap(),
            "subscription_only"
        );
    }
}

#[test]
fn codex_pre_turn_selection_preserves_configured_choice_or_uses_only_advertised_defaults() {
    for (case, model, effort, model_source, effort_source, turns) in [
        (
            "configured-visible",
            "gpt-fixture-configured",
            "high",
            "cli_config",
            "cli_config",
            2,
        ),
        (
            "configured-unsupported-effort",
            "gpt-fixture-configured",
            "medium",
            "cli_config",
            "advertised_default",
            1,
        ),
        (
            "fallback-high",
            "gpt-fixture-default",
            "high",
            "advertised_default",
            "cli_config",
            1,
        ),
        (
            "fallback-effort",
            "gpt-fixture-default",
            "medium",
            "advertised_default",
            "advertised_default",
            1,
        ),
        (
            "no-config",
            "gpt-fixture-default",
            "low",
            "advertised_default",
            "advertised_default",
            1,
        ),
    ] {
        let fixture = Disposable::new(case);
        let host = Host::start(&fixture.0);
        let global = fixture.0.join("owned fake codex home");
        std::fs::create_dir(&global).unwrap();
        let global_path = global.join("config.toml");
        let global_bytes = format!("# {CONFIG_CANARY}\nmodel = \"gpt-fixture-operator-file\"\n");
        std::fs::write(&global_path, &global_bytes).unwrap();
        let workspace_path = host.workspace.join(".codex/config.toml");
        let mut workspace_bytes = std::fs::read(&workspace_path).unwrap();
        workspace_bytes.extend_from_slice(format!("\n# {CONFIG_CANARY}\n").as_bytes());
        std::fs::write(&workspace_path, &workspace_bytes).unwrap();
        let mut command = host.command("codex", "success", &fixture.0);
        command
            .env("NPC_AI_FIXTURE_MODEL_CASE", case)
            .env("CODEX_HOME", &global)
            .env("OPENAI_API_KEY", CONFIG_CANARY)
            .env("CODEX_API_KEY", CONFIG_CANARY)
            .env("CODEX_ACCESS_TOKEN", CONFIG_CANARY)
            .env("ANTHROPIC_API_KEY", CONFIG_CANARY)
            .env("ANTHROPIC_AUTH_TOKEN", CONFIG_CANARY)
            .env("CLAUDE_CODE_USE_BEDROCK", "1")
            .env("CLAUDE_CODE_USE_VERTEX", "1")
            .env("CLAUDE_CODE_USE_FOUNDRY", "1")
            .args([
                "--max-turns",
                &turns.to_string(),
                "--max-runtime-seconds",
                "35",
            ]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(45));
        host.private_output(&output);
        assert!(
            output.status.success(),
            "valid advertised selection must play without a failed trial turn"
        );
        assert_eq!(host.status()["turns_completed"], turns);
        assert_eq!(host.status()["failures"], 0);
        assert_metadata_probe(&fixture.0, &host.workspace);
        assert_eq!(
            std::fs::read(&global_path).unwrap(),
            global_bytes.as_bytes(),
            "selection must not edit global config"
        );
        assert_eq!(
            std::fs::read(&workspace_path).unwrap(),
            workspace_bytes,
            "selection must not edit existing workspace config"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("turns"))
                .unwrap()
                .lines()
                .count(),
            turns as usize
        );
        let first = std::fs::read_to_string(fixture.0.join("turn-1.argv")).unwrap();
        assert_codex_selection(&first, model, effort);
        assert!(
            !first.contains(SESSION),
            "first turn must create a new conversation"
        );
        if turns == 2 {
            let resumed = std::fs::read_to_string(fixture.0.join("turn-2.argv")).unwrap();
            assert_codex_selection(&resumed, model, effort);
            assert!(
                resumed.contains(SESSION) && !resumed.contains("--last"),
                "selection must preserve exact-session continuation"
            );
        }
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0]["selection_intent"],
            json!({
                "requested_model":model,"requested_effort":effort,
                "model_source":model_source,"effort_source":effort_source
            })
        );
        assert_eq!(
            records[0]["model"], "gpt-fixture",
            "requested model cannot replace actual provider metadata"
        );
        assert_eq!(
            records[0]["effort"], "medium",
            "requested effort cannot replace actual provider metadata"
        );
        assert_eq!(records[0]["metadata_source"], "provider_reported");
        host.stop();
    }
}

#[test]
fn codex_uncertain_model_metadata_fails_closed_before_any_provider_turn() {
    let fixture = Disposable::new("closed-model-catalog");
    let host = Host::start(&fixture.0);
    for case in [
        "hidden",
        "ambiguous",
        "none",
        "paginated",
        "malformed",
        "malformed-config",
        "unavailable",
        "duplicate",
        "invalid-default-effort",
    ] {
        let log = fixture.0.join(case);
        std::fs::create_dir(&log).unwrap();
        let mut command = host.command("codex", "success", &log);
        command
            .env("NPC_AI_FIXTURE_MODEL_CASE", case)
            .args(["--max-runtime-seconds", "10"]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(20));
        host.private_output(&output);
        assert!(
            !output.status.success(),
            "uncertain model availability must not dispatch a trial turn"
        );
        assert!(
            !log.join("turn-1.pid").exists()
                && !log.join("turns").exists()
                && !log.join("mcp-ok").exists(),
            "metadata refusal must not enter the game or start a model turn"
        );
        assert!(
            host.rpc.call("session/info", json!({})).is_ok(),
            "metadata refusal leaves the game running"
        );
    }
    host.stop();
}

#[test]
fn workspace_preflight_reports_selection_without_starting_a_thread_turn_or_game_tools() {
    let fixture = Disposable::new("model-preflight");
    let workspace = fixture.0.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    for (case, expected) in [
        (
            "configured-visible",
            Some(("gpt-fixture-configured", "high", "cli_config", "cli_config")),
        ),
        (
            "fallback-high",
            Some((
                "gpt-fixture-default",
                "high",
                "advertised_default",
                "cli_config",
            )),
        ),
        (
            "fallback-effort",
            Some((
                "gpt-fixture-default",
                "medium",
                "advertised_default",
                "advertised_default",
            )),
        ),
        ("hidden", None),
        ("ambiguous", None),
        ("none", None),
        ("paginated", None),
        ("malformed", None),
        ("malformed-config", None),
        ("unavailable", None),
    ] {
        let log = fixture.0.join(case);
        std::fs::create_dir(&log).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
        command
            .args(["check", "--provider", "codex", "--program"])
            .arg(fixture_program())
            .arg("--workspace")
            .arg(&workspace)
            .env("NPC_AI_FIXTURE_PROVIDER", "codex")
            .env("NPC_AI_FIXTURE_MODE", "success")
            .env("NPC_AI_FIXTURE_MODEL_CASE", case)
            .env("NPC_AI_FIXTURE_ROOT", &log);
        quiet(&mut command);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(20));
        assert!(
            output.status.success(),
            "preflight refusal is structured readiness, consistent with login-only checks"
        );
        let public = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !public.contains(CONFIG_CANARY) && !public.contains(SESSION),
            "raw CLI configuration and conversation identity must never enter preflight output"
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ready"], expected.is_some());
        assert_eq!(report["model_selection_ready"], expected.is_some());
        if let Some((model, effort, model_source, effort_source)) = expected {
            assert_eq!(
                report["selection_intent"],
                json!({
                    "requested_model":model,"requested_effort":effort,
                    "model_source":model_source,"effort_source":effort_source
                })
            );
            assert_metadata_probe(&log, &workspace);
        } else {
            assert!(report["selection_intent"].is_null());
        }
        for marker in ["turn-1.pid", "turns", "mcp-ok"] {
            assert!(
                !log.join(marker).exists(),
                "preflight must not create a game/model turn"
            );
        }
        assert!(
            !workspace.join(".ai-client").exists(),
            "preflight must not create client session state or metrics"
        );
    }
    for (provider, with_workspace) in [("codex", false), ("claude", true)] {
        let log = fixture.0.join(format!("login-only-{provider}"));
        std::fs::create_dir(&log).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
        command
            .args(["check", "--provider", provider, "--program"])
            .arg(fixture_program())
            .env("NPC_AI_FIXTURE_PROVIDER", provider)
            .env("NPC_AI_FIXTURE_MODE", "success")
            .env("NPC_AI_FIXTURE_ROOT", &log);
        if with_workspace {
            command.arg("--workspace").arg(&workspace);
        }
        quiet(&mut command);
        let output = command.output().unwrap();
        assert!(output.status.success());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ready"], true);
        assert!(report.get("selection_intent").is_none_or(Value::is_null));
        assert!(
            !log.join("metadata-1.requests").exists(),
            "login-only and Claude checks must not contact Codex app-server"
        );
        assert!(!log.join("turns").exists());
    }
}

fn rehearse_provider(provider: &str) {
    let fixture = Disposable::new(provider);
    let host = Host::start(&fixture.0);
    let metrics_dir = fixture.0.join("export metrics");
    let ignored_metrics_dir = fixture.0.join("environment overridden by explicit flag");
    let mut command = host.command(provider, "success", &fixture.0);
    command.env("NPC_AI_METRICS_DIR", &ignored_metrics_dir);
    command
        .args([
            "--max-turns",
            "2",
            "--max-runtime-seconds",
            "40",
            "--metrics-dir",
        ])
        .arg(&metrics_dir);
    let child = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = child.finish(Duration::from_secs(50));
    host.private_output(&output);
    assert!(output.status.success(), "dedicated client rehearsal failed");
    assert_eq!(
        host.status()["turns_completed"],
        2,
        "both initial and resumed turns must finish"
    );
    let logs_dir = host.workspace.join(".ai-client/logs");
    let first_logs: Vec<_> = std::fs::read_dir(&logs_dir)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(first_logs.len(), 1, "one Markdown log per client run");
    let transcript = std::fs::read_to_string(first_logs[0].path()).unwrap();
    for expected in [
        "# AI agent game client session",
        "## Turn prompt",
        "Historical game notifications",
        "action_rejected",
        "I observed the world and spoke to my neighbours.",
        "Owned fixture checking its game tools.",
        "## CLI stdout",
        "## Turn finished",
        "reported_usage",
        "## Client finished",
    ] {
        assert!(
            transcript.contains(expected),
            "missing transcript content: {expected}"
        );
    }
    assert_eq!(transcript.matches("## Turn prompt —").count(), 2);
    for secret in [&host.token, SESSION, FOREIGN_SESSION] {
        assert!(!transcript.contains(secret));
    }
    let first_bytes = std::fs::read(first_logs[0].path()).unwrap();
    let before_restart = metrics_records(&metrics_dir, &host);
    assert_eq!(
        before_restart.len(),
        1,
        "initial and exact resumed turns must share one local session record"
    );
    assert_metrics_totals(&before_restart[0], provider, 2);
    if provider == "codex" {
        assert_codex_reported_usage(&fixture.0, 1, 1);
        assert_codex_reported_usage(&fixture.0, 2, 2);
        assert_eq!(
            before_restart[0]["history"][1]["reported_scope"],
            "session_cumulative"
        );
        assert_eq!(
            before_restart[0]["history"][1]["usage"]["input_tokens"], 100,
            "second process accounts only the new cumulative delta"
        );
    }
    assert!(
        !ignored_metrics_dir.exists(),
        "explicit --metrics-dir must take precedence over the environment default"
    );
    assert!(
        fixture.0.join("mcp-ok").is_file(),
        "provider must exercise real registered game tools"
    );
    let first = std::fs::read_to_string(fixture.0.join("turn-1.argv")).unwrap();
    let second = std::fs::read_to_string(fixture.0.join("turn-2.argv")).unwrap();
    assert_game_approvals(&first, false);
    assert_game_approvals(&second, false);
    if provider == "codex" {
        assert_codex_selection(&first, "gpt-fixture", "medium");
        assert_codex_selection(&second, "gpt-fixture", "medium");
        assert_metadata_probe(&fixture.0, &host.workspace);
    }
    assert!(!first.contains(SESSION) && !first.contains("--last"));
    assert!(
        second.contains(SESSION) && !second.contains("--last"),
        "resume must identify the exact owned session"
    );
    let initial = std::fs::read_to_string(fixture.0.join("turn-1.prompt")).unwrap();
    let continuation = std::fs::read_to_string(fixture.0.join("turn-2.prompt")).unwrap();
    assert!(
        initial.contains("AGENTS.md"),
        "initial mission must be supplied automatically"
    );
    assert!(
        continuation.contains("action_rejected"),
        "a relevant game event must reach the next provider turn"
    );
    assert!(
        !continuation.contains("fixture_unknown_verb"),
        "closed event summary must omit raw arbitrary payloads"
    );
    assert!(
        !initial.contains(&host.token) && !continuation.contains(&host.token),
        "registration secret must stay out of prompts"
    );
    assert!(
        host.rpc.call("session/info", json!({})).is_ok(),
        "client completion must leave game alive"
    );
    let output = host.control("status");
    host.private_output(&output);
    assert!(output.status.success());
    let mut restart = host.command(provider, "success", &fixture.0);
    let moved_metrics_dir = fixture.0.join("moved metrics");
    restart
        .args([
            "--max-turns",
            "1",
            "--max-runtime-seconds",
            "20",
            "--metrics-dir",
        ])
        .arg(&moved_metrics_dir);
    let output = OwnedChild::new(
        restart
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(30));
    host.private_output(&output);
    assert!(
        output.status.success(),
        "restart must resume persisted private session"
    );
    let restarted = std::fs::read_to_string(fixture.0.join("turn-3.argv")).unwrap();
    assert_game_approvals(&restarted, false);
    if provider == "codex" {
        assert_codex_selection(&restarted, "gpt-fixture", "medium");
    }
    assert!(
        restarted.contains(SESSION) && !restarted.contains("--last"),
        "client restart must retain exact session ownership"
    );
    let after_restart = metrics_records(&moved_metrics_dir, &host);
    let restarted_logs: Vec<_> = std::fs::read_dir(&logs_dir)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        restarted_logs.len(),
        2,
        "restart creates an independent client-run log"
    );
    assert_eq!(
        std::fs::read(first_logs[0].path()).unwrap(),
        first_bytes,
        "restart must preserve the previous transcript"
    );
    for entry in &restarted_logs {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        assert!(
            text.contains(before_restart[0]["metrics_id"].as_str().unwrap()),
            "each run links the same conversation metrics"
        );
    }
    assert_eq!(
        after_restart.len(),
        1,
        "restart must update the same metrics record"
    );
    assert_eq!(
        after_restart[0]["metrics_id"],
        before_restart[0]["metrics_id"]
    );
    assert_metrics_totals(&after_restart[0], provider, 3);
    if provider == "codex" {
        assert_codex_reported_usage(&fixture.0, 3, 3);
        assert_eq!(
            after_restart[0]["history"][2]["usage"]["input_tokens"], 100,
            "persisted baseline survives a client-process restart"
        );
    }
    assert_eq!(
        after_restart[0]["history"].as_array().unwrap().len(),
        3,
        "repeated cumulative events or restart must not duplicate committed turns"
    );
    assert_eq!(after_restart[0]["metadata_source"], "provider_reported");
    if provider == "codex" {
        assert_eq!(after_restart[0]["model"], "gpt-fixture-changed");
        assert_eq!(after_restart[0]["effort"], "high");
        assert_eq!(
            after_restart[0]["history"][0]["usage"]["model"],
            "gpt-fixture"
        );
        assert_eq!(after_restart[0]["history"][0]["usage"]["effort"], "medium");
    } else {
        assert_eq!(after_restart[0]["model"], "claude-fake-model");
        assert!(
            after_restart[0]["effort"].is_null(),
            "missing Claude effort cannot be inferred from this chat or a provider default"
        );
    }
    let mut fresh = host.command(provider, "success", &fixture.0);
    let custom_logs = fixture.0.join("private Markdown logs");
    let ignored_logs = fixture.0.join("overridden environment logs");
    fresh.env("NPC_AI_LOGS_DIR", &ignored_logs);
    fresh.arg("--logs-dir").arg(&custom_logs);
    fresh
        .args([
            "--new-session",
            "--max-turns",
            "1",
            "--max-runtime-seconds",
            "20",
            "--metrics-dir",
        ])
        .arg(&moved_metrics_dir);
    let output = OwnedChild::new(
        fresh
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(30));
    host.private_output(&output);
    assert!(
        output.status.success(),
        "a new conversation must start independently of previous metrics"
    );
    let separate_sessions = metrics_records(&moved_metrics_dir, &host);
    assert_eq!(
        separate_sessions.len(),
        2,
        "new session must have an independent local metrics record"
    );
    let previous = separate_sessions
        .iter()
        .find(|record| record["metrics_id"] == after_restart[0]["metrics_id"])
        .unwrap();
    let new = separate_sessions
        .iter()
        .find(|record| record["metrics_id"] != after_restart[0]["metrics_id"])
        .unwrap();
    assert_metrics_totals(previous, provider, 3);
    assert_metrics_totals(new, provider, 1);
    let custom_entries: Vec<_> = std::fs::read_dir(&custom_logs)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(custom_entries.len(), 1);
    assert_eq!(custom_entries[0].path().extension().unwrap(), "md");
    let fresh_text = std::fs::read_to_string(custom_entries[0].path()).unwrap();
    assert!(fresh_text.contains(new["metrics_id"].as_str().unwrap()));
    assert!(!fresh_text.contains(previous["metrics_id"].as_str().unwrap()));
    assert!(
        !ignored_logs.exists(),
        "explicit log folder overrides environment"
    );
    assert_eq!(std::fs::read_dir(&logs_dir).unwrap().count(), 2);
    if provider == "codex" {
        assert_codex_reported_usage(&fixture.0, 4, 1);
        assert_eq!(
            new["history"][0]["usage"]["input_tokens"], 100,
            "a fresh conversation starts its own zero baseline"
        );
    }
    host.stop();
}

#[test]
fn codex_client_creates_and_resumes_its_exact_session_through_real_game_tools() {
    rehearse_provider("codex");
}

#[test]
fn claude_client_creates_and_resumes_its_exact_session_through_real_game_tools() {
    rehearse_provider("claude");
}

#[test]
fn unwritable_markdown_destination_stops_before_any_provider_probe_or_turn() {
    let fixture = Disposable::new("logs-unavailable");
    let host = Host::start(&fixture.0);
    let destination = fixture.0.join("not-a-folder");
    std::fs::write(&destination, "owned blocker").unwrap();
    let mut command = host.command("codex", "success", &fixture.0);
    command.env("NPC_AI_LOGS_DIR", &destination).env(
        "NPC_AI_FIXTURE_PROCESS_MARKER",
        fixture.0.join("provider-started"),
    );
    let output = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(10));
    assert!(!output.status.success());
    host.private_output(&output);
    assert!(!fixture.0.join("provider-started").exists());
    assert!(!host.workspace.join(".ai-client/state.json").exists());
    assert_eq!(host.status()["state"], "error");
    assert!(
        host.status()["message"]
            .as_str()
            .unwrap()
            .contains("--logs-dir")
    );
    assert!(host.rpc.call("session/info", json!({})).is_ok());
    host.stop();
}

#[test]
fn duplicate_client_is_refused_and_stop_reaps_only_owned_provider_and_releases_host() {
    let fixture = Disposable::new("stop");
    let host = Host::start(&fixture.0);
    let mut command = host.command("codex", "hold", &fixture.0);
    command.args(["--max-runtime-seconds", "30"]);
    let child = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_until(
        Duration::from_secs(20),
        || fixture.0.join("turn-1.pid").is_file(),
        "owned provider did not start",
    );
    let provider_pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
        .unwrap()
        .parse()
        .unwrap();
    let mut duplicate = host.command("codex", "hold", &fixture.0);
    duplicate.args(["--max-runtime-seconds", "2"]);
    let duplicate = duplicate.output().unwrap();
    host.private_output(&duplicate);
    assert!(!duplicate.status.success(), "second client must be refused");
    assert!(
        !fixture.0.join("turn-2.pid").exists(),
        "duplicate must not launch a provider"
    );
    let stop = host.control("stop");
    host.private_output(&stop);
    assert!(stop.status.success(), "stop signal accepted");
    let output = child.finish(Duration::from_secs(10));
    host.private_output(&output);
    assert!(output.status.success());
    assert!(
        !npc_framework::agent_io::scheduler::process_alive(provider_pid),
        "owned provider must be reaped"
    );
    assert!(
        host.rpc.call("session/info", json!({})).is_ok(),
        "stopping client must preserve game"
    );
    let claimed = host
        .rpc
        .call(
            "client/claim",
            json!({"controller":host.agent,"token":host.token,"owner":"owned-fixture-successor-0123456789"}),
        )
        .expect("client stop released host lease");
    assert_eq!(claimed["claimed"], true);
    host.rpc
        .call(
            "client/release",
            json!({"controller":host.agent,"owner":"owned-fixture-successor-0123456789"}),
        )
        .unwrap();
    host.stop();
}

#[test]
fn provider_failure_and_malformed_output_remain_bounded_and_private() {
    for mode in ["fail", "malformed"] {
        let fixture = Disposable::new(mode);
        let host = Host::start(&fixture.0);
        let mut command = host.command("codex", mode, &fixture.0);
        command.args(["--max-runtime-seconds", "8"]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(20));
        host.private_output(&output);
        assert!(
            host.status()["failures"].as_u64().unwrap_or(0) > 0,
            "provider protocol/nonzero failure must be visible"
        );
        assert_eq!(
            host.status()["turns_completed"],
            0,
            "failed provider cannot count as completed turn"
        );
        assert_eq!(
            host.status()["last_failure_code"],
            if mode == "fail" {
                "nonzero_exit"
            } else {
                "missing_terminal"
            },
            "provider fallback diagnostics must remain closed"
        );
        assert!(
            !fixture.0.join("turn-2.pid").exists(),
            "failure backoff prevents a launch storm"
        );
        assert!(host.rpc.call("session/info", json!({})).is_ok());
        host.stop();
    }
}

#[test]
fn closed_provider_failure_categories_survive_stop_without_leaking_structured_or_stderr_text() {
    for provider in ["codex", "claude"] {
        let fixture = Disposable::new(&format!("diagnostics-{provider}"));
        let host = Host::start(&fixture.0);
        for (mode, expected) in [
            ("diagnostic-auth", "authentication"),
            ("diagnostic-rate", "rate_limit"),
            ("diagnostic-mcp", "mcp_startup"),
            ("diagnostic-unknown", "unknown"),
            ("diagnostic-code-secret", "unknown"),
            ("stderr-auth", "authentication"),
            ("stderr-rate", "rate_limit"),
            ("stderr-mcp", "mcp_startup"),
            ("stderr-unknown", "nonzero_exit"),
        ] {
            let log = fixture.0.join(mode);
            std::fs::create_dir(&log).unwrap();
            let mut command = host.command(provider, mode, &log);
            command.args(["--max-runtime-seconds", "20"]);
            let mut child = OwnedChild::new(
                command
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let pid = child.child().id();
            wait_until(
                Duration::from_secs(15),
                || {
                    let status = host.status();
                    if status["pid"] == pid && status["failures"].as_u64().unwrap_or(0) > 0 {
                        return true;
                    }
                    if let Some(exit) = child.child().try_wait().unwrap() {
                        // Re-read after exit so a final atomic status publication
                        // between the first read and try_wait is not missed.
                        let final_status = host.status();
                        assert!(
                            final_status["pid"] == pid
                                && final_status["failures"].as_u64().unwrap_or(0) > 0,
                            "{provider}/{mode}: client exited before recording a diagnostic; exit code {:?}",
                            exit.code()
                        );
                        return true;
                    }
                    false
                },
                &format!(
                    "{provider}/{mode}: provider diagnostic was not recorded before its retry backoff"
                ),
            );
            let stop = host.control("stop");
            host.private_output(&stop);
            assert!(stop.status.success());
            let output = child.finish(Duration::from_secs(10));
            host.private_output(&output);
            assert!(output.status.success());
            let status = host.status();
            assert_eq!(status["state"], "stopped");
            assert_eq!(
                status["failures"], 1,
                "{provider}/{mode}: stop must precede any automatic retry"
            );
            assert_eq!(
                status["last_failure_code"], expected,
                "{provider}/{mode}: failure diagnostics must be closed and survive Stop"
            );
            assert_eq!(status["turns_completed"], 0);
            assert!(
                !log.join("turn-2.pid").exists(),
                "{provider}/{mode}: diagnostic regression must not retry a provider"
            );
            let public = host.control("status");
            host.private_output(&public);
            assert_eq!(
                serde_json::from_slice::<Value>(&public.stdout).unwrap()["last_failure_code"],
                expected
            );
        }
        host.stop();
    }
}

#[test]
fn public_status_without_optional_failure_category_remains_backward_compatible() {
    let fixture = Disposable::new("old-status");
    let private = fixture.0.join(".ai-client");
    std::fs::create_dir(&private).unwrap();
    std::fs::write(private.join("status.json"),serde_json::to_vec(&json!({
        "version":1,"pid":0,"provider":"codex","state":"stopped","message":"Earlier client status",
        "turns_completed":0,"failures":1,"session_ready":false,"connected":false,"updated_unix_seconds":0
    })).unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
    command.args(["status", "--workspace"]).arg(&fixture.0);
    quiet(&mut command);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "earlier status schema must remain readable"
    );
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["failures"], 1);
    assert!(
        status.get("last_failure_code").is_none(),
        "no diagnostic may be invented for old status"
    );
}

#[test]
fn preflight_rejects_signed_out_and_api_key_authentication_without_a_model_turn() {
    let fixture = Disposable::new("auth");
    for provider in ["codex", "claude"] {
        for mode in ["signed-out", "api-auth"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ai-agent-game-client"));
            command
                .args(["check", "--provider", provider, "--program"])
                .arg(fixture_program())
                .env("NPC_AI_FIXTURE_PROVIDER", provider)
                .env("NPC_AI_FIXTURE_MODE", mode)
                .env("NPC_AI_FIXTURE_ROOT", &fixture.0);
            quiet(&mut command);
            let output = command.output().unwrap();
            let report: Value =
                serde_json::from_slice(&output.stdout).expect("sanitized preflight JSON");
            assert_eq!(
                report["ready"], false,
                "subscription preflight must reject unavailable subscription login"
            );
            assert!(
                !fixture.0.join("turn-1.pid").exists(),
                "preflight must not dispatch a model turn"
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn crashed_client_reaps_its_provider_and_restart_resumes_the_persisted_session() {
    let fixture = Disposable::new("crash");
    let host = Host::start(&fixture.0);
    let mut command = host.command("codex", "hold", &fixture.0);
    command.args(["--max-runtime-seconds", "30"]);
    let mut client = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_until(
        Duration::from_secs(20),
        || host.status()["session_ready"] == true && fixture.0.join("turn-1.pid").is_file(),
        "client did not persist the announced provider session",
    );
    let provider_pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
        .unwrap()
        .parse()
        .unwrap();
    // Simulate a client crash, deliberately without killing its process tree.
    client.child().kill().unwrap();
    let output = client.finish(Duration::from_secs(5));
    host.private_output(&output);
    let logs_dir = host.workspace.join(".ai-client/logs");
    let crashed_log = std::fs::read_dir(&logs_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let crashed_text = std::fs::read_to_string(&crashed_log).unwrap();
    assert!(crashed_text.contains("## Turn prompt"));
    assert!(
        !crashed_text.contains("## Client finished"),
        "a crash must not invent a graceful ending"
    );
    wait_until(
        Duration::from_secs(10),
        || !npc_framework::agent_io::scheduler::process_alive(provider_pid),
        "client crash left its owned provider running",
    );
    assert!(
        host.rpc.call("session/info", json!({})).is_ok(),
        "client crash must preserve the game"
    );
    let mut restart = host.command("codex", "success", &fixture.0);
    // The crashed owner's host lease expires after 30 seconds. Recovery must wait for it.
    restart.args(["--max-turns", "1", "--max-runtime-seconds", "50"]);
    let output = OwnedChild::new(
        restart
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(60));
    host.private_output(&output);
    assert!(
        output.status.success(),
        "restart must recover an OS-released client lock"
    );
    let resumed = std::fs::read_to_string(fixture.0.join("turn-2.argv")).unwrap();
    assert!(
        resumed.contains(SESSION),
        "crash recovery must retain exact provider session"
    );
    assert_eq!(host.status()["turns_completed"], 1);
    assert_eq!(std::fs::read_to_string(crashed_log).unwrap(), crashed_text);
    assert_eq!(std::fs::read_dir(logs_dir).unwrap().count(), 2);
    host.stop();
}

#[test]
fn explicit_new_session_archives_invalid_private_state_and_bootstraps_automatically() {
    let fixture = Disposable::new("new-session");
    let host = Host::start(&fixture.0);
    let private = host.workspace.join(".ai-client");
    std::fs::create_dir(&private).unwrap();
    let old = b"private-invalid-state-that-must-be-preserved";
    std::fs::write(private.join("state.json"), old).unwrap();
    let mut command = host.command("codex", "success", &fixture.0);
    command.args([
        "--new-session",
        "--max-turns",
        "1",
        "--max-runtime-seconds",
        "20",
    ]);
    let output = OwnedChild::new(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
    .finish(Duration::from_secs(30));
    host.private_output(&output);
    assert!(
        output.status.success(),
        "explicit new session must recover invalid previous provider state"
    );
    assert_eq!(host.status()["turns_completed"], 1);
    let argv = std::fs::read_to_string(fixture.0.join("turn-1.argv")).unwrap();
    assert!(
        !argv.contains("resume") && !argv.contains(SESSION),
        "fresh provider session must not resume invalid state"
    );
    let archives: Vec<_> = std::fs::read_dir(&private)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("state.previous.")
        })
        .collect();
    assert_eq!(
        archives.len(),
        1,
        "old private state must be archived exactly once"
    );
    assert_eq!(std::fs::read(archives[0].path()).unwrap(), old);
    assert!(host.rpc.call("session/info", json!({})).is_ok());
    host.stop();
}

#[test]
fn configured_metrics_environment_exports_unknown_usage_and_metadata_without_inventing_zero() {
    for provider in ["codex", "claude"] {
        let fixture = Disposable::new(&format!("metrics-missing-{provider}"));
        let host = Host::start(&fixture.0);
        let export = fixture.0.join("operator metrics environment");
        let mut command = host.command(provider, "metrics-missing", &fixture.0);
        command
            .args(["--max-turns", "1", "--max-runtime-seconds", "20"])
            .env("NPC_AI_METRICS_DIR", &export);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(30));
        host.private_output(&output);
        assert!(output.status.success());
        let records = metrics_records(&export, &host);
        assert_eq!(
            records.len(),
            1,
            "environment default must write an actual JSON session record"
        );
        let record = &records[0];
        assert_eq!(record["turns_started"], 1);
        assert_eq!(record["turns_completed"], 1);
        for key in [
            "input_tokens",
            "output_tokens",
            "cached_input_tokens",
            "cache_creation_input_tokens",
            "cost_microusd",
        ] {
            assert!(
                record["totals"][key].is_null(),
                "missing provider usage/cost must be unknown rather than zero"
            );
        }
        assert_eq!(record["totals"]["reported_input_turns"], 0);
        assert_eq!(record["totals"]["reported_output_turns"], 0);
        assert_eq!(record["totals"]["reported_cost_turns"], 0);
        assert!(
            record["model"].is_null() && record["effort"].is_null(),
            "missing metadata must not copy the current chat's model or infer a provider default"
        );
        assert_eq!(record["metadata_source"], "unknown");
        if provider == "codex" {
            assert_eq!(
                record["selection_intent"],
                json!({
                    "requested_model":"gpt-fixture","requested_effort":"medium",
                    "model_source":"cli_config","effort_source":"cli_config"
                }),
                "pre-turn selection intent must remain separate when actual execution metadata is absent"
            );
        }
        assert_eq!(record["cost_available"], false);
        assert_eq!(record["cost_basis"], "unknown");
        host.stop();
    }
}

#[test]
fn failed_turn_metrics_retain_reported_usage_and_cost_without_counting_success() {
    for provider in ["codex", "claude"] {
        let fixture = Disposable::new(&format!("metrics-failed-{provider}"));
        let host = Host::start(&fixture.0);
        let mut first = host.command(provider, "success", &fixture.0);
        first.args(["--max-turns", "1", "--max-runtime-seconds", "20"]);
        let output = OwnedChild::new(
            first
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(30));
        host.private_output(&output);
        assert!(output.status.success());
        let export = host.workspace.join(".ai-client/metrics");
        let previous = metrics_records(&export, &host);
        assert_eq!(previous.len(), 1);
        assert_metrics_totals(&previous[0], provider, 1);
        let mut failing = host.command(provider, "metrics-failed", &fixture.0);
        failing.args(["--max-runtime-seconds", "20"]);
        let mut child = OwnedChild::new(
            failing
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let pid = child.child().id();
        wait_until(
            Duration::from_secs(15),
            || {
                let status = host.status();
                status["pid"] == pid && status["failures"].as_u64().unwrap_or(0) > 0
            },
            "failed provider usage was not recorded before backoff",
        );
        assert!(host.control("stop").status.success());
        let output = child.finish(Duration::from_secs(10));
        host.private_output(&output);
        assert!(output.status.success());
        let records = metrics_records(&export, &host);
        assert_eq!(
            records.len(),
            1,
            "failed resume must retain its existing session metrics"
        );
        let record = &records[0];
        assert_eq!(record["metrics_id"], previous[0]["metrics_id"]);
        assert_eq!(record["turns_started"], 2);
        assert_eq!(record["turns_completed"], 1);
        assert_eq!(record["turns_failed"], 1);
        assert_eq!(
            record["totals"]["input_tokens"],
            if provider == "codex" { 200 } else { 250 }
        );
        assert_eq!(record["totals"]["output_tokens"], 60);
        assert_eq!(record["totals"]["cached_input_tokens"], 40);
        assert_eq!(record["totals"]["reported_input_turns"], 2);
        assert_eq!(record["history"][1]["outcome"], "failed");
        if provider == "claude" {
            assert_eq!(
                record["totals"]["cost_microusd"], 2000,
                "failed-turn provider-reported cumulative cost must contribute only its new delta"
            );
            assert_eq!(record["cost_basis"], "api_equivalent");
        } else {
            assert!(record["totals"]["cost_microusd"].is_null());
            assert_eq!(record["cost_available"], false);
        }
        host.stop();
    }
}

#[test]
fn codex_unknown_versions_do_not_guess_cumulative_usage_scope() {
    for mode in ["metrics-unknown-version", "metrics-unparseable-version"] {
        let fixture = Disposable::new(mode);
        let host = Host::start(&fixture.0);
        let mut command = host.command("codex", mode, &fixture.0);
        command.args(["--max-turns", "1", "--max-runtime-seconds", "20"]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(30));
        host.private_output(&output);
        assert!(
            output.status.success(),
            "unverified token-accounting scope does not block ordinary gameplay"
        );
        assert_codex_reported_usage(&fixture.0, 1, 1);
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record["turns_completed"], 1);
        assert_eq!(record["turns_failed"], 0);
        for field in [
            "input_tokens",
            "output_tokens",
            "cached_input_tokens",
            "cache_creation_input_tokens",
            "cost_microusd",
        ] {
            assert!(
                record["totals"][field].is_null(),
                "unsupported or unknown Codex versions must not guess numeric usage scope"
            );
        }
        assert_eq!(record["totals"]["reported_input_turns"], 0);
        assert_eq!(record["totals"]["reported_output_turns"], 0);
        assert_eq!(record["totals"]["reported_cost_turns"], 0);
        assert_eq!(
            record["model"], "gpt-fixture",
            "unambiguous reported model is independent of accounting scope"
        );
        assert_eq!(record["effort"], "medium");
        assert_eq!(record["metadata_source"], "provider_reported");
        assert_eq!(record["cost_available"], false);
        assert_eq!(record["cost_basis"], "unknown");
        assert_eq!(record["selection_intent"]["requested_model"], "gpt-fixture");
        host.stop();
    }
}

#[test]
fn claude_version_selects_per_call_accounting_and_unknown_version_does_not_guess_usage() {
    for (mode, turns) in [("metrics-old", 2), ("metrics-unknown-version", 1)] {
        let fixture = Disposable::new(mode);
        let host = Host::start(&fixture.0);
        let mut command = host.command("claude", mode, &fixture.0);
        command.args([
            "--max-turns",
            &turns.to_string(),
            "--max-runtime-seconds",
            "30",
        ]);
        let output = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
        .finish(Duration::from_secs(40));
        host.private_output(&output);
        assert!(output.status.success());
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        if mode == "metrics-old" {
            assert_metrics_totals(&records[0], "claude", 2);
            assert_eq!(records[0]["history"][1]["reported_scope"], "turn");
        } else {
            for field in [
                "input_tokens",
                "output_tokens",
                "cached_input_tokens",
                "cache_creation_input_tokens",
                "cost_microusd",
            ] {
                assert!(
                    records[0]["totals"][field].is_null(),
                    "unknown Claude version must not guess usage scope or invent totals"
                );
            }
            assert_eq!(records[0]["cost_available"], false);
            assert_eq!(
                records[0]["model"], "claude-fake-model",
                "unambiguous provider model metadata remains available independently of accounting scope"
            );
        }
        host.stop();
    }
}

#[test]
fn stop_drains_a_queued_terminal_before_the_next_ordinary_poll_and_preserves_its_outcome() {
    for (mode, completed, failed, outcome) in [
        ("success", 1, 0, "completed"),
        ("metrics-failed", 0, 1, "failed"),
    ] {
        let fixture = Disposable::new(&format!("queued-stop-{mode}"));
        let host = Host::start(&fixture.0);
        let mut command = host.command_with_poll("codex", mode, &fixture.0, "5000");
        command.args(["--max-runtime-seconds", "25"]);
        let mut child = OwnedChild::new(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        wait_until(
            Duration::from_secs(15),
            || {
                let exit = child.child().try_wait().unwrap();
                assert!(
                    exit.is_none(),
                    "owned client exited before fixture terminal event; exit code {:?}",
                    exit.and_then(|status| status.code())
                );
                fixture.0.join("turn-1.done").is_file()
            },
            "fixture did not emit its terminal event",
        );
        let pid: u32 = std::fs::read_to_string(fixture.0.join("turn-1.pid"))
            .unwrap()
            .parse()
            .unwrap();
        wait_until(
            Duration::from_secs(3),
            || !npc_framework::agent_io::scheduler::process_alive(pid),
            "owned provider did not exit after its terminal event",
        );
        assert_eq!(
            host.status()["turns_completed"],
            0,
            "terminal must still be queued before this Stop regression"
        );
        assert_eq!(
            host.status()["failures"],
            0,
            "failure must still be queued before this Stop regression"
        );
        assert!(host.control("stop").status.success());
        let output = child.finish(Duration::from_secs(10));
        host.private_output(&output);
        assert!(output.status.success());
        assert_eq!(
            host.status()["turns_completed"],
            completed,
            "Stop cleanup must preserve a queued successful terminal"
        );
        assert_eq!(
            host.status()["failures"],
            failed,
            "Stop cleanup must preserve a queued failed terminal"
        );
        let state: Value = serde_json::from_slice(
            &std::fs::read(host.workspace.join(".ai-client/state.json")).unwrap(),
        )
        .unwrap();
        assert!(
            state["session"].as_str() == Some(SESSION),
            "Stop must retain the newly announced exact session even before the first ordinary poll"
        );
        let records = metrics_records(&host.workspace.join(".ai-client/metrics"), &host);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["turns_completed"], completed);
        assert_eq!(records[0]["turns_failed"], failed);
        assert_eq!(
            records[0]["turns_interrupted"], 0,
            "an already-exited terminal turn is not interrupted"
        );
        assert_eq!(records[0]["history"][0]["outcome"], outcome);
        assert_eq!(records[0]["totals"]["input_tokens"], 100);
        host.stop();
    }
}
