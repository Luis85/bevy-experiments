//! Real-time rates, kept outside the authoritative simulation.
use crate::ViewSystems;
use bevy::prelude::*;
use npc_kernel::driver::{DriverSettings, Pacing};
use npc_kernel::{SimClock, SimControl, SimRate};
use npc_perf::{PerfHandle, PerfTargets, RuntimeStats};
use std::collections::VecDeque;

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct FrameRuntime(pub RuntimeStats);
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RuntimeSet;
#[derive(Default)]
struct Rates {
    frames: VecDeque<u64>,
    frame_ns: u64,
    ticks: VecDeque<(u64, u64)>,
    tick_ns: u64,
    tick_count: u64,
    last_tick: Option<u64>,
    context: Option<(bool, u32, u32, bool)>,
    history_at: f64,
    peak_ms: f64,
}
impl Rates {
    fn record(
        &mut self,
        dt: u64,
        tick: Option<u64>,
        context: (bool, u32, u32, bool),
    ) -> (f64, f64, Option<f64>) {
        if dt == 0 {
            return (0.0, 0.0, tick.map(|_| 0.0));
        }
        self.frames.push_back(dt);
        self.frame_ns = self.frame_ns.saturating_add(dt);
        while self.frames.len() > 120 {
            self.frame_ns = self
                .frame_ns
                .saturating_sub(self.frames.pop_front().unwrap());
        }
        let changed = self.context != Some(context)
            || matches!((tick,self.last_tick),(Some(now),Some(before)) if now<before);
        if changed {
            self.ticks.clear();
            self.tick_ns = 0;
            self.tick_count = 0;
            self.context = Some(context);
        }
        let delta = if changed {
            0
        } else {
            tick.zip(self.last_tick)
                .map_or(0, |(now, before)| now.saturating_sub(before))
        };
        self.last_tick = tick;
        self.ticks.push_back((dt, delta));
        self.tick_ns = self.tick_ns.saturating_add(dt);
        self.tick_count = self.tick_count.saturating_add(delta);
        while (self.tick_ns > 1_000_000_000 && self.ticks.len() > 1) || self.ticks.len() > 600 {
            let (time, count) = self.ticks.pop_front().unwrap();
            self.tick_ns -= time;
            self.tick_count -= count;
        }
        let mean_ms = self.frame_ns as f64 / self.frames.len() as f64 / 1_000_000.0;
        let fps = self.frames.len() as f64 * 1e9 / self.frame_ns.max(1) as f64;
        let tps = tick.map(|_| self.tick_count as f64 * 1e9 / self.tick_ns.max(1) as f64);
        (fps, mean_ms, tps)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<FrameRuntime>()
        .init_resource::<PerfTargets>()
        .add_systems(
            Update,
            update.in_set(RuntimeSet).in_set(ViewSystems::Present),
        );
}
#[allow(clippy::too_many_arguments)]
fn update(
    time: Option<Res<Time<Real>>>,
    virtual_time: Option<Res<Time<Virtual>>>,
    clock: Option<Res<SimClock>>,
    rate: Option<Res<SimRate>>,
    control: Option<Res<SimControl>>,
    driver: Option<Res<DriverSettings>>,
    targets: Res<PerfTargets>,
    collector: Option<Res<PerfHandle>>,
    mut rates: Local<Rates>,
    mut frame: ResMut<FrameRuntime>,
) {
    let stopped = control.as_ref().is_some_and(|c| c.is_stopped());
    let speed = control.as_ref().map_or(1, |c| c.multiplier);
    let hz = rate.as_ref().map_or(0, |r| r.hz);
    let unpaced = driver
        .as_ref()
        .is_some_and(|d| d.pacing == Pacing::EveryRun);
    let scale = virtual_time.as_ref().map_or(1.0, |t| {
        if t.is_paused() {
            0.0
        } else {
            t.relative_speed_f64()
        }
    });
    let paused = stopped || (!unpaced && scale <= 0.0);
    let (fps, frame_ms, tps) = rates.record(
        time.as_ref()
            .map_or(0, |t| t.delta().as_nanos().min(u128::from(u64::MAX)) as u64),
        clock.as_ref().map(|c| c.tick),
        (paused, speed, hz, unpaced),
    );
    frame.0 = RuntimeStats {
        frame_fps: fps,
        frame_ms,
        sim_tps: tps,
        sim_target_tps: if clock.is_some() && rate.is_some() && driver.is_some() && !unpaced {
            Some(if paused {
                0.0
            } else {
                f64::from(hz) * f64::from(speed) * scale
            })
        } else {
            None
        },
        sim_hz: clock.as_ref().and(rate.as_ref()).map(|r| r.hz),
        multiplier: control.as_ref().map(|c| c.multiplier),
        paused,
        frame_target_us: targets.frame_budget_us,
        host_time_scale: virtual_time.as_ref().map(|_| scale),
        driver_cap: driver.as_ref().map(|d| d.max_ticks_per_run),
        pacing: driver
            .as_ref()
            .map(|_| if unpaced { "unpaced" } else { "host_time" }.to_owned()),
    };
    if let Some(c) = collector {
        c.0.record_runtime(frame.0.clone());
        if let Some(time) = time {
            let now = time.elapsed_secs_f64();
            rates.peak_ms = rates.peak_ms.max(time.delta_secs_f64() * 1000.0);
            if now < rates.history_at || now - rates.history_at >= npc_perf::HISTORY_PERIOD_SECS {
                c.0.record_history(now, Some(rates.peak_ms));
                rates.history_at = now;
                rates.peak_ms = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_and_simulation_rates_are_distinct_and_reset_safely() {
        let mut r = Rates::default();
        let mut tick = 100;
        r.record(10_000_000, Some(tick), (false, 1, 20, false));
        for frame in 1..=100 {
            if frame % 5 == 0 {
                tick += 1;
            }
            r.record(10_000_000, Some(tick), (false, 1, 20, false));
        }
        let (fps, ms, tps) = r.record(10_000_000, Some(tick), (false, 1, 20, false));
        assert!((fps - 100.0).abs() < 0.01);
        assert!((ms - 10.0).abs() < 0.01);
        assert!((tps.unwrap() - 20.0).abs() < 0.01);
        let (_, _, paused) = r.record(10_000_000, Some(tick), (true, 1, 20, false));
        assert_eq!(paused, Some(0.0));
        let (_, _, reset) = r.record(10_000_000, Some(0), (true, 1, 20, false));
        assert_eq!(reset, Some(0.0));
        assert_eq!(r.record(10_000_000, None, (false, 1, 0, false)).2, None);
    }
    #[test]
    fn speed_changes_do_not_mix_throughput_windows() {
        let mut r = Rates::default();
        r.record(100_000_000, Some(0), (false, 1, 20, false));
        r.record(100_000_000, Some(2), (false, 1, 20, false));
        assert_eq!(
            r.record(100_000_000, Some(2), (false, 3, 20, false)).2,
            Some(0.0)
        );
    }
}
