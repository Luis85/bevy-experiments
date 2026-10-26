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
mod command;
mod explorer_modal;
mod explorer_panel;
mod field_clipboard;
mod fog;
mod overlay;
mod panels;
mod scene;
mod visuals;

use bevy::prelude::*;
use npc_framework::view_helpers::{DebugPanels, NpcViewHelpersPlugin, ViewSystems};

pub use anim::{Pose, pose_of};
pub use assets::{ASSET_DIR, asset_plugin};
pub use camera::{CameraFocusOnSelected, CameraStart, RtsCamera};
pub use command::{FORMATION_SPACING_MM, context_tool_order};
pub use explorer_modal::{
    ExplorerModalPlugin, ExplorerModalSystems, PromptModal, PromptModalClosed, modal_open,
};
pub use explorer_panel::{ExplorerFiles, NoteField, STALE_NOTE, editor_typing};
pub use fog::{
    CellFog, FogLens, FogOfWar, Look, MASK_EXPLORED, MASK_GREY, MASK_UNEXPLORED, MASK_VISIBLE,
    MaskRegion, area_look, marker_pos, mask_alpha, mask_hash, mask_texel, mask_texels, static_look,
    unit_look,
};
pub use npc_framework::view_helpers::command_ui::{CommandFormation, CommandSelection, CommandUi};
pub use npc_framework::view_helpers::{
    ScreenshotRequest, Selected, VisualIndex, VisualOf, save_screenshot, sim_to_world,
};
pub use overlay::OverlayState;
pub use panels::TestbedPanels;
pub use visuals::{DOOR_SX, V};

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
        if !app.is_plugin_added::<NpcViewHelpersPlugin>() {
            app.add_plugins(NpcViewHelpersPlugin);
        }
        app.add_plugins((
            assets::plugin,
            scene::plugin,
            camera::plugin,
            visuals::plugin,
            anim::plugin,
            overlay::plugin,
            panels::plugin,
            explorer_panel::plugin,
            command::plugin,
            fog::plugin,
        ))
        .add_systems(Update, toggle_debug_panels.in_set(ViewSystems::Input));
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
    ] {
        tags.add(format!("testbed_view::{module}"), format!("view.{module}"));
    }
    tags.add("testbed_view", "view");
}
