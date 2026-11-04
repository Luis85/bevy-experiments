//! A minimal MCP (Model Context Protocol) server over stdio (`npcctl mcp
//! --url ..`), so agent harnesses can drive actors directly.
//!
//! Transport (MCP stdio): newline-delimited JSON-RPC 2.0 messages on
//! stdin/stdout, one per line, no embedded newlines; logs go to stderr;
//! the server exits when stdin closes.
//!
//! Lifecycle: the `initialize` handshake of the 2025-era protocol versions
//! ([`PROTOCOL_VERSIONS`], latest first): a supported requested version is
//! echoed, any other gets [`PROTOCOL_VERSIONS`]`[0]`. `notifications/
//! initialized` and other notifications are accepted silently; `ping`
//! answers `{}`. The stateless 2026-07-28 revision (`server/discover`,
//! per-request `_meta` versions) is not implemented yet.
//!
//! Tools: `observe`, `act`, `act_and_wait`, `cancel`, `events`, `attach`,
//! `list_units`, `notes_list`, `notes_add`, `notes_update`, and the three
//! `map_list` / `map_add` / `map_update` tools, each one RPC
//! to the server
//! at `--url` (`agent/*`; the notes tools are the agent notes methods,
//! `agent/notes/*`, with `controller` defaulting to the server's
//! `--controller`, [`DEFAULT_CONTROLLER`] unless given; there is no player
//! tool: an agent never sets a proposal's status). `events` and
//! `act_and_wait` add a unique `nonce` per call (unless the caller gave
//! one), so concurrent identical polls stay apart on a BRP server
//! (`crate::remote`). `act_and_wait` (`agent/act_and_wait`: act, wait until
//! idle, observe, in one call) simply waits for the RPC answer (its
//! `timeout_ms`, default 30000, at most 120000). Results carry the method's
//! JSON as `structuredContent` and as text (the observation's text form for
//! `observe` with `format: "text"`, the default; for `act_and_wait` an act
//! line, a wait line and the observation's text, `observe.format` defaulting
//! to `"text"`). RPC errors and
//! rejected actions are tool errors (`isError: true`; an RPC error's
//! `data.reason` is in its text and in `structuredContent.error.reason`),
//! unknown tools and bad requests are JSON-RPC errors.

use std::io::{self, BufRead, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::api::RpcError;
use crate::http::{ClientError, RpcClient};

mod map_tools;
mod registered;
pub use registered::{RegisteredClient, handle_registered, registered_tools, serve_registered};

/// A per-call nonce with an OS-random process prefix. PID reuse and a
/// restarted counter cannot reproduce a previous process's request key.
pub fn request_nonce() -> Result<String, RpcError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static PREFIX: OnceLock<Result<String, String>> = OnceLock::new();
    let prefix = PREFIX
        .get_or_init(crate::registration::new_secret)
        .as_ref()
        .map_err(|_| RpcError::new(-32603, "Cannot create a unique request identifier"))?;
    Ok(format!(
        "mcp-{prefix}-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

/// The controller the notes tools use when a call names none (`npcctl mcp
/// --controller`, default `explorer`).
pub const DEFAULT_CONTROLLER: &str = "explorer";

/// Protocol versions this server speaks, latest first.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

fn schema(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

/// The `tools/list` entries.
pub fn tools() -> Value {
    let unit = json!({"type": "integer", "description": "the unit's SimId"});
    let controller = json!({"type": "string", "description": "your controller id (from attach)"});
    let notes_controller = json!({"type": "string", "description": "your controller id (default: this server's --controller)"});
    let book = json!({"type": "string", "enum": ["diary", "memory", "proposals"],
        "description": "diary: what happened; memory: lasting facts; proposals: improvement ideas for the developer"});
    let step = json!({
        "type": "object",
        "properties": {
            "affordance": {"type": "string"},
            "verb": {"type": "string"},
            "target": {"type": "integer"},
            "pos": {"type": "array", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2},
            "args": {"type": "object"}
        }
    });
    let act_props = json!({
        "unit": unit, "controller": controller,
        "affordance": {"type": "string"},
        "verb": {"type": "string"},
        "target": {"type": "integer"},
        "pos": {"type": "array", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2},
        "args": {"type": "object"},
        "steps": {"type": "array", "items": step, "minItems": 1, "maxItems": 16,
                  "description": "several steps as one atomic act (instead of affordance / verb)"},
        "queue": {"type": "boolean"},
        "expect_tick": {"type": "integer"},
        "expect_seq": {"type": "integer"},
        "allow_stale": {"type": "boolean"}
    });
    let mut wait_props = act_props.clone();
    if let Value::Object(m) = &mut wait_props {
        m.insert("until_idle".into(), json!({"type": "boolean",
            "description": "wait until the unit is idle (its actions, orders, interaction, craft and vehicle drive done); default true"}));
        m.insert("kinds".into(), json!({"type": "array", "items": {"type": "string"},
            "description": "event kinds to return; with until_idle the first one of these also ends the wait (e.g. [\"Perceived\"] to stop when something new is sensed)"}));
        m.insert(
            "timeout_ms".into(),
            json!({"type": "integer",
            "description": "longest wait, default 30000, at most 120000"}),
        );
        m.insert(
            "observe".into(),
            json!({"type": "object", "properties": {
            "format": {"type": "string", "enum": ["text", "json"]},
            "brief": {"type": "boolean"}
        }, "description": "the observation returned at the end (format default text)"}),
        );
    }
    let mut all = json!([
        {
            "name": "observe",
            "title": "Observe a unit",
            "description": "What the unit sees and can do now: self, needs, time, recent events and affordances (each with an id to pass to act). format 'text' (default) is a prompt-ready narrative, 'json' the full structure. since_seq (the seq of your last observation) adds what changed since then: things newly sensed or lost, items gained or lost, order, interaction and action results (text: a first line \"Since your last look: ...\").",
            "inputSchema": schema(json!({
                "unit": unit, "controller": controller,
                "format": {"type": "string", "enum": ["text", "json"]},
                "events": {"type": "integer", "description": "recent events to include (default 10)"},
                "brief": {"type": "boolean", "description": "hide structurally impossible affordances (\"door has no lock\")"},
                "since_seq": {"type": "integer", "description": "the seq of your last observation: adds what changed since"}
            }), &["unit"]),
        },
        {
            "name": "act",
            "title": "Act",
            "description": "Do one of the offered affordances ({affordance: id}) or an explicit verb ({verb, target?, pos?: [x,z] mm, args?}), or several at once (steps: [{affordance} | {verb, ..}, ..], validated all or nothing, then run in order). To walk to a thing and use it in one act: {verb: \"interact\", args: {affordance: \"VERB@ID\"}} (e.g. \"pick_up@26\"). The world keeps running: queue: true appends to the unit's action queue instead of replacing it; expect_tick / expect_seq (from observe) reject the act as stale if the world moved on too far.",
            "inputSchema": schema(act_props, &["unit", "controller"]),
        },
        {
            "name": "act_and_wait",
            "title": "Act and wait",
            "description": "One step in one call: act (every field of act), wait until the unit is idle again (or a named event kind fires, or timeout_ms), and get the act result, the events since the act and a fresh observation (its first line says what changed since the act). A rejected act answers at once (no wait). Use it for every step; use observe with since_seq when you only want to look.",
            "inputSchema": schema(wait_props, &["unit", "controller"]),
        },
        {
            "name": "talk",
            "title": "Speak to an actor or aloud",
            "description": "Say a short public line aloud, or address a nearby living actor using its observed target id. Consult your scene's affordances for speech availability and reach. This is spoken dialogue, not a private notebook or hidden reasoning. The ordinary recorded talk action and control rules apply.",
            "inputSchema": schema(json!({
                "unit": unit, "controller": controller,
                "target": {"type":"integer", "minimum":1, "description":"Optional observed actor to address; omit to speak aloud"},
                "text": {"type":"string", "minLength":1, "maxLength":240, "description":"One public line, at most 240 characters / 1024 bytes; no control characters"}
            }), &["unit", "controller", "text"]),
        },
        {
            "name": "cancel",
            "title": "Cancel an action",
            "description": "Cancel a queued or running action by handle (default: the running one).",
            "inputSchema": schema(json!({
                "unit": unit, "controller": controller,
                "handle": {"type": "integer"}
            }), &["unit", "controller"]),
        },
        {
            "name": "events",
            "title": "Wait for events",
            "description": "Events after since_seq (action done/failed, need level changes, ...). With timeout_ms it waits until something happens or the timeout expires. Pass the returned next_seq as since_seq next time.",
            "inputSchema": schema(json!({
                "unit": unit,
                "since_seq": {"type": "integer"},
                "kinds": {"type": "array", "items": {"type": "string"}},
                "timeout_ms": {"type": "integer"},
                "limit": {"type": "integer"},
                "until_idle": {"type": "boolean", "description": "answer once the unit is idle (its actions, orders, interaction, craft and vehicle drive done) or at the timeout; needs unit"},
                "min_events": {"type": "integer", "description": "answer once at least this many matching events happened (default 1)"},
                "dedupe": {"type": "boolean", "description": "collapse consecutive identical events (\"repeats\": n)"}
            }), &[]),
        },
        {
            "name": "attach",
            "title": "Attach to a unit",
            "description": "Take control of a unit. mode Direct (you choose every action; its own behaviour is suspended), Advisor (you give orders) or Observer (read-only); on_idle Idle (stand still when your queue is empty) or Autopilot (its own behaviour runs meanwhile).",
            "inputSchema": schema(json!({
                "unit": unit,
                "controller": controller,
                "mode": {"type": "string", "enum": ["Direct", "Advisor", "Observer"]},
                "on_idle": {"type": "string", "enum": ["Idle", "Autopilot"]},
                "force": {"type": "boolean"}
            }), &["unit"]),
        },
        {
            "name": "list_units",
            "title": "List units",
            "description": "Every unit with its kind, position, whether it can be controlled, and its controller.",
            "inputSchema": schema(json!({"kind": {"type": "string"}}), &[]),
        },
        {
            "name": "notes_list",
            "title": "List your notes",
            "description": "The entries of one of your notebooks (diary, memory or proposals), oldest first, each with id, title, body, created and updated ({tick, date}); proposals also have status and prompted_by. The game keeps them for you between sessions. unreadable: [{file, reason}] names files of the notebook the game cannot read (edited by hand); they are not entries.",
            "inputSchema": schema(json!({
                "unit": unit, "controller": notes_controller, "book": book
            }), &["unit", "book"]),
        },
        {
            "name": "notes_add",
            "title": "Add a note",
            "description": "Add an entry to your diary (what happened), memory (lasting facts) or proposals (an improvement idea for the developer; prompted_by: what gave you the idea). title: one line, at most 200 characters; body: at most 8192 bytes.",
            "inputSchema": schema(json!({
                "unit": unit, "controller": notes_controller, "book": book,
                "title": {"type": "string"},
                "body": {"type": "string"},
                "prompted_by": {"type": "string", "description": "proposals only"}
            }), &["unit", "book", "title", "body"]),
        },
        {
            "name": "notes_update",
            "title": "Change a note",
            "description": "Change the title and/or body of one of your entries. expected: the entry's updated stamp exactly as notes_list / notes_add / notes_update last returned it; if the entry changed since, the update is rejected as stale (list it again). Only the developer sets a proposal's status.",
            "inputSchema": schema(json!({
                "unit": unit, "controller": notes_controller, "book": book,
                "id": {"type": "integer"},
                "expected": {"type": "object", "properties": {
                    "tick": {"type": "integer"}, "date": {"type": "string"}
                }, "required": ["tick", "date"]},
                "title": {"type": "string"},
                "body": {"type": "string"}
            }), &["unit", "book", "id", "expected"]),
        },
    ]);
    all.as_array_mut()
        .expect("tool array")
        .extend(map_tools::tools(&unit, &notes_controller));
    all
}

fn tool_result(value: &Value, text: String, is_error: bool) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn tool_error(message: String) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

/// A JSON-RPC error of the method as a tool error: the text names the code
/// and the `data.reason` (`too_long`, `not_found`, ..) when there is one,
/// and `structuredContent.error` holds `{code, message, reason?, data?}`
/// for agents that read the structure.
fn rpc_tool_error(e: &RpcError) -> Value {
    let reason = e
        .data
        .as_ref()
        .and_then(|d| d.get("reason"))
        .and_then(Value::as_str);
    let text = match reason {
        Some(r) => format!("{} (code {}, reason {r})", e.message, e.code),
        None => format!("{} (code {})", e.message, e.code),
    };
    let mut error = json!({"code": e.code, "message": e.message});
    if let Some(r) = reason {
        error["reason"] = json!(r);
    }
    if let Some(d) = &e.data {
        error["data"] = d.clone();
    }
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": {"error": error},
        "isError": true,
    })
}

/// Runs a `tools/call`: `Ok(result)` or a JSON-RPC error `(code, message)`.
/// `controller` is the default controller of the notes tools.
pub fn call_tool(
    client: &RpcClient,
    controller: &str,
    name: &str,
    args: Value,
) -> Result<Value, (i64, String)> {
    let (method, args) = prepare_call(Some(controller), name, args)?;
    Ok(present_call(name, client.call(method, args)))
}

fn prepare_call(
    controller: Option<&str>,
    name: &str,
    args: Value,
) -> Result<(&'static str, Value), (i64, String)> {
    let mut args = match args {
        Value::Null => json!({}),
        Value::Object(m) => Value::Object(m),
        _ => return Err((-32602, "arguments must be an object".into())),
    };
    let method = match name {
        "observe" => {
            if args.get("format").is_none() {
                args["format"] = json!("text");
            }
            "agent/observe"
        }
        "act" => "agent/act",
        "talk" => {
            let fields = args.as_object_mut().expect("object checked above");
            if fields
                .keys()
                .any(|key| !matches!(key.as_str(), "unit" | "controller" | "target" | "text"))
            {
                return Err((
                    -32602,
                    "talk accepts only text, target and the tool's actor identity".into(),
                ));
            }
            let text = fields
                .remove("text")
                .ok_or_else(|| (-32602, "talk needs text".into()))?;
            let Some(line) = text.as_str() else {
                return Err((-32602, "talk text must be a string".into()));
            };
            if line.trim().is_empty()
                || line.chars().count() > 240
                || line.len() > 1024
                || line
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
            {
                return Err((-32602, "talk text must be one nonempty public line, at most 240 characters / 1024 bytes".into()));
            }
            if fields.get("target").is_some_and(|target| {
                target
                    .as_u64()
                    .is_none_or(|id| id == 0 || id > u64::from(u32::MAX))
            }) {
                return Err((
                    -32602,
                    "talk target must be a positive observed actor id".into(),
                ));
            }
            fields.insert("verb".into(), json!("talk"));
            fields.insert("args".into(), json!({"text":text}));
            "agent/act"
        }
        "act_and_wait" => {
            if args.get("nonce").is_none() {
                args["nonce"] = json!(request_nonce().map_err(|e| (e.code, e.message))?);
            }
            if args.get("observe").is_none_or(Value::is_null) {
                args["observe"] = json!({});
            }
            if args["observe"].is_object() && args["observe"].get("format").is_none() {
                args["observe"]["format"] = json!("text");
            }
            "agent/act_and_wait"
        }
        "cancel" => "agent/cancel",
        "events" => {
            if args.get("nonce").is_none() {
                args["nonce"] = json!(request_nonce().map_err(|e| (e.code, e.message))?);
            }
            "agent/events"
        }
        "attach" => "agent/attach",
        "list_units" => "agent/list_units",
        "notes_list" | "notes_add" | "notes_update" | "map_list" | "map_add" | "map_update" => {
            if let Some(controller) = controller
                && args.get("controller").is_none_or(Value::is_null)
            {
                args["controller"] = json!(controller);
            }
            match name {
                "notes_list" => "agent/notes/list",
                "notes_add" => "agent/notes/add",
                "notes_update" => "agent/notes/update",
                "map_list" => "agent/map/list",
                "map_add" => "agent/map/add",
                "map_update" => "agent/map/update",
                _ => unreachable!("matched notebook/map tool"),
            }
        }
        other => return Err((-32602, format!("Unknown tool: {other}"))),
    };
    Ok((method, args))
}

fn present_call(name: &str, response: Result<Value, ClientError>) -> Value {
    match response {
        Ok(v) => {
            let text = if name == "observe" {
                v.get("text")
                    .and_then(Value::as_str)
                    .map_or_else(|| v.to_string(), str::to_owned)
            } else if name == "act_and_wait" {
                crate::observe::act_and_wait_text(&v)
            } else {
                v.to_string()
            };
            let rejected = v.get("accepted").and_then(Value::as_bool) == Some(false)
                || v.pointer("/act/accepted").and_then(Value::as_bool) == Some(false);
            tool_result(&v, text, rejected)
        }
        Err(ClientError::Rpc(e)) => rpc_tool_error(&e),
        Err(e) => tool_error(e.to_string()),
    }
}

/// Answers one message; `None` for notifications (and responses).
/// `controller`: the notes tools' default controller.
pub fn handle(client: &RpcClient, controller: &str, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str);
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let ok = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result});
    let err = |code: i64, message: String| json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}});
    Some(match method {
        None => err(-32600, "invalid request: no method".into()),
        Some("initialize") => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|v| PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "npcctl", "title": "npc-framework agent control", "version": env!("CARGO_PKG_VERSION")},
                "instructions": format!(
                    "Controls actors of a running npc-framework game at {} in real time: \
                     list_units, attach (Direct), observe, act (affordance ids from observe), \
                     act_and_wait (act, wait until idle and observe in one call), events \
                     (wait for your action to finish); notes_list, notes_add and \
                     notes_update keep your diary, memory and proposals (controller default \
                     `{}`).",
                    client.url(),
                    controller
                ),
            }))
        }
        Some("ping") => ok(json!({})),
        Some("tools/list") => ok(json!({"tools": tools()})),
        Some("tools/call") => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(err(-32602, "tools/call needs a tool name".into()));
            };
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            match call_tool(client, controller, name, args) {
                Ok(r) => ok(r),
                Err((code, message)) => err(code, message),
            }
        }
        Some(other) => err(-32601, format!("Method not found: {other}")),
    })
}

/// Serves MCP on `input`/`output` until `input` closes; `controller` is
/// the notes tools' default controller ([`DEFAULT_CONTROLLER`] in `npcctl
/// mcp` unless `--controller`).
pub fn serve(
    client: &RpcClient,
    controller: &str,
    input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": {"code": -32700, "message": format!("parse error: {e}")}
            })),
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch
                    .iter()
                    .filter_map(|m| handle(client, controller, m))
                    .collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => handle(client, controller, &msg),
        };
        if let Some(reply) = reply {
            // serde_json never emits raw newlines inside a compact document.
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn talk_shortcut_preserves_recorded_action_and_registered_identity_boundaries() {
        let (method, args) =
            prepare_call(None, "talk", json!({"text":"Hello, neighbor!","target":42})).unwrap();
        assert_eq!(method, "agent/act");
        assert_eq!(
            args,
            json!({"verb":"talk","target":42,"args":{"text":"Hello, neighbor!"}})
        );
        let (_, aloud) = prepare_call(
            Some("one"),
            "talk",
            json!({"unit":7,"controller":"one","text":"Good morning."}),
        )
        .unwrap();
        assert_eq!(aloud["unit"], 7);
        assert_eq!(aloud["controller"], "one");
        assert!(aloud.get("target").is_none());
        let tools = registered_tools();
        let talk = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == "talk")
            .unwrap();
        assert_eq!(talk["inputSchema"]["required"], json!(["text"]));
        for private in ["unit", "controller", "session"] {
            assert!(talk["inputSchema"]["properties"].get(private).is_none());
        }
        for invalid in [
            json!({"text":""}),
            json!({"text":"\n"}),
            json!({"text":"hello\u{2028}world"}),
            json!({"text":"hello\u{2029}world"}),
            json!({"text":"x".repeat(241)}),
            json!({"text":"hello","target":0}),
            json!({"text":"hello","target":"42"}),
            json!({"text":"hello","verb":"new_life"}),
            json!({"text":"hello","args":{"body":"unexpected"}}),
        ] {
            assert_eq!(prepare_call(None, "talk", invalid).unwrap_err().0, -32602);
        }
    }

    #[test]
    fn rpc_errors_keep_their_reason() {
        let mut e = RpcError::invalid("title is too long: at most 200 characters");
        e.data = Some(json!({"reason": "too_long"}));
        let r = rpc_tool_error(&e);
        assert_eq!(r["isError"], true);
        assert_eq!(r["structuredContent"]["error"]["reason"], "too_long");
        assert_eq!(r["structuredContent"]["error"]["code"], -32602);
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("reason too_long"), "{text}");
        // Without data, no reason.
        let r = rpc_tool_error(&RpcError::new(-23401, "unknown unit 9"));
        assert!(
            r["structuredContent"]["error"].get("reason").is_none(),
            "{r}"
        );
        assert_eq!(r["content"][0]["text"], "unknown unit 9 (code -23401)");
    }

    #[test]
    fn events_calls_carry_distinct_nonces() {
        let a = request_nonce().unwrap();
        let b = request_nonce().unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("mcp-"), "{a}");
    }

    #[test]
    fn handshake_and_tool_list_without_a_server() {
        // Port 9 (discard) on localhost: nothing answers, which only matters
        // for tools/call.
        let client = RpcClient::new("http://127.0.0.1:9").expect("url");
        let input = [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        ]
        .join("\n");
        let mut out = Vec::new();
        serve(&client, DEFAULT_CONTROLLER, input.as_bytes(), &mut out).expect("serve");
        let lines: Vec<Value> = String::from_utf8(out)
            .expect("utf8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("json line"))
            .collect();
        assert_eq!(lines.len(), 5, "the notification gets no reply: {lines:?}");
        assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            lines[0]["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        let names: Vec<&str> = lines[1]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        assert_eq!(
            names,
            [
                "observe",
                "act",
                "act_and_wait",
                "talk",
                "cancel",
                "events",
                "attach",
                "list_units",
                "notes_list",
                "notes_add",
                "notes_update",
                "map_list",
                "map_add",
                "map_update"
            ]
        );
        assert_eq!(lines[2]["result"], json!({}));
        assert_eq!(lines[3]["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        assert_eq!(lines[4]["error"]["code"], -32602);
    }
}
