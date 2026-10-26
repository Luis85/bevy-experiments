//! Tooling for the NPC framework (never changes sim results).
//!
//! Headless (always): [`output`] (JSON on stdout, exit codes, used by
//! `npcctl`), [`commands_file`] (`--commands` files, submitted with
//! deferred validation), [`recorder`] (the replay recorder shared by the CLI
//! and BRP), [`new_app_journaled`], [`bench`] (the
//! ticks/s and per-mechanic benchmark harness, generic over the content's
//! app factory), the profiler glue ([`PerfTickPlugin`],
//! [`HeadlessDriverPlugin`]) and [`snapshot_check`] (the world snapshot
//! invariant: save → load → run equals an uninterrupted run, ADR 0013).
//!
//! [`out_path`]: the one place where a client-supplied output path (an RPC
//! method's `path` param) is checked and confined under the output root.
//!
//! Feature `remote`: [`remote`], the Bevy Remote Protocol `npc/*` methods
//! (formerly the `npc_remote` crate) and the live replay recorder. It pulls
//! the `bevy` umbrella (`bevy_remote` depends on the renderer crates), so
//! headless tools leave it off.

pub mod bench;
pub mod commands_file;
pub mod out_path;
pub mod output;
pub mod recorder;
#[cfg(feature = "remote")]
pub mod remote;
pub mod snapshot_check;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::NpcDriverPlugin;
use npc_kernel::driver::{Pacing, TickObservers};
use npc_perf::PerfHandle;

/// Builds `factory(seed, scenario, villagers)` with the journal recording
/// from the start (`npc_kernel::journal::with_journal_from_build`), so the
/// scenario's tick-0 `Spawn` entries are in it, then turns the whole journal
/// on (`set_journal`, `AiTrace` included). Used by `npcctl journal` (by
/// default) and `npcctl inspect --journal-from-build true`. Plain factories
/// keep the journal off until enabled, so their journals have no build-time
/// entries.
pub fn new_app_journaled(
    factory: npc_kernel::replay::AppFactory,
    seed: u64,
    scenario: &str,
    villagers: u32,
    verbose: bool,
) -> Result<App, String> {
    let mut app = npc_kernel::journal::with_journal_from_build(verbose, || {
        factory(seed, scenario, villagers)
    })?;
    npc_kernel::journal::set_journal(app.world_mut(), true, verbose);
    Ok(app)
}

/// Closes one profiler frame per tick the driver runs (when a
/// [`PerfHandle`] exists), so per-mechanic stats are per tick.
pub struct PerfTickPlugin;

fn end_perf_frame(world: &mut World) {
    if let Some(perf) = world.get_resource::<PerfHandle>() {
        perf.0.end_frame();
    }
}

impl Plugin for PerfTickPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .get_resource_or_init::<TickObservers>()
            .add("npc_tools.perf_frame", end_perf_frame);
    }
}

/// Marker: the sim is driven headless (frame-stepped, profiler frame per
/// tick). Reported by the remote `ControlState.driver` as `"headless"`.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct HeadlessDriver;

/// Headless driver: [`NpcDriverPlugin`] in `Update` with
/// [`Pacing::EveryRun`] (`SimControl.multiplier` ticks per update, no
/// wall-clock pacing) plus [`PerfTickPlugin`]. Use it instead of the
/// facade's driver (`NpcPlugins::without_driver()`).
pub struct HeadlessDriverPlugin;

impl Plugin for HeadlessDriverPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HeadlessDriver).add_plugins((
            NpcDriverPlugin {
                pacing: Pacing::EveryRun,
                ..NpcDriverPlugin::new(npc_kernel::clock::DEFAULT_TICK_HZ, Update)
            },
            PerfTickPlugin,
        ));
    }
}
