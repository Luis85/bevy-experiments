//! Task B: sim core (ids, rng, math, clock, needs, reservations, effects,
//! state hash, schedule) as the village uses it (kernel + needs + jobs +
//! village content). None of these tests need a `Brain` agent.

use bevy_app::App;
use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_framework::ai::ThinkRequest;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::ids::spawn_sim;
use npc_framework::kernel::rng::{sim_rand, sim_range};
use npc_framework::kernel::{NpcSet, NpcSimTick};
use npc_framework::needs::{NEED_MAX, Need, PauseDecay, update_level};
use npc_framework::prelude::{
    NeedLevel, NeedRegistry, Needs, NpcPlugins, PrevSimPos, Reservations, SimClock, SimConfig,
    SimId, SimIndex, SimPos, run_ticks,
};
use village_content::effects::{EffectKind, SimEffect};
use village_content::needs::{ENERGY, HUNGER, HUNGER_DEF, need_id};
use village_content::{Stockpile, VillagePlugin};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((NpcPlugins::new().seed(7).without_driver(), VillagePlugin));
    app.finish();
    app.cleanup();
    app
}

fn full(world: &World) -> Needs {
    world.resource::<NeedRegistry>().full()
}

#[test]
fn sim_plugin_builds_headless() {
    let mut app = app();
    run_ticks(app.world_mut(), 5);
    assert_eq!(app.world().resource::<SimClock>().tick, 5);
}

#[test]
fn sim_ids_sequential_and_indexed() {
    let mut app = app();
    let world = app.world_mut();
    let (a, _ea) = spawn_sim(world, ());
    let (b, eb) = spawn_sim(world, SimPos::new(1, 2));
    let (c, _ec) = spawn_sim(world, ());
    assert_eq!((a, b, c), (SimId(1), SimId(2), SimId(3)));
    assert_eq!(world.resource::<SimIndex>().get(SimId(2)), Some(eb));
    assert_eq!(world.get::<SimId>(eb), Some(&SimId(2)));
    world.despawn(eb);
    assert_eq!(world.resource::<SimIndex>().get(SimId(2)), None);
    assert!(world.resource::<SimIndex>().get(SimId(3)).is_some());
}

#[test]
fn step_towards_exact_and_capped() {
    let o = SimPos::new(0, 0);
    assert_eq!(
        o.step_towards(SimPos::new(3000, 4000), 1000),
        SimPos::new(600, 800)
    );
    assert_eq!(
        o.step_towards(SimPos::new(30, 40), 100),
        SimPos::new(30, 40)
    );
    assert_eq!(o.dist2(SimPos::new(3, 4)), 25);
}

#[test]
fn sim_rand_deterministic() {
    let id = SimId(3);
    assert_eq!(sim_rand(7, id, 100, 1), sim_rand(7, id, 100, 1));
    assert_ne!(sim_rand(7, id, 100, 1), sim_rand(7, id, 101, 1));
    let mut seen = [false; 10];
    for tick in 0..1000 {
        let v = sim_range(7, id, tick, 2, -5, 5);
        assert!((-5..5).contains(&v), "{v} out of range");
        seen[(v + 5) as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "not every value drawn: {seen:?}");
}

#[test]
fn clock_day_night() {
    let c = SimConfig::default();
    assert!(!c.is_night(0));
    assert!(!c.is_night(1679));
    assert!(c.is_night(1680));
    assert!(c.is_night(2399));
    assert!(!c.is_night(2400));
    assert!(c.phase_changes_at(1680));
    assert!(c.phase_changes_at(2400));
    assert!(!c.phase_changes_at(0));
    assert!(!c.phase_changes_at(1681));
}

#[test]
fn hysteresis_levels() {
    let t = HUNGER_DEF.thresholds;
    let mut n = Need {
        value: NEED_MAX,
        level: NeedLevel::Ok,
    };
    let set = |n: &mut Need, v: i32| {
        n.value = v;
        update_level(n, &t)
    };

    assert!(set(&mut n, 49_999));
    assert_eq!(n.level, NeedLevel::Low);
    assert!(!set(&mut n, 70_000));
    assert_eq!(n.level, NeedLevel::Low);
    assert!(set(&mut n, 19_999));
    assert_eq!(n.level, NeedLevel::Urgent);
    assert!(!set(&mut n, 59_000));
    assert_eq!(n.level, NeedLevel::Urgent);
    assert!(set(&mut n, 61_000));
    assert_eq!(n.level, NeedLevel::Low);
    assert!(set(&mut n, 85_001));
    assert_eq!(n.level, NeedLevel::Ok);
    assert!(!set(&mut n, 85_001));
}

#[test]
fn needs_decay_and_clamp() {
    let mut app = app();
    let world = app.world_mut();
    let (hunger, energy) = (need_id(world, HUNGER), need_id(world, ENERGY));
    let (_, awake) = spawn_sim(world, full(world));
    let (_, asleep) = spawn_sim(world, (full(world), PauseDecay::of(&[energy])));
    let mut starving = full(world);
    starving.get_mut(hunger).value = 30;
    let (_, hungry) = spawn_sim(world, starving);

    run_ticks(world, 1);

    let n = world.get::<Needs>(awake).unwrap();
    assert_eq!(
        (n.get(hunger).value, n.get(energy).value),
        (NEED_MAX - 40, NEED_MAX - 40)
    );
    assert!(
        world.get::<ThinkRequest>(awake).is_none(),
        "no level change, no request"
    );

    let n = world.get::<Needs>(asleep).unwrap();
    assert_eq!(
        (n.get(hunger).value, n.get(energy).value),
        (NEED_MAX - 40, NEED_MAX)
    );

    let n = world.get::<Needs>(hungry).unwrap();
    assert_eq!(n.get(hunger).value, 0);
    assert_eq!(n.get(hunger).level, NeedLevel::Urgent);
    assert!(
        world.get::<ThinkRequest>(hungry).is_some(),
        "level change requests a think"
    );
}

#[test]
fn prev_pos_copied() {
    let mut app = app();
    let world = app.world_mut();
    let (_, e) = spawn_sim(world, SimPos::new(1, 1));
    assert_eq!(
        world.get::<PrevSimPos>(e),
        Some(&PrevSimPos(SimPos::new(1, 1)))
    );
    run_ticks(world, 1);
    *world.get_mut::<SimPos>(e).unwrap() = SimPos::new(5, 5);
    run_ticks(world, 1);
    assert_eq!(world.get::<PrevSimPos>(e).unwrap().0, SimPos::new(5, 5));
}

#[test]
fn reservations_semantics() {
    let (t, a, b) = (SimId(10), SimId(1), SimId(2));
    let mut r = Reservations::default();
    assert!(r.try_reserve(t, a, 1));
    assert!(!r.try_reserve(t, b, 1));
    assert!(r.try_reserve(t, a, 1));
    assert_eq!(r.count(t), 1);
    assert!(r.holds(t, a) && !r.holds(t, b));
    r.release_all(a);
    assert_eq!(r.count(t), 0);
    assert_eq!(r, Reservations::default(), "no empty entries left behind");

    assert!(r.try_reserve(t, a, 2));
    assert!(r.try_reserve(t, b, 2));
    assert_eq!(r.count(t), 2);
    r.release(t, a);
    assert!(!r.holds(t, a) && r.holds(t, b));
}

fn write_effect(world: &mut World, effect: SimEffect) {
    world.resource_mut::<Messages<SimEffect>>().write(effect);
}

#[test]
fn effects_resolved_and_clamped() {
    let mut app = app();
    let world = app.world_mut();
    let (pile, e) = spawn_sim(
        world,
        Stockpile {
            food: 9,
            capacity: 10,
        },
    );
    let src = SimId(99);
    write_effect(
        world,
        SimEffect {
            source: src,
            kind: EffectKind::AddFood {
                stockpile: pile,
                amount: 3,
            },
        },
    );
    run_ticks(world, 1);
    assert_eq!(world.get::<Stockpile>(e).unwrap().food, 10);
    assert!(
        world.resource::<Messages<SimEffect>>().is_empty(),
        "effects drained"
    );

    world.get_mut::<Stockpile>(e).unwrap().food = 0;
    write_effect(
        world,
        SimEffect {
            source: src,
            kind: EffectKind::TakeFood {
                stockpile: pile,
                amount: 1,
            },
        },
    );
    run_ticks(world, 1);
    assert_eq!(world.get::<Stockpile>(e).unwrap().food, 0);

    // Order independence: take-then-add vs add-then-take written in reverse.
    world.get_mut::<Stockpile>(e).unwrap().food = 10;
    write_effect(
        world,
        SimEffect {
            source: SimId(2),
            kind: EffectKind::AddFood {
                stockpile: pile,
                amount: 1,
            },
        },
    );
    write_effect(
        world,
        SimEffect {
            source: SimId(1),
            kind: EffectKind::TakeFood {
                stockpile: pile,
                amount: 1,
            },
        },
    );
    run_ticks(world, 1);
    // Sorted by source: take (10→9) first, then add (9→10).
    assert_eq!(world.get::<Stockpile>(e).unwrap().food, 10);
}

fn hash_scene(shift: i32) -> u64 {
    let mut app = app();
    let world = app.world_mut();
    spawn_sim(world, (SimPos::new(100, 200), Needs::default()));
    spawn_sim(
        world,
        (
            SimPos::new(-5, 7 + shift),
            Stockpile {
                food: 3,
                capacity: 10,
            },
        ),
    );
    world
        .resource_mut::<Reservations>()
        .try_reserve(SimId(2), SimId(1), 1);
    run_ticks(world, 3);
    state_hash(world)
}

#[test]
fn state_hash_equal_and_sensitive() {
    assert_eq!(hash_scene(0), hash_scene(0));
    assert_ne!(hash_scene(0), hash_scene(1));
}

#[test]
#[should_panic(expected = "Error when initializing schedule NpcSimTick")]
fn sim_tick_rejects_ambiguous_systems() {
    fn bump_a(mut c: ResMut<SimClock>) {
        c.tick += 0;
    }
    fn bump_b(mut c: ResMut<SimClock>) {
        c.tick += 0;
    }
    let mut app = app();
    app.add_systems(NpcSimTick, (bump_a, bump_b).in_set(NpcSet::Sense));
    run_ticks(app.world_mut(), 1);
}
