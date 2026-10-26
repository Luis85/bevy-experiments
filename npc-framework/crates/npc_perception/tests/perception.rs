//! Plan wave D1: `npc_perception` (FOV, range, LOS, hearing, memory,
//! stagger, conditions, detection hook).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{JournalEntry, JournalKind};
use npc_kernel::registry::content_hash;
use npc_kernel::spatial::{
    Footprint, GridBounds, GridChanged, GridOccluder, SpatialGrid, SpatialPlugin,
};
use npc_kernel::{
    Journal, JournalPlugin, NpcKernelPlugin, NpcSet, NpcSimTick, SimId, SimPos, run_ticks,
    spawn_sim,
};
use npc_perception::conds::{heard, heard_noise, remembers, remembers_kind, sees, sees_kind};
use npc_perception::{
    Detection, Facing, FixedFacing, Noise, Perceivable, PerceptionAppExt, PerceptionConfig,
    PerceptionFacts, PerceptionMemory, PerceptionPlugin, SenseKind, Sensors, Vision, emit_noise,
    in_fov, is_due,
};

const UNIT: u32 = 1;
const FOOD: u32 = 2;

fn app_with(config: PerceptionConfig, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(config);
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        PerceptionPlugin,
        JournalPlugin::default(),
    ));
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn every_tick() -> PerceptionConfig {
    PerceptionConfig {
        stagger: 1,
        ..Default::default()
    }
}

fn app() -> App {
    app_with(every_tick(), |_| {})
}

/// An observer at `pos` looking along `facing` (fixed).
fn observer(app: &mut App, pos: SimPos, facing: Facing, sensors: Sensors) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, facing, FixedFacing, sensors))
}

fn thing(app: &mut App, pos: SimPos, kinds: u32) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, Perceivable::new(kinds)))
}

fn memory(app: &App, e: Entity) -> PerceptionMemory {
    app.world().get::<PerceptionMemory>(e).unwrap().clone()
}

fn senses(app: &App, e: Entity, target: SimId) -> bool {
    memory(app, e).get(target).is_some_and(|p| p.sensed)
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn perception_entries(app: &App) -> Vec<(u64, SimId, JournalKind)> {
    journal(app)
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                JournalKind::Perceived { .. } | JournalKind::Lost { .. }
            )
        })
        .map(|e| (e.tick, e.unit.unwrap(), e.kind))
        .collect()
}

const SOUTH: Facing = Facing::new(0, 1);

#[test]
fn fov_math_inside_outside_edge_and_behind() {
    let v = Vision::new(10_000, 45);
    // Straight ahead, inside, exactly on both edges, just outside, behind.
    assert!(v.sees_direction(SOUTH, (0, 5000)));
    assert!(v.sees_direction(SOUTH, (3000, 5000)));
    assert!(
        v.sees_direction(SOUTH, (1000, 1000)),
        "on the edge is inside"
    );
    assert!(
        v.sees_direction(SOUTH, (-1000, 1000)),
        "on the edge is inside"
    );
    assert!(!v.sees_direction(SOUTH, (1001, 1000)));
    assert!(!v.sees_direction(SOUTH, (5000, 0)), "to the side");
    assert!(!v.sees_direction(SOUTH, (0, -5000)), "behind");
    // Facing length does not matter.
    assert!(Vision::new(1, 45).sees_direction(Facing::new(0, 60), (1000, 1000)));
    // 90°: the whole front half plane, sides included.
    let half = Vision::new(10_000, 90);
    assert!(half.sees_direction(SOUTH, (5000, 0)));
    assert!(!half.sees_direction(SOUTH, (5000, -1)));
    // Wide (135°): everything except a 90° wedge behind; its edge is inside.
    let wide = Vision::new(10_000, 135);
    assert!(wide.sees_direction(SOUTH, (5000, -1)));
    assert!(
        wide.sees_direction(SOUTH, (1000, -1000)),
        "on the edge is inside"
    );
    assert!(!wide.sees_direction(SOUTH, (999, -1000)));
    assert!(!wide.sees_direction(SOUTH, (0, -1000)));
    // 180° and a zero direction see everything.
    assert!(in_fov(SOUTH, (0, -1), 180, 1_000_000));
    assert!(in_fov(SOUTH, (0, 0), 10, npc_perception::cos2_ppm_of(10)));
    // Huge coordinates do not overflow.
    assert!(v.sees_direction(
        Facing::new(i32::MAX, i32::MAX),
        (i64::from(i32::MAX), i64::from(i32::MAX))
    ));
    assert!(!v.sees_direction(Facing::new(i32::MIN, 0), (i64::from(i32::MAX), 0)));
}

#[test]
fn vision_respects_fov_and_range_in_the_sim() {
    let mut app = app();
    let (_, eye) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(10_000, 45),
    );
    let (ahead, _) = thing(&mut app, SimPos::new(0, 5000), UNIT);
    let (edge, _) = thing(&mut app, SimPos::new(4000, 4000), UNIT);
    let (outside, _) = thing(&mut app, SimPos::new(4001, 4000), UNIT);
    let (behind, _) = thing(&mut app, SimPos::new(0, -2000), UNIT);
    let (at_range, _) = thing(&mut app, SimPos::new(6000, 8000), UNIT);
    let (too_far, _) = thing(&mut app, SimPos::new(0, 10_001), UNIT);
    run_ticks(app.world_mut(), 1);
    assert!(senses(&app, eye, ahead));
    assert!(senses(&app, eye, edge), "exactly on the FOV edge");
    assert!(!senses(&app, eye, outside));
    assert!(!senses(&app, eye, behind));
    assert!(senses(&app, eye, at_range), "exactly at range (10 m)");
    assert!(!senses(&app, eye, too_far));
    let m = memory(&app, eye);
    assert_eq!(m.percepts.len(), 3);
    let p = m.get(ahead).unwrap();
    assert_eq!(
        (p.via, p.pos, p.kinds, p.last_seen_tick),
        (SenseKind::Vision, SimPos::new(0, 5000), UNIT, 0)
    );
    let perceived: Vec<_> = perception_entries(&app);
    assert_eq!(perceived.len(), 3);
    assert!(
        perceived.iter().all(|(t, _, k)| *t == 0
            && matches!(k, JournalKind::Perceived { via, .. } if via == "vision"))
    );
}

#[test]
fn proximity_senses_behind_without_line_of_sight() {
    let mut app = app();
    let (_, eye) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(10_000, 45).proximity(1500),
    );
    let (close_behind, _) = thing(&mut app, SimPos::new(0, -1500), UNIT);
    let (far_behind, _) = thing(&mut app, SimPos::new(0, -1501), UNIT);
    // A wall between does not stop proximity.
    spawn_sim(
        app.world_mut(),
        (SimPos::new(0, -700), GridOccluder::default()),
    );
    run_ticks(app.world_mut(), 1);
    let m = memory(&app, eye);
    assert_eq!(
        m.get(close_behind).map(|p| p.via),
        Some(SenseKind::Proximity)
    );
    assert!(m.get(far_behind).is_none());
}

#[test]
fn line_of_sight_blocked_by_occluder_and_restored_when_removed() {
    let mut app = app();
    let (_, eye) = observer(
        &mut app,
        SimPos::new(500, 500),
        SOUTH,
        Sensors::new().vision(20_000, 60),
    );
    let (target, _) = thing(&mut app, SimPos::new(500, 8500), UNIT);
    let (_, wall) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 4500),
            GridOccluder(Footprint {
                half_x_mm: 2000,
                half_z_mm: 0,
            }),
        ),
    );
    run_ticks(app.world_mut(), 3);
    assert!(memory(&app, eye).get(target).is_none(), "the wall hides it");

    app.world_mut().entity_mut(wall).despawn();
    let mut cursor = app
        .world()
        .resource::<bevy_ecs::message::Messages<GridChanged>>()
        .get_cursor_current();
    run_ticks(app.world_mut(), 1);
    let changed: Vec<GridChanged> = cursor
        .read(
            app.world()
                .resource::<bevy_ecs::message::Messages<GridChanged>>(),
        )
        .cloned()
        .collect();
    assert_eq!(changed.len(), 1, "the grid reports the opened cells");
    assert!(senses(&app, eye, target), "visible again in the same tick");

    // Put an occluder back: lost again (still remembered).
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 6500), GridOccluder::default()),
    );
    run_ticks(app.world_mut(), 1);
    assert!(!senses(&app, eye, target));
    assert!(memory(&app, eye).get(target).is_some());
    let kinds: Vec<_> = perception_entries(&app)
        .into_iter()
        .map(|(t, _, k)| (t, k))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (
                3,
                JournalKind::Perceived {
                    target,
                    via: "vision".into()
                }
            ),
            (
                4,
                JournalKind::Lost {
                    target,
                    forgotten: false
                }
            ),
        ]
    );
}

#[test]
fn static_occluding_cells_block_too() {
    let mut app = app();
    let (_, eye) = observer(
        &mut app,
        SimPos::new(500, 500),
        SOUTH,
        Sensors::new().vision(20_000, 60),
    );
    let (target, _) = thing(&mut app, SimPos::new(500, 5500), UNIT);
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        let mut info = grid.static_cell(npc_kernel::spatial::CellCoord::new(0, 3));
        info.flags.occluding = true;
        grid.set_static(npc_kernel::spatial::CellCoord::new(0, 3), info);
    }
    run_ticks(app.world_mut(), 1);
    assert!(!senses(&app, eye, target));
}

#[test]
fn hearing_radius_and_noise_radius() {
    let mut app = app();
    let (ear_id, ear) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().hearing(5000),
    );
    let (_, deaf) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(1000, 10),
    );
    let (chopper, _) = thing(&mut app, SimPos::new(-5000, 0), UNIT);
    // Inside both radii (exactly at the hearing radius).
    emit_noise(
        app.world_mut(),
        Noise::at(SimPos::new(-5000, 0), 8000)
            .from_source(chopper)
            .kind(4),
    );
    run_ticks(app.world_mut(), 1);
    let m = memory(&app, ear);
    let p = m.get(chopper).expect("the source is remembered");
    assert_eq!(
        (p.via, p.pos, p.sensed, p.kinds),
        (SenseKind::Hearing, SimPos::new(-5000, 0), false, UNIT)
    );
    assert_eq!(m.last_noise.map(|n| (n.tick, n.kind)), Some((0, 4)));
    assert!(memory(&app, deaf).last_noise.is_none());
    assert!(heard_noise(app.world(), ear) && heard::<4>(app.world(), ear));
    assert!(!heard::<8>(app.world(), ear));

    // Beyond the hearing radius, and a quiet noise beyond its own radius.
    emit_noise(app.world_mut(), Noise::at(SimPos::new(0, 5001), 9000));
    emit_noise(app.world_mut(), Noise::at(SimPos::new(0, 1000), 999));
    run_ticks(app.world_mut(), 1);
    assert_eq!(memory(&app, ear).last_noise.map(|n| n.tick), Some(0));
    // Heard once only; `heard_noise` holds for the 20-tick window.
    run_ticks(app.world_mut(), 18);
    assert!(heard_noise(app.world(), ear), "20 ticks after the noise");
    run_ticks(app.world_mut(), 1);
    assert!(!heard_noise(app.world(), ear));
    let perceived = perception_entries(&app);
    assert_eq!(
        perceived,
        vec![(
            0,
            ear_id,
            JournalKind::Perceived {
                target: chopper,
                via: "hearing".into()
            }
        )]
    );
}

#[test]
fn noise_from_a_system_is_heard_next_tick() {
    fn chop(
        mut noises: ResMut<bevy_ecs::message::Messages<Noise>>,
        clock: Res<npc_kernel::SimClock>,
    ) {
        if clock.tick == 5 {
            noises.write(Noise::at(SimPos::new(1000, 0), 3000));
        }
    }
    let mut app = app_with(every_tick(), |app| {
        app.add_systems(NpcSimTick, chop.in_set(NpcSet::Actions));
    });
    let (_, ear) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().hearing(5000),
    );
    run_ticks(app.world_mut(), 6);
    assert!(
        memory(&app, ear).last_noise.is_none(),
        "written after Sense in tick 5"
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(memory(&app, ear).last_noise.map(|n| n.tick), Some(6));
}

#[test]
fn memory_forgets_after_n_ticks() {
    let config = PerceptionConfig {
        stagger: 1,
        forget_after_ticks: 10,
        ..Default::default()
    };
    let mut app = app_with(config, |_| {});
    let (eye_id, eye) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(10_000, 45),
    );
    let (target, te) = thing(&mut app, SimPos::new(0, 3000), FOOD);
    run_ticks(app.world_mut(), 1);
    assert!(sees_kind(app.world(), eye) && sees::<FOOD>(app.world(), eye));
    assert!(!sees::<UNIT>(app.world(), eye));
    // It walks out of view at tick 1.
    *app.world_mut().get_mut::<SimPos>(te).unwrap() = SimPos::new(0, -3000);
    run_ticks(app.world_mut(), 1);
    assert!(!sees_kind(app.world(), eye));
    assert!(remembers_kind(app.world(), eye) && remembers::<FOOD>(app.world(), eye));
    // Last seen at tick 0: still remembered at tick 10, forgotten at tick 11.
    run_ticks(app.world_mut(), 9);
    assert!(memory(&app, eye).get(target).is_some());
    run_ticks(app.world_mut(), 1);
    assert!(memory(&app, eye).get(target).is_none());
    assert!(!remembers_kind(app.world(), eye));
    assert_eq!(
        perception_entries(&app),
        vec![
            (
                0,
                eye_id,
                JournalKind::Perceived {
                    target,
                    via: "vision".into()
                }
            ),
            (
                1,
                eye_id,
                JournalKind::Lost {
                    target,
                    forgotten: false
                }
            ),
            (
                11,
                eye_id,
                JournalKind::Lost {
                    target,
                    forgotten: true
                }
            ),
        ]
    );
}

#[test]
fn memory_is_bounded_oldest_evicted() {
    let config = PerceptionConfig {
        stagger: 1,
        memory_capacity: 2,
        ..Default::default()
    };
    let mut app = app_with(config, |_| {});
    let (eye_id, eye) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(10_000, 45),
    );
    let things: Vec<(SimId, Entity)> = (0..3)
        .map(|i| thing(&mut app, SimPos::new(0, 2000 + i * 1000), UNIT))
        .collect();
    let ids: Vec<SimId> = things.iter().map(|t| t.0).collect();
    run_ticks(app.world_mut(), 1);
    // Attention: only the two nearest are sensed (no churn).
    let m = memory(&app, eye);
    assert_eq!(
        m.percepts.keys().copied().collect::<Vec<_>>(),
        ids[..2].to_vec()
    );
    // One walks behind the observer: the third is sensed and the oldest
    // memory (ids[1], last seen at tick 0) is evicted; then the other walks
    // away too and stays remembered.
    *app.world_mut().get_mut::<SimPos>(things[1].1).unwrap() = SimPos::new(0, -2000);
    run_ticks(app.world_mut(), 1);
    *app.world_mut().get_mut::<SimPos>(things[0].1).unwrap() = SimPos::new(0, -3000);
    run_ticks(app.world_mut(), 1);
    let m = memory(&app, eye);
    assert_eq!(
        m.percepts.keys().copied().collect::<Vec<_>>(),
        vec![ids[0], ids[2]],
        "ids[1] was evicted"
    );
    // Evicted in tick 1, when the third came into view.
    assert!(perception_entries(&app).contains(&(
        1,
        eye_id,
        JournalKind::Lost {
            target: ids[1],
            forgotten: true
        }
    )));
}

#[test]
fn facts_mirror_the_nearest_sensed_percept() {
    let mut app = app();
    let (_, eye) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 0),
            FixedFacing,
            Sensors::new().vision(10_000, 60),
            npc_ai::Blackboard::default(),
            PerceptionFacts {
                kinds: FOOD,
                target_slot: 2,
                pos_slot: 3,
            },
        ),
    );
    thing(&mut app, SimPos::new(0, 2000), UNIT);
    let (food_far, _) = thing(&mut app, SimPos::new(0, 6000), FOOD);
    let (food_near, _) = thing(&mut app, SimPos::new(500, 4000), FOOD);
    run_ticks(app.world_mut(), 1);
    let bb = app.world().get::<npc_ai::Blackboard>(eye).unwrap();
    assert_eq!(bb.slots[2], npc_ai::BbValue::U32(food_near.0));
    assert_eq!(bb.slots[3], npc_ai::BbValue::Pos(500, 4000));
    assert_ne!(food_far, food_near);
}

#[test]
fn facing_follows_movement_unless_fixed() {
    fn walk(mut q: Query<&mut SimPos, With<Sensors>>) {
        for mut p in &mut q {
            p.x += 50;
        }
    }
    let mut app = app_with(every_tick(), |app| {
        app.add_systems(NpcSimTick, walk.in_set(NpcSet::Actions));
    });
    let (_, free) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), Sensors::new().vision(5000, 30)),
    );
    let (_, fixed) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(5000, 30),
    );
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        *app.world().get::<Facing>(free).unwrap(),
        Facing::new(50, 0)
    );
    assert_eq!(*app.world().get::<Facing>(fixed).unwrap(), SOUTH);
}

fn never(_: &World, _: &Detection) -> Option<u16> {
    None
}

fn only_close(_: &World, d: &Detection) -> Option<u16> {
    (d.dist2 <= 9_000_000).then_some(700)
}

#[test]
fn detection_check_hook_decides_by_name() {
    let mut plain = app();
    let mut blind = app_with(every_tick(), |app| {
        app.register_detection_check("never", never)
            .use_detection_check("never");
    });
    let mut picky = app_with(every_tick(), |app| {
        app.register_detection_check("only_close", only_close)
            .use_detection_check("only_close");
    });
    let mut eyes = Vec::new();
    for app in [&mut plain, &mut blind, &mut picky] {
        let (_, eye) = observer(
            app,
            SimPos::new(0, 0),
            SOUTH,
            Sensors::new().vision(10_000, 45),
        );
        thing(app, SimPos::new(0, 2000), UNIT);
        thing(app, SimPos::new(0, 5000), UNIT);
        run_ticks(app.world_mut(), 1);
        eyes.push(eye);
    }
    assert_eq!(memory(&plain, eyes[0]).percepts.len(), 2);
    assert!(
        memory(&plain, eyes[0])
            .percepts
            .values()
            .all(|p| p.confidence == 1000)
    );
    assert!(memory(&blind, eyes[1]).percepts.is_empty());
    let picky_mem = memory(&picky, eyes[2]);
    assert_eq!(picky_mem.percepts.len(), 1);
    assert_eq!(picky_mem.percepts.values().next().unwrap().confidence, 700);
    // The active check is part of the content hash.
    let (a, b) = (
        content_hash(plain.world_mut()),
        content_hash(blind.world_mut()),
    );
    assert_ne!(a, b);
}

#[test]
#[should_panic(expected = "not registered")]
fn unknown_detection_check_panics_at_freeze() {
    let _ = app_with(every_tick(), |app| {
        app.use_detection_check("nope");
    });
}

/// 40 agents walking in a square pattern, seeing and hearing each other.
fn crowd(seed: u64, stagger: u32) -> App {
    fn walk(mut q: Query<(&SimId, &mut SimPos), With<Sensors>>, clock: Res<npc_kernel::SimClock>) {
        for (id, mut p) in &mut q {
            let phase = (clock.tick / 40 + u64::from(id.0)) % 4;
            let (dx, dz) = [(80, 0), (0, 80), (-80, 0), (0, -80)][phase as usize];
            p.x += dx;
            p.z += dz;
        }
    }
    fn shout(
        q: Query<(&SimId, &SimPos), With<Sensors>>,
        clock: Res<npc_kernel::SimClock>,
        mut commands: Commands,
    ) {
        for (id, p) in &q {
            if (clock.tick + u64::from(id.0)).is_multiple_of(50) {
                commands.queue(npc_perception::EmitNoise(
                    Noise::at(*p, 6000).from_source(*id),
                ));
            }
        }
    }
    let config = PerceptionConfig {
        stagger,
        forget_after_ticks: 60,
        ..Default::default()
    };
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(config);
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        SpatialPlugin,
        PerceptionPlugin,
        JournalPlugin::default(),
    ));
    app.add_systems(NpcSimTick, (walk, shout).chain().in_set(NpcSet::Actions));
    app.finish();
    app.cleanup();
    for i in 0..40i32 {
        let pos = SimPos::new((i % 8) * 2500 - 9000, (i / 8) * 2500 - 6000);
        spawn_sim(
            app.world_mut(),
            (
                pos,
                Sensors::new()
                    .vision(8000, 50)
                    .hearing(6000)
                    .proximity(1000),
                Perceivable::new(UNIT),
            ),
        );
    }
    app
}

#[test]
fn staggered_updates_are_deterministic() {
    let run = || {
        let mut app = crowd(3, 4);
        let mut hashes = Vec::new();
        for _ in 0..12 {
            run_ticks(app.world_mut(), 25);
            hashes.push(state_hash(app.world_mut()));
        }
        (hashes, journal(&app))
    };
    let (h1, j1) = run();
    let (h2, j2) = run();
    assert_eq!(h1, h2);
    assert_eq!(j1, j2);
    let perceived = j1
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::Perceived { .. }))
        .count();
    let lost = j1
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::Lost { .. }))
        .count();
    assert!(
        perceived > 40 && lost > 10,
        "perceived {perceived}, lost {lost}"
    );
    // Vision/proximity changes only happen on each agent's stagger tick.
    for e in &j1 {
        if let JournalKind::Perceived { via, .. } = &e.kind
            && via != "hearing"
        {
            assert!(is_due(e.unit.unwrap(), e.tick, 4), "{e:?}");
        }
        if matches!(
            e.kind,
            JournalKind::Lost {
                forgotten: false,
                ..
            }
        ) {
            assert!(is_due(e.unit.unwrap(), e.tick, 4), "{e:?}");
        }
    }
    // A different stagger gives a different (but equally valid) history.
    let mut every = crowd(3, 1);
    run_ticks(every.world_mut(), 300);
    assert_ne!(journal(&every), j1);
}

#[test]
fn journal_off_does_not_change_the_hash() {
    let mut on = crowd(5, 4);
    let mut off = crowd(5, 4);
    npc_kernel::journal::set_journal(off.world_mut(), false, false);
    run_ticks(on.world_mut(), 200);
    run_ticks(off.world_mut(), 200);
    assert_eq!(state_hash(on.world_mut()), state_hash(off.world_mut()));
}

#[test]
fn inspect_section_shows_memory() {
    let mut app = app();
    let (eye_id, _) = observer(
        &mut app,
        SimPos::new(0, 0),
        SOUTH,
        Sensors::new().vision(10_000, 45),
    );
    let (target, _) = thing(&mut app, SimPos::new(0, 2000), UNIT);
    run_ticks(app.world_mut(), 1);
    let json = npc_perception::perception_json_of(app.world(), eye_id).unwrap();
    assert_eq!(json["memory"]["percepts"][0]["target"], target.0);
    assert_eq!(json["memory"]["percepts"][0]["via"], "vision");
    let unit = npc_kernel::inspect::unit_json(app.world(), eye_id).unwrap();
    assert_eq!(
        unit["components"]["perception_memory"]["percepts"][0]["sensed"],
        true
    );
    assert!(npc_perception::perception_json_of(app.world(), target).is_none());
}

/// Final review cleanup 5: noises written after `NpcSet::Sense` wait in
/// `Messages<Noise>` across the tick boundary (heard next tick); they are
/// part of the state hash.
#[test]
fn pending_noises_are_hashed() {
    let mut a = app();
    run_ticks(a.world_mut(), 1);
    let quiet = state_hash(a.world_mut());
    emit_noise(a.world_mut(), Noise::at(SimPos::new(0, 0), 5000).kind(1));
    let noisy = state_hash(a.world_mut());
    assert_ne!(quiet, noisy, "a pending noise changes the hash");
    let mut other = app();
    run_ticks(other.world_mut(), 1);
    emit_noise(
        other.world_mut(),
        Noise::at(SimPos::new(0, 0), 5000).kind(2),
    );
    assert_ne!(
        state_hash(other.world_mut()),
        noisy,
        "its fields are hashed"
    );
    // Heard (drained) in the next tick: nothing pending any more.
    run_ticks(a.world_mut(), 1);
    let mut control = app();
    run_ticks(control.world_mut(), 2);
    assert_eq!(state_hash(a.world_mut()), state_hash(control.world_mut()));
}
