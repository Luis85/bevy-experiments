//! Real crop inputs, distinct food harvests, scoped rules and saved growth.
//! Fixture grants isolate crop mechanics; they are not unaided-play evidence.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, catalog,
    colony::JoinIsland,
    economy,
    homestead::{self, HomesteadAct, HomesteadPlot},
};
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;

fn fixture() -> (App, SimId, Entity, SimId, Entity) {
    let mut app = island_content::new_sim_app(7, "island_colony");
    let id = app.world().resource::<village::VillageState>().played;
    JoinIsland { unit: id.0 }.perform(app.world_mut()).unwrap();
    let actor = app.world().resource::<SimIndex>().get(id).unwrap();
    let residents: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| app.world().get::<village::Villager>(*entity).is_some())
        .collect();
    for entity in residents {
        app.world_mut().entity_mut(entity).remove::<Brain>();
    }
    let (plot_id, plot) = homestead::plots(app.world())
        .into_iter()
        .find(|(_, plot)| homestead::definition(app.world(), *plot).unwrap().use_kind == "garden")
        .unwrap();
    let door = *app.world().get::<SimPos>(plot).unwrap();
    app.world_mut().entity_mut(actor).insert(door);
    HomesteadAct::new(id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .unwrap();
    let bed = homestead::bed_position(app.world(), plot, 0).unwrap();
    app.world_mut().entity_mut(actor).insert(bed);
    (app, id, actor, plot_id, plot)
}

#[test]
fn crop_catalog_extends_colony_without_changing_legacy_grain_rules() {
    let mut colony = island_content::new_sim_app(7, "island_colony");
    let before = state_hash(colony.world_mut());
    let grains = homestead::crop_definition(colony.world(), catalog::GRAIN).unwrap();
    assert_eq!(
        (
            grains.seed_count,
            grains.water_count,
            grains.growth_ticks,
            grains.yield_count
        ),
        (1, 1, 6000, 3)
    );
    let berries = homestead::crop_definition(colony.world(), catalog::BERRIES).unwrap();
    assert_eq!(
        (
            berries.seed_kind,
            berries.growth_ticks,
            berries.yield_kind,
            berries.yield_count
        ),
        (catalog::BERRIES, 9000, catalog::BERRIES, 4)
    );
    assert_eq!(homestead::crop_catalog(colony.world()).len(), 2);
    assert_eq!(state_hash(colony.world_mut()), before);
    let legacy = island_content::new_sim_app(7, "living_village");
    assert_eq!(homestead::crop_catalog(legacy.world()).len(), 1);
    assert!(homestead::crop_definition(legacy.world(), catalog::BERRIES).is_none());
}

#[test]
fn berry_bed_consumes_real_seed_water_and_yields_usable_food_only_after_growth() {
    let (mut app, id, actor, plot_id, plot) = fixture();
    let mut plant = HomesteadAct::new(id, "plant", plot_id);
    plant.kind = catalog::BERRIES.into();
    let before = state_hash(app.world_mut());
    assert!(plant.perform(app.world_mut()).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    economy::grant(app.world_mut(), actor, Item::new(catalog::BERRIES));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    plant.perform(app.world_mut()).unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::BERRIES), 0);
    assert_eq!(
        app.world().get::<HomesteadPlot>(plot).unwrap().beds[0]
            .as_ref()
            .unwrap()
            .kind,
        catalog::BERRIES
    );
    HomesteadAct::new(id, "water_crop", plot_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::WATER), 0);
    assert_eq!(
        app.world().get::<HomesteadPlot>(plot).unwrap().beds[0]
            .as_ref()
            .unwrap()
            .ready_at,
        Some(9000)
    );
    let harvest = HomesteadAct::new(id, "harvest", plot_id);
    assert!(harvest.check(app.world()).is_err());
    run_ticks(app.world_mut(), 9000);
    let pocket = app.world().get::<village::Wallet>(actor).unwrap().coins;
    harvest.perform(app.world_mut()).unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::BERRIES), 4);
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        pocket
    );
    assert!(app.world().get::<HomesteadPlot>(plot).unwrap().beds[0].is_none());
    let hunger = economy::need(app.world(), actor, "Hunger");
    let mut eat = village::VillageAct::new(id, "eat");
    eat.kind = catalog::BERRIES.into();
    eat.perform(app.world_mut()).unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::BERRIES), 3);
    assert!(economy::need(app.world(), actor, "Hunger") > hunger);
}

#[test]
fn full_pack_rejects_crop_harvest_atomically() {
    let (mut app, id, actor, plot_id, plot) = fixture();
    economy::grant(app.world_mut(), actor, Item::new(catalog::BERRIES));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    let mut plant = HomesteadAct::new(id, "plant", plot_id);
    plant.kind = catalog::BERRIES.into();
    plant.perform(app.world_mut()).unwrap();
    HomesteadAct::new(id, "water_crop", plot_id)
        .perform(app.world_mut())
        .unwrap();
    run_ticks(app.world_mut(), 9000);
    app.world_mut()
        .get_mut::<Inventory>(actor)
        .unwrap()
        .max_weight_g = 0;
    let before = state_hash(app.world_mut());
    assert!(
        HomesteadAct::new(id, "harvest", plot_id)
            .perform(app.world_mut())
            .is_err()
    );
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(app.world().get::<HomesteadPlot>(plot).unwrap().beds[0].is_some());
}

fn growing_berries() -> App {
    let (mut app, id, actor, plot_id, _) = fixture();
    economy::grant(app.world_mut(), actor, Item::new(catalog::BERRIES));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    let mut plant = HomesteadAct::new(id, "plant", plot_id);
    plant.kind = catalog::BERRIES.into();
    plant.perform(app.world_mut()).unwrap();
    HomesteadAct::new(id, "water_crop", plot_id)
        .perform(app.world_mut())
        .unwrap();
    app
}

#[test]
fn berry_growth_and_ready_state_resume_exactly() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(growing_berries, 111, 9100, 91);
}

#[test]
fn crop_rules_are_public_but_do_not_reveal_unknown_estates() {
    let mut app = island_content::new_sim_app(7, "island_colony");
    let id = app.world().resource::<village::VillageState>().played;
    JoinIsland { unit: id.0 }.perform(app.world_mut()).unwrap();
    let before = state_hash(app.world_mut());
    let value = observe::observe(app.world(), id, 0).unwrap().0;
    assert_eq!(value["sections"]["homestead"]["plots"], json!([]));
    let definitions = value["sections"]["homestead"]["crops"].as_array().unwrap();
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[1]["kind"], catalog::BERRIES);
    assert_eq!(state_hash(app.world_mut()), before);
}
