//! Ground plane, sun light and debug grid for the current map.
use bevy::gizmos::config::GizmoConfigStore;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::prelude::*;

use crate::camera::{RtsCamera, ground_depth_range};
use crate::debug::{OverlayAppExt, OverlayToggles};
use crate::terrain::{TerrainView, terrain_mesh};

/// Map extent and look; insert before startup to get a ground plane.
#[derive(Resource, Clone, Debug)]
pub struct MapView {
    /// Map size in metres (sim x, sim y).
    pub size_m: Vec2,
    /// Ground colour.
    pub ground_color: Color,
    /// The heightfield drawn instead of the flat plane; `None` is flat ground at height 0.
    pub terrain: Option<TerrainView>,
}

/// Whether the 100 m debug grid is drawn (overlay `"grid"`, toggled with F3).
#[derive(Resource, Default, Debug)]
struct GridVisible(bool);

const GRID_STEP_M: f32 = 100.0;
/// Spacing of the samples a grid line takes the terrain height at, metres.
const GRID_SAMPLE_M: f32 = 4.0;
/// Height of the grid above the ground, metres.
const GRID_LIFT_M: f32 = 0.1;

/// Spawns ground and lighting, and draws the optional debug grid.
pub struct MapViewPlugin;

impl Plugin for MapViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GridVisible>()
            .insert_resource(GlobalAmbientLight {
                brightness: 300.0,
                ..default()
            })
            .add_systems(Startup, spawn_map_view)
            .register_overlay("grid", Some(KeyCode::F3), toggle_grid)
            .add_systems(PostUpdate, tune_cascades)
            .add_systems(Update, refresh_ground)
            .add_systems(
                Update,
                draw_grid
                    .after(OverlayToggles)
                    .run_if(resource_exists::<GizmoConfigStore>)
                    .run_if(|v: Res<GridVisible>| v.0),
            );
    }
}

/// The ground visual of the current map: the terrain mesh or the flat plane.
#[derive(Component)]
pub struct MapGround;

/// Spawns the [`MapGround`] of `map`.
fn spawn_ground(
    commands: &mut Commands,
    map: &MapView,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let (w, h) = (map.size_m.x, map.size_m.y);
    match &map.terrain {
        // Vertex colours carry the terrain types; the material only lights them.
        Some(terrain) => commands.spawn((
            Name::new("Terrain"),
            MapGround,
            Mesh3d(meshes.add(terrain_mesh(terrain))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 1.0,
                ..default()
            })),
        )),
        None => commands.spawn((
            Name::new("Ground"),
            MapGround,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(w, h))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: map.ground_color,
                perceptual_roughness: 1.0,
                ..default()
            })),
            Transform::from_translation(Vec3::new(w * 0.5, 0.0, h * 0.5)),
        )),
    };
}

fn spawn_map_view(
    mut commands: Commands,
    map: Option<Res<MapView>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(map) = map else {
        return;
    };
    let (w, h) = (map.size_m.x, map.size_m.y);
    let centre = Vec3::new(w * 0.5, 0.0, h * 0.5);
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        spawn_ground(&mut commands, &map, &mut meshes, &mut materials);
    }
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(centre + Vec3::new(-300.0, 500.0, 200.0))
            .looking_at(centre, Vec3::Y),
        cascade_config(&RtsCamera::default()),
    ));
}

/// Shadow cascades covering exactly the visible ground of the rig.
///
/// Cascade bounds are view depths from the camera plane. The visible ground spans
/// `ground_depth_range` (far from the camera: ~400 m at the stock rig), so cascades starting
/// at 0 would spend their best resolution on empty depth. The first cascade ends at 15% of
/// the span so near ground, where the player looks, gets the densest map.
pub fn cascade_config(rig: &RtsCamera) -> CascadeShadowConfig {
    let (near, far) = ground_depth_range(rig);
    let min = (near - 1.0).max(0.0);
    let max = (far + 1.0).max(min + 2.0);
    CascadeShadowConfigBuilder {
        num_cascades: 4,
        minimum_distance: min,
        first_cascade_far_bound: min + (max - min) * 0.15,
        maximum_distance: max,
        ..default()
    }
    .build()
}

/// Replaces the [`MapGround`] when a loaded match swaps in another [`MapView`].
fn refresh_ground(
    mut commands: Commands,
    map: Option<Res<MapView>>,
    old: Query<Entity, With<MapGround>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(map) = map.filter(|map| map.is_changed() && !map.is_added()) else {
        return;
    };
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for entity in &old {
        commands.entity(entity).despawn();
    }
    spawn_ground(&mut commands, &map, &mut meshes, &mut materials);
}

/// Re-derives the sun cascades when the camera zoom or distance changes.
fn tune_cascades(
    rigs: Query<&RtsCamera, Changed<RtsCamera>>,
    mut suns: Query<&mut CascadeShadowConfig, With<DirectionalLight>>,
) {
    let Some(rig) = rigs.iter().next() else {
        return;
    };
    for mut cfg in &mut suns {
        *cfg = cascade_config(rig);
    }
}

/// The `"grid"` overlay toggle.
fn toggle_grid(world: &mut World) -> bool {
    let mut visible = world.get_resource_or_init::<GridVisible>();
    visible.0 = !visible.0;
    visible.0
}

fn draw_grid(map: Option<Res<MapView>>, mut gizmos: Gizmos) {
    let Some(map) = map else {
        return;
    };
    let color = Color::srgba(1.0, 1.0, 1.0, 0.35);
    let (w, h) = (map.size_m.x, map.size_m.y);
    let terrain = map.terrain.as_ref();
    // A line rides the terrain, sampled every few metres, or stays flat without terrain.
    let line = |from: Vec3, to: Vec3| {
        let steps = if terrain.is_some() {
            ((to - from).length() / GRID_SAMPLE_M).ceil().max(1.0) as u32
        } else {
            1
        };
        (0..=steps).map(move |i| {
            let p = from.lerp(to, i as f32 / steps as f32);
            let ground = terrain.map_or(0.0, |t| t.height_at(p.x, p.z));
            p.with_y(ground + GRID_LIFT_M)
        })
    };
    let mut x = 0.0;
    while x <= w {
        gizmos.linestrip(line(Vec3::new(x, 0.0, 0.0), Vec3::new(x, 0.0, h)), color);
        x += GRID_STEP_M;
    }
    let mut z = 0.0;
    while z <= h {
        gizmos.linestrip(line(Vec3::new(0.0, 0.0, z), Vec3::new(w, 0.0, z)), color);
        z += GRID_STEP_M;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascades_start_at_visible_ground() {
        let rig = RtsCamera::default();
        let (near, far) = ground_depth_range(&rig);
        let cfg = cascade_config(&rig);
        assert!(cfg.minimum_distance > near - 2.0 && cfg.minimum_distance < near);
        assert!(cfg.bounds.last().copied().unwrap() >= far);
        assert!(cfg.bounds[0] > cfg.minimum_distance);
    }
}
