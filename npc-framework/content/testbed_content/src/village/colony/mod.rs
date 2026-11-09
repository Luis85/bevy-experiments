//! Colony composition: resident life, housing, social state and recorded civic input.
//!
//! Mechanics live in private bounded modules. Public re-exports preserve stable
//! content API paths; registration and the single ordered tick stay here.
mod arrival;
mod commands;
mod housing;
mod model;
mod social;
mod summary;

pub use arrival::JoinIsland;
pub use commands::ColonyAct;
pub use housing::{
    ConstructionIntent, HomeHeating, can_claim_cottage, construction_intent,
    construction_materials, cottage_claimed, home_heated, home_heating, home_of,
    household_materials,
};
pub use model::{AwaitingArrival, ColonyState, Household, HousingProject, SocialLife};
pub use social::{social_available, social_target};
pub use summary::{ColonySummary, summary};
// Compatibility path for callers of the original colony action API. Execution
// belongs to the resident action layer; colony domain queries never call AI.
pub use super::ai::construction_step;

use super::{Role, Villager};
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{Affordance, AgentIoAppExt};
use npc_framework::prelude::*;

pub fn enabled(world: &World) -> bool {
    world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .is_some_and(|active| active.0.colony)
}

/// Coordinate the two resident contexts without resetting either one.
/// First arrival and later successor lives share this same initialization seam.
pub fn participant_spawned(world: &mut World, actor: Entity) {
    if !world.contains_resource::<ColonyState>()
        || !super::living(world, actor)
        || world.get::<Villager>(actor).is_none()
        || world.get::<SimId>(actor).is_none()
    {
        return;
    }
    social::welcome(world, actor);
    housing::house_participant(world, actor);
}

pub fn register(app: &mut App) {
    if !enabled(app.world()) {
        return;
    }
    app.register_sim_resource::<ColonyState>()
        .register_sim_component::<Household>()
        .register_sim_component::<SocialLife>()
        .register_sim_component::<AwaitingArrival>()
        .register_command::<ColonyAct>()
        .register_command::<JoinIsland>()
        .register_verb_gate("island.arrival", |world, entity, verb| {
            (world.get::<AwaitingArrival>(entity).is_some() && verb != "join_island").then(|| {
                "begin your life with join_island before taking part in the village".into()
            })
        })
        .register_action_handler("join_island", arrival::join_handler)
        .register_affordances("island.arrival", |world, ctx| {
            if world.get::<AwaitingArrival>(ctx.entity).is_some() {
                vec![Affordance::new(
                    "join_island",
                    "Begin your mortal life in the island village",
                )]
            } else {
                Vec::new()
            }
        });
}

pub fn spawn(world: &mut World) {
    if !enabled(world) || world.contains_resource::<ColonyState>() {
        return;
    }
    world.insert_resource(ColonyState::default());
    let people: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<Villager>(*entity)
                .cloned()
                .map(|person| (*id, *entity, person))
        })
        .collect();
    for (id, entity, person) in people {
        if person.role == Role::Visitor {
            arrival::reserve_visitor(world, id, entity, person);
            continue;
        }
        social::welcome(world, entity);
        housing::establish_resident_home(world, id, &person);
    }
}

/// Called once by the island Effects orchestrator, after action completions.
pub fn tick(world: &mut World) {
    if !world.contains_resource::<ColonyState>() {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    if !now.is_multiple_of(20) {
        return;
    }
    if housing::tick(world, now) {
        super::colony_demand_changed(world);
    }
    social::tick(world, now);
}
