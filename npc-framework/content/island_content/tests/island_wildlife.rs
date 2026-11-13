//! Deer that live (living world): they graze, wander inside their home range
//! with room between them, gallop away from a resident who comes close and
//! settle again; the same seed moves them the same way, and older profiles
//! have no wildlife motion.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    hunting::{Wildlife, WildlifeHabitat},
    view,
    wildlife::{self, WildlifeMotion},
};
use life_support::*;
use npc_framework::prelude::*;

const DAY: u64 = 6_000;

fn quiet() -> App {
    world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
    })
}

fn home_of(world: &World, animal: SimId) -> SimPos {
    let habitat = world
        .get::<Wildlife>(entity(world, animal))
        .expect("wildlife")
        .habitat;
    world
        .get::<WildlifeHabitat>(entity(world, habitat))
        .expect("habitat")
        .home
}

/// Keeps every resident far from the herds, so nothing scares them.
fn residents_away(app: &mut App) {
    for (_, actor) in all_residents(app.world()) {
        let at = *app.world().get::<SimPos>(actor).expect("position");
        for animal in view::wildlife(app.world()) {
            if animal.position.dist2(at) < 30_000_i64.pow(2) {
                teleport(app.world_mut(), actor, SimPos::new(at.x, at.z - 60_000));
                break;
            }
        }
    }
}

#[test]
fn deer_wander_in_their_range_and_keep_their_distance_over_a_day() {
    let mut app = quiet();
    run_ticks(app.world_mut(), 2);
    let start: Vec<_> = view::wildlife(app.world());
    assert!(start.len() >= 6, "{start:?}");
    let mut walked = 0;
    for _ in 0..(DAY / 50) {
        residents_away(&mut app);
        run_ticks(app.world_mut(), 50);
        let animals = view::wildlife(app.world());
        for animal in &animals {
            if animal.state == "dead" {
                continue;
            }
            let home = home_of(app.world(), animal.id);
            assert!(
                animal.position.dist2(home) <= i64::from(wildlife::RANGE_MM + 1_000).pow(2),
                "{animal:?} left its range around {home:?}"
            );
            assert!(
                wildlife::open_ground(app.world(), animal.position),
                "{animal:?}"
            );
            if animal.state == "walking" {
                walked += 1;
                assert_eq!(animal.speed_mm_per_tick, wildlife::WALK_MM_PER_TICK);
            }
        }
        // Animals at rest keep 1.5 m apart.
        let resting: Vec<_> = animals
            .iter()
            .filter(|animal| animal.state == "grazing")
            .collect();
        for (index, a) in resting.iter().enumerate() {
            for b in &resting[index + 1..] {
                assert!(
                    a.position.dist2(b.position) >= wildlife::SPACING_MM.pow(2),
                    "{a:?} and {b:?} rest too close"
                );
            }
        }
    }
    assert!(walked > 0, "no animal walked in a day");
    let moved = view::wildlife(app.world())
        .iter()
        .zip(&start)
        .filter(|(now, then)| now.position != then.position)
        .count();
    assert!(moved > 0, "no animal moved");
}

#[test]
fn a_resident_walking_into_a_herd_makes_it_flee_and_it_settles() {
    let mut app = quiet();
    run_ticks(app.world_mut(), 2);
    residents_away(&mut app);
    run_ticks(app.world_mut(), 20);
    let deer = view::wildlife(app.world())
        .into_iter()
        .find(|animal| animal.state != "dead")
        .expect("a deer");
    let (walker_id, walker) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role != village::Role::Hunter)
        })
        .expect("a resident");
    teleport(
        app.world_mut(),
        walker,
        SimPos::new(deer.position.x + 3_000, deer.position.z),
    );
    let fled = until(&mut app, 60, 1, |app| {
        view::wildlife(app.world())
            .iter()
            .any(|animal| animal.id == deer.id && animal.state == "fleeing")
    });
    assert!(fled, "the deer did not flee from {walker_id:?}");
    let running = view::wildlife(app.world())
        .into_iter()
        .find(|animal| animal.id == deer.id)
        .expect("deer");
    assert_eq!(running.speed_mm_per_tick, wildlife::GALLOP_MM_PER_TICK);
    const { assert!(wildlife::GALLOP_MM_PER_TICK > 2 * wildlife::WALK_MM_PER_TICK) };
    // The resident walks on; the deer settles to graze again, still in range.
    teleport(
        app.world_mut(),
        walker,
        SimPos::new(deer.position.x, deer.position.z - 80_000),
    );
    let settled = until(&mut app, 1_200, 5, |app| {
        residents_away(app);
        view::wildlife(app.world())
            .iter()
            .any(|animal| animal.id == deer.id && animal.state == "grazing")
    });
    assert!(settled, "the deer never settled");
    let calm = view::wildlife(app.world())
        .into_iter()
        .find(|animal| animal.id == deer.id)
        .expect("deer");
    assert!(calm.position != deer.position, "it fled somewhere");
    let home = home_of(app.world(), deer.id);
    assert!(calm.position.dist2(home) <= i64::from(wildlife::RANGE_MM + 1_000).pow(2));
}

#[test]
fn the_same_seed_moves_the_deer_the_same_way() {
    let run = || {
        let mut app = quiet();
        run_ticks(app.world_mut(), 3_000);
        view::wildlife(app.world())
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b);
    assert!(a.iter().any(|animal| animal.state != "dead"));
}

#[test]
fn older_profiles_have_no_wildlife_motion() {
    for scenario in ["smithy", "testbed"] {
        let Ok(mut app) = std::panic::catch_unwind(|| island_content::new_sim_app(7, scenario))
        else {
            continue;
        };
        run_ticks(app.world_mut(), 300);
        let mut query = app.world_mut().query::<&WildlifeMotion>();
        assert_eq!(query.iter(app.world()).count(), 0, "{scenario}");
    }
}

/// The hunter who stalks a deer (its hunt intent names it) does not scare
/// it off; anybody else does.
#[test]
fn a_deer_does_not_flee_from_the_hunter_stalking_it() {
    let mut app = quiet();
    run_ticks(app.world_mut(), 2);
    residents_away(&mut app);
    run_ticks(app.world_mut(), 20);
    let deer = view::wildlife(app.world())
        .into_iter()
        .find(|animal| animal.state != "dead")
        .expect("a deer");
    let mut people = all_residents(app.world()).into_iter();
    let (_, hunter) = people.next().expect("a hunter");
    let (_, walker) = people.next().expect("a walker");
    app.world_mut()
        .entity_mut(hunter)
        .insert(village::production::JobIntent {
            verb: "hunt".into(),
            good: village::catalog::RAW_MEAT.into(),
            recipe: String::new(),
            target: Some(deer.id),
        });
    teleport(
        app.world_mut(),
        hunter,
        SimPos::new(deer.position.x + 3_000, deer.position.z),
    );
    for _ in 0..60 {
        run_ticks(app.world_mut(), 1);
        let state = view::wildlife(app.world())
            .into_iter()
            .find(|animal| animal.id == deer.id)
            .expect("deer")
            .state;
        assert_ne!(
            state, "fleeing",
            "the deer fled from the hunter stalking it"
        );
    }
    teleport(
        app.world_mut(),
        walker,
        SimPos::new(deer.position.x - 3_000, deer.position.z),
    );
    let fled = until(&mut app, 60, 1, |app| {
        view::wildlife(app.world())
            .iter()
            .any(|animal| animal.id == deer.id && animal.state == "fleeing")
    });
    assert!(fled, "the deer did not flee from somebody else");
}
