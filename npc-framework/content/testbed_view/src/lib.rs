//! Presentation of the Actor Testbed (`testbed_content`, actor-capabilities
//! spec §4.9): Dungeon wall runs and swinging doors, trees and stumps, the
//! ore rock, workstations, items on the ground or in hand, the cart and the
//! siege-ram wagon (wheels spin as they roll), animated actors (Knight and
//! Rogue for faction 1, skeletons for faction 2) with faction rings and a
//! riding pose, an RTS camera and the RTS command UI, plus the F1 overlay,
//! the F5 character sheet, the F6 perception panel and (explorer mode) the F7
//! notes editor (diary, memory, proposals). The F3 journal and
//! F4 perf panels come from `npc_view_helpers`. The explorer's prompt modal
//! ([`ExplorerModalPlugin`]) is added by the app in explorer mode only.
//!
//! Reads sim components, never writes them, and never inserts components on
//! sim entities; orders go out as recorded `IssueOrder` commands. Scales and
//! the door adaptation are documented in [`visuals`](crate#scale).
//!
//! # Scale
//!
//! Every model is brought to KayKit 1:1 and scaled by [`V`] = 0.75 (a
//! Knight is 1.74 m, a Dungeon wall piece 3 m × 3 m). The sim's doors are
//! 1 m cells but the Dungeon doorway is 4 units (3 m) wide: the view
//! squeezes the doorway piece to [`DOOR_SX`] = 0.5 along the wall, so its
//! opening and leaf match the 1 m door cell (its solid ends overlap the
//! neighbouring wall cells), keeps its height (a 2.06 m opening), and hangs
//! the leaf on its own hinge pivot so it swings without shearing.
//!
//! View keys live here (F1/F3/F4/F5/F6, camera, formations, Hold/Stop/
//! Unload); the sim speed keys belong to the app (`apps/actor_testbed`).
//!
//! Robustness rule: every system tolerates missing sim resources, missing
//! sim components and assets that are not (yet) loaded.

// Bevy queries are routinely "complex types"; aliases would only obscure them.
#![allow(clippy::type_complexity)]

mod anim;
mod assets;
mod camera;
mod colony_panel;
mod colony_scene;
mod command;
mod explorer_modal;
mod explorer_panel;
mod field_clipboard;
mod fog;
pub mod game_ui;
mod homestead_panel;
mod island_alerts;
mod island_atmosphere;
mod island_audio;
mod island_build;
mod island_camera;
mod island_control;
mod island_homes;
mod island_poses;
mod island_shell;
mod island_world;
mod landmarks;
mod model_preview;
mod overlay;
mod panel_scroll;
mod panels;
mod player_actions;
mod player_hud;
mod resident_inspector;
mod robots;
mod scene;
mod scene_readiness;
mod shape_tools;
mod village;
mod village_world;
mod visuals;

use bevy::prelude::*;
use npc_framework::view_helpers::{DebugPanels, NpcViewHelpersPlugin, ViewSystems};

pub use anim::{Pose, pose_of};
pub use assets::{ASSET_DIR, asset_plugin};
pub use camera::{CameraFocusOnSelected, CameraStart, IslandCameraSettings, RtsCamera};
pub use command::{FORMATION_SPACING_MM, context_tool_order};
pub use explorer_modal::{
    ExplorerModalPlugin, ExplorerModalSystems, PromptModal, PromptModalClosed, modal_open,
};
pub use explorer_panel::{
    ExplorerFiles, NoteField, NotesControl, NotesInput, STALE_NOTE, editor_typing,
};
pub use fog::{
    CellFog, FogLens, FogOfWar, Look, MASK_EXPLORED, MASK_GREY, MASK_UNEXPLORED, MASK_VISIBLE,
    MaskRegion, area_look, marker_pos, mask_alpha, mask_hash, mask_texel, mask_texels, static_look,
    unit_look,
};
pub use island_audio::IslandAudioSettings;
pub use island_control::IslandMode;
pub use island_world::ambient::AmbientMotion;
pub use player_hud::{
    DeveloperKeys, FrontendGate, FrontendUiRoot, PlayerExperience, PlayerMenuRequest,
};
pub use scene_readiness::SceneReadiness;

/// One control authority for the host shell, player toolbar and advanced modes.
pub fn request_island_mode(world: &mut World, mode: IslandMode) -> Result<(), String> {
    island_control::request_mode(world, mode)
}
pub use island_shell::ToastTone;
pub use island_shell::coach::IslandTips;
/// The host's door for Shape's "Export world": it writes the file.
pub use shape_tools::resources::{WorldExportRequest, take_world_export};
/// The player's Reduce motion setting. Host data, like the other settings.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReduceMotion(pub bool);
/// Show a toast under the island's top bar, in every mode: a save, a failed
/// save, a loaded island. `lasting` stays until it is replaced or cleared;
/// `retry` adds a "Try again" button, reported by [`take_island_toast_retry`].
pub fn island_toast(
    world: &mut World,
    tone: ToastTone,
    text: impl Into<String>,
    lasting: bool,
    retry: bool,
) {
    island_shell::host_toast(world, tone, text, lasting, retry);
}
/// A host toast with its own button (such as "Start a new game on it"); the
/// press is reported by [`take_island_toast_retry`], and the host knows what
/// its button meant.
pub fn island_toast_with(
    world: &mut World,
    tone: ToastTone,
    text: impl Into<String>,
    lasting: bool,
    button: impl Into<String>,
) {
    island_shell::host_toast_with(world, tone, text, lasting, button);
}
/// Take a lasting host toast away.
pub fn clear_island_toast(world: &mut World) {
    island_shell::clear_host_toast(world);
}
/// Whether the player chose "Try again" on a host toast since the last call.
pub fn take_island_toast_retry(world: &mut World) -> bool {
    island_shell::take_toast_retry(world)
}
/// Whether the played character sleeps (the host may run the clock faster).
pub fn island_player_asleep(world: &World) -> bool {
    player_hud::player_asleep(world)
}
/// Explicit spectator choice, including acknowledgement of a completed life.
pub fn watch_island_village(world: &mut World) -> Result<(), String> {
    player_hud::watch_village(world)
}
pub fn native_control_active(world: &World) -> bool {
    player_actions::control(world) == player_actions::ControlState::Native
}
/// Preserve committed work while recording a stop for transient navigation.
/// A host must wait for the receipt before saving; this does not pause or tick.
pub fn prepare_player_suspend(world: &mut World) -> Result<Option<(u64, u32)>, String> {
    player_actions::prepare_suspend(world)
}
/// Discard transient view intent before the host replaces a world snapshot.
/// This does not change authoritative actor state or advance the simulation.
pub fn clear_player_view_intent(world: &mut World) {
    player_hud::clear(world);
    player_actions::clear_view(world);
    island_control::clear_view(world);
}
pub use model_preview::{
    ModelPreview, ModelPreviewPlugin, ModelPreviewSettings, ModelPreviewSystems,
    ModelPreviewViewport, PreviewActor,
};
pub use npc_framework::view_helpers::command_ui::{CommandFormation, CommandSelection, CommandUi};
pub use npc_framework::view_helpers::{
    ScreenshotRequest, Selected, VisualIndex, VisualOf, save_screenshot, sim_to_world,
};
pub use overlay::OverlayState;
pub use panels::TestbedPanels;
pub use visuals::{DOOR_SX, V};

/// Install the same OS copy/cut/paste bridge used by the game's F7 fields
/// in a standalone editor. Calling this more than once is harmless.
pub fn install_text_clipboard(app: &mut App) {
    if !app
        .world()
        .contains_resource::<field_clipboard::ClipboardTools>()
    {
        field_clipboard::plugin(app);
    }
}

/// F3 toggles the journal panel, F4 the perf panel.
fn toggle_debug_panels(keys: Option<Res<ButtonInput<KeyCode>>>, mut panels: ResMut<DebugPanels>) {
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::F3) {
        panels.journal = !panels.journal;
    }
    if keys.just_pressed(KeyCode::F4) {
        panels.perf = !panels.perf;
    }
}

/// Everything visual for the testbed (see the crate docs). Adds
/// `NpcViewHelpersPlugin` and `CommandUiPlugin` when they are missing.
pub struct TestbedViewPlugin;

impl Plugin for TestbedViewPlugin {
    fn build(&self, app: &mut App) {
        game_ui::install(app);
        app.init_resource::<FrontendGate>()
            .configure_sets(Update, ViewSystems::Input.run_if(player_hud::gate_open))
            .configure_sets(
                Update,
                ViewSystems::OverlayInput.run_if(player_hud::gate_open),
            );
        if !app.is_plugin_added::<NpcViewHelpersPlugin>() {
            app.add_plugins(NpcViewHelpersPlugin);
        }
        app.add_plugins((
            assets::plugin,
            village_world::plugin,
            village::plugin,
            homestead_panel::plugin,
            robots::plugin,
            scene::plugin,
            camera::plugin,
            visuals::plugin,
            anim::plugin,
            overlay::plugin,
            panels::plugin,
            explorer_panel::plugin,
            landmarks::plugin,
            command::plugin,
            fog::plugin,
        ))
        .add_plugins((
            island_control::plugin,
            island_camera::plugin,
            colony_panel::plugin,
            colony_scene::plugin,
            player_hud::plugin,
            scene_readiness::plugin,
            island_shell::plugin,
            island_alerts::plugin,
            resident_inspector::plugin,
            shape_tools::plugin,
            island_build::plugin,
            island_homes::plugin,
            island_world::plugin,
        ))
        .add_systems(
            Update,
            toggle_debug_panels
                .run_if(player_hud::developer_keys)
                .in_set(ViewSystems::Input),
        );
    }
}

/// Mechanic tags for the testbed view's systems (`npc_perf`).
#[cfg(feature = "perf")]
pub fn perf_tags(tags: &mut npc_framework::perf::MechanicTags) {
    for module in [
        "anim",
        "assets",
        "camera",
        "command",
        "explorer_modal",
        "explorer_panel",
        "field_clipboard",
        "fog",
        "overlay",
        "panels",
        "scene",
        "visuals",
        "village",
        "village_world",
        "player_hud",
        "player_actions",
        "island_control",
        "island_camera",
        "island_atmosphere",
        "island_audio",
        "island_build",
        "island_homes",
        "island_world",
        "island_poses",
        "island_shell",
        "island_alerts",
        "resident_inspector",
        "shape_tools",
        "colony_panel",
        "colony_scene",
        "homestead_panel",
        "game_ui",
    ] {
        tags.add(format!("testbed_view::{module}"), format!("view.{module}"));
    }
    tags.add("testbed_view", "view");
}

#[cfg(test)]
mod developer_key_tests {
    use super::*;

    fn press(app: &mut App, keys: &[KeyCode]) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        for key in keys {
            input.press(*key);
        }
        app.update();
    }

    fn toggles(app: &App) -> [bool; 6] {
        let debug = app.world().resource::<DebugPanels>();
        let panels = app.world().resource::<TestbedPanels>();
        [
            app.world().resource::<OverlayState>().visible,
            debug.journal,
            debug.perf,
            panels.sheet,
            panels.perception,
            app.world().resource::<FogOfWar>().enabled,
        ]
    }

    /// The same registrations as the plugins, without a renderer.
    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DebugPanels>()
            .init_resource::<TestbedPanels>()
            .init_resource::<FogOfWar>()
            .insert_resource(OverlayState { visible: false })
            .add_systems(
                Update,
                (
                    toggle_debug_panels.run_if(player_hud::developer_keys),
                    overlay::toggle_overlay,
                    panels::toggle_panels.run_if(player_hud::developer_keys),
                    fog::toggle_fog.run_if(player_hud::developer_keys),
                ),
            );
        app
    }

    const KEYS: [KeyCode; 6] = [
        KeyCode::F1,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F8,
    ];

    #[test]
    fn developer_toggles_are_inert_in_the_player_session_without_the_flag() {
        let mut app = app();
        let before = toggles(&app);
        app.insert_resource(PlayerExperience { enabled: true });
        press(&mut app, &KEYS);
        assert_eq!(toggles(&app), before, "player session ignores F1, F3-F8");

        app.insert_resource(DeveloperKeys(true));
        press(&mut app, &KEYS);
        assert_eq!(toggles(&app), before.map(|on| !on), "--dev-keys opts in");
    }

    #[test]
    fn developer_launches_keep_their_toggles() {
        for experience in [None, Some(PlayerExperience { enabled: false })] {
            let mut app = app();
            if let Some(experience) = experience {
                app.insert_resource(experience);
            }
            let before = toggles(&app);
            press(&mut app, &KEYS);
            assert_eq!(toggles(&app), before.map(|on| !on));
        }
    }
}
