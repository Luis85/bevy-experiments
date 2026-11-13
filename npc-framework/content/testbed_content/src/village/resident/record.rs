//! Decision records: why a resident is doing what it is doing.
//!
//! When a resident's running leaf changes, one `VillageDecision` journal effect
//! names the chosen branch and every higher-priority option that was passed
//! over, with the failing condition or the reason the leaf could not start.
//! Nothing here is hashed, saved or read by a decision: the records exist only
//! while the journal is recording, and the simulation is identical without them.
use super::super::ai::{self, Procurement};
use super::{actions, param_name};
use bevy_ecs::prelude::*;
use npc_framework::ai::{NodeIdx, NodeKind, TreeDef};
use npc_framework::kernel::journal::is_recording;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Tried {
    actor: Entity,
    action: &'static str,
    param: String,
    reason: &'static str,
}

/// Scratch of the current tick plus the last recorded leaf per resident.
/// Tooling state: not registered with the hash or the snapshot.
#[derive(Resource, Default)]
pub struct DecisionTrace {
    last: BTreeMap<SimId, NodeIdx>,
    tried: Vec<Tried>,
    started: Vec<(Entity, Option<Value>)>,
    pub(super) travel_failed: bool,
    /// Wanted purchases of one resident at one tick, shared by the buy leaves
    /// of that think. A pure memo: the key is the whole input.
    procure: Option<(Entity, u64, Vec<Procurement>)>,
    /// Records written since the world was built.
    pub recorded: u64,
    /// The structured form of the last record per resident, with the tick.
    views: BTreeMap<SimId, (u64, DecisionView)>,
}

/// One option that was passed over on the way to the running leaf.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ConsideredOption {
    /// Name of the branch in the tree file.
    pub node: String,
    /// The option's guard is false: it does not apply to this resident now.
    /// When false, the option applied and could not start; see `reason`.
    pub does_not_apply: bool,
    /// The guard of the option, when it has one.
    pub condition: Option<String>,
    /// Why an option that applied could not start. Only known for a record
    /// that was written when the choice was made (`recorded`).
    pub reason: Option<String>,
    /// Score under a utility selector.
    pub score: Option<i32>,
    /// The option that runs (only listed under a utility selector).
    pub chosen: bool,
}

/// Why a resident is doing what it is doing, as data.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DecisionView {
    /// Tick the choice was recorded; `None` when read from the live tree.
    pub tick: Option<u64>,
    /// True when this is the record written at the moment of choice (the
    /// journal was on). False: conditions are evaluated now, and reasons a
    /// leaf could not start are unknown.
    pub recorded: bool,
    pub tree: String,
    /// Node names from the root to the running leaf.
    pub branch: Vec<String>,
    /// Number of the leaf in the tree file.
    pub node: usize,
    pub action: String,
    pub param: String,
    pub activity: Option<&'static str>,
    pub considered: Vec<ConsideredOption>,
    /// GOAP plan or craft candidates of the record, when it had any.
    pub detail: Option<Value>,
}

/// The latest decision of a resident as data: the record written when the
/// running leaf was chosen if the journal was on, else the live explanation.
pub fn decision(world: &World, actor: Entity) -> Option<DecisionView> {
    let live = view(world, actor, &[])?;
    let stored = world
        .get::<SimId>(actor)
        .and_then(|id| world.get_resource::<DecisionTrace>()?.views.get(id));
    Some(match stored {
        Some((tick, record)) if record.node == live.node => DecisionView {
            tick: Some(*tick),
            recorded: true,
            ..record.clone()
        },
        _ => live,
    })
}

fn view(world: &World, actor: Entity, tried: &[Tried]) -> Option<DecisionView> {
    let brain = world.get::<Brain>(actor)?;
    let path = &world.get::<BrainState>(actor)?.running_path;
    let registry = world.get_resource::<AiRegistry>()?;
    let tree = registry.tree(brain.tree);
    let leaf = *path.last()?;
    let NodeKind::Leaf { action, param } = &tree.nodes.get(leaf as usize)?.kind else {
        return None;
    };
    let offset = super::spec(world)
        .and_then(|spec| spec.trees.iter().find(|file| file.tree.name == tree.name))
        .map_or(0, |file| {
            tree.nodes.len().saturating_sub(file.tree.nodes.len())
        });
    let path: Vec<NodeIdx> = path
        .iter()
        .copied()
        .filter(|node| *node as usize >= offset)
        .collect();
    let mine: Vec<&Tried> = tried.iter().filter(|entry| entry.actor == actor).collect();
    let mut considered = Vec::new();
    for pair in path.windows(2) {
        let (parent, chosen) = (&tree.nodes[pair[0] as usize], pair[1]);
        match &parent.kind {
            NodeKind::Selector => {
                for &sibling in parent.children.iter().take_while(|child| **child != chosen) {
                    let def = &tree.nodes[sibling as usize];
                    let (condition, holds) = match &def.kind {
                        NodeKind::Guard { cond, .. } => (
                            Some(cond.name.clone()),
                            registry
                                .conds
                                .get(cond.id().0 as usize)
                                .is_some_and(|(_, test)| test(world, actor)),
                        ),
                        _ => (None, true),
                    };
                    let inner = match &def.kind {
                        NodeKind::Guard { .. } => def.children[0],
                        _ => sibling,
                    };
                    considered.push(ConsideredOption {
                        node: def.name.clone(),
                        does_not_apply: !holds,
                        condition,
                        reason: (holds && !tried.is_empty())
                            .then(|| rejection(world, registry, tree, actor, inner, &mine)),
                        score: None,
                        chosen: false,
                    });
                }
            }
            NodeKind::UtilitySelector { scorers } => {
                for (index, &sibling) in parent.children.iter().enumerate() {
                    considered.push(ConsideredOption {
                        node: tree.nodes[sibling as usize].name.clone(),
                        does_not_apply: false,
                        condition: None,
                        reason: None,
                        score: registry
                            .scorers
                            .get(scorers[index].id().0 as usize)
                            .map(|(_, score)| score(world, actor)),
                        chosen: sibling == chosen,
                    });
                }
            }
            _ => {}
        }
    }
    Some(DecisionView {
        tick: None,
        recorded: false,
        tree: tree.name.clone(),
        branch: path
            .iter()
            .map(|node| tree.nodes[*node as usize].name.clone())
            .collect(),
        node: leaf as usize - offset,
        action: action.name.clone(),
        param: param_name(param),
        activity: super::label(world, actor),
        considered,
        detail: None,
    })
}

pub(super) fn clear_travel_failure(world: &mut World) {
    if let Some(mut trace) = world.get_resource_mut::<DecisionTrace>() {
        trace.travel_failed = false;
    }
}

pub(super) fn take_travel_failure(world: &mut World) -> bool {
    world
        .get_resource_mut::<DecisionTrace>()
        .is_some_and(|mut trace| std::mem::take(&mut trace.travel_failed))
}

/// The resident's wanted purchases, computed once per think.
pub(super) fn procurement(world: &mut World, actor: Entity) -> Vec<Procurement> {
    let now = world.resource::<SimClock>().tick;
    if let Some((cached, tick, list)) = &world.resource::<DecisionTrace>().procure
        && *cached == actor
        && *tick == now
    {
        return list.clone();
    }
    let list = ai::procurement(world, actor);
    world.resource_mut::<DecisionTrace>().procure = Some((actor, now, list.clone()));
    list
}

/// Notes a leaf that could not start (only while the journal records).
pub(super) fn tried(
    world: &mut World,
    actor: Entity,
    action: &'static str,
    param: &ParamValue,
    reason: &'static str,
) {
    if is_recording(world) {
        let mut trace = world.resource_mut::<DecisionTrace>();
        // Facts noted by a step that then failed belong to no started leaf.
        trace
            .started
            .retain(|(noted, detail)| *noted != actor || detail.is_none());
        trace.tried.push(Tried {
            actor,
            action,
            param: param_name(param),
            reason,
        });
    }
}

/// Notes a leaf that started (only while the journal records).
pub(super) fn started(world: &mut World, actor: Entity) {
    if is_recording(world) {
        world
            .resource_mut::<DecisionTrace>()
            .started
            .push((actor, None));
    }
}

/// Attaches extra facts (GOAP plan, craft candidates) to the leaf that is
/// starting for `actor`. `build` runs only while the journal records.
pub(super) fn detail(world: &mut World, actor: Entity, build: impl FnOnce(&World) -> Value) {
    if is_recording(world) {
        let value = build(world);
        world
            .resource_mut::<DecisionTrace>()
            .started
            .push((actor, Some(value)));
    }
}

/// Journals a `VillageDecision` for every resident whose running leaf changed
/// this tick. Called once per tick after the think pass.
pub(in super::super) fn flush(world: &mut World) {
    let Some(mut trace) = world.get_resource_mut::<DecisionTrace>() else {
        return;
    };
    if trace.started.is_empty() && trace.tried.is_empty() {
        return;
    }
    let started = std::mem::take(&mut trace.started);
    let tried = std::mem::take(&mut trace.tried);
    if !is_recording(world) {
        return;
    }
    let mut details: BTreeMap<Entity, Value> = BTreeMap::new();
    let mut actors = Vec::new();
    for (actor, detail) in started {
        match detail {
            Some(detail) => {
                details.insert(actor, detail);
            }
            None => actors.push(actor),
        }
    }
    for actor in actors {
        let (Some(id), Some(leaf)) = (
            world.get::<SimId>(actor).copied(),
            world.get::<ActiveLeaf>(actor).copied(),
        ) else {
            continue;
        };
        if world.resource::<DecisionTrace>().last.get(&id) == Some(&leaf.node) {
            continue;
        }
        let Some(mut record) = build(world, actor, &tried) else {
            continue;
        };
        let detail = details.remove(&actor);
        let structured = view(world, actor, &tried).map(|view| DecisionView {
            detail: detail.clone(),
            ..view
        });
        if let (Some(Value::Object(extra)), Value::Object(fields)) = (detail, &mut record) {
            fields.extend(extra);
        }
        let now = world.resource::<SimClock>().tick;
        let mut trace = world.resource_mut::<DecisionTrace>();
        trace.last.insert(id, leaf.node);
        trace.recorded += 1;
        if let Some(structured) = structured {
            trace.views.insert(id, (now, structured));
        }
        record_effect(world, Some(id), "VillageDecision", &record);
    }
}

/// Read-only explanation of the resident's running branch, in the shape of a
/// decision record: the branch, and why each higher-priority option is not
/// running. Conditions are evaluated now; reasons a leaf could not start are
/// only known to the journal record written when the leaf was chosen.
pub fn explain(world: &World, actor: Entity) -> Option<Value> {
    build(world, actor, &[])
}

fn build(world: &World, actor: Entity, tried: &[Tried]) -> Option<Value> {
    let brain = world.get::<Brain>(actor)?;
    let path = &world.get::<BrainState>(actor)?.running_path;
    let registry = world.get_resource::<AiRegistry>()?;
    let tree = registry.tree(brain.tree);
    let leaf = *path.last()?;
    let NodeKind::Leaf { action, param } = &tree.nodes.get(leaf as usize)?.kind else {
        return None;
    };
    // The engine wraps every tree in its own top-priority nodes (external
    // control). Records speak about the authored tree only, in its numbering.
    let offset = super::spec(world)
        .and_then(|spec| spec.trees.iter().find(|file| file.tree.name == tree.name))
        .map_or(0, |file| {
            tree.nodes.len().saturating_sub(file.tree.nodes.len())
        });
    let path: Vec<NodeIdx> = path
        .iter()
        .copied()
        .filter(|node| *node as usize >= offset)
        .collect();
    let mine: Vec<&Tried> = tried.iter().filter(|entry| entry.actor == actor).collect();
    let mut considered = Vec::new();
    for pair in path.windows(2) {
        let (parent, chosen) = (&tree.nodes[pair[0] as usize], pair[1]);
        match &parent.kind {
            NodeKind::Selector => {
                for &sibling in parent.children.iter().take_while(|child| **child != chosen) {
                    considered.push(json!({
                        "node": tree.nodes[sibling as usize].name,
                        "rejected": rejection(world, registry, tree, actor, sibling, &mine),
                    }));
                }
            }
            NodeKind::UtilitySelector { scorers } => {
                for (index, &sibling) in parent.children.iter().enumerate() {
                    let score = registry
                        .scorers
                        .get(scorers[index].id().0 as usize)
                        .map(|(_, score)| score(world, actor));
                    considered.push(json!({
                        "node": tree.nodes[sibling as usize].name,
                        "score": score,
                        "chosen": sibling == chosen,
                    }));
                }
            }
            _ => {}
        }
    }
    Some(json!({
        "tree": tree.name,
        "branch": tree.path_string(&path),
        "node": leaf as usize - offset,
        "action": action.name,
        "param": param_name(param),
        "activity": actions::label(&action.name, param),
        "considered": considered,
    }))
}

/// Why the subtree at `node` is not the running branch.
fn rejection(
    world: &World,
    registry: &AiRegistry,
    tree: &TreeDef,
    actor: Entity,
    node: NodeIdx,
    tried: &[&Tried],
) -> String {
    let def = &tree.nodes[node as usize];
    match &def.kind {
        NodeKind::Guard { cond, .. } => {
            let holds = registry
                .conds
                .get(cond.id().0 as usize)
                .is_some_and(|(_, test)| test(world, actor));
            if holds {
                let inner = rejection(world, registry, tree, actor, def.children[0], tried);
                format!("{} holds, but {inner}", cond.name)
            } else {
                format!("{} is false", cond.name)
            }
        }
        NodeKind::Leaf { action, param } => {
            let param = param_name(param);
            let reason = tried
                .iter()
                .find(|entry| entry.action == action.name && entry.param == param)
                .map_or("it could not start", |entry| entry.reason);
            format!("{} {param}: {reason}", action.name).replace(" :", ":")
        }
        NodeKind::Selector | NodeKind::Sequence | NodeKind::UtilitySelector { .. } => def
            .children
            .iter()
            .map(|child| rejection(world, registry, tree, actor, *child, tried))
            .collect::<Vec<_>>()
            .join("; "),
    }
}
