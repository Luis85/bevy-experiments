//! Controllers, the per-unit action queue and the injected `ExternalControl`
//! branch (spec §3.1). Everything here is sim state: it changes only through
//! the recorded commands below, so a replay reproduces an agent session
//! without the agent.
//!
//! - [`ExternalController`] `{ id, mode, on_idle }` is put on a unit by the
//!   [`AttachController`] command and removed by [`DetachController`].
//! - In `Direct` mode the unit carries an [`ExternalQueue`]. Every tree gets a
//!   reserved top-priority branch at freeze time ([`inject_external_control`]):
//!
//!   ```text
//!   Selector "ExternalControlRoot"
//!   ├ Guard "ExternalControl" [external_control, LowerPriority] → Leaf "ExternalAction"
//!   └ <the tree's own root>
//!   ```
//!
//!   `external_control` holds while the unit is in `Direct` mode and either
//!   its idle policy is `Idle` or it has queued work (`Autopilot`), so the
//!   guard suspends the unit's own behaviour and runs the `ExternalAction`
//!   leaf instead. That leaf works through the queue: each queued action is a
//!   list of [`Step`]s, mostly registered behaviour-tree actions run by name
//!   (their own systems advance them, exactly as when the tree runs them).
//! - [`external_action_system`] runs twice per tick ([`AgentIoSet`]): after
//!   `NpcSet::Inputs` (so an action applied by a command starts in the same
//!   tick, and a detach hands control back before `Think`) and after
//!   `NpcSet::Effects` (to chain the next step as soon as one completes).
//! - Advisor mode injects nothing: actions become ordinary orders
//!   (`SimCommand::Order`) the unit's own tree follows.

use std::collections::VecDeque;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::SystemSet;
use npc_ai::{
    Abort, ActiveLeaf, AiRegistry, BbKey, BbValue, Blackboard, Brain, NameRef, NodeDef, NodeIdx,
    NodeKind, ParamValue, Status, StopReason, ThinkRequest, TreeDef,
};
use npc_kernel::commands::{CommandError, ContentCommand, entity};
use npc_kernel::hash::{Fnv64, SimHash};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::{NpcSet, SimCommand, SimId};
use serde::{Deserialize, Serialize};

/// Name of the reserved action that runs the external queue.
pub const EXTERNAL_ACTION: &str = "ExternalAction";
/// Name of the reserved condition guarding it.
pub const EXTERNAL_CONTROL_COND: &str = "external_control";
/// Node names of the injected branch.
pub const ROOT_NODE: &str = "ExternalControlRoot";
pub const GUARD_NODE: &str = "ExternalControl";
pub const LEAF_NODE: &str = "ExternalAction";

/// Most steps one queued action may have.
pub const MAX_STEPS: usize = 32;
/// Most actions one unit's queue may hold.
pub const MAX_QUEUE: usize = 64;
/// Longest `Wait` step, in ticks.
pub const MAX_WAIT_TICKS: u32 = 1_000_000;

/// How a controller drives its unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControlMode {
    /// The unit's tree is suspended; the agent chooses every action.
    #[default]
    Direct,
    /// The tree keeps running; the agent gives orders, like an RTS player.
    Advisor,
    /// Read-only.
    Observer,
}

impl ControlMode {
    pub fn name(self) -> &'static str {
        match self {
            ControlMode::Direct => "Direct",
            ControlMode::Advisor => "Advisor",
            ControlMode::Observer => "Observer",
        }
    }
}

/// What a `Direct` unit does when its queue is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdlePolicy {
    /// Stand still until the next action arrives (the default).
    #[default]
    Idle,
    /// Fall back to the unit's own tree until the next action arrives.
    Autopilot,
}

impl IdlePolicy {
    pub fn name(self) -> &'static str {
        match self {
            IdlePolicy::Idle => "Idle",
            IdlePolicy::Autopilot => "Autopilot",
        }
    }
}

/// The external controller attached to a unit (hashed sim state).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalController {
    /// The controller's id; mutating calls must name it (owner check).
    pub id: String,
    pub mode: ControlMode,
    pub on_idle: IdlePolicy,
}

impl SimHash for ExternalController {
    const NAME: &'static str = "external_controller";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.id);
        h.u8(self.mode as u8);
        h.u8(self.on_idle as u8);
    }
}

/// One step of a queued action.
/// JSON: `{"type":"Action","action":"MoveToDest","param":"None"}`,
/// `{"type":"SetBlackboard","key":1,"value":{"Pos":[4000,2000]}}`,
/// `{"type":"Wait","ticks":100}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Step {
    /// Run the registered behaviour-tree action `action` with `param` (its
    /// start/stop functions and its own system, as when the tree runs it).
    Action {
        action: String,
        #[serde(default)]
        param: ParamValue,
    },
    /// Write a blackboard slot (instant), e.g. the destination a following
    /// `MoveToDest`-style action reads.
    SetBlackboard { key: u8, value: BbValue },
    /// Stand still for `ticks` ticks.
    Wait { ticks: u32 },
}

fn hash_param(p: &ParamValue, h: &mut Fnv64) {
    match p {
        ParamValue::None => h.u8(0),
        ParamValue::Int(v) => {
            h.u8(1);
            h.i32(*v);
        }
        ParamValue::Pos(x, z) => {
            h.u8(2);
            h.i32(*x);
            h.i32(*z);
        }
        ParamValue::Name(n) => {
            h.u8(3);
            h.str(n);
        }
    }
}

fn hash_bb(v: &BbValue, h: &mut Fnv64) {
    match v {
        BbValue::None => h.u8(0),
        BbValue::U32(x) => {
            h.u8(1);
            h.u32(*x);
        }
        BbValue::I32(x) => {
            h.u8(2);
            h.i32(*x);
        }
        BbValue::Pos(x, z) => {
            h.u8(3);
            h.i32(*x);
            h.i32(*z);
        }
    }
}

impl Step {
    fn sim_hash(&self, h: &mut Fnv64) {
        match self {
            Step::Action { action, param } => {
                h.u8(0);
                h.str(action);
                hash_param(param, h);
            }
            Step::SetBlackboard { key, value } => {
                h.u8(1);
                h.u8(*key);
                hash_bb(value, h);
            }
            Step::Wait { ticks } => {
                h.u8(2);
                h.u32(*ticks);
            }
        }
    }
}

fn len_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// An action waiting in (or running from) a unit's queue.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueuedAction {
    /// Handle returned by `agent/act` (unique per session).
    pub handle: u32,
    pub verb: String,
    pub label: String,
    pub steps: Vec<Step>,
}

impl QueuedAction {
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.handle);
        h.str(&self.verb);
        h.str(&self.label);
        h.u32(len_u32(self.steps.len()));
        for s in &self.steps {
            s.sim_hash(h);
        }
    }
}

/// The action at the head of the queue.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunningAction {
    pub item: QueuedAction,
    /// Index of the current step.
    pub step: u32,
    /// The current step was started and has not finished yet.
    pub step_running: bool,
    /// Ticks left of a running `Wait` step.
    pub wait_left: u32,
}

/// A `Direct` unit's action queue (hashed sim state).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalQueue {
    pub pending: VecDeque<QueuedAction>,
    pub current: Option<RunningAction>,
    /// Set by a cancel/clear/replace command; the current action is
    /// cancelled by the next [`external_action_system`] pass.
    pub cancel_current: bool,
}

impl SimHash for ExternalQueue {
    const NAME: &'static str = "external_queue";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len_u32(self.pending.len()));
        for item in &self.pending {
            item.sim_hash(h);
        }
        h.opt(self.current.as_ref(), |h, cur| {
            cur.item.sim_hash(h);
            h.u32(cur.step);
            h.u8(u8::from(cur.step_running));
            h.u32(cur.wait_left);
        });
        h.u8(u8::from(self.cancel_current));
    }
}

impl ExternalQueue {
    /// Whether the unit has anything to do.
    pub fn busy(&self) -> bool {
        self.current.is_some() || !self.pending.is_empty()
    }
}

/// The two passes of [`external_action_system`] in `NpcSimTick`: `Dispatch`
/// between `NpcSet::Inputs` and `NpcSet::Needs`, `Advance` between
/// `NpcSet::Effects` and `NpcSet::Cleanup`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentIoSet {
    Dispatch,
    Advance,
}

/// Configures [`AgentIoSet`] in `NpcSimTick`.
pub fn configure_sets(app: &mut bevy_app::App) {
    app.configure_sets(
        npc_kernel::NpcSimTick,
        (
            AgentIoSet::Dispatch
                .after(NpcSet::Inputs)
                .before(NpcSet::Needs),
            AgentIoSet::Advance
                .after(NpcSet::Effects)
                .before(NpcSet::Cleanup),
        ),
    );
}

// ---------------------------------------------------------------- tree

/// Tree transform (registered as `npc_agent_io.external_control`): wraps
/// `tree`'s root in `Selector "ExternalControlRoot"` whose first child is the
/// reserved guard (see the module docs). Node indices shift by 3; pre-order
/// is kept. Idempotent.
///
/// # Panics
/// When the tree would exceed `NodeIdx::MAX` nodes.
pub fn inject_external_control(tree: &mut TreeDef) {
    if tree.nodes.first().is_some_and(|n| n.name == ROOT_NODE) {
        return;
    }
    const SHIFT: NodeIdx = 3;
    assert!(
        tree.nodes.len() + usize::from(SHIFT) <= usize::from(NodeIdx::MAX),
        "tree `{}` is too large for the ExternalControl branch",
        tree.name
    );
    let mut nodes = Vec::with_capacity(tree.nodes.len() + 3);
    nodes.push(NodeDef {
        name: ROOT_NODE.to_owned(),
        kind: NodeKind::Selector,
        children: vec![1, SHIFT],
    });
    nodes.push(NodeDef {
        name: GUARD_NODE.to_owned(),
        kind: NodeKind::Guard {
            cond: NameRef::new(EXTERNAL_CONTROL_COND),
            abort: Abort::LowerPriority,
        },
        children: vec![2],
    });
    nodes.push(NodeDef {
        name: LEAF_NODE.to_owned(),
        kind: NodeKind::Leaf {
            action: NameRef::new(EXTERNAL_ACTION),
            param: ParamValue::None,
        },
        children: Vec::new(),
    });
    for mut node in tree.nodes.drain(..) {
        for c in &mut node.children {
            *c += SHIFT;
        }
        nodes.push(node);
    }
    tree.nodes = nodes;
}

/// Condition `external_control`: `Direct` mode and either `Idle` policy or
/// queued work. Implies its child can start: `ExternalAction` then always
/// returns `Running` (idle or working) unless every queued action fails at
/// once.
pub fn cond_external_control(world: &World, entity: Entity) -> bool {
    let Some(c) = world.get::<ExternalController>(entity) else {
        return false;
    };
    if c.mode != ControlMode::Direct {
        return false;
    }
    match c.on_idle {
        IdlePolicy::Idle => true,
        IdlePolicy::Autopilot => world
            .get::<ExternalQueue>(entity)
            .is_some_and(ExternalQueue::busy),
    }
}

fn is_direct(world: &World, agent: Entity) -> Option<IdlePolicy> {
    world
        .get::<ExternalController>(agent)
        .filter(|c| c.mode == ControlMode::Direct)
        .map(|c| c.on_idle)
}

/// `ExternalAction` start: works through the queue; `Running` while working
/// or idling (`Idle` policy), `Failure` when there is nothing to do under
/// `Autopilot` (so the selector falls through to the unit's own tree).
pub fn external_action_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(policy) = is_direct(world, agent) else {
        return Status::Failure;
    };
    let active = advance(world, agent);
    if active || policy == IdlePolicy::Idle {
        Status::Running
    } else {
        Status::Failure
    }
}

/// `ExternalAction` stop: a step still running is aborted (the branch was
/// interrupted, or control was handed back), journaled `cancelled`.
///
/// Exception: a step whose own system finished it *before* the tree's think
/// (e.g. `ExecuteOrder`, finished by the order engine when the order queue
/// runs dry, between `NpcSet::Sense` and `NpcSet::Think`) makes the tree stop
/// the leaf with `Finished`; that step completed, so the action goes on (or
/// ends `done` / `failed`) instead of being cancelled.
pub fn external_action_stop(world: &mut World, agent: Entity, reason: StopReason) {
    if reason == StopReason::Finished && finish_completed_step(world, agent) {
        return;
    }
    abort_current(world, agent, "cancelled");
}

/// See [`external_action_stop`]: the leaf's status says the running
/// behaviour step completed. Returns whether it handled the stop.
fn finish_completed_step(world: &mut World, agent: Entity) -> bool {
    let Some(ext) = world.get_resource::<ExternalActionId>().map(|i| i.0) else {
        return false;
    };
    let Some(leaf) = world
        .get::<ActiveLeaf>(agent)
        .copied()
        .filter(|l| l.action == ext && l.status != Status::Running)
    else {
        return false;
    };
    let Some(cur) = world
        .get::<ExternalQueue>(agent)
        .and_then(|q| q.current.clone())
        .filter(|c| c.step_running)
    else {
        return false;
    };
    let Some(Step::Action { action, .. }) = cur.item.steps.get(cur.step as usize) else {
        return false;
    };
    if let Some(fns) = action_fns(world, action) {
        (fns.stop)(world, agent, StopReason::Finished);
    }
    if leaf.status == Status::Success {
        with_current(world, agent, |c| {
            c.step += 1;
            c.step_running = false;
        });
        if cur.step as usize + 1 >= cur.item.steps.len() {
            finish_current(world, agent, "done");
        }
    } else {
        finish_current(world, agent, "failed");
    }
    true
}

// ---------------------------------------------------------------- queue work

fn sim_id(world: &World, agent: Entity) -> Option<SimId> {
    world.get::<SimId>(agent).copied()
}

/// Journals an `ExternalAction` state change.
pub fn journal_action(
    world: &mut World,
    agent: Entity,
    item: &QueuedAction,
    status: &str,
    latency_ticks: Option<u64>,
) {
    let unit = sim_id(world, agent);
    let (handle, verb) = (item.handle, item.verb.clone());
    journal::record(world, unit, || JournalKind::ExternalAction {
        handle,
        verb,
        status: status.to_owned(),
        latency_ticks,
    });
}

fn action_fns(world: &World, name: &str) -> Option<npc_ai::ActionFns> {
    let registry = world.get_resource::<AiRegistry>()?;
    registry
        .actions
        .iter()
        .find(|a| a.name == name && a.name != EXTERNAL_ACTION)
        .copied()
}

/// Cancels the current action: a running behaviour step is stopped with
/// `Aborted` (it releases what it claimed). Journals `status` for it.
pub fn abort_current(world: &mut World, agent: Entity, status: &str) {
    let Some(cur) = world
        .get_mut::<ExternalQueue>(agent)
        .and_then(|mut q| q.current.take())
    else {
        return;
    };
    if cur.step_running
        && let Some(Step::Action { action, .. }) = cur.item.steps.get(cur.step as usize)
        && let Some(fns) = action_fns(world, action)
    {
        (fns.stop)(world, agent, StopReason::Aborted);
    }
    journal_action(world, agent, &cur.item, status, None);
}

/// Cancels the current action and every pending one.
pub fn cancel_all(world: &mut World, agent: Entity) {
    abort_current(world, agent, "cancelled");
    let pending: Vec<QueuedAction> = world
        .get_mut::<ExternalQueue>(agent)
        .map(|mut q| q.pending.drain(..).collect())
        .unwrap_or_default();
    for item in &pending {
        journal_action(world, agent, item, "cancelled", None);
    }
    if let Some(mut q) = world.get_mut::<ExternalQueue>(agent) {
        q.cancel_current = false;
    }
}

/// What the current step needs next.
enum Next {
    /// Nothing queued.
    Empty,
    /// Pop the next pending action.
    Pop,
    /// The current action has no steps left.
    Done,
    /// Run this step (not started yet).
    Start(Step),
    /// The step is running (behaviour step or wait).
    Running,
}

fn next(world: &World, agent: Entity) -> Next {
    let Some(q) = world.get::<ExternalQueue>(agent) else {
        return Next::Empty;
    };
    match &q.current {
        None if q.pending.is_empty() => Next::Empty,
        None => Next::Pop,
        Some(cur) => match cur.item.steps.get(cur.step as usize) {
            None => Next::Done,
            Some(_) if cur.step_running => Next::Running,
            Some(step) => Next::Start(step.clone()),
        },
    }
}

fn with_current(world: &mut World, agent: Entity, f: impl FnOnce(&mut RunningAction)) {
    if let Some(mut q) = world.get_mut::<ExternalQueue>(agent)
        && let Some(cur) = q.current.as_mut()
    {
        f(cur);
    }
}

fn finish_current(world: &mut World, agent: Entity, status: &str) {
    let done = world
        .get_mut::<ExternalQueue>(agent)
        .and_then(|mut q| q.current.take());
    if let Some(cur) = done {
        journal_action(world, agent, &cur.item, status, None);
    }
}

/// Works through the queue until a step is running or the queue is empty.
/// Returns whether something is running.
pub fn advance(world: &mut World, agent: Entity) -> bool {
    // Every iteration either finishes a step or an action, so this bound
    // (all steps of a full queue, plus pops) is never reached in practice.
    for _ in 0..(MAX_QUEUE * (MAX_STEPS + 2) + 2) {
        match next(world, agent) {
            Next::Empty => return false,
            Next::Running => return true,
            Next::Pop => {
                let popped = world.get_mut::<ExternalQueue>(agent).and_then(|mut q| {
                    let item = q.pending.pop_front()?;
                    q.current = Some(RunningAction {
                        item: item.clone(),
                        step: 0,
                        step_running: false,
                        wait_left: 0,
                    });
                    Some(item)
                });
                if let Some(item) = popped {
                    journal_action(world, agent, &item, "started", None);
                }
            }
            Next::Done => finish_current(world, agent, "done"),
            Next::Start(step) => match step {
                Step::SetBlackboard { key, value } => {
                    if let Some(mut bb) = world.get_mut::<Blackboard>(agent)
                        && usize::from(key) < bb.slots.len()
                    {
                        bb.set(BbKey(key), value);
                    }
                    with_current(world, agent, |c| c.step += 1);
                }
                Step::Wait { ticks } => {
                    if ticks == 0 {
                        with_current(world, agent, |c| c.step += 1);
                    } else {
                        with_current(world, agent, |c| {
                            c.step_running = true;
                            c.wait_left = ticks;
                        });
                    }
                }
                Step::Action { action, param } => {
                    let Some(fns) = action_fns(world, &action) else {
                        finish_current(world, agent, "failed");
                        continue;
                    };
                    match (fns.start)(world, agent, &param) {
                        Status::Running => with_current(world, agent, |c| c.step_running = true),
                        Status::Success => with_current(world, agent, |c| c.step += 1),
                        Status::Failure => finish_current(world, agent, "failed"),
                    }
                }
            },
        }
    }
    false
}

/// Id of the `ExternalAction` action (frozen registries only).
#[derive(Resource, Clone, Copy, Debug)]
pub struct ExternalActionId(pub npc_ai::ActionId);

/// The `ExternalAction` id, cached in [`ExternalActionId`] once content is
/// frozen (`None` before the freeze).
pub fn external_action_id(world: &mut World) -> Option<npc_ai::ActionId> {
    if let Some(id) = world.get_resource::<ExternalActionId>() {
        return Some(id.0);
    }
    let registry = world.get_resource::<AiRegistry>()?;
    if !registry.is_frozen() {
        return None;
    }
    let id = registry.action_id(EXTERNAL_ACTION)?;
    world.insert_resource(ExternalActionId(id));
    Some(id)
}

/// Which pass of [`external_action_system`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Dispatch,
    Advance,
}

/// Units with an [`ExternalQueue`] (each pass's cached query).
pub type ExternalQueueQuery = QueryState<(&'static SimId, Entity), With<ExternalQueue>>;

/// The `ExternalAction` leaf's own system (both [`AgentIoSet`] passes), over
/// every unit with an [`ExternalQueue`] in `SimId` order:
/// - not (or no longer) `Direct`: the queue is cancelled and removed, and a
///   running `ExternalAction` leaf ends (`Failure`) so the tree takes over;
/// - a pending cancel stops the current action;
/// - a behaviour step that finished (its own system set the leaf's status)
///   is stopped with `Finished` and the next step starts; a `Wait` counts
///   down (in the `Advance` pass only, once per tick);
/// - the leaf stays `Running` while working or idling (`Idle` policy) and
///   ends with `Failure` when there is nothing to do under `Autopilot`.
pub fn external_action_system(world: &mut World, pass: Pass, queues: &mut ExternalQueueQuery) {
    if pass == Pass::Dispatch {
        journal_skipped_commands(world);
    }
    let Some(ext) = external_action_id(world) else {
        return;
    };
    let mut agents: Vec<(SimId, Entity)> = queues.iter(world).map(|(&id, e)| (id, e)).collect();
    agents.sort_unstable_by_key(|&(id, _)| id);
    for (_, agent) in agents {
        let leaf = world
            .get::<ActiveLeaf>(agent)
            .copied()
            .filter(|l| l.action == ext);
        let Some(policy) = is_direct(world, agent) else {
            cancel_all(world, agent);
            world.entity_mut(agent).remove::<ExternalQueue>();
            if let Some(l) = leaf
                && l.status == Status::Running
                && let Some(mut active) = world.get_mut::<ActiveLeaf>(agent)
            {
                active.status = Status::Failure;
            }
            continue;
        };
        let Some(leaf) = leaf else {
            // The tree has not picked the queue up yet (the guard does at
            // the next think); a cancel with nothing running is moot.
            let idle = world
                .get::<ExternalQueue>(agent)
                .is_some_and(|q| q.current.is_none());
            if idle && let Some(mut q) = world.get_mut::<ExternalQueue>(agent) {
                q.cancel_current = false;
            }
            continue;
        };
        if world
            .get::<ExternalQueue>(agent)
            .is_some_and(|q| q.cancel_current)
        {
            abort_current(world, agent, "cancelled");
            if let Some(mut q) = world.get_mut::<ExternalQueue>(agent) {
                q.cancel_current = false;
            }
        } else if leaf.status != Status::Running {
            // A behaviour step finished: its system wrote the leaf status.
            let cur = world
                .get::<ExternalQueue>(agent)
                .and_then(|q| q.current.clone());
            if let Some(cur) = cur
                && cur.step_running
                && let Some(Step::Action { action, .. }) = cur.item.steps.get(cur.step as usize)
            {
                if let Some(fns) = action_fns(world, action) {
                    (fns.stop)(world, agent, StopReason::Finished);
                }
                if leaf.status == Status::Success {
                    with_current(world, agent, |c| {
                        c.step += 1;
                        c.step_running = false;
                    });
                } else {
                    finish_current(world, agent, "failed");
                }
            }
        }
        if pass == Pass::Advance {
            let cur = world
                .get::<ExternalQueue>(agent)
                .and_then(|q| q.current.clone());
            if let Some(cur) = cur
                && cur.step_running
                && matches!(
                    cur.item.steps.get(cur.step as usize),
                    Some(Step::Wait { .. })
                )
            {
                with_current(world, agent, |c| {
                    c.wait_left = c.wait_left.saturating_sub(1);
                    if c.wait_left == 0 {
                        c.step += 1;
                        c.step_running = false;
                    }
                });
            }
        }
        let active = advance(world, agent);
        let status = if active || policy == IdlePolicy::Idle {
            Status::Running
        } else {
            Status::Failure
        };
        if let Some(mut l) = world.get_mut::<ActiveLeaf>(agent) {
            l.status = status;
        }
    }
}

/// How many entries of the kernel's `SkippedCommands` were journaled
/// (tooling, not hashed).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SkippedSeen(pub usize);

/// Agent commands accepted by `agent/*` but skipped when their tick came
/// (they no longer validated: a detach or forced attach landed first, the
/// unit vanished, ..) are journaled so the agent hears about them, and
/// waiting `agent/events` polls wake up:
/// - a skipped `AgentAct`: `ExternalAction {handle, verb, status:"rejected"}`
///   and `ActionRejected {controller, verb, reason:"skipped at tick T: .."}`;
/// - other agent commands (`AgentCancel`, `AgentClear`, `AttachController`,
///   `DetachController`) and any command for a controlled unit:
///   `ActionRejected {controller, verb: <command type>, reason}`.
///
/// Reads only the kernel's tooling record of skipped commands and writes
/// only the journal: sim state is untouched (`AgentIoSet::Dispatch`, right
/// after the commands applied).
pub fn journal_skipped_commands(world: &mut World) {
    use npc_kernel::commands::SkippedCommands;
    let Some(skipped) = world.get_resource::<SkippedCommands>() else {
        return;
    };
    let seen = world.get_resource::<SkippedSeen>().map_or(0, |s| s.0);
    let len = skipped.entries.len();
    if len == seen {
        return;
    }
    let fresh: Vec<_> = skipped.entries[seen.min(len)..].to_vec();
    world.insert_resource(SkippedSeen(len));
    for s in fresh {
        let reason = format!("skipped at tick {}: {}", s.tick, s.error);
        let kind = s.cmd.kind().to_owned();
        if let SimCommand::Content { payload, .. } = &s.cmd
            && kind == AgentAct::NAME
            && let Ok(act) = payload.to_typed::<AgentAct>()
        {
            let unit = Some(SimId(act.unit));
            let (handle, verb) = (act.handle, act.verb.clone());
            journal::record(world, unit, || JournalKind::ExternalAction {
                handle,
                verb,
                status: "rejected".to_owned(),
                latency_ticks: None,
            });
            journal::record(world, unit, || JournalKind::ActionRejected {
                controller: act.controller,
                verb: act.verb,
                reason,
            });
            continue;
        }
        let (unit, controller) = match &s.cmd {
            SimCommand::Content { payload, .. }
                if matches!(
                    kind.as_str(),
                    "AgentCancel" | "AgentClear" | "AttachController" | "DetachController"
                ) =>
            {
                let unit = payload
                    .get("unit")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|u| u32::try_from(u).ok());
                let controller = payload
                    .get("controller")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                (unit.map(SimId), controller)
            }
            cmd => {
                let unit = cmd.unit_in(world);
                let controller = unit
                    .and_then(|u| world.get_resource::<npc_kernel::SimIndex>()?.get(u))
                    .and_then(|e| world.get::<ExternalController>(e))
                    .map(|c| c.id.clone());
                match controller {
                    Some(c) => (unit, c),
                    None => continue,
                }
            }
        };
        journal::record(world, unit, || JournalKind::ActionRejected {
            controller,
            verb: kind,
            reason,
        });
    }
}

/// `AgentIoSet::Dispatch` pass.
pub fn external_dispatch_system(world: &mut World, queues: &mut ExternalQueueQuery) {
    external_action_system(world, Pass::Dispatch, queues);
}

/// `AgentIoSet::Advance` pass.
pub fn external_advance_system(world: &mut World, queues: &mut ExternalQueueQuery) {
    external_action_system(world, Pass::Advance, queues);
}

// ---------------------------------------------------------------- commands

fn unavailable(reason: impl Into<String>) -> CommandError {
    CommandError::Unavailable {
        reason: reason.into(),
    }
}

/// The unit's entity, which must be controlled by `controller`.
pub fn owned(world: &World, unit: u32, controller: &str) -> Result<Entity, CommandError> {
    let e = entity(world, unit)?;
    match world.get::<ExternalController>(e) {
        None => Err(unavailable(format!(
            "unit {unit} has no external controller (agent/attach first)"
        ))),
        Some(c) if c.id != controller => Err(unavailable(format!(
            "unit {unit} is controlled by `{}`, not `{controller}`",
            c.id
        ))),
        Some(_) => Ok(e),
    }
}

/// Attaches `controller` to `unit` (recorded, so replays reproduce agent
/// sessions). JSON: `{"type":"AttachController","unit":5,"controller":"agent",
/// "mode":"Direct","on_idle":"Idle","force":false}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachController {
    pub unit: u32,
    pub controller: String,
    #[serde(default)]
    pub mode: ControlMode,
    #[serde(default)]
    pub on_idle: IdlePolicy,
    /// Take the unit over from another controller.
    #[serde(default)]
    pub force: bool,
}

impl ContentCommand for AttachController {
    const NAME: &'static str = "AttachController";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = entity(world, self.unit)?;
        if self.controller.is_empty() || self.controller.len() > 64 {
            return Err(CommandError::InvalidValue {
                field: "controller".into(),
                reason: "must be 1..=64 characters".into(),
            });
        }
        if self.mode == ControlMode::Direct && world.get::<Brain>(e).is_none() {
            return Err(unavailable(format!(
                "unit {} has no behaviour tree to suspend; attach as Advisor or Observer",
                self.unit
            )));
        }
        if let Some(c) = world.get::<ExternalController>(e)
            && c.id != self.controller
            && !self.force
        {
            return Err(unavailable(format!(
                "unit {} is already controlled by `{}` (detach it, or attach with force)",
                self.unit, c.id
            )));
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        attach_now(world, e, &self.controller, self.mode, self.on_idle);
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Puts a controller on `agent` right away (what [`AttachController`]
/// applies; scenario loaders use it to pre-attach before tick 0). A
/// different controller's queue is cancelled.
pub fn attach_now(
    world: &mut World,
    agent: Entity,
    controller: &str,
    mode: ControlMode,
    on_idle: IdlePolicy,
) {
    let previous = world.get::<ExternalController>(agent).cloned();
    if previous.as_ref().is_some_and(|p| p.id != controller) {
        cancel_all(world, agent);
    }
    let mut e = world.entity_mut(agent);
    e.insert(ExternalController {
        id: controller.to_owned(),
        mode,
        on_idle,
    });
    if mode == ControlMode::Direct && !e.contains::<ExternalQueue>() {
        e.insert(ExternalQueue::default());
    }
    if e.contains::<Brain>() {
        e.insert(ThinkRequest);
    }
    let unit = sim_id(world, agent);
    journal::record(world, unit, || JournalKind::ControllerAttached {
        controller: controller.to_owned(),
        mode: mode.name().to_owned(),
        on_idle: on_idle.name().to_owned(),
    });
}

/// Detaches the unit's controller; its queue is cancelled and the unit's own
/// tree resumes. JSON: `{"type":"DetachController","unit":5,"controller":"agent"}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DetachController {
    pub unit: u32,
    pub controller: String,
}

impl ContentCommand for DetachController {
    const NAME: &'static str = "DetachController";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        owned(world, self.unit, &self.controller).map(drop)
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        let mut ent = world.entity_mut(e);
        ent.remove::<ExternalController>();
        if ent.contains::<Brain>() {
            ent.insert(ThinkRequest);
        }
        let unit = Some(SimId(self.unit));
        journal::record(world, unit, || JournalKind::ControllerDetached {
            controller: self.controller.clone(),
        });
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Queues an action on a `Direct` unit (`replace`: cancel the current and
/// pending actions first). The steps are resolved by the verb's
/// `ActionHandler` when the agent acts, so the command is self-contained.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentAct {
    pub unit: u32,
    pub controller: String,
    pub handle: u32,
    #[serde(default)]
    pub replace: bool,
    pub verb: String,
    #[serde(default)]
    pub label: String,
    pub steps: Vec<Step>,
    /// Ticks between the observation the agent acted on and the act.
    #[serde(default)]
    pub latency_ticks: Option<u64>,
}

/// The length `unit`'s queue will have once the `AgentAct`s already
/// submitted for the current tick apply (a `replace` act empties it first),
/// starting from `pending`. At apply time the due commands have left the
/// `CommandQueue`, so this counts only acts still waiting: request-time and
/// apply-time validation agree.
pub fn with_same_tick_acts(world: &World, unit: u32, pending: usize) -> usize {
    let (Some(clock), Some(queue)) = (
        world.get_resource::<npc_kernel::SimClock>(),
        world.get_resource::<npc_kernel::commands::CommandQueue>(),
    ) else {
        return pending;
    };
    let tick = clock.tick;
    let mut n = pending;
    for cmd in queue
        .pending
        .range((tick, 0)..=(tick, u32::MAX))
        .map(|(_, c)| c)
    {
        if let SimCommand::Content { kind, payload } = cmd
            && kind == AgentAct::NAME
            && payload.get("unit").and_then(serde_json::Value::as_u64) == Some(u64::from(unit))
        {
            if payload.get("replace").and_then(serde_json::Value::as_bool) == Some(true) {
                n = 1;
            } else {
                n += 1;
            }
        }
    }
    n
}

/// Checks steps against the registered actions.
pub fn validate_steps(world: &World, steps: &[Step]) -> Result<(), CommandError> {
    let invalid = |reason: String| CommandError::InvalidValue {
        field: "steps".into(),
        reason,
    };
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(invalid(format!("needs 1..={MAX_STEPS} steps")));
    }
    for s in steps {
        match s {
            Step::Action { action, .. } => {
                if action_fns(world, action).is_none() {
                    return Err(invalid(format!("unknown action `{action}`")));
                }
            }
            Step::SetBlackboard { key, .. } => {
                if *key >= 8 {
                    return Err(invalid(format!("blackboard key {key} is not 0..8")));
                }
            }
            Step::Wait { ticks } => {
                if *ticks > MAX_WAIT_TICKS {
                    return Err(invalid(format!("wait {ticks} exceeds {MAX_WAIT_TICKS}")));
                }
            }
        }
    }
    Ok(())
}

impl ContentCommand for AgentAct {
    const NAME: &'static str = "AgentAct";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = owned(world, self.unit, &self.controller)?;
        if is_direct(world, e).is_none() {
            return Err(unavailable(format!(
                "unit {} is not under Direct control",
                self.unit
            )));
        }
        let pending = world.get::<ExternalQueue>(e).map_or(0, |q| q.pending.len());
        let queued = with_same_tick_acts(world, self.unit, pending);
        if !self.replace && queued >= MAX_QUEUE {
            return Err(unavailable(format!("the queue is full ({MAX_QUEUE})")));
        }
        validate_steps(world, &self.steps)
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        let item = QueuedAction {
            handle: self.handle,
            verb: self.verb,
            label: self.label,
            steps: self.steps,
        };
        if world.get::<ExternalQueue>(e).is_none() {
            world.entity_mut(e).insert(ExternalQueue::default());
        }
        let replaced: Vec<QueuedAction> = {
            let Some(mut q) = world.get_mut::<ExternalQueue>(e) else {
                return;
            };
            let replaced = if self.replace {
                q.cancel_current |= q.current.is_some();
                q.pending.drain(..).collect()
            } else {
                Vec::new()
            };
            q.pending.push_back(item.clone());
            replaced
        };
        for old in &replaced {
            journal_action(world, e, old, "cancelled", None);
        }
        journal_action(world, e, &item, "queued", self.latency_ticks);
        world.entity_mut(e).insert(ThinkRequest);
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Cancels one action (`handle`), or the current one (`handle: null`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentCancel {
    pub unit: u32,
    pub controller: String,
    #[serde(default)]
    pub handle: Option<u32>,
}

impl ContentCommand for AgentCancel {
    const NAME: &'static str = "AgentCancel";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = owned(world, self.unit, &self.controller)?;
        let Some(handle) = self.handle else {
            return Ok(());
        };
        let known = world.get::<ExternalQueue>(e).is_some_and(|q| {
            q.current.as_ref().is_some_and(|c| c.item.handle == handle)
                || q.pending.iter().any(|i| i.handle == handle)
        });
        if known {
            Ok(())
        } else {
            Err(unavailable(format!(
                "action {handle} is not queued or running"
            )))
        }
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        let removed = world.get_mut::<ExternalQueue>(e).and_then(|mut q| {
            let current = q.current.as_ref().map(|c| c.item.handle);
            match self.handle {
                None => {
                    q.cancel_current |= current.is_some();
                    None
                }
                Some(h) if current == Some(h) => {
                    q.cancel_current = true;
                    None
                }
                Some(h) => {
                    let i = q.pending.iter().position(|item| item.handle == h)?;
                    q.pending.remove(i)
                }
            }
        });
        if let Some(item) = removed {
            journal_action(world, e, &item, "cancelled", None);
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Cancels the current action and empties the queue.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentClear {
    pub unit: u32,
    pub controller: String,
}

impl ContentCommand for AgentClear {
    const NAME: &'static str = "AgentClear";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        owned(world, self.unit, &self.controller).map(drop)
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        let pending: Vec<QueuedAction> = world
            .get_mut::<ExternalQueue>(e)
            .map(|mut q| {
                q.cancel_current |= q.current.is_some();
                q.pending.drain(..).collect()
            })
            .unwrap_or_default();
        for item in &pending {
            journal_action(world, e, item, "cancelled", None);
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_wraps_the_root_and_shifts_indices() {
        let mut b = npc_ai::TreeBuilder::new("t");
        b.selector("Root", |b| {
            b.leaf("A", "a", ());
            b.leaf("B", "b", ());
        });
        let mut tree = b.build();
        inject_external_control(&mut tree);
        let names: Vec<&str> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            [ROOT_NODE, GUARD_NODE, LEAF_NODE, "Root", "A", "B"],
            "pre-order kept"
        );
        assert_eq!(tree.nodes[0].children, [1, 3]);
        assert_eq!(tree.nodes[3].children, [4, 5]);
        let before = tree.clone();
        inject_external_control(&mut tree);
        assert_eq!(tree, before, "idempotent");
    }
}
