//! Physical homestead contracts. Fixture grants/relocation isolate mechanics;
//! they are not evidence of unaided discovery, travel or material acquisition.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::interaction::items;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{Journal, set_journal};
use npc_framework::prelude::*;
use testbed_content::village::{
    self, Villager, catalog, economy,
    homestead::{self, CottageBuild, HomesteadAct, HomesteadPlot},
};

fn fixture() -> (App, SimId, Entity) {
    let mut app = testbed_content::new_sim_app(7, "living_village");
    let people: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Villager)>()
        .iter(app.world())
        .filter(|(_, v)| v.role != village::Role::Visitor)
        .map(|(e, _)| e)
        .collect();
    for person in people {
        app.world_mut().entity_mut(person).remove::<Brain>();
    }
    run_ticks(app.world_mut(), 1);
    let id = app.world().resource::<village::VillageState>().played;
    let actor = app.world().resource::<SimIndex>().get(id).unwrap();
    set_journal(app.world_mut(), true, false);
    (app, id, actor)
}
fn plot(app: &App, use_kind: &str, index: usize) -> (SimId, Entity) {
    homestead::plots(app.world())
        .into_iter()
        .filter(|(_, e)| homestead::definition(app.world(), *e).unwrap().use_kind == use_kind)
        .nth(index)
        .unwrap()
}
fn stand(app: &mut App, actor: Entity, pos: SimPos) {
    app.world_mut()
        .entity_mut(actor)
        .insert(pos)
        .remove::<MoveIntent>();
}
fn center(app: &App, plot: Entity) -> SimPos {
    *app.world().get::<SimPos>(plot).unwrap()
}
fn claim(app: &mut App, actor_id: SimId, actor: Entity, plot_id: SimId, plot: Entity) {
    let pos = center(app, plot);
    stand(app, actor, pos);
    HomesteadAct::new(actor_id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .unwrap();
}
fn materials(app: &mut App, actor: Entity) {
    economy::grant(app.world_mut(), actor, Item::new("plank").stack(8));
    economy::grant(app.world_mut(), actor, Item::new("log").stack(4));
}
fn raw_count(app: &App, actor: Entity, kind: &str) -> u32 {
    items::carried_items(app.world(), actor)
        .into_iter()
        .filter_map(|id| {
            app.world()
                .resource::<SimIndex>()
                .get(id)
                .and_then(|e| app.world().get::<Item>(e))
        })
        .filter(|item| item.kind == kind)
        .map(|item| item.stack)
        .sum()
}
fn events(app: &App, kind: &str) -> usize {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| {
            matches!(&entry.kind,JournalKind::Effect{effect,applied:true}
            if effect.get("type").and_then(serde_json::Value::as_str)==Some(kind))
        })
        .count()
}

#[test]
fn plots_start_unclaimed_with_fixed_empty_beds_and_no_player_gifts() {
    let (app, _, actor) = fixture();
    assert_eq!(homestead::plots(app.world()).len(), 6);
    assert_eq!(app.world().get::<village::Wallet>(actor).unwrap().coins, 0);
    assert_eq!(raw_count(&app, actor, catalog::GRAIN), 0);
    for (_, plot) in homestead::plots(app.world()) {
        let state = app.world().get::<HomesteadPlot>(plot).unwrap();
        assert_eq!(state.owner, None);
        assert!(!state.cottage_built);
        assert!(state.beds.iter().all(Option::is_none));
        assert_eq!(
            state.beds.len(),
            if homestead::definition(app.world(), plot).unwrap().use_kind == "housing" {
                3
            } else {
                6
            }
        );
    }
}

#[test]
fn claims_are_rechecked_and_limited_to_one_house_and_one_garden() {
    let (mut app, actor_id, actor) = fixture();
    let (house_id, house) = plot(&app, "housing", 0);
    claim(&mut app, actor_id, actor, house_id, house);
    assert!(
        HomesteadAct::new(actor_id, "claim_plot", house_id)
            .perform(app.world_mut())
            .is_err()
    );
    let ada_id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    let ada = app.world().resource::<SimIndex>().get(ada_id).unwrap();
    let pos = center(&app, house);
    stand(&mut app, ada, pos);
    assert!(
        HomesteadAct::new(ada_id, "claim_plot", house_id)
            .perform(app.world_mut())
            .is_err()
    );
    let (other_id, other) = plot(&app, "housing", 1);
    let pos = center(&app, other);
    stand(&mut app, actor, pos);
    assert!(
        HomesteadAct::new(actor_id, "claim_plot", other_id)
            .perform(app.world_mut())
            .is_err()
    );
    let (garden_id, garden) = plot(&app, "garden", 0);
    claim(&mut app, actor_id, actor, garden_id, garden);
    assert_eq!(
        app.world().get::<HomesteadPlot>(garden).unwrap().owner,
        Some(actor_id)
    );
    let (second_id, second) = plot(&app, "garden", 1);
    let pos = center(&app, second);
    stand(&mut app, actor, pos);
    assert!(
        HomesteadAct::new(actor_id, "claim_plot", second_id)
            .check(app.world())
            .is_err()
    );
    assert_eq!(events(&app, "VillagePlotClaimed"), 2);
}

#[test]
fn cottage_reserves_actual_materials_spends_only_at_deadline_and_does_not_teleport() {
    let (mut app, actor_id, actor) = fixture();
    let (house_id, house) = plot(&app, "housing", 0);
    claim(&mut app, actor_id, actor, house_id, house);
    materials(&mut app, actor);
    let position = *app.world().get::<SimPos>(actor).unwrap();
    let coins = app.world().get::<village::Wallet>(actor).unwrap().coins;
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&HomesteadAct::new(actor_id, "build_cottage", house_id)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let work = app.world().get::<CottageBuild>(actor).unwrap().clone();
    assert!(
        app.world()
            .resource::<Reservations>()
            .holds(house_id, actor_id)
    );
    assert_eq!(raw_count(&app, actor, "plank"), 8);
    assert_eq!(raw_count(&app, actor, "log"), 4);
    for material in &work.materials {
        assert!(
            app.world()
                .resource::<Reservations>()
                .holds(material.item, actor_id)
        );
    }
    let remaining = work.ends - app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), remaining);
    assert!(
        !app.world()
            .get::<HomesteadPlot>(house)
            .unwrap()
            .cottage_built
    );
    assert_eq!(raw_count(&app, actor, "plank"), 8);
    run_ticks(app.world_mut(), 1);
    let estate = app.world().get::<HomesteadPlot>(house).unwrap();
    assert!(estate.cottage_built);
    assert_eq!(raw_count(&app, actor, "plank"), 0);
    assert_eq!(raw_count(&app, actor, "log"), 0);
    assert_eq!(*app.world().get::<SimPos>(actor).unwrap(), position);
    assert_eq!(
        app.world().get::<Villager>(actor).unwrap().home,
        homestead::front_door(center(&app, house))
    );
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        coins
    );
    assert!(
        app.world()
            .resource::<Reservations>()
            .held_by(actor_id)
            .is_empty()
    );
    let cottage = app
        .world()
        .resource::<SimIndex>()
        .get(estate.cottage_entity.unwrap())
        .unwrap();
    assert!(app.world().get::<GridBlocker>(cottage).is_some());
    run_ticks(app.world_mut(), 1);
    let grid = app.world().resource::<SpatialGrid>();
    assert!(
        !grid.is_blocking(grid.cell_of(position)),
        "builder and door stay outside the real cottage blocker"
    );
    assert!(grid.is_blocking(grid.cell_of(homestead::cottage_center(center(&app, house)))));
    assert_eq!(events(&app, "VillageCottageBuilt"), 1);
}

#[test]
fn cancelled_failed_and_dead_builders_release_claims_without_losing_materials() {
    for cause in ["cancel", "leave", "death"] {
        let (mut app, actor_id, actor) = fixture();
        let (house_id, house) = plot(&app, "housing", 0);
        claim(&mut app, actor_id, actor, house_id, house);
        materials(&mut app, actor);
        HomesteadAct::new(actor_id, "build_cottage", house_id)
            .perform(app.world_mut())
            .unwrap();
        let materials = app
            .world()
            .get::<CottageBuild>(actor)
            .unwrap()
            .materials
            .clone();
        match cause {
            "cancel" => homestead::cancel(app.world_mut(), actor),
            "leave" => stand(&mut app, actor, SimPos::new(500, -1500)),
            "death" => {
                app.world_mut()
                    .get_mut::<village::Vitality>(actor)
                    .unwrap()
                    .health = 0
            }
            _ => unreachable!(),
        }
        run_ticks(app.world_mut(), u64::from(homestead::BUILD_TICKS) + 1);
        assert!(
            !app.world()
                .get::<HomesteadPlot>(house)
                .unwrap()
                .cottage_built,
            "{cause}"
        );
        assert!(app.world().get::<CottageBuild>(actor).is_none(), "{cause}");
        assert!(
            app.world()
                .resource::<Reservations>()
                .held_by(actor_id)
                .is_empty(),
            "{cause}"
        );
        for material in materials {
            let entity = app
                .world()
                .resource::<SimIndex>()
                .get(material.item)
                .expect("the actual material entity survives cancellation/death");
            assert!(
                app.world().get::<Item>(entity).unwrap().stack >= material.count,
                "{cause}"
            );
        }
    }
}

#[test]
fn crop_cycle_spends_one_seed_and_water_then_yields_three_once_with_atomic_capacity_checks() {
    let (mut app, actor_id, actor) = fixture();
    let (garden_id, garden) = plot(&app, "garden", 0);
    claim(&mut app, actor_id, actor, garden_id, garden);
    let bed = homestead::bed_position(app.world(), garden, 0).unwrap();
    stand(&mut app, actor, bed);
    economy::grant(app.world_mut(), actor, Item::new(catalog::GRAIN));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    HomesteadAct::new(actor_id, "plant", garden_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::GRAIN), 0);
    assert!(
        HomesteadAct::new(actor_id, "plant", garden_id)
            .check(app.world())
            .is_err()
    );
    assert!(
        HomesteadAct::new(actor_id, "harvest", garden_id)
            .check(app.world())
            .is_err()
    );
    HomesteadAct::new(actor_id, "water_crop", garden_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::WATER), 0);
    assert!(
        HomesteadAct::new(actor_id, "water_crop", garden_id)
            .check(app.world())
            .is_err()
    );
    run_ticks(app.world_mut(), u64::from(homestead::GROW_TICKS));
    assert_eq!(
        events(&app, "VillageCropReady"),
        0,
        "deadline Effects have not run yet"
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(events(&app, "VillageCropReady"), 1);
    run_ticks(app.world_mut(), 20);
    assert_eq!(
        events(&app, "VillageCropReady"),
        1,
        "ready is announced once"
    );
    app.world_mut()
        .get_mut::<Inventory>(actor)
        .unwrap()
        .max_weight_g = 0;
    let hash = state_hash(app.world_mut());
    assert!(
        HomesteadAct::new(actor_id, "harvest", garden_id)
            .perform(app.world_mut())
            .is_err()
    );
    assert_eq!(state_hash(app.world_mut()), hash);
    assert!(app.world().get::<HomesteadPlot>(garden).unwrap().beds[0].is_some());
    app.world_mut()
        .get_mut::<Inventory>(actor)
        .unwrap()
        .max_weight_g = 30_000;
    HomesteadAct::new(actor_id, "harvest", garden_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(economy::count(app.world(), actor, catalog::GRAIN), 3);
    assert!(app.world().get::<HomesteadPlot>(garden).unwrap().beds[0].is_none());
    assert!(
        HomesteadAct::new(actor_id, "harvest", garden_id)
            .perform(app.world_mut())
            .is_err()
    );
    assert_eq!(economy::count(app.world(), actor, catalog::GRAIN), 3);
}

fn growing() -> App {
    let (mut app, actor_id, actor) = fixture();
    let (garden_id, garden) = plot(&app, "garden", 0);
    claim(&mut app, actor_id, actor, garden_id, garden);
    let pos = homestead::bed_position(app.world(), garden, 0).unwrap();
    stand(&mut app, actor, pos);
    economy::grant(app.world_mut(), actor, Item::new(catalog::GRAIN));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    HomesteadAct::new(actor_id, "plant", garden_id)
        .perform(app.world_mut())
        .unwrap();
    HomesteadAct::new(actor_id, "water_crop", garden_id)
        .perform(app.world_mut())
        .unwrap();
    app
}

#[test]
fn saved_growing_beds_resume_with_identical_crop_and_ready_event_state() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(growing, 77, 6100, 100);
}

#[test]
fn competing_recorded_claims_cannot_take_the_same_plot_twice() {
    let (mut app, actor_id, actor) = fixture();
    let (plot_id, plot) = plot(&app, "housing", 0);
    let pos = center(&app, plot);
    stand(&mut app, actor, pos);
    let other_id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    let other = app.world().resource::<SimIndex>().get(other_id).unwrap();
    stand(&mut app, other, pos);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&HomesteadAct::new(actor_id, "claim_plot", plot_id)),
    )
    .unwrap();
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&HomesteadAct::new(other_id, "claim_plot", plot_id)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().get::<HomesteadPlot>(plot).unwrap().owner,
        Some(actor_id)
    );
    assert_eq!(events(&app, "VillagePlotClaimed"), 1);
}

#[test]
fn reclaiming_a_dead_owners_built_estate_preserves_its_house_and_crops() {
    let (mut app, actor_id, actor) = fixture();
    let (house_id, house) = plot(&app, "housing", 0);
    claim(&mut app, actor_id, actor, house_id, house);
    materials(&mut app, actor);
    HomesteadAct::new(actor_id, "build_cottage", house_id)
        .perform(app.world_mut())
        .unwrap();
    run_ticks(app.world_mut(), u64::from(homestead::BUILD_TICKS) + 1);
    let pos = homestead::bed_position(app.world(), house, 0).unwrap();
    stand(&mut app, actor, pos);
    economy::grant(app.world_mut(), actor, Item::new(catalog::GRAIN));
    economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
    HomesteadAct::new(actor_id, "plant", house_id)
        .perform(app.world_mut())
        .unwrap();
    HomesteadAct::new(actor_id, "water_crop", house_id)
        .perform(app.world_mut())
        .unwrap();
    let old = app.world().get::<HomesteadPlot>(house).unwrap().clone();
    app.world_mut()
        .get_mut::<village::Vitality>(actor)
        .unwrap()
        .died_at = Some(0);
    let other_id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    let other = app.world().resource::<SimIndex>().get(other_id).unwrap();
    let position = homestead::front_door(center(&app, house));
    stand(&mut app, other, position);
    HomesteadAct::new(other_id, "claim_plot", house_id)
        .perform(app.world_mut())
        .unwrap();
    let inherited = app.world().get::<HomesteadPlot>(house).unwrap();
    assert_eq!(inherited.owner, Some(other_id));
    assert_eq!(inherited.cottage_entity, old.cottage_entity);
    assert_eq!(inherited.beds, old.beds);
    assert_eq!(app.world().get::<Villager>(other).unwrap().home, position);
    assert_eq!(*app.world().get::<SimPos>(other).unwrap(), position);
}

fn building() -> App {
    let (mut app, actor_id, actor) = fixture();
    let (house_id, house) = plot(&app, "housing", 0);
    claim(&mut app, actor_id, actor, house_id, house);
    materials(&mut app, actor);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&HomesteadAct::new(actor_id, "build_cottage", house_id)),
    )
    .unwrap();
    app
}

#[test]
fn running_construction_claims_materials_and_new_home_resume_identically() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(building, 77, 800, 40);
}

#[test]
fn a_resident_uses_the_same_claim_and_construction_rules_as_the_visitor() {
    let (mut app, _, _) = fixture();
    let actor_id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
    let (house_id, house) = plot(&app, "housing", 0);
    claim(&mut app, actor_id, actor, house_id, house);
    assert!(
        HomesteadAct::new(actor_id, "build_cottage", house_id)
            .check(app.world())
            .is_err()
    );
    materials(&mut app, actor);
    let coins = app.world().get::<village::Wallet>(actor).unwrap().coins;
    HomesteadAct::new(actor_id, "build_cottage", house_id)
        .perform(app.world_mut())
        .unwrap();
    run_ticks(app.world_mut(), u64::from(homestead::BUILD_TICKS) + 1);
    assert!(
        app.world()
            .get::<HomesteadPlot>(house)
            .unwrap()
            .cottage_built
    );
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        coins
    );
    assert_eq!(raw_count(&app, actor, "plank"), 0);
    assert_eq!(raw_count(&app, actor, "log"), 0);
}
