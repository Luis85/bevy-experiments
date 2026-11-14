//! Opt-in regional island composition. Domain modules own their state.
pub mod activities;
pub mod building_spec;
pub mod ecology;
pub mod economy_spec;
pub mod labor;
pub mod life_spec;
pub mod oversee;
pub mod props;
pub mod routes;
pub mod settlements;
pub mod shape;
pub mod terrain;
pub mod world_spec;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
pub use settlements::{Settlement, SettlementMember, SettlementSite};
pub use shape::{PlaceResourcePatch, RemoveResourcePatch, export_spec, export_world};
pub use world_spec::{Biome, HomeDef, LivingWorldSpec, SettlementDef, SiteDef};

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
    let economy = economy_spec::EconomyRegistry::new(&definition.economy);
    // The roles of goods and trades, from the data (validated at load).
    let roles =
        super::roles::from_spec(&definition.economy).unwrap_or_else(|error| panic!("{error}"));
    app.insert_resource(definition)
        .insert_resource(economy)
        .insert_resource(super::roles::WorldRoles(roles))
        .register_sim_resource::<LivingWorldSpec>();
    super::catalog::register_frontier(app);
    activities::register(app);
    settlements::register(app);
    terrain::register(app);
    ecology::register(app);
    labor::register(app);
    oversee::register(app);
    props::register(app);
    super::civic::register(app);
    // A pure memo of route searches; see `routes::RouteMemo`.
    app.init_resource::<routes::RouteMemo>();
    super::weather::register(app);
    super::fields::register(app);
    super::merchant::register(app);
    super::demography::register(app);
    super::indoors::register(app);
    super::live::register(app);
    super::stock::register(app);
    super::ledger::register(app);
    shape::register(app);
    app.register_command::<shape::PlaceResourcePatch>()
        .register_command::<shape::RemoveResourcePatch>()
        .register_command::<shape::PlacePlot>()
        .register_command::<shape::RemovePlot>();
}
pub fn spawn_places(world: &mut World, ids: &mut crate::IslandIds) {
    settlements::spawn(world, ids);
    terrain::spawn(world);
    ecology::spawn(world, ids);
}
pub fn spawn(world: &mut World) {
    labor::spawn(world);
    // Each founder runs the tree its trade or stage names (S3.1).
    for (_, actor) in super::demography::residents(world) {
        super::resident::assign_tree(world, actor);
    }
    super::homestead::spawn(world);
    super::colony::spawn(world);
    // Planner memory only: no entity is spawned, so no identity moves.
    super::civic::spawn(world);
    // Hashed state only: ages of the founders and the merchant's purse. The
    // founding fields and the trading post are laid out at the first tick, so
    // every identity of the authored world stays where it was.
    super::demography::spawn(world);
    super::merchant::spawn(world);
    // An exported world's own buildings and sites, as they stood.
    let placed = spec(world)
        .map(|spec| spec.buildings.clone())
        .unwrap_or_default();
    for building in placed {
        if let Err(reason) = super::civic::lay_out(
            world,
            &building.building,
            building.x,
            building.z,
            building.rotation,
            building.finished,
        ) {
            record_effect(
                world,
                None,
                "RegionalBuildingNotLaidOut",
                &serde_json::json!({"building":building.building,"x":building.x,"z":building.z,
                    "reason":reason}),
            );
        }
    }
    // An exported world's props, after its buildings.
    props::spawn(world);
    super::production::validate(world).expect("regional production graph must be acyclic");
}
pub fn tick(world: &mut World) {
    super::ledger::tick(world);
    activities::tick(world);
    labor::tick(world);
    super::weather::tick(world);
    super::spoilage::tick(world);
    super::fields::tick(world);
    super::civic::tick(world);
    super::merchant::tick(world);
    super::demography::tick(world);
    super::indoors::tick(world);
    super::live::tick(world);
    super::living::wear_carried_coats(world);
}
