use crate::{
    launch::LaunchConfig,
    runner::{GameSession, RuntimeSet},
    ui::BattleUi,
};
use bevy::prelude::*;
use horde_core::sim::{Attempt, Enemy};

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
        .add_systems(Update, (update_blend_fraction, advance_sky_clock));
    }
}

/// Advances the storm lightning clock (rulings RL, W-8): unscaled real time,
/// paused frames excluded, so lightning keeps its wall-clock pacing at every
/// observation speed.
fn advance_sky_clock(
    time: Res<Time<Real>>,
    config: Res<LaunchConfig>,
    session: Res<GameSession>,
    mut battle: ResMut<BattleUi>,
) {
    let game = &session.game;
    let fixed_step = config.sequence_path().is_some() || config.capture_path().is_some();
    let frozen = game.global_paused() || !any_attempt_running(game);
    battle.sky_wall_ms = sky_wall_ms(
        battle.sky_wall_ms,
        time.delta_secs_f64() * 1000.0,
        frozen,
        fixed_step.then(|| game.attempt().map_or(0, |a| a.tick)),
    );
}

/// True while the active attempt or any parallel battle is unpaused.
fn any_attempt_running(game: &horde_core::sim::Game) -> bool {
    game.attempt().is_some_and(|a| !a.paused)
        || game
            .battles()
            .iter()
            .any(|b| b.game().attempt().is_some_and(|a| !a.paused))
}

/// The lightning clock after one frame of `dt_ms` real time. Fixed-step
/// exports and automated screenshots (`export_tick`) read the attempt's
/// tick at 60 Hz instead, so their frames do not depend on the wall clock.
pub fn sky_wall_ms(wall_ms: f64, dt_ms: f64, frozen: bool, export_tick: Option<u64>) -> f64 {
    if let Some(tick) = export_tick {
        return crate::art::SkyClock::of_tick(tick, 1).wall_ms;
    }
    let mut clock = crate::art::SkyClock { wall_ms, speed: 1 };
    clock.advance(dt_ms, frozen);
    clock.wall_ms
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

#[cfg(test)]
mod sky_clock_tests {
    use super::sky_wall_ms;

    /// The lightning clock counts real time over unpaused frames only, and a
    /// fixed-step export reads the attempt's tick instead of the wall clock.
    #[test]
    fn sky_clock_counts_unpaused_real_time_and_exports_read_the_tick() {
        let mut wall = 0.0;
        for _ in 0..60 {
            wall = sky_wall_ms(wall, 16.0, false, None);
        }
        assert_eq!(wall, 960.0);
        assert_eq!(sky_wall_ms(wall, 16.0, true, None), 960.0, "paused");
        assert_eq!(sky_wall_ms(wall, f64::NAN, false, None), 960.0);
        assert_eq!(sky_wall_ms(wall, -5.0, false, None), 960.0);
        // 600 ticks are 10 s of export time, whatever the wall clock says.
        assert_eq!(sky_wall_ms(wall, 16.0, false, Some(600)), 10_000.0);
        assert_eq!(sky_wall_ms(wall, 900.0, true, Some(600)), 10_000.0);
    }
}

#[derive(Clone, Debug, Default)]
struct TickSnapshot {
    /// `None` for the active attempt, else the parallel battle id.
    scope: Option<u64>,
    attempt: u64,
    tick: u64,
    /// `(id, position)` of every enemy, in `a.enemies` order.
    positions: Vec<(u64, [f32; 2])>,
    /// The largest recorded id: an enemy with a larger one spawned since.
    newest: Option<u64>,
}

/// Presentation-only cache of the positions each actor had before the latest
/// fixed step, per scene, in the attempt's enemy order.
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
    /// Replaces the snapshots, reusing their position buffers.
    pub fn record<'a>(&mut self, attempts: impl IntoIterator<Item = (Option<u64>, &'a Attempt)>) {
        let mut recorded = 0;
        for (scope, a) in attempts {
            if recorded == self.snapshots.len() {
                self.snapshots.push(TickSnapshot::default());
            }
            let snapshot = &mut self.snapshots[recorded];
            snapshot.scope = scope;
            snapshot.attempt = a.id;
            snapshot.tick = a.tick;
            snapshot.positions.clear();
            snapshot
                .positions
                .extend(a.enemies.iter().map(|e| (e.id, [e.x, e.y()])));
            snapshot.newest = a.enemies.iter().map(|e| e.id).max();
            recorded += 1;
        }
        self.snapshots.truncate(recorded);
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
            previous: snapshot.map(|s| (s.positions.as_slice(), s.newest)),
        }
    }
}

/// (id, position) of a snapshot's enemies, in attempt order.
type Positions<'a> = &'a [(u64, [f32; 2])];

/// Copyable blend handle for one attempt's frame.
#[derive(Clone, Copy, Debug)]
pub struct SceneBlend<'a> {
    fraction: f32,
    /// The snapshot's positions in attempt order, and its largest id.
    previous: Option<(Positions<'a>, Option<u64>)>,
}
impl SceneBlend<'_> {
    /// Draw authoritative positions exactly.
    #[cfg(test)]
    pub const NONE: SceneBlend<'static> = SceneBlend {
        fraction: 1.0,
        previous: None,
    };
    /// The drawn position of each of `enemies`, in their order.
    ///
    /// The snapshot is walked once beside them: the core removes enemies in
    /// place and appends new ones, so each enemy's previous position lies
    /// after the one before it. An id beyond the snapshot's largest spawned
    /// since and draws where it is. Only an enemy found neither way (a
    /// reordered list) is searched for by id.
    pub fn positions(&self, enemies: &[Enemy]) -> Vec<[f32; 2]> {
        let current = |e: &Enemy| [e.x, e.y()];
        let (previous, newest) = match self.previous {
            Some(previous) if self.fraction < 1.0 => previous,
            _ => return enemies.iter().map(current).collect(),
        };
        let mut cursor = 0;
        enemies
            .iter()
            .map(|e| {
                let now = current(e);
                let ahead = previous[cursor..].iter().position(|(id, _)| *id == e.id);
                let before = match ahead {
                    Some(skip) => {
                        cursor += skip + 1;
                        Some(previous[cursor - 1].1)
                    }
                    None if newest.is_none_or(|newest| e.id > newest) => None,
                    None => {
                        #[cfg(test)]
                        super::builds::record_id_lookup();
                        previous[..cursor]
                            .iter()
                            .find(|(id, _)| *id == e.id)
                            .map(|(_, at)| *at)
                    }
                };
                match before {
                    Some(before) => [
                        before[0] + (now[0] - before[0]) * self.fraction,
                        before[1] + (now[1] - before[1]) * self.fraction,
                    ],
                    None => now,
                }
            })
            .collect()
    }
}
