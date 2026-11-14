//! Fixture gifts/relocation isolate material ownership, cancellation and persistence.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, VillageAct, VillageState, catalog,
    colony::JoinIsland,
    economy,
    frontier::{
        activities::{EcologyWork, FrontierAct, WORK_TICKS},
        ecology::EcologicalPatch,
    },
};
use npc_framework::{kernel::hash::state_hash, prelude::*, snapshot};

fn scene() -> App {
    let mut doc = island_content::scenario::scenario_doc("living_world").unwrap();
    let preset = island_content::worlds::builtin_preset("living_world").unwrap();
    island_content::construction::Construction::for_world(
        "human",
        "human",
        &island_content::actors::human_preset(),
        "living_world",
        &preset,
    )
    .write_to(&mut doc)
    .unwrap();
    island_content::load_scenario(&doc, 7).unwrap().app
}
fn fixture() -> (App, SimId, Entity, SimId, Entity) {
    let mut app = scene();
    let actors: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .collect();
    for actor in actors {
        app.world_mut().entity_mut(actor).remove::<Brain>();
    }
    let id = app.world().resource::<VillageState>().played;
    JoinIsland { unit: id.0 }.perform(app.world_mut()).unwrap();
    let actor = app.world().resource::<SimIndex>().get(id).unwrap();
    let (patch, site) = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, e)| {
            app.world()
                .get::<village::ResourcePatch>(**e)
                .is_some_and(|p| p.kind == "log")
        })
        .map(|(id, e)| (*id, *e))
        .unwrap();
    let pos = *app.world().get::<SimPos>(site).unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    app.world_mut()
        .get_mut::<EcologicalPatch>(site)
        .unwrap()
        .biomass_milli = 50000;
    (app, id, actor, patch, site)
}
fn issue(world: &mut World, command: &impl ContentCommand) {
    submit(world, None, SimCommand::content(command)).unwrap();
    run_ticks(world, 1);
}
#[test]
fn restoration_rejects_missing_materials_and_cancellation_returns_exact_claims() {
    let (mut app, id, actor, patch, site) = fixture();
    let command = FrontierAct {
        unit: id,
        target: patch,
        verb: "plant_tree".into(),
    };
    assert!(command.check(app.world()).is_err());
    economy::grant(app.world_mut(), actor, Item::new("log"));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    let logs = economy::count(app.world(), actor, "log");
    let water = economy::count(app.world(), actor, catalog::WATER);
    issue(app.world_mut(), &command);
    assert!(app.world().get::<EcologyWork>(actor).is_some());
    assert!(app.world().resource::<Reservations>().holds(patch, id));
    assert!(command.check(app.world()).is_err());
    issue(app.world_mut(), &VillageAct::new(id, "stop_work"));
    assert!(app.world().get::<EcologyWork>(actor).is_none());
    assert_eq!(app.world().resource::<Reservations>().count(patch), 0);
    assert_eq!(economy::count(app.world(), actor, "log"), logs);
    assert_eq!(economy::count(app.world(), actor, catalog::WATER), water);
    assert_eq!(
        app.world()
            .get::<EcologicalPatch>(site)
            .unwrap()
            .planted_units,
        0
    );
}
#[test]
fn committed_restoration_resumes_exactly_and_consumes_once_before_delayed_growth() {
    let (mut app, id, actor, patch, site) = fixture();
    economy::grant(app.world_mut(), actor, Item::new("log"));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    let logs = economy::count(app.world(), actor, "log");
    let water = economy::count(app.world(), actor, catalog::WATER);
    let stock = app
        .world()
        .get::<village::ResourcePatch>(site)
        .unwrap()
        .remaining;
    issue(
        app.world_mut(),
        &FrontierAct {
            unit: id,
            target: patch,
            verb: "plant_tree".into(),
        },
    );
    run_ticks(app.world_mut(), WORK_TICKS / 2);
    let saved = snapshot::save(app.world_mut()).unwrap();
    let mut resumed = scene();
    snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).unwrap();
    run_ticks(app.world_mut(), WORK_TICKS);
    run_ticks(resumed.world_mut(), WORK_TICKS);
    assert_eq!(state_hash(app.world_mut()), state_hash(resumed.world_mut()));
    assert_eq!(economy::count(app.world(), actor, "log"), logs - 1);
    assert_eq!(
        economy::count(app.world(), actor, catalog::WATER),
        water - 1
    );
    assert_eq!(
        app.world()
            .get::<village::ResourcePatch>(site)
            .unwrap()
            .remaining,
        stock
    );
    let state = app.world().get::<EcologicalPatch>(site).unwrap();
    assert_eq!(state.planted_units, 1);
    assert_eq!(state.pending_growth.len(), 1);
    assert!(state.pending_growth[0].matures_at > app.world().resource::<SimClock>().tick);
    assert!(app.world().get::<EcologyWork>(actor).is_none());
}

#[test]
fn persisted_restoration_rejects_missing_duplicate_and_wrong_material_bundles() {
    let (mut app, id, actor, patch, _) = fixture();
    economy::grant(app.world_mut(), actor, Item::new("log"));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    issue(
        app.world_mut(),
        &FrontierAct {
            unit: id,
            target: patch,
            verb: "plant_tree".into(),
        },
    );
    let valid = app.world().get::<EcologyWork>(actor).unwrap().clone();
    use island_content::village::frontier::activities::validate_work;
    validate_work(app.world(), actor, &valid).unwrap();
    let mut missing = valid.clone();
    missing.materials.clear();
    let mut duplicate = valid.clone();
    duplicate.materials.push(duplicate.materials[0].clone());
    let mut wrong = valid.clone();
    wrong.materials[0].count += 1;
    for invalid in [missing, duplicate, wrong] {
        assert!(validate_work(app.world(), actor, &invalid).is_err());
    }
    validate_work(app.world(), actor, &valid).unwrap();
}
#[test]
fn fishing_requires_a_real_nonconsumed_rod_for_the_shared_gather_action() {
    let (mut app, id, actor, _, _) = fixture();
    let (patch, site) = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, e)| {
            app.world()
                .get::<village::ResourcePatch>(**e)
                .is_some_and(|p| p.kind == catalog::FISH)
        })
        .map(|(id, e)| (*id, *e))
        .unwrap();
    let pos = *app.world().get::<SimPos>(site).unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    let mut fish = VillageAct::new(id, "gather");
    fish.target = Some(patch);
    assert!(fish.check(app.world()).unwrap_err().contains("fishing rod"));
    economy::grant(app.world_mut(), actor, Item::new(catalog::FISHING_ROD));
    issue(app.world_mut(), &fish);
    run_ticks(app.world_mut(), 240);
    assert_eq!(economy::count(app.world(), actor, catalog::FISH), 1);
    assert_eq!(economy::count(app.world(), actor, catalog::FISHING_ROD), 1);
    assert_eq!(
        app.world()
            .get::<EcologicalPatch>(site)
            .unwrap()
            .harvested_units,
        1
    );
    assert!(
        catalog::food_restore_in(app.world(), catalog::FISH).is_none(),
        "raw fish requires cooking"
    );
}
