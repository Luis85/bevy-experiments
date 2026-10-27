//! The opt-in, session-bound MCP client. Secrets live only in this process.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use serde_json::{Value, json};

use super::{PROTOCOL_VERSIONS, prepare_call, present_call, schema, tools};
use crate::api::RpcError;
use crate::http::{ClientError, RpcClient};
use crate::stats::wall_now;

/// Long enough for ordinary 20 Hz hosts without advancing their clock.
const CONTROL_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Credentials {
    agent_id: String,
    token: String,
}

/// A registered connection to one assigned actor. Its credential/session
/// fields are deliberately private and it implements no secret-bearing Debug.
pub struct RegisteredClient<'a> {
    client: &'a RpcClient,
    credentials: Option<Credentials>,
    session: Option<String>,
    desired_control: bool,
    ready_control: bool,
}

fn error(reason: &str, message: &str) -> ClientError {
    let mut error = RpcError::invalid(message);
    error.data = Some(json!({"reason": reason}));
    ClientError::Rpc(error)
}

fn revoked(error: &ClientError) -> bool {
    matches!(error, ClientError::Rpc(e) if e.data.as_ref().and_then(|d| d.get("revoked")).and_then(Value::as_bool) == Some(true))
}

fn restart(error: &ClientError) -> bool {
    !revoked(error)
        && matches!(error, ClientError::Rpc(e) if matches!(e.data.as_ref().and_then(|d| d.get("reason")).and_then(Value::as_str), Some("unknown_session" | "not_registered")))
}

fn control_required(method: &str) -> bool {
    !matches!(method, "agent/enter_world" | "agent/take_control")
        && !method.starts_with("agent/notes/")
        && !method.starts_with("agent/map/")
}

fn reset_read_cursor(method: &str, params: &mut Value) {
    // Event rings are tooling state and start fresh in a resumed host.
    // Optimistic act guards are caller intent and must never be discarded.
    if matches!(method, "agent/observe" | "agent/events")
        && params.get("since_seq").and_then(Value::as_u64).is_some()
    {
        params["since_seq"] = json!(0);
    }
}

fn redact_text(text: &str, secrets: &[&str]) -> String {
    secrets
        .iter()
        .filter(|s| !s.is_empty())
        .fold(text.to_owned(), |text, secret| {
            text.replace(secret, "[redacted]")
        })
}

fn redact(value: &mut Value, secrets: &[&str]) {
    match value {
        Value::String(text) => *text = redact_text(text, secrets),
        Value::Array(values) => values.iter_mut().for_each(|v| redact(v, secrets)),
        Value::Object(values) => values.values_mut().for_each(|v| redact(v, secrets)),
        _ => {}
    }
}

fn redact_error(error: ClientError, secrets: &[&str]) -> ClientError {
    match error {
        ClientError::Transport(text) => ClientError::Transport(redact_text(&text, secrets)),
        ClientError::Rpc(mut error) => {
            // These are closed protocol constants, not echoed credential
            // values. Keep them usable even when a custom short credential
            // happens to be a substring (for example "session").
            let auth_reason = error
                .data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str)
                .filter(|reason| {
                    matches!(
                        *reason,
                        "not_registered"
                            | "unknown_session"
                            | "bad_credentials"
                            | "not_yours"
                            | "not_in_control"
                    )
                })
                .map(str::to_owned);
            error.message = redact_text(&error.message, secrets);
            if let Some(data) = &mut error.data {
                redact(data, secrets);
                if let Some(reason) = auth_reason {
                    data["reason"] = json!(reason);
                }
            }
            ClientError::Rpc(error)
        }
    }
}

impl<'a> RegisteredClient<'a> {
    pub fn new(client: &'a RpcClient) -> Self {
        Self {
            client,
            credentials: None,
            session: None,
            desired_control: false,
            ready_control: false,
        }
    }

    fn clear(&mut self) {
        self.credentials = None;
        self.session = None;
        self.desired_control = false;
        self.ready_control = false;
    }

    /// Register supplied credentials. Return only the public acknowledgement;
    /// the session key remains private and is never echoed to MCP/CLI stdout.
    pub fn register(&mut self, agent_id: &str, token: &str) -> Result<Value, ClientError> {
        self.clear();
        if agent_id.is_empty() || token.is_empty() {
            return Err(error(
                "bad_credentials",
                "agent_id and token must be nonempty strings",
            ));
        }
        let result = self
            .client
            .call("agent/register", json!({"agent_id":agent_id,"token":token}))
            .map_err(|e| redact_error(e, &[token]))?;
        let session = result
            .get("session")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                error(
                    "invalid_registration",
                    "The world returned no registration session",
                )
            })?
            .to_owned();
        let mut public = json!({"registered":true});
        for key in ["agent_id", "world", "tick", "seq", "epoch"] {
            if let Some(value) = result.get(key) {
                public[key] = value.clone();
            }
        }
        redact(&mut public, &[token, &session]);
        self.credentials = Some(Credentials {
            agent_id: agent_id.into(),
            token: token.into(),
        });
        self.session = Some(session);
        Ok(public)
    }

    fn send(&self, method: &str, mut params: Value) -> Result<Value, ClientError> {
        let session = self
            .session
            .as_deref()
            .ok_or_else(|| error("not_registered", "Register before using your actor's tools"))?;
        let params = params
            .as_object_mut()
            .ok_or_else(|| error("invalid", "Tool arguments must be an object"))?;
        params.insert("session".into(), json!(session));
        let token = self.credentials.as_ref().map_or("", |c| c.token.as_str());
        self.client
            .call(method, Value::Object(params.clone()))
            .map(|mut value| {
                redact(&mut value, &[token, session]);
                value
            })
            .map_err(|e| redact_error(e, &[token, session]))
    }

    fn send_retry(&mut self, method: &str, mut params: Value) -> Result<Value, ClientError> {
        let response = self.send(method, params.clone());
        if response.as_ref().is_err_and(revoked) {
            self.clear();
            return response;
        }
        if !response.as_ref().is_err_and(restart) {
            return response;
        }
        let Some(credentials) = self.credentials.clone() else {
            return response;
        };
        let desired = self.desired_control;
        self.register(&credentials.agent_id, &credentials.token)?;
        self.desired_control = desired;
        if desired && control_required(method) {
            self.ensure_control(false)?;
        }
        reset_read_cursor(method, &mut params);
        let response = self.send(method, params);
        if response.as_ref().is_err_and(revoked) {
            self.clear();
        }
        response
    }

    fn acquire(&mut self, params: Value, retry: bool) -> Result<Value, ClientError> {
        let started = wall_now();
        let mut result = if retry {
            self.send_retry("agent/take_control", params.clone())?
        } else {
            self.send("agent/take_control", params.clone())?
        };
        if result.get("accepted").and_then(Value::as_bool) == Some(false) {
            return Ok(result);
        }
        self.desired_control = true;
        while result.get("in_control").and_then(Value::as_bool) != Some(true)
            && started.elapsed() < CONTROL_WAIT
        {
            // Host-side client pacing only. Never resume or step simulation.
            std::thread::sleep(Duration::from_millis(10));
            result = self.send("agent/take_control", params.clone())?;
            if result.get("accepted").and_then(Value::as_bool) == Some(false) {
                break;
            }
        }
        self.ready_control = result.get("in_control").and_then(Value::as_bool) == Some(true);
        Ok(result)
    }

    fn ensure_control(&mut self, retry: bool) -> Result<(), ClientError> {
        let ready = self.acquire(json!({}), retry)?;
        if ready.get("in_control").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        let mut failure = error(
            "not_in_control",
            "Control is queued; resume the world before acting",
        );
        if let ClientError::Rpc(e) = &mut failure {
            e.data.as_mut().expect("error data")["pending"] = json!(true);
        }
        Err(failure)
    }

    /// Make an agent call with the cached session. Only a known session
    /// loss reconnects; transport failures never retry potentially accepted acts.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ClientError> {
        if !method.starts_with("agent/") || method == "agent/register" {
            return Err(error(
                "invalid",
                "Registered clients call actor methods only",
            ));
        }
        let result = if method == "agent/take_control" {
            self.acquire(params, true)
        } else if self.desired_control && !self.ready_control && control_required(method) {
            match self.ensure_control(true) {
                Ok(()) => self.send_retry(method, params),
                Err(e) => Err(e),
            }
        } else {
            self.send_retry(method, params)
        };
        if result.as_ref().is_err_and(revoked) {
            self.clear();
        }
        if method == "agent/release"
            && result
                .as_ref()
                .is_ok_and(|v| v.get("accepted").and_then(Value::as_bool) != Some(false))
        {
            self.desired_control = false;
            self.ready_control = false;
        }
        result
    }
}

/// Opt-in tool schema: no unit/controller, attachment, player tools or roster.
pub fn registered_tools() -> Value {
    let mut list = vec![
        json!({"name":"register","title":"Register","description":"Register the agent_id and token from AGENTS.md before entering the world. The game keeps the session private.","inputSchema":schema(json!({"agent_id":{"type":"string"},"token":{"type":"string"}}),&["agent_id","token"])}),
        json!({"name":"enter_world","title":"Enter the world","description":"Meet your assigned character and learn the scene. Then take_control to begin.","inputSchema":schema(json!({}),&[])}),
        json!({"name":"take_control","title":"Take control","description":"Take Direct control of your assigned actor. pending means the world must run its next tick.","inputSchema":schema(json!({}),&[])}),
        json!({"name":"release","title":"Release control","description":"Release your character and leave it idle. You can still read and write your notebooks.","inputSchema":schema(json!({}),&[])}),
    ];
    for mut tool in tools().as_array().expect("tool array").iter().cloned() {
        if matches!(tool["name"].as_str(), Some("attach" | "list_units")) {
            continue;
        }
        if let Some(properties) = tool["inputSchema"]["properties"].as_object_mut() {
            properties.remove("unit");
            properties.remove("controller");
        }
        if let Some(required) = tool["inputSchema"]["required"].as_array_mut() {
            required.retain(|v| v != "unit" && v != "controller");
        }
        list.push(tool);
    }
    for tool in &mut list {
        tool["inputSchema"]["additionalProperties"] = json!(false);
    }
    Value::Array(list)
}

fn tool_call(
    client: &mut RegisteredClient<'_>,
    name: &str,
    args: Value,
) -> Result<Value, (i64, String)> {
    let mut args = match args {
        Value::Null => json!({}),
        Value::Object(_) => args,
        _ => return Err((-32602, "arguments must be an object".into())),
    };
    let definitions = registered_tools();
    let Some(definition) = definitions
        .as_array()
        .expect("tool array")
        .iter()
        .find(|tool| tool["name"] == name)
    else {
        return Err((-32602, format!("Unknown tool: {name}")));
    };
    if ["session", "unit", "controller"]
        .iter()
        .any(|key| args.get(*key).is_some())
    {
        return Err((
            -32602,
            "Actor identity and session are supplied by registration".into(),
        ));
    }
    if args.as_object().is_some_and(|args| {
        args.keys()
            .any(|key| definition["inputSchema"]["properties"].get(key).is_none())
    }) {
        return Err((-32602, "Unsupported tool argument".into()));
    }
    if name == "register" {
        if args
            .as_object()
            .is_some_and(|p| p.keys().any(|k| k != "agent_id" && k != "token"))
        {
            return Err((-32602, "register accepts agent_id and token only".into()));
        }
        let (Some(agent_id), Some(token)) = (args["agent_id"].as_str(), args["token"].as_str())
        else {
            return Err((-32602, "register needs agent_id and token strings".into()));
        };
        return Ok(present_call(name, client.register(agent_id, token)));
    }
    let method = match name {
        "enter_world" => "agent/enter_world",
        "take_control" => "agent/take_control",
        "release" => "agent/release",
        _ => {
            let prepared = prepare_call(None, name, args)?;
            args = prepared.1;
            return Ok(present_call(name, client.call(prepared.0, args)));
        }
    };
    if args.as_object().is_some_and(|p| !p.is_empty()) {
        return Err((-32602, "This tool takes no arguments".into()));
    }
    Ok(present_call(name, client.call(method, args)))
}

/// Stateful equivalent of the legacy handler, selected only by --registered.
pub fn handle_registered(client: &mut RegisteredClient<'_>, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let ok = |value| json!({"jsonrpc":"2.0","id":id,"result":value});
    let err =
        |code, message| json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}});
    Some(match msg.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            let asked = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str);
            let version = asked
                .filter(|v| PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            ok(
                json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"npcctl","title":"npc-framework registered actor","version":env!("CARGO_PKG_VERSION")},"instructions":"Read AGENTS.md, register, enter_world, then take_control. Your actor's observe, act, act_and_wait, events, cancel and notebooks are session-bound. A revoked session needs an explicit new register; restarting or moving your world reconnects transparently."}),
            )
        }
        Some("ping") => ok(json!({})),
        Some("tools/list") => ok(json!({"tools":registered_tools()})),
        Some("tools/call") => {
            let Some(name) = msg.pointer("/params/name").and_then(Value::as_str) else {
                return Some(err(-32602, "tools/call needs a tool name"));
            };
            match tool_call(
                client,
                name,
                msg.pointer("/params/arguments")
                    .cloned()
                    .unwrap_or(Value::Null),
            ) {
                Ok(value) => ok(value),
                Err((code, message)) => err(code, &message),
            }
        }
        Some(_) => err(-32601, "Method not found"),
        None => err(-32600, "invalid request: no method"),
    })
}

/// Registered-mode stdio server. Credentials are supplied through the
/// register tool, not command arguments, logs or files written by this client.
pub fn serve_registered(
    client: &RpcClient,
    input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    let mut state = RegisteredClient::new(client);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(error) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {error}")}}),
            ),
            Ok(Value::Array(batch)) => {
                let replies: Vec<_> = batch
                    .iter()
                    .filter_map(|msg| handle_registered(&mut state, msg))
                    .collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => handle_registered(&mut state, &msg),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpServer;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    struct Host {
        client: RpcClient,
        calls: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Host {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                thread.join().unwrap();
            }
        }
    }
    fn host(revoked: bool) -> Host {
        let server = HttpServer::bind(0).unwrap();
        let client = RpcClient::new(&server.url()).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&calls);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut registrations = 0;
            let mut observations = 0;
            while !done.load(Ordering::Relaxed) {
                while let Some(call) = server.try_recv() {
                    log.lock().unwrap().push(call.method.clone());
                    let result = match call.method.as_str() {
                        "agent/register" => {
                            registrations += 1;
                            Ok(
                                json!({"session":format!("session-key-{registrations}-abcdefghijklmnop"),"agent_id":"wren","world":"Field"}),
                            )
                        }
                        "agent/take_control" => Ok(
                            json!({"accepted":true,"in_control":true,"unit":42,"controller":"wren"}),
                        ),
                        "agent/observe" => {
                            observations += 1;
                            if observations == 1 {
                                let mut e = RpcError::invalid("Session ended");
                                e.data =
                                    Some(json!({"reason":"unknown_session","revoked":revoked}));
                                Err(e)
                            } else {
                                Ok(json!({"text":"You are here."}))
                            }
                        }
                        _ => Ok(json!({"accepted":true})),
                    };
                    call.reply(result);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Host {
            client,
            calls,
            stop,
            thread: Some(thread),
        }
    }

    fn custom_host(
        mut reply: impl FnMut(&str, Option<&Value>) -> Result<Value, RpcError> + Send + 'static,
    ) -> Host {
        let server = HttpServer::bind(0).unwrap();
        let client = RpcClient::new(&server.url()).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&calls);
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                while let Some(call) = server.try_recv() {
                    log.lock().unwrap().push(call.method.clone());
                    let result = reply(&call.method, call.params.as_ref());
                    call.reply(result);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Host {
            client,
            calls,
            stop,
            thread: Some(thread),
        }
    }

    fn session_lost() -> RpcError {
        let mut error = RpcError::invalid("Host restarted");
        error.data = Some(json!({"reason":"unknown_session"}));
        error
    }

    fn ack(registration: u32) -> Value {
        json!({"session":format!("session-{registration}-private-abcdefghijklmnop"),"agent_id":"wren","world":"Field"})
    }

    #[test]
    fn restart_resets_read_cursors_for_a_fresh_event_ring() {
        for method in ["agent/observe", "agent/events"] {
            let mut registrations = 0;
            let host = custom_host(move |name, params| match name {
                "agent/register" => {
                    registrations += 1;
                    Ok(ack(registrations))
                }
                "agent/take_control" => Ok(json!({"accepted":true,"in_control":true})),
                name if name == method => {
                    if registrations == 1 {
                        Err(session_lost())
                    } else {
                        assert_eq!(
                            params.unwrap()["since_seq"],
                            0,
                            "read cursor must refer to the new ring"
                        );
                        Ok(json!({"seq":2,"events":[]}))
                    }
                }
                _ => panic!("unexpected method"),
            });
            let mut client = RegisteredClient::new(&host.client);
            client.register("wren", TOKEN).unwrap();
            client.call("agent/take_control", json!({})).unwrap();
            assert_eq!(
                client.call(method, json!({"since_seq":9000})).unwrap()["seq"],
                2
            );
        }
    }

    #[test]
    fn restart_preserves_explicit_optimistic_act_guards() {
        let mut registrations = 0;
        let host = custom_host(move |method, params| match method {
            "agent/register" => {
                registrations += 1;
                Ok(ack(registrations))
            }
            "agent/take_control" => Ok(json!({"accepted":true,"in_control":true})),
            "agent/act" => {
                assert_eq!(params.unwrap()["expect_seq"], 9000);
                assert_eq!(params.unwrap()["expect_tick"], 73);
                if registrations == 1 {
                    Err(session_lost())
                } else {
                    let mut error = RpcError::invalid("Fresh observation required");
                    error.data = Some(json!({"reason":"stale"}));
                    Err(error)
                }
            }
            _ => panic!("unexpected method"),
        });
        let mut client = RegisteredClient::new(&host.client);
        client.register("wren", TOKEN).unwrap();
        client.call("agent/take_control", json!({})).unwrap();
        let error = client
            .call("agent/act", json!({"expect_seq":9000,"expect_tick":73}))
            .unwrap_err();
        assert!(
            matches!(error,ClientError::Rpc(error) if error.data.as_ref().unwrap()["reason"]=="stale")
        );
    }

    #[test]
    fn paused_reconnect_keeps_notes_usable_and_restores_before_active_calls() {
        let running = Arc::new(AtomicBool::new(false));
        let now_running = Arc::clone(&running);
        let mut registrations = 0;
        let host = custom_host(move |method, _| match method {
            "agent/register" => {
                registrations += 1;
                Ok(ack(registrations))
            }
            "agent/take_control" => Ok(
                json!({"accepted":true,"in_control":registrations==1||now_running.load(Ordering::Relaxed)}),
            ),
            "agent/notes/list" => {
                if registrations == 1 {
                    Err(session_lost())
                } else {
                    Ok(json!({"notes":[]}))
                }
            }
            "agent/observe" => Ok(json!({"seq":2})),
            _ => panic!("unexpected method"),
        });
        let mut client = RegisteredClient::new(&host.client);
        client.register("wren", TOKEN).unwrap();
        client.call("agent/take_control", json!({})).unwrap();
        assert!(
            client
                .call("agent/notes/list", json!({"book":"memory"}))
                .is_ok()
        );
        assert_eq!(
            *host.calls.lock().unwrap(),
            [
                "agent/register",
                "agent/take_control",
                "agent/notes/list",
                "agent/register",
                "agent/notes/list"
            ]
        );
        running.store(true, Ordering::Relaxed);
        assert_eq!(client.call("agent/observe", json!({})).unwrap()["seq"], 2);
        assert_eq!(
            &host.calls.lock().unwrap()[5..],
            ["agent/take_control", "agent/observe"]
        );
    }

    #[test]
    fn schemas_and_acknowledgements_keep_sessions_private() {
        let list = registered_tools();
        let names: Vec<_> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(!names.contains(&"attach") && !names.contains(&"list_units"));
        for tool in list.as_array().unwrap() {
            assert!(tool["inputSchema"]["properties"].get("unit").is_none());
            assert!(
                tool["inputSchema"]["properties"]
                    .get("controller")
                    .is_none()
            );
        }
        let host = host(false);
        let mut client = RegisteredClient::new(&host.client);
        let result = client.register("wren", TOKEN).unwrap().to_string();
        assert!(!result.contains(TOKEN) && !result.contains("session-key"));
    }

    #[test]
    fn reconnect_restores_control_once_without_replaying_an_act() {
        let host = host(false);
        let mut client = RegisteredClient::new(&host.client);
        client.register("wren", TOKEN).unwrap();
        client.call("agent/take_control", json!({})).unwrap();
        assert_eq!(
            client.call("agent/observe", json!({})).unwrap()["text"],
            "You are here."
        );
        assert_eq!(
            *host.calls.lock().unwrap(),
            [
                "agent/register",
                "agent/take_control",
                "agent/observe",
                "agent/register",
                "agent/take_control",
                "agent/observe"
            ]
        );
    }

    #[test]
    fn revoked_sessions_require_explicit_new_credentials() {
        let host = host(true);
        let mut client = RegisteredClient::new(&host.client);
        client.register("wren", TOKEN).unwrap();
        client.call("agent/take_control", json!({})).unwrap();
        assert!(client.call("agent/observe", json!({})).is_err());
        assert!(client.call("agent/observe", json!({})).is_err());
        assert_eq!(
            *host.calls.lock().unwrap(),
            ["agent/register", "agent/take_control", "agent/observe"]
        );
    }
}
