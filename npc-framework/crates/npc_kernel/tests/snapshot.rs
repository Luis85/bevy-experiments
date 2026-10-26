//! ADR 0013: the world snapshot registry, save and load (game menu plan Task 1).

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash, SimHashResource, state_hash};
use npc_kernel::journal::set_journal;
use npc_kernel::snapshot::{
    self, LoadError, LoadMode, ResourceSnapshotFns, SNAPSHOT_VERSION, SnapshotAppExt, load, save,
};
use npc_kernel::spatial::{
    CellInfo, Footprint, GridBlocker, GridOccluder, SpatialGrid, SpatialPlugin,
    register_spatial_hash,
};
use npc_kernel::{
    Journal, KernelAppExt, NpcKernelPlugin, NpcSet, NpcSimTick, PrevSimPos, SimClock, SimId,
    SimIdAllocator, SimPos, content_hash, run_ticks, spawn_sim,
};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
struct A(i32);

impl SimHash for A {
    const NAME: &'static str = "a";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.0);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
struct B {
    v: Vec<u32>,
}

impl SimHash for B {
    const NAME: &'static str = "b";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.v.len()).unwrap());
        for &x in &self.v {
            h.u32(x);
        }
    }
}

/// A component registered for snapshots only (not hashed).
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct C(u8);

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct R {
    total: u64,
}

impl SimHashResource for R {
    const NAME: &'static str = "R";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.total);
    }
    fn snapshot_fns() -> Option<ResourceSnapshotFns> {
        Some(snapshot::serde_resource_fns::<Self>())
    }
}

/// Every tick: each `A` grows by 1, `B` gains the tick, `R` counts the `A`s.
fn step(clock: Res<SimClock>, mut r: ResMut<R>, mut a: Query<&mut A>, mut b: Query<&mut B>) {
    for mut a in &mut a {
        a.0 += 1;
        r.total += 1;
    }
    for mut b in &mut b {
        b.v.push(u32::try_from(clock.tick).unwrap());
    }
}

fn build(extra: bool) -> App {
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_sim_component::<A>()
        .register_sim_component::<B>()
        .init_resource::<R>()
        .register_sim_resource::<R>()
        .add_systems(NpcSimTick, step.in_set(NpcSet::Effects));
    if extra {
        app.register_snapshot_component_named::<C>("c");
    }
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    build(false)
}

/// Three sim entities (one with `SimPos`), run a few ticks.
fn populated() -> App {
    let mut app = app();
    let world = app.world_mut();
    spawn_sim(world, (A(1),));
    spawn_sim(world, (A(2), B { v: vec![7] }));
    spawn_sim(world, (SimPos::new(1000, -500),));
    run_ticks(world, 5);
    app
}

#[test]
fn save_lists_entities_by_sim_id_and_components_by_name() {
    let mut app = app();
    let world = app.world_mut();
    // Spawned out of SimId order, so Entity order differs from SimId order.
    world.spawn((SimId(3), A(30)));
    world.spawn((SimId(1), B { v: vec![1] }, A(10)));
    world.spawn((SimId(2), SimPos::new(5, 6)));
    world.resource_mut::<SimIdAllocator>().next = 4;
    let snap = save(world).unwrap();
    assert_eq!(snap.version, SNAPSHOT_VERSION);
    assert_eq!(snap.content_hash, content_hash(world));
    let ids: Vec<u64> = snap.entities.iter().map(|e| e.id).collect();
    assert_eq!(ids, [1, 2, 3]);
    let keys: Vec<&str> = snap.entities[0]
        .components
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["a", "b"]);
    assert_eq!(
        snap.entities[0].components["b"],
        serde_json::json!({"v": [1]})
    );
    assert_eq!(
        snap.entities[1].components["pos"],
        serde_json::json!({"x": 5, "z": 6})
    );
    let resources: Vec<&str> = snap.resources.keys().map(String::as_str).collect();
    assert_eq!(
        resources,
        ["R", "command_queue", "sim_clock", "sim_id_allocator"]
    );
}

#[test]
fn save_load_save_is_identity() {
    let mut a = populated();
    let s1 = save(a.world_mut()).unwrap();
    assert_eq!(s1.tick, 5);
    let mut fresh = app();
    let report = load(fresh.world_mut(), &s1, LoadMode::Strict).unwrap();
    assert!(
        report.skipped.is_empty() && report.defaulted.is_empty(),
        "{report:?}"
    );
    assert_eq!(save(fresh.world_mut()).unwrap(), s1);
    assert_eq!(state_hash(fresh.world_mut()), state_hash(a.world_mut()));
    // The loaded world keeps running like the original.
    run_ticks(a.world_mut(), 7);
    run_ticks(fresh.world_mut(), 7);
    assert_eq!(state_hash(fresh.world_mut()), state_hash(a.world_mut()));
}

#[test]
fn load_restores_allocator_and_clock() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let mut fresh = app();
    // The fresh build has its own sim entities: they are replaced.
    spawn_sim(fresh.world_mut(), (A(99),));
    set_journal(fresh.world_mut(), true, false);
    load(fresh.world_mut(), &snap, LoadMode::Strict).unwrap();
    let world = fresh.world_mut();
    assert_eq!(world.resource::<SimClock>().tick, snap.tick);
    assert!(world.resource::<Journal>().entries.is_empty());
    assert!(!world.contains_resource::<snapshot::SnapshotLoading>());
    // PrevSimPos follows SimPos.
    let mut q = world.query::<(&SimPos, &PrevSimPos)>();
    let rows: Vec<_> = q.iter(world).map(|(p, prev)| (*p, prev.0)).collect();
    assert_eq!(rows, [(SimPos::new(1000, -500), SimPos::new(1000, -500))]);
    let (id, _) = spawn_sim(world, (A(0),));
    assert_eq!(id, SimId(4));
    assert_eq!(world.query::<&SimId>().iter(world).count(), 4);
}

/// Counts `SimId` leave-hook calls.
#[derive(Resource, Default)]
struct Left(u32);

#[test]
fn load_skips_leave_hooks_and_journal() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let mut fresh = App::new();
    fresh
        .add_plugins(NpcKernelPlugin::default())
        .register_sim_component::<A>()
        .register_sim_component::<B>()
        .init_resource::<R>()
        .register_sim_resource::<R>()
        .init_resource::<Left>()
        .on_sim_id_leave("count", |world, _, _| world.resource_mut::<Left>().0 += 1);
    fresh.finish();
    fresh.cleanup();
    let world = fresh.world_mut();
    let (_, e) = spawn_sim(world, (A(5),));
    set_journal(world, true, false);
    load(world, &snap, LoadMode::Strict).unwrap();
    assert_eq!(world.resource::<Left>().0, 0);
    assert!(world.resource::<Journal>().entries.is_empty());
    assert!(world.get_entity(e).is_err(), "the built entity is replaced");
    // After the load, leaving the sim runs the hooks again.
    let gone = world
        .resource::<npc_kernel::SimIndex>()
        .get(SimId(1))
        .unwrap();
    world.despawn(gone);
    assert_eq!(world.resource::<Left>().0, 1);
}

#[test]
fn strict_load_rejects_other_version_and_content_hash() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let mut other = snap.clone();
    other.version = 99;
    let mut fresh = app();
    assert!(matches!(
        load(fresh.world_mut(), &other, LoadMode::Strict),
        Err(LoadError::Version {
            found: 99,
            expected: SNAPSHOT_VERSION
        })
    ));
    let mut other = snap.clone();
    other.content_hash ^= 1;
    let err = load(fresh.world_mut(), &other, LoadMode::Strict).unwrap_err();
    assert!(
        matches!(err, LoadError::ContentHash { found, expected }
            if found == snap.content_hash ^ 1 && expected == snap.content_hash),
        "{err}"
    );
    assert!(err.to_string().contains("content hash"), "{err}");
    // Lenient loads it anyway and says so.
    let report = load(fresh.world_mut(), &other, LoadMode::Lenient).unwrap();
    assert_eq!(report.skipped.len(), 1, "{report:?}");
    assert!(report.skipped[0].contains("content hash"), "{report:?}");
}

#[test]
fn lenient_load_reports_unknown_and_missing() {
    let mut a = populated();
    let mut snap = save(a.world_mut()).unwrap();
    snap.entities[0]
        .components
        .insert("Gone".to_owned(), serde_json::json!(1));
    snap.resources.remove("R");
    let mut fresh = app();
    fresh.world_mut().resource_mut::<R>().total = 42;
    let report = load(fresh.world_mut(), &snap, LoadMode::Lenient).unwrap();
    assert_eq!(report.skipped, ["Gone"]);
    assert_eq!(report.defaulted, ["R"]);
    // The built value is kept; the known component still loads.
    let world = fresh.world_mut();
    assert_eq!(world.resource::<R>().total, 42);
    assert_eq!(world.query::<&A>().iter(world).count(), 2);
}

#[test]
fn registering_snapshot_entries_keeps_content_hash() {
    let mut plain = build(false);
    let mut extra = build(true);
    assert_eq!(
        content_hash(plain.world_mut()),
        content_hash(extra.world_mut())
    );
    // The extra entry is saved under its explicit name and loads back.
    let world = extra.world_mut();
    spawn_sim(world, (C(3),));
    let snap = save(world).unwrap();
    assert_eq!(snap.entities[0].components["c"], serde_json::json!(3));
    let mut fresh = build(true);
    load(fresh.world_mut(), &snap, LoadMode::Strict).unwrap();
    let world = fresh.world_mut();
    let cs: Vec<C> = world.query::<&C>().iter(world).cloned().collect();
    assert_eq!(cs, [C(3)]);
}

#[test]
fn lenient_load_without_allocator_never_reuses_loaded_ids() {
    let mut a = populated();
    let mut snap = save(a.world_mut()).unwrap();
    snap.resources.remove("sim_id_allocator");
    let mut fresh = app();
    let report = load(fresh.world_mut(), &snap, LoadMode::Lenient).unwrap();
    assert!(report.defaulted.contains(&"sim_id_allocator".to_owned()));
    // The fresh build's allocator would start at 1: the load moves it past
    // the highest loaded id.
    let (id, _) = spawn_sim(fresh.world_mut(), (A(0),));
    assert_eq!(id, SimId(4));
}

#[test]
fn bad_entity_id_is_refused_before_anything_changes() {
    let mut a = populated();
    let mut snap = save(a.world_mut()).unwrap();
    snap.entities.last_mut().unwrap().id = u64::from(u32::MAX) + 1;
    let mut fresh = app();
    spawn_sim(fresh.world_mut(), (A(99),));
    let before = save(fresh.world_mut()).unwrap();
    for mode in [LoadMode::Strict, LoadMode::Lenient] {
        let err = load(fresh.world_mut(), &snap, mode).unwrap_err();
        assert!(matches!(err, LoadError::Bad(_)), "{err}");
        assert_eq!(save(fresh.world_mut()).unwrap(), before, "world untouched");
    }
    let mut zero = save(a.world_mut()).unwrap();
    zero.entities[0].id = 0;
    assert!(matches!(
        load(fresh.world_mut(), &zero, LoadMode::Lenient),
        Err(LoadError::Bad(_))
    ));
    assert_eq!(save(fresh.world_mut()).unwrap(), before);
}

/// Counts the `A`s a system sees as changed (or added) since it last ran.
#[derive(Resource, Default)]
struct SeenChanged(Vec<usize>);

fn count_changed(q: Query<(), Changed<A>>, mut seen: ResMut<SeenChanged>) {
    seen.0.push(q.iter().count());
}

#[test]
fn loaded_components_do_not_look_changed() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let mut fresh = App::new();
    fresh
        .add_plugins(NpcKernelPlugin::default())
        .register_sim_component::<A>()
        .register_sim_component::<B>()
        .init_resource::<R>()
        .register_sim_resource::<R>()
        .init_resource::<SeenChanged>()
        // Before `step` (Effects): sees only what changed since its last run.
        .add_systems(NpcSimTick, step.in_set(NpcSet::Effects))
        .add_systems(NpcSimTick, count_changed.in_set(NpcSet::Needs));
    fresh.finish();
    fresh.cleanup();
    load(fresh.world_mut(), &snap, LoadMode::Strict).unwrap();
    // The system never ran: without settling, both loaded `A`s would look added.
    run_ticks(fresh.world_mut(), 2);
    assert_eq!(fresh.world().resource::<SeenChanged>().0, [0, 2]);
}

/// A fresh empty directory under the system temp dir, per test name.
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("npc_snapshot_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn write_atomic_leaves_no_tmp() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let dir = temp_dir("atomic");
    let file = dir.join("save.json");
    snapshot::write_atomic(&file, &snap).unwrap();
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["save.json"]);
    assert_eq!(snapshot::read(&file).unwrap(), snap);
    // Overwriting an existing save works (rename over a file, also on Windows).
    let mut other = snap.clone();
    other.tick += 1;
    snapshot::write_atomic(&file, &other).unwrap();
    assert_eq!(snapshot::read(&file).unwrap(), other);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn read_reports_truncated_file() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let dir = temp_dir("truncated");
    let file = dir.join("save.json");
    snapshot::write_atomic(&file, &snap).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, &text[..text.len() / 2]).unwrap();
    let err = snapshot::read(&file).unwrap_err();
    assert!(err.contains("unreadable save"), "{err}");
    assert!(err.contains(&file.display().to_string()), "{err}");
    let missing = dir.join("missing.json");
    let err = snapshot::read(&missing).unwrap_err();
    assert!(
        err.contains("unreadable save") && err.contains("missing.json"),
        "{err}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_flags_content_hash() {
    let mut a = populated();
    let snap = save(a.world_mut()).unwrap();
    let mut same = app();
    assert_eq!(snapshot::check(same.world_mut(), &snap), Ok(()));
    // A save from another build carries another content hash.
    let mut foreign = snap.clone();
    foreign.content_hash ^= 1;
    assert!(matches!(
        snapshot::check(same.world_mut(), &foreign),
        Err(LoadError::ContentHash { .. })
    ));
    let mut old = snap.clone();
    old.version = 99;
    assert!(matches!(
        snapshot::check(same.world_mut(), &old),
        Err(LoadError::Version { found: 99, .. })
    ));
    // Checking never loads: the world is unchanged.
    assert_eq!(same.world().resource::<SimClock>().tick, 0);
}

/// Whether and when the spatial grid joins the state hash.
#[derive(Clone, Copy, Debug)]
enum SpatialHash {
    /// `SpatialPlugin` alone: no capability calls `register_spatial_hash`.
    Off,
    /// `register_spatial_hash` before `SpatialPlugin` is added.
    Before,
    /// `register_spatial_hash` after `SpatialPlugin` (what the capabilities do).
    After,
}

fn spatial_app(hash: SpatialHash) -> App {
    let mut app = App::new();
    if matches!(hash, SpatialHash::Before) {
        register_spatial_hash(&mut app);
    }
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin));
    if matches!(hash, SpatialHash::After) {
        register_spatial_hash(&mut app);
    }
    app.finish();
    app.cleanup();
    app
}

/// Every in-bounds cell's effective info (static + dynamic layer).
fn grid_cells(world: &World) -> Vec<CellInfo> {
    let g = world.resource::<SpatialGrid>();
    (0..g.bounds().len())
        .map(|i| g.cell_at(u32::try_from(i).unwrap()))
        .collect()
}

fn footprints(world: &mut World) -> Vec<(SimId, Option<GridBlocker>, Option<GridOccluder>)> {
    let mut rows: Vec<_> = world
        .query_filtered::<(&SimId, Option<&GridBlocker>, Option<&GridOccluder>), Or<(With<GridBlocker>, With<GridOccluder>)>>()
        .iter(world)
        .map(|(&id, b, o)| (id, b.copied(), o.copied()))
        .collect();
    rows.sort_by_key(|r| r.0);
    rows
}

fn move_to(world: &mut World, id: SimId, pos: SimPos) {
    let e = world.resource::<npc_kernel::SimIndex>().get(id).unwrap();
    *world.get_mut::<SimPos>(e).unwrap() = pos;
}

/// Docs review: `GridBlocker` / `GridOccluder` are read by
/// `spatial_update_system` on the next tick, so a host with `SpatialPlugin`
/// and no capability that hashes them must still save them (ADR 0013).
#[test]
fn spatial_plugin_saves_blockers_and_occluders_with_or_without_hashing() {
    let fp = Footprint {
        half_x_mm: 1000,
        half_z_mm: 0,
    };
    for hash in [SpatialHash::Off, SpatialHash::Before, SpatialHash::After] {
        let mut a = spatial_app(hash);
        let w = a.world_mut();
        let (wall, _) = spawn_sim(w, (SimPos::new(2000, 3000), GridBlocker(fp)));
        let (hedge, _) = spawn_sim(w, (SimPos::new(-4000, 1000), GridOccluder(fp)));
        run_ticks(w, 3);
        let before = grid_cells(w);
        assert!(before.iter().any(|c| c.flags.blocking), "{hash:?}");
        assert!(before.iter().any(|c| c.flags.occluding), "{hash:?}");

        let snap = save(w).unwrap();
        let entity = |id: SimId| {
            snap.entities
                .iter()
                .find(|e| e.id == u64::from(id.0))
                .unwrap()
        };
        assert_eq!(
            entity(wall).components.get("grid_blocker"),
            Some(&serde_json::json!({"half_x_mm": 1000, "half_z_mm": 0})),
            "{hash:?}"
        );
        assert!(
            entity(hedge).components.contains_key("grid_occluder"),
            "{hash:?}"
        );

        let mut fresh = spatial_app(hash);
        let report = load(fresh.world_mut(), &snap, LoadMode::Strict).unwrap();
        assert!(
            report.skipped.is_empty() && report.defaulted.is_empty(),
            "{hash:?}: {report:?}"
        );
        let f = fresh.world_mut();
        assert_eq!(footprints(f), footprints(a.world_mut()), "{hash:?}");
        assert_eq!(grid_cells(f), before, "{hash:?}");

        // The first update after the load re-syncs against the loaded
        // footprints: the cells stay as they are, and a blocker moved later
        // moves its cells in both worlds alike.
        run_ticks(a.world_mut(), 2);
        run_ticks(fresh.world_mut(), 2);
        assert_eq!(
            grid_cells(fresh.world()),
            grid_cells(a.world()),
            "{hash:?}: after resuming"
        );
        move_to(a.world_mut(), wall, SimPos::new(6000, -2000));
        move_to(fresh.world_mut(), wall, SimPos::new(6000, -2000));
        run_ticks(a.world_mut(), 1);
        run_ticks(fresh.world_mut(), 1);
        assert_eq!(
            grid_cells(fresh.world()),
            grid_cells(a.world()),
            "{hash:?}: after a move"
        );
        assert_ne!(grid_cells(a.world()), before, "{hash:?}: the wall moved");
        assert_eq!(
            save(fresh.world_mut()).unwrap(),
            save(a.world_mut()).unwrap(),
            "{hash:?}"
        );
        assert_eq!(
            state_hash(fresh.world_mut()),
            state_hash(a.world_mut()),
            "{hash:?}"
        );
    }
}

/// The snapshot entries `SpatialPlugin` adds and those `register_spatial_hash`
/// adds share their names, so either order builds (the second is a no-op)
/// and neither changes the content hash.
#[test]
fn spatial_snapshot_entries_and_hash_registration_in_either_order() {
    let mut off = spatial_app(SpatialHash::Off);
    let mut before = spatial_app(SpatialHash::Before);
    let mut after = spatial_app(SpatialHash::After);
    for app in [&off, &before, &after] {
        let names = app
            .world()
            .resource::<snapshot::SnapshotRegistry>()
            .component_names();
        assert!(names.contains(&GridBlocker::NAME), "{names:?}");
        assert!(names.contains(&GridOccluder::NAME), "{names:?}");
    }
    assert_eq!(
        content_hash(before.world_mut()),
        content_hash(after.world_mut())
    );
    // Hashing the grid is what changes the content hash, not the snapshot
    // entries.
    assert_ne!(
        content_hash(off.world_mut()),
        content_hash(after.world_mut())
    );
    let mut bare = App::new();
    bare.add_plugins(NpcKernelPlugin::default());
    bare.finish();
    bare.cleanup();
    assert_eq!(
        content_hash(off.world_mut()),
        content_hash(bare.world_mut())
    );
}
