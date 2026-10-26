//! Per-mechanic p95 time budgets and breach records.

use std::collections::BTreeMap;

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

/// p95 budget in µs per mechanic, checked by
/// [`PerfCollector::end_frame`](crate::PerfCollector::end_frame) against the
/// mechanic's rolling per-frame samples. Serializes as a plain JSON object
/// `{ "<mechanic>": <p95 µs>, .. }`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Resource)]
#[serde(transparent)]
pub struct PerfBudgets(pub BTreeMap<String, u64>);

/// `(mechanic, fixed µs, per-agent µs)`: the budget at `n` agents is
/// `fixed + per_agent * n`. See [`PerfBudgets::default_npc`]. The
/// framework's own mechanics only: content adds its rows by name
/// ([`PerfBudgetTable`]; the village registers `actions.eat`,
/// `actions.work`, `actions.sleep`, `actions.idle`).
pub const NPC_BUDGET_TABLE: &[(&str, u64, u64)] = &[
    ("inputs", 20, 1),
    ("needs", 15, 2),
    ("clock", 10, 0),
    ("think", 40, 20),
    ("actions.move", 10, 2),
    ("spatial", 10, 1),
    ("locomotion", 10, 2),
    ("nav", 30, 2),
    ("nav.astar", 2500, 0),
    ("perception", 50, 5),
    // Approved by the dev (plan Task 9 fix round): fog
    // of war, measured in the release explorer world (one actor) at p95
    // 16 µs idle / 38 µs walking windowed, 2 / 14 µs headless.
    ("fog", 55, 0),
    ("rules", 10, 1),
    ("agent_io", 15, 2),
    ("interaction", 25, 2),
    ("progression", 10, 2),
    ("crafting", 15, 2),
    ("command", 15, 2),
    ("vehicles", 30, 2),
    ("effects", 15, 2),
    ("journal", 20, 2),
    ("view.visuals", 200, 10),
    ("view.anim", 200, 10),
    ("view.overlay", 500, 0),
    ("view.camera", 100, 0),
    ("view.scene", 100, 0),
];

/// Budget rows content registers for its own mechanics (`PerfAppExt::
/// register_budget` in the facade, `NpcPerfPlugin::budget`): mechanic →
/// `(fixed µs, per-agent µs)`, like [`NPC_BUDGET_TABLE`]. A plain resource
/// in the sim app; never hashed, never read by the sim. Hosts turn it into
/// budgets with [`PerfBudgets::default_npc_with`] (the bench and
/// `NpcPerfPlugin` do).
#[derive(Clone, Debug, Default, PartialEq, Eq, Resource)]
pub struct PerfBudgetTable(pub BTreeMap<String, (u64, u64)>);

impl PerfBudgetTable {
    /// Adds (or replaces) the row of `mechanic`.
    pub fn add(
        &mut self,
        mechanic: impl Into<String>,
        fixed_us: u64,
        per_agent_us: u64,
    ) -> &mut Self {
        self.0.insert(mechanic.into(), (fixed_us, per_agent_us));
        self
    }

    /// `(mechanic, fixed µs, per-agent µs)` in mechanic order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64, u64)> {
        self.0.iter().map(|(m, &(f, p))| (m.as_str(), f, p))
    }
}

/// Factor [`PerfBudgets::scaled_for_profile`] applies in builds with debug
/// assertions (dev builds run the sim at `opt-level = 1`, about 5-10x
/// slower than release; see `docs/debugging.md` §7).
pub const DEBUG_BUDGET_SCALE: u64 = 10;

impl PerfBudgets {
    /// Sets the p95 budget (µs) for `mechanic`.
    pub fn set(&mut self, mechanic: impl Into<String>, p95_us: u64) -> &mut Self {
        self.0.insert(mechanic.into(), p95_us);
        self
    }

    /// The p95 budget (µs) for `mechanic`, if any.
    pub fn get(&self, mechanic: &str) -> Option<u64> {
        self.0.get(mechanic).copied()
    }

    /// Removes the budget for `mechanic`.
    pub fn remove(&mut self, mechanic: &str) -> Option<u64> {
        self.0.remove(mechanic)
    }

    /// `(mechanic, p95 µs)` in mechanic order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.0.iter().map(|(m, &us)| (m.as_str(), us))
    }

    /// Default budgets for **one** villager, per frame (or per `SimTick` when
    /// `end_frame` is called once per tick), as p95 µs. Generous on purpose:
    /// they catch regressions of an order of magnitude, not noise.
    ///
    /// | mechanic | p95 µs @ 1 agent | + per agent |
    /// |---|---|---|
    /// | `inputs` | 21 | 1 |
    /// | `needs` | 17 | 2 |
    /// | `clock` | 10 | 0 |
    /// | `think` | 60 | 20 |
    /// | `actions.move` | 12 | 2 |
    /// | `effects` | 17 | 2 |
    /// | `journal` | 22 | 2 |
    /// | `view.visuals`, `view.anim` | 210 | 10 |
    /// | `view.overlay` | 500 | 0 |
    /// | `view.camera`, `view.scene` | 100 | 0 |
    ///
    /// Measured on 2026-10-23 (release, 1 villager, 3000 ticks): every sim
    /// system is under 0.5 µs p95 and `think` is ~3 µs p95 (up to ~50 µs on a
    /// tick where the agent re-thinks), so these leave ample room for debug
    /// builds (`opt-level = 1`) and the layer's own ~0.25 µs per span.
    ///
    /// `engine`, `render`, `view.driver` (inclusive of the whole tick),
    /// `untagged` and the `think.*` breakdown spans from `npc_ai` have no
    /// budget. Use [`PerfBudgets::default_npc_for`] for
    /// larger villages, and [`PerfBudgets::default_npc_with`] to add the
    /// rows content registered (the village's `actions.eat`, ..: 12 µs + 2
    /// per agent each).
    pub fn default_npc() -> Self {
        Self::default_npc_for(1)
    }

    /// Every budget multiplied by `factor` (saturating).
    pub fn scaled(mut self, factor: u64) -> Self {
        for us in self.0.values_mut() {
            *us = us.saturating_mul(factor);
        }
        self
    }

    /// The budgets for the build profile: multiplied by
    /// [`DEBUG_BUDGET_SCALE`] when `cfg(debug_assertions)`, unchanged in
    /// release. Hosts (the village game) use this so a dev build does not
    /// warn on every mechanic; benchmarks and baselines are release-only.
    pub fn scaled_for_profile(self) -> Self {
        if cfg!(debug_assertions) {
            self.scaled(DEBUG_BUDGET_SCALE)
        } else {
            self
        }
    }

    /// [`PerfBudgets::default_npc`] scaled to `agents` agents: each budget is
    /// `fixed + per_agent * agents` from [`NPC_BUDGET_TABLE`].
    pub fn default_npc_for(agents: u32) -> Self {
        let mut budgets = Self::default();
        for &(mechanic, fixed, per_agent) in NPC_BUDGET_TABLE {
            budgets.set(mechanic, fixed + per_agent * u64::from(agents));
        }
        budgets
    }

    /// [`PerfBudgets::default_npc_for`] plus content's rows (`table`, which
    /// win over a core row of the same mechanic), at `agents` agents.
    pub fn default_npc_with(table: &PerfBudgetTable, agents: u32) -> Self {
        let mut budgets = Self::default_npc_for(agents);
        budgets.add_rows(table, agents);
        budgets
    }

    /// Sets `fixed + per_agent * agents` for every row of `table`.
    pub fn add_rows(&mut self, table: &PerfBudgetTable, agents: u32) -> &mut Self {
        for (mechanic, fixed, per_agent) in table.iter() {
            self.set(
                mechanic,
                fixed.saturating_add(per_agent.saturating_mul(u64::from(agents))),
            );
        }
        self
    }

    /// [`PerfBudgets::default_npc_with`] the [`PerfBudgetTable`] of
    /// `world` (none registered: [`PerfBudgets::default_npc_for`]).
    pub fn default_npc_for_world(world: &bevy_ecs::world::World, agents: u32) -> Self {
        match world.get_resource::<PerfBudgetTable>() {
            Some(table) => Self::default_npc_with(table, agents),
            None => Self::default_npc_for(agents),
        }
    }
}

/// A mechanic whose rolling p95 went over its budget. Recorded by
/// `end_frame` at most once per mechanic per 5 s of wall time, and only once
/// the mechanic has at least `min_samples` samples since the last reset
/// ([`PerfCollector::set_min_samples`](crate::PerfCollector::set_min_samples)).
/// Never part of the sim journal or state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetBreach {
    pub mechanic: String,
    /// Rolling p95 at the time of the breach.
    pub p95_us: f64,
    pub budget_us: u64,
    /// Value of the collector's frame counter (1-based) when it was detected.
    pub frame: u64,
}
