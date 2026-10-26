//! `ScreenshotRequest`: capture the primary window after `after_secs` of real
//! time, save it, then exit with `AppExit::Success`.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
};

use crate::ScreenshotRequest;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Last,
        take_screenshot.run_if(resource_exists::<ScreenshotRequest>),
    );
}

fn take_screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    request: Res<ScreenshotRequest>,
    mut requested: Local<bool>,
) {
    if *requested || time.elapsed_secs() < request.after_secs {
        return;
    }
    *requested = true;
    let path = request.path.clone();
    prepare_dir(&path);
    info!("npc_view_helpers: taking screenshot -> {}", path.display());
    let mut save = save_to_disk(path);
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            // `save_to_disk` writes synchronously, so the file exists afterwards.
            save(captured);
            exit.write(AppExit::Success);
        },
    );
}

fn prepare_dir(path: &std::path::Path) {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        error!("npc_view_helpers: cannot create {}: {e}", dir.display());
    }
}

/// Captures the primary window to `path` (PNG by extension) without exiting.
/// The capture completes a frame or two later; used by the remote
/// `npc/screenshot` method.
pub fn save_screenshot(world: &mut World, path: std::path::PathBuf) {
    prepare_dir(&path);
    info!("npc_view_helpers: taking screenshot -> {}", path.display());
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}
