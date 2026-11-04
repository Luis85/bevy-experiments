//! Native, subscription-free provider fixture. Compiled by ai_client.rs with rustc.
//! It speaks the providers' public JSON protocols and exercises real registered MCP tools.
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const FOREIGN_SESSION: &str = "22222222-2222-4222-8222-222222222222";
const CONFIG_CANARY: &str = "private-config-canary-never-export";

fn var(name: &str) -> String {
    std::env::var(format!("NPC_AI_FIXTURE_{name}")).expect("fixture environment")
}
fn json_string(text: &str) -> String {
    let mut value = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => value.push_str("\\\""),
            '\\' => value.push_str("\\\\"),
            '\n' => value.push_str("\\n"),
            '\r' => value.push_str("\\r"),
            '\t' => value.push_str("\\t"),
            ch if ch < ' ' => value.push_str(&format!("\\u{:04x}", u32::from(ch))),
            ch => value.push(ch),
        }
    }
    value.push('"');
    value
}
fn barrier(root: &Path, ready: &str, release: &str) {
    fs::write(root.join(ready), "owned fixture barrier").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join(release).is_file() {
        assert!(
            Instant::now() < deadline,
            "owned game connection barrier expired"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn codex_cumulative_usage(root: &Path, index: usize, session_turn: usize) -> String {
    let usage = format!(
        "{{\"input_tokens\":{},\"cached_input_tokens\":{},\"output_tokens\":{}}}",
        100 * session_turn,
        20 * session_turn,
        30 * session_turn
    );
    fs::write(
        root.join(format!("turn-{index}.reported-usage.json")),
        &usage,
    )
    .unwrap();
    usage
}

fn subscription_environment() {
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
        assert!(std::env::var_os(name).is_none(), "subscription environment");
    }
}

fn json_field<'a>(line: &'a str, field: &str) -> &'a str {
    let key = format!("\"{field}\"");
    let tail = &line[line.find(&key).expect("metadata request field") + key.len()..];
    tail[tail.find(':').expect("metadata field separator") + 1..].trim_start()
}

fn advertised_model(name: &str, default: bool, hidden: bool, effort: &str, high: bool) -> String {
    let high = if high {
        ",{\"reasoningEffort\":\"high\",\"description\":\"Fixture high\"}"
    } else {
        ""
    };
    format!(
        "{{\"id\":\"{name}\",\"model\":\"{name}\",\"displayName\":\"Fixture model\",\"description\":\"Owned subscription-free fixture\",\"isDefault\":{default},\"hidden\":{hidden},\"defaultReasoningEffort\":\"{effort}\",\"supportedReasoningEfforts\":[{{\"reasoningEffort\":\"low\",\"description\":\"Fixture low\"}},{{\"reasoningEffort\":\"medium\",\"description\":\"Fixture medium\"}}{high}]}}"
    )
}

fn metadata() {
    subscription_environment();
    let root = PathBuf::from(var("ROOT"));
    let case = std::env::var("NPC_AI_FIXTURE_MODEL_CASE").unwrap_or_default();
    let index = (1..100)
        .find(|n| !root.join(format!("metadata-{n}.requests")).exists())
        .unwrap();
    fs::write(
        root.join(format!("metadata-{index}.cwd")),
        std::env::current_dir().unwrap().to_str().unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(format!("metadata-{index}.environment")),
        "subscription_only",
    )
    .unwrap();
    let mut requests = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(format!("metadata-{index}.requests")))
        .unwrap();
    if case == "unavailable" {
        eprintln!("{CONFIG_CANARY}");
        std::process::exit(9);
    }
    let (model, effort) = match case.as_str() {
        "configured-visible" => ("\"gpt-fixture-configured\"", "\"high\""),
        "configured-unsupported-effort" => ("\"gpt-fixture-configured\"", "\"ultra\""),
        "fallback-high" => ("\"gpt-fixture-unavailable\"", "\"high\""),
        "ambiguous" => ("\"gpt-fixture-unavailable\"", "\"medium\""),
        "fallback-effort" => ("\"gpt-fixture-unavailable\"", "\"ultra\""),
        "no-config" => ("null", "null"),
        "malformed-config" => ("123", "\"medium\""),
        _ => ("\"gpt-fixture\"", "\"medium\""),
    };
    let game_policy = match case.as_str() {
        "policy-server-prompt" => {
            ",\"mcp_servers\":{\"npc\":{\"default_tools_approval_mode\":\"prompt\"}}"
        }
        "policy-tool-writes" => {
            ",\"mcp_servers\":{\"npc\":{\"tools\":{\"register\":{\"approval_mode\":\"writes\"}}}}"
        }
        "policy-server-disabled" => ",\"mcp_servers\":{\"npc\":{\"enabled\":false}}",
        _ => "",
    };
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        writeln!(requests, "{line}").unwrap();
        requests.flush().unwrap();
        let method = json_field(&line, "method")
            .strip_prefix('"')
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        if method == "initialized" {
            continue;
        }
        let id = json_field(&line, "id")
            .split(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .next()
            .unwrap();
        match method {
            "initialize" => {
                println!("{{\"id\":{id},\"result\":{{\"userAgent\":\"owned-fixture\"}}}}")
            }
            "config/read" => println!(
                "{{\"id\":{id},\"result\":{{\"config\":{{\"model\":{model},\"model_reasoning_effort\":{effort},\"model_provider\":null,\"raw_config_canary\":\"{CONFIG_CANARY}\"{game_policy}}},\"origins\":{{}},\"layers\":null}}}}"
            ),
            "model/list" => {
                let ordinary = advertised_model("gpt-fixture", true, false, "medium", true);
                let default = advertised_model("gpt-fixture-default", true, false, "medium", true);
                let data = match case.as_str() {
                    "configured-visible" | "configured-unsupported-effort" => format!(
                        "{default},{}",
                        advertised_model("gpt-fixture-configured", false, false, "medium", true)
                    ),
                    "fallback-high" | "fallback-effort" => default,
                    "no-config" => {
                        advertised_model("gpt-fixture-default", true, false, "low", true)
                    }
                    "hidden" => advertised_model("gpt-fixture", true, true, "medium", true),
                    "ambiguous" => format!("{ordinary},{default}"),
                    "none" => String::new(),
                    "malformed" => "{\"model\":\"gpt-fixture\",\"isDefault\":true}".into(),
                    "invalid-default-effort" => {
                        advertised_model("gpt-fixture", true, false, "ultra", true)
                    }
                    "duplicate" => format!("{ordinary},{ordinary}"),
                    _ => ordinary,
                };
                let cursor = if case == "paginated" {
                    format!("\"{CONFIG_CANARY}\"")
                } else {
                    "null".into()
                };
                println!(
                    "{{\"id\":{id},\"result\":{{\"data\":[{data}],\"nextCursor\":{cursor}}}}}"
                );
                std::io::stdout().flush().unwrap();
                return;
            }
            _ => panic!("metadata must not create threads or turns"),
        }
        std::io::stdout().flush().unwrap();
    }
}

struct Mcp(std::process::Child);
impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn tools(index: usize, session_turn: usize) {
    let provider = var("PROVIDER");
    let mode = var("MODE");
    let root = PathBuf::from(var("ROOT"));
    let mut command = Command::new(var("NPCCTL"));
    command.args(["mcp", "--url", &var("URL"), "--registered"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = Mcp(command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap());
    let mut input = child.0.stdin.take().unwrap();
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut id = 0;
    let mut request = |name: &str, arguments: &str, successful: bool| {
        id += 1;
        // Test names, registration ids and tokens are generated ASCII strings.
        writeln!(input, "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"tools/call\",\"params\":{{\"name\":\"{name}\",\"arguments\":{arguments}}}}}").unwrap();
        input.flush().unwrap();
        let mut reply = String::new();
        assert_ne!(
            output.read_line(&mut reply).unwrap(),
            0,
            "MCP response ended"
        );
        if successful {
            assert!(
                !reply.contains("\"isError\":true"),
                "fixture game tool failed"
            );
            assert!(!reply.contains("\"error\":"), "fixture protocol error");
        } else {
            assert!(
                reply.contains("\"isError\":true"),
                "fixture expected a rejected action"
            );
        }
        // Emit the same public stream evidence a CLI exposes around the real
        // MCP exchange. The client must retain useful content but redact auth.
        if provider == "codex" {
            // Exec emits the MCP result, not its JSON-RPC envelope, and uses
            // snake_case for the optional structured_content field.
            let result = json_field(&reply, "result")
                .trim()
                .strip_suffix('}')
                .expect("MCP result envelope")
                .trim_end()
                .replace("\"structuredContent\":", "\"structured_content\":");
            println!(
                "{{\"type\":\"item.completed\",\"item\":{{\"id\":\"tool_{id}\",\"type\":\"mcp_tool_call\",\"server\":\"npc\",\"tool\":\"{name}\",\"arguments\":{arguments},\"result\":{result},\"status\":\"completed\"}}}}"
            );
        } else {
            println!(
                "{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"id\":\"tool_{id}\",\"name\":\"mcp__npc__{name}\",\"input\":{arguments}}}]}}}}"
            );
            println!(
                "{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"tool_{id}\",\"content\":{}}}]}}}}",
                json_string(reply.trim())
            );
        }
        if mode == "revoked-between-polls" && name == "observe" && !successful {
            if provider == "codex" {
                let usage = codex_cumulative_usage(&root, index, session_turn);
                println!("{{\"type\":\"turn.completed\",\"usage\":{usage}}}");
            } else {
                println!(
                    "{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"session_id\":\"{SESSION}\",\"modelUsage\":{{\"claude-fake-model\":{{\"inputTokens\":100,\"outputTokens\":30,\"cacheReadInputTokens\":20,\"cacheCreationInputTokens\":5,\"costUSD\":0.001}}}},\"total_cost_usd\":0.001}}"
                );
            }
            std::io::stdout().flush().unwrap();
        }
    };
    if mode == "attachment-barrier" {
        barrier(&root, "registration-wait", "allow-registration");
    }
    request(
        "register",
        &format!(
            "{{\"agent_id\":\"{}\",\"token\":\"{}\"}}",
            var("AGENT"),
            var("TOKEN")
        ),
        true,
    );
    request("enter_world", "{}", true);
    request("take_control", "{}", true);
    request("observe", "{}", true);
    if mode == "revoked-between-polls" {
        barrier(&root, "registered-ready", "use-revoked-session");
        request("observe", "{}", false);
        fs::write(
            root.join("revoked-tool-result"),
            "actual owned MCP error received",
        )
        .unwrap();
    }
    if mode == "revoked-between-polls" {
        drop(request);
        drop(input);
        assert!(
            child.0.wait().unwrap().success(),
            "revoked fixture MCP process failed"
        );
        return;
    }
    request(
        "talk",
        "{\"text\":\"Owned fixture checking its game tools.\"}",
        true,
    );
    // A known rejected command generates a closed, own-actor outcome for notification testing.
    request("act", "{\"verb\":\"fixture_unknown_verb\"}", false);
    drop(request);
    drop(input);
    assert!(
        child.0.wait().unwrap().success(),
        "fixture MCP process failed"
    );
    fs::write(
        PathBuf::from(var("ROOT")).join("mcp-ok"),
        "registered tools exercised",
    )
    .unwrap();
    if provider == "codex" {
        println!(
            "{{\"type\":\"item.completed\",\"item\":{{\"id\":\"reply\",\"type\":\"agent_message\",\"text\":\"I observed the world and spoke to my neighbours.\"}}}}"
        );
    } else {
        println!(
            "{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"I observed the world and spoke to my neighbours.\"}}]}}}}"
        );
    }
}

fn main() {
    if let Some(marker) = std::env::var_os("NPC_AI_FIXTURE_PROCESS_MARKER") {
        fs::write(marker, "owned fixture process started").unwrap();
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        let provider = std::env::var("NPC_AI_FIXTURE_PROVIDER").unwrap_or_else(|_| "codex".into());
        let mode = std::env::var("NPC_AI_FIXTURE_MODE").unwrap_or_default();
        if provider == "claude" {
            match mode.as_str() {
                "metrics-old" => println!("2.1.276 (Claude Code)"),
                "metrics-unknown-version" => println!("owned-native-fixture 1.0"),
                _ => println!("2.1.281 (Claude Code)"),
            }
        } else {
            match mode.as_str() {
                "metrics-unknown-version" => println!("codex-cli 0.146.0"),
                "metrics-unparseable-version" => println!("owned-native-fixture 1.0"),
                _ => println!("codex-cli 0.145.0"),
            }
        }
        return;
    }
    if args.first().map(String::as_str) == Some("login") {
        match std::env::var("NPC_AI_FIXTURE_MODE")
            .unwrap_or_default()
            .as_str()
        {
            "signed-out" => std::process::exit(1),
            "api-auth" => println!("Logged in using an API key"),
            _ => println!("Logged in using ChatGPT"),
        }
        return;
    }
    if args.first().map(String::as_str) == Some("auth") {
        match std::env::var("NPC_AI_FIXTURE_MODE")
            .unwrap_or_default()
            .as_str()
        {
            "signed-out" => println!("{{\"loggedIn\":false}}"),
            "api-auth" => println!("{{\"loggedIn\":true,\"authMethod\":\"api_key\"}}"),
            _ => println!("{{\"loggedIn\":true,\"authMethod\":\"claude.ai\"}}"),
        }
        return;
    }
    if args.first().map(String::as_str) == Some("app-server") {
        metadata();
        return;
    }
    subscription_environment();
    let mut prompt = String::new();
    std::io::stdin().read_to_string(&mut prompt).unwrap();
    assert!(
        !prompt.trim().is_empty(),
        "client must provide prompt stdin"
    );
    let root = PathBuf::from(var("ROOT"));
    let index = (1..100)
        .find(|n| !root.join(format!("turn-{n}.prompt")).exists())
        .unwrap();
    let resumed = args
        .iter()
        .any(|arg| matches!(arg.as_str(), "resume" | "--resume"));
    let session_turn =
        if resumed && std::env::var("NPC_AI_FIXTURE_MODE").ok().as_deref() != Some("metrics-old") {
            fs::read_to_string(root.join("session-turn"))
                .ok()
                .and_then(|text| text.parse::<usize>().ok())
                .unwrap_or(0)
                + 1
        } else {
            1
        };
    fs::write(root.join("session-turn"), session_turn.to_string()).unwrap();
    fs::write(root.join(format!("turn-{index}.prompt")), &prompt).unwrap();
    fs::write(root.join(format!("turn-{index}.argv")), args.join("\n")).unwrap();
    fs::write(
        root.join(format!("turn-{index}.pid")),
        std::process::id().to_string(),
    )
    .unwrap();
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("turns"))
        .unwrap()
        .write_all(b"turn\n")
        .unwrap();
    let provider = var("PROVIDER");
    let mode = var("MODE");
    if let Some(kind) = mode.strip_prefix("diagnostic-") {
        let code = match kind {
            "auth" => "authentication_error".into(),
            "rate" => "rate_limit_exceeded".into(),
            "mcp" => "mcp_startup_failed".into(),
            "code-secret" => format!("private-authentication_error-{}", var("TOKEN")),
            _ => "private_unrecognized_error".into(),
        };
        let message = format!(
            "private-provider-transcript-marker credentials={}",
            var("TOKEN")
        );
        if provider == "codex" {
            println!(
                "{{\"type\":\"turn.failed\",\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}"
            );
        } else {
            println!(
                "{{\"type\":\"result\",\"subtype\":\"error_during_execution\",\"is_error\":true,\"error\":{{\"code\":\"{code}\",\"message\":\"{message}\"}}}}"
            );
        }
        return;
    }
    if let Some(kind) = mode.strip_prefix("stderr-") {
        let phrase = match kind {
            "auth" => "authentication failed",
            "rate" => "rate limit exceeded",
            "mcp" => "required MCP server failed to initialize",
            _ => "private-provider-stderr-marker",
        };
        eprintln!(
            "{phrase}; private-provider-stderr-marker credentials={}",
            var("TOKEN")
        );
        std::process::exit(9);
    }
    if mode == "malformed" {
        println!("private-provider-transcript-marker: this is not JSON");
        eprintln!("private-provider-stderr-marker");
        return;
    }
    if mode == "fail" {
        eprintln!("private-provider-stderr-marker");
        std::process::exit(9);
    }
    let missing_metrics = mode == "metrics-missing";
    if provider == "codex" {
        if !missing_metrics {
            let (model, effort) = if index >= 3 {
                ("gpt-fixture-changed", "high")
            } else {
                ("gpt-fixture", "medium")
            };
            let version = match mode.as_str() {
                "metrics-unknown-version" => "0.146.0",
                "metrics-unparseable-version" => "fixture",
                _ => "0.145.0",
            };
            eprintln!(
                "OpenAI Codex v{version}\n--------\nmodel: {model}\nreasoning effort: {effort}\n--------"
            );
        }
        println!("{{\"type\":\"thread.started\",\"thread_id\":\"{SESSION}\"}}");
    } else {
        let game_fields = if mode == "game-tools-unavailable" {
            ",\"mcp_servers\":[{\"name\":\"npc\",\"status\":\"failed\"}],\"mcp_server_errors\":[{\"name\":\"npc\",\"type\":\"connection\",\"message\":\"private-provider-transcript-marker\"}]"
        } else {
            ""
        };
        if missing_metrics {
            println!(
                "{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"{SESSION}\"{game_fields}}}"
            );
        } else {
            println!(
                "{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"{SESSION}\",\"model\":\"claude-fake-model\"{game_fields}}}"
            );
        }
    }
    std::io::stdout().flush().unwrap();
    if mode == "hold" {
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let identity_conflict = matches!(
        mode.as_str(),
        "game-tool-cancelled-identity-barrier" | "identity-conflict-barrier"
    );
    let barrier = mode == "game-tool-cancelled-barrier" || identity_conflict;
    if identity_conflict {
        assert_eq!(provider, "codex", "owned Codex identity-fault fixture");
        let usage = codex_cumulative_usage(&root, index, session_turn);
        println!("{{\"type\":\"turn.completed\",\"usage\":{usage}}}");
        std::io::stdout().flush().unwrap();
    }
    if barrier {
        fs::write(
            root.join("cancel-barrier-ready"),
            "owned cancellation barrier",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("release-cancel").is_file() {
            assert!(
                Instant::now() < deadline,
                "owned cancellation barrier expired"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if matches!(
        mode.as_str(),
        "game-tool-cancelled"
            | "game-tool-cancelled-barrier"
            | "game-tool-cancelled-identity-barrier"
    ) {
        assert_eq!(provider, "codex", "owned Codex cancellation fixture");
        println!(
            "{{\"type\":\"item.completed\",\"item\":{{\"id\":\"fixture-cancelled-register\",\"type\":\"mcp_tool_call\",\"server\":\"npc\",\"tool\":\"register\",\"status\":\"failed\",\"arguments\":{{\"agent_id\":\"fixture\",\"token\":\"{}\",\"private_payload\":\"{CONFIG_CANARY} private-provider-transcript-marker\"}},\"result\":null,\"error\":{{\"message\":\"user cancelled MCP tool call\"}}}}}}",
            var("TOKEN")
        );
    } else if mode != "metrics-failed"
        && !identity_conflict
        && !matches!(
            mode.as_str(),
            "game-tools-unavailable" | "game-permission-denied"
        )
    {
        tools(index, session_turn);
    }
    if mode == "revoked-between-polls" {
        return;
    }
    if mode == "attachment-barrier" {
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if provider == "codex" {
        if identity_conflict {
            println!("{{\"type\":\"thread.started\",\"thread_id\":\"{FOREIGN_SESSION}\"}}");
            println!(
                "{{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":9000001,\"cached_input_tokens\":8000002,\"output_tokens\":7000003}}}}"
            );
        } else if mode == "metrics-failed" {
            let usage = codex_cumulative_usage(&root, index, session_turn);
            println!(
                "{{\"type\":\"turn.failed\",\"usage\":{usage},\"error\":{{\"code\":\"rate_limit_exceeded\",\"message\":\"private-provider-transcript-marker credentials={}\"}}}}",
                var("TOKEN")
            );
        } else if missing_metrics {
            println!("{{\"type\":\"turn.completed\"}}");
        } else {
            let usage = codex_cumulative_usage(&root, index, session_turn);
            println!("{{\"type\":\"turn.completed\",\"usage\":{usage}}}");
        }
    } else {
        let (subtype, is_error) = if mode == "metrics-failed" {
            ("error_during_execution", true)
        } else {
            ("success", false)
        };
        let usage = if missing_metrics {
            String::new()
        } else {
            format!(
                ",\"modelUsage\":{{\"claude-fake-model\":{{\"inputTokens\":{},\"outputTokens\":{},\"cacheReadInputTokens\":{},\"cacheCreationInputTokens\":{},\"costUSD\":0.{session_turn:03}}}}},\"total_cost_usd\":0.{session_turn:03}",
                100 * session_turn,
                30 * session_turn,
                20 * session_turn,
                5 * session_turn
            )
        };
        let denials = if mode == "game-permission-denied" {
            ",\"permission_denials\":[{\"tool_name\":\"mcp__npc__register\",\"tool_use_id\":\"tool_1\",\"tool_input\":{\"private_payload\":\"private-provider-transcript-marker\"}}]"
        } else {
            ""
        };
        println!(
            "{{\"type\":\"result\",\"subtype\":\"{subtype}\",\"is_error\":{is_error},\"session_id\":\"{SESSION}\",\"result\":\"private-provider-transcript-marker\"{usage}{denials}}}"
        );
    }
    std::io::stdout().flush().unwrap();
    fs::write(
        root.join(format!("turn-{index}.done")),
        "terminal event emitted",
    )
    .unwrap();
    if barrier {
        fs::write(
            root.join("cancel-held-after-terminal"),
            "owned provider awaits client cleanup",
        )
        .unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
