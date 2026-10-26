//! Presentation of the example village: models, animation, RTS camera,
//! ground and day/night lighting, the F1 debug overlay and the village's
//! journal-panel effect descriptions. Built on the generic
//! `npc_view_helpers` (via the facade's `view-helpers` feature). Reads sim
//! components, never writes them, and never inserts components on sim
//! entities.
//!
//! View key bindings live here (F1 overlay, F3/F4 panels, camera); the sim
//! speed keys (Space, 1-4, `.`) belong to the app (`apps/village`).
//!
//! Robustness rule: every system tolerates missing sim resources, missing sim
//! components and assets that are not (yet) loaded. It skips that part for
//! the frame instead of panicking.

// Bevy queries are routinely "complex types"; aliases would only obscure them.
#![allow(clippy::type_complexity)]

mod anim;
mod assets;
mod camera;
mod overlay;
mod scene;
mod visuals;

use bevy::prelude::*;
use npc_framework::kernel::JsonObject;
use npc_framework::view_helpers::{
    DebugPanels, EffectDescriber, NpcViewHelpersPlugin, ViewSystems,
};
use village_content::effects::EffectKind;

pub use assets::{ASSET_DIR, asset_plugin};
pub use camera::RtsCamera;
pub use npc_framework::view_helpers::{
    ScreenshotRequest, Selected, VisualIndex, VisualOf, save_screenshot, sim_to_world,
};
pub use overlay::OverlayState;
pub use village_content::villager::{BB_DEST, BB_TARGET};

/// F3 toggles the journal panel, F4 the perf panel.
fn toggle_panels(keys: Option<Res<ButtonInput<KeyCode>>>, mut panels: ResMut<DebugPanels>) {
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::F3) {
        panels.journal = !panels.journal;
    }
    if keys.just_pressed(KeyCode::F4) {
        panels.perf = !panels.perf;
    }
}

/// Journal-panel text for the village's effects.
fn describe_effect(effect: &JsonObject) -> Option<String> {
    Some(match effect.to_typed::<EffectKind>().ok()? {
        EffectKind::AddFood { stockpile, amount } => format!("+{amount} food -> #{}", stockpile.0),
        EffectKind::TakeFood { stockpile, amount } => format!("-{amount} food <- #{}", stockpile.0),
    })
}

/// Everything visual for the village: the generic helpers
/// (`NpcViewHelpersPlugin`), visuals for places and villagers (`VisualOf`),
/// animation, RTS camera, ground + dressing, day/night light, F1 overlay,
/// F3/F4 panel keys and the effect descriptions.
pub struct VillageViewPlugin;

impl Plugin for VillageViewPlugin {
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
        ))
        .insert_resource(EffectDescriber(describe_effect))
        .add_systems(Update, toggle_panels.in_set(ViewSystems::Input));
    }
}

/// Mechanic tags for the village view's systems (`npc_perf`), keeping the
/// pre-split `view.<module>` mechanic names (budgets in
/// `PerfBudgets::default_npc`).
#[cfg(feature = "perf")]
pub fn perf_tags(tags: &mut npc_framework::perf::MechanicTags) {
    for module in ["anim", "assets", "camera", "overlay", "scene", "visuals"] {
        tags.add(format!("village_view::{module}"), format!("view.{module}"));
    }
    tags.add("village_view", "view");
}
