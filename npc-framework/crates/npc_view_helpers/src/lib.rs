//! Generic presentation helpers for `npc_framework` hosts. They read the sim,
//! never write it, and never insert components on sim entities.
//!
//! - [`VisualOf`] / [`VisualIndex`]: the link from a visual entity to the sim
//!   entity it shows (content spawns the visuals); [`ViewHidden`]: sim
//!   entities the view hides (a fog of war), which the gizmos and the
//!   selection here skip.
//! - [`interp`]: the interpolation factor `PrevSimPos → SimPos` from the
//!   kernel driver's own accumulator (never `Time<Fixed>`).
//! - [`SimTickTiming`]: wall time per sim tick, measured around the driver.
//! - Journal panel ([`journal_panel`]), perf panel (feature `perf`),
//!   path gizmos (feature `nav`, `nav_gizmos`), vision-cone gizmos (feature
//!   `perception`, `perception_gizmos`), crew gizmos (feature `vehicles`,
//!   `vehicle_gizmos`), selection ([`Selected`],
//!   [`Selectable`]) and screenshots; with feature `command` the opt-in RTS
//!   command UI (`command_ui::CommandUiPlugin`: multi-selection, control
//!   groups, context orders, order gizmos); the windowed host's remote
//!   flags ([`remote::RemoteArgs`]) and, with feature `remote`, its BRP setup
//!   (`npc/screenshot` with confined paths, `--record`).
//!
//! Key bindings are not part of this crate: an app toggles [`DebugPanels`]
//! and drives `SimControl` from its own input map.
//!
//! Robustness rule: every system tolerates missing sim resources, missing sim
//! components and assets that are not (yet) loaded. It skips that part for
//! the frame instead of panicking.

// Bevy queries are routinely "complex types"; aliases would only obscure them.
#![allow(clippy::type_complexity)]

#[cfg(feature = "command")]
pub mod command_ui;
pub mod interp;
pub mod journal_panel;
#[cfg(feature = "nav")]
pub mod nav_gizmos;
#[cfg(feature = "perception")]
pub mod perception_gizmos;
#[cfg(feature = "perf")]
mod perf_panel;
#[cfg(feature = "perf")]
pub mod perf_setup;
pub mod remote;
mod screenshot;
mod select;
mod timing;
#[cfg(feature = "vehicles")]
pub mod vehicle_gizmos;
mod visuals;

use bevy::prelude::*;

pub use journal_panel::{EffectDescriber, JOURNAL_ROWS, describe, format_entry};
pub use screenshot::save_screenshot;
pub use select::{PICK_HEIGHT, PICK_RADIUS, Selectable, Selected, pick};
pub use timing::SimTickTiming;
pub use visuals::{ViewHidden, VisualIndex, VisualOf, hidden};

/// Ordering of the view's `Update` systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewSystems {
    /// Keyboard/mouse handling (camera, overlay toggle).
    Input,
    /// Spawn/despawn visual entities for sim entities.
    Sync,
    /// Write visual transforms and animation state from sim components.
    Animate,
    /// Lighting, overlay text and gizmos.
    Present,
}

/// Which debug panels are shown (`journal`: F3 in the village app, `perf`:
/// F4, only with the `perf` feature). Insert it before the plugin to choose
/// the initial state.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugPanels {
    pub journal: bool,
    pub perf: bool,
}

/// Optional: save a screenshot to `path` after `after_secs` of real time, then exit.
#[derive(Resource, Clone, Debug)]
pub struct ScreenshotRequest {
    pub path: std::path::PathBuf,
    pub after_secs: f32,
}

/// The generic view: `ViewSystems` chained in `Update`, `VisualIndex`,
/// selection, the journal panel, the perf panel (feature `perf`), tick
/// timing and `ScreenshotRequest` handling.
pub struct NpcViewHelpersPlugin;

impl Plugin for NpcViewHelpersPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                ViewSystems::Input,
                ViewSystems::Sync,
                ViewSystems::Animate,
                ViewSystems::Present,
            )
                .chain(),
        )
        .add_plugins((
            visuals::plugin,
            select::plugin,
            journal_panel::plugin,
            screenshot::plugin,
            timing::plugin,
        ))
        .init_resource::<DebugPanels>();
        #[cfg(feature = "perf")]
        app.add_plugins(perf_panel::plugin);
        #[cfg(feature = "nav")]
        app.add_plugins(nav_gizmos::plugin);
        #[cfg(feature = "perception")]
        app.add_plugins(perception_gizmos::plugin);
        #[cfg(feature = "vehicles")]
        app.add_plugins(vehicle_gizmos::plugin);
    }
}

/// Converts a sim position (mm) to a world position on the ground plane (m).
pub fn sim_to_world(pos: npc_kernel::SimPos) -> Vec3 {
    Vec3::new(pos.x as f32 / 1000.0, 0.0, pos.z as f32 / 1000.0)
}
