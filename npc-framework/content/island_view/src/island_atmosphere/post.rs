//! Island camera post-processing: an HDR target, restrained bloom for lamps,
//! windows and embers, and an explicit filmic tonemapper. UI is composited
//! after tonemapping and window screenshots read the final surface, so
//! neither is affected by the HDR intermediate.
use bevy::{
    camera::Hdr,
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    post_process::bloom::Bloom,
    prelude::*,
};

use crate::RtsCamera;

/// Scatter strength. Bevy's `NATURAL` preset is 0.15; the island's pale sand
/// and snow would haze at that level.
pub(crate) const BLOOM_INTENSITY: f32 = 0.09;

pub(crate) fn bloom() -> Bloom {
    Bloom {
        intensity: BLOOM_INTENSITY,
        ..Bloom::NATURAL
    }
}

/// `ISLAND_VIEW_POST=off` leaves the camera on Bevy's plain LDR path, for
/// comparing a capture with and without the post stack.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PostEnabled(pub bool);

/// Only the island's own play camera: preview and editor cameras keep theirs.
pub(crate) fn install(
    mut commands: Commands,
    enabled: Option<Res<PostEnabled>>,
    cameras: Query<Entity, (With<Camera3d>, With<RtsCamera>, Without<Bloom>)>,
) {
    if enabled.is_some_and(|enabled| !enabled.0) {
        return;
    }
    for entity in &cameras {
        commands.entity(entity).insert((
            Hdr,
            bloom(),
            Tonemapping::TonyMcMapface,
            DebandDither::Enabled,
        ));
    }
}

/// The sun's shadow cascades for an island camera `distance` metres from
/// its focus: tight and sharp up close (14 m, Live), reaching past the
/// whole view at island zoom so no shadow drops out there. The first
/// cascade's far bound and the maximum distance, metres.
pub(crate) fn cascade_bounds(distance: f32) -> (f32, f32) {
    let first = (distance * 0.9).clamp(10.0, 70.0);
    let farthest = (distance * 3.2).clamp(60.0, 520.0);
    (first, farthest)
}

/// Fit the sun's shadow cascades to the island camera, when its distance
/// has moved on by a fifth (cascades are not rebuilt every frame).
pub(crate) fn fit_cascades(
    mut commands: Commands,
    cameras: Query<&RtsCamera>,
    suns: Query<Entity, With<DirectionalLight>>,
    mut fitted: Local<Option<f32>>,
) {
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let distance = camera.distance.max(1.0);
    if fitted.is_some_and(|last| (distance / last - 1.0).abs() < 0.2) {
        return;
    }
    *fitted = Some(distance);
    let (first, farthest) = cascade_bounds(distance);
    for sun in &suns {
        commands.entity(sun).insert(
            bevy::light::CascadeShadowConfigBuilder {
                num_cascades: 4,
                first_cascade_far_bound: first,
                maximum_distance: farthest,
                ..default()
            }
            .build(),
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cascades_follow_the_camera_from_live_to_the_island_overview() {
        let (close_first, close_far) = super::cascade_bounds(14.0);
        assert!(close_first <= 14.0 && close_far >= 45.0);
        let (island_first, island_far) = super::cascade_bounds(150.0);
        assert!(island_far >= 450.0, "the whole overview keeps its shadows");
        assert!(island_first > close_first);
    }

    use super::*;

    #[test]
    fn only_the_island_play_camera_receives_post_processing() {
        let mut app = App::new();
        app.add_systems(Update, install);
        let play = app
            .world_mut()
            .spawn((Camera3d::default(), RtsCamera::default()))
            .id();
        let preview = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        app.update();
        let bloom = app.world().get::<Bloom>(play).expect("bloom installed");
        assert!(bloom.intensity > 0.0 && bloom.intensity <= Bloom::NATURAL.intensity);
        assert!(app.world().get::<Hdr>(play).is_some());
        assert_eq!(
            app.world().get::<Tonemapping>(play),
            Some(&Tonemapping::TonyMcMapface)
        );
        assert!(app.world().get::<Bloom>(preview).is_none());
        assert!(app.world().get::<Hdr>(preview).is_none());
    }

    #[test]
    fn the_comparison_switch_leaves_the_camera_untouched() {
        let mut app = App::new();
        app.insert_resource(PostEnabled(false))
            .add_systems(Update, install);
        let play = app
            .world_mut()
            .spawn((Camera3d::default(), RtsCamera::default()))
            .id();
        app.update();
        app.update();
        assert!(app.world().get::<Bloom>(play).is_none());
        assert!(app.world().get::<Hdr>(play).is_none());
    }
}
