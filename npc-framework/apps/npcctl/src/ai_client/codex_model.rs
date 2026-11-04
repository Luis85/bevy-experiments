//! Resolve a process-local Codex selection from effective config and its advertised catalog.
//! The metadata connection never creates or resumes an AI thread or starts a turn.
use super::{job::Job, provider};
use serde::{Deserialize, Serialize, de};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    fmt,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::{Duration, Instant},
};

const ERROR: &str = "Cannot verify a usable Codex model and effort from CLI metadata; check the logged-in CLI configuration before starting the agent";
pub(super) const GAME_TOOL_POLICY_ERROR: &str = "Codex game-tool configuration conflicts with the AI client's requested tools; review the npc MCP server and explicit approval restrictions in your CLI settings";
const MAX_LINE: usize = 256 * 1024;
const MAX_OUTPUT: usize = 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionSource {
    CliConfig,
    AdvertisedDefault,
}

/// Requested configuration is distinct from model/effort observed in provider output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionIntent {
    pub requested_model: String,
    pub requested_effort: Option<String>,
    pub model_source: SelectionSource,
    pub effort_source: SelectionSource,
}

struct Model {
    name: String,
    hidden: bool,
    default: bool,
    effort: String,
    supported: BTreeSet<String>,
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => Err(ERROR.into()),
    }
}

fn game_tool_policy(config: &Value, approve_game_tools: bool) -> Result<(), String> {
    let config = config["config"].as_object().ok_or(GAME_TOOL_POLICY_ERROR)?;
    let servers = match config.get("mcp_servers") {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Object(servers)) => servers,
        _ => return Err(GAME_TOOL_POLICY_ERROR.into()),
    };
    let npc = match servers.get("npc") {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Object(npc)) => npc,
        _ => return Err(GAME_TOOL_POLICY_ERROR.into()),
    };
    // The process-local game transport enables this server even without an
    // approval grant. Never overwrite an operator's explicit disable decision.
    match npc.get("enabled") {
        None | Some(Value::Null | Value::Bool(true)) => {}
        _ => return Err(GAME_TOOL_POLICY_ERROR.into()),
    }
    if !approve_game_tools {
        return Ok(());
    }
    fn permits_grant(mode: Option<&Value>) -> bool {
        // Explicit Auto still delegates a decision to provider policy. Only an
        // already explicit Approve or an absent restriction permits our grant.
        matches!(mode, None | Some(Value::Null)) || mode.and_then(Value::as_str) == Some("approve")
    }
    if !permits_grant(npc.get("default_tools_approval_mode")) {
        return Err(GAME_TOOL_POLICY_ERROR.into());
    }
    let tools = match npc.get("tools") {
        None | Some(Value::Null) => return Ok(()),
        Some(Value::Object(tools)) if tools.len() <= 256 => tools,
        _ => return Err(GAME_TOOL_POLICY_ERROR.into()),
    };
    // Only these closed registered tools receive a process-local approval
    // override. Unknown tools, enabled/disabled filters and managed constraints
    // remain untouched and cannot be granted through this client.
    for tool in super::mcp_config::REGISTERED_GAME_TOOLS {
        if let Some(settings) = tools.get(tool).filter(|settings| !settings.is_null()) {
            let settings = settings.as_object().ok_or(GAME_TOOL_POLICY_ERROR)?;
            if !permits_grant(settings.get("approval_mode")) {
                return Err(GAME_TOOL_POLICY_ERROR.into());
            }
        }
    }
    Ok(())
}

fn select(config: &Value, catalog: &Value) -> Result<SelectionIntent, String> {
    let config = config["config"].as_object().ok_or(ERROR)?;
    if optional_string(config, "model_provider")?.is_some_and(|provider| provider != "openai") {
        // A custom backend must not silently become a subscription model selection.
        return Err(ERROR.into());
    }
    let configured_model = optional_string(config, "model")?
        .map(|name| provider::usage::model(name).ok_or(ERROR))
        .transpose()?;
    let configured_effort = optional_string(config, "model_reasoning_effort")?
        .map(|effort| provider::usage::effort(effort).ok_or(ERROR))
        .transpose()?;
    let catalog = catalog.as_object().ok_or(ERROR)?;
    if catalog
        .get("nextCursor")
        .is_some_and(|value| !value.is_null())
    {
        return Err(ERROR.into());
    }
    let entries = catalog.get("data").and_then(Value::as_array).ok_or(ERROR)?;
    if entries.is_empty() || entries.len() > 100 {
        return Err(ERROR.into());
    }
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut models = Vec::new();
    for entry in entries {
        let name = entry["model"]
            .as_str()
            .and_then(provider::usage::model)
            .ok_or(ERROR)?;
        let id = entry["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 200 && !id.chars().any(char::is_control))
            .ok_or(ERROR)?;
        if !names.insert(name.clone()) || !ids.insert(id) {
            return Err(ERROR.into());
        }
        let hidden = entry["hidden"].as_bool().ok_or(ERROR)?;
        let default = entry["isDefault"].as_bool().ok_or(ERROR)?;
        let effort = entry["defaultReasoningEffort"]
            .as_str()
            .and_then(provider::usage::effort)
            .ok_or(ERROR)?;
        let choices = entry["supportedReasoningEfforts"].as_array().ok_or(ERROR)?;
        if choices.is_empty() || choices.len() > 16 {
            return Err(ERROR.into());
        }
        let mut supported = BTreeSet::new();
        for choice in choices {
            let choice = choice["reasoningEffort"]
                .as_str()
                .and_then(provider::usage::effort)
                .ok_or(ERROR)?;
            if !supported.insert(choice) {
                return Err(ERROR.into());
            }
        }
        if !supported.contains(&effort) {
            return Err(ERROR.into());
        }
        models.push(Model {
            name,
            hidden,
            default,
            effort,
            supported,
        });
    }
    let configured = models
        .iter()
        .find(|model| !model.hidden && configured_model.as_deref() == Some(model.name.as_str()));
    let (model, model_source) = if let Some(model) = configured {
        (model, SelectionSource::CliConfig)
    } else {
        let mut defaults = models.iter().filter(|model| !model.hidden && model.default);
        let model = defaults.next().ok_or(ERROR)?;
        if defaults.next().is_some() {
            return Err(ERROR.into());
        }
        (model, SelectionSource::AdvertisedDefault)
    };
    let (effort, effort_source) =
        if let Some(effort) = configured_effort.filter(|effort| model.supported.contains(effort)) {
            (effort, SelectionSource::CliConfig)
        } else {
            (model.effort.clone(), SelectionSource::AdvertisedDefault)
        };
    Ok(SelectionIntent {
        requested_model: model.name.clone(),
        requested_effort: Some(effort),
        model_source,
        effort_source,
    })
}

// serde_json::Value normally accepts duplicate keys. Reject them at every depth
// so metadata cannot hide an ambiguous model, visibility flag or response ID.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| UniqueJson(Value::Number(number)))
                    .ok_or_else(|| E::custom("invalid metadata number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(UniqueJson(value)) = sequence.next_element()? {
                    result.push(value);
                }
                Ok(UniqueJson(Value::Array(result)))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut object: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = Map::new();
                while let Some(key) = object.next_key::<String>()? {
                    if result.contains_key(&key) {
                        return Err(de::Error::custom("duplicate metadata key"));
                    }
                    let UniqueJson(value) = object.next_value()?;
                    result.insert(key, value);
                }
                Ok(UniqueJson(Value::Object(result)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn read_metadata(stdout: impl Read, sender: SyncSender<Result<Value, ()>>) {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    let mut total = 0;
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) if !chunk.is_empty() => chunk,
            _ => break,
        };
        let count = chunk
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(chunk.len(), |i| i + 1);
        total += count;
        if total > MAX_OUTPUT || line.len() + count > MAX_LINE {
            break;
        }
        line.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if line.last() == Some(&b'\n') {
            let value = serde_json::from_slice::<UniqueJson>(&line)
                .map(|value| value.0)
                .map_err(|_| ());
            let valid = value.is_ok();
            if sender.try_send(value).is_err() || !valid {
                return;
            }
            line.clear();
        }
    }
    let _ = sender.try_send(Err(()));
}

fn response(value: &Value, id: u64, notifications: &mut usize) -> Result<Option<Value>, String> {
    let object = value.as_object().ok_or(ERROR)?;
    if object.contains_key("method") && !object.contains_key("id") {
        *notifications += 1;
        if *notifications > 32 || !object["method"].is_string() {
            return Err(ERROR.into());
        }
        return Ok(None);
    }
    if object.contains_key("method")
        || object.get("id").and_then(Value::as_u64) != Some(id)
        || object.contains_key("error")
        || !object.contains_key("result")
    {
        return Err(ERROR.into());
    }
    Ok(Some(object["result"].clone()))
}

struct Probe {
    child: Child,
    _job: Job,
    writer: SyncSender<Vec<u8>>,
    responses: Receiver<Result<Value, ()>>,
    deadline: Instant,
    notifications: usize,
}
impl Probe {
    fn open(program: &Path, workspace: &Path) -> Result<Self, String> {
        let mut command = Command::new(program);
        command
            .arg("app-server")
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        provider::quiet(&mut command);
        // Discard diagnostics directly: zero retained bytes and no stderr pipe pressure.
        command.stderr(Stdio::null());
        provider::subscription_environment(&mut command);
        let mut child = command.spawn().map_err(|_| ERROR)?;
        let job = match Job::attach(&child) {
            Ok(job) => job,
            Err(_) => {
                provider::terminate(&mut child);
                return Err(ERROR.into());
            }
        };
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            provider::terminate(&mut child);
            return Err(ERROR.into());
        };
        let (sender, responses) = mpsc::sync_channel(16);
        thread::spawn(move || read_metadata(stdout, sender));
        let (writer, requests) = mpsc::sync_channel::<Vec<u8>>(4);
        thread::spawn(move || {
            while let Ok(bytes) = requests.recv() {
                if stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            _job: job,
            writer,
            responses,
            deadline: Instant::now() + DEADLINE,
            notifications: 0,
        })
    }
    fn send(&self, method: &str, params: Value, id: Option<u64>) -> Result<(), String> {
        if !matches!(
            method,
            "initialize" | "initialized" | "config/read" | "model/list"
        ) {
            return Err(ERROR.into());
        }
        let mut request = json!({"method":method,"params":params});
        if let Some(id) = id {
            request["id"] = id.into();
        }
        let mut bytes = serde_json::to_vec(&request).map_err(|_| ERROR)?;
        if bytes.len() > 16 * 1024 {
            return Err(ERROR.into());
        }
        bytes.push(b'\n');
        self.writer.try_send(bytes).map_err(|_| ERROR.into())
    }
    fn receive(&mut self, id: u64) -> Result<Value, String> {
        loop {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ERROR.into());
            }
            let value = self
                .responses
                .recv_timeout(remaining)
                .map_err(|_| ERROR)?
                .map_err(|()| ERROR)?;
            if let Some(result) = response(&value, id, &mut self.notifications)? {
                return Ok(result);
            }
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        provider::terminate(&mut self.child);
    }
}

pub fn prepare(
    program: &Path,
    workspace: &Path,
    approve_game_tools: bool,
) -> Result<SelectionIntent, String> {
    let cwd = workspace
        .to_str()
        .filter(|_| workspace.is_absolute())
        .ok_or(ERROR)?;
    let mut probe = Probe::open(program, workspace)?;
    probe.send("initialize", json!({"clientInfo":{"name":"npc_framework_ai_client_metadata","title":"NPC Framework AI client metadata","version":env!("CARGO_PKG_VERSION")}}), Some(1))?;
    if !probe.receive(1)?.is_object() {
        return Err(ERROR.into());
    }
    probe.send("initialized", json!({}), None)?;
    probe.send(
        "config/read",
        json!({"includeLayers":false,"cwd":cwd}),
        Some(2),
    )?;
    let config = probe.receive(2)?;
    // This is the effective operator configuration, before any process-local
    // game MCP or approval overrides are supplied to a provider turn.
    game_tool_policy(&config, approve_game_tools)?;
    probe.send(
        "model/list",
        json!({"limit":100,"includeHidden":false}),
        Some(3),
    )?;
    let catalog = probe.receive(3)?;
    select(&config, &catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(model: Option<&str>, effort: Option<&str>) -> Value {
        json!({"config":{"model":model,"model_reasoning_effort":effort,"model_provider":null}})
    }
    fn model(name: &str, default: bool, hidden: bool) -> Value {
        json!({"id":name,"model":name,"isDefault":default,"hidden":hidden,"defaultReasoningEffort":"low","supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"high"}]})
    }
    fn catalog() -> Value {
        json!({"data":[model("gpt-fixture-a",true,false),model("gpt-fixture-b",false,false)],"nextCursor":null})
    }

    #[test]
    fn game_policy_allows_missing_null_and_already_approved_game_settings() {
        for config in [
            json!({"config":{}}),
            json!({"config":{"mcp_servers":null}}),
            json!({"config":{"mcp_servers":{"npc":null}}}),
            json!({"config":{"mcp_servers":{"npc":{}}}}),
            json!({"config":{"mcp_servers":{"npc":{"enabled":null,"default_tools_approval_mode":null,"tools":null}}}}),
            json!({"config":{"mcp_servers":{"npc":{"enabled":true,"default_tools_approval_mode":"approve","tools":{"observe":{"approval_mode":"approve"},"act":{"approval_mode":null},"events":null}}}}}),
        ] {
            for grant in [false, true] {
                assert!(game_tool_policy(&config, grant).is_ok());
            }
        }
    }

    #[test]
    fn game_approval_never_overrides_explicit_server_or_registered_tool_policy() {
        for mode in ["prompt", "writes", "auto", "unknown-private-marker"] {
            let config =
                json!({"config":{"mcp_servers":{"npc":{"default_tools_approval_mode":mode}}}});
            assert_eq!(
                game_tool_policy(&config, true).unwrap_err(),
                GAME_TOOL_POLICY_ERROR
            );
            assert!(game_tool_policy(&config, false).is_ok());
            for tool in super::super::mcp_config::REGISTERED_GAME_TOOLS {
                let mut config = json!({"config":{"mcp_servers":{"npc":{"tools":{}}}}});
                config["config"]["mcp_servers"]["npc"]["tools"][tool] =
                    json!({"approval_mode":mode});
                assert_eq!(
                    game_tool_policy(&config, true).unwrap_err(),
                    GAME_TOOL_POLICY_ERROR
                );
                assert!(game_tool_policy(&config, false).is_ok());
            }
        }
    }

    #[test]
    fn disabled_or_malformed_needed_game_settings_fail_closed_without_secret_echoes() {
        for config in [
            Value::Null,
            json!({"config":null}),
            json!({"config":{"mcp_servers":"private-server-marker"}}),
            json!({"config":{"mcp_servers":{"npc":"private-npc-marker"}}}),
            json!({"config":{"mcp_servers":{"npc":{"enabled":false}}}}),
            json!({"config":{"mcp_servers":{"npc":{"enabled":"private-enabled-marker"}}}}),
            json!({"config":{"mcp_servers":{"npc":{"enabled":1}}}}),
        ] {
            for grant in [false, true] {
                assert_eq!(
                    game_tool_policy(&config, grant).unwrap_err(),
                    GAME_TOOL_POLICY_ERROR
                );
            }
        }
        for settings in [
            json!({"default_tools_approval_mode":true}),
            json!({"default_tools_approval_mode":{"secret":"private-mode-marker"}}),
            json!({"tools":[]}),
            json!({"tools":"private-tools-marker"}),
            json!({"tools":{"act":false}}),
            json!({"tools":{"observe":{"approval_mode":[]}}}),
        ] {
            let config = json!({"config":{"mcp_servers":{"npc":settings}}});
            assert_eq!(
                game_tool_policy(&config, true).unwrap_err(),
                GAME_TOOL_POLICY_ERROR
            );
        }
        let tools: Map<String, Value> = (0..257)
            .map(|index| (format!("unrelated_{index}"), Value::Null))
            .collect();
        let config = json!({"config":{"mcp_servers":{"npc":{"tools":tools}}}});
        assert_eq!(
            game_tool_policy(&config, true).unwrap_err(),
            GAME_TOOL_POLICY_ERROR
        );
    }

    #[test]
    fn game_policy_leaves_tool_filters_managed_constraints_and_unknown_fields_unchanged() {
        let config = json!({"config":{
            "private_unknown":"private-config-marker",
            "managed_constraints":{"private":"private-managed-marker"},
            "mcp_servers":{
                "external":{"default_tools_approval_mode":"prompt"},
                "npc":{
                    "enabled":true,
                    "enabled_tools":["observe"],
                    "disabled_tools":["act"],
                    "private_unknown":"private-npc-marker",
                    "tools":{
                        "observe":{"approval_mode":"approve","private":"private-tool-marker"},
                        "act":{"enabled":false},
                        "unrelated_future_tool":{"approval_mode":"prompt"}
                    }
                }
            }
        }});
        let original = config.clone();
        assert!(game_tool_policy(&config, true).is_ok());
        assert_eq!(config, original);
    }

    #[test]
    fn visible_configuration_is_preserved_and_missing_model_uses_only_advertised_default() {
        let preserved = select(&config(Some("gpt-fixture-b"), Some("high")), &catalog()).unwrap();
        assert_eq!(preserved.requested_model, "gpt-fixture-b");
        assert_eq!(preserved.requested_effort.as_deref(), Some("high"));
        assert_eq!(preserved.model_source, SelectionSource::CliConfig);
        assert_eq!(preserved.effort_source, SelectionSource::CliConfig);
        let fallback = select(
            &config(Some("gpt-fixture-unavailable"), Some("high")),
            &catalog(),
        )
        .unwrap();
        assert_eq!(fallback.requested_model, "gpt-fixture-a");
        assert_eq!(fallback.requested_effort.as_deref(), Some("high"));
        assert_eq!(fallback.model_source, SelectionSource::AdvertisedDefault);
        assert_eq!(fallback.effort_source, SelectionSource::CliConfig);
    }
    #[test]
    fn absent_or_unsupported_effort_uses_only_supported_reported_default() {
        for effort in [None, Some("ultra")] {
            let selected = select(&config(None, effort), &catalog()).unwrap();
            assert_eq!(selected.requested_effort.as_deref(), Some("low"));
            assert_eq!(selected.effort_source, SelectionSource::AdvertisedDefault);
        }
    }
    #[test]
    fn incomplete_paginated_duplicated_hidden_and_ambiguous_catalogs_fail_closed() {
        let mut variants = vec![
            Value::Null,
            json!({"data":[]}),
            json!({"data":[model("gpt-fixture-a",true,false)],"nextCursor":"more"}),
        ];
        for field in [
            "id",
            "model",
            "hidden",
            "isDefault",
            "defaultReasoningEffort",
            "supportedReasoningEfforts",
        ] {
            let mut value = catalog();
            value["data"][0].as_object_mut().unwrap().remove(field);
            variants.push(value);
        }
        variants.push(
            json!({"data":[model("gpt-fixture-a",true,false),model("gpt-fixture-a",false,false)]}),
        );
        variants.push(json!({"data":[model("gpt-fixture-a",true,true)]}));
        variants.push(json!({"data":[model("gpt-fixture-a",false,false)]}));
        variants.push(
            json!({"data":[model("gpt-fixture-a",true,false),model("gpt-fixture-b",true,false)]}),
        );
        let mut invalid_default = catalog();
        invalid_default["data"][0]["defaultReasoningEffort"] = json!("ultra");
        variants.push(invalid_default);
        let mut duplicate_effort = catalog();
        duplicate_effort["data"][0]["supportedReasoningEfforts"] =
            json!([{"reasoningEffort":"low"},{"reasoningEffort":"low"}]);
        variants.push(duplicate_effort);
        let mut duplicate_id = catalog();
        duplicate_id["data"][1]["id"] = json!("gpt-fixture-a");
        variants.push(duplicate_id);
        for value in variants {
            assert_eq!(select(&config(None, None), &value).unwrap_err(), ERROR);
        }
    }
    #[test]
    fn hidden_configured_model_cannot_be_selected_and_metadata_never_echoes_secrets() {
        let hidden =
            json!({"data":[model("gpt-fixture-a",false,true),model("gpt-fixture-b",true,false)]});
        assert_eq!(
            select(&config(Some("gpt-fixture-a"), None), &hidden)
                .unwrap()
                .requested_model,
            "gpt-fixture-b"
        );
        for config in [
            json!({"config":{"model":17}}),
            json!({"config":{"model":"private-secret-marker"}}),
            json!({"config":{"model_reasoning_effort":"private-secret-marker"}}),
            json!({"config":{"model_provider":"private-secret-marker"}}),
        ] {
            assert_eq!(select(&config, &catalog()).unwrap_err(), ERROR);
        }
        let selected = select(&config(None, None), &catalog()).unwrap();
        let serialized = serde_json::to_string(&selected).unwrap();
        assert!(!serialized.contains("private-secret-marker"));
        assert_eq!(
            serde_json::from_str::<SelectionIntent>(&serialized).unwrap(),
            selected
        );
    }
    #[test]
    fn metadata_reader_rejects_duplicate_keys_malformed_lines_and_limits() {
        for bytes in [
            b"{\"id\":1,\"id\":1,\"result\":{}}\n".to_vec(),
            b"{\"id\":1,\"result\":{\"model\":1,\"model\":2}}\n".to_vec(),
            b"private-secret-invalid-json\n".to_vec(),
            vec![b'x'; MAX_LINE + 1],
        ] {
            let (sender, receiver) = mpsc::sync_channel(16);
            read_metadata(&bytes[..], sender);
            assert!(receiver.recv().unwrap().is_err());
        }
        let (sender, receiver) = mpsc::sync_channel(16);
        read_metadata(&b"{\"id\":1,\"result\":{}}\n"[..], sender);
        assert_eq!(
            receiver.recv().unwrap().unwrap(),
            json!({"id":1,"result":{}})
        );
    }

    #[test]
    fn response_ids_errors_server_requests_and_notification_floods_fail_closed() {
        for value in [
            Value::Null,
            json!({"id":1,"result":{}}),
            json!({"id":"2","result":{}}),
            json!({"id":2}),
            json!({"id":2,"error":{"message":"private-secret-marker"}}),
            json!({"id":2,"method":"thread/start","result":{}}),
        ] {
            assert_eq!(response(&value, 2, &mut 0).unwrap_err(), ERROR);
        }
        assert_eq!(
            response(&json!({"id":2,"result":{"config":{}}}), 2, &mut 0).unwrap(),
            Some(json!({"config":{}}))
        );
        let mut notifications = 0;
        for _ in 0..32 {
            assert!(
                response(
                    &json!({"method":"notification","params":{"private":"private-secret-marker"}}),
                    2,
                    &mut notifications
                )
                .unwrap()
                .is_none()
            );
        }
        assert_eq!(
            response(&json!({"method":"notification"}), 2, &mut notifications).unwrap_err(),
            ERROR
        );
    }
}
