//! Bounded, half-second diagnostic history. A sample stores existing rolling statistics,
//! not a fabricated timeline of individual calls.
use crate::{PerfDomain, PerfInterval, PerfTreeReport, TraceId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const HISTORY_CAPACITY: usize = 240;
pub const HISTORY_PERIOD_SECS: f64 = 0.5;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistorySelection {
    Domain {
        domain: PerfDomain,
    },
    Group {
        domain: PerfDomain,
        mechanic: String,
    },
    Trace {
        id: TraceId,
    },
    Budget {
        mechanic: String,
    },
}
impl Default for HistorySelection {
    fn default() -> Self {
        Self::Domain {
            domain: PerfDomain::Simulation,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryBudget {
    pub mechanic: String,
    pub p95_us: Option<f64>,
    pub limit_us: u64,
    pub samples: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistorySample {
    pub time_s: f64,
    pub frame_ms: Option<f64>,
    pub frame_peak_ms: Option<f64>,
    pub frame_target_ms: Option<f64>,
    pub sim_tps: Option<f64>,
    pub sim_target_tps: Option<f64>,
    pub tick_p95_us: Option<f64>,
    pub domains: Vec<(PerfDomain, f64)>,
    pub groups: Vec<(PerfDomain, String, f64)>,
    pub traces: Vec<(TraceId, f64)>,
    pub budgets: Vec<HistoryBudget>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistoryCapture {
    pub generation: u64,
    pub interval: PerfInterval,
    pub samples: VecDeque<HistorySample>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesRole {
    Actual,
    Peak,
    Target,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistorySeries {
    pub name: String,
    pub role: SeriesRole,
    pub values: Vec<Option<f64>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryChart {
    pub title: String,
    pub unit: String,
    pub series: Vec<HistorySeries>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfHistoryReport {
    pub generation: u64,
    pub period_s: f64,
    pub capacity: usize,
    pub times_s: Vec<f64>,
    pub charts: Vec<HistoryChart>,
}

#[derive(Debug, Default)]
pub(crate) struct HistoryStore {
    capture: HistoryCapture,
    origin: Option<f64>,
    last: Option<f64>,
}
impl HistoryStore {
    pub(crate) fn record(&mut self, time: f64, peak: Option<f64>, tree: &PerfTreeReport) {
        if !time.is_finite() || time < 0.0 {
            return;
        }
        if self.capture.generation != tree.generation
            || self.capture.interval != tree.interval
            || self.last.is_some_and(|old| time < old)
        {
            self.clear();
            self.capture.generation = tree.generation;
            self.capture.interval = tree.interval;
        }
        if self
            .last
            .is_some_and(|old| time - old < HISTORY_PERIOD_SECS * 0.98)
        {
            return;
        }
        let origin = *self.origin.get_or_insert(time);
        let previous = self.capture.samples.back();
        let budgets = tree
            .budgets
            .iter()
            .map(|b| HistoryBudget {
                mechanic: b.mechanic.clone(),
                limit_us: b.budget_us,
                samples: b.samples,
                p95_us: if b.ready
                    && previous.is_none_or(|p| {
                        p.budgets
                            .iter()
                            .find(|old| old.mechanic == b.mechanic)
                            .is_none_or(|old| old.samples != b.samples)
                    }) {
                    b.observed_p95_us
                } else {
                    None
                },
            })
            .collect();
        let runtime = tree.runtime.as_ref();
        self.capture.samples.push_back(HistorySample {
            time_s: time - origin,
            frame_ms: runtime.map(|r| r.frame_ms),
            frame_peak_ms: peak,
            frame_target_ms: runtime
                .filter(|r| r.frame_target_us > 0)
                .map(|r| r.frame_target_us as f64 / 1000.0),
            sim_tps: runtime.and_then(|r| r.sim_tps),
            sim_target_tps: runtime.and_then(|r| r.sim_target_tps),
            tick_p95_us: tree.tick.as_ref().map(|t| t.p95_us),
            domains: tree
                .domains
                .iter()
                .map(|d| (d.domain, d.work.mean_us))
                .collect(),
            groups: tree
                .groups
                .iter()
                .map(|g| (g.domain, g.mechanic.clone(), g.work.mean_us))
                .collect(),
            traces: tree.nodes.iter().map(|n| (n.id, n.self_mean_us)).collect(),
            budgets,
        });
        self.last = Some(time);
        while self.capture.samples.len() > HISTORY_CAPACITY
            || self.capture.samples.front().is_some_and(|s| {
                time - origin - s.time_s > HISTORY_CAPACITY as f64 * HISTORY_PERIOD_SECS
            })
        {
            self.capture.samples.pop_front();
        }
    }
    pub(crate) fn capture(&self) -> HistoryCapture {
        self.capture.clone()
    }
    pub(crate) fn report(
        &self,
        selection: &HistorySelection,
        label: Option<&str>,
    ) -> PerfHistoryReport {
        self.capture.report(selection, label)
    }
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}
impl HistoryCapture {
    pub fn report(&self, selection: &HistorySelection, label: Option<&str>) -> PerfHistoryReport {
        let make = |name: &str, role: SeriesRole, field: fn(&HistorySample) -> Option<f64>| {
            HistorySeries {
                name: name.into(),
                role,
                values: self.samples.iter().map(field).collect(),
            }
        };
        let frame = HistoryChart {
            title: "Frame intervals: rolling mean and sampled peak".into(),
            unit: "ms".into(),
            series: vec![
                make("Mean", SeriesRole::Actual, |s| s.frame_ms),
                make("Peak", SeriesRole::Peak, |s| s.frame_peak_ms),
                make("Target", SeriesRole::Target, |s| s.frame_target_ms),
            ],
        };
        let sim = HistoryChart {
            title: "Simulation throughput (real time)".into(),
            unit: "ticks/s".into(),
            series: vec![
                make("Achieved", SeriesRole::Actual, |s| s.sim_tps),
                make("Target", SeriesRole::Target, |s| s.sim_target_tps),
            ],
        };
        let tick = HistoryChart {
            title: "Simulation tick wall time (rolling p95)".into(),
            unit: "us/tick".into(),
            series: vec![make("p95", SeriesRole::Actual, |s| s.tick_p95_us)],
        };
        let values = self
            .samples
            .iter()
            .map(|sample| match selection {
                HistorySelection::Domain { domain } => sample
                    .domains
                    .iter()
                    .find(|(d, _)| d == domain)
                    .map(|(_, v)| *v),
                HistorySelection::Group { domain, mechanic } => sample
                    .groups
                    .iter()
                    .find(|(d, m, _)| d == domain && m == mechanic)
                    .map(|(_, _, v)| *v),
                HistorySelection::Trace { id } => {
                    sample.traces.iter().find(|(i, _)| i == id).map(|(_, v)| *v)
                }
                HistorySelection::Budget { mechanic } => sample
                    .budgets
                    .iter()
                    .find(|b| &b.mechanic == mechanic)
                    .and_then(|b| b.p95_us),
            })
            .collect();
        let title = label.map(str::to_owned).unwrap_or_else(|| match selection {
            HistorySelection::Domain { domain } => domain.label().into(),
            HistorySelection::Group { mechanic, .. } => mechanic.clone(),
            HistorySelection::Trace { id } => format!("Trace #{id}"),
            HistorySelection::Budget { mechanic } => mechanic.clone(),
        });
        let budget = matches!(selection, HistorySelection::Budget { .. });
        let mut selected = HistoryChart {
            title: format!(
                "{}: {}",
                title,
                if budget {
                    "inclusive rolling p95"
                } else {
                    "rolling mean self time"
                }
            ),
            unit: format!("us/{}", self.interval.label()),
            series: vec![HistorySeries {
                name: if budget { "p95" } else { "Self" }.into(),
                role: SeriesRole::Actual,
                values,
            }],
        };
        if let HistorySelection::Budget { mechanic } = selection {
            selected.series.push(HistorySeries {
                name: "Budget".into(),
                role: SeriesRole::Target,
                values: self
                    .samples
                    .iter()
                    .map(|s| {
                        s.budgets
                            .iter()
                            .find(|b| &b.mechanic == mechanic)
                            .map(|b| b.limit_us as f64)
                    })
                    .collect(),
            });
        }
        PerfHistoryReport {
            generation: self.generation,
            period_s: HISTORY_PERIOD_SECS,
            capacity: HISTORY_CAPACITY,
            times_s: self.samples.iter().map(|s| s.time_s).collect(),
            charts: vec![frame, sim, selected, tick],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_bounded_resettable_and_selection_keeps_missing_data_as_gaps() {
        let c = crate::PerfCollector::new(16);
        c.record_for_test("npc_needs::decay", 50);
        c.end_frame();
        let mut tree = c.tree_report(None);
        tree.runtime = Some(crate::RuntimeStats {
            frame_ms: 16.7,
            frame_target_us: 16667,
            sim_tps: Some(20.0),
            sim_target_tps: Some(20.0),
            ..Default::default()
        });
        let mut store = HistoryStore::default();
        for i in 0..300 {
            store.record(i as f64 * 0.5, Some(30.0), &tree);
        }
        let capture = store.capture();
        assert_eq!(capture.samples.len(), HISTORY_CAPACITY);
        let report = capture.report(
            &HistorySelection::Trace {
                id: tree.nodes[0].id,
            },
            Some("decay"),
        );
        assert_eq!(report.charts[2].series[0].values[0], Some(50.0));
        assert_eq!(report.charts[0].series[1].values[0], Some(30.0));
        let missing = capture.report(&HistorySelection::Trace { id: 99 }, None);
        assert!(
            missing.charts[2].series[0]
                .values
                .iter()
                .all(Option::is_none)
        );
        store.record(500.0, None, &tree);
        assert_eq!(
            store.capture().samples.len(),
            1,
            "retention uses elapsed time as well as count"
        );
        tree.generation += 1;
        store.record(501.0, None, &tree);
        assert_eq!(store.capture().samples.len(), 1);
        store.clear();
        assert!(store.capture().samples.is_empty());
    }
    #[test]
    fn budget_history_keeps_limits_and_gaps_during_inactivity_and_warmup() {
        let c = crate::PerfCollector::new(16);
        let mut limits = crate::PerfBudgets::default();
        limits.set("needs", 55);
        c.set_budgets(limits);
        c.set_min_samples(2);
        c.record_for_test("npc_needs::decay", 70);
        c.end_frame();
        c.record_history(1.0, None);
        c.record_for_test("npc_needs::decay", 80);
        c.end_frame();
        c.record_history(1.5, None);
        c.end_frame();
        c.record_history(2.0, None);
        let report = c.history_report(
            &HistorySelection::Budget {
                mechanic: "needs".into(),
            },
            None,
        );
        assert_eq!(
            report.charts[2].series[0].values,
            vec![None, Some(80.0), None]
        );
        assert_eq!(report.charts[2].series[1].values, vec![Some(55.0); 3]);
        let frozen = c.capture_tree().history.unwrap();
        c.reset();
        assert_eq!(frozen.samples.len(), 3);
        assert!(
            c.history_report(&HistorySelection::default(), None)
                .times_s
                .is_empty()
        );
    }
}
