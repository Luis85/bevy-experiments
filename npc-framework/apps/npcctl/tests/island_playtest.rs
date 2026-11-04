//! Registered HTTP/MCP rehearsals against an owned, out-of-process explorer host.
//! Agent decisions use only observations/affordances. The separately named
//! operator harness advances the paused clock; no actor can resume or step it.
use npc_framework::agent_io::http::{ClientError, RpcClient};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

struct DisposableHome(PathBuf, PathBuf);
impl DisposableHome {
    fn new(tag: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!("island-playtest-{tag}-{}", std::process::id()));
        std::fs::create_dir(&path).expect("new disposable fixture directory");
        let path = path.canonicalize().unwrap();
        assert!(path.starts_with(&root) && path != root);
        Self(path, root)
    }
}
impl Drop for DisposableHome {
    fn drop(&mut self) {
        if let Ok(resolved) = self.0.canonicalize()
            && resolved.is_absolute()
            && resolved.starts_with(&self.1)
            && resolved != self.1
        {
            let _ = std::fs::remove_dir_all(resolved);
        }
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct OwnedHost {
    child: OwnedChild,
    operator: RpcClient,
    url: String,
    seed: u64,
    _lines: mpsc::Receiver<String>,
}
impl OwnedHost {
    fn start(home: &Path, resume: bool) -> Self {
        Self::start_with_timeout(home, resume, Duration::from_secs(180))
    }

    fn start_with_timeout(home: &Path, resume: bool, timeout: Duration) -> Self {
        Self::start_with_preset_timeout(home, resume, timeout, "island_village")
    }

    fn start_with_preset(home: &Path, resume: bool, preset: &str) -> Self {
        Self::start_with_preset_timeout(home, resume, Duration::from_secs(180), preset)
    }

    fn start_with_preset_timeout(
        home: &Path,
        resume: bool,
        timeout: Duration,
        preset: &str,
    ) -> Self {
        // These registered rehearsals never invoke a model, including when a
        // developer's parent environment names a live scheduler bindings file.
        let bindings = home.join("playtest-empty-bindings.json");
        std::fs::write(&bindings, "{\"bindings\":[]}").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_npcctl"));
        command
            .env("NPC_AI_SCHEDULER_BINDINGS", &bindings)
            .args(["serve", "--explorer-home"])
            .arg(home)
            .arg("--actors-dir")
            .arg(home.join("actors"))
            .arg("--worlds-dir")
            .arg(home.join("worlds"))
            .args(["--agent", "Herp", "--port", "0", "--paused"]);
        if resume {
            command.arg("--resume");
        } else {
            command.args([
                "--world",
                "Island",
                "--preset",
                "human",
                "--world-preset",
                preset,
            ]);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = OwnedChild(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let stdout = child.0.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let first = match rx.recv_timeout(timeout) {
            Ok(line) => line,
            Err(_) => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                panic!("owned rehearsal host did not become ready");
            }
        };
        let ready: Value = serde_json::from_str(&first).expect("host readiness JSON");
        let url = ready["listening"]
            .as_str()
            .expect("listening URL")
            .to_owned();
        // Operator evidence only. Actor decisions never read immutable save metadata.
        let save: Value =
            serde_json::from_slice(&std::fs::read(home.join("herp/island.world.json")).unwrap())
                .unwrap();
        let seed = save["header"]["seed"].as_u64().unwrap();
        Self {
            child,
            operator: RpcClient::new(&url).unwrap(),
            url,
            seed,
            _lines: rx,
        }
    }

    fn operator_call(&self, method: &str, args: Value) -> Value {
        self.operator
            .call(method, args)
            .unwrap_or_else(|_| panic!("operator {method} failed"))
    }

    fn step(&self, ticks: u64) {
        self.operator_call("npc/step", json!({"ticks":ticks}));
    }

    fn stop(self) {
        self.stop_with_timeout(Duration::from_secs(120));
    }

    fn stop_with_timeout(mut self, timeout: Duration) {
        assert_eq!(
            self.operator_call("session/stop", json!({}))["stopped"],
            true
        );
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.0.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "owned host did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A real newline-framed registered MCP process. Its credential lives only in
/// private fixture memory/stdin; failure messages deliberately omit responses.
struct OwnedMcp {
    child: OwnedChild,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<Result<Value, ()>>,
    reader: Option<std::thread::JoinHandle<()>>,
    deadline: Instant,
    next_id: u64,
    token: String,
}
impl OwnedMcp {
    fn start(host: &OwnedHost, deadline: Instant, token: String) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_npcctl"));
        command.args(["mcp", "--url", &host.url, "--registered"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = OwnedChild(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("owned MCP process starts"),
        );
        let input = child.0.stdin.take();
        let stdout = child.0.stdout.take().expect("owned MCP stdout");
        let (tx, replies) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let reply = line
                    .map_err(|_| ())
                    .and_then(|line| serde_json::from_str::<Value>(&line).map_err(|_| ()));
                if tx.send(reply).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            replies,
            reader: Some(reader),
            deadline,
            next_id: 0,
            token,
        }
    }

    fn send(&mut self, request: &Value) {
        assert!(
            Instant::now() < self.deadline,
            "MCP fixture exceeded its wall-time budget"
        );
        let input = self.input.as_mut().expect("owned MCP stdin remains open");
        serde_json::to_writer(&mut *input, request).expect("write MCP request");
        input.write_all(b"\n").expect("frame MCP request");
        input.flush().expect("flush MCP request");
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let reply = self
            .replies
            .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
            .expect("owned MCP response arrived within fixture budget")
            .expect("owned MCP returned a JSON response");
        assert!(
            !reply.to_string().contains(&self.token),
            "MCP response disclosed private credentials"
        );
        assert!(
            reply["id"] == id && reply["error"].is_null(),
            "MCP protocol request failed"
        );
        reply["result"].clone()
    }

    fn tool(&mut self, name: &str, args: Value) -> Value {
        let reply = self.request("tools/call", json!({"name":name,"arguments":args}));
        assert!(
            reply["isError"] == false,
            "registered MCP tool {name} failed"
        );
        fn private_key(value: &Value) -> bool {
            match value {
                Value::Object(object) => object.iter().any(|(key, value)| {
                    matches!(key.as_str(), "session" | "token") || private_key(value)
                }),
                Value::Array(array) => array.iter().any(private_key),
                _ => false,
            }
        }
        assert!(
            !private_key(&reply),
            "MCP tool response included private session fields"
        );
        let result = reply["structuredContent"].clone();
        assert!(result.is_object(), "MCP tool has structured JSON output");
        result
    }

    fn stop(mut self) {
        self.input.take(); // EOF is the MCP server's normal shutdown protocol.
        let deadline = self.deadline.min(Instant::now() + Duration::from_secs(5));
        loop {
            if let Some(status) = self.child.0.try_wait().expect("owned MCP status") {
                assert!(
                    status.success(),
                    "owned MCP exits successfully after stdin closes"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned MCP did not stop within its budget"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for OwnedMcp {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.0.kill();
        let _ = self.child.0.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct RegisteredAgent {
    client: RpcClient,
    session: String,
    known: BTreeMap<u64, Value>,
    workplaces: BTreeMap<u64, Value>,
    resource_kinds: BTreeMap<u64, String>,
    events: Vec<Value>,
    opportunities: BTreeMap<String, Value>,
    exploration: usize,
    rejections: u64,
    requests: u64,
}
impl RegisteredAgent {
    fn connect(host: &OwnedHost, home: &Path) -> Self {
        let metadata: Value =
            serde_json::from_slice(&std::fs::read(home.join("herp/agent.json")).unwrap()).unwrap();
        let client = RpcClient::new(&host.url).unwrap();
        let registered = client
            .call(
                "agent/register",
                json!({"agent_id":metadata["agent_id"],"token":metadata["token"]}),
            )
            .unwrap_or_else(|_| panic!("disposable credentials could not register"));
        let mut agent = Self {
            client,
            session: registered["session"].as_str().unwrap().into(),
            known: BTreeMap::new(),
            workplaces: BTreeMap::new(),
            resource_kinds: BTreeMap::new(),
            events: Vec::new(),
            opportunities: BTreeMap::new(),
            exploration: 0,
            rejections: 0,
            requests: 0,
        };
        let enter = agent.call("agent/enter_world", json!({}));
        assert!(enter["character"].is_object());
        let pending = agent.call("agent/take_control", json!({}));
        assert_ne!(
            pending["in_control"], true,
            "paused control must remain queued"
        );
        let before = agent.call("agent/enter_world", json!({}))["tick"].clone();
        assert_eq!(
            before, pending["tick"],
            "agent bootstrap advanced the paused clock"
        );
        host.step(1); // Operator applies the recorded claim. The agent never steps.
        assert_eq!(
            agent.call("agent/take_control", json!({}))["in_control"],
            true
        );
        agent
    }

    fn call(&mut self, method: &str, mut args: Value) -> Value {
        assert!(
            method.starts_with("agent/"),
            "agent has no operator methods"
        );
        assert!(args.get("unit").is_none() && args.get("controller").is_none());
        args["session"] = json!(self.session);
        self.requests += 1;
        self.client
            .call(method, args)
            .unwrap_or_else(|error| match error {
                ClientError::Rpc(error) => panic!(
                    "registered {method} RPC {}: {}",
                    error.code,
                    error
                        .message
                        .replace(&self.session, "[session]")
                        .chars()
                        .take(240)
                        .collect::<String>()
                ),
                ClientError::Transport(_) => panic!("registered {method} transport failed"),
            })
    }

    fn observe(&mut self) -> Value {
        let obs = self.call("agent/observe", json!({"events":30}));
        // Observing acknowledges delivery. Keep bounded legitimate client
        // memory so nested observations cannot discard an actual public receipt.
        self.events.extend(
            obs["recent_events"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        if self.events.len() > 512 {
            self.events.drain(..self.events.len() - 512);
        }
        assert_eq!(obs["omniscient"], false);
        for field in ["remembered", "sensed"] {
            for perceived in obs["sections"]["perception"][field]
                .as_array()
                .into_iter()
                .flatten()
            {
                self.known
                    .insert(perceived["id"].as_u64().unwrap(), perceived.clone());
            }
        }
        // Affordances are a full current observation, unlike the client's own
        // historical map. Expired offers must trigger discovery again instead
        // of sending the actor back to a trader's forgotten position forever.
        self.opportunities.clear();
        // Learned source identities and kinds are static map knowledge, just
        // like workplace recipes. Do not cache live remaining stock as an offer.
        for resource in obs["sections"]["village"]["environment"]["resources"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let (Some(id), Some(kind)) = (resource["id"].as_u64(), resource["kind"].as_str()) {
                self.resource_kinds.insert(id, kind.to_owned());
            }
        }
        for place in obs["sections"]["village"]["environment"]["workstations"]
            .as_array()
            .into_iter()
            .flatten()
        {
            self.workplaces
                .insert(place["id"].as_u64().unwrap(), place.clone());
        }
        for a in obs["affordances"].as_array().unwrap() {
            self.opportunities
                .insert(a["id"].as_str().unwrap().to_owned(), a.clone());
        }
        obs
    }

    fn act(&mut self, args: Value) -> Value {
        let result = self.call("agent/act", args);
        if result["accepted"] != true {
            self.rejections += 1;
        }
        result
    }

    fn walk(&mut self, host: &OwnedHost, pos: Value) {
        let result = self.act(json!({"steps":[
            {"verb":"order","args":{"order":{"type":"Move","pos":pos}}},
            {"verb":"execute_orders"}]}));
        assert_eq!(result["accepted"], true);
        for _ in 0..15 {
            host.step(40);
            let obs = self.observe();
            if obs["sections"]["village"]["activity"]["busy"] == false {
                return;
            }
        }
        panic!("movement did not finish within the bounded fixture budget");
    }

    fn explore(&mut self, host: &OwnedHost) {
        // Relative exploration is chosen from the actor's own position, without
        // scene coordinates, entity IDs, preset reads or an optimal route.
        let obs = self.observe();
        let pos = &obs["sections"]["self"]["pos"];
        let directions = [(4000, 0), (0, 4000), (-4000, 0), (0, -4000)];
        let (x, z) = directions[self.exploration % directions.len()];
        let length = (self.exploration / 2 + 1) as i64;
        self.exploration += 1;
        assert!(
            self.exploration <= 24,
            "perceived discovery exceeded exploration budget"
        );
        self.walk(
            host,
            json!([
                pos[0].as_i64().unwrap() + x * length,
                pos[1].as_i64().unwrap() + z * length
            ]),
        );
    }

    fn discover(&mut self, host: &OwnedHost, verb: &str, kind: &str) -> Value {
        let mut searched_places = BTreeSet::new();
        for _ in 0..25 {
            let obs = self.observe();
            if let Some(affordance) = self.opportunities.values().find(|a| {
                a["verb"] == verb
                    && (!matches!(verb, "buy" | "sell" | "rest")
                        || obs["sections"]["perception"]["sensed"]
                            .as_array()
                            .is_some_and(|entries| {
                                entries.iter().any(|entry| entry["id"] == a["target"])
                            }))
                    && (kind.is_empty()
                        || a["args"]["kind"]
                            .as_str()
                            .is_some_and(|keys| keys.split(", ").any(|k| k == kind))
                        || (verb == "gather"
                            && a["label"].as_str().is_some_and(|label| {
                                label.starts_with(&format!("Gather {kind} ("))
                            })))
            }) {
                return affordance.clone();
            }
            // A remembered workplace is a search destination, never a claim
            // about a merchant's live position or current inventory. Use only
            // learned public products/demand and remembered coordinates.
            if matches!(verb, "buy" | "sell" | "rest") {
                let own = &obs["sections"]["self"]["pos"];
                let waypoint = self
                    .workplaces
                    .iter()
                    .filter_map(|(id, place)| {
                        let relevant = if verb == "rest" {
                            place["role"] == "innkeeper"
                        } else {
                            let goods = if verb == "buy" { "products" } else { "accepts" };
                            place[goods]
                                .as_array()
                                .is_some_and(|goods| goods.contains(&json!(kind)))
                        };
                        if !relevant || searched_places.contains(id) {
                            return None;
                        }
                        let pos = self.known.get(id)?["pos"].clone();
                        let dx = pos[0].as_i64()? - own[0].as_i64()?;
                        let dz = pos[1].as_i64()? - own[1].as_i64()?;
                        Some((dx * dx + dz * dz, *id, pos))
                    })
                    .min_by_key(|(distance, id, _)| (*distance, *id));
                if let Some((_, id, pos)) = waypoint {
                    searched_places.insert(id);
                    self.walk(host, pos);
                    continue;
                }
            }
            if self.exploration >= 12 {
                let public: Vec<_> = obs["affordances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .take(30)
                    .map(|a| json!({"verb":a["verb"],"label":a["label"],"args":a["args"]}))
                    .collect();
                panic!(
                    "discovery phase {verb}/{kind} exhausted at {}: public affordances {public:?}",
                    obs["sections"]["self"]["pos"]
                );
            }
            self.explore(host);
        }
        panic!("could not discover {verb}/{kind} through registered observations");
    }

    fn approach(&mut self, host: &OwnedHost, target: u64) {
        let mut trace = Vec::new();
        for _ in 0..8 {
            let obs = self.observe();
            trace.push(
                json!({"tick":obs["tick"],"self_pos":obs["sections"]["self"]["pos"],
                "target":target,"known":self.known.get(&target),
                "activity":obs["sections"]["village"]["activity"]}),
            );
            if obs["sections"]["perception"]["within_reach"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(target)))
            {
                return;
            }
            let pos = self.known.get(&target).expect("target was perceived")["pos"].clone();
            self.walk(host, pos);
        }
        panic!(
            "perceived target could not be reached; re-observe moving trader: {trace}",
            trace = json!(trace)
        );
    }

    fn trade(&mut self, host: &OwnedHost, direction: &str, kind: &str, quantity: u64) -> Value {
        for _ in 0..8 {
            let affordance = self.discover(host, direction, kind);
            let target = affordance["target"].as_u64().unwrap();
            self.approach(host, target);
            let obs = self.observe();
            let offer = obs["sections"]["village"]["nearby_market"]
                .as_array()
                .unwrap()
                .iter()
                .find(|merchant| merchant["id"] == target)
                .and_then(|merchant| {
                    merchant["offers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|o| o["direction"] == direction && o["kind"] == kind)
                })
                .cloned();
            if let Some(offer) = offer
                && offer["available"] == true
            {
                let result = self.act(json!({"verb":direction,"target":target,"args":{"kind":kind,"quantity":quantity}}));
                if result["accepted"] == true {
                    host.step(1);
                    return offer;
                }
            }
            host.step(100); // Operator wait for stock/reach; no game state injection.
        }
        panic!("no available {direction} opportunity for {kind} within retry budget");
    }
}

struct Evidence {
    fixture: &'static str,
    passed: bool,
}
impl Evidence {
    fn new(fixture: &'static str) -> Self {
        Self {
            fixture,
            passed: false,
        }
    }
    fn pass(&mut self, data: Value) {
        self.passed = true;
        println!(
            "ISLAND_PLAYTEST_SUMMARY {}",
            json!({"ok":true,"fixture":self.fixture,
            "seed":data["seed"],"world_preset":"island_village","owned_hosts_stopped":true,
        "clock_owner":"operator harness; paused simulation, not wall-time performance evidence",
        "real_provider":false,"data":data})
        );
    }
}
impl Drop for Evidence {
    fn drop(&mut self) {
        if !self.passed {
            println!(
                "ISLAND_PLAYTEST_SUMMARY {}",
                json!({"ok":false,"fixture":self.fixture,
                "real_provider":false,"reason":"rehearsal assertion failed; inspect the test failure; owned child guards perform cleanup"})
            );
        }
    }
}

#[test]
fn registered_mcp_stdio_connects_controls_observes_acts_and_keeps_notes() {
    let mut evidence = Evidence::new("registered_mcp_stdio_herp_control_action_notes");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(50);
    let home = DisposableHome::new("mcp");
    let host = OwnedHost::start_with_timeout(&home.0, false, Duration::from_secs(15));
    let seed = host.seed;
    let metadata: Value = serde_json::from_slice(
        &std::fs::read(home.0.join("herp/agent.json")).expect("owned fixture metadata"),
    )
    .expect("owned fixture metadata is JSON");
    let token = metadata["token"]
        .as_str()
        .expect("private fixture credential")
        .to_owned();
    let mut mcp = OwnedMcp::start(&host, deadline, token);
    let initialized = mcp.request(
        "initialize",
        json!({
            "protocolVersion":"2025-11-25","capabilities":{},
            "clientInfo":{"name":"island-playtest","version":"1"}
        }),
    );
    assert_eq!(initialized["protocolVersion"], "2025-11-25");
    mcp.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let listed = mcp.request("tools/list", json!({}));
    let tools = listed["tools"].as_array().expect("MCP tool definitions");
    for name in [
        "register",
        "enter_world",
        "take_control",
        "observe",
        "act",
        "talk",
        "notes_add",
        "notes_list",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "registered MCP offers {name}"
        );
    }
    for tool in tools {
        assert!(
            !matches!(tool["name"].as_str(), Some("attach" | "list_units")),
            "registered MCP does not offer operator control tools"
        );
        for forbidden in ["unit", "controller", "session"] {
            assert!(
                tool["inputSchema"]["properties"].get(forbidden).is_none(),
                "registered MCP supplies identity privately"
            );
        }
    }
    assert_eq!(
        mcp.tool(
            "register",
            json!({"agent_id":metadata["agent_id"],"token":metadata["token"]})
        )["registered"],
        true
    );
    let entered = mcp.tool("enter_world", json!({}));
    assert!(entered["character"].is_object());
    let pending = mcp.tool("take_control", json!({}));
    assert_ne!(
        pending["in_control"], true,
        "MCP must not advance a paused host to claim control"
    );
    assert_eq!(pending["tick"], entered["tick"]);
    host.step(1); // Explicit operator harness, never an MCP game tool.
    assert_eq!(mcp.tool("take_control", json!({}))["in_control"], true);
    let observed = mcp.tool("observe", json!({"format":"json","events":20}));
    assert_eq!(observed["omniscient"], false);
    assert_eq!(observed["sections"]["village"]["name"], "Herp");
    let actor = observed["unit"].clone();
    let spoken = mcp.tool("talk", json!({"text":"Hello island"}));
    assert_eq!(spoken["accepted"], true);
    host.step(3);
    let after_speech = mcp.tool("observe", json!({"format":"json","events":30}));
    assert!(
        after_speech["recent_events"]
            .as_array()
            .expect("speech events")
            .iter()
            .any(|event| {
                event["unit"] == actor
                    && event["event"]["type"] == "Effect"
                    && event["event"]["effect"]["type"] == "VillageSpeech"
                    && event["event"]["effect"]["text"] == "Hello island"
            }),
        "MCP talk must produce the speaker's public recorded speech"
    );
    let action = mcp.tool("act", json!({"verb":"write_diary","args":{"title":"MCP arrival","body":"HERP_MCP_COMPLETED_ACTION"}}));
    assert_eq!(action["accepted"], true);
    host.step(3);
    let after = mcp.tool("observe", json!({"format":"json","events":30}));
    assert_eq!(after["unit"], actor);
    // Command plans do not create queued-leaf ExternalAction lifecycle events.
    // The operator verifies the resulting physical page; actor decisions above
    // still use only the registered MCP interface and its own observation.
    let state = host.operator_call("world/state", json!({}));
    let pages = state["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["sim_id"] == actor)
        .unwrap()["components"]["island_diary"]["pages"]
        .as_array()
        .unwrap();
    assert_eq!(
        pages.len(),
        1,
        "MCP action must complete, not merely be accepted"
    );
    assert_eq!(pages[0]["title"], "MCP arrival");
    assert_eq!(pages[0]["body"], "HERP_MCP_COMPLETED_ACTION");
    assert!(pages[0]["tick"].as_u64().unwrap() <= after["tick"].as_u64().unwrap());
    let note = mcp.tool(
        "notes_add",
        json!({"book":"diary","title":"MCP connection","body":"HERP_MCP_NOTE_PERSISTED"}),
    );
    assert_eq!(note["accepted"], true);
    let listed = mcp.tool("notes_list", json!({"book":"diary"}));
    let notes = listed["notes"].as_array().expect("MCP diary notes");
    assert!(
        notes
            .iter()
            .any(|note| note["title"] == "MCP connection"
                && note["body"] == "HERP_MCP_NOTE_PERSISTED")
    );
    let requests = mcp.next_id;
    mcp.stop();
    host.stop_with_timeout(Duration::from_secs(5));
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "MCP fixture exceeded sixty seconds"
    );
    evidence.pass(json!({"seed":seed,"hero":"Herp","transport":"registered MCP stdio to actual HTTP explorer host",
        "protocol":"2025-11-25","requests":requests,"action_completed":true,"notes_round_trip":true,
        "credential_fields_returned":false,"fixture_wall_ms":started.elapsed().as_millis(),
        "interventions":["operator applies control claim and advances paused clock","operator verifies the completed physical diary page"],"real_provider":false}));
}

#[test]
fn registered_cold_income_needs_and_save_resume() {
    let mut evidence = Evidence::new("registered_cold_income_needs_save_resume");
    let started = Instant::now();
    let home = DisposableHome::new("economy");
    let host = OwnedHost::start(&home.0, false);
    let seed = host.seed;
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    let initial = agent.observe();
    assert_eq!(initial["sections"]["village"]["coins"], 0);
    let gather = agent.discover(&host, "gather", "log");
    let patch = gather["target"].as_u64().unwrap();
    agent.approach(&host, patch);
    // Real agent requests exercise interruption/retry. No fixture component or
    // wallet edits: cancellation must not produce a free item after its old end.
    let empty_pack = agent.observe()["sections"]["inventory"]["items"].clone();
    assert_eq!(
        agent.act(json!({"verb":"gather","target":patch}))["accepted"],
        true
    );
    host.step(1);
    assert_eq!(
        agent.observe()["sections"]["village"]["activity"]["kind"],
        "gathering"
    );
    assert_eq!(
        agent.act(json!({"verb":"gather","target":patch}))["accepted"],
        false,
        "repeating accepted work must not create a second gathering job"
    );
    assert_eq!(agent.act(json!({"verb":"stop_work"}))["accepted"], true);
    host.step(401);
    let cancelled = agent.observe();
    assert_eq!(cancelled["sections"]["village"]["activity"]["busy"], false);
    assert_eq!(cancelled["sections"]["inventory"]["items"], empty_pack);
    for _ in 0..5 {
        assert_eq!(
            agent.act(json!({"verb":"gather","target":patch}))["accepted"],
            true
        );
        for _ in 0..10 {
            host.step(40);
            if agent.observe()["sections"]["village"]["activity"]["busy"] == false {
                break;
            }
        }
    }
    let sale = agent.trade(&host, "sell", "log", 4);
    let after_sale = agent.observe();
    assert_eq!(
        after_sale["sections"]["village"]["coins"],
        sale["unit_price"].as_u64().unwrap() * 4
    );
    let first_sale_tick = after_sale["tick"].as_u64().unwrap();
    assert!(
        first_sale_tick <= 12_000,
        "first sale missed ten minutes of 1x simulation"
    );
    let mut purchases = Vec::new();
    for (kind, verb, need) in [
        ("bread", "eat", "hunger"),
        ("village_water", "drink", "thirst"),
    ] {
        let offer = agent.trade(&host, "buy", kind, 1);
        let before = agent.observe()["sections"]["village"]["needs"][need]
            .as_i64()
            .unwrap();
        assert_eq!(agent.act(json!({"verb":verb}))["accepted"], true);
        host.step(1);
        assert!(
            agent.observe()["sections"]["village"]["needs"][need]
                .as_i64()
                .unwrap()
                > before
        );
        purchases.push(json!({"kind":kind,"unit_price":offer["unit_price"]}));
    }
    let bed = agent.discover(&host, "rest", "");
    let keeper = bed["target"].as_u64().unwrap();
    agent.approach(&host, keeper);
    let before = agent.observe()["sections"]["village"]["needs"]["energy"]
        .as_i64()
        .unwrap();
    assert_eq!(
        agent.act(json!({"verb":"rest","target":keeper}))["accepted"],
        true
    );
    host.step(302);
    let before_save = agent.observe();
    assert!(
        before_save["sections"]["village"]["needs"]["energy"]
            .as_i64()
            .unwrap()
            > before
    );
    assert!(before_save["tick"].as_u64().unwrap() < 24_000);
    let coins = before_save["sections"]["village"]["coins"].clone();
    let inventory = before_save["sections"]["inventory"]["items"].clone();
    assert!(
        inventory
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "log" && i["stack"] == 1)
    );
    let old_actor = before_save["unit"].clone();
    let old_session = agent.session.clone();
    assert_eq!(
        agent.call(
            "agent/notes/add",
            json!({"book":"diary","title":"First day","body":"HERP_COMPLETED_INCOME_AND_NEEDS"})
        )["accepted"],
        true
    );
    let requests = agent.requests;
    let rejections = agent.rejections;
    host.stop();
    let resumed = OwnedHost::start(&home.0, true);
    assert!(
        RpcClient::new(&resumed.url)
            .unwrap()
            .call("agent/observe", json!({"session":old_session}))
            .is_err(),
        "a restarted host must require a new registration handshake"
    );
    let mut agent = RegisteredAgent::connect(&resumed, &home.0);
    let after = agent.observe();
    assert_eq!(after["unit"], old_actor);
    assert_eq!(after["sections"]["village"]["coins"], coins);
    assert_eq!(after["sections"]["inventory"]["items"], inventory);
    let diary = agent.call("agent/notes/list", json!({"book":"diary"}));
    assert_eq!(diary["notes"].as_array().unwrap().len(), 1);
    assert!(diary.to_string().contains("First day"));
    resumed.stop();
    evidence.pass(json!({"seed":seed,"first_sale_tick":first_sale_tick,
        "cycle_tick":before_save["tick"],"purchases":purchases,"requests":requests,
        "tool_rejections":rejections,"fixture_wall_ms":started.elapsed().as_millis(),
        "hero":"Herp","cancelled_work_has_no_output":true,
        "restart_requires_registration":true,"diary_preserved":true,
        "interventions":["operator applies control claim and advances paused clock"],
        "discovery":"observed affordances and perceived/remembered positions only"}));
}

#[test]
fn registered_living_raw_materials_to_finished_offer_and_npc_purchase_resume() {
    // The chosen goal is a hunting spear. Concrete good/recipe identifiers,
    // ingredient counts, stations and routes all come from game observations.
    // This is a bounded scripted engineering rehearsal, not an LLM play claim.
    fn inventory_count(obs: &Value, kind: &str) -> u64 {
        obs["sections"]["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["kind"] == kind)
            .map(|item| item["stack"].as_u64().unwrap_or(1))
            .sum()
    }
    fn budget(obs: &Value, initial: u64) {
        assert!(
            obs["tick"].as_u64().unwrap().saturating_sub(initial) <= 12_000,
            "finished-good first income exceeded its ten-minute simulation budget"
        );
        assert_eq!(obs["sections"]["village"]["alive"], true);
    }
    fn wait_work(agent: &mut RegisteredAgent, host: &OwnedHost, initial: u64) -> Value {
        for _ in 0..60 {
            host.step(20);
            let obs = agent.observe();
            budget(&obs, initial);
            if obs["sections"]["village"]["activity"]["busy"] == false {
                return obs;
            }
        }
        panic!("physical work did not finish within its bounded wait");
    }
    fn make_good(
        agent: &mut RegisteredAgent,
        host: &OwnedHost,
        good: &str,
        needed: u64,
        initial: u64,
        depth: u32,
        completed: &mut Vec<Value>,
    ) {
        assert!(depth <= 8, "observed material recipe graph must be bounded");
        let mut visited = BTreeSet::new();
        for _ in 0..32 {
            let obs = agent.observe();
            budget(&obs, initial);
            if inventory_count(&obs, good) >= needed {
                return;
            }
            let recipe = agent.workplaces.iter().find_map(|(station, place)| {
                place["recipes"]
                    .as_array()?
                    .iter()
                    .find(|recipe| {
                        recipe["outputs"].as_array().is_some_and(|outputs| {
                            outputs.iter().any(|output| output["kind"] == good)
                        })
                    })
                    .map(|recipe| (*station, recipe.clone()))
            });
            if let Some((station, recipe)) = recipe {
                let out = recipe["outputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|output| output["kind"] == good)
                    .unwrap()["count"]
                    .as_u64()
                    .unwrap();
                let batches = needed
                    .saturating_sub(inventory_count(&obs, good))
                    .div_ceil(out);
                for input in recipe["inputs"].as_array().unwrap() {
                    make_good(
                        agent,
                        host,
                        input["kind"].as_str().unwrap(),
                        input["count"].as_u64().unwrap() * batches,
                        initial,
                        depth + 1,
                        completed,
                    );
                }
                // A shared station can be occupied by a resident. Re-observe
                // the advertised action rather than evicting or editing them.
                let mut crafted = false;
                for _ in 0..30 {
                    agent.approach(host, station);
                    let fresh = agent.observe();
                    budget(&fresh, initial);
                    let verb = format!("craft:{}", recipe["name"].as_str().unwrap());
                    let action = fresh["affordances"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|a| a["verb"] == verb && a["target"] == station && a["ok"] == true)
                        .cloned();
                    if let Some(action) = action {
                        let before = inventory_count(&fresh, good);
                        let accepted = agent.act(json!({"affordance":action["id"]}));
                        if accepted["accepted"] == true {
                            let after = wait_work(agent, host, initial);
                            assert!(
                                inventory_count(&after, good) >= before + out,
                                "an accepted craft must produce its real declared output"
                            );
                            completed.push(json!({"kind":good,"recipe":recipe["name"],"inputs":recipe["inputs"],"outputs":recipe["outputs"]}));
                            crafted = true;
                            break;
                        }
                    }
                    host.step(20);
                }
                assert!(
                    crafted,
                    "shared crafting station never admitted the public recipe for {good}"
                );
                continue;
            }
            if let Some(gather) = obs["affordances"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| {
                    a["verb"] == "gather"
                        && a["label"]
                            .as_str()
                            .is_some_and(|label| label.starts_with(&format!("Gather {good} (")))
                })
                .cloned()
            {
                let target = gather["target"].as_u64().unwrap();
                agent.approach(host, target);
                let before = inventory_count(&agent.observe(), good);
                let accepted = agent.act(json!({"verb":"gather","target":target}));
                assert_eq!(
                    accepted["accepted"], true,
                    "raw gathering must use the shared checked action"
                );
                let after = wait_work(agent, host, initial);
                assert_eq!(inventory_count(&after, good), before + 1);
                completed.push(json!({"kind":good,"action":"gather","quantity":1}));
                continue;
            }
            // Search a learned producer or downstream consumer workplace. This
            // is public geographical knowledge, never a preset route or oracle.
            let search = agent.workplaces.iter().find_map(|(id, place)| {
                let relevant = place["products"]
                    .as_array()
                    .is_some_and(|goods| goods.contains(&json!(good)))
                    || place["recipes"].as_array().is_some_and(|recipes| {
                        recipes.iter().any(|recipe| {
                            recipe["inputs"].as_array().is_some_and(|inputs| {
                                inputs.iter().any(|input| input["kind"] == good)
                            })
                        })
                    });
                if relevant && !visited.contains(id) {
                    Some((*id, agent.known.get(id)?["pos"].clone()))
                } else {
                    None
                }
            });
            if let Some((id, pos)) = search {
                visited.insert(id);
                agent.walk(host, pos);
            } else {
                agent.explore(host);
            }
        }
        panic!("public discovery and real production could not supply {needed} {good}");
    }
    fn saved_cash(home: &Path) -> u64 {
        // Operator verification only. None of these private save contents are
        // supplied to the actor's decisions, routes, prices or opportunities.
        let save: Value =
            serde_json::from_slice(&std::fs::read(home.join("herp/island.world.json")).unwrap())
                .unwrap();
        let carried: u64 = save["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| {
                entity["components"]["island_wallet"]["coins"]
                    .as_u64()
                    .unwrap_or(0)
                    + entity["components"]["island_coin_purse"]["coins"]
                        .as_u64()
                        .unwrap_or(0)
            })
            .sum();
        carried
            + save["resources"]["island_village"]["fund"]
                .as_u64()
                .unwrap()
    }

    let started = Instant::now();
    let home = DisposableHome::new("living-finished-market");
    let host = OwnedHost::start_with_preset(&home.0, false, "living_island");
    let seed = host.seed;
    let currency_total = saved_cash(&home.0);
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    let first = agent.observe();
    let initial_tick = first["tick"].as_u64().unwrap();
    assert_eq!(first["sections"]["village"]["coins"], 0);
    assert_eq!(
        first["sections"]["village"]["economy"]["model"],
        "needs_driven"
    );
    let good = first["sections"]["village"]["economy"]["market"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row["kind"].as_str().filter(|kind| kind.ends_with("spear")))
        .unwrap()
        .to_owned();
    let mut completed = Vec::new();
    make_good(&mut agent, &host, &good, 1, initial_tick, 0, &mut completed);
    assert!(
        completed
            .iter()
            .filter(|stage| stage["recipe"].is_string())
            .count()
            >= 3,
        "the finished good must join distinct smelting and timber chains"
    );
    assert!(
        completed
            .iter()
            .filter(|stage| stage["action"] == "gather")
            .count()
            >= 4,
        "the actor must gather the real raw materials, not receive a fixture grant"
    );
    let crafted = agent.observe();
    assert_eq!(
        crafted["sections"]["village"]["coins"], 0,
        "no intermediate subsidy or forced sale"
    );
    let item = crafted["sections"]["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == good)
        .unwrap()["id"]
        .clone();
    let own_home = &crafted["sections"]["village"]["home"];
    agent.walk(&host, json!([own_home["x"], own_home["z"]]));
    assert_eq!(
        agent.act(json!({"verb":"offer_goods","args":{"kind":good,"quantity":1,"limit_price":1}}))
            ["accepted"],
        true
    );
    host.step(1);
    let listed = agent.observe();
    budget(&listed, initial_tick);
    assert_eq!(
        listed["sections"]["village"]["coins"], 0,
        "posting is not a sale"
    );
    let listings = listed["sections"]["village"]["economy"]["own_offers"].clone();
    assert!(
        listings
            .as_array()
            .unwrap()
            .iter()
            .any(|offer| offer["kind"] == good && offer["quantity"] == 1)
    );
    let actor = listed["unit"].clone();
    let old_session = agent.session.clone();
    host.stop();
    assert_eq!(saved_cash(&home.0), currency_total);

    let resumed = OwnedHost::start_with_preset(&home.0, true, "living_island");
    assert!(
        RpcClient::new(&resumed.url)
            .unwrap()
            .call("agent/observe", json!({"session":old_session}))
            .is_err()
    );
    let mut agent = RegisteredAgent::connect(&resumed, &home.0);
    let restored = agent.observe();
    assert_eq!(restored["unit"], actor);
    // A quote may adapt on the registration tick. The saved listing's kind,
    // stated quantity and price floor must survive, independently of that quote.
    let restored_offers = &restored["sections"]["village"]["economy"]["own_offers"];
    for expected in listings.as_array().unwrap() {
        let actual = restored_offers
            .as_array()
            .unwrap()
            .iter()
            .find(|offer| offer["kind"] == expected["kind"])
            .expect("saved offer survives resume");
        assert_eq!(actual["quantity"], expected["quantity"]);
        assert_eq!(actual["minimum_unit_price"], expected["minimum_unit_price"]);
    }
    assert!(
        restored["sections"]["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|owned| owned["id"] == item)
    );
    let mut sold = None;
    for _ in 0..1200 {
        resumed.step(10);
        let obs = agent.observe();
        budget(&obs, initial_tick);
        if obs["sections"]["village"]["coins"].as_u64().unwrap() > 0 {
            sold = Some(obs);
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(240),
            "bounded registered market fixture exceeded its wall budget"
        );
    }
    let sold = sold.expect("a funded NPC must physically purchase the useful finished good");
    let income = sold["sections"]["village"]["coins"].as_u64().unwrap();
    assert!(income > 0);
    assert_eq!(inventory_count(&sold, &good), 0);
    assert!(
        !sold["sections"]["village"]["economy"]["own_offers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|offer| offer["kind"] == good)
    );
    let receipt = sold["recent_events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| {
            event["event"]["effect"]["type"] == "VillageSaleCompleted"
                && event["event"]["effect"]["kind"] == good
        })
        .expect("real sale receipt belongs to the registered seller")["event"]["effect"]
        .clone();
    assert_eq!(receipt["quantity"], 1);
    assert_eq!(receipt["coins"].as_u64(), Some(income));
    assert_eq!(receipt["unit_price"].as_u64(), Some(income));
    let state = resumed.operator_call("world/state", json!({}));
    let entities = state["entities"].as_array().unwrap();
    let transferred = entities
        .iter()
        .find(|entity| entity["sim_id"] == item)
        .expect("the same nonstackable finished item survives transfer");
    let buyer_id = transferred["components"]["carried_by"]["holder"].clone();
    assert_ne!(buyer_id, actor);
    let buyer = entities
        .iter()
        .find(|entity| entity["sim_id"] == buyer_id)
        .unwrap();
    assert_ne!(buyer["components"]["island_villager"]["role"], "visitor");
    let seller = entities
        .iter()
        .find(|entity| entity["sim_id"] == actor)
        .unwrap();
    let buyer_pos = &buyer["components"]["pos"];
    let seller_pos = &seller["components"]["pos"];
    let dx = buyer_pos["x"].as_i64().unwrap() - seller_pos["x"].as_i64().unwrap();
    let dz = buyer_pos["z"].as_i64().unwrap() - seller_pos["z"].as_i64().unwrap();
    assert!(
        dx * dx + dz * dz <= 1_500 * 1_500,
        "purchase must occur at physical reach"
    );
    assert!(sold["tick"].as_u64().unwrap().saturating_sub(initial_tick) <= 12_000);
    assert!(sold["tick"].as_u64().unwrap().saturating_sub(initial_tick) < 24_000);

    // Spend only the actual sale proceeds on a bodily need. The actor discovers
    // the clean-water good, its moving seller and current quote through the
    // same registered observations; the harness neither supplies nor edits it.
    let water_kind = sold["sections"]["village"]["economy"]["market"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row["kind"].as_str().filter(|kind| kind.ends_with("water")))
        .expect("public market describes clean water")
        .to_owned();
    let original_water = inventory_count(&sold, &water_kind);
    let mut water_purchase = None;
    let mut water_trace = Vec::new();
    for _ in 0..8 {
        let opportunity = agent.discover(&resumed, "buy", &water_kind);
        let merchant = opportunity["target"].as_u64().unwrap();
        agent.approach(&resumed, merchant);
        let before = agent.observe();
        assert_eq!(
            before["sections"]["village"]["coins"].as_u64(),
            Some(income)
        );
        assert!(
            before["tick"]
                .as_u64()
                .unwrap()
                .saturating_sub(initial_tick)
                < 24_000
        );
        let quote = before["sections"]["village"]["nearby_market"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trader| trader["id"] == merchant)
            .and_then(|trader| {
                trader["offers"]
                    .as_array()?
                    .iter()
                    .find(|offer| offer["direction"] == "buy" && offer["kind"] == water_kind)
            })
            .cloned();
        if let Some(quote) = quote {
            let price = quote["unit_price"]
                .as_u64()
                .expect("water has a public current quote");
            water_trace.push(json!({"tick":before["tick"],"cash":income,"quote":quote}));
            assert!(
                price <= income,
                "earned spear income cannot afford observed drinking water: income={income}, quote={price}, trace={water_trace:?}"
            );
            if quote["available"] == true {
                let accepted = agent.act(json!({"verb":"buy","target":merchant,
                    "args":{"kind":water_kind,"quantity":1,"limit_price":price}}));
                if accepted["accepted"] == true {
                    resumed.step(1);
                    let acquired = agent.observe();
                    assert_eq!(
                        acquired["sections"]["village"]["coins"].as_u64(),
                        Some(income - price)
                    );
                    assert_eq!(
                        inventory_count(&acquired, &water_kind),
                        original_water + 1,
                        "the paid purchase must deliver exactly one real water item"
                    );
                    water_purchase = Some((price, acquired));
                    break;
                }
            }
        }
        resumed.step(100);
    }
    let (water_price, acquired) = water_purchase.unwrap_or_else(|| {
        panic!("no physical drinking-water purchase was available within the bounded retry: {water_trace:?}")
    });
    let thirst_before = acquired["sections"]["village"]["needs"]["thirst"]
        .as_i64()
        .unwrap();
    assert_eq!(agent.act(json!({"verb":"drink"}))["accepted"], true);
    resumed.step(1);
    let drank = agent.observe();
    let thirst_after = drank["sections"]["village"]["needs"]["thirst"]
        .as_i64()
        .unwrap();
    assert!(
        thirst_after > thirst_before,
        "drinking the purchased water must improve the real Thirst need"
    );
    assert_eq!(
        inventory_count(&drank, &water_kind),
        original_water,
        "drinking consumes exactly the purchased unit"
    );
    assert_eq!(
        drank["sections"]["village"]["coins"].as_u64(),
        Some(income - water_price)
    );
    assert!(
        drank["tick"].as_u64().unwrap().saturating_sub(initial_tick) < 24_000,
        "earned-income need cycle exceeded its normal-day simulation budget"
    );
    assert_eq!(drank["sections"]["village"]["alive"], true);
    resumed.stop();
    assert_eq!(
        saved_cash(&home.0),
        currency_total,
        "the full finite world purse is conserved across the market cycle and restart"
    );
    println!(
        "ISLAND_PLAYTEST_SUMMARY {}",
        json!({"ok":true,"fixture":"registered_living_finished_goods_market",
        "world_preset":"living_island","seed":seed,"hero":"Herp","starting_coins":0,"income":income,
        "first_sale_elapsed_ticks":sold["tick"].as_u64().unwrap()-initial_tick,"stages":completed,
        "need_cycle_elapsed_ticks":drank["tick"].as_u64().unwrap()-initial_tick,
        "water_unit_price":water_price,"coins_after_water":income-water_price,
        "thirst_before":thirst_before,"thirst_after":thirst_after,"earned_income_pays_drinking_water":true,
        "same_item_transferred":true,"listing_preserved_across_resume":true,"currency_total":currency_total,
        "owned_hosts_stopped":true,"real_provider":false,
        "interventions":["operator advances paused clock","operator verifies final item ownership and saved finite currency; no actor decisions use those reads"],
        "discovery":"registered observations, static learned workplaces, actual recipe inputs and outputs; no preset coordinates or catalog IDs"})
    );
}

#[test]
fn registered_disposable_death_new_life_and_public_diary_privacy() {
    let mut evidence = Evidence::new("registered_forced_death_new_life_diary_privacy");
    let home = DisposableHome::new("death");
    let host = OwnedHost::start(&home.0, false);
    let seed = host.seed;
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    let actor = agent.observe()["unit"].clone();
    for (book, body) in [
        ("diary", "PUBLIC_REHEARSAL_DIARY"),
        ("memory", "PRIVATE_REHEARSAL_MEMORY"),
        ("proposals", "PRIVATE_REHEARSAL_PROPOSAL"),
    ] {
        assert_eq!(
            agent.call(
                "agent/notes/add",
                json!({"book":book,"title":"Fixture page","body":body})
            )["accepted"],
            true
        );
    }
    // Separate labeled forced-death fixture. Only the operator sends recorded
    // SetNeed commands; this is never used to prove unaided survival.
    for need in ["Hunger", "Thirst"] {
        host.operator_call(
            "npc/command",
            json!({"cmd":{"type":"SetNeed","unit":actor,"need":need,"value":0}}),
        );
    }
    host.step(2000);
    assert_eq!(agent.observe()["sections"]["village"]["alive"], false);
    host.stop();
    let host = OwnedHost::start(&home.0, true);
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    assert_eq!(
        agent.observe()["sections"]["village"]["alive"],
        false,
        "saving must not resurrect the dead actor"
    );
    let old_session = agent.session.clone();
    assert_eq!(agent.act(json!({"verb":"new_life"}))["accepted"], true);
    host.step(1);
    assert!(
        agent
            .client
            .call("agent/observe", json!({"session":old_session}))
            .is_err(),
        "old control must be revoked"
    );
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    let successor = agent.observe();
    assert_ne!(successor["unit"], actor);
    assert_eq!(successor["sections"]["village"]["coins"], 0);
    for book in ["diary", "memory", "proposals"] {
        let notes = agent.call("agent/notes/list", json!({"book":book}));
        assert!(
            notes["notes"].as_array().unwrap().is_empty(),
            "successor's books must be fresh"
        );
    }
    let diary = agent.discover(&host, "pick_up", "");
    let mut target = diary["target"].as_u64().unwrap();
    // Select the known diary itself, rather than an arbitrary nearby item.
    for _ in 0..8 {
        let obs = agent.observe();
        if let Some(perceived) = obs["sections"]["perception"]["sensed"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| {
                p["name"]
                    .as_str()
                    .is_some_and(|name| name.contains("diary"))
            })
        {
            target = perceived["id"].as_u64().unwrap();
            break;
        }
        agent.explore(&host);
    }
    agent.approach(&host, target);
    assert_eq!(
        agent.act(json!({"verb":"interact","target":target,"args":{"verb":"pick_up"}}))["accepted"],
        true
    );
    host.step(40);
    let found = agent.observe();
    let books = found["sections"]["village"]["found_diaries"]
        .as_array()
        .unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0]["page_count"], 1);
    let diary_id = books[0]["id"].clone();
    assert_eq!(
        agent.act(json!({"verb":"read_diary","target":diary_id}))["accepted"],
        true
    );
    host.step(1);
    let public = agent.observe().to_string();
    assert!(public.contains("PUBLIC_REHEARSAL_DIARY"));
    assert!(
        !public.contains("PRIVATE_REHEARSAL_MEMORY")
            && !public.contains("PRIVATE_REHEARSAL_PROPOSAL")
    );
    host.stop();
    evidence.pass(json!({"seed":seed,
        "permanent_death_saved":true,"new_identity":true,"old_control_revoked":true,
        "fresh_private_books":true,"public_diary_pages":1,
        "interventions":["operator records SetNeed(Hunger,0) and SetNeed(Thirst,0) in a separate disposable death fixture","operator advances paused clock"]}));
}

#[test]
fn registered_homestead_commons_build_crop_resume_food_and_income() {
    // Scripted registered-client evidence. All goals, material recipes, plot
    // targets and approach positions below come from the actor's observations.
    // The operator only advances the paused clock and verifies saved outcomes.
    fn count(obs: &Value, kind: &str) -> u64 {
        obs["sections"]["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["kind"] == kind)
            .map(|item| item["stack"].as_u64().unwrap_or(1))
            .sum()
    }
    fn budget(obs: &Value, initial: u64) {
        assert!(
            obs["tick"].as_u64().unwrap().saturating_sub(initial) <= 32_000,
            "registered homestead cycle exceeded its bounded simulation budget"
        );
        assert_eq!(obs["sections"]["village"]["alive"], true);
    }
    fn point(pos: &Value) -> Value {
        json!([pos["x"].as_i64().unwrap(), pos["z"].as_i64().unwrap()])
    }
    fn estate(obs: &Value, id: u64) -> Value {
        obs["sections"]["homestead"]["plots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|plot| plot["id"] == id)
            .expect("owned plot remains observable")
            .clone()
    }
    fn wait_work(agent: &mut RegisteredAgent, host: &OwnedHost, initial: u64) -> Value {
        for _ in 0..80 {
            host.step(20);
            let obs = agent.observe();
            budget(&obs, initial);
            if obs["sections"]["village"]["activity"]["busy"] == false {
                return obs;
            }
        }
        panic!("registered homestead work did not finish within its bounded wait");
    }
    fn produce(
        agent: &mut RegisteredAgent,
        host: &OwnedHost,
        good: &str,
        needed: u64,
        initial: u64,
        depth: u32,
        stages: &mut Vec<Value>,
    ) {
        assert!(depth <= 8, "observed recipe recursion must stay bounded");
        let mut searched = BTreeSet::new();
        for _ in 0..48 {
            let obs = agent.observe();
            budget(&obs, initial);
            if count(&obs, good) >= needed {
                return;
            }
            let recipe = agent.workplaces.iter().find_map(|(id, place)| {
                place["recipes"]
                    .as_array()?
                    .iter()
                    .find(|recipe| {
                        recipe["outputs"].as_array().is_some_and(|outputs| {
                            outputs.iter().any(|output| output["kind"] == good)
                        })
                    })
                    .map(|recipe| (*id, recipe.clone()))
            });
            if let Some((station, recipe)) = recipe {
                for input in recipe["inputs"].as_array().unwrap() {
                    produce(
                        agent,
                        host,
                        input["kind"].as_str().unwrap(),
                        input["count"].as_u64().unwrap(),
                        initial,
                        depth + 1,
                        stages,
                    );
                }
                let mut made = false;
                for _ in 0..30 {
                    agent.approach(host, station);
                    let fresh = agent.observe();
                    budget(&fresh, initial);
                    let verb = format!("craft:{}", recipe["name"].as_str().unwrap());
                    if let Some(action) =
                        fresh["affordances"].as_array().unwrap().iter().find(|a| {
                            a["verb"] == verb && a["target"] == station && a["ok"] == true
                        })
                    {
                        let before = count(&fresh, good);
                        assert_eq!(
                            agent.act(json!({"affordance":action["id"]}))["accepted"],
                            true
                        );
                        let after = wait_work(agent, host, initial);
                        let output = recipe["outputs"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|output| output["kind"] == good)
                            .unwrap()["count"]
                            .as_u64()
                            .unwrap();
                        assert_eq!(
                            count(&after, good),
                            before + output,
                            "checked crafting must deliver actual advertised output"
                        );
                        stages.push(json!({"action":"craft","kind":good,"recipe":recipe["name"],"inputs":recipe["inputs"],"outputs":recipe["outputs"]}));
                        made = true;
                        break;
                    }
                    host.step(20);
                }
                assert!(made, "the learned shared station never admitted {good}");
                continue;
            }
            let raw = agent
                .resource_kinds
                .iter()
                .find(|&(id, kind)| kind.as_str() == good && agent.known.contains_key(id))
                .map(|(id, _)| *id);
            if let Some(target) = raw {
                agent.approach(host, target);
                let before = count(&agent.observe(), good);
                assert_eq!(
                    agent.act(json!({"verb":"gather","target":target}))["accepted"],
                    true
                );
                let after = wait_work(agent, host, initial);
                assert_eq!(count(&after, good), before + 1);
                stages.push(json!({"action":"gather","kind":good,"quantity":1}));
                continue;
            }
            let waypoint = agent.workplaces.iter().find_map(|(id, place)| {
                let relevant = place["products"]
                    .as_array()
                    .is_some_and(|goods| goods.contains(&json!(good)))
                    || place["recipes"].as_array().is_some_and(|recipes| {
                        recipes.iter().any(|recipe| {
                            recipe["inputs"].as_array().is_some_and(|inputs| {
                                inputs.iter().any(|input| input["kind"] == good)
                            })
                        })
                    });
                if relevant && !searched.contains(id) {
                    Some((*id, agent.known.get(id)?["pos"].clone()))
                } else {
                    None
                }
            });
            if let Some((id, pos)) = waypoint {
                searched.insert(id);
                agent.walk(host, pos);
            } else {
                agent.explore(host);
            }
        }
        panic!("observed commons and recipes could not supply {needed} {good}");
    }
    fn refresh_thirst(
        agent: &mut RegisteredAgent,
        host: &OwnedHost,
        water: &str,
        initial: u64,
        stages: &mut Vec<Value>,
    ) {
        let obs = agent.observe();
        budget(&obs, initial);
        if obs["sections"]["village"]["needs"]["thirst"]
            .as_i64()
            .unwrap()
            < 40_000
        {
            produce(agent, host, water, 1, initial, 0, stages);
            assert_eq!(agent.act(json!({"verb":"drink"}))["accepted"], true);
            host.step(1);
        }
    }
    fn saved_cash(home: &Path) -> u64 {
        // Operator verification only: no private save data drives the actor.
        let save: Value =
            serde_json::from_slice(&std::fs::read(home.join("herp/island.world.json")).unwrap())
                .unwrap();
        save["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entity| {
                entity["components"]["island_wallet"]["coins"]
                    .as_u64()
                    .unwrap_or(0)
                    + entity["components"]["island_coin_purse"]["coins"]
                        .as_u64()
                        .unwrap_or(0)
            })
            .sum::<u64>()
            + save["resources"]["island_village"]["fund"]
                .as_u64()
                .unwrap()
    }

    let started = Instant::now();
    let mut evidence = Evidence::new("registered_homestead_commons_crop_food_income");
    let home = DisposableHome::new("homestead");
    let host = OwnedHost::start_with_preset(&home.0, false, "living_homestead");
    let seed = host.seed;
    let currency = saved_cash(&home.0);
    let mut agent = RegisteredAgent::connect(&host, &home.0);
    let first = agent.observe();
    let initial = first["tick"].as_u64().unwrap();
    let actor = first["unit"].clone();
    let original_home = first["sections"]["village"]["home"].clone();
    assert_eq!(first["sections"]["village"]["coins"], 0);
    assert_eq!(
        first["sections"]["village"]["economy"]["model"],
        "needs_driven"
    );
    let facts = first["sections"]["homestead"].clone();
    let materials = facts["cottage"]["inputs"]
        .as_array()
        .expect("public construction bill")
        .clone();
    let seed_kind = facts["crop"]["seed_kind"].as_str().unwrap().to_owned();
    let water_kind = facts["crop"]["water_kind"].as_str().unwrap().to_owned();
    let seed_count = facts["crop"]["seed_count"].as_u64().unwrap();
    let water_count = facts["crop"]["water_count"].as_u64().unwrap();
    let yield_count = facts["crop"]["yield_count"].as_u64().unwrap();
    assert_eq!(seed_count, 1);
    assert_eq!(yield_count, 3);
    assert_eq!(facts["crop"]["yield_kind"], seed_kind);
    let mut selected = None;
    for _ in 0..20 {
        let obs = agent.observe();
        budget(&obs, initial);
        if let Some(plot) = obs["sections"]["homestead"]["plots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|plot| {
                plot["use_kind"] == "housing" && plot["current"] == true && plot["owner"].is_null()
            })
            .min_by_key(|plot| {
                let own = &obs["sections"]["self"]["pos"];
                let dx = plot["front_door"]["x"].as_i64().unwrap() - own[0].as_i64().unwrap();
                let dz = plot["front_door"]["z"].as_i64().unwrap() - own[1].as_i64().unwrap();
                (dx * dx + dz * dz, plot["id"].as_u64().unwrap())
            })
        {
            selected = Some(plot.clone());
            break;
        }
        agent.explore(&host);
    }
    let selected = selected.expect("relative observation must discover an unowned housing plot");
    let plot = selected["id"].as_u64().unwrap();
    let door = selected["front_door"].clone();
    agent.walk(&host, point(&door));
    assert_eq!(
        agent.act(json!({"verb":"claim_plot","target":plot}))["accepted"],
        true
    );
    host.step(1);
    assert_eq!(estate(&agent.observe(), plot)["owner"], actor);
    let mut stages = Vec::new();
    for material in &materials {
        produce(
            &mut agent,
            &host,
            material["kind"].as_str().unwrap(),
            material["count"].as_u64().unwrap(),
            initial,
            0,
            &mut stages,
        );
    }
    refresh_thirst(&mut agent, &host, &water_kind, initial, &mut stages);
    agent.walk(&host, point(&door));
    let before_build = agent.observe();
    for material in &materials {
        assert_eq!(
            count(&before_build, material["kind"].as_str().unwrap()),
            material["count"].as_u64().unwrap()
        );
    }
    assert_eq!(
        agent.act(json!({"verb":"build_cottage","target":plot}))["accepted"],
        true
    );
    let built = wait_work(&mut agent, &host, initial);
    assert_eq!(estate(&built, plot)["cottage_built"], true);
    assert_eq!(built["sections"]["village"]["home"], door);
    assert_ne!(built["sections"]["village"]["home"], original_home);
    for material in &materials {
        assert_eq!(
            count(&built, material["kind"].as_str().unwrap()),
            0,
            "cottage consumes its complete real bill exactly once"
        );
    }
    let building_stages = stages.len();

    produce(
        &mut agent,
        &host,
        &seed_kind,
        seed_count,
        initial,
        0,
        &mut stages,
    );
    produce(
        &mut agent,
        &host,
        &water_kind,
        water_count,
        initial,
        0,
        &mut stages,
    );
    let own = estate(&agent.observe(), plot);
    let bed = own["beds"].as_array().unwrap().first().unwrap().clone();
    let index = bed["index"].as_u64().unwrap();
    agent.walk(&host, point(&bed["position"]));
    assert_eq!(
        agent.act(json!({"verb":"plant","target":plot,"args":{"bed":index,"kind":seed_kind}}))["accepted"],
        true
    );
    host.step(1);
    assert_eq!(
        count(&agent.observe(), &seed_kind),
        0,
        "the actual seed is spent"
    );
    assert_eq!(
        agent.act(json!({"verb":"water_crop","target":plot,"args":{"bed":index}}))["accepted"],
        true
    );
    host.step(1);
    let watered = agent.observe();
    assert_eq!(
        count(&watered, &water_kind),
        0,
        "irrigation spends real clean water"
    );
    let crop = estate(&watered, plot)["beds"][index as usize].clone();
    assert_eq!(crop["stage"], "growing");
    let ready = crop["ready_at"].as_u64().unwrap();
    assert_eq!(
        ready - crop["watered_at"].as_u64().unwrap(),
        facts["crop"]["growth_ticks"].as_u64().unwrap()
    );
    agent.walk(&host, point(&door));
    let energy = agent.observe()["sections"]["village"]["needs"]["energy"]
        .as_i64()
        .unwrap();
    assert_eq!(agent.act(json!({"verb":"rest_home"}))["accepted"], true);
    let resting = wait_work(&mut agent, &host, initial);
    assert!(
        resting["sections"]["village"]["needs"]["energy"]
            .as_i64()
            .unwrap()
            > energy
    );
    let before_save = estate(&resting, plot);
    assert_eq!(before_save["beds"][index as usize]["stage"], "growing");
    assert_eq!(resting["sections"]["village"]["coins"], 0);
    let known = agent.known.clone();
    let workplaces = agent.workplaces.clone();
    let resources = agent.resource_kinds.clone();
    let old_session = agent.session.clone();
    host.stop();
    assert_eq!(saved_cash(&home.0), currency);
    let resumed = OwnedHost::start_with_preset(&home.0, true, "living_homestead");
    assert!(
        RpcClient::new(&resumed.url)
            .unwrap()
            .call("agent/observe", json!({"session":old_session}))
            .is_err()
    );
    let mut agent = RegisteredAgent::connect(&resumed, &home.0);
    // Legitimate client memory retains learned static coordinates; offers are
    // rebuilt from fresh observations, never carried across the restart.
    for (id, entry) in known {
        agent.known.entry(id).or_insert(entry);
    }
    for (id, entry) in workplaces {
        agent.workplaces.entry(id).or_insert(entry);
    }
    for (id, kind) in resources {
        agent.resource_kinds.entry(id).or_insert(kind);
    }
    let after = agent.observe();
    assert_eq!(after["unit"], actor);
    assert_eq!(after["sections"]["village"]["home"], door);
    assert_eq!(
        estate(&after, plot),
        before_save,
        "owned estate and growing crop survive restart exactly"
    );
    while agent.observe()["tick"].as_u64().unwrap() < ready {
        refresh_thirst(&mut agent, &resumed, &water_kind, initial, &mut stages);
        let now = agent.observe()["tick"].as_u64().unwrap();
        if now < ready {
            resumed.step((ready - now).min(500));
        }
        budget(&agent.observe(), initial);
    }
    agent.walk(&resumed, point(&bed["position"]));
    let mature = agent.observe();
    assert_eq!(
        estate(&mature, plot)["beds"][index as usize]["stage"],
        "ready"
    );
    assert_eq!(count(&mature, &seed_kind), 0);
    assert_eq!(
        agent.act(json!({"verb":"harvest","target":plot,"args":{"bed":index}}))["accepted"],
        true
    );
    resumed.step(1);
    assert_eq!(count(&agent.observe(), &seed_kind), yield_count);
    let harvest_stages = stages.len();
    let bread = agent.observe()["sections"]["village"]["economy"]["market"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| row["kind"].as_str().filter(|kind| *kind == "bread"))
        .expect("public food market names bread")
        .to_owned();
    refresh_thirst(&mut agent, &resumed, &water_kind, initial, &mut stages);
    produce(&mut agent, &resumed, &bread, 1, initial, 0, &mut stages);
    let cooked = agent.observe();
    assert_eq!(
        count(&cooked, &seed_kind),
        1,
        "milling/baking uses two harvested grain and leaves one for trade"
    );
    assert!(
        !stages[harvest_stages..]
            .iter()
            .any(|stage| stage["action"] == "gather" && stage["kind"] == seed_kind),
        "cooking must use the harvest, not a replacement grain grant or gather"
    );
    let hunger = cooked["sections"]["village"]["needs"]["hunger"]
        .as_i64()
        .unwrap();
    let food = count(&cooked, &bread);
    assert_eq!(
        agent.act(json!({"verb":"eat","args":{"kind":bread}}))["accepted"],
        true
    );
    resumed.step(1);
    let ate = agent.observe();
    assert_eq!(count(&ate, &bread), food - 1);
    assert!(
        ate["sections"]["village"]["needs"]["hunger"]
            .as_i64()
            .unwrap()
            > hunger
    );
    assert_eq!(ate["sections"]["village"]["coins"], 0);
    // Drink real water before waiting for a buyer, then return to the learned
    // public mill used for our flour. No live merchant position is assumed.
    produce(
        &mut agent,
        &resumed,
        &water_kind,
        1,
        initial,
        0,
        &mut stages,
    );
    let thirst = agent.observe()["sections"]["village"]["needs"]["thirst"]
        .as_i64()
        .unwrap();
    assert_eq!(agent.act(json!({"verb":"drink"}))["accepted"], true);
    resumed.step(1);
    assert!(
        agent.observe()["sections"]["village"]["needs"]["thirst"]
            .as_i64()
            .unwrap()
            > thirst
    );
    let mill = agent
        .workplaces
        .iter()
        .find_map(|(id, place)| {
            let used = place["recipes"].as_array()?.iter().any(|recipe| {
                recipe["inputs"]
                    .as_array()
                    .is_some_and(|inputs| inputs.iter().any(|input| input["kind"] == seed_kind))
                    && stages.iter().any(|stage| {
                        stage["action"] == "craft" && stage["recipe"] == recipe["name"]
                    })
            });
            if used {
                Some(agent.known.get(id)?["pos"].clone())
            } else {
                None
            }
        })
        .expect("the previously used mill is a learned public grain consumer");
    agent.walk(&resumed, mill);
    let advertised = agent.observe();
    let offer_seq = advertised["seq"].as_u64().unwrap();
    let minimum = advertised["sections"]["village"]["economy"]["market"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == seed_kind)
        .unwrap()["unit_price"]
        .as_u64()
        .unwrap();
    assert_eq!(agent.act(json!({"verb":"offer_goods","args":{"kind":seed_kind,"quantity":1,"limit_price":minimum}}))["accepted"], true);
    resumed.step(1);
    let listing = agent.observe();
    let mut purchased = if listing["sections"]["village"]["coins"].as_u64().unwrap() > 0 {
        Some(listing)
    } else {
        assert_eq!(
            count(&listing, &seed_kind),
            1,
            "listing retains the real unsold grain"
        );
        assert!(
            listing["sections"]["village"]["economy"]["own_offers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|offer| offer["kind"] == seed_kind && offer["quantity"] == 1)
        );
        None
    };
    // Observe at the same ten-tick cadence as the existing paid-spear
    // rehearsal so the bounded actor-event ring retains the sale receipt.
    for _ in 0..3200 {
        if purchased.is_some() {
            break;
        }
        resumed.step(10);
        let obs = agent.observe();
        budget(&obs, initial);
        assert!(started.elapsed() < Duration::from_secs(240));
        if obs["sections"]["village"]["coins"].as_u64().unwrap() > 0 {
            purchased = Some(obs);
        }
    }
    let sold =
        purchased.expect("an arriving, genuinely funded NPC must pay for the listed surplus grain");
    budget(&sold, initial);
    assert_eq!(count(&sold, &seed_kind), 0);
    assert!(sold["sections"]["village"]["coins"].as_u64().unwrap() > 0);
    // The incremental registered stream is authoritative for delivered events.
    // A recent-events summary can prioritize older orders/crafts over Effects.
    let delivery = agent.call(
        "agent/events",
        json!({"since_seq":offer_seq,"kinds":["Effect"],"limit":128}),
    );
    assert_eq!(
        delivery["dropped"], 0,
        "the bounded registered event stream must retain the sale"
    );
    let receipt = delivery["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| {
            event["unit"] == actor
                && event["event"]["effect"]["type"] == "VillageSaleCompleted"
                && event["event"]["effect"]["kind"] == seed_kind
        })
        .expect("registered seller receives its real completed sale receipt")["event"]["effect"]
        .clone();
    assert_eq!(receipt["quantity"], 1);
    assert_eq!(sold["sections"]["village"]["coins"], receipt["coins"]);
    assert_eq!(receipt["unit_price"], receipt["coins"]);
    assert!(receipt["unit_price"].as_u64().unwrap() >= minimum);
    assert!(
        !sold["sections"]["village"]["economy"]["own_offers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|offer| offer["kind"] == seed_kind)
    );
    assert!(
        stages[building_stages..]
            .iter()
            .any(|stage| stage["action"] == "gather" && stage["kind"] == "log"),
        "cooking uses a new real log after the cottage spent its bill"
    );
    assert!(
        stages[harvest_stages..]
            .iter()
            .any(|stage| stage["action"] == "gather" && stage["kind"] == water_kind),
        "baking uses real extra water after irrigation"
    );
    resumed.stop();
    assert_eq!(saved_cash(&home.0), currency);
    // Independent operator verification is deliberately last and cannot feed
    // any target, route, recipe, price or action decision above.
    let saved: Value =
        serde_json::from_slice(&std::fs::read(home.0.join("herp/island.world.json")).unwrap())
            .unwrap();
    let entities = saved["entities"].as_array().unwrap();
    let stored_plot = &entities
        .iter()
        .find(|entity| entity["id"] == plot)
        .expect("claimed plot persists")["components"]["island_homestead_plot_v1"];
    assert_eq!(stored_plot["owner"], actor);
    assert_eq!(stored_plot["cottage_built"], true);
    assert!(stored_plot["beds"][index as usize].is_null());
    let saved_actor = &entities
        .iter()
        .find(|entity| entity["id"] == actor)
        .expect("same actor persists")["components"]["island_villager"];
    assert_eq!(saved_actor["home"], door);
    assert!(started.elapsed() < Duration::from_secs(240));
    evidence.passed = true;
    println!(
        "ISLAND_PLAYTEST_SUMMARY {}",
        json!({"ok":true,"fixture":"registered_homestead_commons_crop_food_income",
        "world_preset":"living_homestead","seed":seed,"hero":"Herp","starting_coins":0,"materials":materials,
        "plot":plot,"ownership_crop_home_preserved":true,"crop_growth_ticks":facts["crop"]["growth_ticks"],"harvest":yield_count,
        "completed_stages":stages,"crop_became_food":true,"income":sold["sections"]["village"]["coins"],"sale_receipt":receipt,
        "elapsed_ticks":sold["tick"].as_u64().unwrap()-initial,"currency_total":currency,"owned_hosts_stopped":true,"real_provider":false,
        "interventions":["operator advances paused clock","operator independently verifies saved estate/home/currency only after actor decisions"],
        "discovery":"registered observations, relative exploration and remembered public places only; no preset coordinates or privileged targets"})
    );
}
