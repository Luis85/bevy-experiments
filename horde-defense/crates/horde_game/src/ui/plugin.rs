//! `UiPlugin`: egui, the camera, the screen draw pass, notice-driven
//! navigation, the window lifecycle and the scene-audio
//! bridge. Expects `FrontendParts::insert_into` to have inserted the resources
//! and states.
use super::{CampaignUi, Screen, Tab, notices};
use crate::{
    atmosphere_audio,
    launch::LaunchConfig,
    music,
    runner::{AppCommand, GameSession, Runtime, RuntimeSet, SaveIntent},
    settings::Settings,
    visual_assets,
};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::{PrimaryWindow, WindowCloseRequested},
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((EguiPlugin::default(), NavigationPlugin))
            .add_systems(Startup, (spawn_camera, visual_assets::load_assets))
            .add_systems(
                EguiPrimaryContextPass,
                (visual_assets::refresh_art, super::draw).chain(),
            )
            .add_systems(
                Update,
                (
                    close_window,
                    screenshot_hotkey,
                    show_window.run_if(window_hidden),
                    publish_scene_audio
                        .run_if(scene_audio_inputs_changed)
                        .before(atmosphere_audio::AmbienceUpdate),
                    publish_music.before(music::MusicUpdate),
                ),
            );
    }
}

/// The window-free part of the UI: runtime notices become state transitions,
/// and `OnEnter(Screen::Battle)` resets a new deployment. Headless tests run it
/// without egui.
pub(crate) struct NavigationPlugin;
impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        // Notices from this frame's Apply pass (and the previous frame's
        // Advance/Persist passes) become NextState before StateTransition.
        app.add_systems(PreUpdate, notices::handle_notices.after(RuntimeSet::Apply))
            .add_systems(OnEnter(Screen::Battle), notices::enter_battle)
            // The Results screen's loot reveal and coin batch (A6-14).
            .add_systems(Update, notices::results_audio);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Native close requests pause and save; the runner exits after the commit.
fn close_window(mut close: MessageReader<WindowCloseRequested>, mut runtime: ResMut<Runtime>) {
    if close.read().next().is_some() {
        runtime.enqueue(horde_core::command::Command::SetPaused(true), "");
        runtime.enqueue(AppCommand::Save(SaveIntent::Quit), "");
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

/// UI-owned ambience inputs. The UI resources are written every frame, so
/// their change ticks are not informative; compare the values instead.
type AudioKey = (Screen, Tab, Option<u64>, String, usize, u32, u32);
fn scene_audio_inputs_changed(
    session: Res<GameSession>,
    screen: Res<State<Screen>>,
    tab: Option<Res<State<Tab>>>,
    campaign: Res<CampaignUi>,
    settings: Res<Settings>,
    mut last: Local<Option<AudioKey>>,
) -> bool {
    let key = (
        *screen.get(),
        tab.map_or(Tab::default(), |tab| *tab.get()),
        campaign.observed,
        campaign.selected_country.clone(),
        campaign.stage,
        settings.master.to_bits(),
        settings.ambience.to_bits(),
    );
    let changed = last.as_ref() != Some(&key);
    *last = Some(key);
    changed || session.is_changed()
}
/// The music director's input, next to the ambience target. It follows
/// the battle tick by tick, so it is published every frame.
fn publish_music(
    session: Res<GameSession>,
    screen: Res<State<Screen>>,
    tab: Option<Res<State<Tab>>>,
    mut target: ResMut<music::MusicTarget>,
) {
    let input = super::music_input(
        &session.game,
        *screen.get(),
        tab.map_or(Tab::default(), |tab| *tab.get()),
    );
    if target.0 != input {
        target.0 = input;
    }
}
fn publish_scene_audio(
    session: Res<GameSession>,
    screen: Res<State<Screen>>,
    tab: Option<Res<State<Tab>>>,
    campaign: Res<CampaignUi>,
    settings: Res<Settings>,
    mut target: ResMut<atmosphere_audio::SceneAudioTarget>,
) {
    *target = super::ambience_target(
        &session.game,
        *screen.get(),
        tab.map_or(Tab::default(), |tab| *tab.get()),
        &campaign,
        &settings,
    );
}
