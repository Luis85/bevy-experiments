//! Explicit per-turn game tooling. Provider project trust is not a setup step.
//!
//! Workspace files are untrusted input: only the deployed npcctl sibling and
//! the running world's registered, loopback transport may be executed.
use super::Provider;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;

// Deliberately closed: this grant must not automatically include future tools.
pub(super) const REGISTERED_GAME_TOOLS: [&str; 16] = [
    "act",
    "act_and_wait",
    "cancel",
    "enter_world",
    "events",
    "map_add",
    "map_list",
    "map_update",
    "notes_add",
    "notes_list",
    "notes_update",
    "observe",
    "register",
    "release",
    "take_control",
    "talk",
];

fn document(path: &Path) -> Result<Value, String> {
    let metadata =
        fs::metadata(path).map_err(|_| "generated game tool configuration is missing")?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err("generated game tool configuration is invalid or too large".into());
    }
    let bytes = fs::read(path).map_err(|_| "cannot read generated game tool configuration")?;
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
        .map_err(|_| "generated game tool configuration is invalid JSON".into())
}

fn only_keys(value: &Value, allowed: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == allowed.len() && object.keys().all(|key| allowed.contains(&key.as_str()))
    })
}

fn validated(workspace: &Path, trusted_program: &Path) -> Result<Value, String> {
    let source = document(&workspace.join(".mcp.json"))?;
    if !only_keys(&source, &["mcpServers"])
        || !only_keys(&source["mcpServers"], &["npc"])
        || !only_keys(&source["mcpServers"]["npc"], &["type", "command", "args"])
    {
        return Err("game tooling must contain only the generated registered npc server".into());
    }
    let server = &source["mcpServers"]["npc"];
    if server["type"] != "stdio" {
        return Err("game tooling must use the registered stdio transport".into());
    }
    let configured = server["command"]
        .as_str()
        .filter(|value| !value.chars().any(char::is_control))
        .ok_or("game tool executable is invalid")?;
    let configured = Path::new(configured);
    if !configured.is_absolute() || !configured.is_file() {
        return Err("game tool executable must be the deployed native npcctl".into());
    }
    let trusted = trusted_program
        .canonicalize()
        .map_err(|_| "npcctl must be installed beside ai-agent-game-client")?;
    if configured.canonicalize().ok().as_ref() != Some(&trusted) {
        return Err("game tool executable does not match the deployed npcctl".into());
    }
    let run = document(&workspace.join("run.json"))?;
    let port = run["port"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or("running game has an invalid transport port")?;
    let url = format!("http://127.0.0.1:{port}");
    let expected = json!(["mcp", "--url", url, "--registered"]);
    if server["args"] != expected {
        return Err(
            "game tools must target this running world's registered loopback endpoint".into(),
        );
    }
    // Construct a closed configuration rather than forwarding arbitrary workspace JSON.
    Ok(
        json!({"mcpServers":{"npc":{"type":"stdio","command":trusted.to_string_lossy(),"args":expected}}}),
    )
}

fn configure_trusted(
    command: &mut Command,
    provider: Provider,
    workspace: &Path,
    trusted_program: &Path,
    approve_game_tools: bool,
) -> Result<(), String> {
    let config = validated(workspace, trusted_program)?;
    if approve_game_tools && provider != Provider::Codex {
        return Err("--approve-game-tools applies only to the registered Codex game tools".into());
    }
    match provider {
        Provider::Codex => {
            // JSON string/array syntax is valid for these TOML basic values.
            // Only the validated npc server is supplied. Broader CLI policy is unchanged.
            for key in ["command", "args"] {
                command.arg("-c").arg(format!(
                    "mcp_servers.npc.{key}={}",
                    config["mcpServers"]["npc"][key]
                ));
            }
            for key in ["enabled", "required"] {
                command.arg("-c").arg(format!("mcp_servers.npc.{key}=true"));
            }
            if approve_game_tools {
                for tool in REGISTERED_GAME_TOOLS {
                    command.arg("-c").arg(format!(
                        "mcp_servers.npc.tools.{tool}.approval_mode=\"approve\""
                    ));
                }
            }
        }
        Provider::Claude => {
            // Inline JSON avoids re-reading an untrusted file between validation and spawn.
            command
                .arg("--mcp-config")
                .arg(config.to_string())
                .arg("--strict-mcp-config");
        }
    }
    Ok(())
}

pub fn configure(
    command: &mut Command,
    provider: Provider,
    workspace: &Path,
    approve_game_tools: bool,
) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|_| "cannot locate AI client installation")?;
    let directory = executable
        .parent()
        .ok_or("cannot locate AI client installation")?;
    let trusted = directory.join(format!("npcctl{}", std::env::consts::EXE_SUFFIX));
    configure_trusted(command, provider, workspace, &trusted, approve_game_tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Fixture(PathBuf, PathBuf);
    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().canonicalize().unwrap();
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = base.join(format!("npc-ai-mcp-{}-{nonce}", std::process::id()));
            fs::create_dir(&root).unwrap();
            let fixture = Self(root.canonicalize().unwrap(), base);
            fs::write(fixture.program(), b"owned native executable fixture").unwrap();
            fs::write(fixture.0.join("run.json"), b"{\"port\":15702}").unwrap();
            fixture.write(fixture.valid());
            fixture
        }
        fn program(&self) -> PathBuf {
            self.0
                .join(format!("npcctl{}", std::env::consts::EXE_SUFFIX))
        }
        fn valid(&self) -> Value {
            json!({"mcpServers":{"npc":{"type":"stdio","command":self.program(),"args":["mcp","--url","http://127.0.0.1:15702","--registered"]}}})
        }
        fn write(&self, config: Value) {
            fs::write(
                self.0.join(".mcp.json"),
                serde_json::to_vec(&config).unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Ok(root) = self.0.canonicalize()
                && root.is_absolute()
                && root.starts_with(&self.1)
                && root != self.1
            {
                let _ = fs::remove_dir_all(root);
            }
        }
    }

    #[test]
    fn both_providers_receive_explicit_registered_game_tools_without_project_trust_or_secrets() {
        let fixture = Fixture::new();
        for provider in [Provider::Codex, Provider::Claude] {
            let mut command = Command::new("never-launched");
            configure_trusted(
                &mut command,
                provider,
                &fixture.0,
                &fixture.program(),
                false,
            )
            .unwrap();
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            let joined = args.join(" ");
            assert!(joined.contains("npcctl") && joined.contains("--registered"));
            assert!(
                !joined.contains("--token")
                    && !joined.contains("trust_level")
                    && !joined.contains("sandbox_mode")
                    && !joined.contains("approval_policy")
                    && !joined.contains("model=")
            );
            if provider == Provider::Claude {
                assert_eq!(args[0], "--mcp-config");
                let source: Value = serde_json::from_str(&args[1]).unwrap();
                assert_eq!(
                    source["mcpServers"]["npc"]["args"][2],
                    "http://127.0.0.1:15702"
                );
                assert_eq!(args[2], "--strict-mcp-config");
            } else {
                assert_eq!(args.len(), 8);
                assert!(args.contains(&"mcp_servers.npc.enabled=true".into()));
                assert!(args.contains(&"mcp_servers.npc.required=true".into()));
                for (key, value) in [(&args[1], "command"), (&args[3], "args")] {
                    let literal = key
                        .strip_prefix(&format!("mcp_servers.npc.{value}="))
                        .unwrap();
                    let _: Value = serde_json::from_str(literal).unwrap();
                }
            }
        }
    }

    #[test]
    fn explicit_approval_is_limited_to_the_closed_registered_game_tool_set() {
        let fixture = Fixture::new();
        let mut command = Command::new("never-launched");
        configure_trusted(
            &mut command,
            Provider::Codex,
            &fixture.0,
            &fixture.program(),
            true,
        )
        .unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(args.len(), 40);
        let grants: std::collections::BTreeSet<_> = args
            .as_chunks::<2>()
            .0
            .iter()
            .filter_map(|pair| {
                assert_eq!(pair[0], "-c");
                pair[1]
                    .strip_prefix("mcp_servers.npc.tools.")
                    .map(|value| value.strip_suffix(".approval_mode=\"approve\"").unwrap())
            })
            .collect();
        assert_eq!(grants.len(), 16);
        let definitions = npc_framework::agent_io::mcp::registered_tools();
        let registered: std::collections::BTreeSet<_> = definitions
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(grants.is_subset(&registered));
        assert!(!args.iter().any(|arg| {
            arg.contains("default_tools_approval_mode")
                || arg.contains("approval_policy")
                || arg.contains("sandbox")
                || arg.contains("enabled_tools")
                || arg.contains("disabled_tools")
                || arg.contains("trust_level")
                || arg.contains("approvals_reviewer")
        }));
    }

    #[test]
    fn codex_approval_opt_in_is_not_applied_to_claude() {
        let fixture = Fixture::new();
        let mut command = Command::new("never-launched");
        assert!(
            configure_trusted(
                &mut command,
                Provider::Claude,
                &fixture.0,
                &fixture.program(),
                true,
            )
            .is_err()
        );
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn rejects_arbitrary_executables_servers_secrets_and_wrong_world_endpoints() {
        let fixture = Fixture::new();
        let rogue = fixture
            .0
            .join(format!("rogue{}", std::env::consts::EXE_SUFFIX));
        fs::write(&rogue, b"untrusted executable").unwrap();
        let mut variants = Vec::new();
        let mut config = fixture.valid();
        config["mcpServers"]["npc"]["command"] = json!(rogue);
        variants.push(config);
        let mut config = fixture.valid();
        config["mcpServers"]["npc"]["command"] = json!("npcctl");
        variants.push(config);
        let mut config = fixture.valid();
        config["mcpServers"]["other"] = json!({"command":"arbitrary-program"});
        variants.push(config);
        for args in [
            json!(["mcp", "--url", "http://127.0.0.1:15703", "--registered"]),
            json!(["mcp", "--url", "https://outside.example", "--registered"]),
            json!([
                "mcp",
                "--url",
                "http://127.0.0.1:15702",
                "--token",
                "private-token"
            ]),
        ] {
            let mut config = fixture.valid();
            config["mcpServers"]["npc"]["args"] = args;
            variants.push(config);
        }
        let mut config = fixture.valid();
        config["mcpServers"]["npc"]["env"] = json!({"SECRET":"private-token"});
        variants.push(config);
        for config in variants {
            fixture.write(config);
            let mut command = Command::new("never-launched");
            let error = configure_trusted(
                &mut command,
                Provider::Codex,
                &fixture.0,
                &fixture.program(),
                true,
            )
            .unwrap_err();
            assert!(!error.contains("private-token") && !error.contains("outside.example"));
            assert_eq!(
                command.get_args().count(),
                0,
                "invalid configuration must not mutate provider argv"
            );
        }
    }

    #[test]
    fn malformed_and_oversized_configs_are_rejected_without_echoing_contents() {
        let fixture = Fixture::new();
        for bytes in [
            b"private invalid JSON".to_vec(),
            vec![b'x'; MAX_CONFIG_BYTES as usize + 1],
        ] {
            fs::write(fixture.0.join(".mcp.json"), bytes).unwrap();
            let error = validated(&fixture.0, &fixture.program()).unwrap_err();
            assert!(!error.contains("private invalid JSON"));
        }
    }
}
