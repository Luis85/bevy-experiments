//! Per-agent brain state and the think pass.

use std::cmp::Reverse;

use bevy_ecs::entity::EntityHashSet;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::info_span;

use crate::registry::{ActionFns, AiRegistry, CondFn, ScoreFn, StopReason};
use crate::trace::{AiTrace, AiTraceEvent, AiTraceKind, ThinkReason};
use crate::tree::{
    Abort, ActionId, CondId, NodeIdx, NodeKind, ParamValue, ScoreId, Status, TreeDef, TreeId,
};

/// Which shared tree this agent runs.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[require(BrainState, Blackboard, BrainStats)]
pub struct Brain {
    pub tree: TreeId,
}

/// Per-agent memory. Plain data so it can be hashed/serialized.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrainState {
    /// Nodes from root to the running leaf (empty when nothing runs).
    pub running_path: Vec<NodeIdx>,
    /// One word per tree node (lazily sized to the tree). Sequences store
    /// the index of their current child here.
    pub mem: Vec<u32>,
}

/// Debug/diagnostic counters.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrainStats {
    pub leaf_starts: u32,
    pub aborts: u32,
}

/// The leaf currently running on this agent. Inserted by [`think`] when a
/// leaf's start returns `Running`; its `status` is then written only by that
/// action's own system. Removed by [`think`] when it consumes the result.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[component(storage = "SparseSet")]
pub struct ActiveLeaf {
    pub node: NodeIdx,
    pub action: ActionId,
    pub status: Status,
}

/// Marker: this agent must think this tick even if its leaf is still running
/// (e.g. a need crossed a threshold, day turned to night).
#[derive(Component, Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[component(storage = "SparseSet")]
pub struct ThinkRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BbKey(pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BbValue {
    #[default]
    None,
    U32(u32),
    I32(i32),
    Pos(i32, i32),
}

/// Small fixed-slot blackboard for passing data between leaves.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Blackboard {
    pub slots: [BbValue; 8],
}

impl Blackboard {
    pub fn get(&self, key: BbKey) -> BbValue {
        self.slots[key.0 as usize]
    }
    pub fn set(&mut self, key: BbKey, value: BbValue) {
        self.slots[key.0 as usize] = value;
    }
}

/// Runs one think step for each agent in `agents`, in the given order.
///
/// An agent is evaluated when it has a `ThinkRequest`, has no `ActiveLeaf`,
/// or its `ActiveLeaf.status` is not `Running`; otherwise it is skipped.
/// `ThinkRequest` is removed from every agent processed.
///
/// Semantics (see plan Task A for the full contract):
/// - Leaf: if it is the active leaf and Running → Running. If it is the active
///   leaf and finished → call `stop`, remove `ActiveLeaf`, return its status.
///   Otherwise call `start`; on Running insert `ActiveLeaf` (stats.leaf_starts += 1).
///   A `start` returning Success/Failure calls nothing else.
/// - Sequence: resumes at its stored child index when on the running path.
/// - Selector: re-evaluates from child 0. Children before the running branch
///   are only checked if they are `Guard { abort: LowerPriority }`; if such a
///   guard's cond passes, the running branch is aborted (active leaf `stop`
///   with `Aborted`, even if that leaf already completed,
///   `ActiveLeaf` removed, its subtree memory reset, stats.aborts += 1) and the
///   guard's child is evaluated; on its failure evaluation continues with the
///   following children (the aborted branch restarts fresh).
///   When the running branch itself fails, the selector re-evaluates from
///   child 0 (all children, not only reactive guards), skipping only that
///   failed child for the rest of this call: a higher-priority sibling that
///   became true while the branch ran (e.g. night fell) wins over the
///   siblings after it.
/// - Guard: cond false → Failure. On the running path the cond is not re-checked.
/// - UtilitySelector: on the running path continue the running child; otherwise
///   order children by score desc (ties → lower index) and act like a selector.
/// - Each agent's tree is evaluated at most once per call.
///
/// Observability: when an [`AiTrace`] resource exists with `enabled`, every
/// decision is appended to it (see [`AiTraceKind`]). Work is wrapped in
/// `tracing` spans named `"mechanic"` with field `name` = `"think.agent"`
/// (one agent's evaluation), `"think.cond"`, `"think.start"` and
/// `"think.stop"`, so a profiler can break `think` down.
pub fn think(world: &mut World, agents: &[Entity]) {
    if agents.is_empty() {
        return;
    }
    let (tracing, verbose) = world
        .get_resource::<AiTrace>()
        .map_or((false, false), |t| (t.enabled, t.enabled && t.verbose));
    // Ids exist only once the registry is frozen (normally in
    // `Plugin::finish`); freezing here too keeps bare-`World` hosts working.
    if !world.resource::<AiRegistry>().is_frozen() {
        world.resource_mut::<AiRegistry>().freeze();
    }
    // Copy the registry's function pointers out once per call so actions and
    // conditions get an unrestricted `World` (they may read `AiRegistry`).
    let fns = Fns::from(world.resource::<AiRegistry>());
    let mut trees: Vec<Option<TreeDef>> = Vec::new();
    let mut seen = EntityHashSet::default();

    for &agent in agents {
        if !seen.insert(agent) {
            continue;
        }
        let Ok(entity) = world.get_entity(agent) else {
            continue;
        };
        let Some(&Brain { tree: tree_id }) = entity.get::<Brain>() else {
            continue;
        };
        let requested = entity.contains::<ThinkRequest>();
        let leaf_status = entity.get::<ActiveLeaf>().map(|leaf| leaf.status);
        let running = leaf_status == Some(Status::Running);
        if running && !requested {
            continue;
        }
        let _agent_span = info_span!("mechanic", name = "think.agent").entered();
        world.entity_mut(agent).remove::<ThinkRequest>();
        if tracing {
            let reason = match leaf_status {
                _ if requested => ThinkReason::Request,
                Some(_) => ThinkReason::LeafDone,
                None => ThinkReason::NoLeaf,
            };
            emit(world, agent, AiTraceKind::ThinkStart { reason });
        }

        let slot = tree_id.0 as usize;
        if trees.len() <= slot {
            trees.resize(slot + 1, None);
        }
        let tree =
            trees[slot].get_or_insert_with(|| world.resource::<AiRegistry>().tree(tree_id).clone());

        let mut state = world
            .get_mut::<BrainState>(agent)
            .map(|mut s| std::mem::take(&mut *s))
            .unwrap_or_default();
        let stats = world.get::<BrainStats>(agent).copied().unwrap_or_default();
        state.mem.resize(tree.nodes.len(), 0);

        let mut cx = Ctx {
            world: &mut *world,
            agent,
            tree,
            fns: &fns,
            prev: std::mem::take(&mut state.running_path),
            mem: std::mem::take(&mut state.mem),
            stack: Vec::new(),
            new_path: Vec::new(),
            stats,
            tracing,
            verbose,
        };
        let status = cx.eval(0);
        if tracing && status != Status::Running {
            emit(cx.world, agent, AiTraceKind::RootDone { status });
        }
        let Ctx {
            mem,
            new_path,
            stats,
            ..
        } = cx;
        state.mem = mem;
        state.running_path = if status == Status::Running {
            new_path
        } else {
            Vec::new()
        };

        if let Some(mut s) = world.get_mut::<BrainState>(agent) {
            *s = state;
        }
        if let Some(mut s) = world.get_mut::<BrainStats>(agent) {
            *s = stats;
        }
    }
}

/// Appends one event to the `AiTrace` resource, if present.
fn emit(world: &mut World, agent: Entity, kind: AiTraceKind) {
    if let Some(mut trace) = world.get_resource_mut::<AiTrace>() {
        trace.events.push(AiTraceEvent { agent, kind });
    }
}

/// Function pointers copied out of the [`AiRegistry`].
struct Fns {
    actions: Vec<ActionFns>,
    conds: Vec<CondFn>,
    scorers: Vec<ScoreFn>,
}

impl From<&AiRegistry> for Fns {
    fn from(reg: &AiRegistry) -> Self {
        Self {
            actions: reg.actions.clone(),
            conds: reg.conds.iter().map(|&(_, f)| f).collect(),
            scorers: reg.scorers.iter().map(|&(_, f)| f).collect(),
        }
    }
}

/// One agent's evaluation of its tree.
struct Ctx<'a> {
    world: &'a mut World,
    agent: Entity,
    tree: &'a TreeDef,
    fns: &'a Fns,
    /// Running path at the start of this think (cleared by an abort).
    prev: Vec<NodeIdx>,
    mem: Vec<u32>,
    /// Nodes from the root to the node being evaluated.
    stack: Vec<NodeIdx>,
    /// Path to the leaf that returned `Running`, if any.
    new_path: Vec<NodeIdx>,
    stats: BrainStats,
    /// Record `AiTrace` events.
    tracing: bool,
    /// Record `CondEval` events too.
    verbose: bool,
}

impl Ctx<'_> {
    fn eval(&mut self, n: NodeIdx) -> Status {
        self.stack.push(n);
        let status = self.eval_inner(n);
        self.stack.pop();
        status
    }

    fn eval_inner(&mut self, n: NodeIdx) -> Status {
        let tree = self.tree;
        let node = &tree.nodes[n as usize];
        match &node.kind {
            NodeKind::Leaf { action, param } => self.leaf(n, action.id(), param),
            NodeKind::Sequence => {
                let start = if self.on_path(n) {
                    self.mem[n as usize] as usize
                } else {
                    0
                };
                for (i, &child) in node.children.iter().enumerate().skip(start) {
                    self.mem[n as usize] = i as u32;
                    match self.eval(child) {
                        Status::Running => return Status::Running,
                        Status::Failure => {
                            self.mem[n as usize] = 0;
                            return Status::Failure;
                        }
                        Status::Success => {}
                    }
                }
                self.mem[n as usize] = 0;
                Status::Success
            }
            NodeKind::Selector => {
                let mut running = self.path_child(n);
                // The running child that failed during this call, if any.
                let mut failed = None;
                let mut i = 0;
                while i < node.children.len() {
                    let child = node.children[i];
                    if failed == Some(i) {
                        i += 1;
                        continue;
                    }
                    if let Some(r) = running
                        && i < r
                    {
                        // Before the running branch only reactive guards are re-checked.
                        if let NodeKind::Guard {
                            cond,
                            abort: Abort::LowerPriority,
                        } = &tree.nodes[child as usize].kind
                            && self.cond(child, cond.id())
                        {
                            self.abort_branch(child, node.children[r]);
                            running = None;
                            // The cond just passed: evaluate the guard's child directly.
                            self.stack.push(child);
                            let status = self.eval(tree.nodes[child as usize].children[0]);
                            self.stack.pop();
                            if status != Status::Failure {
                                return status;
                            }
                        }
                        i += 1;
                        continue;
                    }
                    let status = self.eval(child);
                    if status != Status::Failure {
                        return status;
                    }
                    if running == Some(i) {
                        // The running branch failed: reconsider every sibling
                        // from the first, except this one.
                        running = None;
                        failed = Some(i);
                        i = 0;
                        continue;
                    }
                    i += 1;
                }
                Status::Failure
            }
            NodeKind::Guard { cond, .. } => {
                if self.on_path(n) || self.cond(n, cond.id()) {
                    self.eval(node.children[0])
                } else {
                    Status::Failure
                }
            }
            NodeKind::UtilitySelector { scorers } => {
                let tried = self.path_child(n);
                if let Some(r) = tried {
                    let status = self.eval(node.children[r]);
                    if status != Status::Failure {
                        return status;
                    }
                }
                let scores: Vec<i32> = scorers.iter().map(|s| self.score(s.id())).collect();
                let mut order: Vec<usize> = (0..node.children.len())
                    .filter(|&i| Some(i) != tried)
                    .collect();
                order.sort_by_key(|&i| (Reverse(scores[i]), i));
                for i in order {
                    let status = self.eval(node.children[i]);
                    if status != Status::Failure {
                        return status;
                    }
                }
                Status::Failure
            }
        }
    }

    fn leaf(&mut self, n: NodeIdx, action: ActionId, param: &ParamValue) -> Status {
        let active = self.world.get::<ActiveLeaf>(self.agent).copied();
        match active {
            Some(leaf) if leaf.node == n && leaf.action == action => {
                if leaf.status == Status::Running {
                    self.new_path.clone_from(&self.stack);
                } else {
                    self.stop_active(leaf, StopReason::Finished);
                }
                leaf.status
            }
            other => {
                // Safety net: never start a leaf while another is still active
                // (e.g. the agent's tree was swapped), so nothing leaks.
                if let Some(stale) = other {
                    self.stop_active(stale, StopReason::Aborted);
                    self.stats.aborts += 1;
                }
                let status = {
                    let _span = info_span!("mechanic", name = "think.start").entered();
                    (self.fns.actions[action.0 as usize].start)(self.world, self.agent, param)
                };
                if self.tracing {
                    self.emit(AiTraceKind::LeafStart {
                        node: n,
                        action,
                        status,
                    });
                }
                if status == Status::Running {
                    if let Ok(mut e) = self.world.get_entity_mut(self.agent) {
                        e.insert(ActiveLeaf {
                            node: n,
                            action,
                            status,
                        });
                    }
                    self.stats.leaf_starts += 1;
                    self.new_path.clone_from(&self.stack);
                }
                status
            }
        }
    }

    /// Aborts the running branch rooted at `branch`: stops the active leaf
    /// with `StopReason::Aborted`, resets the subtree's memory and forgets the
    /// old running path. The reason is `Aborted` even when the leaf already
    /// completed (its status is not consumed yet): the branch it belongs to was
    /// interrupted, so the stop must drop everything that branch claimed.
    fn abort_branch(&mut self, guard: NodeIdx, branch: NodeIdx) {
        if self.tracing {
            self.emit(AiTraceKind::Abort {
                guard,
                aborted: self.prev.clone(),
            });
        }
        if let Some(leaf) = self.world.get::<ActiveLeaf>(self.agent).copied() {
            self.stop_active(leaf, StopReason::Aborted);
        }
        let end = self.tree.subtree_end(branch);
        self.mem[branch as usize..end].fill(0);
        self.prev.clear();
        self.stats.aborts += 1;
    }

    /// Calls the leaf's `stop`, then removes `ActiveLeaf`.
    /// `LeafStop` is traced before `stop` runs, so whatever the stop does
    /// (e.g. releasing reservations) is observed after it.
    fn stop_active(&mut self, leaf: ActiveLeaf, reason: StopReason) {
        if self.tracing {
            self.emit(AiTraceKind::LeafStop {
                node: leaf.node,
                action: leaf.action,
                reason,
            });
        }
        {
            let _span = info_span!("mechanic", name = "think.stop").entered();
            (self.fns.actions[leaf.action.0 as usize].stop)(self.world, self.agent, reason);
        }
        if let Ok(mut e) = self.world.get_entity_mut(self.agent) {
            e.remove::<ActiveLeaf>();
        }
    }

    fn on_path(&self, n: NodeIdx) -> bool {
        self.prev.contains(&n)
    }

    /// Index (within `n`'s children) of the child on the running path.
    fn path_child(&self, n: NodeIdx) -> Option<usize> {
        let pos = self.prev.iter().position(|&p| p == n)?;
        let next = *self.prev.get(pos + 1)?;
        self.tree.nodes[n as usize]
            .children
            .iter()
            .position(|&c| c == next)
    }

    /// Evaluates `id` for guard `node` (traced as `CondEval` when verbose).
    fn cond(&mut self, node: NodeIdx, id: CondId) -> bool {
        let result = {
            let _span = info_span!("mechanic", name = "think.cond").entered();
            (self.fns.conds[id.0 as usize])(self.world, self.agent)
        };
        if self.verbose {
            self.emit(AiTraceKind::CondEval {
                node,
                cond: id,
                result,
            });
        }
        result
    }

    fn emit(&mut self, kind: AiTraceKind) {
        emit(self.world, self.agent, kind);
    }

    fn score(&self, id: ScoreId) -> i32 {
        (self.fns.scorers[id.0 as usize])(self.world, self.agent)
    }
}
