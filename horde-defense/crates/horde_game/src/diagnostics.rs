//! Opt-in work timers. The bounded frame-evidence report that reads them is
//! review tooling (`frame_report`). Normal play does not time or write anything.
use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};
#[cfg(feature = "review")]
mod frame_report;
#[cfg(feature = "review")]
pub use frame_report::capture_performance;

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
#[cfg(feature = "review")]
fn take_work() -> [f64; 3] {
    std::array::from_fn(|i| WORK_NS[i].swap(0, Ordering::Relaxed) as f64 / 1_000_000.)
}
