//! Read-only, perception-scoped descriptions of the physical village economy.
use super::{ResourcePatch, VillagePlace, catalog, economy};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{AgentCtx, observe::known_targets};
use npc_framework::crafting::{RecipeRegistry, Workstation};
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// Recipes are static construction data, not a second permission path for crafting.
pub fn station_recipes(world: &World, entity: Entity) -> Vec<Value> {
    let Some(station) = world.get::<Workstation>(entity) else {
        return Vec::new();
    };
    world.resource::<RecipeRegistry>().recipes.values()
        .filter(|recipe| recipe.stations.iter().any(|kind| station.is(kind)))
        .map(|recipe| json!({
            "name":recipe.name,"duration_ticks":recipe.duration_ticks,
            "inputs":recipe.inputs.iter().map(|input| json!({"kind":input.key,"count":input.count})).collect::<Vec<_>>(),
            "outputs":recipe.outputs.iter().map(|output| json!({"kind":output.kind,"count":output.count})).collect::<Vec<_>>()
        }))
        .collect()
}

pub fn observe(world: &World, ctx: &AgentCtx) -> Value {
    let known = known_targets(world, ctx).unwrap_or_default();
    let mut resources = Vec::new();
    let mut workstations = Vec::new();
    let mut wildlife = Vec::new();
    for id in known {
        let Some(entity) = world.resource::<SimIndex>().get(id) else {
            continue;
        };
        if world.get::<super::hunting::Wildlife>(entity).is_some() {
            let current = economy::near(world, ctx.entity, entity)
                || world
                    .get::<PerceptionMemory>(ctx.entity)
                    .and_then(|memory| memory.get(id))
                    .is_some_and(|p| p.sensed);
            wildlife.push(json!({"id":id,"kind":"deer","current":current,"alive":current.then(||super::living(world,entity)),"hunt_ticks":super::hunting::HUNT_TICKS,"tool":catalog::SPEAR}));
        }
        if let Some(patch) = world.get::<ResourcePatch>(entity) {
            let current = economy::near(world, ctx.entity, entity)
                || world
                    .get::<PerceptionMemory>(ctx.entity)
                    .and_then(|memory| memory.get(id))
                    .is_some_and(|percept| percept.sensed);
            resources.push(json!({"id":id,"kind":patch.kind,"gather_ticks":patch.gather_ticks,
                "current":current,"remaining":current.then_some(patch.remaining),
                "capacity":current.then_some(patch.capacity),"replenish_at":current.then_some(patch.replenish_at)}));
        }
        if let Some(place) = world.get::<VillagePlace>(entity) {
            let recipes = station_recipes(world, entity);
            // A known workplace is a public place to search, even when it has
            // no manufacturing recipe. These static opportunities say nothing
            // about a moving resident's current presence, stock or finances.
            let accepts: Vec<_> = catalog::trade_kinds(world)
                .iter()
                .copied()
                .filter(|kind| catalog::wants_for(world, place.role, kind))
                .collect();
            workstations.push(json!({"id":id,"name":place.label,"role":place.role,
                "products":catalog::outputs_for(world,place.role),"accepts":accepts,"recipes":recipes}));
        }
    }
    json!({"resources":resources,"workstations":workstations,"wildlife":wildlife})
}
