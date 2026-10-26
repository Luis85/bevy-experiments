//! Deterministic decision and event log (spec §3.2).
//!
//! Every entry is stamped with the tick it happened in and a per-tick `seq`,
//! so `(tick, seq)` is a total order. Entries name units by [`SimId`] and tree
//! nodes by name; they never contain `Entity` ids, wall-clock times or perf
//! data, so the same seed and commands give a byte-identical journal on every
//! machine. Recording only reads sim state: the state hash is the same with
//! the journal on or off.
//!
//! - [`NpcKernelPlugin`](crate::NpcKernelPlugin) always installs a disabled [`Journal`]
//!   (and a disabled [`AiTrace`]), so readers can always find it.
//! - Enabling is one call: `app.add_plugins(JournalPlugin::default())`, or
//!   [`set_journal`] at runtime (e.g. from a remote "record" command).
//! - Storage is a ring buffer of `capacity` entries (default
//!   [`DEFAULT_CAPACITY`]) plus an optional streaming [`JournalSink`] (JSON
//!   Lines, flushed at the end of every tick), so long runs never grow memory.
//!
//! Who records what:
//! - AI events: `npc_ai::think` writes [`AiTrace`]; `think_system` (and every
//!   [`record`] call, so order is kept) drains it, mapping `Entity → SimId`
//!   and node indices to names.
//! - `NeedLevel`: `npc_needs`' decay system; `PhaseChange`: `day_night_system`;
//!   `Effect`: content effect resolvers; `Command`: `apply_commands_system`.
//! - `Reserve` / `Release`: `npc_jobs` (actions and the `SimId` leave hook).
//! - `Spawn`: `spawn_sim`; `Despawn`: the `SimId` despawn hook.

use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use npc_ai::{
    AiRegistry, AiTrace, AiTraceKind, Brain, NodeIdx, Status, StopReason, ThinkReason, TreeDef,
};
use serde::{Deserialize, Serialize};
use tracing::info_span;

use crate::clock::SimClock;
use crate::commands::SimCommand;
use crate::ids::SimId;
use crate::json::JsonObject;

/// Coarse need state derived from a need's value with hysteresis (Ok →
/// Low → Urgent). Defined in the kernel because the journal records level
/// changes; `npc_needs` re-exports it.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    bevy_reflect::Reflect,
)]
pub enum NeedLevel {
    #[default]
    Ok,
    /// "Peckish" / "tired": worth satisfying when convenient.
    Low,
    Urgent,
}

/// Default ring-buffer size of the [`Journal`].
pub const DEFAULT_CAPACITY: usize = 50_000;

/// One journal line. JSON: `{"tick":5,"seq":0,"unit":5,"kind":{"type":"LeafStart",..}}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The tick being run (`SimClock.tick` while it runs).
    pub tick: u64,
    /// Position within the tick, from 0.
    pub seq: u32,
    /// The unit the entry is about (`None` for world events like `PhaseChange`).
    pub unit: Option<SimId>,
    pub kind: JournalKind,
}

/// Why a reservation was released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReleaseReason {
    /// The branch's terminal action finished and freed its target.
    Finished,
    /// The branch was interrupted (its leaf stopped with `Aborted`).
    Aborted,
    /// An action's start failed, failing the whole branch.
    Failed,
    /// A `Reserve*` action dropped old claims before claiming anew.
    Reclaim,
    /// The claimant or the target left the sim (despawned, or its `SimId`
    /// removed).
    Despawn,
}

/// What happened. Serialized with a `"type"` tag naming the variant; filter
/// kinds by that name ([`JournalKind::name`]).
///
/// AI events mirror [`AiTraceKind`] with indices resolved: `node` is the node
/// name, `path` the full `Root > .. > Leaf` path (node names repeat across
/// branches, the path does not), `aborted` the interrupted running path as
/// names from the root.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum JournalKind {
    ThinkStart {
        reason: ThinkReason,
    },
    LeafStart {
        node: String,
        path: String,
        action: String,
        status: Status,
    },
    LeafStop {
        node: String,
        path: String,
        action: String,
        reason: StopReason,
    },
    Abort {
        guard: String,
        aborted: Vec<String>,
    },
    /// Only with a verbose journal.
    CondEval {
        node: String,
        cond: String,
        result: bool,
    },
    RootDone {
        status: Status,
    },
    /// `need` is the need's registered name (e.g. `"Hunger"`).
    NeedLevel {
        need: String,
        from: NeedLevel,
        to: NeedLevel,
    },
    Reserve {
        target: SimId,
        ok: bool,
    },
    Release {
        target: SimId,
        reason: ReleaseReason,
    },
    /// A cross-entity effect was applied (`applied == false`: its target was
    /// missing and it was dropped). `unit` is the effect's source; `effect` is
    /// the content's effect as JSON (field order kept), e.g.
    /// `{"type":"AddFood","stockpile":3,"amount":3}`.
    Effect {
        effect: JsonObject,
        applied: bool,
    },
    /// An applied command (journaled just before it takes effect).
    Command {
        cmd: SimCommand,
    },
    PhaseChange {
        night: bool,
    },
    /// `kind` is the unit kind as in inspection (`"villager"`, `"house"`, ..).
    Spawn {
        kind: String,
    },
    Despawn,
    /// A path request was served (`npc_nav`): `len` waypoints after
    /// smoothing, `nodes` A* expansions (over every tick it took).
    PathFound {
        len: u32,
        nodes: u32,
    },
    /// A path request failed (`npc_nav`).
    PathFailed {
        reason: crate::spatial::PathFailReason,
    },
    /// A success roll (`npc_rules`): `skill` is the skill or stat rolled
    /// against (`"Climbing"`, `"Per"`), `target` the effective level plus
    /// modifiers, `roll` the dice total, `margin = target - roll`, `purpose`
    /// the caller's purpose name (also part of the dice key). In JSON the
    /// target is `target_number` (not to be confused with the entity
    /// `target` of other kinds); `target` is still accepted when reading.
    Roll {
        skill: String,
        #[serde(rename = "target_number", alias = "target")]
        target: i32,
        roll: i32,
        margin: i32,
        success: bool,
        crit: Critical,
        purpose: String,
    },
    /// A unit started sensing `target` (`npc_perception`): `via` is the
    /// sense (`"vision"`, `"proximity"`, `"hearing"`).
    Perceived {
        target: SimId,
        via: String,
    },
    /// `target` left a unit's perception (`npc_perception`): `forgotten` is
    /// false when it is no longer sensed (still remembered) and true when
    /// the memory of it expired or was evicted.
    Lost {
        target: SimId,
        forgotten: bool,
    },
    /// An external controller attached to the unit (`npc_agent_io`):
    /// `mode` is `"Direct"`, `"Advisor"` or `"Observer"`, `on_idle`
    /// `"Idle"` or `"Autopilot"`.
    ControllerAttached {
        controller: String,
        mode: String,
        on_idle: String,
    },
    /// The unit's external controller detached (`npc_agent_io`).
    ControllerDetached {
        controller: String,
    },
    /// An externally controlled action changed state (`npc_agent_io`):
    /// `status` is `"queued"`, `"started"`, `"done"`, `"failed"` or
    /// `"cancelled"`; `latency_ticks` (queued only) is how many ticks the
    /// agent's decision lagged its observation.
    ExternalAction {
        handle: u32,
        verb: String,
        status: String,
        latency_ticks: Option<u64>,
    },
    /// An agent's action was rejected before it became a command
    /// (`npc_agent_io`; tooling-originated, so a replay of the session does
    /// not repeat it): `reason` is a short code (`"stale"`, `"read_only"`, ..).
    ActionRejected {
        controller: String,
        verb: String,
        reason: String,
    },
    /// An interaction changed state (`npc_interaction`): `outcome` is
    /// `"started"`, `"success"`, `"failed"` (the skill check failed: the time
    /// is spent, nothing happens), `"invalid"` (the world changed while it
    /// ran), `"cancelled"` or `"rejected"` (it could not start); `reason`
    /// says why for the last four. A success that put new items on the
    /// ground (a tool yield at the actor's feet) lists them in `dropped`
    /// (left out of the JSON when empty) and, when the actor's pack could
    /// not take them, says why in `reason` (`"pack would carry 30950 of
    /// 30000 g"`). Journal only: nothing here is hashed.
    Interacted {
        verb: String,
        target: SimId,
        item: Option<SimId>,
        outcome: String,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        dropped: Vec<DroppedItem>,
    },
    /// An item changed place (`npc_interaction`): `from`/`to` are holders
    /// (`None`: the ground), `slot` the equipment slot it went into (`None`:
    /// the inventory or the ground). `unit` is the actor.
    ItemMoved {
        item: SimId,
        kind: String,
        stack: u32,
        from: Option<SimId>,
        to: Option<SimId>,
        slot: Option<String>,
    },
    /// A door was opened, closed, locked or unlocked (`npc_interaction`;
    /// `unit` is the actor).
    DoorChanged {
        door: SimId,
        open: bool,
        locked: bool,
    },
    /// A unit took a seat in a vehicle (`npc_vehicles`): `seat` is the
    /// seat index, `role` its role (`"Driver"`, `"Passenger"`, `"Gunner"`).
    Mounted {
        vehicle: SimId,
        seat: u8,
        role: String,
    },
    /// A unit left its vehicle seat (`npc_vehicles`) and now stands at `at`.
    Dismounted {
        vehicle: SimId,
        seat: u8,
        at: crate::math::SimPos,
    },
    /// A vehicle was asked to move but cannot (`npc_vehicles`; `unit` is
    /// the vehicle): `reason` says why (`"no driver"`, a required seat is
    /// empty). Its intent is dropped and its status is `Blocked`.
    VehicleStalled {
        reason: String,
    },
    /// A unit gained experience (`npc_progression`): `amount` XP from the
    /// registered source `source` (`"roll"`, `"craft"`, ..), `xp` the new
    /// total.
    XpGained {
        source: String,
        amount: u32,
        xp: u64,
    },
    /// A unit reached `level` (`npc_progression`) and was granted `points`
    /// banked character points.
    LevelUp {
        level: u32,
        points: i32,
    },
    /// A skill gained `points` skill points (`npc_progression`): `via` is
    /// `"practice"`, `"study"`, `"teaching"` or `"spend"` (the AI spend
    /// policy); `level` is the new effective level.
    SkillImproved {
        skill: String,
        points: i32,
        level: i32,
        via: String,
    },
    /// A craft started (`npc_crafting`): it completes at tick `ends`.
    CraftStarted {
        recipe: String,
        workstation: Option<SimId>,
        ends: u64,
    },
    /// A craft succeeded (`npc_crafting`): `margin` of the skill roll (0
    /// without one), `quality` of the `outputs` it made.
    CraftDone {
        recipe: String,
        workstation: Option<SimId>,
        margin: i32,
        quality: i32,
        outputs: Vec<SimId>,
    },
    /// A craft did not succeed (`npc_crafting`): `outcome` is `"rejected"`
    /// (it could not start), `"failed"` (the roll failed: time spent),
    /// `"critical"` (critical failure: inputs consumed, tool damaged),
    /// `"invalid"` (the world changed while it ran) or `"cancelled"`;
    /// `consumed` whether the inputs were used up.
    CraftFailed {
        recipe: String,
        workstation: Option<SimId>,
        outcome: String,
        reason: String,
        consumed: bool,
    },
    /// An RTS order was given to the unit (`npc_command`): `order` is the
    /// order kind (`"Move"`, `"Patrol"`, ..), `queued` whether it was
    /// shift-queued behind the unit's other orders, `player` who issued it.
    OrderIssued {
        order: String,
        queued: bool,
        player: u32,
    },
    /// The unit started executing an order (`npc_command`).
    OrderStarted {
        order: String,
    },
    /// The unit completed an order (`npc_command`).
    OrderDone {
        order: String,
    },
    /// An order ended without completing (`npc_command`): `reason` is e.g.
    /// `"blocked"`, `"target gone"`, `"replaced"`, `"stopped"`. A walk that
    /// failed as `"blocked"` (with a grid) also says how far it could get:
    /// `nearest_reachable` (mm, the centre of the reachable cell closest to
    /// the goal) and, when known, the `blocker` (`npc_nav::explain_blocked`;
    /// both left out of the JSON when `None`).
    OrderFailed {
        order: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nearest_reachable: Option<[i32; 2]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blocker: Option<crate::spatial::Blocker>,
    },
}

/// An item an interaction put on the ground ([`JournalKind::Interacted`]'s
/// `dropped`): its id, kind, stack and position (`[x, z]`, mm).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DroppedItem {
    pub item: SimId,
    pub kind: String,
    pub stack: u32,
    pub pos: [i32; 2],
}

/// Whether a roll was critical (`npc_rules`). Defined in the kernel because
/// the journal records it; `npc_rules` re-exports it.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    bevy_reflect::Reflect,
)]
pub enum Critical {
    #[default]
    None,
    Success,
    Failure,
}

impl JournalKind {
    /// Every variant name ([`JournalKind::name`], the JSON `"type"` tag), in
    /// declaration order: what `npcctl journal --kind` and BRP filters
    /// accept. A new variant must be added here too (the
    /// `names_cover_every_variant` test checks `name()` against it).
    pub const NAMES: [&'static str; 39] = [
        "ThinkStart",
        "LeafStart",
        "LeafStop",
        "Abort",
        "CondEval",
        "RootDone",
        "NeedLevel",
        "Reserve",
        "Release",
        "Effect",
        "Command",
        "PhaseChange",
        "Spawn",
        "Despawn",
        "PathFound",
        "PathFailed",
        "Roll",
        "Perceived",
        "Lost",
        "ControllerAttached",
        "ControllerDetached",
        "ExternalAction",
        "ActionRejected",
        "Interacted",
        "ItemMoved",
        "DoorChanged",
        "Mounted",
        "Dismounted",
        "VehicleStalled",
        "XpGained",
        "LevelUp",
        "SkillImproved",
        "CraftStarted",
        "CraftDone",
        "CraftFailed",
        "OrderIssued",
        "OrderStarted",
        "OrderDone",
        "OrderFailed",
    ];

    /// The canonical name for `name` (case-insensitive), if it is a kind.
    pub fn parse_name(name: &str) -> Option<&'static str> {
        Self::NAMES
            .iter()
            .copied()
            .find(|n| n.eq_ignore_ascii_case(name))
    }

    /// The variant name, as in the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            JournalKind::ThinkStart { .. } => "ThinkStart",
            JournalKind::LeafStart { .. } => "LeafStart",
            JournalKind::LeafStop { .. } => "LeafStop",
            JournalKind::Abort { .. } => "Abort",
            JournalKind::CondEval { .. } => "CondEval",
            JournalKind::RootDone { .. } => "RootDone",
            JournalKind::NeedLevel { .. } => "NeedLevel",
            JournalKind::Reserve { .. } => "Reserve",
            JournalKind::Release { .. } => "Release",
            JournalKind::Effect { .. } => "Effect",
            JournalKind::Command { .. } => "Command",
            JournalKind::PhaseChange { .. } => "PhaseChange",
            JournalKind::Spawn { .. } => "Spawn",
            JournalKind::Despawn => "Despawn",
            JournalKind::PathFound { .. } => "PathFound",
            JournalKind::PathFailed { .. } => "PathFailed",
            JournalKind::Roll { .. } => "Roll",
            JournalKind::Perceived { .. } => "Perceived",
            JournalKind::Lost { .. } => "Lost",
            JournalKind::ControllerAttached { .. } => "ControllerAttached",
            JournalKind::ControllerDetached { .. } => "ControllerDetached",
            JournalKind::ExternalAction { .. } => "ExternalAction",
            JournalKind::ActionRejected { .. } => "ActionRejected",
            JournalKind::Interacted { .. } => "Interacted",
            JournalKind::ItemMoved { .. } => "ItemMoved",
            JournalKind::DoorChanged { .. } => "DoorChanged",
            JournalKind::Mounted { .. } => "Mounted",
            JournalKind::Dismounted { .. } => "Dismounted",
            JournalKind::VehicleStalled { .. } => "VehicleStalled",
            JournalKind::XpGained { .. } => "XpGained",
            JournalKind::LevelUp { .. } => "LevelUp",
            JournalKind::SkillImproved { .. } => "SkillImproved",
            JournalKind::CraftStarted { .. } => "CraftStarted",
            JournalKind::CraftDone { .. } => "CraftDone",
            JournalKind::CraftFailed { .. } => "CraftFailed",
            JournalKind::OrderIssued { .. } => "OrderIssued",
            JournalKind::OrderStarted { .. } => "OrderStarted",
            JournalKind::OrderDone { .. } => "OrderDone",
            JournalKind::OrderFailed { .. } => "OrderFailed",
        }
    }
}

/// Streaming JSON Lines writer. Every entry is written as it is pushed and
/// the writer is flushed at the end of every tick (`flush_journal_system`).
/// After the first I/O error it logs one warning and stops writing.
pub struct JournalSink {
    out: BufWriter<Box<dyn Write + Send + Sync>>,
    failed: bool,
}

impl JournalSink {
    pub fn new(writer: Box<dyn Write + Send + Sync>) -> Self {
        Self {
            out: BufWriter::new(writer),
            failed: false,
        }
    }

    /// Creates (truncates) `path`.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(Box::new(File::create(path)?)))
    }

    fn write(&mut self, entry: &JournalEntry) {
        if self.failed {
            return;
        }
        let result = serde_json::to_writer(&mut self.out, entry)
            .map_err(io::Error::from)
            .and_then(|()| self.out.write_all(b"\n"));
        self.check(result);
    }

    pub fn flush(&mut self) {
        if !self.failed {
            let result = self.out.flush();
            self.check(result);
        }
    }

    fn check(&mut self, result: io::Result<()>) {
        if let Err(err) = result {
            tracing::warn!("journal sink failed, no further entries are written: {err}");
            self.failed = true;
        }
    }
}

impl fmt::Debug for JournalSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JournalSink")
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

/// The journal. Not hashed, never read by sim decisions.
#[derive(Resource, Debug)]
pub struct Journal {
    /// Record anything at all. AI events also need `AiTrace.enabled`; use
    /// [`set_journal`] / [`JournalPlugin`] to switch both.
    pub enabled: bool,
    /// Also record every condition evaluation (`CondEval`). Informational:
    /// the `AiTrace.verbose` flag produces those events, so change both
    /// together with [`set_journal`].
    pub verbose: bool,
    /// Ring-buffer size; the oldest entries are dropped beyond it.
    pub capacity: usize,
    /// The most recent `capacity` entries in `(tick, seq)` order.
    pub entries: VecDeque<JournalEntry>,
    /// Optional streaming copy of every entry (unaffected by `capacity`).
    pub sink: Option<JournalSink>,
    /// Tick of the last entry and the next seq within it.
    cursor: Option<(u64, u32)>,
}

thread_local! {
    /// `Some(verbose)` inside [`with_journal_from_build`].
    static FROM_BUILD: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Runs `build` (typically a content app factory, e.g. `village_content::
/// try_new_sim_app`) with every [`Journal`] created on this thread starting
/// **enabled**, so the entries recorded while the app is built, i.e. the
/// scenario's tick-0 `Spawn` entries, are kept. Without it the kernel's
/// journal starts disabled and those entries are lost (enabling it after the
/// factory returns is too late).
///
/// Only the `Journal` is affected; call [`set_journal`] on the built app
/// afterwards to switch on the `AiTrace` too (`npc_tools::new_app_journaled`
/// does both). The default (no wrapper) is unchanged: the journal starts off.
pub fn with_journal_from_build<T>(verbose: bool, build: impl FnOnce() -> T) -> T {
    struct Restore(Option<bool>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FROM_BUILD.set(self.0);
        }
    }
    let _restore = Restore(FROM_BUILD.replace(Some(verbose)));
    build()
}

impl Default for Journal {
    /// Disabled, except inside [`with_journal_from_build`].
    fn default() -> Self {
        let from_build = FROM_BUILD.get();
        Self {
            enabled: from_build.is_some(),
            verbose: from_build.unwrap_or(false),
            capacity: DEFAULT_CAPACITY,
            entries: VecDeque::new(),
            sink: None,
            cursor: None,
        }
    }
}

impl Journal {
    /// Appends an entry for `tick` (no-op when disabled).
    pub fn push(&mut self, tick: u64, unit: Option<SimId>, kind: JournalKind) {
        if !self.enabled {
            return;
        }
        let seq = match self.cursor {
            Some((t, next)) if t == tick => next,
            _ => 0,
        };
        self.cursor = Some((tick, seq + 1));
        let entry = JournalEntry {
            tick,
            seq,
            unit,
            kind,
        };
        if let Some(sink) = &mut self.sink {
            sink.write(&entry);
        }
        while self.entries.len() >= self.capacity.max(1) {
            self.entries.pop_front();
        }
        if self.capacity > 0 {
            self.entries.push_back(entry);
        }
    }

    /// Drops every buffered entry and restarts the per-tick seq (the sink,
    /// `enabled`, `verbose` and `capacity` are kept). Used by snapshot loads.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cursor = None;
    }

    /// Flushes the sink, if any.
    pub fn flush(&mut self) {
        if let Some(sink) = &mut self.sink {
            sink.flush();
        }
    }
}

/// Turns recording on: `app.add_plugins(JournalPlugin::default())`.
/// Works before or after [`NpcKernelPlugin`](crate::NpcKernelPlugin).
pub struct JournalPlugin {
    /// Also record condition evaluations.
    pub verbose: bool,
    /// Ring-buffer size.
    pub capacity: usize,
}

impl Default for JournalPlugin {
    fn default() -> Self {
        Self {
            verbose: false,
            capacity: DEFAULT_CAPACITY,
        }
    }
}

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        let world = app.world_mut();
        set_journal(world, true, self.verbose);
        world.resource_mut::<Journal>().capacity = self.capacity;
    }
}

/// Enables or disables the journal (and the `AiTrace` feeding it) at runtime,
/// inserting both resources if missing. Entries already recorded are kept.
pub fn set_journal(world: &mut World, enabled: bool, verbose: bool) {
    {
        let mut journal = world.get_resource_or_init::<Journal>();
        journal.enabled = enabled;
        journal.verbose = verbose;
    }
    let mut trace = world.get_resource_or_init::<AiTrace>();
    trace.enabled = enabled;
    trace.verbose = enabled && verbose;
    if !enabled {
        trace.events.clear();
    }
}

/// True when the journal exists and is enabled.
pub fn is_recording(world: &World) -> bool {
    world.get_resource::<Journal>().is_some_and(|j| j.enabled)
}

fn current_tick(world: &World) -> u64 {
    world.get_resource::<SimClock>().map_or(0, |c| c.tick)
}

/// Records one entry at the current tick when the journal is on; `kind` is
/// only built then. Pending `AiTrace` events are drained first, so an entry
/// recorded from inside an action's start/stop lands after the AI events
/// that led to it.
pub fn record(world: &mut World, unit: Option<SimId>, kind: impl FnOnce() -> JournalKind) {
    if !is_recording(world) {
        return;
    }
    drain_ai_trace(world);
    let tick = current_tick(world);
    world.resource_mut::<Journal>().push(tick, unit, kind());
}

impl JournalKind {
    /// An [`JournalKind::OrderFailed`] without a blocked-walk report.
    pub fn order_failed(order: impl Into<String>, reason: impl Into<String>) -> Self {
        JournalKind::OrderFailed {
            order: order.into(),
            reason: reason.into(),
            nearest_reachable: None,
            blocker: None,
        }
    }

    /// An applied content [`JournalKind::Effect`] named `kind`: the JSON
    /// object `{"type": kind, ..payload's fields}` (field order kept). A
    /// payload that is not an object is stored as `"value"` (`()` / `None`:
    /// no field). Use it with `Journal::push` in a system that holds
    /// `ResMut<Journal>`; from `&mut World` use [`record_effect`].
    pub fn effect(kind: &str, payload: &impl Serialize) -> JournalKind {
        JournalKind::Effect {
            effect: effect_object(kind, payload),
            applied: true,
        }
    }
}

fn effect_object(kind: &str, payload: &impl Serialize) -> JsonObject {
    let mut fields = vec![("type".to_owned(), serde_json::Value::from(kind))];
    match JsonObject::from_typed(payload) {
        Ok(obj) => fields.extend(obj.0.into_iter().filter(|(k, _)| k != "type")),
        Err(_) => match serde_json::to_value(payload) {
            Ok(serde_json::Value::Null) | Err(_) => {}
            Ok(v) => fields.push(("value".to_owned(), v)),
        },
    }
    JsonObject(fields)
}

/// Journals a content effect of `unit` (its source): an applied
/// `Effect { effect: {"type": kind, ..payload}, applied: true }`
/// ([`JournalKind::effect`]), built only while the journal records. The
/// typed one-liner for content (`npcctl journal --kind Effect` shows it):
///
/// ```ignore
/// #[derive(Serialize)]
/// struct WallRepaired { wall: u32, hp: u32 }
/// record_effect(world, Some(engineer), "WallRepaired", &WallRepaired { wall: 12, hp: 1000 });
/// ```
pub fn record_effect(world: &mut World, unit: Option<SimId>, kind: &str, payload: &impl Serialize) {
    record(world, unit, || JournalKind::effect(kind, payload));
}

/// [`record`] for component hooks, which only get a `DeferredWorld` and so
/// cannot drain the `AiTrace` first. Sim hooks (the `SimId` despawn/discard
/// hooks) fire from commands and despawns, outside `think`'s decisions.
pub fn record_deferred(
    world: &mut DeferredWorld,
    unit: Option<SimId>,
    kind: impl FnOnce() -> JournalKind,
) {
    if !world.get_resource::<Journal>().is_some_and(|j| j.enabled) {
        return;
    }
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    if let Some(mut journal) = world.get_resource_mut::<Journal>() {
        journal.push(tick, unit, kind());
    }
}

/// Moves pending `AiTrace` events into the journal (or discards them when
/// the journal is off), resolving agents to `SimId`s and node indices to names.
pub fn drain_ai_trace(world: &mut World) {
    let Some(mut trace) = world.get_resource_mut::<AiTrace>() else {
        return;
    };
    if trace.events.is_empty() {
        return;
    }
    let events = std::mem::take(&mut trace.events);
    if !is_recording(world) {
        return;
    }
    let _span = info_span!("mechanic", name = "journal").entered();
    let tick = current_tick(world);
    let registry = world.get_resource::<AiRegistry>();
    let resolved: Vec<(Option<SimId>, JournalKind)> = events
        .into_iter()
        .map(|ev| {
            let unit = world.get::<SimId>(ev.agent).copied();
            let tree = registry
                .zip(world.get::<Brain>(ev.agent))
                .map(|(r, b)| r.tree(b.tree));
            (unit, resolve(registry, tree, ev.kind))
        })
        .collect();
    let mut journal = world.resource_mut::<Journal>();
    for (unit, kind) in resolved {
        journal.push(tick, unit, kind);
    }
}

fn resolve(
    registry: Option<&AiRegistry>,
    tree: Option<&TreeDef>,
    kind: AiTraceKind,
) -> JournalKind {
    let node = |n: NodeIdx| node_name(tree, n);
    let path = |n: NodeIdx| match tree {
        Some(t) => t.path_string(&node_path(t, n)),
        None => node_name(None, n),
    };
    let action = |a: npc_ai::ActionId| {
        registry
            .and_then(|r| r.actions.get(a.0 as usize))
            .map_or_else(|| format!("#{}", a.0), |f| f.name.to_owned())
    };
    match kind {
        AiTraceKind::ThinkStart { reason } => JournalKind::ThinkStart { reason },
        AiTraceKind::LeafStart {
            node: n,
            action: a,
            status,
        } => JournalKind::LeafStart {
            node: node(n),
            path: path(n),
            action: action(a),
            status,
        },
        AiTraceKind::LeafStop {
            node: n,
            action: a,
            reason,
        } => JournalKind::LeafStop {
            node: node(n),
            path: path(n),
            action: action(a),
            reason,
        },
        AiTraceKind::Abort { guard, aborted } => JournalKind::Abort {
            guard: node(guard),
            aborted: aborted.into_iter().map(node).collect(),
        },
        AiTraceKind::CondEval {
            node: n,
            cond,
            result,
        } => JournalKind::CondEval {
            node: node(n),
            cond: registry
                .and_then(|r| r.conds.get(cond.0 as usize))
                .map_or_else(|| format!("#{}", cond.0), |c| c.0.to_owned()),
            result,
        },
        AiTraceKind::RootDone { status } => JournalKind::RootDone { status },
    }
}

fn node_name(tree: Option<&TreeDef>, n: NodeIdx) -> String {
    tree.and_then(|t| t.nodes.get(n as usize))
        .map_or_else(|| format!("#{n}"), |node| node.name.clone())
}

/// Node indices from the root to `target` (pre-order: a subtree is the
/// contiguous index range `c..subtree_end(c)`).
pub fn node_path(tree: &TreeDef, target: NodeIdx) -> Vec<NodeIdx> {
    let mut path = vec![0];
    let mut n: NodeIdx = 0;
    while n != target {
        let next = tree.nodes.get(n as usize).and_then(|node| {
            node.children
                .iter()
                .copied()
                .find(|&c| c <= target && (target as usize) < tree.subtree_end(c))
        });
        match next {
            Some(c) => {
                path.push(c);
                n = c;
            }
            None => break,
        }
    }
    path
}

/// `NpcSet::Cleanup`: flushes the journal sink once per tick.
pub fn flush_journal_system(journal: Option<ResMut<Journal>>) {
    if let Some(mut journal) = journal
        && journal.sink.is_some()
    {
        journal.flush();
    }
}

/// Shared journal filter (CLI, remote interface, F3 panel). Every set field
/// must match; `Default` matches everything. JSON fields are all optional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JournalFilter {
    /// Only entries about this unit.
    pub unit: Option<SimId>,
    /// Only these kinds (variant names, case-insensitive); empty = all.
    pub kinds: Vec<String>,
    /// First tick, inclusive.
    pub from: Option<u64>,
    /// Last tick, inclusive.
    pub to: Option<u64>,
    /// Case-sensitive substring of the entry's JSON line.
    pub grep: Option<String>,
    /// Keep only the most recent `limit` matches.
    pub limit: Option<usize>,
}

impl JournalFilter {
    /// Whether `entry` passes every criterion except `limit`.
    pub fn matches(&self, entry: &JournalEntry) -> bool {
        if self.unit.is_some() && entry.unit != self.unit {
            return false;
        }
        if self.from.is_some_and(|from| entry.tick < from)
            || self.to.is_some_and(|to| entry.tick > to)
        {
            return false;
        }
        if !self.kinds.is_empty() {
            let name = entry.kind.name();
            if !self.kinds.iter().any(|k| k.eq_ignore_ascii_case(name)) {
                return false;
            }
        }
        match &self.grep {
            Some(text) => {
                serde_json::to_string(entry).is_ok_and(|line| line.contains(text.as_str()))
            }
            None => true,
        }
    }
}

/// Matching entries in `(tick, seq)` order; with `limit`, the most recent ones.
pub fn query(journal: &Journal, filter: &JournalFilter) -> Vec<JournalEntry> {
    let limit = filter.limit.unwrap_or(usize::MAX);
    let mut hits: Vec<JournalEntry> = journal
        .entries
        .iter()
        .rev()
        .filter(|e| filter.matches(e))
        .take(limit)
        .cloned()
        .collect();
    hits.reverse();
    hits
}

#[cfg(test)]
mod names_tests {
    use super::*;

    /// One value of every variant. The match in `variant_count` has no
    /// wildcard, so a new variant fails to compile until it is listed here
    /// and in `JournalKind::NAMES`.
    fn samples() -> Vec<JournalKind> {
        vec![
            JournalKind::ThinkStart {
                reason: ThinkReason::Request,
            },
            JournalKind::LeafStart {
                node: String::new(),
                path: String::new(),
                action: String::new(),
                status: Status::Running,
            },
            JournalKind::LeafStop {
                node: String::new(),
                path: String::new(),
                action: String::new(),
                reason: StopReason::Finished,
            },
            JournalKind::Abort {
                guard: String::new(),
                aborted: Vec::new(),
            },
            JournalKind::CondEval {
                node: String::new(),
                cond: String::new(),
                result: true,
            },
            JournalKind::RootDone {
                status: Status::Success,
            },
            JournalKind::NeedLevel {
                need: String::new(),
                from: NeedLevel::Ok,
                to: NeedLevel::Low,
            },
            JournalKind::Reserve {
                target: SimId(1),
                ok: true,
            },
            JournalKind::Release {
                target: SimId(1),
                reason: ReleaseReason::Finished,
            },
            JournalKind::Effect {
                effect: JsonObject::default(),
                applied: true,
            },
            JournalKind::Command {
                cmd: SimCommand::Despawn { unit: 1 },
            },
            JournalKind::PhaseChange { night: true },
            JournalKind::Spawn {
                kind: String::new(),
            },
            JournalKind::Despawn,
            JournalKind::PathFound { len: 1, nodes: 1 },
            JournalKind::PathFailed {
                reason: crate::spatial::PathFailReason::Unreachable,
            },
            JournalKind::Roll {
                skill: String::new(),
                target: 10,
                roll: 10,
                margin: 0,
                success: true,
                crit: Critical::None,
                purpose: String::new(),
            },
            JournalKind::Perceived {
                target: SimId(2),
                via: String::new(),
            },
            JournalKind::Lost {
                target: SimId(2),
                forgotten: false,
            },
            JournalKind::ControllerAttached {
                controller: String::new(),
                mode: String::new(),
                on_idle: String::new(),
            },
            JournalKind::ControllerDetached {
                controller: String::new(),
            },
            JournalKind::ExternalAction {
                handle: 1,
                verb: String::new(),
                status: String::new(),
                latency_ticks: None,
            },
            JournalKind::ActionRejected {
                controller: String::new(),
                verb: String::new(),
                reason: String::new(),
            },
            JournalKind::Interacted {
                verb: String::new(),
                target: SimId(1),
                item: None,
                outcome: String::new(),
                reason: None,
                dropped: vec![DroppedItem {
                    item: SimId(2),
                    kind: String::new(),
                    stack: 1,
                    pos: [0, 0],
                }],
            },
            JournalKind::ItemMoved {
                item: SimId(1),
                kind: String::new(),
                stack: 1,
                from: None,
                to: Some(SimId(2)),
                slot: None,
            },
            JournalKind::DoorChanged {
                door: SimId(1),
                open: true,
                locked: false,
            },
            JournalKind::Mounted {
                vehicle: SimId(1),
                seat: 0,
                role: String::new(),
            },
            JournalKind::Dismounted {
                vehicle: SimId(1),
                seat: 0,
                at: crate::math::SimPos::new(0, 0),
            },
            JournalKind::VehicleStalled {
                reason: String::new(),
            },
            JournalKind::XpGained {
                source: String::new(),
                amount: 0,
                xp: 0,
            },
            JournalKind::LevelUp {
                level: 1,
                points: 0,
            },
            JournalKind::SkillImproved {
                skill: String::new(),
                points: 1,
                level: 0,
                via: String::new(),
            },
            JournalKind::CraftStarted {
                recipe: String::new(),
                workstation: None,
                ends: 0,
            },
            JournalKind::CraftDone {
                recipe: String::new(),
                workstation: None,
                margin: 0,
                quality: 0,
                outputs: Vec::new(),
            },
            JournalKind::CraftFailed {
                recipe: String::new(),
                workstation: None,
                outcome: String::new(),
                reason: String::new(),
                consumed: false,
            },
            JournalKind::OrderIssued {
                order: String::new(),
                queued: false,
                player: 0,
            },
            JournalKind::OrderStarted {
                order: String::new(),
            },
            JournalKind::OrderDone {
                order: String::new(),
            },
            JournalKind::OrderFailed {
                order: String::new(),
                reason: String::new(),
                nearest_reachable: Some([0, 0]),
                blocker: Some(crate::spatial::Blocker::OutOfBounds),
            },
        ]
    }

    fn variant_index(kind: &JournalKind) -> usize {
        match kind {
            JournalKind::ThinkStart { .. } => 0,
            JournalKind::LeafStart { .. } => 1,
            JournalKind::LeafStop { .. } => 2,
            JournalKind::Abort { .. } => 3,
            JournalKind::CondEval { .. } => 4,
            JournalKind::RootDone { .. } => 5,
            JournalKind::NeedLevel { .. } => 6,
            JournalKind::Reserve { .. } => 7,
            JournalKind::Release { .. } => 8,
            JournalKind::Effect { .. } => 9,
            JournalKind::Command { .. } => 10,
            JournalKind::PhaseChange { .. } => 11,
            JournalKind::Spawn { .. } => 12,
            JournalKind::Despawn => 13,
            JournalKind::PathFound { .. } => 14,
            JournalKind::PathFailed { .. } => 15,
            JournalKind::Roll { .. } => 16,
            JournalKind::Perceived { .. } => 17,
            JournalKind::Lost { .. } => 18,
            JournalKind::ControllerAttached { .. } => 19,
            JournalKind::ControllerDetached { .. } => 20,
            JournalKind::ExternalAction { .. } => 21,
            JournalKind::ActionRejected { .. } => 22,
            JournalKind::Interacted { .. } => 23,
            JournalKind::ItemMoved { .. } => 24,
            JournalKind::DoorChanged { .. } => 25,
            JournalKind::Mounted { .. } => 26,
            JournalKind::Dismounted { .. } => 27,
            JournalKind::VehicleStalled { .. } => 28,
            JournalKind::XpGained { .. } => 29,
            JournalKind::LevelUp { .. } => 30,
            JournalKind::SkillImproved { .. } => 31,
            JournalKind::CraftStarted { .. } => 32,
            JournalKind::CraftDone { .. } => 33,
            JournalKind::CraftFailed { .. } => 34,
            JournalKind::OrderIssued { .. } => 35,
            JournalKind::OrderStarted { .. } => 36,
            JournalKind::OrderDone { .. } => 37,
            JournalKind::OrderFailed { .. } => 38,
        }
    }

    #[test]
    fn typed_effects_keep_field_order() {
        #[derive(Serialize)]
        struct Repaired {
            wall: u32,
            by: u32,
        }
        let kind = JournalKind::effect("WallRepaired", &Repaired { wall: 12, by: 3 });
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            r#"{"type":"Effect","effect":{"type":"WallRepaired","wall":12,"by":3},"applied":true}"#
        );
        let bare = JournalKind::effect("Ping", &());
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            r#"{"type":"Effect","effect":{"type":"Ping"},"applied":true}"#
        );
        let mut world = World::new();
        set_journal(&mut world, true, false);
        record_effect(&mut world, Some(SimId(4)), "Count", &7);
        let journal = world.resource::<Journal>();
        let e = journal.entries.back().expect("recorded");
        assert_eq!(e.unit, Some(SimId(4)));
        assert_eq!(
            serde_json::to_string(&e.kind).unwrap(),
            r#"{"type":"Effect","effect":{"type":"Count","value":7},"applied":true}"#
        );
    }

    #[test]
    fn names_cover_every_variant() {
        let samples = samples();
        assert_eq!(samples.len(), JournalKind::NAMES.len());
        for (i, kind) in samples.iter().enumerate() {
            assert_eq!(variant_index(kind), i);
            assert_eq!(kind.name(), JournalKind::NAMES[i], "declaration order");
            // The JSON tag is the name.
            let json = serde_json::to_value(kind).expect("serializes");
            assert_eq!(json["type"], kind.name());
        }
        let unique: std::collections::BTreeSet<_> = JournalKind::NAMES.iter().collect();
        assert_eq!(unique.len(), JournalKind::NAMES.len(), "no duplicates");
    }

    #[test]
    fn parse_name_is_case_insensitive() {
        assert_eq!(JournalKind::parse_name("leafstart"), Some("LeafStart"));
        assert_eq!(JournalKind::parse_name("DESPAWN"), Some("Despawn"));
        assert_eq!(JournalKind::parse_name("Nope"), None);
    }
}

#[cfg(test)]
mod from_build_tests {
    use super::*;

    #[test]
    fn journal_from_build_is_scoped() {
        assert!(!Journal::default().enabled, "off by default");
        let (on, verbose) = with_journal_from_build(true, || {
            let j = Journal::default();
            (j.enabled, j.verbose)
        });
        assert!(on && verbose);
        assert!(!Journal::default().enabled, "restored after the scope");
        // Restored even when the build panics.
        let caught = std::panic::catch_unwind(|| with_journal_from_build(false, || panic!("x")));
        assert!(caught.is_err());
        assert!(!Journal::default().enabled);
    }

    #[test]
    fn spawns_during_build_are_journaled() {
        let mut app = App::new();
        with_journal_from_build(false, || {
            app.add_plugins(crate::NpcKernelPlugin::default());
            let world = app.world_mut();
            crate::ids::spawn_sim(world, ());
            crate::ids::spawn_sim(world, ());
        });
        let journal = app.world().resource::<Journal>();
        let entries: Vec<_> = journal.entries.iter().cloned().collect();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert!(
            entries
                .iter()
                .all(|e| e.tick == 0 && e.kind.name() == "Spawn")
        );
        assert_eq!((entries[0].seq, entries[1].seq), (0, 1));
    }
}
