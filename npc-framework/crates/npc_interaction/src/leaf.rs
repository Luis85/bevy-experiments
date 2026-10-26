//! The generic `Interact` behaviour-tree action.
//!
//! `TreeBuilder::leaf(node, "Interact", "open")`: the leaf's `ParamValue::
//! Name` is the verb; the target is the `SimId` in blackboard slot
//! `InteractionConfig::target_slot` (`BbValue::U32`), the optional item the
//! one in `item_slot` (read only for verbs that take an item). The leaf
//! calls [`begin`](crate::run::begin) (start fails with `Failure` when it is
//! refused) and is `Running` until the interaction completes: the
//! completion writes `Success` or `Failure` into the `ActiveLeaf`. Aborting
//! the branch cancels the interaction. A `Direct` agent's queue runs the same
//! leaf (`SetBlackboard` steps, then `Action "Interact"`).

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{BbKey, BbValue, Blackboard, ParamValue, Status, StopReason};
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};

use crate::InteractionConfig;
use crate::defs::{InteractionRegistry, ItemArg};
use crate::run::{Interacting, Source, begin, cancel};

/// The registered action name.
pub const INTERACT_ACTION: &str = "Interact";

/// The `Interact` action component (what the leaf started).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct InteractLeaf {
    pub verb: String,
    pub target: SimId,
    pub item: Option<SimId>,
}

fn slot_id(bb: Option<&Blackboard>, slot: u8) -> Option<SimId> {
    let bb = bb?;
    if usize::from(slot) >= bb.slots.len() {
        return None;
    }
    match bb.get(BbKey(slot)) {
        BbValue::U32(id) => Some(SimId(id)),
        _ => None,
    }
}

/// `Interact` start.
pub fn interact_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let Some(verb) = param.as_name().map(str::to_owned) else {
        tracing::warn!("Interact leaf without a verb name (ParamValue::Name)");
        return Status::Failure;
    };
    let config = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default();
    let bb = world.get::<Blackboard>(agent);
    let Some(target) = slot_id(bb, config.target_slot) else {
        return Status::Failure;
    };
    let takes_item = world
        .get_resource::<InteractionRegistry>()
        .and_then(|r| r.verb(&verb))
        .is_some_and(|d| d.item != ItemArg::None);
    let item = if takes_item {
        slot_id(bb, config.item_slot)
    } else {
        None
    };
    match begin(world, agent, &verb, target, item, Source::Tree) {
        Ok(()) => {
            world
                .entity_mut(agent)
                .insert(InteractLeaf { verb, target, item });
            Status::Running
        }
        Err(_) => Status::Failure,
    }
}

/// `Interact` stop: an aborted leaf cancels its interaction.
pub fn interact_stop(world: &mut World, agent: Entity, reason: StopReason) {
    if world.get::<InteractLeaf>(agent).is_none() {
        return;
    }
    world.entity_mut(agent).remove::<InteractLeaf>();
    if world
        .get::<Interacting>(agent)
        .is_some_and(|i| i.source == Source::Tree)
    {
        let why = match reason {
            StopReason::Aborted => "aborted",
            StopReason::Finished => "finished",
        };
        cancel(world, agent, why);
    }
}
