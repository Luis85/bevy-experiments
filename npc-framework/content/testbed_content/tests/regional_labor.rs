//! Employment changes preserve physical work, finite wealth and resident identity.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::{CraftSource, Crafting, begin_craft, complete_craft};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use testbed_content::{
    TestbedIds,
    village::{
        self, Role, Villager, Wallet, catalog, economy,
        frontier::{
            labor::{self, AssignProfession, Employment, ResidentProfile},
            settlements,
        },
    },
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "living_world")
}
fn person(app: &App, name: &str) -> Entity {
    let id = app.world().resource::<TestbedIds>().id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn workplace(app: &App, actor: Entity, role: Role) -> SimId {
    settlements::workplace(
        app.world(),
        settlements::member(app.world(), actor).unwrap(),
        role,
    )
    .unwrap()
}
fn money(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(
                app.world()
                    .get::<Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
            ) + u64::from(
                app.world()
                    .get::<village::CoinPurse>(*entity)
                    .map_or(0, |purse| purse.coins),
            )
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<village::VillageState>().fund)
}

#[test]
fn regional_residents_have_independent_homes_and_persistent_identity() {
    let mut app = scene();
    let residents: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| app.world().get::<ResidentProfile>(*entity).is_some())
        .collect();
    assert_eq!(residents.len(), 42);
    assert_eq!(money(&app), 42 * u64::from(village::NPC_SEED_COINS));
    for resident in residents {
        let id = *app.world().get::<SimId>(resident).unwrap();
        let home = village::colony::home_of(app.world(), id).unwrap();
        assert_ne!(
            home,
            app.world().get::<Villager>(resident).unwrap().workplace
        );
    }
    village::integrity::validate_colony(app.world()).unwrap();
    let before = state_hash(app.world_mut());
    let jun = person(&app, "Jun");
    assert!(labor::summary(app.world(), jun).is_some());
    assert!(!labor::vacancies(app.world()).is_empty());
    assert_eq!(state_hash(app.world_mut()), before);
    let old = testbed_content::new_sim_app(7, "island_colony");
    assert!(
        old.world()
            .resource::<SimIndex>()
            .map
            .values()
            .all(|entity| old.world().get::<Employment>(*entity).is_none())
    );
}

fn crafting_request() -> App {
    let mut app = scene();
    let jun = person(&app, "Jun");
    let current = app.world().get::<Villager>(jun).unwrap().workplace;
    let position = *app
        .world()
        .get::<SimPos>(app.world().resource::<SimIndex>().get(current).unwrap())
        .unwrap();
    app.world_mut().entity_mut(jun).insert(position);
    begin_craft(
        app.world_mut(),
        jun,
        catalog::SAW,
        Some(current),
        CraftSource::Command,
        None,
    )
    .unwrap();
    let command = AssignProfession {
        resident: *app.world().get::<SimId>(jun).unwrap(),
        workplace: workplace(&app, jun, Role::Miller),
    };
    command.perform(app.world_mut()).unwrap();
    app
}

#[test]
fn assignment_waits_for_real_craft_then_keeps_home_money_and_profile() {
    let mut app = crafting_request();
    let jun = person(&app, "Jun");
    let id = *app.world().get::<SimId>(jun).unwrap();
    let old = app.world().get::<Villager>(jun).unwrap().clone();
    let profile = app.world().get::<ResidentProfile>(jun).unwrap().clone();
    let wallet = *app.world().get::<Wallet>(jun).unwrap();
    let home = village::colony::home_of(app.world(), id);
    let planks = economy::count(app.world(), jun, "plank");
    let claims = app.world().resource::<Reservations>().count(old.workplace);
    assert!(claims > 0);
    assert!(!labor::consider_assignment(app.world_mut(), jun));
    assert_eq!(
        app.world().get::<Villager>(jun).unwrap().role,
        Role::Carpenter
    );
    assert!(
        app.world()
            .get::<Employment>(jun)
            .unwrap()
            .pending
            .is_some()
    );
    assert_eq!(
        app.world().resource::<Reservations>().count(old.workplace),
        claims
    );
    // Intercept the ordinary completion boundary to test the transition itself;
    // unrelated trades are intentionally not advanced in this conservation test.
    let ends = app.world().get::<Crafting>(jun).unwrap().ends;
    app.world_mut().resource_mut::<SimClock>().tick = ends;
    assert!(complete_craft(app.world_mut(), jun).is_some());
    assert_eq!(
        economy::count(app.world(), jun, "plank"),
        planks + catalog::SAW_PLANK_YIELD
    );
    assert!(labor::consider_assignment(app.world_mut(), jun));
    assert_eq!(app.world().get::<Villager>(jun).unwrap().role, Role::Miller);
    assert_eq!(app.world().get::<Villager>(jun).unwrap().home, old.home);
    assert_eq!(app.world().get::<ResidentProfile>(jun), Some(&profile));
    assert_eq!(app.world().get::<Wallet>(jun), Some(&wallet));
    assert_eq!(village::colony::home_of(app.world(), id), home);
    assert_eq!(
        app.world().resource::<Reservations>().count(old.workplace),
        0
    );
    village::integrity::validate_colony(app.world()).unwrap();
}

#[test]
fn simultaneous_requests_revalidate_capacity_without_canceling_the_loser() {
    let mut app = scene();
    let nell = person(&app, "Nell");
    let piper = person(&app, "Piper");
    let farm = workplace(&app, nell, Role::Farmer);
    for actor in [nell, piper] {
        let command = AssignProfession {
            resident: *app.world().get::<SimId>(actor).unwrap(),
            workplace: farm,
        };
        command.perform(app.world_mut()).unwrap();
    }
    assert!(labor::consider_assignment(app.world_mut(), nell));
    assert!(!labor::consider_assignment(app.world_mut(), piper));
    assert_eq!(labor::assigned(app.world(), farm), 2);
    assert_eq!(
        app.world().get::<Villager>(piper).unwrap().role,
        Role::Woodcutter
    );
    assert!(
        app.world()
            .get::<Employment>(piper)
            .unwrap()
            .last_reason
            .contains("vacant position")
    );
    assert_eq!(money(&app), 8400);
}

#[test]
fn autonomous_work_fills_a_missing_profession_without_replacing_the_person() {
    let mut app = scene();
    let rowan = person(&app, "Rowan");
    let mill = app.world().get::<Villager>(rowan).unwrap().workplace;
    let original_money = money(&app);
    // A declared initial disruption. This fixture tests employment recovery,
    // while permanent-death/drop semantics have their separate existing tests.
    app.world_mut()
        .get_mut::<village::Vitality>(rowan)
        .unwrap()
        .died_at = Some(0);
    run_ticks(app.world_mut(), 1200);
    assert!(
        app.world()
            .resource::<SimIndex>()
            .map
            .values()
            .any(|actor| {
                *actor != rowan
                    && village::living(app.world(), *actor)
                    && app.world().get::<Villager>(*actor).is_some_and(|person| {
                        person.workplace == mill && person.role == Role::Miller
                    })
            })
    );
    assert!(!village::living(app.world(), rowan));
    assert_eq!(money(&app), original_money);
    assert_eq!(
        app.world()
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|entity| app.world().get::<ResidentProfile>(**entity).is_some())
            .count(),
        42
    );
}

#[test]
fn pending_assignment_and_committed_work_resume_deterministically() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(crafting_request, 81, 600, 100);
}

#[test]
fn depleted_profession_reconsiders_at_work_boundary_and_obeys_cooldown() {
    let mut app = scene();
    let piper = person(&app, "Piper");
    let original = app.world().get::<ResidentProfile>(piper).unwrap().clone();
    let patches: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| {
            app.world()
                .get::<village::ResourcePatch>(*entity)
                .is_some_and(|patch| patch.kind == "log")
        })
        .collect();
    for patch in patches {
        app.world_mut()
            .get_mut::<village::ResourcePatch>(patch)
            .unwrap()
            .remaining = 0;
    }
    // The declared disruption persists across a complete employment review window.
    // This isolates reassignment from ecology's separately tested regrowth.
    app.world_mut().resource_mut::<SimClock>().tick = 300;
    labor::tick(app.world_mut());
    assert!(!labor::consider_assignment(app.world_mut(), piper));
    app.world_mut().resource_mut::<SimClock>().tick = 1500;
    labor::tick(app.world_mut());
    assert!(labor::consider_assignment(app.world_mut(), piper));
    assert_ne!(
        app.world().get::<Villager>(piper).unwrap().role,
        Role::Woodcutter
    );
    let assignment = app.world().get::<Villager>(piper).unwrap().workplace;
    assert!(!labor::consider_assignment(app.world_mut(), piper));
    assert_eq!(
        app.world().get::<Villager>(piper).unwrap().workplace,
        assignment
    );
    assert_eq!(app.world().get::<ResidentProfile>(piper), Some(&original));
    assert_eq!(money(&app), 8400);
}

#[test]
fn hunters_start_with_real_individually_usable_spears() {
    let app = scene();
    let rhea = person(&app, "Rhea");
    let spears: Vec<_> = economy::free_items(app.world(), rhea)
        .into_iter()
        .filter(|(_, item)| item.kind == catalog::SPEAR)
        .collect();
    assert_eq!(spears.len(), 2);
    assert!(spears.iter().all(|(_, item)| item.stack == 1));
}

#[test]
fn duplicate_producers_receive_separate_funded_work() {
    let mut app = scene();
    let region = settlements::member(app.world(), person(&app, "Oak")).unwrap();
    let people: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|actor| {
            settlements::member(app.world(), *actor) == Some(region)
                && app.world().get::<Villager>(*actor).is_some()
        })
        .collect();
    for actor in people {
        let logs = economy::count(app.world(), actor, "log");
        if logs > 0 {
            economy::consume(app.world_mut(), actor, "log", logs).unwrap();
        }
    }
    village::production::refresh(app.world_mut());
    for name in ["Oak", "Piper"] {
        assert!(
            village::production::production_demand(app.world(), person(&app, name), "log") > 0,
            "{name} received no forestry work"
        );
    }
    assert_eq!(money(&app), 8400);
}
