//! Closed provider diagnostics. Raw text never enters public status or metrics;
//! the separate private Markdown transcript captures redacted CLI streams.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Read, sync::mpsc::SyncSender};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
impl FailureCode {
    pub fn requires_attention(self) -> bool {
        matches!(
            self,
            Self::GameToolCancelled
                | Self::GameAccessRevoked
                | Self::GameToolsUnavailable
                | Self::GamePermissionDenied
        )
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Authentication => {
                "Provider authentication failed; sign in again using the selected CLI, then restart the AI client"
            }
            Self::RateLimit => {
                "Provider usage limit reached; wait for your allowance to reset before restarting the AI client"
            }
            Self::ModelUnavailable => {
                "The configured model is unavailable to this account; check its model access in the provider CLI"
            }
            Self::McpStartup => {
                "Registered game tools could not start; check that npcctl is installed beside the AI client and this game is running"
            }
            Self::GameToolCancelled => {
                "The provider cancelled a game tool call; the AI client stopped and requires operator attention before restarting"
            }
            Self::GameAccessRevoked => {
                "Game access was explicitly revoked; the AI client stopped without reconnecting or replaying an action"
            }
            Self::GameToolsUnavailable => {
                "The required npc game tools failed to initialize; check the running game and CLI setup before restarting"
            }
            Self::GamePermissionDenied => {
                "The CLI denied a required npc game tool; review its game-tool permissions before restarting"
            }
            Self::ProviderConfig => {
                "Provider configuration was rejected; check the selected CLI configuration before restarting"
            }
            Self::Network => {
                "Provider connection failed; check network access before restarting the AI client"
            }
            Self::OutputLimit => {
                "Provider output exceeded the client limit; the owned turn was stopped"
            }
            Self::MissingTerminal => {
                "Provider exited without a valid completion event; check the CLI installation and game tools"
            }
            Self::NonzeroExit => {
                "Provider process exited unsuccessfully; check the selected CLI before restarting"
            }
            Self::Unknown => {
                "Provider turn failed without a recognized diagnostic; check the selected CLI and game tools before restarting"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}
impl ConnectionFault {
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::Unavailable
                | Self::OwnershipBusy
                | Self::CapacityBusy
                | Self::GameBusy
                | Self::LeaseLost
        )
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Unavailable => {
                "Start or resume this workspace's game server; the AI client is waiting for its connection"
            }
            Self::OwnershipBusy => {
                "Another client or legacy AI turn owns this actor; waiting for it to release control"
            }
            Self::CapacityBusy => {
                "The game has no available AI turn slot; waiting for another turn to finish"
            }
            Self::GameBusy => {
                "The game cannot start an AI turn now; waiting for the actor or game to be ready"
            }
            Self::LeaseLost => {
                "The game client lease was lost; the owned AI turn stopped before reconnecting"
            }
            Self::Authentication => {
                "The game rejected this workspace's credentials; stop and reopen the correct world before restarting the AI client"
            }
            Self::AssignmentChanged => {
                "The game actor assignment or controller changed; reopen the correct world and start a new AI session"
            }
            Self::ActorUnavailable => {
                "The assigned actor is unavailable; use the game's recovery controls before restarting the AI client"
            }
            Self::Revoked => {
                "The game explicitly revoked this client; it stopped without automatically reconnecting"
            }
            Self::Protocol => {
                "The game server returned an incompatible client protocol; use matching game and client builds before restarting"
            }
        }
    }
}

pub fn game_tool(name: &str) -> bool {
    super::mcp_config::REGISTERED_GAME_TOOLS.contains(&name)
}
pub fn claude_game_tool(name: &str) -> bool {
    name.strip_prefix("mcp__npc__").is_some_and(game_tool)
}
fn revoked_error(value: &Value) -> bool {
    value["error"]["code"] == -32602
        && value["error"]["reason"] == "unknown_session"
        && value["error"]["data"]["revoked"] == true
}
fn revoked_payload(value: &Value) -> bool {
    revoked_error(value)
        || revoked_error(&value["structured_content"])
        || revoked_error(&value["structuredContent"])
        || (value["jsonrpc"] == "2.0"
            && value["result"]["isError"] == true
            && revoked_error(&value["result"]["structuredContent"]))
}
/// Only the game's closed authority error is meaningful, not matching prose or
/// arbitrary nested fields in observations, notebooks or tool arguments.
pub fn game_access_revoked(result: &Value) -> bool {
    if revoked_payload(result) {
        return true;
    }
    let nested = match result {
        Value::String(text) if text.len() <= 16 * 1024 => serde_json::from_str::<Value>(text).ok(),
        _ => None,
    };
    if nested.as_ref().is_some_and(revoked_payload) {
        return true;
    }
    let content = if let Some(content) = result.as_array() {
        Some(content)
    } else {
        result["content"].as_array()
    };
    content.is_some_and(|blocks| {
        blocks.len() <= 16
            && blocks.iter().any(|block| {
                if block["type"] != "text" {
                    return false;
                }
                block["text"]
                    .as_str()
                    .filter(|text| text.len() <= 16 * 1024)
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    .is_some_and(|value| revoked_payload(&value))
            })
    })
}
pub fn codex_game_access_revoked(value: &Value) -> bool {
    let item = &value["item"];
    value["type"] == "item.completed"
        && item["type"] == "mcp_tool_call"
        && item["id"].as_str().is_some_and(|id| {
            !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
        })
        && item["server"] == "npc"
        && item["tool"].as_str().is_some_and(game_tool)
        && matches!(item["status"].as_str(), Some("completed" | "failed"))
        && (item["result"]["content"].is_array()
            || item["result"]["structured_content"].is_object())
        && game_access_revoked(&item["result"])
}

/// These structured fields are documented for the verified installed Claude CLI.
/// Unknown versions never acquire new failure semantics from lookalike payloads.
pub fn claude_game_setup_failure(value: &Value, verified: bool) -> Option<FailureCode> {
    if !verified {
        return None;
    }
    if value["type"] == "system" && value["subtype"] == "init" {
        let failed = value["mcp_servers"].as_array().is_some_and(|servers| {
            servers.len() <= 64
                && servers
                    .iter()
                    .any(|server| server["name"] == "npc" && server["status"] == "failed")
        });
        let error = value["mcp_server_errors"].as_array().is_some_and(|errors| {
            errors.len() <= 64
                && errors
                    .iter()
                    .any(|error| error["name"] == "npc" && error["type"].is_string())
        });
        if failed || error {
            return Some(FailureCode::GameToolsUnavailable);
        }
    }
    if value["type"] == "result"
        && value["permission_denials"]
            .as_array()
            .is_some_and(|denials| {
                denials.len() <= 256
                    && denials.iter().any(|denial| {
                        denial["tool_name"].as_str().is_some_and(claude_game_tool)
                            && denial["tool_use_id"].as_str().is_some_and(|id| {
                                !id.is_empty()
                                    && id.len() <= 128
                                    && !id.chars().any(char::is_control)
                            })
                            && denial["tool_input"].is_object()
                    })
            })
    {
        return Some(FailureCode::GamePermissionDenied);
    }
    None
}

/// Recognize only the pinned Codex exec MCP item and its canonical cancellation.
/// Arguments, results and arbitrary error text never cross this boundary.
pub fn codex_game_tool_cancelled(value: &Value) -> bool {
    let item = &value["item"];
    value["type"] == "item.completed"
        && item["type"] == "mcp_tool_call"
        && item["id"].as_str().is_some_and(|id| {
            !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
        })
        && item["server"] == "npc"
        && item["tool"]
            .as_str()
            .is_some_and(|tool| super::mcp_config::REGISTERED_GAME_TOOLS.contains(&tool))
        && item["status"] == "failed"
        && item["error"]["message"] == "user cancelled MCP tool call"
}

fn symbol(value: &str) -> Option<FailureCode> {
    if value.len() > 128 {
        return None;
    }
    match value.to_ascii_lowercase().as_str() {
        "authentication_error" | "authentication_failed" | "unauthorized" | "invalid_api_key" => {
            Some(FailureCode::Authentication)
        }
        "rate_limit" | "rate_limit_exceeded" | "usage_limit_exceeded" | "usagelimitexceeded" => {
            Some(FailureCode::RateLimit)
        }
        "model_not_found" | "model_unavailable" => Some(FailureCode::ModelUnavailable),
        "mcp_startup_failed" | "mcp_startup" | "mcp_server_init_failed" => {
            Some(FailureCode::McpStartup)
        }
        "invalid_configuration" | "config_error" | "configuration_error" => {
            Some(FailureCode::ProviderConfig)
        }
        "connection_failed"
        | "request_timeout"
        | "response_stream_disconnected"
        | "responsestreamdisconnected"
        | "httpconnectionfailed" => Some(FailureCode::Network),
        _ => None,
    }
}

fn text(value: &str) -> Option<FailureCode> {
    let bytes = &value.as_bytes()[..value.len().min(8192)];
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    if text.contains("required mcp server")
        || text.contains("mcp startup failed")
        || (text.contains("mcp server")
            && (text.contains("failed to initialize")
                || text.contains("failed to start")
                || text.contains("handshake failed")))
    {
        Some(FailureCode::McpStartup)
    } else if [
        "authentication failed",
        "not logged in",
        "please log in",
        "unauthorized",
        "token has expired",
        "invalid api key",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(FailureCode::Authentication)
    } else if [
        "rate limit",
        "usage limit",
        "quota exceeded",
        "429 too many requests",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(FailureCode::RateLimit)
    } else if text.contains("model")
        && [
            "not supported",
            "not found",
            "does not exist",
            "do not have access",
        ]
        .iter()
        .any(|phrase| text.contains(phrase))
    {
        Some(FailureCode::ModelUnavailable)
    } else if [
        "error loading config",
        "invalid configuration",
        "failed to load configuration",
        "failed to parse config",
        "unrecognized subcommand",
        "unexpected argument",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(FailureCode::ProviderConfig)
    } else if [
        "connection refused",
        "dns error",
        "failed to resolve",
        "network is unreachable",
        "error sending request",
        "stream disconnected",
        "connection reset",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(FailureCode::Network)
    } else {
        None
    }
}

pub fn structured(value: &Value) -> Option<FailureCode> {
    let error = &value["error"];
    for field in [
        &error["code"],
        &error["type"],
        &error["codexErrorInfo"],
        &value["code"],
        &value["codexErrorInfo"],
    ] {
        if let Some(code) = field.as_str().and_then(symbol) {
            return Some(code);
        }
    }
    for message in [
        error.as_str(),
        error["message"].as_str(),
        value["message"].as_str(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(code) = text(message) {
            return Some(code);
        }
    }
    None
}

/// Drain diagnostics without retaining raw lines. Only a closed classification crosses threads.
#[cfg(test)]
pub fn drain_stderr(reader: impl Read, sender: SyncSender<FailureCode>) {
    drain_stderr_with_metadata(reader, sender, None)
}

pub fn drain_stderr_with_metadata(
    mut reader: impl Read,
    sender: SyncSender<FailureCode>,
    metadata: Option<SyncSender<super::metrics::UsageSample>>,
) {
    let mut buffer = [0u8; 8192];
    let mut line = Vec::new();
    let mut total = 0usize;
    let mut oversized = false;
    let mut reported = false;
    let mut startup = super::provider::usage::StartupMetadata::default();
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        total = total.saturating_add(count);
        // Keep draining beyond the processing cap so pipe pressure cannot stall the provider.
        if total > 256 * 1024 {
            line.clear();
            continue;
        }
        for byte in &buffer[..count] {
            if *byte == b'\n' {
                if !oversized {
                    let value = String::from_utf8_lossy(&line);
                    if let Some(metadata) = &metadata
                        && let Some(sample) = startup.line(&value)
                    {
                        let _ = metadata.try_send(sample);
                    }
                    if !reported && let Some(code) = text(&value) {
                        let _ = sender.try_send(code);
                        reported = true;
                    }
                }
                line.clear();
                oversized = false;
            } else if !oversized {
                if line.len() == 16 * 1024 {
                    line.clear();
                    oversized = true;
                } else {
                    line.push(*byte);
                }
            }
        }
    }
    if !reported
        && !oversized
        && let Some(code) = text(&String::from_utf8_lossy(&line))
    {
        let _ = sender.try_send(code);
    }
    if !oversized
        && let Some(metadata) = metadata
        && let Some(sample) = startup.line(&String::from_utf8_lossy(&line))
    {
        let _ = metadata.try_send(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_requires_closed_authority_metadata_in_a_known_game_tool_result() {
        let payload = serde_json::json!({"error":{"code":-32602,"reason":"unknown_session","data":{"reason":"unknown_session","revoked":true},"message":"PRIVATE_TOOL_TOKEN"}});
        let event = serde_json::json!({"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"npc","tool":"observe","status":"completed","result":{"content":[],"structured_content":payload}}});
        assert!(codex_game_access_revoked(&event));
        for tool in super::super::mcp_config::REGISTERED_GAME_TOOLS {
            let mut value = event.clone();
            value["item"]["tool"] = tool.into();
            assert!(codex_game_access_revoked(&value));
        }
        let mut variants = vec![Value::Null];
        for (field, value) in [
            ("server", serde_json::json!("other")),
            ("tool", serde_json::json!("unknown")),
            ("id", Value::Null),
            ("status", serde_json::json!("started")),
            ("result", serde_json::json!({"arguments":payload})),
        ] {
            let mut other = event.clone();
            other["item"][field] = value;
            variants.push(other);
        }
        for (field, value) in [
            ("code", serde_json::json!(-1)),
            ("reason", serde_json::json!("not_ready")),
            ("data", serde_json::json!({"revoked":false})),
            ("data", serde_json::json!({"revoked":"true"})),
        ] {
            let mut other = event.clone();
            other["item"]["result"]["structured_content"]["error"][field] = value;
            variants.push(other);
        }
        for other in variants {
            assert!(!codex_game_access_revoked(&other));
        }
        let nested = serde_json::json!({"content":[{"type":"text","text":serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"isError":true,"structuredContent":payload}}).to_string()}]});
        assert!(game_access_revoked(&nested));
        assert!(!game_access_revoked(
            &serde_json::json!({"accepted":false,"reason":"ordinary action rejection","revoked":true})
        ));
        assert!(!game_access_revoked(
            &serde_json::json!({"notes":[{"body":payload.to_string()}]})
        ));
        assert!(
            !serde_json::to_string(&FailureCode::GameAccessRevoked)
                .unwrap()
                .contains("PRIVATE_")
        );
    }
    #[test]
    fn claude_required_game_failures_are_version_and_namespace_guarded() {
        let init = serde_json::json!({"type":"system","subtype":"init","mcp_servers":[{"name":"npc","status":"failed"}],"mcp_server_errors":[{"name":"npc","type":"connection","message":"PRIVATE_TOOL_TOKEN"}]});
        assert_eq!(
            claude_game_setup_failure(&init, true),
            Some(FailureCode::GameToolsUnavailable)
        );
        assert_eq!(claude_game_setup_failure(&init, false), None);
        let mut other = init.clone();
        other["mcp_servers"][0]["name"] = "other".into();
        other["mcp_server_errors"][0]["name"] = "other".into();
        assert_eq!(claude_game_setup_failure(&other, true), None);
        let denial = serde_json::json!({"type":"result","subtype":"success","is_error":false,"permission_denials":[{"tool_name":"mcp__npc__register","tool_use_id":"tool_1","tool_input":{"token":"PRIVATE_TOOL_TOKEN"}}]});
        assert_eq!(
            claude_game_setup_failure(&denial, true),
            Some(FailureCode::GamePermissionDenied)
        );
        assert_eq!(claude_game_setup_failure(&denial, false), None);
        for name in [
            "Read",
            "mcp__other__register",
            "mcp__npc__not_a_game_tool",
            "mcp__npc__register_private",
        ] {
            let mut value = denial.clone();
            value["permission_denials"][0]["tool_name"] = name.into();
            assert_eq!(claude_game_setup_failure(&value, true), None);
        }
    }

    fn cancellation() -> Value {
        serde_json::json!({"type":"item.completed","item":{"id":"item_0","type":"mcp_tool_call","server":"npc","tool":"register","status":"failed","error":{"message":"user cancelled MCP tool call"},"arguments":{"token":"private-token-marker"},"result":{"content":["private-result-marker"]}}})
    }

    #[test]
    fn game_cancellation_requires_the_exact_codex_namespace_tool_status_and_message() {
        for tool in super::super::mcp_config::REGISTERED_GAME_TOOLS {
            let mut value = cancellation();
            value["item"]["tool"] = tool.into();
            assert!(codex_game_tool_cancelled(&value));
        }
        let mut variants = vec![Value::Null];
        for (field, replacement) in [
            ("id", Value::Null),
            ("id", serde_json::json!("")),
            ("id", serde_json::json!(17)),
            ("type", serde_json::json!("agent_message")),
            ("server", serde_json::json!("other")),
            ("tool", serde_json::json!("unknown_game_tool")),
            ("tool", serde_json::json!("npc.register")),
            ("status", serde_json::json!("completed")),
            ("status", serde_json::json!("cancelled")),
            (
                "error",
                serde_json::json!({"message":"ordinary game rejection"}),
            ),
            (
                "error",
                serde_json::json!({"message":"user cancelled MCP tool call private-token-marker"}),
            ),
            ("error", serde_json::json!("user cancelled MCP tool call")),
        ] {
            let mut value = cancellation();
            value["item"][field] = replacement;
            variants.push(value);
        }
        let mut started = cancellation();
        started["type"] = "item.started".into();
        variants.push(started);
        for value in variants {
            assert!(!codex_game_tool_cancelled(&value));
        }
        let serialized = serde_json::to_string(&FailureCode::GameToolCancelled).unwrap();
        assert_eq!(serialized, "\"game_tool_cancelled\"");
        assert!(
            !FailureCode::GameToolCancelled
                .message()
                .contains("private-")
        );
    }

    #[test]
    fn error_classification_never_uses_tool_payload_or_unknown_code_fragments() {
        assert_eq!(
            structured(
                &serde_json::json!({"error":{"code":"authentication_error","message":"private-token"}})
            ),
            Some(FailureCode::Authentication)
        );
        assert_eq!(
            structured(
                &serde_json::json!({"error":{"code":"private_authentication_error_token","message":"private-token"},"item":{"text":"unauthorized"}})
            ),
            None
        );
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        drain_stderr(
            &b"required MCP server failed to initialize: private-token\n"[..],
            sender,
        );
        assert_eq!(receiver.recv().unwrap(), FailureCode::McpStartup);
        assert!(
            !serde_json::to_string(&FailureCode::McpStartup)
                .unwrap()
                .contains("private-token")
        );
    }
}
