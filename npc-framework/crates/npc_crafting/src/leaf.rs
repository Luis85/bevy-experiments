//! The generic `Craft` behaviour-tree action.
//!
//! `TreeBuilder::leaf(node, "Craft", "bread")`: the leaf's `ParamValue::Name`
//! is the recipe; the workstation (for recipes that need one) is the
//! `SimId` in blackboard slot `InteractionConfig::target_slot`
//! (`BbValue::U32`). Without a name (`ParamValue::None`) the leaf works the
//! first order on that workstation it can do ([`next_order_for`]). The
//! leaf calls [`begin_craft`] (start fails with `Failure` when it is refused) and is `Running` until the craft
//! completes: the completion writes `Success` (it succeeded) or `Failure`
//! (failed, critical failure, invalid). Aborting the branch cancels the
//! craft.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{BbKey, BbValue, Blackboard, ParamValue, Status, StopReason};
use npc_interaction::InteractionConfig;
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};

use crate::defs::RecipeRegistry;
use crate::run::{CraftSource, Crafting, begin_craft, cancel_craft};
use crate::station::next_order_for;

/// The registered action name.
pub const CRAFT_ACTION: &str = "Craft";

/// The `Craft` action component (what the leaf started).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct CraftLeaf {
    pub recipe: String,
    pub workstation: Option<SimId>,
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

/// `Craft` start.
pub fn craft_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let slot = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .target_slot;
    let station = slot_id(world.get::<Blackboard>(agent), slot);
    let (recipe, workstation, order) = match param.as_name() {
        Some(recipe) => {
            let needs_station = world
                .get_resource::<RecipeRegistry>()
                .and_then(|r| r.get(recipe))
                .is_some_and(|d| !d.stations.is_empty());
            let ws = if needs_station { station } else { None };
            (recipe.to_owned(), ws, None)
        }
        None => {
            let Some(ws) = station else {
                return Status::Failure;
            };
            match next_order_for(world, agent, ws) {
                Ok((id, recipe)) => (recipe, Some(ws), Some(id)),
                Err(_) => return Status::Failure,
            }
        }
    };
    match begin_craft(world, agent, &recipe, workstation, CraftSource::Tree, order) {
        Ok(()) => {
            world.entity_mut(agent).insert(CraftLeaf {
                recipe,
                workstation,
            });
            Status::Running
        }
        Err(_) => Status::Failure,
    }
}

/// `Craft` stop: an aborted leaf cancels its craft.
pub fn craft_stop(world: &mut World, agent: Entity, reason: StopReason) {
    if world.get::<CraftLeaf>(agent).is_none() {
        return;
    }
    world.entity_mut(agent).remove::<CraftLeaf>();
    if world
        .get::<Crafting>(agent)
        .is_some_and(|c| c.source == CraftSource::Tree)
    {
        let why = match reason {
            StopReason::Aborted => "aborted",
            StopReason::Finished => "finished",
        };
        cancel_craft(world, agent, why);
    }
}
