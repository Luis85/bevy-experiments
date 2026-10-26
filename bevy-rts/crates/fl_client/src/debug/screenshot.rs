//! `fl.screenshot`: a dedicated offscreen camera renders the game camera's view into an
//! `Image`, which `Screenshot::image` captures and saves as a PNG (spec §15 C, B10). A window
//! capture comes back black when the window is hidden behind another app; this does not.
use std::path::{Path, PathBuf};

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::camera::RtsCamera;

/// The size of an offscreen screenshot, in pixels.
pub const SHOT_SIZE: UVec2 = UVec2::new(1280, 720);
/// Frames the offscreen camera renders before the capture.
const WARMUP_FRAMES: u32 = 3;
/// Captures of a single-colour image (pipelines still compiling) before it is saved anyway.
const MAX_ATTEMPTS: u32 = 30;

/// Marks an app that renders: only then can [`request_screenshot`] work.
#[derive(Resource)]
pub struct ScreenshotSupport;

/// An offscreen camera waiting to capture its image into `path`; despawned once saved.
#[derive(Component, Debug)]
pub struct OffscreenShot {
    path: PathBuf,
    image: Handle<Image>,
    frames: u32,
    attempts: u32,
    capturing: bool,
}

/// Adds the offscreen screenshot machinery to an app that renders.
pub(crate) fn build(app: &mut App) {
    if app.get_sub_app(RenderApp).is_none() {
        return;
    }
    app.insert_resource(ScreenshotSupport)
        .add_systems(Update, run_offscreen_shots);
}

/// Spawns an offscreen camera with the game camera's pose and projection, rendering into a new
/// [`SHOT_SIZE`] image; the PNG is written to `path` a few frames later. Returns the size.
///
/// # Errors
///
/// If the app does not render, there is no game camera, or `path`'s directory cannot be
/// created.
pub fn request_screenshot(world: &mut World, path: &Path) -> Result<UVec2, String> {
    if !world.contains_resource::<ScreenshotSupport>() {
        return Err("this app does not render; no screenshot".to_owned());
    }
    let (transform, projection) = world
        .query_filtered::<(&Transform, &Projection), With<RtsCamera>>()
        .iter(world)
        .next()
        .map(|(t, p)| (*t, p.clone()))
        .ok_or("there is no game camera")?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("cannot create {}: {err}", dir.display()))?;
    }
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            SHOT_SIZE.x,
            SHOT_SIZE.y,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    world.spawn((
        Name::new("offscreen screenshot camera"),
        Camera3d::default(),
        Camera {
            order: -1,
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        projection,
        transform,
        OffscreenShot {
            path: path.to_owned(),
            image,
            frames: 0,
            attempts: 0,
            capturing: false,
        },
    ));
    Ok(SHOT_SIZE)
}

/// Captures each offscreen camera's image once it has rendered [`WARMUP_FRAMES`] frames.
fn run_offscreen_shots(mut commands: Commands, mut shots: Query<(Entity, &mut OffscreenShot)>) {
    for (camera, mut shot) in &mut shots {
        if shot.capturing {
            continue;
        }
        shot.frames += 1;
        if shot.frames < WARMUP_FRAMES {
            continue;
        }
        shot.capturing = true;
        commands
            .spawn(Screenshot::image(shot.image.clone()))
            .observe(on_captured(camera));
    }
}

/// Saves the captured image and despawns the camera; captures again (up to [`MAX_ATTEMPTS`])
/// while the image is a single colour, as it is before the camera's pipelines are ready.
fn on_captured(
    camera: Entity,
) -> impl FnMut(On<ScreenshotCaptured>, Commands, Query<&mut OffscreenShot>) {
    move |captured, mut commands, mut shots| {
        let Ok(mut shot) = shots.get_mut(camera) else {
            return;
        };
        if is_single_colour(&captured.image) && shot.attempts + 1 < MAX_ATTEMPTS {
            shot.attempts += 1;
            shot.frames = 0;
            shot.capturing = false;
            return;
        }
        let mut save = save_to_disk(shot.path.clone());
        commands.entity(camera).despawn();
        save(captured);
    }
}

/// Whether every 4-byte pixel of `image` equals the first.
fn is_single_colour(image: &Image) -> bool {
    let Some(data) = image.data.as_deref() else {
        return true;
    };
    let (pixels, _) = data.as_chunks::<4>();
    let Some((first, rest)) = pixels.split_first() else {
        return true;
    };
    rest.iter().all(|pixel| pixel == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_colour_images_are_detected() {
        let mut image = Image::new_target_texture(2, 1, TextureFormat::Rgba8UnormSrgb, None);
        image.data = Some(vec![1, 2, 3, 255, 1, 2, 3, 255]);
        assert!(is_single_colour(&image));
        image.data = Some(vec![1, 2, 3, 255, 9, 2, 3, 255]);
        assert!(!is_single_colour(&image));
    }

    #[test]
    fn no_screenshot_without_a_renderer() {
        let mut world = World::new();
        let err = request_screenshot(&mut world, Path::new("x.png")).expect_err("no renderer");
        assert!(err.contains("does not render"), "{err}");
    }
}
