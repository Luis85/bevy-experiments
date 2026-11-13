//! Shared physical environment and perception-scoped planning information.
//! Relocation/depletion in these fixtures is operator setup, not unaided play.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, ResourcePatch, Role, VillageAct, Villager};
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::{Value, json};

fn scene() -> App {
    island_content::new_sim_app(7, "ai_village")
}

fn id(app: &App, name: &str) -> SimId {
    app.world()
        .resource::<island_content::TestbedIds>()
        .id(name)
}

fn entity(app: &App, id: SimId) -> Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

fn observed(app: &App) -> Value {
    observe::observe(app.world(), id(app, "human"), 10)
        .unwrap()
        .0
}

fn resource(observation: &Value, target: SimId) -> Value {
    observation["sections"]["village"]["environment"]["resources"]
        .as_array()
        .expect("structured resource opportunities")
        .iter()
        .find(|r| r["id"] == json!(target))
        .expect("known resource")
        .clone()
}

fn workplace(observation: &Value, target: SimId) -> Value {
    observation["sections"]["village"]["environment"]["workstations"]
        .as_array()
        .expect("structured public workplace directory")
        .iter()
        .find(|place| place["id"] == json!(target))
        .expect("known workplace")
        .clone()
}

fn reach(app: &mut App, target: SimId) {
    let actor = entity(app, id(app, "human"));
    let pos = *app.world().get::<SimPos>(entity(app, target)).unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    run_ticks(app.world_mut(), 40);
}

#[test]
fn environment_information_requires_knowledge_and_reveals_own_home_and_role() {
    let app = scene();
    let before = observed(&app);
    let village = &before["sections"]["village"];
    assert_eq!(village["role"], "visitor");
    let actor = entity(&app, id(&app, "human"));
    assert_eq!(
        village["home"],
        json!(app.world().get::<Villager>(actor).unwrap().home)
    );
    for field in ["resources", "workstations"] {
        assert!(
            village["environment"][field].as_array().unwrap().is_empty(),
            "no perception scan has discovered {field} yet"
        );
    }
}

#[test]
fn sensed_patch_has_actionable_regrowth_information_without_observation_mutating_sim() {
    let mut app = scene();
    let patch = id(&app, "patch_log");
    reach(&mut app, patch);
    let patch_entity = entity(&app, patch);
    {
        let mut field = app
            .world_mut()
            .get_mut::<ResourcePatch>(patch_entity)
            .unwrap();
        field.remaining = 17;
        field.replenish_at = 43_210;
    }
    let hash = state_hash(app.world_mut());
    let view = observed(&app);
    assert_eq!(state_hash(app.world_mut()), hash);
    let field = resource(&view, patch);
    assert_eq!(field["kind"], "log");
    assert_eq!(field["current"], true);
    assert_eq!(field["remaining"], 17);
    assert_eq!(field["capacity"], 60);
    assert_eq!(field["replenish_at"], 43_210);
    assert_eq!(field["gather_ticks"], 240);
}

#[test]
fn remembered_resources_do_not_disclose_remote_depletion_or_regrowth_changes() {
    let mut app = scene();
    let patch = id(&app, "patch_log");
    reach(&mut app, patch);
    let body = entity(&app, id(&app, "human"));
    app.world_mut()
        .entity_mut(body)
        .insert(SimPos::new(-100_000, -100_000));
    run_ticks(app.world_mut(), 40);
    let before = resource(&observed(&app), patch);
    assert_eq!(before["current"], false);
    for field in ["remaining", "capacity", "replenish_at"] {
        assert!(before[field].is_null(), "remote {field} must be unknown");
    }
    let patch_entity = entity(&app, patch);
    {
        let mut field = app
            .world_mut()
            .get_mut::<ResourcePatch>(patch_entity)
            .unwrap();
        field.remaining = 0;
        field.capacity = 31;
        field.replenish_at = 99_999;
    }
    assert_eq!(resource(&observed(&app), patch), before);
    reach(&mut app, patch);
    let after = resource(&observed(&app), patch);
    assert_eq!(after["current"], true);
    assert_eq!(after["remaining"], 0);
    assert_eq!(after["capacity"], 31);
    assert_eq!(after["replenish_at"], 99_999);
}

#[test]
fn a_known_bakery_explains_its_actual_production_chain() {
    let mut app = scene();
    let bakery = id(&app, &format!("village_{}", Role::Baker.tag()));
    reach(&mut app, bakery);
    let view = observed(&app);
    let station = view["sections"]["village"]["environment"]["workstations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|station| station["id"] == json!(bakery))
        .expect("known bakery");
    assert_eq!(station["name"], "Mira's bakery");
    let recipes = station["recipes"].as_array().unwrap();
    assert_eq!(recipes.len(), 1, "only recipes this station can host");
    let recipe = &recipes[0];
    assert_eq!(recipe["name"], village::catalog::BAKE);
    assert_eq!(recipe["duration_ticks"], 240);
    for (kind, count) in [
        (village::catalog::FLOUR, 2),
        (village::catalog::WATER, 1),
        ("log", 1),
    ] {
        assert!(
            recipe["inputs"]
                .as_array()
                .unwrap()
                .contains(&json!({"kind":kind,"count":count}))
        );
    }
    assert_eq!(recipe["outputs"], json!([{"kind":"bread","count":4}]));
}

#[test]
fn known_well_and_inn_offer_public_search_destinations_without_crafting_recipes() {
    let mut app = scene();
    for role in [Role::WaterKeeper, Role::Innkeeper] {
        let place = id(&app, &format!("village_{}", role.tag()));
        reach(&mut app, place);
        let entry = workplace(&observed(&app), place);
        assert_eq!(entry["role"], json!(role));
        assert_eq!(entry["recipes"], json!([]));
        let accepts = entry["accepts"].as_array().unwrap();
        assert!(accepts.contains(&json!("bread")));
        assert!(accepts.contains(&json!(village::catalog::WATER)));
        if role == Role::WaterKeeper {
            assert_eq!(entry["name"], "Nell's well");
            assert_eq!(entry["products"], json!([village::catalog::WATER]));
        } else {
            assert_eq!(entry["name"], "The Lantern Inn");
            assert_eq!(entry["products"], json!([]));
            for kind in ["log", "hammer", village::catalog::BOWL] {
                assert!(accepts.contains(&json!(kind)));
            }
        }
        for private in [
            "resident",
            "position",
            "coins",
            "health",
            "stock",
            "inventory",
        ] {
            assert!(
                entry.get(private).is_none(),
                "place is not live trader tracking: {private}"
            );
        }
    }
}

#[test]
fn remembered_workplace_directory_does_not_track_unseen_residents() {
    let mut app = scene();
    for role in [Role::WaterKeeper, Role::Innkeeper] {
        let place = id(&app, &format!("village_{}", role.tag()));
        reach(&mut app, place);
    }
    let actor = entity(&app, id(&app, "human"));
    app.world_mut()
        .entity_mut(actor)
        .insert(SimPos::new(-100_000, -100_000));
    run_ticks(app.world_mut(), 40);
    let before = observed(&app);
    let directory = before["sections"]["village"]["environment"]["workstations"].clone();
    for role in [Role::WaterKeeper, Role::Innkeeper] {
        let resident = id(&app, role.person());
        assert!(
            !before["sections"]["perception"]["sensed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|percept| percept["id"] == json!(resident))
        );
        let person = entity(&app, resident);
        app.world_mut()
            .entity_mut(person)
            .insert(SimPos::new(80_000, 80_000));
        app.world_mut()
            .get_mut::<village::Wallet>(person)
            .unwrap()
            .coins = 0;
        app.world_mut()
            .get_mut::<village::Vitality>(person)
            .unwrap()
            .health = 1234;
        app.world_mut().get_mut::<Villager>(person).unwrap().name =
            "PRIVATE_RESIDENT_CANARY".into();
        for kind in village::catalog::TRADE_KINDS {
            let count = village::economy::count(app.world(), person, kind);
            if count > 0 {
                village::economy::consume(app.world_mut(), person, kind, count).unwrap();
            }
        }
    }
    let hash = state_hash(app.world_mut());
    let after = observed(&app);
    assert_eq!(
        state_hash(app.world_mut()),
        hash,
        "reading the directory must not change the world"
    );
    assert_eq!(
        after["sections"]["village"]["environment"]["workstations"], directory,
        "a learned public place does not promise a resident's current presence or trade availability"
    );
    assert!(!directory.to_string().contains("PRIVATE_RESIDENT_CANARY"));
}

#[test]
fn observer_resource_timing_is_complete_and_cannot_change_future_simulation() {
    let mut observed = scene();
    let mut control = scene();
    for _ in 0..3 {
        let before = state_hash(observed.world_mut());
        let view = island_content::projection::world_state(observed.world_mut()).unwrap();
        assert_eq!(state_hash(observed.world_mut()), before);
        let resources: Vec<_> = view["entities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "resource")
            .collect();
        assert_eq!(resources.len(), 4);
        for field in resources {
            let id = SimId(field["id"].as_u64().unwrap() as u32);
            let patch = observed
                .world()
                .get::<ResourcePatch>(entity(&observed, id))
                .unwrap();
            assert_eq!(field["gather_ticks"], patch.gather_ticks);
            assert_eq!(field["replenish_at"], patch.replenish_at);
        }
        run_ticks(observed.world_mut(), 80);
        run_ticks(control.world_mut(), 80);
        assert_eq!(
            state_hash(observed.world_mut()),
            state_hash(control.world_mut())
        );
    }
}

fn contested_harvest() -> App {
    let mut app = scene();
    // Freeze unrelated autonomous decisions, preserving the ordinary clock,
    // needs and gather completion systems. Both workers submit the same verb.
    for role in Role::RESIDENTS {
        let body = entity(&app, id(&app, role.person()));
        app.world_mut().entity_mut(body).remove::<Brain>();
        app.world_mut()
            .get_mut::<Villager>(body)
            .unwrap()
            .next_decision = u64::MAX;
    }
    let patch = id(&app, "patch_log");
    let patch_entity = entity(&app, patch);
    let pos = *app.world().get::<SimPos>(patch_entity).unwrap();
    app.world_mut()
        .get_mut::<ResourcePatch>(patch_entity)
        .unwrap()
        .remaining = 1;
    for worker in [id(&app, "human"), id(&app, Role::Farmer.person())] {
        let body = entity(&app, worker);
        app.world_mut().entity_mut(body).insert(pos);
        let mut command = VillageAct::new(worker, "gather");
        command.target = Some(patch);
        submit(app.world_mut(), None, SimCommand::content(&command)).unwrap();
    }
    app
}

#[test]
fn two_recorded_workers_share_the_last_resource_without_duplication() {
    let mut app = contested_harvest();
    let mut workers = [id(&app, "human"), id(&app, Role::Farmer.person())];
    workers.sort();
    let patch = entity(&app, id(&app, "patch_log"));
    run_ticks(app.world_mut(), 1);
    for worker in workers {
        assert!(
            app.world()
                .get::<village::Gathering>(entity(&app, worker))
                .is_some()
        );
    }
    assert_eq!(
        app.world().get::<ResourcePatch>(patch).unwrap().remaining,
        1
    );
    run_ticks(app.world_mut(), 241);
    assert_eq!(
        app.world().get::<ResourcePatch>(patch).unwrap().remaining,
        0
    );
    assert_eq!(
        village::economy::count(app.world(), entity(&app, workers[0]), "log"),
        1
    );
    assert_eq!(
        village::economy::count(app.world(), entity(&app, workers[1]), "log"),
        0
    );
    for worker in workers {
        assert!(
            app.world()
                .get::<village::Gathering>(entity(&app, worker))
                .is_none()
        );
    }
    assert_eq!(
        app.world()
            .resource::<village::VillageState>()
            .produced
            .get("log"),
        Some(&1)
    );
}

#[test]
fn simultaneous_harvest_continues_identically_across_a_save() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(contested_harvest, 40, 400, 40);
}
