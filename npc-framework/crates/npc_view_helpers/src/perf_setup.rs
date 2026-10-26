//! The profiler in a windowed Bevy app in two calls (feature `perf`; the
//! facade re-exports this as `npc_framework::perf::{log_plugin,
//! perf_layer, NpcPerfPlugin}`):
//!
//! ```ignore
//! use npc_framework::perf::{NpcPerfPlugin, log_plugin};
//!
//! app.add_plugins(DefaultPlugins.set(log_plugin()))
//!     .add_plugins(NpcPerfPlugin::new().tag("my_game::sim", "game.sim"));
//! ```
//!
//! - [`log_plugin`]: Bevy's `LogPlugin` with the process-wide
//!   [`PerfCollector`] as its `custom_layer` ([`perf_layer`]), so every
//!   system span (Bevy's `trace` feature) and mechanic span is timed, and
//!   the `npc=debug` filter so the framework's own spans are enabled.
//! - [`NpcPerfPlugin`]: the mechanic tags ([`MechanicTags::default_npc`]
//!   plus the host's), the budgets ([`PerfBudgets::default_npc`] scaled for
//!   the build profile by default, plus the rows content registered in a
//!   [`PerfBudgetTable`] and the host's [`NpcPerfPlugin::budget`] rows),
//!   [`PerfHandle`] (the F4 panel and BRP `npc/perf` read it),
//!   [`end_frame_system`] in `Last`, and quiet breach logging
//!   ([`BreachLog::WarnOnce`]: one `warn!` per slow mechanic, then
//!   `debug!`; the F4 panel still marks it; hosts offer `--perf-warn` for
//!   [`BreachLog::Warn`]).
//!
//! Headless hosts do not need this: `npc_perf::install_global` and a
//! `PerfCollector::end_frame` per tick (see `npc_tools`). Profiling never
//! feeds the simulation.

use bevy::log::{BoxedLayer, DEFAULT_FILTER, LogPlugin};
use bevy::prelude::*;
use npc_perf::{
    BreachLog, MechanicTags, PerfBudgetTable, PerfBudgets, PerfCollector, PerfHandle,
    end_frame_system,
};

/// The log filter [`log_plugin`] uses: Bevy's default plus `npc=debug`, and
/// `npc_perf=info` so that the profiler's `debug!` repeats of a budget
/// breach ([`BreachLog::WarnOnce`]) stay off the console (`RUST_LOG`
/// overrides it).
pub fn perf_log_filter() -> String {
    format!("{DEFAULT_FILTER}npc=debug,npc_perf=info")
}

/// Bevy's `LogPlugin` with the profiler layer ([`perf_layer`]) and
/// [`perf_log_filter`]; set it on `DefaultPlugins`
/// (`DefaultPlugins.set(log_plugin())`), then add [`NpcPerfPlugin`].
pub fn log_plugin() -> LogPlugin {
    LogPlugin {
        custom_layer: perf_layer,
        filter: perf_log_filter(),
        ..default()
    }
}

/// Installs [`log_plugin`]'s global subscriber **now**, for a host that
/// adds its sim plugins before `DefaultPlugins` (the testbed builds the sim
/// app first, then its window): Bevy creates a system's span when the
/// system is added, and a span created before any subscriber exists stays
/// disabled forever, so without this none of those systems would be timed.
/// Call it once at the start of `main`, then add
/// `DefaultPlugins.disable::<LogPlugin>()` (the subscriber is already set;
/// a second `LogPlugin` only logs that it could not set it again).
pub fn install_log_plugin_now() {
    log_plugin().build(&mut App::new());
}

/// `LogPlugin::custom_layer`: the process-wide [`PerfCollector`] (default
/// tags, default budgets scaled for the build profile, until
/// [`NpcPerfPlugin`] sets the host's), also inserted as [`PerfHandle`].
pub fn perf_layer(app: &mut App) -> Option<BoxedLayer> {
    let collector = PerfCollector::global();
    collector.set_budgets(PerfBudgets::default_npc().scaled_for_profile());
    app.insert_resource(PerfHandle(collector.clone()));
    Some(collector.boxed_layer())
}

/// Configures the process-wide profiler for this app (see the module
/// docs): tags, budgets, [`PerfHandle`] and the per-frame
/// [`end_frame_system`] in `Last`. Without [`log_plugin`] nothing records
/// spans, but the handle, budgets and panel still work.
#[derive(Clone, Debug)]
pub struct NpcPerfPlugin {
    /// [`MechanicTags::default_npc`] plus the host's (`tag`, `tags_with`).
    pub tags: MechanicTags,
    /// Default: [`PerfBudgets::default_npc`] scaled for the build profile
    /// (×10 with debug assertions). In `Plugin::finish` the rows content
    /// registered ([`PerfBudgetTable`], at one agent, scaled the same way)
    /// fill the mechanics this lacks.
    pub budgets: PerfBudgets,
    /// Add [`end_frame_system`] in `Last` (default `true`; off when the
    /// host closes frames itself, e.g. once per sim tick).
    pub end_frame: bool,
    /// How breaches are logged (default [`BreachLog::WarnOnce`]: normal
    /// play is not spammed every 5 s; breaches are still recorded and the
    /// F4 panel marks the mechanic).
    pub breach_log: BreachLog,
}

impl Default for NpcPerfPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl NpcPerfPlugin {
    pub fn new() -> Self {
        Self {
            tags: MechanicTags::default_npc(),
            budgets: PerfBudgets::default_npc().scaled_for_profile(),
            end_frame: true,
            breach_log: BreachLog::WarnOnce,
        }
    }

    /// Tags systems whose type path starts with `prefix` as `mechanic`.
    pub fn tag(mut self, prefix: impl Into<String>, mechanic: impl Into<String>) -> Self {
        self.tags.add(prefix, mechanic);
        self
    }

    /// Lets `f` add tags, e.g. content's `perf_tags(&mut MechanicTags)`.
    pub fn tags_with(mut self, f: impl FnOnce(&mut MechanicTags)) -> Self {
        f(&mut self.tags);
        self
    }

    /// Replaces the budgets.
    pub fn budgets(mut self, budgets: PerfBudgets) -> Self {
        self.budgets = budgets;
        self
    }

    /// Sets the budget of `mechanic` to `fixed_us + per_agent_us` (one
    /// agent, like [`PerfBudgets::default_npc`]) scaled for the build
    /// profile: a host's own row, winning over content's.
    pub fn budget(mut self, mechanic: impl Into<String>, fixed_us: u64, per_agent_us: u64) -> Self {
        let mut table = PerfBudgetTable::default();
        table.add(mechanic, fixed_us, per_agent_us);
        let mut row = PerfBudgets::default();
        row.add_rows(&table, 1);
        for (m, us) in row.scaled_for_profile().iter() {
            self.budgets.set(m, us);
        }
        self
    }

    /// The budgets with content's rows (`table`) filling the mechanics
    /// [`NpcPerfPlugin::budgets`] lacks, at one agent, scaled for the build
    /// profile.
    pub fn budgets_with(&self, table: &PerfBudgetTable) -> PerfBudgets {
        let mut out = self.budgets.clone();
        let mut rows = PerfBudgets::default();
        rows.add_rows(table, 1);
        for (m, us) in rows.scaled_for_profile().iter() {
            if out.get(m).is_none() {
                out.set(m, us);
            }
        }
        out
    }

    /// Sets how breaches are logged ([`NpcPerfPlugin::breach_log`]).
    pub fn breach_log(mut self, mode: BreachLog) -> Self {
        self.breach_log = mode;
        self
    }

    /// Whether to add [`end_frame_system`] in `Last`.
    pub fn end_frame(mut self, on: bool) -> Self {
        self.end_frame = on;
        self
    }
}

impl Plugin for NpcPerfPlugin {
    fn build(&self, app: &mut App) {
        let collector = PerfCollector::global();
        collector.set_tags(self.tags.clone());
        collector.set_budgets(self.budgets.clone());
        collector.set_breach_log(self.breach_log);
        if !app.world().contains_resource::<PerfHandle>() {
            app.insert_resource(PerfHandle(collector.clone()));
        }
        if self.end_frame {
            app.add_systems(Last, end_frame_system);
        }
    }

    /// Content plugins registered their budget rows by now (any plugin
    /// order).
    fn finish(&self, app: &mut App) {
        if let Some(table) = app.world().get_resource::<PerfBudgetTable>() {
            PerfCollector::global().set_budgets(self.budgets_with(table));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_sets_tags_budgets_handle_and_frames() {
        let plugin = NpcPerfPlugin::new()
            .tag("my_game::sim", "game.sim")
            .tags_with(|t| {
                t.add("my_game::view", "game.view");
            });
        assert_eq!(plugin.tags.mechanic_for("my_game::sim::walk"), "game.sim");
        assert_eq!(plugin.tags.mechanic_for("my_game::view::draw"), "game.view");
        assert_eq!(
            plugin.tags.mechanic_for("npc_needs::decay"),
            "needs",
            "the defaults stay"
        );
        assert_eq!(
            plugin.budgets,
            PerfBudgets::default_npc().scaled_for_profile()
        );
        let mut app = App::new();
        app.add_plugins(plugin);
        let handle = app.world().resource::<PerfHandle>().0.clone();
        assert_eq!(
            handle.tags().mechanic_for("my_game::sim::walk"),
            "game.sim",
            "the global collector got the host's tags"
        );
        assert_eq!(
            handle.budgets(),
            PerfBudgets::default_npc().scaled_for_profile()
        );
        assert_eq!(handle.breach_log(), BreachLog::WarnOnce, "quiet by default");
        app.update(); // end_frame_system runs in Last without panicking
        let filter = log_plugin().filter;
        assert!(filter.ends_with("npc=debug,npc_perf=info"), "{filter}");
    }

    #[test]
    fn content_and_host_budget_rows() {
        let scale = if cfg!(debug_assertions) { 10 } else { 1 };
        let plugin = NpcPerfPlugin::new().budget("game.host", 5, 1);
        assert_eq!(plugin.budgets.get("game.host"), Some(6 * scale));
        let mut table = PerfBudgetTable::default();
        table.add("game.content", 10, 2).add("game.host", 100, 0);
        let all = plugin.budgets_with(&table);
        assert_eq!(all.get("game.content"), Some(12 * scale));
        assert_eq!(all.get("game.host"), Some(6 * scale), "the host's row wins");
        assert_eq!(all.get("think"), plugin.budgets.get("think"));
        let filter = log_plugin().filter;
        assert!(filter.ends_with("npc=debug,npc_perf=info"), "{filter}");
    }
}
