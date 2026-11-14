//! Host-side frame work of the island: the village host's frame and the idle
//! supervisor (ADR 0023). The older visitor HUD, its diary reader and its
//! supervisor page are gone; the player HUD (`player_hud`) and the ledger
//! (`colony_panel`) show the played character and the villages.
use bevy::prelude::*;
use island_content::village;
use npc_framework::agent_io::scheduler::{self, SchedulerRuntime};

/// The older visitor HUD's root. Nothing spawns it any more; the marker stays
/// only while `village_world`'s camera queries still name it.
#[derive(Component)]
pub(crate) struct Hud;
/// The older diary reader's root. As [`Hud`]: never spawned.
#[derive(Component)]
pub(crate) struct BookPanel;

pub(crate) fn plugin(app: &mut App) {
    // The supervisor is host-side in every scene.
    if !app.world().contains_resource::<SchedulerRuntime>() {
        scheduler::configure_host(app.world_mut(), scheduler::default_bindings_path());
    }
    // The F1 overlay starts hidden on the island; `overlay::plugin` keeps an
    // existing value.
    if !app
        .world()
        .contains_resource::<crate::overlay::OverlayState>()
    {
        app.insert_resource(crate::overlay::OverlayState { visible: false });
    }
    app.add_systems(PostUpdate, background);
}

fn background(world: &mut World) {
    village::host::frame(world);
    scheduler::frame(world);
}
