//! Simulation time in ticks. The tick rate is the host's choice
//! ([`SimRate`](crate::driver::SimRate), default [`DEFAULT_TICK_HZ`]); days
//! are `SimConfig::day_length` ticks. Sim systems only ever see the tick.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{Brain, ThinkRequest};
use serde::{Deserialize, Serialize};

use crate::ids::SimId;
use crate::journal::{Journal, JournalKind};

/// Default ticks per simulated second (`NpcPlugins::tick_hz` changes it).
pub const DEFAULT_TICK_HZ: u32 = 20;

/// Every `Brain` agent re-thinks at least once per this many ticks, so a
/// long-running leaf (e.g. a night's sleep) never hides a reactive guard
/// that became true without a need-level change (e.g. food appeared).
pub const RETHINK_INTERVAL: u64 = 40;

/// `NpcSet::Needs`: on the first tick of every night and every day, every
/// `Brain` agent gets a `ThinkRequest` (nightfall and morning are re-think
/// triggers). Journals `PhaseChange`.
pub fn day_night_system(
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    brains: Query<Entity, With<Brain>>,
    journal: Option<ResMut<Journal>>,
    mut commands: Commands,
) {
    if !config.phase_changes_at(clock.tick) {
        return;
    }
    if let Some(mut journal) = journal {
        let night = config.is_night(clock.tick);
        journal.push(clock.tick, None, JournalKind::PhaseChange { night });
    }
    for entity in &brains {
        commands.entity(entity).insert(ThinkRequest);
    }
}

/// `NpcSet::Needs`: the staggered re-think timer. Every `Brain` agent gets a
/// `ThinkRequest` when `(tick + sim_id) % RETHINK_INTERVAL == 0`, so agents
/// re-think every `RETHINK_INTERVAL` ticks, spread over different ticks.
/// Purely per-entity, so iteration order does not matter.
pub fn rethink_timer_system(
    clock: Res<SimClock>,
    brains: Query<(Entity, &SimId), With<Brain>>,
    mut commands: Commands,
) {
    for (entity, id) in &brains {
        if (clock.tick.wrapping_add(u64::from(id.0))) % RETHINK_INTERVAL == 0 {
            commands.entity(entity).insert(ThinkRequest);
        }
    }
}

/// `NpcSet::Cleanup`: `tick += 1`, the last thing every `NpcSimTick` does.
pub fn advance_clock_system(mut clock: ResMut<SimClock>) {
    clock.tick += 1;
}

/// Current tick. Advanced by exactly 1 at the end of every `NpcSimTick` run.
#[derive(
    Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Resource)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::clock"]
pub struct SimClock {
    pub tick: u64,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimConfig {
    /// Ticks per day. Default 2400 (2 minutes at 1×).
    pub day_length: u64,
    /// Night is the last `night_permille` of each day. Default 300.
    pub night_permille: u32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            day_length: 2400,
            night_permille: 300,
        }
    }
}

impl SimConfig {
    /// 0-based day number.
    pub fn day(&self, tick: u64) -> u64 {
        tick / self.day_length
    }

    /// Position within the day, 0..1000.
    pub fn time_of_day_permille(&self, tick: u64) -> u32 {
        ((tick % self.day_length) * 1000 / self.day_length) as u32
    }

    pub fn is_night(&self, tick: u64) -> bool {
        self.time_of_day_permille(tick) >= 1000 - self.night_permille
    }

    /// True when `tick` is the first tick of a night or of a day (never for tick 0).
    pub fn phase_changes_at(&self, tick: u64) -> bool {
        tick > 0 && self.is_night(tick) != self.is_night(tick - 1)
    }
}
