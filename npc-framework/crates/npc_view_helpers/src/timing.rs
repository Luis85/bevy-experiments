//! Wall time per sim tick, for the perf panel. Measured view-side around the
//! kernel driver (`NpcDriverSet`), so the sim crates never read a clock.

use std::time::Instant;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use npc_kernel::SimTime;
use npc_kernel::driver::NpcDriverSet;

/// Wall time of the last driver run per tick (view-side, never read by the
/// sim): shown by the F4 panel.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SimTickTiming {
    /// The last measured tick, in µs.
    pub last_us: Option<f32>,
    /// Exponential moving average per tick, in µs.
    pub ema_us: Option<f32>,
}

impl SimTickTiming {
    fn record(&mut self, us: f32) {
        self.last_us = Some(us);
        self.ema_us = Some(self.ema_us.map_or(us, |e| e + (us - e) * 0.05));
    }
}

/// When the current driver run started, and `SimTime.ticks_driven` then.
#[derive(Resource, Default)]
struct RunStart(Option<(Instant, u64)>);

fn before_driver(mut start: ResMut<RunStart>, time: Option<Res<SimTime>>) {
    start.0 = Some((Instant::now(), time.map_or(0, |t| t.ticks_driven)));
}

/// Records µs per tick when the driver ran ticks since `before_driver` (the
/// pair runs in every schedule the driver may live in; only the one with
/// the driver sees ticks).
fn after_driver(
    mut start: ResMut<RunStart>,
    time: Option<Res<SimTime>>,
    mut timing: ResMut<SimTickTiming>,
) {
    let (Some((started, before)), Some(time)) = (start.0.take(), time) else {
        return;
    };
    let ticks = time.ticks_driven.saturating_sub(before);
    if ticks > 0 {
        let us = started.elapsed().as_secs_f32() * 1e6 / ticks as f32;
        timing.record(us);
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<SimTickTiming>()
        .init_resource::<RunStart>();
    // The driver may run in any schedule; time it wherever it is. The
    // systems are no-ops in schedules without the driver.
    for schedule in [FixedUpdate.intern(), Update.intern()] {
        app.add_systems(
            schedule,
            (
                before_driver.before(NpcDriverSet),
                after_driver.after(NpcDriverSet),
            ),
        );
    }
}
