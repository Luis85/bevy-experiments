//! The [`NpcSimTick`] schedule and the plugin that installs the kernel.

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::{LogLevel, ScheduleBuildSettings, ScheduleLabel, SingleThreadedExecutor};
use npc_ai::{ActiveLeaf, AiRegistry, AiTrace, Blackboard, Brain, BrainState};

use crate::clock::{
    SimClock, SimConfig, advance_clock_system, day_night_system, rethink_timer_system,
};
use crate::commands::{CommandLog, CommandQueue, CommandRegistry, apply_commands_system};
use crate::driver::{DeterminismMode, apply_host_positions_system};
use crate::hash::{Fnv64, SimHash, SimHashRegistry};
use crate::ids::{SimId, SimIdAllocator, SimIdHooks, SimIndex};
use crate::inspect::KindNames;
use crate::journal::{Journal, drain_ai_trace, flush_journal_system};
use crate::math::{SimPos, copy_prev_pos_system};
use crate::orders::{CurrentOrder, OrderKind};
use crate::registry::{FreezeHooks, KernelAppExt, ensure_frozen};
use crate::rng::WorldSeed;
use crate::snapshot::{self, SnapshotRegistry};

/// One simulation step. Run it with [`run_ticks`] (tests, CLI) or let the
/// driver ([`NpcDriverPlugin`](crate::NpcDriverPlugin)) run it from the
/// host's schedule.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NpcSimTick;

/// Phases of a tick, chained in this order. Content adds its systems to
/// these sets; they are the ordering seams.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NpcSet {
    /// Apply due `SimCommand`s (`apply_commands_system`), host positions
    /// (Relaxed mode only), then copy SimPos → PrevSimPos.
    Inputs,
    /// Need decay and level changes → ThinkRequest; day/night transitions →
    /// ThinkRequest on all brains; the staggered re-think timer.
    Needs,
    /// Refresh blackboard facts (reserved for later; may be empty).
    Sense,
    /// Exclusive: `npc_ai::think` over all `Brain` agents sorted by SimId.
    Think,
    /// One system per action component. Actions that move write a
    /// `MoveIntent` (`npc_locomotion`) instead of moving `SimPos` themselves.
    Actions,
    /// Movers (`npc_locomotion::NpcMoverSet`: the straight-line mover, the
    /// grid path mover, host adapters) turn `MoveIntent`s into `SimPos`
    /// changes and `MoveStatus`; systems that react to `MoveStatus` in the
    /// same tick (an action noticing its arrival) run after the movers, in
    /// this set. Empty without a locomotion plugin.
    Locomotion,
    /// Apply cross-entity effects sorted.
    Effects,
    /// Flush the journal sink, then advance `SimClock`.
    Cleanup,
}

/// Installs the kernel: resources (`SimClock`, `SimConfig`, `WorldSeed(seed)`,
/// `SimIdAllocator`, `SimIndex`, `AiRegistry`, the registries, `CommandQueue`,
/// `CommandLog`, and the tooling resources `Journal` and `AiTrace`, both
/// disabled), the [`NpcSimTick`] schedule with the chained [`NpcSet`]s
/// (ambiguity detection `Error` in [`DeterminismMode::Strict`], `Warn` in
/// `Relaxed`) and the core systems; registers the kernel's sim components
/// (`pos`, `brain_state`, `active_leaf`, `blackboard`, `order`).
///
/// `Plugin::finish` freezes every registry ([`ensure_frozen`]). Works with
/// `MinimalPlugins` or a bare `App` (no rendering, no `Time` needed). Does
/// not drive the schedule: add [`NpcDriverPlugin`](crate::NpcDriverPlugin)
/// or call [`run_ticks`].
///
/// Resources are added with `init_resource` (except `WorldSeed`), so values
/// inserted before the plugin (e.g. a custom `SimConfig`) are kept.
pub struct NpcKernelPlugin {
    pub seed: u64,
    pub determinism: DeterminismMode,
}

impl Default for NpcKernelPlugin {
    fn default() -> Self {
        Self {
            seed: 0,
            determinism: DeterminismMode::Strict,
        }
    }
}

impl Plugin for NpcKernelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SimClock>()
            .init_resource::<SimConfig>()
            .insert_resource(WorldSeed(self.seed))
            .insert_resource(self.determinism)
            .init_resource::<SimIdAllocator>()
            .init_resource::<SimIndex>()
            .init_resource::<SimIdHooks>()
            .init_resource::<AiRegistry>()
            .init_resource::<SimHashRegistry>()
            .init_resource::<CommandRegistry>()
            .init_resource::<FreezeHooks>()
            .init_resource::<KindNames>()
            .init_resource::<Journal>()
            .init_resource::<AiTrace>()
            .init_resource::<CommandQueue>()
            .init_resource::<CommandLog>()
            .init_resource::<SnapshotRegistry>();
        snapshot::register_kernel_entries(app);

        app.register_type::<SimId>()
            .register_type::<SimClock>()
            .register_sim_component::<SimPos>()
            .register_sim_component::<CurrentOrder>();
        // npc_ai's components are not `Reflect`: hashed and inspected, not
        // reflection-registered.
        {
            let mut hashed = app.world_mut().resource_mut::<SimHashRegistry>();
            hashed.add_component::<BrainState>();
            hashed.add_component::<ActiveLeaf>();
            hashed.add_component::<Blackboard>();
        }

        let ambiguity = match self.determinism {
            DeterminismMode::Strict => LogLevel::Error,
            DeterminismMode::Relaxed => LogLevel::Warn,
        };
        app.init_schedule(NpcSimTick)
            .edit_schedule(NpcSimTick, |schedule| {
                schedule.set_build_settings(ScheduleBuildSettings {
                    ambiguity_detection: ambiguity,
                    ..Default::default()
                });
                // The tick is a chain of small, mostly exclusive systems:
                // with `bevy_ecs/multi_threaded` (on in every windowed
                // game) the default executor hands each one between
                // threads and costs ~0.45 ms per tick more than the work
                // itself (`npcctl bench`, explorer and village_n 1..500,
                // docs/_notes/explorer-perf-budgets.md). The order is the
                // same either way (ambiguities are errors), so the state
                // hash is too. A host can still `set_executor` its own.
                schedule.set_executor(SingleThreadedExecutor::new());
            });

        app.configure_sets(
            NpcSimTick,
            (
                NpcSet::Inputs,
                NpcSet::Needs,
                NpcSet::Sense,
                NpcSet::Think,
                NpcSet::Actions,
                NpcSet::Locomotion,
                NpcSet::Effects,
                NpcSet::Cleanup,
            )
                .chain(),
        )
        .add_systems(
            NpcSimTick,
            (
                (
                    apply_commands_system,
                    apply_host_positions_system,
                    copy_prev_pos_system,
                )
                    .chain()
                    .in_set(NpcSet::Inputs),
                (day_night_system, rethink_timer_system)
                    .chain()
                    .in_set(NpcSet::Needs),
                think_system.in_set(NpcSet::Think),
                (flush_journal_system, advance_clock_system)
                    .chain()
                    .in_set(NpcSet::Cleanup),
            ),
        );
    }

    fn finish(&self, app: &mut App) {
        ensure_frozen(app.world_mut());
    }
}

// The kernel registers these npc_ai/kernel types; the impls live here
// because `SimHash` is the kernel's trait.
impl SimHash for CurrentOrder {
    const NAME: &'static str = "order";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.0.as_ref(), |h, kind| match kind {
            OrderKind::GoTo { pos } => {
                h.u8(0);
                pos.sim_hash(h);
            }
        });
    }
}

/// The thinking agents ([`think_system`]'s cached query).
pub type BrainQuery = QueryState<(&'static SimId, Entity), With<Brain>>;

/// `NpcSet::Think` (exclusive): runs `npc_ai::think` once over every agent with
/// `Brain` and a `SimId`, in ascending `SimId` order. Agents without `Brain` are
/// untouched (their `ThinkRequest`s stay). With no `Brain` agents it returns
/// without calling `think` at all. Afterwards drains the `AiTrace` into the
/// `Journal` (a no-op unless the journal is on).
pub fn think_system(world: &mut World, brains: &mut BrainQuery) {
    let mut agents: Vec<(SimId, Entity)> = brains.iter(world).map(|(&id, e)| (id, e)).collect();
    if agents.is_empty() {
        return;
    }
    // SimIds are unique, so an unstable sort is still a total, deterministic order.
    agents.sort_unstable_by_key(|&(id, _)| id);
    let entities: Vec<Entity> = agents.into_iter().map(|(_, e)| e).collect();
    npc_ai::think(world, &entities);
    drain_ai_trace(world);
}

/// Runs [`NpcSimTick`] `n` times on `world` (freezing the registries first
/// if the app was never finished).
pub fn run_ticks(world: &mut World, n: u64) {
    ensure_frozen(world);
    for _ in 0..n {
        world.run_schedule(NpcSimTick);
    }
}
