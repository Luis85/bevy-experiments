//! Opt-in regional island composition. Domain modules own their state.
pub mod activities;
pub mod ecology;
pub mod labor;
pub mod routes;
pub mod settlements;
pub mod terrain;
pub mod world_spec;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
pub use settlements::{Settlement, SettlementMember, SettlementSite};
pub use world_spec::{Biome, HomeDef, LivingWorldSpec, SettlementDef, SiteDef};

pub fn enabled(world: &World) -> bool {
    world
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .is_some_and(|w| w.0.living_world.is_some())
}
pub fn spec(world: &World) -> Option<&LivingWorldSpec> {
    world.get_resource::<LivingWorldSpec>()
}
pub fn register(app: &mut App) {
    let Some(definition) = app
        .world()
        .resource::<crate::world_runtime::ActiveWorld>()
        .0
        .living_world
        .clone()
    else {
        return;
    };
    app.insert_resource(definition)
        .register_sim_resource::<LivingWorldSpec>();
    super::catalog::register_frontier(app);
    for verb in super::catalog::FRONTIER_RECIPE_VERBS {
        use npc_framework::agent_io::AgentIoAppExt;
        app.register_action_handler(verb, npc_framework::glue::crafting_agent::craft_handler);
    }
    activities::register(app);
    settlements::register(app);
    terrain::register(app);
    ecology::register(app);
    labor::register(app);
}
pub fn spawn_places(world: &mut World, ids: &mut crate::TestbedIds) {
    settlements::spawn(world, ids);
    terrain::spawn(world);
    ecology::spawn(world, ids);
}
pub fn spawn(world: &mut World) {
    labor::spawn(world);
    super::homestead::spawn(world);
    super::colony::spawn(world);
    super::production::validate(world).expect("regional production graph must be acyclic");
}
pub fn tick(world: &mut World) {
    if enabled(world) {
        activities::tick(world);
        labor::tick(world);
    }
}
