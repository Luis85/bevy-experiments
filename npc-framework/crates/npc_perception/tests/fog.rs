//! Plan Task 9 (spec G8, fog of war): the explored and visible cells of an
//! actor that reveals fog ([`RevealsFog`]) and the last-seen state of the
//! statics in them ([`SeenStatics`]). Sim-side, integer-only and
//! snapshot-only: no state or content hash moves.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::hash::{SimHashRegistry, state_hash};
use npc_kernel::registry::content_hash;
use npc_kernel::snapshot::{LoadMode, SnapshotRegistry, WorldSnapshot, load, save};
use npc_kernel::spatial::{
    CellCoord, Footprint, GridBounds, GridOccluder, SpatialGrid, SpatialPlugin,
};
use npc_kernel::{NpcKernelPlugin, SimClock, SimId, SimPos, run_ticks, spawn_sim};
use npc_perception::{
    EXPLORED_CELLS, ExploredCells, Facing, FixedFacing, FogResets, FogStatics, Perceivable,
    PerceptionAppExt, PerceptionConfig, PerceptionPlugin, REVEALS_FOG, ReachSense, RevealsFog,
    SEEN_STATICS, SeenState, SeenStatics, Sensor, Sensors, VISIBLE_CELLS, VisibleCells, Vision,
    is_due, line_of_sight, reveal_cells, reveal_reach_cells,
};

const UNIT: u32 = 1;
const DOOR: u32 = 1 << 2;
const TREE: u32 = 1 << 3;
const ITEM: u32 = 1 << 1;
const WALL: u32 = 1 << 8;
/// Facing +z.
const NORTH: Facing = Facing::new(0, 1);
const SOUTH: Facing = Facing::new(0, -1);
const RANGE_MM: i32 = 15_000;
const HALF_FOV: u16 = 60;

/// A door stand-in (the real one lives in `npc_interaction`).
#[derive(Component, Clone, Copy)]
struct TestDoor {
    open: bool,
}

/// A felled-tree stand-in.
#[derive(Component, Clone, Copy)]
struct TestTree {
    felled: bool,
}

fn app_with(stagger: u32) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger,
            ..Default::default()
        });
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, PerceptionPlugin));
    app.set_fog_statics(FogStatics {
        kinds: DOOR | TREE | ITEM,
        occluder_kind: WALL,
        open: Some(|e| e.get::<TestDoor>().map(|d| d.open)),
        felled: Some(|e| e.get::<TestTree>().map(|t| t.felled)),
    });
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    app_with(1)
}

/// The centre of cell `(x, z)`.
const fn cell(x: i32, z: i32) -> SimPos {
    SimPos::new(x * 1000 + 500, z * 1000 + 500)
}

/// A human-like eye at `pos`: 15 m / 60° sight along `facing` (fixed); it
/// reveals fog when `fog`.
fn eye(app: &mut App, pos: SimPos, facing: Facing, fog: bool) -> (SimId, Entity) {
    let (id, e) = spawn_sim(
        app.world_mut(),
        (
            pos,
            facing,
            FixedFacing,
            Sensors::new().vision(RANGE_MM, HALF_FOV),
            Perceivable::new(UNIT),
        ),
    );
    if fog {
        app.world_mut().entity_mut(e).insert(RevealsFog);
    }
    (id, e)
}

fn wall_cell(app: &mut App, at: SimPos) {
    spawn_sim(app.world_mut(), (at, GridOccluder::default()));
}

fn explored(app: &App, e: Entity, c: (i32, i32)) -> bool {
    let grid = app.world().resource::<SpatialGrid>();
    app.world()
        .get::<ExploredCells>(e)
        .unwrap()
        .is_explored(grid, CellCoord::new(c.0, c.1))
}

fn visible(app: &App, e: Entity, c: (i32, i32)) -> bool {
    let grid = app.world().resource::<SpatialGrid>();
    app.world()
        .get::<VisibleCells>(e)
        .unwrap()
        .is_visible(grid, CellCoord::new(c.0, c.1))
}

fn explored_count(app: &App, e: Entity) -> u32 {
    app.world().get::<ExploredCells>(e).unwrap().count()
}

fn statics(app: &App, e: Entity) -> SeenStatics {
    app.world().get::<SeenStatics>(e).unwrap().clone()
}

fn set_facing(app: &mut App, e: Entity, f: Facing) {
    *app.world_mut().get_mut::<Facing>(e).unwrap() = f;
}

fn vision() -> Sensor {
    Sensors::new().vision(RANGE_MM, HALF_FOV).sensors[0]
}

#[test]
fn fog_respects_line_of_sight() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    // A wall cell three cells north of the eye.
    wall_cell(&mut app, cell(0, 3));
    run_ticks(app.world_mut(), 1);

    let grid = app.world().resource::<SpatialGrid>();
    let cells = reveal_cells(grid, cell(0, 0), NORTH, &vision());
    let has = |x: i32, z: i32| cells.contains(&CellCoord::new(x, z));
    assert!(has(0, 0), "the eye's own cell");
    assert!(has(0, 2), "in front of the wall");
    assert!(has(0, 3), "the wall's own face");
    assert!(!has(0, 4) && !has(0, 6) && !has(0, 10), "behind the wall");
    assert!(has(5, 10), "beside the wall's shadow");
    assert!(!has(0, -3), "behind the eye: outside the cone");
    assert!(has(3, 14) && !has(3, 15), "range 15 m by cell centre");
    let mut sorted = cells.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, cells, "sorted, no duplicates");

    // The pass records exactly those cells, explored and visible.
    for &c in &cells {
        assert!(explored(&app, e, (c.x, c.z)) && visible(&app, e, (c.x, c.z)));
    }
    assert_eq!(explored_count(&app, e), cells.len() as u32);
    assert!(!explored(&app, e, (0, 6)));

    // Other sensors reveal nothing (robot sensors: Task E3).
    let hearing = Sensors::new().hearing(10_000).sensors[0];
    let proximity = Sensors::new().proximity(1500).sensors[0];
    assert!(reveal_cells(grid, cell(0, 0), NORTH, &hearing).is_empty());
    assert!(reveal_cells(grid, cell(0, 0), NORTH, &proximity).is_empty());
}

#[test]
fn reach_reveals_all_round_but_not_through_walls() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    app.world_mut()
        .entity_mut(e)
        .insert(ReachSense { radius_mm: 1500 });
    // A wall cell west of the eye.
    wall_cell(&mut app, cell(-1, 0));
    run_ticks(app.world_mut(), 1);
    let grid = app.world().resource::<SpatialGrid>();
    let cells = reveal_reach_cells(grid, cell(0, 0), ReachSense { radius_mm: 1500 });
    let has = |x: i32, z: i32| cells.contains(&CellCoord::new(x, z));
    assert!(
        has(0, -1) && has(1, -1) && has(-1, 0),
        "behind, and the wall face"
    );
    assert!(!has(-2, 0), "past the wall");
    assert!(!has(0, -2), "beyond 1.5 m");
    // The pass reveals behind the eye through reach.
    assert!(explored(&app, e, (0, -1)) && visible(&app, e, (0, -1)));
    assert!(!explored(&app, e, (0, -2)));
}

#[test]
fn closed_door_blocks_fog() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    // A closed door is an occluding cell (`npc_interaction` sets the
    // static layer); walls around it.
    let set_door = |app: &mut App, closed: bool| {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        let c = CellCoord::new(0, 3);
        let mut info = grid.static_cell(c);
        info.flags.occluding = closed;
        info.flags.blocking = closed;
        grid.set_static(c, info);
    };
    for x in [-1, 1] {
        wall_cell(&mut app, cell(x, 3));
    }
    set_door(&mut app, true);
    run_ticks(app.world_mut(), 1);
    assert!(explored(&app, e, (0, 3)), "the door itself");
    assert!(!explored(&app, e, (0, 5)), "behind the closed door");

    set_door(&mut app, false);
    run_ticks(app.world_mut(), 1);
    assert!(explored(&app, e, (0, 5)) && visible(&app, e, (0, 5)));

    // Closed again: still explored, no longer visible.
    set_door(&mut app, true);
    run_ticks(app.world_mut(), 1);
    assert!(explored(&app, e, (0, 5)) && !visible(&app, e, (0, 5)));
}

#[test]
fn explored_grows_while_walking() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    run_ticks(app.world_mut(), 1);
    let start = explored_count(&app, e);
    assert!(start > 0);
    assert!(visible(&app, e, (-5, 8)));
    let mut last = start;
    for step in 1..=20 {
        *app.world_mut().get_mut::<SimPos>(e).unwrap() = cell(step, 0);
        run_ticks(app.world_mut(), 1);
        let now = explored_count(&app, e);
        assert!(now >= last, "explored never shrinks ({last} -> {now})");
        last = now;
        // Visible is always a subset of explored.
        let world = app.world();
        let v = world.get::<VisibleCells>(e).unwrap();
        let x = world.get::<ExploredCells>(e).unwrap();
        assert!(v.bits.iter().zip(&x.bits).all(|(v, x)| v & !x == 0));
    }
    assert!(
        last > start + 100,
        "walking 20 m east explores more ({start} -> {last})"
    );
    // Seen at the start, out of sight now: explored, not visible.
    assert!(explored(&app, e, (-5, 8)) && !visible(&app, e, (-5, 8)));
    assert!(visible(&app, e, (20, 8)));
}

#[test]
fn fog_updates_on_scan_ticks_only() {
    let mut app = app_with(4);
    let (id, e) = eye(&mut app, cell(0, 0), NORTH, true);
    let mut last = 0;
    for _ in 0..12 {
        let tick = app.world().resource::<SimClock>().tick;
        *app.world_mut().get_mut::<SimPos>(e).unwrap() = cell(i32::try_from(tick).unwrap(), 0);
        run_ticks(app.world_mut(), 1);
        let now = explored_count(&app, e);
        if is_due(id, tick, 4) {
            assert!(now > last, "tick {tick}: a scan tick explores");
        } else {
            assert_eq!(now, last, "tick {tick}: no scan, no change");
        }
        last = now;
    }
}

#[test]
fn only_actors_that_reveal_fog_get_a_record() {
    let mut app = app();
    let (_, plain) = eye(&mut app, cell(0, 0), NORTH, false);
    run_ticks(app.world_mut(), 1);
    let world = app.world();
    assert!(world.get::<ExploredCells>(plain).is_none());
    assert!(world.get::<VisibleCells>(plain).is_none());
    assert!(world.get::<SeenStatics>(plain).is_none());
}

#[test]
fn seen_statics_keep_last_door_state() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    let (door, door_e) = spawn_sim(
        app.world_mut(),
        (cell(0, 4), TestDoor { open: true }, Perceivable::new(DOOR)),
    );
    let (tree, tree_e) = spawn_sim(
        app.world_mut(),
        (
            cell(3, 6),
            TestTree { felled: false },
            Perceivable::new(TREE),
        ),
    );
    // A unit in view is no static: never recorded.
    let (unit, _) = spawn_sim(app.world_mut(), (cell(-2, 5), Perceivable::new(UNIT)));
    // A wall run (cells x 3..=7 on row 9), not perceivable: recorded as WALL.
    let (wall, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(5500, 9500),
            GridOccluder(Footprint {
                half_x_mm: 2000,
                half_z_mm: 0,
            }),
        ),
    );
    run_ticks(app.world_mut(), 1);
    let seen = statics(&app, e);
    assert_eq!(
        seen.0.get(&door),
        Some(&SeenState {
            kind: DOOR,
            pos: cell(0, 4),
            open: Some(true),
            felled: None,
            footprint: None,
        })
    );
    assert_eq!(seen.0[&tree].felled, Some(false));
    assert_eq!(seen.0[&tree].open, None);
    assert_eq!(seen.0[&wall].kind, WALL);
    assert_eq!(seen.0[&wall].pos, SimPos::new(5500, 9500));
    assert_eq!(
        seen.0[&wall].footprint,
        Some(Footprint {
            half_x_mm: 2000,
            half_z_mm: 0
        })
    );
    assert!(!seen.0.contains_key(&unit), "units are not statics");

    // Turned away: the door closes and the tree falls unseen.
    set_facing(&mut app, e, SOUTH);
    run_ticks(app.world_mut(), 1);
    app.world_mut().get_mut::<TestDoor>(door_e).unwrap().open = false;
    app.world_mut().get_mut::<TestTree>(tree_e).unwrap().felled = true;
    run_ticks(app.world_mut(), 8);
    assert!(!visible(&app, e, (0, 4)) && explored(&app, e, (0, 4)));
    let seen = statics(&app, e);
    assert_eq!(seen.0[&door].open, Some(true), "last seen open");
    assert_eq!(seen.0[&tree].felled, Some(false), "last seen standing");

    // Seen again: the record follows.
    set_facing(&mut app, e, NORTH);
    run_ticks(app.world_mut(), 1);
    let seen = statics(&app, e);
    assert_eq!(seen.0[&door].open, Some(false));
    assert_eq!(seen.0[&tree].felled, Some(true));
}

#[test]
fn seen_statics_forget_what_is_gone_only_when_seen_again() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    let (item, item_e) = spawn_sim(app.world_mut(), (cell(1, 5), Perceivable::new(ITEM)));
    let (moved, moved_e) = spawn_sim(app.world_mut(), (cell(-1, 5), Perceivable::new(ITEM)));
    run_ticks(app.world_mut(), 1);
    assert!(statics(&app, e).0.contains_key(&item));

    set_facing(&mut app, e, SOUTH);
    run_ticks(app.world_mut(), 1);
    // Taken (an item picked up loses its position) and moved, unseen.
    app.world_mut().entity_mut(item_e).remove::<SimPos>();
    *app.world_mut().get_mut::<SimPos>(moved_e).unwrap() = cell(-1, 30);
    run_ticks(app.world_mut(), 2);
    let seen = statics(&app, e);
    assert_eq!(seen.0[&item].pos, cell(1, 5), "still shown where last seen");
    assert_eq!(seen.0[&moved].pos, cell(-1, 5));

    set_facing(&mut app, e, NORTH);
    run_ticks(app.world_mut(), 1);
    let seen = statics(&app, e);
    assert!(!seen.0.contains_key(&item), "its cell is seen empty");
    assert!(
        !seen.0.contains_key(&moved),
        "moved out of sight: gone from where it was"
    );
}

#[test]
fn fog_is_snapshot_only() {
    let mut a = app();
    let hashes = a.world().resource::<SimHashRegistry>();
    for name in [REVEALS_FOG, EXPLORED_CELLS, VISIBLE_CELLS, SEEN_STATICS] {
        assert!(
            hashes.components.iter().all(|c| c.name != name),
            "{name} must not join the state hash"
        );
        let snapshots = a.world().resource::<SnapshotRegistry>();
        assert!(snapshots.component_names().contains(&name), "{name}");
    }
    // The same world with and without fog: same content and state hash.
    let mut b = app();
    for (app, fog) in [(&mut a, true), (&mut b, false)] {
        eye(app, cell(0, 0), NORTH, fog);
        spawn_sim(
            app.world_mut(),
            (cell(0, 4), TestDoor { open: true }, Perceivable::new(DOOR)),
        );
        wall_cell(app, cell(2, 3));
    }
    assert_eq!(content_hash(a.world_mut()), content_hash(b.world_mut()));
    run_ticks(a.world_mut(), 8);
    run_ticks(b.world_mut(), 8);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}

#[test]
fn fog_survives_save_and_load() {
    let make = || {
        let mut app = app_with(4);
        let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
        spawn_sim(
            app.world_mut(),
            (cell(0, 4), TestDoor { open: true }, Perceivable::new(DOOR)),
        );
        wall_cell(&mut app, cell(2, 3));
        (app, e)
    };
    let walk = |app: &mut App, e: Entity, ticks: i32| {
        for _ in 0..ticks {
            let t = i32::try_from(app.world().resource::<SimClock>().tick).unwrap();
            *app.world_mut().get_mut::<SimPos>(e).unwrap() = SimPos::new(500 + t * 250, 500);
            run_ticks(app.world_mut(), 1);
        }
    };
    let (mut never, never_e) = make();
    walk(&mut never, never_e, 40);
    let (mut saved, saved_e) = make();
    walk(&mut saved, saved_e, 40);
    let snap = save(saved.world_mut()).unwrap();
    let text = serde_json::to_string(&snap).unwrap();
    let snap: WorldSnapshot = serde_json::from_str(&text).unwrap();
    let (mut loaded, _) = make();
    run_ticks(loaded.world_mut(), 0);
    let report = load(loaded.world_mut(), &snap, LoadMode::Strict).unwrap();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let loaded_e = loaded
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(*never.world().get::<SimId>(never_e).unwrap())
        .unwrap();
    let record = |app: &App, e: Entity| {
        let w = app.world();
        (
            w.get::<ExploredCells>(e).cloned(),
            w.get::<VisibleCells>(e).cloned(),
            w.get::<SeenStatics>(e).cloned(),
            w.get::<RevealsFog>(e).is_some(),
        )
    };
    assert_eq!(record(&loaded, loaded_e), record(&never, never_e));
    assert!(explored_count(&loaded, loaded_e) > 0);
    walk(&mut never, never_e, 20);
    walk(&mut loaded, loaded_e, 20);
    assert_eq!(record(&loaded, loaded_e), record(&never, never_e));
    assert_eq!(
        state_hash(loaded.world_mut()),
        state_hash(never.world_mut())
    );
}

#[test]
fn explored_resets_when_the_grid_changes_size() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    // A record from another grid (e.g. a save from another build).
    *app.world_mut().get_mut::<ExploredCells>(e).unwrap() = ExploredCells {
        width: 10,
        bits: vec![u64::MAX; 2],
        min: None,
    };
    run_ticks(app.world_mut(), 1);
    let x = app.world().get::<ExploredCells>(e).unwrap().clone();
    let v = app.world().get::<VisibleCells>(e).unwrap().clone();
    assert_eq!(x.width, 64);
    assert_eq!(x.bits.len(), 64 * 64 / 64);
    assert_eq!(x.min, Some(CellCoord::new(-32, -32)));
    assert_eq!(x.bits, v.bits, "only what is seen now");

    // Same size, another lowest cell: does not fit either.
    let full = ExploredCells {
        width: 64,
        bits: vec![u64::MAX; 64],
        min: Some(CellCoord::new(0, 0)),
    };
    let bounds = app.world().resource::<SpatialGrid>().bounds();
    assert!(!full.fits(bounds));
    *app.world_mut().get_mut::<ExploredCells>(e).unwrap() = full;
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<ExploredCells>(e).unwrap().bits, v.bits);
    // A record from before `min` existed is kept when width and size fit.
    let old = ExploredCells {
        width: 64,
        bits: vec![u64::MAX; 64],
        min: None,
    };
    assert!(old.fits(bounds));
    *app.world_mut().get_mut::<ExploredCells>(e).unwrap() = old;
    run_ticks(app.world_mut(), 1);
    let kept = app.world().get::<ExploredCells>(e).unwrap();
    assert_eq!((kept.count(), kept.min), (64 * 64, Some(bounds.min)));
}

/// The reference for the fog's visible cells: from the centre of `from`'s
/// cell, that cell plus every in-bounds cell whose centre is in range (and,
/// for vision, in the cone) with `line_of_sight` between the centres.
fn reference(
    grid: &SpatialGrid,
    from: SimPos,
    facing: Facing,
    visions: &[Vision],
    reach_mm: Option<i32>,
) -> Vec<CellCoord> {
    let own = grid.cell_of(from);
    let eye = grid.center(own);
    let b = grid.bounds();
    let mut want = Vec::new();
    for i in 0..u32::try_from(b.len()).unwrap() {
        let c = b.coord(i);
        let p = grid.center(c);
        let d = (
            i64::from(p.x) - i64::from(eye.x),
            i64::from(p.z) - i64::from(eye.z),
        );
        let within = |r: i32| eye.dist2(p) <= i64::from(r) * i64::from(r);
        let sensed = visions
            .iter()
            .any(|v| within(v.range_mm) && v.sees_direction(facing, d))
            || reach_mm.is_some_and(within);
        if c == own || (sensed && line_of_sight(grid, eye, p)) {
            want.push(c);
        }
    }
    want.sort_unstable();
    want
}

/// The fog's visible cells are exactly [`reference`] (the scan uses traced
/// tables and reads the occluders once): static occluding cells and
/// `GridOccluder` entities, an eye exactly on a cell boundary, an eye
/// standing in an occluding cell, several cones in one sensor set, reach,
/// the grid's edge; through `reveal_cells` / `reveal_reach_cells` and
/// through the pass.
#[test]
fn reveal_matches_line_of_sight_per_cell() {
    let mut app = app();
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for x in -20i32..20 {
            for z in -20i32..20 {
                if (x * 7 + z * 3).rem_euclid(11) == 0 {
                    let c = CellCoord::new(x, z);
                    let mut info = grid.static_cell(c);
                    info.flags.occluding = true;
                    grid.set_static(c, info);
                }
            }
        }
    }
    // Wall runs (the dynamic layer).
    for (at, half_x, half_z) in [
        (SimPos::new(4000, 6500), 1500, 0),
        (SimPos::new(-6500, -2000), 0, 2500),
        (SimPos::new(-2500, 9500), 500, 500),
    ] {
        spawn_sim(
            app.world_mut(),
            (
                at,
                GridOccluder(Footprint {
                    half_x_mm: half_x,
                    half_z_mm: half_z,
                }),
            ),
        );
    }
    let senses = Sensors::new().vision(RANGE_MM, HALF_FOV).vision(5000, 150);
    let cases = [
        (SimPos::new(250, 730), NORTH),
        // Exactly on a cell corner (the cell above and right of it).
        (SimPos::new(1000, 2000), Facing::new(-2, 1)),
        // Standing in an occluding cell (made so below).
        (SimPos::new(-3100, 4999), Facing::new(3, -1)),
        // At the grid's edge (cell (31, -32)).
        (SimPos::new(31_900, -31_900), Facing::new(-1, 1)),
    ];
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        let c = grid.cell_of(cases[2].0);
        let mut info = grid.static_cell(c);
        info.flags.occluding = true;
        grid.set_static(c, info);
    }
    let mut eyes = Vec::new();
    for (from, facing) in cases {
        let (_, e) = spawn_sim(
            app.world_mut(),
            (
                from,
                facing,
                FixedFacing,
                senses.clone(),
                ReachSense { radius_mm: 1500 },
                RevealsFog,
            ),
        );
        eyes.push(e);
    }
    run_ticks(app.world_mut(), 1);
    let grid = app.world().resource::<SpatialGrid>();
    assert!(grid.is_occluding(grid.cell_of(cases[2].0)));
    assert!(grid.is_occluding(CellCoord::new(3, 6)), "a wall run");
    let visions: Vec<Vision> = senses.visions().collect();
    for ((from, facing), e) in cases.into_iter().zip(eyes) {
        // One sensor at a time.
        for (i, v) in visions.iter().enumerate() {
            let got = reveal_cells(grid, from, facing, &senses.sensors[i]);
            let want = reference(grid, from, facing, &[*v], None);
            assert_eq!(got, want, "{from:?} cone {i}");
        }
        let got = reveal_reach_cells(grid, from, ReachSense { radius_mm: 1500 });
        let want = reference(grid, from, facing, &[], Some(1500));
        assert_eq!(got, want, "{from:?} reach");
        // Everything at once, through the pass.
        let want = reference(grid, from, facing, &visions, Some(1500));
        let v = app.world().get::<VisibleCells>(e).unwrap();
        let b = grid.bounds();
        let got: Vec<CellCoord> = {
            let mut got: Vec<CellCoord> = (0..u32::try_from(b.len()).unwrap())
                .filter(|&i| v.get(i))
                .map(|i| b.coord(i))
                .collect();
            got.sort_unstable();
            got
        };
        assert_eq!(got, want, "{from:?} all senses");
        assert!(got.len() > 20, "{from:?}: {}", got.len());
    }
}

/// Review fix 5: a wall run seen only by cells away from its centre is
/// dropped when it is gone and one of its cells is seen again.
#[test]
fn despawned_wall_seen_by_a_side_cell_is_dropped() {
    let mut app = app();
    // A wall over cells x -10..=10 of row 5; its centre cell (0, 5) is 79°
    // off the eye's facing, outside the cone; its east end is in view.
    let (wall, wall_e) = spawn_sim(
        app.world_mut(),
        (
            cell(0, 5),
            GridOccluder(Footprint {
                half_x_mm: 10_000,
                half_z_mm: 0,
            }),
        ),
    );
    let (_, e) = eye(&mut app, cell(10, 3), NORTH, true);
    run_ticks(app.world_mut(), 1);
    assert!(visible(&app, e, (10, 5)) && !visible(&app, e, (0, 5)));
    assert!(statics(&app, e).0.contains_key(&wall));
    app.world_mut().entity_mut(wall_e).despawn();
    run_ticks(app.world_mut(), 2);
    assert!(visible(&app, e, (10, 5)) && !visible(&app, e, (0, 5)));
    assert!(
        !statics(&app, e).0.contains_key(&wall),
        "gone: one of its cells is seen empty"
    );
}

/// An idle actor's scans reuse its visible cells; any change of the grid's
/// cells (a door opening) recomputes them on the next scan.
#[test]
fn idle_scans_reuse_until_the_grid_changes() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    let set_door = |app: &mut App, closed: bool| {
        let door = CellCoord::new(0, 3);
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        let mut info = grid.static_cell(door);
        info.flags.occluding = closed;
        grid.set_static(door, info);
    };
    set_door(&mut app, true);
    run_ticks(app.world_mut(), 3);
    assert!(!explored(&app, e, (0, 5)));
    // Opened between ticks: the next scan sees through it.
    set_door(&mut app, false);
    run_ticks(app.world_mut(), 1);
    assert!(visible(&app, e, (0, 5)));
    set_door(&mut app, true);
    run_ticks(app.world_mut(), 1);
    assert!(!visible(&app, e, (0, 5)) && explored(&app, e, (0, 5)));
    // Moving inside one cell: the same cells (fog is per cell).
    let before = app.world().get::<VisibleCells>(e).unwrap().clone();
    *app.world_mut().get_mut::<SimPos>(e).unwrap() = SimPos::new(900, 100);
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<VisibleCells>(e).unwrap(), &before);
}

/// Review fix 2: a save whose explored record does not fit the loaded grid
/// starts over at the load, and the load says so ([`FogResets`]); the
/// visible cells of another size read as nothing visible.
#[test]
fn load_resets_a_record_for_another_grid() {
    let make = || {
        let mut app = app();
        let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
        (app, e)
    };
    let (mut saved, _) = make();
    run_ticks(saved.world_mut(), 4);
    let mut snap = save(saved.world_mut()).unwrap();
    // As if saved by a build with a 32-cell grid.
    for entity in &mut snap.entities {
        if let Some(x) = entity.components.get_mut(EXPLORED_CELLS) {
            *x = serde_json::json!({"width": 32, "bits": vec![u64::MAX; 16], "min": [-16, -16]});
        }
        if let Some(v) = entity.components.get_mut(VISIBLE_CELLS) {
            *v = serde_json::json!({"bits": vec![u64::MAX; 16]});
        }
    }
    let (mut loaded, _) = make();
    run_ticks(loaded.world_mut(), 0);
    load(loaded.world_mut(), &snap, LoadMode::Strict).unwrap();
    let id = SimId(1);
    let e = loaded
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(id)
        .unwrap();
    assert_eq!(
        loaded.world().get_resource::<FogResets>(),
        Some(&FogResets(vec![id]))
    );
    let grid = loaded.world().resource::<SpatialGrid>();
    let x = loaded.world().get::<ExploredCells>(e).unwrap();
    assert!(x.fits(grid.bounds()) && x.count() == 0, "started over");
    let v = loaded.world().get::<VisibleCells>(e).unwrap();
    assert!(
        !v.is_visible(grid, CellCoord::new(0, 0)),
        "stale size reads empty"
    );
    // A fitting save reports nothing.
    let snap = save(saved.world_mut()).unwrap();
    let (mut again, _) = make();
    run_ticks(again.world_mut(), 0);
    load(again.world_mut(), &snap, LoadMode::Strict).unwrap();
    assert!(again.world().get_resource::<FogResets>().is_none());
}

/// Review round 2: a range beyond `MAX_TABLE_RADIUS_CELLS` builds no ray
/// table and traces each cell instead: the same cells as the reference,
/// through `reveal_cells`, `reveal_reach_cells` and the pass.
#[test]
fn long_range_sensors_match_the_reference() {
    let mut app = app();
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for x in -32i32..32 {
            for z in -32i32..32 {
                if (x * 5 + z * 7).rem_euclid(13) == 0 {
                    let c = CellCoord::new(x, z);
                    let mut info = grid.static_cell(c);
                    info.flags.occluding = true;
                    grid.set_static(c, info);
                }
            }
        }
    }
    let far = npc_perception::fog::MAX_TABLE_RADIUS_CELLS * 1000 + 15_500;
    let senses = Sensors::new().vision(far, 45).vision(RANGE_MM, HALF_FOV);
    let reach = ReachSense { radius_mm: far };
    let from = SimPos::new(-20_300, -18_700);
    let facing = Facing::new(2, 3);
    let (_, e) = spawn_sim(
        app.world_mut(),
        (from, facing, FixedFacing, senses.clone(), reach, RevealsFog),
    );
    run_ticks(app.world_mut(), 1);
    let grid = app.world().resource::<SpatialGrid>();
    let visions: Vec<Vision> = senses.visions().collect();
    let got = reveal_cells(grid, from, facing, &senses.sensors[0]);
    assert_eq!(got, reference(grid, from, facing, &visions[..1], None));
    let got = reveal_reach_cells(grid, from, reach);
    assert_eq!(got, reference(grid, from, facing, &[], Some(far)));
    let want = reference(grid, from, facing, &visions, Some(far));
    let v = app.world().get::<VisibleCells>(e).unwrap();
    let b = grid.bounds();
    let mut got: Vec<CellCoord> = (0..u32::try_from(b.len()).unwrap())
        .filter(|&i| v.get(i))
        .map(|i| b.coord(i))
        .collect();
    got.sort_unstable();
    assert_eq!(got, want);
    assert!(got.len() > 300, "{}", got.len());
}

/// Review round 2: a `Vision` with a `cos2_ppm` above `COS2_SCALE` (the
/// fields are public and deserialized) neither overflows nor differs from
/// `in_fov`.
#[test]
fn out_of_range_cos2_does_not_overflow() {
    let mut app = app();
    run_ticks(app.world_mut(), 1);
    let grid = app.world().resource::<SpatialGrid>();
    for half_fov_deg in [30, 120] {
        let v = Vision {
            range_mm: RANGE_MM,
            half_fov_deg,
            cos2_ppm: u32::MAX,
        };
        let from = cell(0, 0);
        let got = reveal_cells(grid, from, Facing::new(30_000, 29_000), &Sensor::Vision(v));
        let want = reference(grid, from, Facing::new(30_000, 29_000), &[v], None);
        assert_eq!(got, want, "half {half_fov_deg}");
    }
}

/// Review round 2: an idle actor's visible cells follow the dynamic layer:
/// a `GridOccluder` spawned in front of it, then moved aside.
#[test]
fn idle_scans_follow_occluders_that_spawn_and_move() {
    let mut app = app();
    let (_, e) = eye(&mut app, cell(0, 0), NORTH, true);
    run_ticks(app.world_mut(), 2);
    assert!(visible(&app, e, (0, 4)));
    let (_, wall) = spawn_sim(app.world_mut(), (cell(0, 2), GridOccluder::default()));
    run_ticks(app.world_mut(), 1);
    assert!(visible(&app, e, (0, 2)), "the occluder's own cell");
    assert!(!visible(&app, e, (0, 4)), "behind it");
    *app.world_mut().get_mut::<SimPos>(wall).unwrap() = cell(5, 2);
    run_ticks(app.world_mut(), 1);
    assert!(visible(&app, e, (0, 4)), "moved aside");
}
