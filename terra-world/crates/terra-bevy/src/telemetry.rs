//! Bounded, host-local timing evidence. Frame intervals are not GPU timings.
use bevy::prelude::*;
use serde::Serialize;
use std::{collections::BTreeMap, time::Instant};

pub const TIMING_WINDOW: usize = 240;

#[derive(Debug, Clone)]
pub struct TimingWindow {
    values: [f64; TIMING_WINDOW],
    cursor: usize,
    len: usize,
    total_samples: u64,
}
impl Default for TimingWindow {
    fn default() -> Self {
        Self {
            values: [0.; TIMING_WINDOW],
            cursor: 0,
            len: 0,
            total_samples: 0,
        }
    }
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct TimingSummary {
    pub last_ms: f64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub window_samples: usize,
    pub total_samples: u64,
}
impl TimingWindow {
    /// Invalid measurements cannot poison a report. The window has fixed storage.
    pub fn record_ms(&mut self, value: f64) {
        if !value.is_finite() || value < 0. {
            return;
        }
        self.values[self.cursor] = value;
        self.cursor = (self.cursor + 1) % TIMING_WINDOW;
        self.len = (self.len + 1).min(TIMING_WINDOW);
        self.total_samples = self.total_samples.saturating_add(1);
    }
    pub fn summary(&self) -> TimingSummary {
        if self.len == 0 {
            return TimingSummary::default();
        }
        let mut sorted = self.values[..self.len].to_vec();
        sorted.sort_by(f64::total_cmp);
        let percentile = |p: f64| sorted[((p * self.len as f64).ceil() as usize).saturating_sub(1)];
        TimingSummary {
            last_ms: self.values[(self.cursor + TIMING_WINDOW - 1) % TIMING_WINDOW],
            mean_ms: sorted.iter().sum::<f64>() / self.len as f64,
            p50_ms: percentile(0.5),
            p95_ms: percentile(0.95),
            max_ms: sorted[self.len - 1],
            window_samples: self.len,
            total_samples: self.total_samples,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStage {
    Streaming,
    BaseWorld,
    Ecology,
    Living,
    Economy,
    ControlledMotion,
    Persistence,
}

#[derive(Resource, Debug, Default)]
pub struct WorldDiagnostics {
    pub frame: TimingWindow,
    stages: BTreeMap<WorldStage, TimingWindow>,
}
#[derive(Debug, Clone, Serialize)]
pub struct WorldTimingReport {
    pub window_capacity: usize,
    pub frame_interval: TimingSummary,
    pub cpu_stages: BTreeMap<WorldStage, TimingSummary>,
}
impl WorldDiagnostics {
    pub fn record(&mut self, stage: WorldStage, milliseconds: f64) {
        self.stages
            .entry(stage)
            .or_default()
            .record_ms(milliseconds);
    }
    pub fn stage(&self, stage: WorldStage) -> TimingSummary {
        self.stages
            .get(&stage)
            .map(TimingWindow::summary)
            .unwrap_or_default()
    }
    pub fn report(&self) -> WorldTimingReport {
        WorldTimingReport {
            window_capacity: TIMING_WINDOW,
            frame_interval: self.frame.summary(),
            cpu_stages: self
                .stages
                .iter()
                .map(|(stage, times)| (*stage, times.summary()))
                .collect(),
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(crate) fn measure_frame(
    mut timings: ResMut<WorldDiagnostics>,
    mut previous: Local<Option<Instant>>,
) {
    let now = Instant::now();
    if let Some(before) = previous.replace(now) {
        timings
            .frame
            .record_ms(now.duration_since(before).as_secs_f64() * 1000.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_evidence_drops_startup_spikes_and_rejects_invalid_samples() {
        let mut window = TimingWindow::default();
        window.record_ms(5_000.);
        for _ in 0..TIMING_WINDOW {
            window.record_ms(4.);
        }
        for invalid in [f64::NAN, f64::INFINITY, -1.] {
            window.record_ms(invalid);
        }
        let report = window.summary();
        assert_eq!(report.window_samples, TIMING_WINDOW);
        assert_eq!(report.total_samples, (TIMING_WINDOW + 1) as u64);
        assert_eq!(report.p95_ms, 4.);
        assert_eq!(report.max_ms, 4.);
        assert!(serde_json::to_string(&report).is_ok());
    }
}
