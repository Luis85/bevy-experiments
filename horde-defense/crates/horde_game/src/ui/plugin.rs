//! `UiPlugin`: egui, the camera, the screen draw pass, the native cursor, the
//! window lifecycle and the scene-audio bridge.
use crate::{atmosphere_audio, cursor, launch::LaunchConfig, runner, visual_assets};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{PrimaryWindow, WindowCloseRequested},
};
use bevy_egui::{EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass};

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .add_systems(Startup, (spawn_camera, visual_assets::load_assets))
            .add_systems(EguiPrimaryContextPass, super::draw)
            .add_systems(
                PostUpdate,
                cursor::sync_native_cursor.after(EguiPostUpdateSet::ProcessOutput),
            )
            .add_systems(
                Update,
                (
                    close_window,
                    screenshot_hotkey,
                    show_window.run_if(window_hidden),
                    publish_scene_audio.before(atmosphere_audio::AmbienceUpdate),
                ),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Native close requests pause and save; the runner exits after the commit.
fn close_window(
    mut close: MessageReader<WindowCloseRequested>,
    mut frontend: ResMut<super::Frontend>,
) {
    if close.read().next().is_some() {
        frontend
            .runtime
            .enqueue(horde_core::command::Command::SetPaused(true), "");
        frontend
            .runtime
            .enqueue(runner::AppCommand::Save(runner::SaveIntent::Quit), "");
    }
}

fn window_hidden(window: Single<&Window, With<PrimaryWindow>>) -> bool {
    !window.visible
}
/// The window opens hidden and is shown after a few presented frames, so the
/// first visible frame is not an unstyled egui pass. Stops once shown.
fn show_window(
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    frames: Res<bevy::diagnostic::FrameCount>,
) {
    if frames.0 >= 3 {
        window.visible = true;
    }
}

/// F12 writes `lastlight-<n>.png` to `LASTLIGHT_CAPTURE_DIR` or `<save dir>/screenshots`.
fn screenshot_hotkey(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<LaunchConfig>,
    mut index: Local<u32>,
) {
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    let folder = config
        .capture_dir
        .clone()
        .unwrap_or_else(|| config.save_directory.join("screenshots"));
    if let Err(error) = std::fs::create_dir_all(&folder) {
        error!("Could not create screenshot folder: {error}");
        return;
    }
    let path = folder.join(format!("lastlight-{}.png", *index));
    *index += 1;
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

fn publish_scene_audio(
    frontend: Res<super::Frontend>,
    mut target: ResMut<atmosphere_audio::SceneAudioTarget>,
) {
    *target = frontend.ambience_target();
}
