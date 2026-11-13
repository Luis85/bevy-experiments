//! The whole island view in a headless app: building it on the shipped
//! `living_world` island and running a few frames fails on conflicting system
//! parameters (Bevy's B0001) and on missing resources, which only show when
//! the schedule first runs.
//!
//! Left out (no renderer, window or GPU here): `DefaultPlugins`' render,
//! window, winit, picking backends and the glTF loader. In
//! their place: `MinimalPlugins`, the asset, state, input and gizmo plugins,
//! the asset types the view uses, and `InputFocus` (`InputFocusPlugin`
//! provides it in the app). The models never finish loading, so the visual
//! roots stay without meshes.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use island_view::TestbedViewPlugin;
use npc_framework::prelude::SimControl;
use npc_framework::view_helpers::VisualIndex;

fn app() -> App {
    island_content::try_new_sim_app_with(7, "living_world", |app| {
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::state::app::StatesPlugin,
            bevy::input::InputPlugin,
            bevy::gizmos::GizmoPlugin,
            // Its output stays silent without a device; the island is muted.
            bevy::audio::AudioPlugin::default(),
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
        .init_resource::<bevy::input_focus::InputFocus>()
        .insert_resource(island_view::IslandAudioSettings::muted())
        // The host's driver resource (the window drives the clock; here
        // nothing ticks).
        .insert_resource(SimControl {
            paused: true,
            ..default()
        })
        .add_plugins(TestbedViewPlugin);
    })
    .expect("the shipped island builds")
}

#[test]
fn the_island_view_builds_and_runs_headless() {
    let mut app = app();
    for _ in 0..3 {
        app.update();
    }
    assert!(
        !app.world().resource::<VisualIndex>().map.is_empty(),
        "the island's residents and places get visuals"
    );
}
