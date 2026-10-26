//! Terrain view tests: the heightfield built from the base testbed's layers, its mesh, ground
//! picking, and unit visuals standing on it.
//!
//! The client never sees `fl_sim`: the unit test drives `SimViewPlugin` with a fake runner on
//! `fl_view::runner_channel` that acknowledges each tick.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::driver::LocalDriver;
use fl_client::link::RunnerLink;
use fl_client::map_view::{MapGround, MapView, MapViewPlugin};
use fl_client::terrain::{TerrainView, ray_ground, terrain_mesh};
use fl_client::view::{SimViewPlugin, ViewPose};
use fl_content::{ContentDb, DefRegistry, MapDef, TerrainDef, load_dirs};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{MatchHeader, PlayerId, SimId, StringTables, Tick, UnitTypeId};
use fl_view::{
    EntityRecord, FromRunner, RunnerPort, SimSnapshot, TickDriver, TickOutput, ToRunner,
    runner_channel,
};

const ACK_TIMEOUT: Duration = Duration::from_secs(10);
const TESTBED: &str = "base.testbed_1km";
/// The testbed's hill: centre cell (60, 200), 12 m high.
const HILL_CELL: (u32, u32) = (60, 200);

/// A scratch directory removed on drop, also when the test panics.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The base mod's terrain and maps in a scratch mod, so no sim feature has to register the
/// components of its units. The scratch directory is per process.
fn base_terrain_content() -> ContentDb {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base");
    let scratch =
        ScratchDir(std::env::temp_dir().join(format!("fl_client_terrain_{}", std::process::id())));
    let dir = &scratch.0;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::copy(base.join("mod.ron"), dir.join("mod.ron")).unwrap();
    for sub in ["maps", "terrain"] {
        copy_dir(&base.join(sub), &dir.join(sub));
    }
    load_dirs(DefRegistry::with_builtins(), std::slice::from_ref(dir))
        .unwrap_or_else(|errors| panic!("{errors}"))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn testbed_view(content: &ContentDb) -> TerrainView {
    let map = content.kind::<MapDef>().get(TESTBED).expect("testbed map");
    TerrainView::from_content(map, content.kind::<TerrainDef>()).expect("testbed has layers")
}

fn small_view(cells: UVec2, heights_m: Vec<f32>) -> TerrainView {
    let n = heights_m.len();
    TerrainView {
        cells,
        cell_m: 4.0,
        heights_m,
        colors: vec![[0.2, 0.4, 0.6]; n],
    }
}

fn hill_centre_m() -> (f32, f32) {
    (
        (HILL_CELL.0 as f32 + 0.5) * 4.0,
        (HILL_CELL.1 as f32 + 0.5) * 4.0,
    )
}

#[test]
fn terrain_view_from_base_testbed() {
    let content = base_terrain_content();
    let view = testbed_view(&content);
    assert_eq!(view.cells, UVec2::new(250, 250));
    assert_eq!(view.heights_m.len(), 250 * 250);
    let water = content
        .kind::<TerrainDef>()
        .get("base.water")
        .unwrap()
        .color;
    let i = 124 * 250 + 10;
    assert_eq!(view.colors[i], [water.0, water.1, water.2]);
    let (x, z) = hill_centre_m();
    assert!(view.height_at(x, z) > 11.0);
    // A map without layers has no view.
    let mut open = content.kind::<MapDef>().get(TESTBED).unwrap().clone();
    open.layers = None;
    assert_eq!(
        TerrainView::from_content(&open, content.kind::<TerrainDef>()),
        None
    );
}

#[test]
fn height_at_bilinear_and_clamped() {
    let view = small_view(UVec2::new(2, 1), vec![0.0, 2.0]);
    // Cell centres are at x = 2 and x = 6.
    assert!((view.height_at(4.0, 2.0) - 1.0).abs() < 1e-6);
    assert!((view.height_at(2.0, 2.0) - 0.0).abs() < 1e-6);
    assert!((view.height_at(-50.0, 2.0) - 0.0).abs() < 1e-6);
    assert!((view.height_at(500.0, -9.0) - 2.0).abs() < 1e-6);
}

#[test]
fn terrain_mesh_counts() {
    let view = small_view(UVec2::new(4, 3), (0..12).map(|i| i as f32).collect());
    let mesh = terrain_mesh(&view);
    assert_eq!(mesh.count_vertices(), 12);
    let Some(Indices::U32(indices)) = mesh.indices() else {
        panic!("u32 indices");
    };
    assert_eq!(indices.len(), 36);
    assert!(indices.iter().all(|&i| i < 12));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("positions");
    };
    // Cell (1, 2): x = 1.5 cells, z = 2.5 cells, height 9.
    assert_eq!(positions[2 * 4 + 1], [6.0, 9.0, 10.0]);
    assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
}

#[test]
fn terrain_mesh_faces_up() {
    let view = small_view(UVec2::new(3, 3), vec![1.0; 9]);
    let mesh = terrain_mesh(&view);
    let Some(Indices::U32(indices)) = mesh.indices() else {
        panic!("u32 indices");
    };
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        panic!("positions");
    };
    for tri in indices.chunks(3) {
        let v = |k: usize| Vec3::from(p[tri[k] as usize]);
        let normal = (v(1) - v(0)).cross(v(2) - v(0));
        assert!(normal.y > 0.0, "{tri:?}");
    }
}

#[test]
fn ray_ground_hits_hill() {
    let content = base_terrain_content();
    let view = testbed_view(&content);
    let (x, z) = hill_centre_m();
    let ray = Ray3d::new(Vec3::new(x, 200.0, z), Dir3::NEG_Y);
    let hit = ray_ground(ray, Some(&view)).expect("hits the hill");
    assert!((hit.y - view.height_at(hit.x, hit.z)).abs() < 0.05, "{hit}");
    assert!((hit.y - view.height_at(x, z)).abs() < 0.05, "{hit}");
    // An oblique ray lands on the surface too.
    let dir = Dir3::new(Vec3::new(1.0, -1.0, 0.5)).unwrap();
    let origin = Vec3::new(x - 80.0, 100.0, z - 40.0);
    let hit = ray_ground(Ray3d::new(origin, dir), Some(&view)).expect("hits the terrain");
    assert!((hit.y - view.height_at(hit.x, hit.z)).abs() < 0.05, "{hit}");
}

#[test]
fn ray_ground_off_map_is_none() {
    let content = base_terrain_content();
    let view = testbed_view(&content);
    // Straight down just outside each edge of the 1 km map, and a ray that lands past the
    // far edge: no hit on the clamped, edge-extended heightfield.
    for (x, z) in [
        (-5.0, 500.0),
        (1005.0, 500.0),
        (500.0, -5.0),
        (500.0, 1005.0),
    ] {
        let ray = Ray3d::new(Vec3::new(x, 100.0, z), Dir3::NEG_Y);
        assert_eq!(ray_ground(ray, Some(&view)), None, "({x}, {z})");
    }
    let dir = Dir3::new(Vec3::new(1.0, -0.2, 0.0)).unwrap();
    let ray = Ray3d::new(Vec3::new(900.0, 30.0, 500.0), dir);
    assert_eq!(ray_ground(ray, Some(&view)), None);
    // Just inside the edge still hits.
    let ray = Ray3d::new(Vec3::new(995.0, 100.0, 500.0), Dir3::NEG_Y);
    assert!(ray_ground(ray, Some(&view)).is_some());
}

#[test]
fn ray_ground_flat_without_terrain() {
    let dir = Dir3::new(Vec3::new(1.0, -1.0, 0.0)).unwrap();
    let hit = ray_ground(Ray3d::new(Vec3::new(0.0, 10.0, 5.0), dir), None).unwrap();
    assert!((hit - Vec3::new(10.0, 0.0, 5.0)).length() < 1e-4, "{hit}");
    assert_eq!(ray_ground(Ray3d::new(Vec3::Y, Dir3::Y), None), None);
}

// --- units stand on the terrain ---

fn header() -> MatchHeader {
    MatchHeader {
        format_version: fl_protocol::FORMAT_VERSION,
        build_id: "test".to_owned(),
        sim_version: 1,
        features: Vec::new(),
        mods: Vec::new(),
        rules_hash: 0,
        cheats_allowed: true,
        seed: 1,
        string_tables: StringTables::default(),
    }
}

/// A runner thread that answers every tick with a snapshot of `entities`, holds the answer
/// until the test releases it, and acknowledges it, so the test never sleeps.
struct FakeRunner {
    release: Option<crossbeam_channel::Sender<()>>,
    acks: crossbeam_channel::Receiver<()>,
    shutdown: crossbeam_channel::Sender<ToRunner>,
    thread: Option<JoinHandle<()>>,
}

impl FakeRunner {
    fn spawn(entities: Vec<EntityRecord>) -> (Self, RunnerPort) {
        let (port, end) = runner_channel();
        let (ack_tx, acks) = crossbeam_channel::unbounded();
        let (release, released) = crossbeam_channel::unbounded::<()>();
        let thread = std::thread::spawn(move || {
            while let Ok(message) = end.rx.recv() {
                let input = match message {
                    ToRunner::Tick(input) => input,
                    ToRunner::Shutdown => return,
                    ToRunner::Query { .. } => continue,
                    // `ToRunner` is non-exhaustive; this fake never receives a `Control`.
                    _ => continue,
                };
                let out = TickOutput {
                    tick: input.tick,
                    hash: None,
                    snapshot: Arc::new(SimSnapshot {
                        tick: input.tick,
                        entities: entities.clone(),
                        channels: Vec::new(),
                    }),
                    events: Vec::new(),
                    journal: Vec::new(),
                };
                if released.recv().is_err() || end.tx.send(FromRunner::Tick(out)).is_err() {
                    return;
                }
                let _ = ack_tx.send(());
            }
        });
        let runner = FakeRunner {
            release: Some(release),
            acks,
            shutdown: port.tx.clone(),
            thread: Some(thread),
        };
        (runner, port)
    }

    /// One `app.update()`, then releases and awaits the answer to every tick it sent.
    fn update(&self, app: &mut App) {
        let before = app.world().resource::<LocalDriver>().next_tick();
        app.update();
        let after = app.world().resource::<LocalDriver>().next_tick();
        for _ in before.0..after.0 {
            self.release.as_ref().unwrap().send(()).unwrap();
            self.acks.recv_timeout(ACK_TIMEOUT).unwrap();
        }
    }
}

impl Drop for FakeRunner {
    fn drop(&mut self) {
        self.release = None;
        let _ = self.shutdown.send(ToRunner::Shutdown);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .expect("the fake runner thread does not panic");
        }
    }
}

#[test]
fn units_stand_on_terrain() {
    let content = base_terrain_content();
    let view = testbed_view(&content);
    let (x, z) = hill_centre_m();
    // Between cell centres, so the bilinear height is exercised.
    let (pos_x, pos_y) = (x as i32 + 1, z as i32 + 1);
    let record = EntityRecord {
        sim_id: SimId(1),
        unit: UnitTypeId(0),
        owner: PlayerId(0),
        pos: FxVec2::new(Fx::from_int(pos_x), Fx::from_int(pos_y)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    };
    let expected = view.height_at(pos_x as f32, pos_y as f32);
    assert!(expected > 10.0, "the unit is on the hill: {expected}");

    let (runner, port) = FakeRunner::spawn(vec![record]);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SimViewPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .insert_resource(MapView {
            size_m: Vec2::new(1000.0, 1000.0),
            ground_color: Color::BLACK,
            terrain: Some(view),
        })
        .insert_resource(RunnerLink {
            port,
            header: header(),
        })
        .insert_resource(LocalDriver::new(Tick(0)));
    for _ in 0..4 {
        runner.update(&mut app);
    }
    let world = app.world_mut();
    let mut query = world.query::<(&ViewPose, &Transform)>();
    let (pose, transform) = query.single(world).expect("one unit view");
    assert!((pose.curr.y - expected).abs() < 0.05, "{pose:?}");
    assert!(
        (transform.translation.y - expected).abs() < 0.05,
        "{:?}",
        transform.translation
    );
}

// --- a loaded match swaps the ground ---

#[test]
fn changed_map_view_replaces_the_ground() {
    let content = base_terrain_content();
    let view = testbed_view(&content);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins(MapViewPlugin)
        .insert_resource(MapView {
            size_m: Vec2::new(1000.0, 1000.0),
            ground_color: Color::BLACK,
            terrain: None,
        });
    let names = |app: &mut App| -> Vec<String> {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Name, With<MapGround>>();
        query.iter(world).map(|n| n.as_str().to_owned()).collect()
    };
    app.update();
    assert_eq!(names(&mut app), ["Ground"]);
    app.update();
    assert_eq!(
        names(&mut app),
        ["Ground"],
        "an unchanged map keeps its ground"
    );
    app.world_mut().resource_mut::<MapView>().terrain = Some(view);
    app.update();
    assert_eq!(names(&mut app), ["Terrain"]);
}
