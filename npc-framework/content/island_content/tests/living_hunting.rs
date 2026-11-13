//! Operator-arranged physical hunting fixtures, never provider or unaided-play evidence.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, VillageAct, Villager, Vitality, Wallet, hunting};
use npc_framework::interaction::items;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;

fn fixture() -> (App, SimId, Entity, SimId) {
    let mut app = island_content::new_sim_app(7, "living_village");
    let people: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Villager)>()
        .iter(app.world())
        .filter(|(_, person)| person.role != village::Role::Visitor)
        .map(|(entity, _)| entity)
        .collect();
    for person in people {
        app.world_mut().entity_mut(person).remove::<Brain>();
    }
    run_ticks(app.world_mut(), 1);
    let actor_id = app.world().resource::<village::VillageState>().played;
    let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
    let (prey, _, pos) = hunting::available_prey(app.world())
        .into_iter()
        .next()
        .unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    (app, actor_id, actor, prey)
}

fn spear(app: &mut App, actor: Entity) -> SimId {
    village::economy::grant(app.world_mut(), actor, Item::new(village::catalog::SPEAR));
    items::carried_matching(app.world(), actor, village::catalog::SPEAR).unwrap()
}

fn command(actor: SimId, prey: SimId) -> VillageAct {
    let mut command = VillageAct::new(actor, "hunt");
    command.target = Some(prey);
    command
}

#[test]
fn hunting_requires_a_living_reachable_prey_and_a_real_carried_spear() {
    let (mut app, actor_id, actor, prey) = fixture();
    let act = command(actor_id, prey);
    let hash = state_hash(app.world_mut());
    assert!(act.perform(app.world_mut()).unwrap_err().contains("spear"));
    assert_eq!(state_hash(app.world_mut()), hash);
    spear(&mut app, actor);
    let person = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("Ada");
    assert!(
        command(actor_id, person)
            .check(app.world())
            .unwrap_err()
            .contains("wild animal")
    );
    let original = *app.world().get::<SimPos>(actor).unwrap();
    app.world_mut()
        .entity_mut(actor)
        .insert(SimPos::new(original.x + 1501, original.z));
    assert!(act.check(app.world()).unwrap_err().contains("1.5 m"));
    app.world_mut().entity_mut(actor).insert(original);
    let prey_entity = app.world().resource::<SimIndex>().get(prey).unwrap();
    app.world_mut()
        .get_mut::<Vitality>(prey_entity)
        .unwrap()
        .died_at = Some(0);
    assert!(act.check(app.world()).unwrap_err().contains("living"));
}

#[test]
fn recorded_hunt_spends_one_spear_after_work_and_grants_only_physical_yield() {
    let (mut app, actor_id, actor, prey) = fixture();
    let tool = spear(&mut app, actor);
    let coins = app.world().get::<Wallet>(actor).unwrap().coins;
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&command(actor_id, prey)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let work = *app.world().get::<hunting::Hunting>(actor).unwrap();
    assert_eq!(work.target, prey);
    assert_eq!(work.tool, tool);
    assert!(app.world().resource::<Reservations>().holds(prey, actor_id));
    assert!(app.world().resource::<Reservations>().holds(tool, actor_id));
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        0
    );
    let remaining = work.ends - app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), remaining);
    assert_eq!(app.world().resource::<SimClock>().tick, work.ends);
    assert!(app.world().get::<hunting::Hunting>(actor).is_some());
    assert!(app.world().resource::<SimIndex>().get(prey).is_some());
    assert!(app.world().resource::<SimIndex>().get(tool).is_some());
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        0
    );
    // run_ticks executes the current tick before advancing the clock. The
    // deadline has been reached, but its Effects have not run until this step.
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<hunting::Hunting>(actor).is_none());
    assert!(app.world().resource::<SimIndex>().get(prey).is_none());
    assert!(app.world().resource::<SimIndex>().get(tool).is_none());
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        2
    );
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::HIDE),
        1
    );
    assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, coins);
    assert!(
        app.world()
            .resource::<Reservations>()
            .held_by(actor_id)
            .is_empty()
    );
}

#[test]
fn cancellation_and_an_interrupted_target_cannot_duplicate_yield_or_consume_the_spear() {
    for interrupt in ["stop", "target_gone", "out_of_reach", "full_pack"] {
        let (mut app, actor_id, actor, prey) = fixture();
        let tool = spear(&mut app, actor);
        command(actor_id, prey).perform(app.world_mut()).unwrap();
        match interrupt {
            "stop" => VillageAct::new(actor_id, "stop_work")
                .perform(app.world_mut())
                .unwrap(),
            "target_gone" => {
                let target = app.world().resource::<SimIndex>().get(prey).unwrap();
                app.world_mut().despawn(target);
            }
            "out_of_reach" => {
                let pos = *app.world().get::<SimPos>(actor).unwrap();
                app.world_mut()
                    .entity_mut(actor)
                    .insert(SimPos::new(pos.x + 10_000, pos.z));
            }
            "full_pack" => {
                app.world_mut()
                    .get_mut::<Inventory>(actor)
                    .unwrap()
                    .max_weight_g = 0
            }
            _ => unreachable!(),
        }
        run_ticks(app.world_mut(), u64::from(hunting::HUNT_TICKS) + 1);
        assert!(
            app.world().get::<hunting::Hunting>(actor).is_none(),
            "{interrupt}"
        );
        assert!(
            app.world().resource::<SimIndex>().get(tool).is_some(),
            "{interrupt}"
        );
        assert!(
            app.world()
                .resource::<Reservations>()
                .held_by(actor_id)
                .is_empty(),
            "{interrupt}"
        );
        assert_eq!(
            village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
            0,
            "{interrupt}"
        );
        assert_eq!(
            village::economy::count(app.world(), actor, village::catalog::HIDE),
            0,
            "{interrupt}"
        );
    }
}

#[test]
fn a_second_actor_cannot_claim_the_same_finite_prey() {
    let (mut app, actor_id, actor, prey) = fixture();
    let other_id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("Ada");
    let other = app.world().resource::<SimIndex>().get(other_id).unwrap();
    let pos = *app.world().get::<SimPos>(actor).unwrap();
    app.world_mut().entity_mut(other).insert(pos);
    spear(&mut app, actor);
    spear(&mut app, other);
    command(actor_id, prey).perform(app.world_mut()).unwrap();
    assert!(command(other_id, prey).perform(app.world_mut()).is_err());
    run_ticks(app.world_mut(), u64::from(hunting::HUNT_TICKS));
    assert!(app.world().get::<hunting::Hunting>(actor).is_some());
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        0
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        2
    );
    assert_eq!(
        village::economy::count(app.world(), other, village::catalog::RAW_MEAT),
        0
    );
    assert_eq!(
        village::economy::count(app.world(), other, village::catalog::SPEAR),
        1
    );
}

#[test]
fn hunt_capacity_accounts_for_the_exact_spear_spent_on_completion() {
    let (mut app, actor_id, actor, prey) = fixture();
    app.world_mut()
        .entity_mut(actor)
        .insert(Inventory::new(2, 1500));
    spear(&mut app, actor);
    command(actor_id, prey).perform(app.world_mut()).unwrap();
    run_ticks(app.world_mut(), u64::from(hunting::HUNT_TICKS));
    assert!(app.world().get::<hunting::Hunting>(actor).is_some());
    assert_eq!(app.world().get::<Inventory>(actor).unwrap().items.len(), 1);
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        0
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(items::carried_weight(app.world(), actor), 1500);
    assert_eq!(app.world().get::<Inventory>(actor).unwrap().items.len(), 2);
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::RAW_MEAT),
        2
    );
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::HIDE),
        1
    );
}

#[test]
fn wildlife_replenishes_only_at_its_boundary_with_a_fresh_individual_identity() {
    let (mut app, actor_id, actor, prey) = fixture();
    let population = hunting::available_prey(app.world()).len();
    spear(&mut app, actor);
    command(actor_id, prey).perform(app.world_mut()).unwrap();
    run_ticks(app.world_mut(), u64::from(hunting::HUNT_TICKS));
    assert!(app.world().get::<hunting::Hunting>(actor).is_some());
    assert_eq!(hunting::available_prey(app.world()).len(), population);
    run_ticks(app.world_mut(), 1);
    assert_eq!(hunting::available_prey(app.world()).len(), population - 1);
    let habitat_id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("wildlife_habitat");
    let habitat = app.world().resource::<SimIndex>().get(habitat_id).unwrap();
    let boundary = app
        .world()
        .get::<hunting::WildlifeHabitat>(habitat)
        .unwrap()
        .renew_at;
    app.world_mut().resource_mut::<SimClock>().tick = boundary - 2;
    run_ticks(app.world_mut(), 1);
    assert_eq!(hunting::available_prey(app.world()).len(), population - 1);
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().resource::<SimClock>().tick, boundary);
    assert_eq!(hunting::available_prey(app.world()).len(), population - 1);
    run_ticks(app.world_mut(), 1);
    let renewed = hunting::available_prey(app.world());
    assert_eq!(renewed.len(), population);
    assert!(renewed.iter().all(|(id, _, _)| *id != prey));
    assert!(renewed.iter().any(|(id, _, _)| *id > prey));
}

fn running_hunt() -> App {
    let (mut app, actor_id, actor, prey) = fixture();
    spear(&mut app, actor);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&command(actor_id, prey)),
    )
    .unwrap();
    app
}

#[test]
fn an_in_progress_hunt_resumes_with_the_same_claims_and_single_yield() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(running_hunt, 20, 400, 20);
}

#[test]
fn new_fiber_and_berry_sources_use_the_same_finite_gather_action() {
    let (mut app, actor_id, actor, _) = fixture();
    for kind in [village::catalog::FIBER, village::catalog::BERRIES] {
        let patch = app
            .world()
            .resource::<island_content::TestbedIds>()
            .id(&format!("patch_{kind}"));
        let target = app.world().resource::<SimIndex>().get(patch).unwrap();
        let pos = *app.world().get::<SimPos>(target).unwrap();
        let source = app
            .world()
            .get::<village::ResourcePatch>(target)
            .unwrap()
            .clone();
        app.world_mut().entity_mut(actor).insert(pos);
        let mut gather = VillageAct::new(actor_id, "gather");
        gather.target = Some(patch);
        gather.perform(app.world_mut()).unwrap();
        assert_eq!(village::economy::count(app.world(), actor, kind), 0);
        run_ticks(app.world_mut(), u64::from(source.gather_ticks));
        assert!(app.world().get::<village::Gathering>(actor).is_some());
        assert_eq!(village::economy::count(app.world(), actor, kind), 0);
        assert_eq!(
            app.world()
                .get::<village::ResourcePatch>(target)
                .unwrap()
                .remaining,
            source.remaining
        );
        run_ticks(app.world_mut(), 1);
        assert_eq!(village::economy::count(app.world(), actor, kind), 1);
        assert_eq!(
            app.world()
                .get::<village::ResourcePatch>(target)
                .unwrap()
                .remaining,
            source.remaining - 1
        );
    }
}

#[test]
fn replacing_the_last_hunted_animal_preserves_unique_positions_and_allocates_a_fresh_id() {
    let (mut app, actor_id, actor, _) = fixture();
    let original = hunting::available_prey(app.world());
    assert_eq!(original.len(), hunting::MAX_WILDLIFE as usize);
    let original_ids: std::collections::BTreeSet<_> =
        original.iter().map(|(id, _, _)| *id).collect();
    let (last_id, _, last_position) = *original.last().unwrap();
    let original_max = *original_ids.last().unwrap();
    app.world_mut().entity_mut(actor).insert(last_position);
    spear(&mut app, actor);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&command(actor_id, last_id)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let ends = app.world().get::<hunting::Hunting>(actor).unwrap().ends;
    let remaining = ends - app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), remaining);
    assert!(app.world().get::<hunting::Hunting>(actor).is_some());
    assert_eq!(hunting::available_prey(app.world()).len(), original.len());
    run_ticks(app.world_mut(), 1);
    assert!(app.world().resource::<SimIndex>().get(last_id).is_none());
    assert_eq!(
        hunting::available_prey(app.world()).len(),
        original.len() - 1
    );

    let habitat_id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("wildlife_habitat");
    let habitat = app.world().resource::<SimIndex>().get(habitat_id).unwrap();
    let boundary = app
        .world()
        .get::<hunting::WildlifeHabitat>(habitat)
        .unwrap()
        .renew_at;
    app.world_mut().resource_mut::<SimClock>().tick = boundary - 1;
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().resource::<SimClock>().tick, boundary);
    assert_eq!(
        hunting::available_prey(app.world()).len(),
        original.len() - 1
    );
    run_ticks(app.world_mut(), 1);

    let renewed = hunting::available_prey(app.world());
    assert_eq!(renewed.len(), original.len());
    let positions: std::collections::BTreeSet<_> =
        renewed.iter().map(|(_, _, pos)| (pos.x, pos.z)).collect();
    assert_eq!(
        positions.len(),
        original.len(),
        "a replacement must use an unoccupied grazing position"
    );
    let new_ids: Vec<_> = renewed
        .iter()
        .map(|(id, _, _)| *id)
        .filter(|id| !original_ids.contains(id))
        .collect();
    assert_eq!(new_ids.len(), 1, "renew only the single missing individual");
    assert!(
        new_ids[0] > original_max,
        "a replacement must receive a new identity beyond every previous animal"
    );
    for (id, _, pos) in original.into_iter().filter(|(id, _, _)| *id != last_id) {
        assert!(
            renewed
                .iter()
                .any(|(survivor, _, survivor_pos)| *survivor == id && *survivor_pos == pos),
            "renewal must not relocate or replace a surviving animal"
        );
    }
}
