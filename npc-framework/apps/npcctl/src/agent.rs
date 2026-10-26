//! Agent control (agent-control spec §2.7): `serve` (the headless real-time
//! session host), the stateless clients `agent ...` (with `agent notes
//! ...`), `notes ...` (the player's notes), `world ...`, `session ...` (one
//! JSON-RPC call each, `--url`), `mcp` (a stdio MCP server in front of a
//! session) and `replay verify-session`.

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use npc_framework::agent_io::http::{ClientError, RpcClient};
use npc_framework::agent_io::scenario::{
    ScenarioDoc, ScenarioLoaders, SessionRecord, verify_session,
};
use npc_framework::agent_io::session::{Session, SessionOptions};
use npc_framework::tools::output::{EXIT_CHECK_FAILED, EXIT_OK, Failure, Out, read_json};
use serde_json::{Value, json};

/// Default session / BRP address.
pub const DEFAULT_URL: &str = "http://127.0.0.1:15702";

/// The scenario loaders npcctl knows.
pub fn loaders() -> ScenarioLoaders {
    let mut l = ScenarioLoaders::default();
    village_content::agent::register_scenario_loader(&mut l);
    testbed_content::register_scenario_loader(&mut l);
    l
}

/// The scenario text of `--scenario`: the file, or (when no such file
/// exists) an embedded testbed scenario by name (`testbed`, `doors`, ..).
fn scenario_text(path: &std::path::Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) => path
            .to_str()
            .and_then(testbed_content::scenario::scenario_text)
            .map(str::to_owned)
            .ok_or(e),
    }
}

#[derive(Args, Debug)]
#[command(group(
    clap::ArgGroup::new("source")
        .required(true)
        .args(["scenario", "explorer_home"])
))]
pub struct ServeArgs {
    /// Scenario JSON file (`{"version":1,"content":"village",..}`), or the
    /// name of an embedded testbed scenario (`testbed`, `doors`, `chop`, ..).
    #[arg(long, value_name = "FILE")]
    pub scenario: Option<PathBuf>,
    /// Ticks per second (real time).
    #[arg(long, default_value_t = 20)]
    pub tick_hz: u32,
    /// Write the session record (scenario + replay) here on `session stop`.
    #[arg(long, value_name = "FILE")]
    pub record: Option<PathBuf>,
    /// Port on 127.0.0.1 (0: any free port; the chosen one is printed).
    /// Default 15702; an explorer world takes the first free one in
    /// 15702..=15721 whose next port is free too (for the window).
    #[arg(long)]
    pub port: Option<u16>,
    /// Overrides the scenario's seed.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Require `Authorization: Bearer TOKEN` on every request (clients:
    /// `--token TOKEN`). The server always refuses browser (`Origin`),
    /// wrong-`Host` and non-JSON requests; a token also keeps out other
    /// local programs that do not know it.
    #[arg(long, value_name = "TOKEN")]
    pub token: Option<String>,
    #[command(flatten)]
    pub explorer: crate::explorer::ExplorerServeArgs,
}

/// Output of the client commands.
#[derive(Copy, Clone, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum Format {
    /// The method's JSON result.
    #[default]
    Json,
    /// Prompt-ready text where the method has it (observe), else JSON.
    Text,
}

#[derive(Args, Debug)]
pub struct ClientArgs {
    /// The session (`npcctl serve`) or game BRP server.
    #[arg(long, global = true, default_value = DEFAULT_URL)]
    pub url: String,
    /// Bearer token of a session started with `serve --token`.
    #[arg(long, global = true, value_name = "TOKEN")]
    pub token: Option<String>,
    #[arg(long, global = true, value_enum, default_value_t = Format::Json)]
    pub format: Format,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Mode {
    Direct,
    Advisor,
    Observer,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum OnIdle {
    Idle,
    Autopilot,
}

#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    /// Attach a controller to a unit: {controller, unit, mode, on_idle, applies_at}.
    #[command(after_help = "Example: npcctl agent attach --unit 5 --controller me --mode direct")]
    Attach {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: Option<String>,
        #[arg(long, value_enum, default_value_t = Mode::Direct)]
        mode: Mode,
        #[arg(long, value_enum, default_value_t = OnIdle::Idle)]
        on_idle: OnIdle,
        /// Take the unit over from another controller.
        #[arg(long)]
        force: bool,
    },
    /// Detach; the unit's own behaviour resumes.
    Detach {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
    },
    /// The unit's egocentric observation (affordances included).
    #[command(
        after_help = "Examples:\n  npcctl agent observe --unit 5 --format text --brief\n  npcctl agent observe --unit 5 --format text --since-seq 40   # first line: what changed since seq 40"
    )]
    Observe {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: Option<String>,
        /// Recent events to include.
        #[arg(long)]
        events: Option<usize>,
        /// Hide structurally impossible affordances ("door has no lock",
        /// "door is already closed"); the default lists every one.
        #[arg(long)]
        brief: bool,
        /// Add what changed since this seq (your last observation's `seq`):
        /// `changes` and, in text, a first "Since your last look" line.
        #[arg(long)]
        since_seq: Option<u64>,
    },
    /// Act: an offered affordance (--affordance ID), a verb (--verb, --target, --pos, --args),
    /// walk-and-interact (--interact VERB@ID) or several steps at once (--steps JSON).
    #[command(
        after_help = "Examples:\n  npcctl agent act --unit 5 --controller me --affordance eat@3\n  npcctl agent act --unit 5 --controller me --verb walk_to --pos 4000,2000 --queue\n  npcctl agent act --unit 33 --controller me --interact pick_up@26\n  npcctl agent act --unit 33 --controller me --steps '[{\"verb\":\"order\",\"args\":{\"order\":{\"type\":\"Move\",\"pos\":[4500,-1500]}}},{\"verb\":\"execute_orders\"}]'\n  npcctl agent act --unit 33 --controller me --interact pick_up@26 --wait --format text   # act, wait until idle, observe\n\n--wait (agent/act_and_wait) returns {act, events, observation} once the unit is idle again, at the\nfirst --kinds event, or after --timeout-ms (default 30000, at most 120000); a rejection returns at once.\n\n--queue appends to the unit's *action* queue (agent/act). To shift-queue an RTS order behind\nthe unit's other orders, put \"queue\":true inside --args of the order verb instead."
    )]
    Act(ActArgs),
    /// Cancel an action (--handle) or the running one.
    Cancel {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
        #[arg(long)]
        handle: Option<u32>,
    },
    /// Cancel the running action and empty the queue.
    Clear {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
    },
    /// Events after --since-seq; with --timeout-ms waits until one happens (long-poll).
    #[command(
        after_help = "Examples:\n  npcctl agent events --unit 5 --since-seq 12 --kinds ExternalAction --timeout-ms 5000\n  npcctl agent events --unit 33 --since-seq 40 --until-idle --dedupe   # wait until the unit is done"
    )]
    Events {
        #[arg(long)]
        unit: Option<u32>,
        #[arg(long, default_value_t = 0)]
        since_seq: u64,
        /// Comma-separated kinds (`all` for everything; unknown names are an error).
        #[arg(long)]
        kinds: Option<String>,
        /// Wait up to this long (with --until-idle: default 30000).
        #[arg(long, default_value_t = 0)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Return once the unit is idle (its action queue, orders, interaction, craft and vehicle drive done)
        /// instead of on the first event; needs --unit.
        #[arg(long)]
        until_idle: bool,
        /// Return once at least this many matching events happened.
        #[arg(long)]
        min_events: Option<usize>,
        /// Collapse consecutive identical events (`"repeats": n`).
        #[arg(long)]
        dedupe: bool,
    },
    /// Per-controller counters and observe→act latency (ticks, ms).
    Stats {
        #[arg(long)]
        controller: Option<String>,
    },
    /// Every unit with kind, position, controllability, controller and (RTS) owner player.
    ListUnits {
        #[arg(long)]
        kind: Option<String>,
    },
    /// Your notes (diary, memory, proposals) as the unit's controller: list, add, update.
    #[command(
        subcommand,
        after_help = "Examples:\n  npcctl agent notes list --unit 33 --controller explorer --book memory\n  npcctl agent notes add --unit 33 --controller explorer --book diary --title \"Found the key\" --body \"Under the mat.\"\n  npcctl agent notes update --unit 33 --controller explorer --book diary --id 1 --expected-tick 120 --expected-date 2026-10-25T10:00:00Z --body \"..\""
    )]
    Notes(AgentNotesCmd),
}

/// A notebook.
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum BookArg {
    /// What happened.
    Diary,
    /// Lasting facts.
    Memory,
    /// Improvement ideas for the developer.
    Proposals,
}

impl BookArg {
    fn name(self) -> &'static str {
        match self {
            BookArg::Diary => "diary",
            BookArg::Memory => "memory",
            BookArg::Proposals => "proposals",
        }
    }
}

/// A proposal's status (player only).
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum StatusArg {
    New,
    Liked,
    Pursuing,
    Rejected,
}

impl StatusArg {
    fn name(self) -> &'static str {
        match self {
            StatusArg::New => "new",
            StatusArg::Liked => "liked",
            StatusArg::Pursuing => "pursuing",
            StatusArg::Rejected => "rejected",
        }
    }
}

/// A note to add.
#[derive(Args, Debug)]
pub struct NewNoteArgs {
    /// One line, at most 200 characters.
    #[arg(long)]
    title: String,
    /// At most 8192 bytes.
    #[arg(long, default_value = "")]
    body: String,
    /// Proposals only: what prompted the idea.
    #[arg(long)]
    prompted_by: Option<String>,
}

/// What an update changes, and the version it changes.
#[derive(Args, Debug)]
pub struct NoteChangeArgs {
    /// The entry's id.
    #[arg(long)]
    id: u32,
    /// The entry's `updated.tick` as last listed or returned.
    #[arg(long)]
    expected_tick: u64,
    /// The entry's `updated.date` exactly as last listed or returned (with any `#rN`).
    #[arg(long, default_value = "")]
    expected_date: String,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    body: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum AgentNotesCmd {
    /// The entries of a book, oldest first: {unit, book, notes:[{id, title, body, created, updated, status?, prompted_by?}], unreadable:[{file, reason}]}.
    List {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
        #[arg(long, value_enum)]
        book: BookArg,
    },
    /// Add an entry: {accepted, note}.
    Add {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
        #[arg(long, value_enum)]
        book: BookArg,
        #[command(flatten)]
        note: NewNoteArgs,
    },
    /// Change an entry's title or body if it is unchanged since --expected-*: {accepted, note}
    /// (stale: exit 1 with `current`).
    Update {
        #[arg(long)]
        unit: u32,
        #[arg(long)]
        controller: String,
        #[arg(long, value_enum)]
        book: BookArg,
        #[command(flatten)]
        change: NoteChangeArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum NotesCmd {
    /// The entries of a unit's book, oldest first.
    List {
        #[arg(long)]
        unit: u32,
        #[arg(long, value_enum)]
        book: BookArg,
    },
    /// Add an entry as the player.
    Add {
        #[arg(long)]
        unit: u32,
        #[arg(long, value_enum)]
        book: BookArg,
        #[command(flatten)]
        note: NewNoteArgs,
    },
    /// Change an entry (title, body, a proposal's --status) as the player.
    Update {
        #[arg(long)]
        unit: u32,
        #[arg(long, value_enum)]
        book: BookArg,
        #[command(flatten)]
        change: NoteChangeArgs,
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
    },
}

fn new_note_params(p: &mut Value, n: &NewNoteArgs) {
    p["title"] = json!(n.title);
    p["body"] = json!(n.body);
    if let Some(by) = &n.prompted_by {
        p["prompted_by"] = json!(by);
    }
}

fn change_params(p: &mut Value, c: &NoteChangeArgs) {
    p["id"] = json!(c.id);
    p["expected"] = json!({"tick": c.expected_tick, "date": c.expected_date});
    if let Some(t) = &c.title {
        p["title"] = json!(t);
    }
    if let Some(b) = &c.body {
        p["body"] = json!(b);
    }
}

/// The method and params of an `agent notes` command.
fn agent_notes(cmd: &AgentNotesCmd) -> (&'static str, Value) {
    match cmd {
        AgentNotesCmd::List {
            unit,
            controller,
            book,
        } => (
            "agent/notes/list",
            json!({"unit": unit, "controller": controller, "book": book.name()}),
        ),
        AgentNotesCmd::Add {
            unit,
            controller,
            book,
            note,
        } => {
            let mut p = json!({"unit": unit, "controller": controller, "book": book.name()});
            new_note_params(&mut p, note);
            ("agent/notes/add", p)
        }
        AgentNotesCmd::Update {
            unit,
            controller,
            book,
            change,
        } => {
            let mut p = json!({"unit": unit, "controller": controller, "book": book.name()});
            change_params(&mut p, change);
            ("agent/notes/update", p)
        }
    }
}

/// `npcctl notes ...`: the player's notes methods (no controller).
pub fn cmd_notes(cmd: &NotesCmd, c: &ClientArgs, out: &mut Out) -> Result<u8, Failure> {
    let client = client(&c.url, c.token.as_deref())?;
    let (method, params) = match cmd {
        NotesCmd::List { unit, book } => ("notes/list", json!({"unit": unit, "book": book.name()})),
        NotesCmd::Add { unit, book, note } => {
            let mut p = json!({"unit": unit, "book": book.name()});
            new_note_params(&mut p, note);
            ("notes/add", p)
        }
        NotesCmd::Update {
            unit,
            book,
            change,
            status,
        } => {
            let mut p = json!({"unit": unit, "book": book.name()});
            change_params(&mut p, change);
            if let Some(s) = status {
                p["status"] = json!(s.name());
            }
            ("notes/update", p)
        }
    };
    let v = rpc(&client, method, params)?;
    print(out, c.format, &v)?;
    Ok(exit_code(&v))
}

/// Exit 1 for a rejection (`accepted: false`, or `act.accepted: false` of
/// `act --wait`), else 0.
fn exit_code(v: &Value) -> u8 {
    if v.get("accepted").and_then(Value::as_bool) == Some(false)
        || v.pointer("/act/accepted").and_then(Value::as_bool) == Some(false)
    {
        EXIT_CHECK_FAILED
    } else {
        EXIT_OK
    }
}

#[derive(Args, Debug)]
pub struct ActArgs {
    #[arg(long)]
    unit: u32,
    #[arg(long)]
    controller: String,
    #[arg(long)]
    affordance: Option<String>,
    #[arg(long)]
    verb: Option<String>,
    /// Walk into reach and do it, in one act: `VERB@ID` (e.g. `pick_up@26`); the `interact` verb.
    #[arg(long, value_name = "VERB@ID", conflicts_with_all = ["affordance", "verb", "steps"])]
    interact: Option<String>,
    /// Several steps as one atomic act (all validated before any applies): a JSON list of
    /// `{"affordance":ID}` / `{"verb":V,"target":ID,"pos":[x,z],"args":{..}}`.
    #[arg(long, value_name = "JSON", conflicts_with_all = ["affordance", "verb"])]
    steps: Option<String>,
    #[arg(long)]
    target: Option<u32>,
    /// `x,z` in mm.
    #[arg(long, value_parser = parse_pos)]
    pos: Option<(i32, i32)>,
    /// Verb arguments as a JSON object (`{"ticks":50}`).
    #[arg(long)]
    args: Option<String>,
    /// Append to the queue instead of replacing it.
    #[arg(long)]
    queue: bool,
    #[arg(long)]
    expect_tick: Option<u64>,
    #[arg(long)]
    expect_seq: Option<u64>,
    #[arg(long)]
    allow_stale: bool,
    /// Act, then wait until the unit is idle again and observe, in one call (agent/act_and_wait):
    /// {act, events, observation}.
    #[arg(long)]
    wait: bool,
    /// With --wait: event kinds to return; the first one also ends the wait (comma-separated).
    #[arg(long, requires = "wait")]
    kinds: Option<String>,
    /// With --wait: the longest wait in ms (default 30000, at most 120000).
    #[arg(long, requires = "wait")]
    timeout_ms: Option<u64>,
}

fn parse_pos(s: &str) -> Result<(i32, i32), String> {
    let (x, z) = s
        .split_once(',')
        .ok_or_else(|| format!("`{s}` is not x,z"))?;
    Ok((
        x.trim().parse().map_err(|e| format!("x: {e}"))?,
        z.trim().parse().map_err(|e| format!("z: {e}"))?,
    ))
}

#[derive(Subcommand, Debug)]
pub enum WorldCmd {
    /// Replace the running world with a scenario file (session host only).
    LoadScenario {
        file: PathBuf,
        #[arg(long)]
        seed: Option<u64>,
    },
    /// Rebuild the world from its scenario (session host only).
    Reset {
        #[arg(long)]
        seed: Option<u64>,
    },
    /// The world snapshot (the content's inspector).
    State,
}

#[derive(Subcommand, Debug)]
pub enum SessionCmd {
    /// Scenario, seed, tick rate, URL and methods of the session.
    Info,
    /// Stop the session (writes its --record).
    Stop,
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Direct => "Direct",
        Mode::Advisor => "Advisor",
        Mode::Observer => "Observer",
    }
}

fn idle_name(m: OnIdle) -> &'static str {
    match m {
        OnIdle::Idle => "Idle",
        OnIdle::Autopilot => "Autopilot",
    }
}

fn client(url: &str, token: Option<&str>) -> Result<RpcClient, Failure> {
    RpcClient::new(url)
        .map(|c| c.with_token(token.map(str::to_owned)))
        .map_err(Failure::usage)
}

/// One call; RPC errors exit 2 with `{"error", "code", "data"}`.
fn rpc(c: &RpcClient, method: &str, params: Value) -> Result<Value, Failure> {
    c.call(method, params).map_err(|e| match e {
        ClientError::Rpc(err) => Failure::usage(err.message.clone())
            .with("code", err.code)
            .with("data", err.data)
            .with("method", method),
        ClientError::Transport(t) => {
            Failure::usage(format!("{t} (is `npcctl serve` running at {}?)", c.url()))
                .with("method", method)
        }
    })
}

fn print(out: &mut Out, format: Format, v: &Value) -> Result<(), Failure> {
    if format == Format::Text {
        if let Some(text) = v.get("text").and_then(Value::as_str) {
            print!("{text}");
            return Ok(());
        }
        if v.get("act").is_some() && v.get("observation").is_some() {
            print!("{}", npc_framework::agent_io::observe::act_and_wait_text(v));
            return Ok(());
        }
    }
    out.json(v)
}

fn nonce() -> Value {
    // Distinguishes concurrent identical long-polls on BRP servers and keys
    // an `act --wait` there (a BRP server keeps each finished act-and-wait's
    // answer for 60 s under its params, nonce included): the process id
    // plus the wall clock, so a later process reusing the id still differs.
    // Tooling only, never sim input.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    json!(format!("npcctl-{}-{nanos}", std::process::id()))
}

pub fn cmd_agent(cmd: &AgentCmd, c: &ClientArgs, out: &mut Out) -> Result<u8, Failure> {
    let client = client(&c.url, c.token.as_deref())?;
    let (method, params) = match cmd {
        AgentCmd::Attach {
            unit,
            controller,
            mode,
            on_idle,
            force,
        } => (
            "agent/attach",
            json!({"unit": unit, "controller": controller, "mode": mode_name(*mode),
                   "on_idle": idle_name(*on_idle), "force": force}),
        ),
        AgentCmd::Detach { unit, controller } => (
            "agent/detach",
            json!({"unit": unit, "controller": controller}),
        ),
        AgentCmd::Observe {
            unit,
            controller,
            events,
            brief,
            since_seq,
        } => {
            let mut p = json!({"unit": unit, "controller": controller,
                               "format": if c.format == Format::Text { "text" } else { "json" }});
            if let Some(n) = events {
                p["events"] = json!(n);
            }
            if *brief {
                p["brief"] = json!(true);
            }
            if let Some(s) = since_seq {
                p["since_seq"] = json!(s);
            }
            ("agent/observe", p)
        }
        AgentCmd::Act(a) => {
            let args = match &a.args {
                None => Value::Null,
                Some(s) => serde_json::from_str(s)
                    .map_err(|e| Failure::usage(format!("--args is not JSON: {e}")))?,
            };
            let mut p = json!({"unit": a.unit, "controller": a.controller, "queue": a.queue,
                               "allow_stale": a.allow_stale});
            let opt = |p: &mut Value, k: &str, v: Value| {
                if !v.is_null() {
                    p[k] = v;
                }
            };
            let mut args = args;
            let mut verb = a.verb.clone();
            if let Some(i) = &a.interact {
                // `interact` verb: walk into reach, then do VERB to ID.
                verb = Some("interact".to_owned());
                if args.is_null() {
                    args = json!({});
                }
                args["affordance"] = json!(i);
            }
            if verb.as_deref() == Some("order") && a.queue {
                eprintln!(
                    "note: --queue appends to the unit's action queue; to shift-queue an order behind \
                     the unit's other orders, put \"queue\":true inside --args"
                );
            }
            if let Some(steps) = &a.steps {
                let steps: Value = serde_json::from_str(steps)
                    .map_err(|e| Failure::usage(format!("--steps is not JSON: {e}")))?;
                if !steps.is_array() {
                    return Err(Failure::usage("--steps must be a JSON list of steps"));
                }
                p["steps"] = steps;
            }
            opt(&mut p, "affordance", json!(a.affordance));
            opt(&mut p, "verb", json!(verb));
            opt(&mut p, "target", json!(a.target));
            opt(&mut p, "pos", json!(a.pos));
            opt(&mut p, "args", args);
            opt(&mut p, "expect_tick", json!(a.expect_tick));
            opt(&mut p, "expect_seq", json!(a.expect_seq));
            if a.wait {
                opt(&mut p, "kinds", json!(a.kinds));
                opt(&mut p, "timeout_ms", json!(a.timeout_ms));
                p["observe"] =
                    json!({"format": if c.format == Format::Text { "text" } else { "json" }});
                p["nonce"] = nonce();
                ("agent/act_and_wait", p)
            } else {
                ("agent/act", p)
            }
        }
        AgentCmd::Cancel {
            unit,
            controller,
            handle,
        } => (
            "agent/cancel",
            json!({"unit": unit, "controller": controller, "handle": handle}),
        ),
        AgentCmd::Clear { unit, controller } => (
            "agent/clear",
            json!({"unit": unit, "controller": controller}),
        ),
        AgentCmd::Events {
            unit,
            since_seq,
            kinds,
            timeout_ms,
            limit,
            until_idle,
            min_events,
            dedupe,
        } => {
            let timeout = if *until_idle && *timeout_ms == 0 {
                30_000
            } else {
                *timeout_ms
            };
            let mut p = json!({"unit": unit, "since_seq": since_seq, "kinds": kinds,
                               "timeout_ms": timeout, "limit": limit, "nonce": nonce()});
            if *until_idle {
                p["until_idle"] = json!(true);
            }
            if let Some(n) = min_events {
                p["min_events"] = json!(n);
            }
            if *dedupe {
                p["dedupe"] = json!(true);
            }
            ("agent/events", p)
        }
        AgentCmd::Stats { controller } => ("agent/stats", json!({"controller": controller})),
        AgentCmd::ListUnits { kind } => ("agent/list_units", json!({"kind": kind})),
        AgentCmd::Notes(n) => agent_notes(n),
    };
    let v = rpc(&client, method, params)?;
    print(out, c.format, &v)?;
    Ok(exit_code(&v))
}

pub fn cmd_world(cmd: &WorldCmd, c: &ClientArgs, out: &mut Out) -> Result<u8, Failure> {
    let client = client(&c.url, c.token.as_deref())?;
    let (method, params) = match cmd {
        WorldCmd::LoadScenario { file, seed } => {
            let doc: Value = read_json(file, "scenario")?;
            (
                "world/load_scenario",
                json!({"scenario": doc, "seed": seed}),
            )
        }
        WorldCmd::Reset { seed } => ("world/reset", json!({"seed": seed})),
        WorldCmd::State => ("world/state", json!({})),
    };
    let v = rpc(&client, method, params)?;
    print(out, c.format, &v)?;
    Ok(EXIT_OK)
}

pub fn cmd_session(cmd: &SessionCmd, c: &ClientArgs, out: &mut Out) -> Result<u8, Failure> {
    let client = client(&c.url, c.token.as_deref())?;
    let method = match cmd {
        SessionCmd::Info => "session/info",
        SessionCmd::Stop => "session/stop",
    };
    let v = rpc(&client, method, json!({}))?;
    print(out, c.format, &v)?;
    Ok(EXIT_OK)
}

pub fn cmd_serve(a: &ServeArgs, out: &mut Out) -> Result<u8, Failure> {
    if a.tick_hz == 0 || a.tick_hz > 10_000 {
        return Err(Failure::usage("--tick-hz must be 1..=10000"));
    }
    let Some(scenario) = &a.scenario else {
        // The group makes one of the two required.
        return crate::explorer::cmd_serve_explorer(a, out);
    };
    let text = scenario_text(scenario)
        .map_err(|e| Failure::usage(format!("cannot read scenario {}: {e}", scenario.display())))?;
    let doc = ScenarioDoc::parse(&text)
        .map_err(|e| Failure::usage(format!("scenario {}: {e}", scenario.display())))?;
    let options = SessionOptions {
        port: a.port.unwrap_or(15702),
        tick_hz: a.tick_hz,
        record: a.record.clone(),
        seed: a.seed,
        token: a.token.clone(),
        ..SessionOptions::default()
    };
    let session = Session::start(loaders(), doc, options).map_err(Failure::usage)?;
    out.json(&json!({
        "listening": session.url(),
        "port": session.port(),
        "scenario": scenario.display().to_string(),
        "tick_hz": a.tick_hz,
        "record": a.record.as_ref().map(|p| p.display().to_string()),
        "token_required": a.token.as_ref().is_some_and(|t| !t.is_empty()),
    }))?;
    // The first line must reach a waiting client before the loop starts.
    out.finish()?;
    let written = session.run().map_err(Failure::usage)?;
    out.json(&json!({"stopped": true, "record": written.map(|p| p.display().to_string())}))?;
    Ok(EXIT_OK)
}

pub fn cmd_mcp(url: &str, token: Option<&str>, controller: &str) -> Result<u8, Failure> {
    let client = client(url, token)?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    npc_framework::agent_io::mcp::serve(&client, controller, stdin.lock(), stdout.lock())
        .map_err(|e| Failure::usage(format!("mcp: {e}")))?;
    Ok(EXIT_OK)
}

pub fn cmd_verify_session(file: &std::path::Path, out: &mut Out) -> Result<u8, Failure> {
    let record: SessionRecord = read_json(file, "session record")?;
    let report = verify_session(&loaders(), &record);
    out.json(&report)?;
    Ok(if report.ok {
        EXIT_OK
    } else {
        EXIT_CHECK_FAILED
    })
}
