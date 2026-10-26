//! The `agent/*` and `world/state` methods, transport-agnostic: plain
//! functions over `&mut World` with JSON params, shared by the headless
//! session host ([`crate::session`]) and, with feature `remote`, Bevy's BRP
//! ([`crate::remote`]). Every method validates its params and never panics on
//! bad input. Responses carry the current `tick` (the next tick to run), the
//! latest event `seq` and the world's `epoch` (a new number whenever the
//! world is rebuilt, e.g. by `world/reset` or `world/load_scenario`: seqs and
//! handles restart then, so a client that sees the epoch change starts over
//! from `since_seq: 0`).
//!
//! Rejected actions are *results* (`{"accepted":false,"reason":..}`), so an
//! agent can reason about them; malformed params, unknown units and
//! rejected commands are JSON-RPC errors ([`RpcError`]).

use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::prelude::*;
use npc_kernel::commands::{
    CommandError, CommandLog, CommandQueue, SimCommand, canonical, submit, validate,
};
use npc_kernel::journal::{self, Journal, JournalFilter, JournalKind};
use npc_kernel::{SimClock, SimControl, SimId, SimIndex, inspect};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::control::{
    AgentAct, AgentCancel, AgentClear, AttachController, ControlMode, MAX_QUEUE, Step,
    validate_steps, with_same_tick_acts,
};
use crate::control::{DetachController, ExternalController, ExternalQueue, IdlePolicy};
use crate::events::{self, AgentEvent, AgentEvents, EventQuery};
use crate::notes::{
    Book, NewNote, NoteChange, NotesClock, NotesDirs, NotesError, NotesStores, ProposalStatus,
    Stamp, Writer,
};
use crate::observe;
use crate::registry::{ActionPlan, ActionRequest, AgentCtx, AgentRegistry, Rejection};
use crate::stats::AgentStats;

/// JSON-RPC error codes (the BRP ones where they exist).
pub mod codes {
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL: i64 = -32603;
    /// Unknown unit (BRP `ENTITY_NOT_FOUND`).
    pub const UNKNOWN_UNIT: i64 = -23401;
    /// The journal is off (BRP `RESOURCE_ERROR`).
    pub const JOURNAL_OFF: i64 = -23501;
    /// A sim resource is missing (BRP `RESOURCE_NOT_PRESENT`).
    pub const NOT_READY: i64 = -23502;
    /// The method needs a session host (`npcctl serve`).
    pub const NEEDS_SESSION: i64 = -23601;
}

/// A JSON-RPC error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }

    /// A rejected command (`data` is the `CommandError` JSON).
    pub fn command(err: &CommandError) -> Self {
        let code = match err {
            CommandError::UnknownUnit { .. } => codes::UNKNOWN_UNIT,
            CommandError::NotReady { .. } => codes::NOT_READY,
            _ => codes::INVALID_PARAMS,
        };
        Self {
            code,
            message: err.to_string(),
            data: serde_json::to_value(err).ok(),
        }
    }
}

pub type RpcResult = Result<Value, RpcError>;

/// Default [`AgentIoConfig::stale_ticks`]: 30 s at 20 Hz, room for an LLM
/// turn (a think step took ~165 ticks in the first fresh-agent play-test).
pub const DEFAULT_STALE_TICKS: u64 = 600;

/// Tunables (a resource; insert your own before or after the plugin).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentIoConfig {
    /// An act with `expect_tick` older than this many ticks is `stale`
    /// (default [`DEFAULT_STALE_TICKS`]).
    pub stale_ticks: u64,
    /// An act with `expect_seq` is `stale` when more than this many
    /// *relevant* interrupting events happened to the unit since that seq
    /// ([`events::AgentEvent::interrupts_for`]: its own actions, orders,
    /// interactions and crafts ending, need levels, mounting, detach,
    /// despawn, failed paths, and percepts of the act's own targets).
    pub stale_events: usize,
    /// Longest `agent/events` wait.
    pub max_timeout_ms: u64,
}

impl Default for AgentIoConfig {
    fn default() -> Self {
        Self {
            stale_ticks: DEFAULT_STALE_TICKS,
            stale_events: 0,
            max_timeout_ms: 60_000,
        }
    }
}

/// Worlds built in this process so far: the source of [`AgentIoState::epoch`].
static WORLDS: AtomicU64 = AtomicU64::new(0);

/// Tooling counters (not hashed): handles, default controller ids and the
/// world's epoch.
#[derive(Resource, Clone, Copy, Debug)]
pub struct AgentIoState {
    pub next_handle: u32,
    pub next_controller: u32,
    /// Identifies this world instance: every world built (an app with
    /// `AgentIoPlugin`, a `world/reset`, a `world/load_scenario`) gets a new,
    /// larger number, so a client can tell that seqs and handles restarted.
    pub epoch: u64,
}

impl Default for AgentIoState {
    fn default() -> Self {
        Self {
            next_handle: 1,
            next_controller: 1,
            epoch: WORLDS.fetch_add(1, Ordering::Relaxed) + 1,
        }
    }
}

/// Snapshot save of [`AgentIoState`] (ADR 0013): the handle and controller id
/// counters, so a resumed world never hands out an id twice. The epoch is
/// not saved: the loaded world is a new world instance.
pub(crate) fn save_agent_io_state(world: &World) -> Option<Value> {
    let st = world.get_resource::<AgentIoState>()?;
    Some(json!({ "next_handle": st.next_handle, "next_controller": st.next_controller }))
}

pub(crate) fn load_agent_io_state(world: &mut World, value: Value) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Saved {
        next_handle: u32,
        next_controller: u32,
    }
    let saved: Saved = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut st = world.get_resource_or_init::<AgentIoState>();
    st.next_handle = saved.next_handle;
    st.next_controller = saved.next_controller;
    Ok(())
}

/// The world's epoch (see [`AgentIoState::epoch`]; 0 without the plugin).
pub fn epoch(world: &World) -> u64 {
    world.get_resource::<AgentIoState>().map_or(0, |s| s.epoch)
}

/// The `agent/*`, `notes/*` and `world/*` method names this module serves
/// (the session host adds `world/load_scenario`, `world/reset`,
/// `session/stop` and a subset of `npc/*`).
pub const METHODS: [&str; 17] = [
    "agent/attach",
    "agent/detach",
    "agent/observe",
    "agent/act",
    "agent/act_and_wait",
    "agent/cancel",
    "agent/clear",
    "agent/events",
    "agent/stats",
    "agent/list_units",
    "agent/notes/list",
    "agent/notes/add",
    "agent/notes/update",
    "notes/list",
    "notes/add",
    "notes/update",
    "world/state",
];

fn params<T: DeserializeOwned>(p: Option<Value>, shape: &str) -> Result<T, RpcError> {
    let v = match p {
        None | Some(Value::Null) => json!({}),
        Some(v) => v,
    };
    serde_json::from_value(v)
        .map_err(|e| RpcError::invalid(format!("bad params (expected {shape}): {e}")))
}

fn now(world: &World) -> Result<u64, RpcError> {
    world
        .get_resource::<SimClock>()
        .map(|c| c.tick)
        .ok_or_else(|| RpcError::new(codes::NOT_READY, "sim not ready: missing SimClock"))
}

fn stamp(world: &World, mut v: Value) -> RpcResult {
    let tick = now(world)?;
    if let Value::Object(m) = &mut v {
        m.insert("tick".into(), json!(tick));
        m.insert("seq".into(), json!(events::last_seq(world)));
        m.insert("epoch".into(), json!(epoch(world)));
    }
    Ok(v)
}

fn unit_entity(world: &World, unit: u32) -> Result<Entity, RpcError> {
    world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(SimId(unit)))
        .ok_or_else(|| RpcError::command(&CommandError::UnknownUnit { unit }))
}

fn submit_cmd(world: &mut World, cmd: SimCommand) -> Result<(u64, u32), RpcError> {
    submit(world, None, cmd).map_err(|e| RpcError::command(&e))
}

/// Dispatches one instant method (see [`METHODS`]). `agent/events` and
/// `agent/act_and_wait` answer at once here (the act is done, nothing is
/// waited for); transports that support waiting use [`events_ready`] and
/// [`events_result`], [`act_and_wait_start`], [`act_wait_ready`] and
/// [`act_wait_result`].
pub fn call(world: &mut World, method: &str, p: Option<Value>) -> RpcResult {
    events::harvest(world);
    match method {
        "agent/attach" => attach(world, params(p, ATTACH)?),
        "agent/detach" => detach(world, params(p, OWNER)?),
        "agent/observe" => observe_method(world, params(p, OBSERVE)?),
        "agent/act" => act(world, act_params(p, ACT)?),
        "agent/act_and_wait" => {
            let w = act_and_wait_start(world, p)?;
            act_wait_result(world, &w, false)
        }
        "agent/cancel" => cancel(world, params(p, CANCEL)?),
        "agent/clear" => clear(world, params(p, OWNER)?),
        "agent/events" => {
            let p = parse_events(p)?;
            events_result(world, &p, false)
        }
        "agent/stats" => stats(world, params(p, "{controller?}")?),
        "agent/list_units" => list_units(world, params(p, "{kind?}")?),
        "agent/notes/list" => {
            let p: AgentNotesList = params(p, AGENT_NOTES_LIST)?;
            notes_call(world, p.unit, Some(&p.controller), &p.book, NotesOp::List)
        }
        "agent/notes/add" => {
            let p: AgentNotesAdd = params(p, AGENT_NOTES_ADD)?;
            let op = NotesOp::Add(NewNote {
                title: p.title,
                body: p.body,
                prompted_by: p.prompted_by,
            });
            notes_call(world, p.unit, Some(&p.controller), &p.book, op)
        }
        "agent/notes/update" => {
            let p: AgentNotesUpdate = params(p, AGENT_NOTES_UPDATE)?;
            let change = NoteChange {
                title: p.title,
                body: p.body,
                status: None,
            };
            let op = NotesOp::Update(p.id, p.expected, change);
            notes_call(world, p.unit, Some(&p.controller), &p.book, op)
        }
        "notes/list" => {
            let p: PlayerNotesList = params(p, PLAYER_NOTES_LIST)?;
            notes_call(world, p.unit, None, &p.book, NotesOp::List)
        }
        "notes/add" => {
            let p: PlayerNotesAdd = params(p, PLAYER_NOTES_ADD)?;
            let op = NotesOp::Add(NewNote {
                title: p.title,
                body: p.body,
                prompted_by: p.prompted_by,
            });
            notes_call(world, p.unit, None, &p.book, op)
        }
        "notes/update" => {
            let p: PlayerNotesUpdate = params(p, PLAYER_NOTES_UPDATE)?;
            let status = p.status.as_deref().map(parse_status).transpose()?;
            let change = NoteChange {
                title: p.title,
                body: p.body,
                status,
            };
            let op = NotesOp::Update(p.id, p.expected, change);
            notes_call(world, p.unit, None, &p.book, op)
        }
        "world/state" => {
            now(world)?;
            Ok(inspect::world_json(world))
        }
        other => Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!("Method `{other}` not found"),
        )),
    }
}

// ---------------------------------------------------------------- attach

const ATTACH: &str =
    "{unit, controller?, mode?: Direct|Advisor|Observer, on_idle?: Idle|Autopilot, force?}";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachParams {
    unit: u32,
    #[serde(default)]
    controller: Option<String>,
    #[serde(default)]
    mode: ControlMode,
    #[serde(default)]
    on_idle: IdlePolicy,
    #[serde(default)]
    force: bool,
}

fn attach(world: &mut World, p: AttachParams) -> RpcResult {
    let controller = match p.controller {
        Some(c) => c,
        None => {
            let mut st = world.get_resource_or_init::<AgentIoState>();
            st.next_controller += 1;
            format!("agent-{}", st.next_controller - 1)
        }
    };
    let cmd = AttachController {
        unit: p.unit,
        controller: controller.clone(),
        mode: p.mode,
        on_idle: p.on_idle,
        force: p.force,
    };
    let (tick, _) = submit_cmd(world, SimCommand::content(&cmd))?;
    stamp(
        world,
        json!({
            "controller": controller, "unit": p.unit, "mode": p.mode.name(),
            "on_idle": p.on_idle.name(), "applies_at": tick,
        }),
    )
}

const OWNER: &str = "{unit, controller}";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerParams {
    unit: u32,
    controller: String,
}

fn detach(world: &mut World, p: OwnerParams) -> RpcResult {
    let cmd = DetachController {
        unit: p.unit,
        controller: p.controller,
    };
    let (tick, _) = submit_cmd(world, SimCommand::content(&cmd))?;
    stamp(world, json!({"unit": p.unit, "applies_at": tick}))
}

const CANCEL: &str = "{unit, controller, handle?}";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelParams {
    unit: u32,
    controller: String,
    #[serde(default)]
    handle: Option<u32>,
}

fn cancel(world: &mut World, p: CancelParams) -> RpcResult {
    let cmd = AgentCancel {
        unit: p.unit,
        controller: p.controller,
        handle: p.handle,
    };
    let (tick, _) = submit_cmd(world, SimCommand::content(&cmd))?;
    stamp(
        world,
        json!({"unit": p.unit, "handle": p.handle, "applies_at": tick}),
    )
}

fn clear(world: &mut World, p: OwnerParams) -> RpcResult {
    let cmd = AgentClear {
        unit: p.unit,
        controller: p.controller,
    };
    let (tick, _) = submit_cmd(world, SimCommand::content(&cmd))?;
    stamp(world, json!({"unit": p.unit, "applies_at": tick}))
}

// ---------------------------------------------------------------- observe

const OBSERVE: &str = "{unit, controller?, format?: json|text, events?: N, brief?, since_seq?}";

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Json,
    Text,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObserveParams {
    unit: u32,
    #[serde(default)]
    controller: Option<String>,
    #[serde(default)]
    format: Format,
    #[serde(default = "default_recent")]
    events: usize,
    /// Hide structurally impossible affordances ("door has no lock").
    #[serde(default)]
    brief: bool,
    /// Add `changes` since this event seq ([`observe::changes`]) and, in
    /// text, a first "Since your last look" line.
    #[serde(default)]
    since_seq: Option<u64>,
}

fn default_recent() -> usize {
    observe::DEFAULT_RECENT_EVENTS
}

fn observe_method(world: &mut World, p: ObserveParams) -> RpcResult {
    let entity = unit_entity(world, p.unit)?;
    let tick = now(world)?;
    let owner = world
        .get::<ExternalController>(entity)
        .map(|c| c.id.clone());
    if let (Some(given), Some(owner)) = (&p.controller, &owner)
        && given != owner
    {
        return Err(RpcError::invalid(format!(
            "unit {} is controlled by `{owner}`, not `{given}`",
            p.unit
        )));
    }
    if let Some(c) = p.controller.as_ref().or(owner.as_ref()) {
        world
            .get_resource_or_init::<AgentStats>()
            .observed(c, p.unit, tick);
    }
    let opts = observe::ObserveOptions {
        recent: p.events.min(200),
        brief: p.brief,
    };
    if let Some(s) = p.since_seq {
        check_since(world, s)?;
    }
    let (mut obs, lines) = observe::observe_with(world, SimId(p.unit), opts)
        .ok_or_else(|| RpcError::command(&CommandError::UnknownUnit { unit: p.unit }))?;
    let changes = p
        .since_seq
        .map(|s| observe::changes(world, SimId(p.unit), s));
    Ok(match p.format {
        Format::Json => {
            if let Some(c) = changes {
                obs["changes"] = c;
            }
            obs
        }
        Format::Text => {
            let mut text = observe::render_text(&obs, &lines);
            if let Some(c) = &changes {
                text = format!("{}\n{text}", observe::since_line(c));
            }
            let mut out = json!({
                "unit": p.unit,
                "tick": obs["tick"],
                "seq": obs["seq"],
                "epoch": obs["epoch"],
                "format": "text",
                "text": text,
            });
            if let Some(c) = changes {
                out["changes"] = c;
            }
            out
        }
    })
}

/// A `since_seq` beyond the latest event seq is an error: the world was
/// rebuilt (`world/reset`, `world/load_scenario`; see `epoch`).
fn check_since(world: &World, since_seq: u64) -> Result<(), RpcError> {
    let last = events::last_seq(world);
    if since_seq <= last {
        return Ok(());
    }
    let mut e = RpcError::invalid(format!(
        "since_seq {since_seq} is ahead of the latest seq {last} (epoch {}): the world was rebuilt \
         (world/reset or world/load_scenario); poll again from since_seq 0",
        epoch(world)
    ));
    e.data = Some(json!({"last_seq": last, "epoch": epoch(world)}));
    Err(e)
}

// ---------------------------------------------------------------- act

const ACT: &str = "{unit, controller, affordance? | verb + target?/pos?/args? | steps: [{affordance? | verb, target?, pos?, args?}], queue?, expect_tick?, expect_seq?, allow_stale?}";

/// What a position that is not integer millimetres is refused with (agent
/// feedback, proposal 003: `[-6.5, 6.0]` meant metres).
pub const POS_HINT: &str = "positions are integer millimetres: 6.5 m is 6500";

fn is_mm(v: &Value) -> bool {
    v.as_i64().is_some_and(|n| i32::try_from(n).is_ok())
}

/// Whether `v` is a list of numbers that is not an `[x, z]` pair of
/// `i32` millimetres (other shapes are left to the type's own error).
fn bad_pair(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| a.iter().all(Value::is_number) && (a.len() != 2 || !a.iter().all(is_mm)))
}

/// The first position in `v` that is not integer millimetres: a `pos`
/// (`[x, z]`) or a `points` entry (`[[x, z], ..]`) at any depth, as
/// `"pos [-6.5,6.0]: positions are integer millimetres: 6.5 m is 6500"`.
pub fn bad_position(v: &Value) -> Option<String> {
    match v {
        Value::Object(m) => m.iter().find_map(|(k, x)| {
            let bad = match k.as_str() {
                "pos" => bad_pair(x).then(|| x.to_string()),
                "points" => x
                    .as_array()
                    .and_then(|a| a.iter().find(|p| bad_pair(p)))
                    .map(Value::to_string),
                _ => None,
            };
            match bad {
                Some(shown) => Some(format!("{k} {shown}: {POS_HINT}")),
                None => bad_position(x),
            }
        }),
        Value::Array(a) => a.iter().find_map(bad_position),
        _ => None,
    }
}

/// [`params`] for `agent/act` (and `agent/act_and_wait`): a `pos` of the
/// act or of a step that is not integer millimetres is refused with
/// [`POS_HINT`] (`args` are checked per step, as a rejection).
fn act_params(p: Option<Value>, shape: &str) -> Result<ActParams, RpcError> {
    if let Some(Value::Object(m)) = &p {
        let mut own = serde_json::Map::new();
        for k in ["pos", "steps"] {
            if let Some(v) = m.get(k) {
                own.insert(k.to_owned(), v.clone());
            }
        }
        if let Some(steps) = own.get_mut("steps").and_then(Value::as_array_mut) {
            for step in steps.iter_mut().filter_map(Value::as_object_mut) {
                step.remove("args");
            }
        }
        if let Some(why) = bad_position(&Value::Object(own)) {
            return Err(RpcError::invalid(format!("bad params: {why}")));
        }
    }
    params(p, shape)
}

/// Most steps of one atomic `agent/act`.
pub const MAX_ACT_STEPS: usize = 16;

/// One step of an atomic multi-step act (`steps`): an affordance id or a
/// verb, like a single act.
#[derive(Deserialize, Serialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActStep {
    #[serde(default)]
    pub affordance: Option<String>,
    #[serde(default)]
    pub verb: Option<String>,
    #[serde(default)]
    pub target: Option<u32>,
    #[serde(default)]
    pub pos: Option<(i32, i32)>,
    #[serde(default)]
    pub args: Option<Value>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ActParams {
    pub unit: u32,
    pub controller: String,
    #[serde(default)]
    pub affordance: Option<String>,
    #[serde(default)]
    pub verb: Option<String>,
    #[serde(default)]
    pub target: Option<u32>,
    #[serde(default)]
    pub pos: Option<(i32, i32)>,
    #[serde(default)]
    pub args: Option<Value>,
    /// Several steps validated all or nothing, then applied in order (the
    /// first replaces the action queue unless `queue`, the rest append).
    /// Excludes `affordance` / `verb`.
    #[serde(default)]
    pub steps: Option<Vec<ActStep>>,
    /// Append to the queue (default: replace it).
    #[serde(default)]
    pub queue: bool,
    #[serde(default)]
    pub expect_tick: Option<u64>,
    #[serde(default)]
    pub expect_seq: Option<u64>,
    #[serde(default)]
    pub allow_stale: bool,
}

fn reject_with(
    world: &mut World,
    unit: u32,
    controller: &str,
    verb: &str,
    r: Rejection,
    extra: Value,
) -> RpcResult {
    world
        .get_resource_or_init::<AgentStats>()
        .rejected(controller, r.reason == "stale");
    // The unit's own stream gets it (its observation's recent events).
    let unit = world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(SimId(unit)))
        .map(|_| SimId(unit));
    let (c, v, reason) = (controller.to_owned(), verb.to_owned(), r.reason.clone());
    journal::record(world, unit, || JournalKind::ActionRejected {
        controller: c,
        verb: v,
        reason,
    });
    let mut out = json!({"accepted": false, "reason": r.reason, "message": r.message});
    if let (Value::Object(m), Value::Object(x)) = (&mut out, extra) {
        m.extend(x);
    }
    stamp(world, out)
}

fn reject(world: &mut World, unit: u32, controller: &str, verb: &str, r: Rejection) -> RpcResult {
    reject_with(world, unit, controller, verb, r, json!({}))
}

/// The entities a request refers to (its target, `args.item`, an order's
/// target or vehicle, an affordance id's `@target`): percepts of these
/// count as interruptions for the staleness guard.
fn referenced_targets(step: &ActStep) -> Vec<SimId> {
    let mut out = Vec::new();
    out.extend(step.target.map(SimId));
    if let Some(id) = &step.affordance
        && let Some((_, t)) = id.rsplit_once('@')
        && let Ok(t) = t.parse::<u32>()
    {
        out.push(SimId(t));
    }
    if let Some(args) = &step.args {
        for v in [
            args.get("item"),
            args.get("target"),
            args.pointer("/order/target"),
            args.pointer("/order/vehicle"),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(t) = v.as_u64().and_then(|t| u32::try_from(t).ok()) {
                out.push(SimId(t));
            }
        }
    }
    out
}

/// One planned part of an act.
enum Part {
    /// An `AgentAct` (Direct): queued steps.
    Act {
        verb: String,
        label: String,
        steps: Vec<Step>,
        est_ticks: Option<u32>,
    },
    /// Plain commands (orders, interactions for Advisors, ..).
    Cmds {
        verb: String,
        label: String,
        commands: Vec<SimCommand>,
    },
}

fn flatten(verb: &str, plan: ActionPlan, out: &mut Vec<Part>) {
    match plan {
        ActionPlan::Queue {
            label,
            steps,
            est_ticks,
        } => out.push(Part::Act {
            verb: verb.to_owned(),
            label,
            steps,
            est_ticks,
        }),
        ActionPlan::Commands { label, commands } => out.push(Part::Cmds {
            verb: verb.to_owned(),
            label,
            commands,
        }),
        ActionPlan::Sequence { plans, .. } => {
            for p in plans {
                flatten(verb, p, out);
            }
        }
    }
}

/// Turns one step into a request (resolving an affordance id).
fn resolve(world: &World, ctx: &AgentCtx, step: &ActStep) -> Result<ActionRequest, Rejection> {
    if let Some(why) = step.args.as_ref().and_then(bad_position) {
        return Err(Rejection::new("invalid_args", why));
    }
    let args = match &step.args {
        None | Some(Value::Null) => json!({}),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(v) => {
            return Err(Rejection::new(
                "invalid_args",
                format!("args must be a JSON object, got {v}"),
            ));
        }
    };
    if let Some(id) = &step.affordance {
        let affs = observe::affordances(world, ctx);
        let Some(a) = affs.into_iter().find(|a| &a.id == id) else {
            return Err(Rejection::new(
                "unknown_affordance",
                format!("no affordance `{id}` is offered now (re-observe)"),
            ));
        };
        // A later step of an atomic act may depend on the earlier ones
        // (`execute_orders` after an `order`): its handler decides.
        if !a.ok && ctx.planned.is_empty() {
            return Err(Rejection::new("unavailable", a.why_not.unwrap_or_default()));
        }
        // `a.args` is the schema; the values are the caller's.
        Ok(ActionRequest {
            verb: a.verb,
            target: a.target,
            pos: step.pos.or(a.pos),
            args,
        })
    } else if let Some(verb) = &step.verb {
        Ok(ActionRequest {
            verb: verb.clone(),
            target: step.target.map(SimId),
            pos: step.pos,
            args,
        })
    } else {
        Err(Rejection::new(
            "invalid",
            format!("give `affordance` or `verb` (expected {ACT})"),
        ))
    }
}

/// The unit's controller, if it is `controller`: `not_attached` (no
/// controller) or `not_controller` (another one) otherwise.
fn controller_of(
    world: &World,
    entity: Entity,
    unit: u32,
    controller: &str,
) -> Result<ExternalController, Rejection> {
    let Some(ctl) = world.get::<ExternalController>(entity).cloned() else {
        return Err(Rejection::new(
            "not_attached",
            format!("unit {unit} has no controller"),
        ));
    };
    if ctl.id != controller {
        return Err(Rejection::new(
            "not_controller",
            format!("unit {unit} is controlled by `{}`", ctl.id),
        ));
    }
    Ok(ctl)
}

/// `read_only` for an Observer controller.
fn writable(ctl: &ExternalController) -> Result<(), Rejection> {
    if ctl.mode == ControlMode::Observer {
        return Err(Rejection::new(
            "read_only",
            "Observer controllers cannot act",
        ));
    }
    Ok(())
}

/// The check `agent/act` and the agent notes methods share: `controller`
/// controls `unit` and is not an Observer (`not_attached`,
/// `not_controller`, `read_only`). A unit that does not exist has no
/// controller.
pub fn check_controller(world: &World, unit: u32, controller: &str) -> Result<(), Rejection> {
    let Some(entity) = world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(SimId(unit)))
    else {
        return Err(Rejection::new(
            "not_attached",
            format!("unit {unit} has no controller"),
        ));
    };
    writable(&controller_of(world, entity, unit, controller)?)
}

fn next_handle(world: &mut World) -> u32 {
    let mut st = world.get_resource_or_init::<AgentIoState>();
    st.next_handle += 1;
    st.next_handle - 1
}

/// `agent/act`: validates the request against the unit's controller,
/// staleness, the offered affordances and the verb's handler, then submits
/// it as recorded commands (`AgentAct` for `Direct` queued steps, the
/// handler's commands such as an `IssueOrder`). With `steps`, every step is
/// planned and validated first and nothing is submitted unless all are
/// valid (all or nothing); a `Commands` plan's commands likewise.
pub fn act(world: &mut World, p: ActParams) -> RpcResult {
    let entity = unit_entity(world, p.unit)?;
    let tick = now(world)?;
    let multi = p.steps.is_some();
    let steps: Vec<ActStep> = match &p.steps {
        Some(steps) => {
            if p.affordance.is_some() || p.verb.is_some() {
                return Err(RpcError::invalid(
                    "give either `steps` or `affordance` / `verb`, not both",
                ));
            }
            if steps.is_empty() || steps.len() > MAX_ACT_STEPS {
                return Err(RpcError::invalid(format!(
                    "steps must have 1..={MAX_ACT_STEPS} entries"
                )));
            }
            steps.clone()
        }
        None => vec![ActStep {
            affordance: p.affordance.clone(),
            verb: p.verb.clone(),
            target: p.target,
            pos: p.pos,
            args: p.args.clone(),
        }],
    };
    if steps
        .iter()
        .any(|s| s.affordance.is_none() && s.verb.is_none())
    {
        return Err(RpcError::invalid(format!(
            "give `affordance` or `verb` (expected {ACT})"
        )));
    }
    let verb_hint = steps
        .first()
        .and_then(|s| s.verb.clone().or_else(|| s.affordance.clone()))
        .unwrap_or_default();
    let (unit, controller) = (p.unit, p.controller.clone());
    // `check_controller` in two halves: an Observer's act still counts as
    // an act in the stats.
    let ctl = match controller_of(world, entity, unit, &controller) {
        Ok(ctl) => ctl,
        Err(r) => return reject(world, unit, &controller, &verb_hint, r),
    };
    let latency = world
        .get_resource_or_init::<AgentStats>()
        .acted(&controller, unit, tick);
    if let Err(r) = writable(&ctl) {
        return reject(world, unit, &controller, &verb_hint, r);
    }
    if !p.allow_stale {
        let config = world
            .get_resource::<AgentIoConfig>()
            .copied()
            .unwrap_or_default();
        if let Some(t) = p.expect_tick
            && tick.saturating_sub(t) > config.stale_ticks
        {
            return reject(
                world,
                unit,
                &controller,
                &verb_hint,
                Rejection::new(
                    "stale",
                    format!(
                        "observed at tick {t}, now {tick}: more than {} ticks ago (re-observe, or allow_stale)",
                        config.stale_ticks
                    ),
                ),
            );
        }
        if let Some(s) = p.expect_seq {
            let targets: Vec<SimId> = steps.iter().flat_map(referenced_targets).collect();
            let found = events::interruptions_since(world, SimId(unit), s, &targets);
            if found.len() > config.stale_events {
                let shown: Vec<String> = found
                    .iter()
                    .take(5)
                    .map(|e| {
                        observe::event_line(&serde_json::to_value(e).unwrap_or(Value::Null))
                            .trim_start_matches("- ")
                            .to_owned()
                    })
                    .collect();
                let more = found.len().saturating_sub(shown.len());
                let message = format!(
                    "{} event(s) changed the unit's situation since seq {s}: {}{} (re-observe, or allow_stale)",
                    found.len(),
                    shown.join("; "),
                    if more > 0 {
                        format!("; and {more} more")
                    } else {
                        String::new()
                    }
                );
                return reject_with(
                    world,
                    unit,
                    &controller,
                    &verb_hint,
                    Rejection::new("stale", message),
                    json!({"interrupting": found}),
                );
            }
        }
    }
    // Plan every step; nothing is submitted until all are valid.
    let mut planned: Vec<SimCommand> = Vec::new();
    let mut parts: Vec<Part> = Vec::new();
    let mut verbs: Vec<String> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let ctx = AgentCtx {
            unit: SimId(unit),
            entity,
            controller: Some(&ctl),
            planned: &planned,
        };
        let step_extra = if multi { json!({"step": i}) } else { json!({}) };
        let hint = step
            .verb
            .clone()
            .or_else(|| step.affordance.clone())
            .unwrap_or_default();
        let request = match resolve(world, &ctx, step) {
            Ok(r) => r,
            Err(r) => return reject_with(world, unit, &controller, &hint, r, step_extra),
        };
        if let Some(reason) = crate::verb_gate(world, entity, &request.verb) {
            let verb = request.verb.clone();
            return reject_with(
                world,
                unit,
                &controller,
                &verb,
                Rejection::new("not_allowed", reason),
                step_extra,
            );
        }
        let handler = world
            .get_resource::<AgentRegistry>()
            .and_then(|r| r.handlers.get(request.verb.as_str()).copied());
        let Some(handler) = handler else {
            let verb = request.verb.clone();
            return reject_with(
                world,
                unit,
                &controller,
                &verb,
                Rejection::new("unknown_verb", format!("no handler for verb `{verb}`")),
                step_extra,
            );
        };
        let plan = match handler(world, &ctx, &request) {
            Ok(plan) => plan,
            Err(r) => return reject_with(world, unit, &controller, &request.verb, r, step_extra),
        };
        let mut new_parts = Vec::new();
        flatten(&request.verb, plan, &mut new_parts);
        for part in &new_parts {
            match part {
                Part::Act { .. } if ctl.mode != ControlMode::Direct => {
                    return reject_with(
                        world,
                        unit,
                        &controller,
                        &request.verb,
                        Rejection::new(
                            "direct_only",
                            format!("`{}` needs Direct control", request.verb),
                        ),
                        step_extra,
                    );
                }
                Part::Act { steps, .. } => {
                    if let Err(e) = validate_steps(world, steps) {
                        return reject_with(
                            world,
                            unit,
                            &controller,
                            &request.verb,
                            Rejection::new("invalid", e.to_string()),
                            step_extra,
                        );
                    }
                }
                Part::Cmds { commands, .. } => {
                    for cmd in commands {
                        let checked = canonical(world, cmd.clone())
                            .and_then(|c| validate(world, &c).map(|()| c));
                        match checked {
                            Ok(c) => planned.push(c),
                            Err(e) => {
                                return reject_with(
                                    world,
                                    unit,
                                    &controller,
                                    &request.verb,
                                    Rejection::new("invalid", e.to_string()),
                                    step_extra,
                                );
                            }
                        }
                    }
                }
            }
        }
        verbs.push(request.verb);
        parts.extend(new_parts);
    }
    // The action queue must hold every queued part.
    let acts = parts
        .iter()
        .filter(|x| matches!(x, Part::Act { .. }))
        .count();
    if acts > 0 {
        let pending = world
            .get::<ExternalQueue>(entity)
            .map_or(0, |q| q.pending.len());
        let base = if p.queue {
            with_same_tick_acts(world, unit, pending)
        } else {
            0
        };
        if base + acts > MAX_QUEUE {
            return reject(
                world,
                unit,
                &controller,
                &verb_hint,
                Rejection::new("invalid", format!("the queue is full ({MAX_QUEUE})")),
            );
        }
    }
    // Submit, in order.
    let mut applies_at = tick;
    let mut first_act = true;
    let mut report = Vec::new();
    for part in parts {
        match part {
            Part::Act {
                verb,
                label,
                steps,
                est_ticks,
            } => {
                let handle = next_handle(world);
                let cmd = AgentAct {
                    unit,
                    controller: controller.clone(),
                    handle,
                    replace: first_act && !p.queue,
                    verb: verb.clone(),
                    label: label.clone(),
                    steps,
                    latency_ticks: latency,
                };
                first_act = false;
                match submit(world, None, SimCommand::content(&cmd)) {
                    Ok((at, _)) => applies_at = at,
                    Err(e) => {
                        return reject(
                            world,
                            unit,
                            &controller,
                            &verb,
                            Rejection::new("invalid", e.to_string()),
                        );
                    }
                }
                report.push(json!({"verb": verb, "label": label, "handle": handle,
                                   "est_ticks": est_ticks}));
            }
            Part::Cmds {
                verb,
                label,
                commands,
            } => {
                for cmd in commands {
                    match submit(world, None, cmd) {
                        Ok((at, _)) => applies_at = at,
                        Err(e) => {
                            return reject(
                                world,
                                unit,
                                &controller,
                                &verb,
                                Rejection::new("invalid", e.to_string()),
                            );
                        }
                    }
                }
                report.push(json!({"verb": verb, "label": label, "handle": null}));
            }
        }
    }
    world
        .get_resource_or_init::<AgentStats>()
        .accepted(&controller);
    let handles: Vec<Value> = report
        .iter()
        .map(|r| r["handle"].clone())
        .filter(|h| !h.is_null())
        .collect();
    let out = if multi || report.len() > 1 {
        json!({
            "accepted": true, "steps": report, "handles": handles,
            "handle": handles.last().cloned().unwrap_or(Value::Null),
            "verb": verbs.join("+"),
            "label": report.iter().filter_map(|r| r["label"].as_str()).collect::<Vec<_>>().join("; "),
            "queued": p.queue, "applies_at": applies_at, "mode": ctl.mode.name(),
        })
    } else {
        let r = &report[0];
        if r["handle"].is_null() {
            json!({
                "accepted": true, "handle": null, "verb": r["verb"], "label": r["label"],
                "applies_at": applies_at, "mode": ctl.mode.name(),
            })
        } else {
            json!({
                "accepted": true, "handle": r["handle"], "verb": r["verb"],
                "label": r["label"], "est_ticks": r["est_ticks"], "queued": p.queue,
                "applies_at": applies_at, "mode": ctl.mode.name(),
            })
        }
    };
    stamp(world, out)
}

// ---------------------------------------------------------------- notes

const AGENT_NOTES_LIST: &str = "{unit, controller, book: diary|memory|proposals}";
const AGENT_NOTES_ADD: &str =
    "{unit, controller, book: diary|memory|proposals, title, body, prompted_by?}";
const AGENT_NOTES_UPDATE: &str = "{unit, controller, book: diary|memory|proposals, id, expected: {tick, date}, title?, body?} (only the player sets a status)";
const PLAYER_NOTES_LIST: &str = "{unit, book: diary|memory|proposals}";
const PLAYER_NOTES_ADD: &str = "{unit, book: diary|memory|proposals, title, body, prompted_by?}";
const PLAYER_NOTES_UPDATE: &str = "{unit, book: diary|memory|proposals, id, expected: {tick, date}, title?, body?, status?: new|liked|pursuing|rejected}";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentNotesList {
    unit: u32,
    controller: String,
    book: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentNotesAdd {
    unit: u32,
    controller: String,
    book: String,
    title: String,
    body: String,
    #[serde(default)]
    prompted_by: Option<String>,
}

/// No `status`: an agent never sets one (an unknown field is invalid
/// params, so an agent cannot pose as the player).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentNotesUpdate {
    unit: u32,
    controller: String,
    book: String,
    id: u32,
    expected: Stamp,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerNotesList {
    unit: u32,
    book: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerNotesAdd {
    unit: u32,
    book: String,
    title: String,
    body: String,
    #[serde(default)]
    prompted_by: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayerNotesUpdate {
    unit: u32,
    book: String,
    id: u32,
    expected: Stamp,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// One notes operation.
enum NotesOp {
    List,
    Add(NewNote),
    /// Entry id, the `updated` stamp the writer read, the change.
    Update(u32, Stamp, NoteChange),
}

fn parse_book(name: &str) -> Result<Book, RpcError> {
    Book::parse(name).ok_or_else(|| {
        RpcError::invalid(format!(
            "unknown book `{name}`; books: diary, memory, proposals"
        ))
    })
}

fn parse_status(name: &str) -> Result<ProposalStatus, RpcError> {
    ProposalStatus::parse(name).ok_or_else(|| {
        RpcError::invalid(format!(
            "unknown status `{name}`; statuses: new, liked, pursuing, rejected"
        ))
    })
}

/// A store error as a reply: `Stale` is a rejection result (like a stale
/// act, with the entry's `current` stamp), bad input is invalid params
/// (`data.reason`), a file error is internal.
fn notes_error(world: &World, e: &NotesError) -> RpcResult {
    let (code, reason) = match e {
        NotesError::Stale { current, .. } => {
            return stamp(
                world,
                json!({"accepted": false, "reason": "stale", "message": e.to_string(),
                       "current": current}),
            );
        }
        NotesError::TooLong { .. } => (codes::INVALID_PARAMS, "too_long"),
        NotesError::NotFound(_) => (codes::INVALID_PARAMS, "not_found"),
        NotesError::Forbidden(_) => (codes::INVALID_PARAMS, "forbidden"),
        NotesError::Io(_) => (codes::INTERNAL, "io"),
    };
    let mut err = RpcError::new(code, e.to_string());
    err.data = Some(json!({"reason": reason}));
    Err(err)
}

/// The notes methods: `controller` given for the agent ones (writes are
/// checked like `agent/act`, a list only needs the controller, so an
/// Observer reads; writer `Agent`), `None` for the player ones (writer
/// `Player`). Stamps are the current tick and [`NotesClock`]'s date. Notes
/// are host-side: nothing here touches sim state.
fn notes_call(
    world: &mut World,
    unit: u32,
    controller: Option<&str>,
    book: &str,
    op: NotesOp,
) -> RpcResult {
    let book = parse_book(book)?;
    let tick = now(world)?;
    let writer = match controller {
        Some(c) => {
            let entity = unit_entity(world, unit)?;
            // Reading is not acting: an Observer lists its notes; writes
            // need the full `agent/act` check.
            let checked = match op {
                NotesOp::List => controller_of(world, entity, unit, c).map(drop),
                NotesOp::Add(_) | NotesOp::Update(..) => check_controller(world, unit, c),
            };
            if let Err(r) = checked {
                return stamp(
                    world,
                    json!({"accepted": false, "reason": r.reason, "message": r.message}),
                );
            }
            Writer::Agent
        }
        None => Writer::Player,
    };
    let Some(dir) = world
        .get_resource::<NotesDirs>()
        .and_then(|d| d.0.get(&SimId(unit)))
        .cloned()
    else {
        return Err(RpcError::invalid(format!("unit {unit} keeps no notes")));
    };
    let at = Stamp {
        tick,
        date: world
            .get_resource::<NotesClock>()
            .map_or_else(String::new, |c| (c.0)()),
    };
    let result = {
        let mut stores = world.get_resource_or_init::<NotesStores>();
        stores.store_for(&dir).and_then(|store| {
            match op {
            NotesOp::List => store.list(book).map(|notes| {
                json!({"unit": unit, "book": book.as_str(), "notes": notes,
                       "unreadable": store.unreadable(book)})
            }),
            NotesOp::Add(new) => store.add(book, writer, new, at).map(|note| {
                json!({"accepted": true, "unit": unit, "book": book.as_str(), "note": note})
            }),
            NotesOp::Update(id, expected, change) => store
                .update(book, writer, id, &expected, change, at)
                .map(|note| {
                    json!({"accepted": true, "unit": unit, "book": book.as_str(), "note": note})
                }),
        }
        })
    };
    match result {
        Ok(v) => stamp(world, v),
        Err(e) => notes_error(world, &e),
    }
}

// ---------------------------------------------------------------- events

/// `agent/events` params.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct EventsParams {
    #[serde(default)]
    pub unit: Option<u32>,
    #[serde(default)]
    pub controller: Option<String>,
    #[serde(default)]
    pub since_seq: u64,
    /// Kind names (a list or a comma-separated string, case-insensitive,
    /// checked against the journal's kinds); default: the significant kinds
    /// without perception rolls; `["all"]`: everything.
    #[serde(default, deserialize_with = "kinds")]
    pub kinds: Vec<String>,
    /// Wait up to this long for a matching event (0: answer at once).
    #[serde(default)]
    pub timeout_ms: u64,
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Answer only once `unit` is idle (its action queue, its orders and
    /// whatever else a busy predicate reports are done, and no command of
    /// it is waiting to apply), or at the timeout. Needs `unit`.
    #[serde(default)]
    pub until_idle: bool,
    /// Answer once at least this many matching events are buffered (default
    /// 1), or at the timeout.
    #[serde(default)]
    pub min_events: usize,
    /// Collapse consecutive identical events into one with `"repeats": n`.
    #[serde(default)]
    pub dedupe: bool,
    /// Free-form, ignored: makes otherwise identical concurrent polls
    /// distinct (BRP watching requests are keyed by their params).
    #[serde(default)]
    pub nonce: Option<Value>,
}

fn default_limit() -> usize {
    100
}

fn kinds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum K {
        One(String),
        Many(Vec<String>),
        None(()),
    }
    Ok(match K::deserialize(d)? {
        K::One(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        K::Many(v) => v,
        K::None(()) => Vec::new(),
    })
}

/// Parses `agent/events` params; unknown kind names are an error listing
/// the valid ones.
pub fn parse_events(p: Option<Value>) -> Result<EventsParams, RpcError> {
    let mut p: EventsParams = params(
        p,
        "{unit?, since_seq?, kinds?, timeout_ms?, limit?, until_idle?, min_events?, dedupe?, nonce?}",
    )?;
    let mut canonical = Vec::with_capacity(p.kinds.len());
    for k in &p.kinds {
        if k.eq_ignore_ascii_case("all") {
            canonical.push("all".to_owned());
        } else if let Some(name) = JournalKind::parse_name(k) {
            canonical.push(name.to_owned());
        } else {
            let mut e = RpcError::invalid(format!(
                "unknown event kind `{k}`; valid kinds: all, {}",
                JournalKind::NAMES.join(", ")
            ));
            e.data = Some(json!({"valid_kinds": JournalKind::NAMES.to_vec()}));
            return Err(e);
        }
    }
    p.kinds = canonical;
    if p.until_idle && p.unit.is_none() {
        return Err(RpcError::invalid("until_idle needs `unit`"));
    }
    Ok(p)
}

fn query(p: &EventsParams) -> EventQuery {
    EventQuery {
        unit: p.unit.map(SimId),
        since_seq: p.since_seq,
        kinds: p.kinds.clone(),
        limit: p.limit.clamp(1, 10_000),
    }
}

/// What keeps `unit` busy, if anything: its action queue, a command of it
/// still waiting to apply, or a registered busy predicate (orders, an
/// interaction, a craft, a vehicle drive).
pub fn busy_reason(world: &World, unit: SimId) -> Option<String> {
    let e = world.get_resource::<SimIndex>()?.get(unit)?;
    if let Some(q) = world.get::<ExternalQueue>(e)
        && q.busy()
    {
        return Some("its action queue".to_owned());
    }
    if let Some(q) = world.get_resource::<CommandQueue>()
        && q.pending.values().any(|c| {
            c.unit_in(world) == Some(unit)
                || matches!(c, SimCommand::Content { payload, .. }
                if payload.get("units").and_then(Value::as_array).is_some_and(|u| {
                    u.iter().any(|x| x.as_u64() == Some(u64::from(unit.0)))
                }))
        })
    {
        return Some("a command waiting to apply".to_owned());
    }
    world
        .get_resource::<AgentRegistry>()?
        .busy
        .values()
        .find_map(|f| f(world, e))
}

/// Whether `agent/events` with `p` has something to return now: enough
/// matching events (`min_events`, default 1), or with `until_idle` the unit
/// is idle; also at once when `since_seq` is ahead of the latest seq (the
/// result is then an error: the world was rebuilt).
pub fn events_ready(world: &mut World, p: &EventsParams) -> bool {
    events::harvest(world);
    let Some(ev) = world.get_resource::<AgentEvents>() else {
        return false;
    };
    if p.since_seq > ev.last_seq() {
        return true;
    }
    if p.until_idle {
        return p
            .unit
            .is_none_or(|u| busy_reason(world, SimId(u)).is_none());
    }
    query(p).count(ev) >= p.min_events.max(1)
}

fn dedupe(found: &[AgentEvent]) -> Vec<Value> {
    let mut out: Vec<(Value, u64)> = Vec::new();
    for e in found {
        if let Some((last, n)) = out.last_mut()
            && last["unit"] == json!(e.unit)
            && last["event"] == serde_json::to_value(&e.event).unwrap_or(Value::Null)
        {
            *n += 1;
            last["last_seq"] = json!(e.seq);
            last["last_tick"] = json!(e.tick);
            continue;
        }
        out.push((serde_json::to_value(e).unwrap_or(Value::Null), 1));
    }
    out.into_iter()
        .map(|(mut v, n)| {
            if n > 1 {
                v["repeats"] = json!(n);
            }
            v
        })
        .collect()
}

/// The `agent/events` result: `{events, next_seq, timed_out, oldest_seq,
/// dropped, journal, tick, seq, epoch}` (plus `idle` / `busy` with
/// `until_idle`); `next_seq` is what to pass as `since_seq` next time.
/// `dropped` counts events after `since_seq` that the ring buffer evicted
/// before this call (the poller missed them). `since_seq` beyond the latest
/// seq is an error (the world was rebuilt: see `epoch`).
pub fn events_result(world: &mut World, p: &EventsParams, timed_out: bool) -> RpcResult {
    events::harvest(world);
    if let Some(unit) = p.unit {
        unit_entity(world, unit).or_else(|e| {
            // A despawned unit still has its last events.
            if world
                .get_resource::<AgentEvents>()
                .is_some_and(|ev| ev.events.iter().any(|x| x.unit == Some(SimId(unit))))
            {
                Ok(Entity::PLACEHOLDER)
            } else {
                Err(e)
            }
        })?;
    }
    let (last, oldest, dropped_total) = world
        .get_resource::<AgentEvents>()
        .map_or((0, 1, 0), |ev| (ev.last_seq(), ev.oldest_seq(), ev.dropped));
    check_since(world, p.since_seq)?;
    let q = query(p);
    let found = world
        .get_resource::<AgentEvents>()
        .map(|ev| q.run(ev))
        .unwrap_or_default();
    let next_seq = if found.len() >= q.limit {
        found.last().map_or(p.since_seq, |e| e.seq)
    } else {
        // Nothing else after `since_seq` matched: skip ahead.
        last.max(p.since_seq)
    };
    let missed = if dropped_total > 0 && oldest > p.since_seq + 1 {
        oldest - p.since_seq - 1
    } else {
        0
    };
    let journal_on = journal::is_recording(world);
    let events: Value = if p.dedupe {
        Value::Array(dedupe(&found))
    } else {
        json!(found)
    };
    let mut out = json!({
        "events": events, "next_seq": next_seq,
        "timed_out": timed_out && (p.until_idle || found.is_empty()),
        "oldest_seq": oldest, "dropped": missed, "journal": journal_on,
    });
    if p.until_idle
        && let Some(u) = p.unit
    {
        let busy = busy_reason(world, SimId(u));
        out["idle"] = json!(busy.is_none());
        out["busy"] = json!(busy);
    }
    stamp(world, out)
}

/// The timeout of an `agent/events` call, clamped to the configured maximum.
pub fn events_timeout_ms(world: &World, p: &EventsParams) -> u64 {
    let max = world
        .get_resource::<AgentIoConfig>()
        .map_or(AgentIoConfig::default().max_timeout_ms, |c| {
            c.max_timeout_ms
        });
    p.timeout_ms.min(max)
}

// ---------------------------------------------------------------- act and wait

/// `agent/act_and_wait`'s default wait.
pub const ACT_WAIT_DEFAULT_MS: u64 = 30_000;
/// `agent/act_and_wait`'s longest wait (larger `timeout_ms` are clamped).
pub const ACT_WAIT_MAX_MS: u64 = 120_000;

const ACT_AND_WAIT: &str = "{unit, controller, ..every agent/act field.., until_idle?: bool (default true), kinds?: [kind], timeout_ms? (default 30000, max 120000), observe?: {format?: json|text, brief?}, nonce?}";

/// The fields `agent/act_and_wait` adds to `agent/act`'s.
const WAIT_FIELDS: [&str; 5] = ["until_idle", "kinds", "timeout_ms", "observe", "nonce"];

/// `agent/act_and_wait`'s `observe` options.
#[derive(Deserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WaitObserve {
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub brief: bool,
}

/// An `agent/act_and_wait` whose act is done: what is left is the wait
/// ([`act_wait_ready`]) and the answer ([`act_wait_result`]).
#[derive(Clone, Debug)]
pub struct ActWait {
    /// The `agent/act` result.
    pub act: Value,
    pub unit: u32,
    /// The events to return and wait for: `unit`, `since_seq` (the act's
    /// `seq`), `kinds`, `until_idle`.
    pub events: EventsParams,
    /// Clamped to [`ACT_WAIT_MAX_MS`].
    pub timeout_ms: u64,
    pub observe: WaitObserve,
}

impl ActWait {
    /// Whether the act was accepted (a rejection answers at once).
    pub fn accepted(&self) -> bool {
        self.act.get("accepted").and_then(Value::as_bool) == Some(true)
    }

    /// Answer without waiting: a rejected act, or `timeout_ms: 0`.
    pub fn answers_at_once(&self) -> bool {
        !self.accepted() || self.timeout_ms == 0
    }
}

/// `agent/act_and_wait`, part one: validates every param (nothing is acted
/// on bad params), then acts exactly like `agent/act`. Errors are those of
/// `agent/act` plus invalid wait params; a rejection is a result
/// ([`ActWait::answers_at_once`]).
pub fn act_and_wait_start(world: &mut World, p: Option<Value>) -> Result<ActWait, RpcError> {
    events::harvest(world);
    let mut map = match p {
        Some(Value::Object(m)) => m,
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(other) => {
            return Err(RpcError::invalid(format!(
                "bad params (expected {ACT_AND_WAIT}): got {other}"
            )));
        }
    };
    let mut wait = serde_json::Map::new();
    for k in WAIT_FIELDS {
        if let Some(v) = map.remove(k) {
            wait.insert(k.to_owned(), v);
        }
    }
    let act_params: ActParams = act_params(Some(Value::Object(map)), ACT_AND_WAIT)?;
    let until_idle = match wait.get("until_idle") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(v) => {
            return Err(RpcError::invalid(format!(
                "until_idle must be a bool, got {v}"
            )));
        }
    };
    let timeout_ms = match wait.get("timeout_ms") {
        None | Some(Value::Null) => ACT_WAIT_DEFAULT_MS,
        Some(v) => v.as_u64().ok_or_else(|| {
            RpcError::invalid(format!(
                "timeout_ms must be a non-negative integer, got {v}"
            ))
        })?,
    }
    .min(ACT_WAIT_MAX_MS);
    let observe: WaitObserve = params(
        wait.get("observe").cloned(),
        "observe: {format?: json|text, brief?}",
    )?;
    let mut events = parse_events(Some(json!({
        "unit": act_params.unit,
        "kinds": wait.get("kinds").cloned().unwrap_or(Value::Null),
        "until_idle": until_idle,
    })))?;
    let unit = act_params.unit;
    let act = act(world, act_params)?;
    events.since_seq = act["seq"]
        .as_u64()
        .unwrap_or_else(|| events::last_seq(world));
    Ok(ActWait {
        act,
        unit,
        events,
        timeout_ms,
        observe,
    })
}

/// Whether a parked `agent/act_and_wait` can answer now: at once for a
/// rejected act; with `until_idle` once the unit is idle or (with `kinds`)
/// an event of those kinds happened; without, at the first matching event.
pub fn act_wait_ready(world: &mut World, w: &ActWait) -> bool {
    if !w.accepted() {
        return true;
    }
    if !w.events.until_idle {
        return events_ready(world, &w.events);
    }
    if events_ready(world, &w.events) {
        return true;
    }
    if w.events.kinds.is_empty() {
        return false;
    }
    let first = EventsParams {
        until_idle: false,
        ..w.events.clone()
    };
    events_ready(world, &first)
}

/// `agent/act_and_wait`'s answer: `{act, events, observation, tick, seq,
/// epoch}` with `events` the `agent/events` result for the seqs after the
/// act (`timed_out` when the wait ran out) and `observation` the unit's
/// `agent/observe` result, with `changes` since the act. Should the unit be
/// gone, `observation` is null and `observation_error` says why.
pub fn act_wait_result(world: &mut World, w: &ActWait, timed_out: bool) -> RpcResult {
    events::harvest(world);
    let events = events_result(world, &w.events, timed_out)?;
    let observed = observe_method(
        world,
        ObserveParams {
            unit: w.unit,
            controller: None,
            format: w.observe.format,
            events: default_recent(),
            brief: w.observe.brief,
            since_seq: Some(w.events.since_seq),
        },
    );
    let mut out = json!({"act": w.act, "events": events});
    match observed {
        Ok(o) => out["observation"] = o,
        Err(e) => {
            out["observation"] = Value::Null;
            out["observation_error"] = json!({"code": e.code, "message": e.message});
        }
    }
    stamp(world, out)
}

// ---------------------------------------------------------------- stats, units

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatsParams {
    #[serde(default)]
    controller: Option<String>,
}

fn stats(world: &mut World, p: StatsParams) -> RpcResult {
    let report = world
        .get_resource_or_init::<AgentStats>()
        .report(p.controller.as_deref());
    stamp(world, json!({"controllers": report}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitsParams {
    #[serde(default)]
    kind: Option<String>,
}

fn list_units(world: &mut World, p: UnitsParams) -> RpcResult {
    let info: Vec<crate::registry::UnitInfoFn> = world
        .get_resource::<AgentRegistry>()
        .map(|r| r.unit_info.values().copied().collect())
        .unwrap_or_default();
    let units: Vec<Value> = inspect::sim_entities(world)
        .into_iter()
        .filter_map(|(id, e)| {
            let kind = inspect::kind_name(world, e);
            if p.kind.as_deref().is_some_and(|k| k != kind) {
                return None;
            }
            let pos = world.get::<npc_kernel::SimPos>(e).map(|p| [p.x, p.z]);
            let controller = world
                .get::<ExternalController>(e)
                .map(|c| json!({"id": c.id, "mode": c.mode.name(), "on_idle": c.on_idle.name()}));
            let mut row = json!({
                "id": id, "kind": kind, "pos": pos,
                "controllable": world.get::<npc_ai::Brain>(e).is_some(),
                "controller": controller,
            });
            for f in &info {
                if let (Some(Value::Object(extra)), Value::Object(m)) = (f(world, e), &mut row) {
                    m.extend(extra);
                }
            }
            Some(row)
        })
        .collect();
    stamp(world, json!({"units": units}))
}

// ---------------------------------------------------------------- npc/* subset

/// The `npc/*` methods the headless session host serves (the BRP host has
/// `npc_tools`' full set): pause, resume, speed, step, state, unit,
/// command, journal.
pub const NPC_METHODS: [&str; 8] = [
    "npc/pause",
    "npc/resume",
    "npc/speed",
    "npc/step",
    "npc/state",
    "npc/unit",
    "npc/command",
    "npc/journal",
];

fn control_state(world: &mut World) -> RpcResult {
    let tick = now(world)?;
    let c = *world.get_resource_or_init::<SimControl>();
    Ok(json!({"tick": tick, "paused": c.paused, "multiplier": c.multiplier, "driver": "session"}))
}

/// Dispatches one of [`NPC_METHODS`].
pub fn call_npc(world: &mut World, method: &str, p: Option<Value>) -> RpcResult {
    match method {
        "npc/pause" | "npc/resume" => {
            now(world)?;
            world.get_resource_or_init::<SimControl>().paused = method == "npc/pause";
            control_state(world)
        }
        "npc/speed" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct P {
                multiplier: u32,
            }
            let P { multiplier } = params(p, "{multiplier}")?;
            if multiplier > 1000 {
                return Err(RpcError::invalid("multiplier exceeds 1000"));
            }
            world.get_resource_or_init::<SimControl>().multiplier = multiplier;
            control_state(world)
        }
        "npc/step" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct P {
                #[serde(default = "one")]
                ticks: u64,
            }
            fn one() -> u64 {
                1
            }
            let P { ticks } = params(p, "{ticks?}")?;
            if ticks > 1_000_000 {
                return Err(RpcError::invalid("ticks exceeds 1000000"));
            }
            now(world)?;
            npc_kernel::run_ticks(world, ticks);
            let hash = npc_kernel::hash::state_hash(world);
            Ok(
                json!({"tick": now(world)?, "ticks": ticks, "hash": hash, "hash_hex": format!("{hash:016x}")}),
            )
        }
        "npc/state" => call(world, "world/state", None),
        "npc/unit" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct P {
                id: u32,
            }
            let P { id } = params(p, "{id}")?;
            inspect::unit_json(world, SimId(id))
                .ok_or_else(|| RpcError::command(&CommandError::UnknownUnit { unit: id }))
        }
        "npc/command" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct P {
                cmd: SimCommand,
                #[serde(default)]
                at_tick: Option<u64>,
            }
            let P { cmd, at_tick } = params(p, "{cmd: SimCommand, at_tick?}")?;
            let (tick, seq) = submit(world, at_tick, cmd).map_err(|e| RpcError::command(&e))?;
            Ok(json!({"tick": tick, "seq": seq}))
        }
        "npc/journal" => {
            let filter: JournalFilter = match p {
                Some(Value::Object(mut m)) if m.contains_key("filter") => {
                    params(m.remove("filter"), "JournalFilter")?
                }
                other => params(other, "JournalFilter")?,
            };
            let journal = world
                .get_resource::<Journal>()
                .ok_or_else(|| RpcError::new(codes::NOT_READY, "no journal"))?;
            if !journal.enabled && journal.entries.is_empty() {
                return Err(RpcError::new(codes::JOURNAL_OFF, "the journal is off"));
            }
            serde_json::to_value(journal::query(journal, &filter))
                .map_err(|e| RpcError::new(codes::INTERNAL, e.to_string()))
        }
        other => Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!("Method `{other}` not found"),
        )),
    }
}

/// Every command applied so far (for tests and session records).
pub fn command_log(world: &World) -> Vec<(u64, u32, SimCommand)> {
    world
        .get_resource::<CommandLog>()
        .map(|l| l.entries.clone())
        .unwrap_or_default()
}
