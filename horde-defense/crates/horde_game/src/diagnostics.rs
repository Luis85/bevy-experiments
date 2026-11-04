//! Opt-in, bounded native frame evidence. Normal play does not write benchmark files.
use bevy::{app::AppExit, prelude::*, render::renderer::RenderAdapterInfo};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

#[derive(Clone, Copy)]
pub enum WorkPhase {
    Simulation,
    Persistence,
    Interface,
}
static WORK_NS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static MEASURE_WORK: AtomicBool = AtomicBool::new(false);
/// Set once by `main` from `LaunchConfig`: only performance reports time work.
pub fn enable_work_timing(enabled: bool) {
    MEASURE_WORK.store(enabled, Ordering::Relaxed);
}
pub struct WorkTimer {
    phase: usize,
    start: Instant,
}
impl Drop for WorkTimer {
    fn drop(&mut self) {
        let ns = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        WORK_NS[self.phase].fetch_add(ns, Ordering::Relaxed);
    }
}
/// Opt-in aggregate work costs. No timer, allocation or filesystem IO in normal play.
pub fn work_timer(phase: WorkPhase) -> Option<WorkTimer> {
    MEASURE_WORK.load(Ordering::Relaxed).then(|| WorkTimer {
        phase: phase as usize,
        start: Instant::now(),
    })
}
fn take_work() -> [f64; 3] {
    std::array::from_fn(|i| WORK_NS[i].swap(0, Ordering::Relaxed) as f64 / 1_000_000.)
}
#[derive(Default)]
pub struct FrameCapture {
    initialized: bool,
    path: Option<PathBuf>,
    frames: u32,
    samples: Vec<f64>,
    work: Vec<[f64; 3]>,
    unfocused_samples: usize,
    warmup: u32,
    target_samples: u32,
    active_only: bool,
    initial_battle: Option<BattleContext>,
    peak_enemies: usize,
    peak_corpses: usize,
    done: bool,
}
#[derive(Clone, Serialize)]
struct BattleContext {
    mode: horde_core::content::Mode,
    simulation_tick: u64,
    lane_count: u8,
    width_yards: f32,
    enemies: usize,
    corpses: usize,
}
#[derive(Serialize)]
struct FrameReport {
    version: u32,
    scene: String,
    scope: &'static str,
    cpu: String,
    adapter: String,
    backend: String,
    width_physical: u32,
    height_physical: u32,
    windows_scale: f64,
    ui_scale: f32,
    warmup_frames: u32,
    sample_frames: usize,
    requested_sample_frames: u32,
    completed_sample_target: bool,
    initial_battle: Option<BattleContext>,
    peak_visible_enemies: usize,
    peak_corpses: usize,
    persistence: crate::runner::PersistenceMetrics,
    mean_frame_ms: f64,
    median_frame_ms: f64,
    p95_frame_ms: f64,
    p99_frame_ms: f64,
    mean_observed_fps: f64,
    visible_enemies: usize,
    lane_count: Option<u8>,
    width_yards: Option<f32>,
    population_cap: Option<usize>,
    threat_rank: Option<u32>,
    visible_structures: usize,
    parallel_battles: usize,
    simulation_ticks: u64,
    paused: bool,
    window_focused: bool,
    unfocused_sample_frames: usize,
    frames_over_33ms: usize,
    frames_over_50ms: usize,
    frames_over_100ms: usize,
    frames_over_250ms: usize,
    work_scope: &'static str,
    work_mean_ms: [f64; 3],
    work_max_ms: [f64; 3],
    longest_frames: Vec<SlowFrame>,
}
#[derive(Serialize)]
struct SlowFrame {
    sample_index: usize,
    frame_ms: f64,
    work_ms: [f64; 3],
}
fn quantile(sorted: &[f64], percent: f64) -> f64 {
    if sorted.is_empty() {
        return 0.;
    }
    sorted[((sorted.len() - 1) as f64 * percent).round() as usize]
}
pub fn capture_performance(
    mut state: Local<FrameCapture>,
    time: Res<Time<Real>>,
    mut frontend: ResMut<crate::ui::Frontend>,
    window: Single<&Window>,
    adapter: Option<Res<RenderAdapterInfo>>,
    config: Res<crate::launch::LaunchConfig>,
    mut exit: MessageWriter<AppExit>,
) {
    if !state.initialized {
        state.initialized = true;
        if let Some(perf) = config.perf() {
            state.path = perf.report.clone();
            state.warmup = perf.warmup_frames;
            state.target_samples = perf.frames;
            state.active_only = perf.active_only;
        }
    }
    if state.path.is_none() || state.done {
        return;
    }
    let work = take_work();
    if let Some(a) = &frontend.game.attempt {
        if state.initial_battle.is_none() {
            state.initial_battle = Some(BattleContext {
                mode: a.mode,
                simulation_tick: a.tick,
                lane_count: a.lane_count,
                width_yards: a.width_yards,
                enemies: a.enemies.len(),
                corpses: a.corpses.len(),
            });
        }
        state.peak_enemies = state.peak_enemies.max(a.enemies.len());
        state.peak_corpses = state.peak_corpses.max(a.corpses.len());
    }
    let attempt_finished =
        state.active_only && frontend.game.attempt.is_none() && !state.samples.is_empty();
    if !attempt_finished {
        if state.active_only
            && (frontend.game.global_paused
                || frontend.game.attempt.as_ref().is_none_or(|a| a.paused))
        {
            return;
        }
        state.frames += 1;
        if state.frames <= state.warmup {
            return;
        }
        let ms = time.delta_secs_f64() * 1000.;
        if ms.is_finite() && ms > 0. {
            state.samples.push(ms);
            state.work.push(work);
            state.unfocused_samples += usize::from(!window.focused);
        }
        if state.samples.len() < state.target_samples as usize {
            return;
        }
    }
    state.done = true;
    let mean = state.samples.iter().sum::<f64>() / state.samples.len() as f64;
    let mut sorted = state.samples.clone();
    sorted.sort_by(f64::total_cmp);
    let attempt = frontend.game.attempt.as_ref();
    let mut longest: Vec<_> = state.samples.iter().copied().enumerate().collect();
    longest.sort_by(|a, b| b.1.total_cmp(&a.1));
    longest.truncate(20);
    let work_mean_ms = std::array::from_fn(|i| {
        state.work.iter().map(|w| w[i]).sum::<f64>() / state.work.len() as f64
    });
    let work_max_ms = std::array::from_fn(|i| state.work.iter().map(|w| w[i]).fold(0., f64::max));
    let report = FrameReport {
        version: 2,
        scene: config
            .scenario
            .clone()
            .unwrap_or_else(|| "ordinary-session".into()),
        scope: "Observed native application frame intervals including VSync and OS scheduling; not GPU timestamps or a minimum-spec guarantee",
        cpu: config
            .perf()
            .and_then(|perf| perf.cpu.clone())
            .unwrap_or_else(|| "not recorded".into()),
        adapter: adapter
            .as_ref()
            .map_or_else(|| "not reported".into(), |a| a.name.clone()),
        backend: adapter
            .as_ref()
            .map_or_else(|| "not reported".into(), |a| format!("{:?}", a.backend)),
        width_physical: window.resolution.physical_width(),
        height_physical: window.resolution.physical_height(),
        windows_scale: f64::from(window.resolution.scale_factor()),
        ui_scale: config.review().and_then(|r| r.scale).unwrap_or(1.),
        warmup_frames: state.warmup,
        sample_frames: sorted.len(),
        requested_sample_frames: state.target_samples,
        completed_sample_target: state.samples.len() >= state.target_samples as usize,
        initial_battle: state.initial_battle.clone(),
        peak_visible_enemies: state.peak_enemies,
        peak_corpses: state.peak_corpses,
        persistence: frontend.runtime.persistence_metrics(),
        mean_frame_ms: mean,
        median_frame_ms: quantile(&sorted, 0.5),
        p95_frame_ms: quantile(&sorted, 0.95),
        p99_frame_ms: quantile(&sorted, 0.99),
        mean_observed_fps: 1000. / mean,
        visible_enemies: attempt.map_or(0, |a| a.enemies.len()),
        lane_count: attempt.map(|a| a.lane_count),
        width_yards: attempt.map(|a| a.width_yards),
        population_cap: attempt.map(|a| a.enemy_cap()),
        threat_rank: attempt.map(|a| a.progression.rank),
        visible_structures: attempt.map_or(0, |a| a.structures.len()),
        parallel_battles: frontend.game.battles.iter().filter(|b| !b.settled).count(),
        simulation_ticks: frontend.runtime.simulated_ticks,
        paused: frontend.game.global_paused || attempt.is_some_and(|a| a.paused),
        window_focused: window.focused,
        unfocused_sample_frames: state.unfocused_samples,
        frames_over_33ms: state.samples.iter().filter(|&&ms| ms > 33.).count(),
        frames_over_50ms: state.samples.iter().filter(|&&ms| ms > 50.).count(),
        frames_over_100ms: state.samples.iter().filter(|&&ms| ms > 100.).count(),
        frames_over_250ms: state.samples.iter().filter(|&&ms| ms > 250.).count(),
        work_scope: "Simulation, persistence, interface elapsed application work in milliseconds, in that order (wall clock, including waits). Concurrent ECS scheduling can offset attribution by one native frame; aggregates and maxima locate expensive phases, not GPU timestamps or CPU-cycle samples.",
        work_mean_ms,
        work_max_ms,
        longest_frames: longest
            .into_iter()
            .map(|(sample_index, frame_ms)| SlowFrame {
                sample_index,
                frame_ms,
                work_ms: state.work[sample_index],
            })
            .collect(),
    };
    let path = state.path.as_ref().expect("configured report");
    let result = (|| -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => {
            info!("Native frame evidence: {}", path.display());
            // The ordinary persistence service owns the durable close barrier.
            // Do not exit over an older periodic checkpoint still in flight.
            frontend.request_save(crate::runner::SaveIntent::Quit);
        }
        Err(error) => {
            error!("Frame report failed: {error}");
            exit.write(AppExit::error());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tail_frame_quantiles_use_measured_intervals_without_discarding_jank() {
        let data = [16., 16., 16., 17., 100.];
        assert_eq!(quantile(&data, 0.5), 16.);
        assert_eq!(quantile(&data, 0.95), 100.);
        assert_eq!(quantile(&data, 0.99), 100.);
        assert_eq!(quantile(&[], 0.95), 0.);
    }
}
