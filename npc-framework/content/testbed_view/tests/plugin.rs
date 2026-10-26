//! The whole testbed view in a headless app: building it and running a few
//! frames fails on conflicting system parameters (Bevy's B0001) and on
//! missing resources, which only show when the schedule first runs. It has
//! the `TestbedViewPlugin` plus the explorer's prompt modal and F7 notes
//! panel (`ExplorerModalPlugin`, `ExplorerFiles`), as explorer mode adds
//! them.
//!
//! Left out (no renderer, window or GPU here): `DefaultPlugins`' render,
//! window, winit, picking backends and the glTF loader. In their place:
//! `MinimalPlugins`, the asset, state, input and gizmo plugins, the asset
//! types the view uses, and `InputFocus` (`InputFocusPlugin` provides it in
//! the app). The models never finish loading, so the visual roots stay
//! without meshes; the fog's decisions run on them anyway.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use npc_framework::perception::{
    ExploredCells, Percept, PerceptionMemory, RevealsFog, SenseKind, VisibleCells,
};
use npc_framework::prelude::{GridBounds, SimClock, SimId, SimIndex, SimPos, SpatialGrid};
use npc_framework::view_helpers::{ViewHidden, VisualIndex};
use testbed_content::TestbedUnit;
use testbed_content::world::Behaviour;
use testbed_view::{ExplorerFiles, ExplorerModalPlugin, FogOfWar, PromptModal, TestbedViewPlugin};

fn unit(id: u32, x: i32) -> impl Bundle {
    (
        SimId(id),
        SimPos::new(x, 500),
        TestbedUnit {
            template: "scout".into(),
            behaviour: Behaviour::Idle,
            speed_mm_per_tick: 70,
            home: SimPos::new(x, 500),
            wander_radius_mm: 0,
        },
    )
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::state::app::StatesPlugin,
        bevy::input::InputPlugin,
        bevy::gizmos::GizmoPlugin,
    ))
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_asset::<Image>()
    .init_asset::<Gltf>()
    .init_asset::<WorldAsset>()
    .init_asset::<AnimationClip>()
    .init_asset::<AnimationGraph>()
    .init_asset::<Font>()
    .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
    // `DefaultPlugins`' `InputFocusPlugin` provides it in the app.
    .init_resource::<bevy::input_focus::InputFocus>()
    .add_plugins(TestbedViewPlugin)
    .add_plugins(ExplorerModalPlugin)
    .insert_resource(PromptModal {
        title: "prompt".into(),
        text: "hello".into(),
        open: true,
        ..Default::default()
    })
    .insert_resource(ExplorerFiles {
        dir: std::env::temp_dir().join(format!("testbed-view-plugin-{}", std::process::id())),
    });
    app
}

/// Builds, runs, and checks that the fog hides an unsensed unit from the
/// generic helpers (`ViewHidden`) while the actor and a sensed unit show.
#[test]
fn view_plugin_runs_headless_and_fogs_units() {
    let mut app = app();
    let bounds = GridBounds::centered(16, 16);
    let world = app.world_mut();
    world.insert_resource(SpatialGrid::new(1000, bounds));
    world.insert_resource(SimClock::default());
    world.insert_resource(FogOfWar { enabled: true });
    let mut memory = PerceptionMemory::default();
    memory.percepts.insert(
        SimId(2),
        Percept {
            last_seen_tick: 0,
            pos: SimPos::new(2500, 500),
            via: SenseKind::Vision,
            confidence: 1000,
            kinds: 1,
            sensed: true,
        },
    );
    let words = ExploredCells::sized(bounds).bits.len();
    let actor = world
        .spawn((
            unit(1, 500),
            RevealsFog,
            ExploredCells::sized(bounds),
            VisibleCells {
                bits: vec![0; words],
            },
            memory,
        ))
        .id();
    let sensed = world.spawn(unit(2, 2500)).id();
    let unseen = world.spawn(unit(3, 4500)).id();
    let mut index = SimIndex::default();
    for (id, e) in [(1, actor), (2, sensed), (3, unseen)] {
        index.map.insert(SimId(id), e);
    }
    world.insert_resource(index);
    for _ in 0..5 {
        app.update();
    }
    let hidden = app.world().resource::<ViewHidden>();
    assert!(hidden.hides(unseen), "an unsensed unit is hidden");
    assert!(!hidden.hides(actor), "the actor always shows");
    assert!(!hidden.hides(sensed), "a sensed unit shows");

    // Its visual root is hidden too, and fog off shows everything.
    let root = |app: &App, sim| app.world().resource::<VisualIndex>().map.get(&sim).copied();
    let v = root(&app, unseen).expect("every unit gets a visual root");
    assert_eq!(app.world().get::<Visibility>(v), Some(&Visibility::Hidden));
    app.world_mut().resource_mut::<FogOfWar>().enabled = false;
    app.update();
    assert!(app.world().resource::<ViewHidden>().0.is_empty());
    assert_eq!(
        app.world().get::<Visibility>(v),
        Some(&Visibility::Inherited)
    );
    let dir = app.world().resource::<ExplorerFiles>().dir.clone();
    let _ = std::fs::remove_dir_all(dir);
}
