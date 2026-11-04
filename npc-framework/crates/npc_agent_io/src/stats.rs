//! Per-controller latency statistics (spec §2.5): how long an agent took
//! from an observation to the action based on it, in ticks and wall-clock
//! microseconds. Tooling only: kept in a non-hashed resource, never read by
//! the sim (the tick latency also goes into the journal with each queued
//! action; wall-clock values never do).

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use bevy_ecs::prelude::*;
use serde::Serialize;

/// Samples kept per controller.
pub const MAX_SAMPLES: usize = 1024;

/// The host wall clock, for latency, deadlines, pacing and idle-supervisor timers only.
#[allow(clippy::disallowed_methods)]
// determinism: host tooling timing (stats, deadlines, pacing and idle nudges);
// never stored in sim state or the journal.
pub fn wall_now() -> Instant {
    Instant::now()
}

/// The latest refusal details are host-only and omitted from public statistics.
#[derive(Clone, Debug)]
pub struct LastRejection {
    pub unit: u32,
    pub tick: u64,
    pub verb: String,
    pub reason: String,
    pub message: String,
}

/// One controller's counters and samples.
#[derive(Clone, Debug, Default)]
pub struct ControllerStats {
    /// Authorized agent-tool requests, including notebook/map/event calls (host-only activity).
    pub requests: u64,
    pub unit: Option<u32>,
    pub observes: u64,
    pub acts: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub stale: u64,
    /// The latest observation: `(tick, when)`.
    pub last_observe: Option<(u64, Instant)>,
    pub last_rejection: Option<LastRejection>,
    pub latency_ticks: VecDeque<u64>,
    pub latency_us: VecDeque<u64>,
}

/// Every controller's stats, by controller id.
#[derive(Resource, Clone, Debug, Default)]
pub struct AgentStats {
    pub controllers: BTreeMap<String, ControllerStats>,
}

/// Public accepted-action labels for observer views. No arguments, notebook text, credentials,
/// private reasoning or handler-provided free-form labels are retained. Never simulation state.
#[derive(Clone, Debug)]
pub struct ActionNotice {
    pub seq: u64,
    pub epoch: u64,
    pub tick: u64,
    pub unit: u32,
    pub verb: String,
}
#[derive(Resource, Default)]
pub struct ActionNotices {
    pub entries: VecDeque<ActionNotice>,
    next_seq: u64,
}
impl ActionNotices {
    pub(crate) fn accepted(&mut self, epoch: u64, tick: u64, unit: u32, verb: &str) {
        if self
            .entries
            .back()
            .is_some_and(|notice| notice.epoch != epoch)
        {
            self.entries.clear();
        }
        self.next_seq = self.next_seq.saturating_add(1);
        self.entries.push_back(ActionNotice {
            seq: self.next_seq,
            epoch,
            tick,
            unit,
            verb: verb.chars().take(64).collect(),
        });
        while self.entries.len() > 256 {
            self.entries.pop_front();
        }
    }
}

/// `{n, min, p50, p95, max, mean}` of samples (integers; `null`s when empty).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub n: usize,
    pub min: Option<u64>,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub max: Option<u64>,
    pub mean: Option<u64>,
}

impl Summary {
    pub fn of(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut s = samples.to_vec();
        s.sort_unstable();
        let pick = |pct: usize| s[(s.len() - 1) * pct / 100];
        let sum: u128 = s.iter().map(|&v| u128::from(v)).sum();
        Self {
            n: s.len(),
            min: s.first().copied(),
            p50: Some(pick(50)),
            p95: Some(pick(95)),
            max: s.last().copied(),
            mean: u64::try_from(sum / s.len() as u128).ok(),
        }
    }
}

/// What `agent/stats` reports per controller.
#[derive(Clone, Debug, Serialize)]
pub struct StatsReport {
    pub controller: String,
    pub unit: Option<u32>,
    pub observes: u64,
    pub acts: u64,
    pub accepted: u64,
    pub rejected: u64,
    pub stale: u64,
    /// Observation → act, in ticks.
    pub latency_ticks: Summary,
    /// Observation → act, in wall-clock milliseconds (rounded down).
    pub latency_ms: Summary,
    /// Observation → act, in wall-clock microseconds.
    pub latency_us: Summary,
}

fn push(v: &mut VecDeque<u64>, x: u64) {
    if v.len() >= MAX_SAMPLES {
        v.pop_front();
    }
    v.push_back(x);
}

impl AgentStats {
    pub fn requested(&mut self, controller: &str, unit: u32) {
        let s = self.entry(controller);
        s.unit = Some(unit);
        s.requests = s.requests.saturating_add(1);
    }
    fn entry(&mut self, controller: &str) -> &mut ControllerStats {
        self.controllers.entry(controller.to_owned()).or_default()
    }

    /// Records an observation by `controller` of `unit` at `tick`.
    pub fn observed(&mut self, controller: &str, unit: u32, tick: u64) {
        let s = self.entry(controller);
        s.unit = Some(unit);
        s.observes += 1;
        s.last_observe = Some((tick, wall_now()));
    }

    /// Records an act at `tick`; returns its latency in ticks (since the
    /// latest observation), when there was one.
    pub fn acted(&mut self, controller: &str, unit: u32, tick: u64) -> Option<u64> {
        let s = self.entry(controller);
        s.unit = Some(unit);
        s.acts += 1;
        let (obs_tick, when) = s.last_observe?;
        let ticks = tick.saturating_sub(obs_tick);
        let us = u64::try_from(wall_now().duration_since(when).as_micros()).unwrap_or(u64::MAX);
        push(&mut s.latency_ticks, ticks);
        push(&mut s.latency_us, us);
        Some(ticks)
    }

    pub fn accepted(&mut self, controller: &str) {
        let s = self.entry(controller);
        s.accepted += 1;
        s.last_rejection = None;
    }

    pub fn rejected(&mut self, controller: &str, stale: bool) {
        let s = self.entry(controller);
        s.rejected += 1;
        s.stale += u64::from(stale);
    }

    /// Reports, by controller id (only `controller` when given).
    pub fn report(&self, controller: Option<&str>) -> Vec<StatsReport> {
        self.controllers
            .iter()
            .filter(|(id, _)| controller.is_none_or(|c| c == id.as_str()))
            .map(|(id, s)| {
                let ms: Vec<u64> = s.latency_us.iter().map(|us| us / 1000).collect();
                StatsReport {
                    controller: id.clone(),
                    unit: s.unit,
                    observes: s.observes,
                    acts: s.acts,
                    accepted: s.accepted,
                    rejected: s.rejected,
                    stale: s.stale,
                    latency_ticks: Summary::of(
                        &s.latency_ticks.iter().copied().collect::<Vec<_>>(),
                    ),
                    latency_ms: Summary::of(&ms),
                    latency_us: Summary::of(&s.latency_us.iter().copied().collect::<Vec<_>>()),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_is_integer() {
        let s = Summary::of(&[5, 1, 3, 9]);
        assert_eq!(
            (s.n, s.min, s.p50, s.max, s.mean),
            (4, Some(1), Some(3), Some(9), Some(4))
        );
        assert_eq!(Summary::of(&[]).n, 0);
    }
}
