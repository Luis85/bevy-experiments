use crate::{
    launch::LaunchConfig,
    runner::{GameSession, RuntimeSet},
    ui::BattleUi,
};
use bevy::prelude::*;
use horde_core::sim::Attempt;
use std::collections::BTreeMap;

/// Observation speed advances at most this many whole ticks per fixed step.
pub(super) const MAX_TICKS_PER_STEP: u64 = 4;

/// Records pre-tick positions and the frame's blend fraction for `BattleUi`.
pub struct BattleViewPlugin;
impl Plugin for BattleViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            record_tick_snapshots.before(RuntimeSet::Advance),
        )
        .add_systems(Update, update_blend_fraction);
    }
}

/// Runs before each fixed step's ticks: what is drawn as "previous".
fn record_tick_snapshots(session: Res<GameSession>, mut battle: ResMut<BattleUi>) {
    let game = &session.game;
    battle.interpolation.record(
        game.attempt().into_iter().map(|a| (None, a)).chain(
            game.battles()
                .iter()
                .filter_map(|b| b.game().attempt().map(|a| (Some(b.id), a))),
        ),
    );
}

fn update_blend_fraction(
    time: Res<Time<Fixed>>,
    config: Res<LaunchConfig>,
    mut battle: ResMut<BattleUi>,
) {
    battle.interpolation.fraction = blend_fraction(&config, time.overstep_fraction());
}

/// Fixed-step exports and automated screenshots draw the authoritative tick
/// exactly (fraction 1), so their frames do not depend on wall-clock overstep.
pub fn blend_fraction(config: &LaunchConfig, overstep: f32) -> f32 {
    if config.sequence_path().is_some() || config.capture_path().is_some() {
        1.0
    } else if overstep.is_finite() {
        overstep.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

#[derive(Clone, Debug)]
struct TickSnapshot {
    /// `None` for the active attempt, else the parallel battle id.
    scope: Option<u64>,
    attempt: u64,
    tick: u64,
    positions: BTreeMap<u64, [f32; 2]>,
}

/// Presentation-only cache of the positions each actor had before the latest
/// fixed step, keyed by scene and actor id.
#[derive(Clone, Debug)]
pub struct ActorInterpolation {
    pub fraction: f32,
    snapshots: Vec<TickSnapshot>,
}
impl Default for ActorInterpolation {
    fn default() -> Self {
        Self {
            fraction: 1.0,
            snapshots: Vec::new(),
        }
    }
}
impl ActorInterpolation {
    pub fn record<'a>(&mut self, attempts: impl IntoIterator<Item = (Option<u64>, &'a Attempt)>) {
        self.snapshots.clear();
        for (scope, a) in attempts {
            self.snapshots.push(TickSnapshot {
                scope,
                attempt: a.id,
                tick: a.tick,
                positions: a.enemies.iter().map(|e| (e.id, [e.x, e.y()])).collect(),
            });
        }
    }
    /// The blend for one drawn attempt; identity unless a snapshot of this
    /// scene was taken within the latest fixed step.
    pub fn scene(&self, scope: Option<u64>, a: &Attempt) -> SceneBlend<'_> {
        let snapshot = self.snapshots.iter().find(|s| {
            s.scope == scope
                && s.attempt == a.id
                && s.tick < a.tick
                && a.tick - s.tick <= MAX_TICKS_PER_STEP
        });
        SceneBlend {
            fraction: self.fraction,
            previous: snapshot.map(|s| &s.positions),
        }
    }
}

/// Copyable blend handle for one attempt's frame.
#[derive(Clone, Copy, Debug)]
pub struct SceneBlend<'a> {
    fraction: f32,
    previous: Option<&'a BTreeMap<u64, [f32; 2]>>,
}
impl SceneBlend<'_> {
    /// Draw authoritative positions exactly.
    #[cfg(test)]
    pub const NONE: SceneBlend<'static> = SceneBlend {
        fraction: 1.0,
        previous: None,
    };
    pub fn position(&self, id: u64, current: [f32; 2]) -> [f32; 2] {
        match self.previous.and_then(|previous| previous.get(&id)) {
            Some(before) if self.fraction < 1.0 => [
                before[0] + (current[0] - before[0]) * self.fraction,
                before[1] + (current[1] - before[1]) * self.fraction,
            ],
            _ => current,
        }
    }
}
