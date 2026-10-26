//! The serde report and its text table.

use std::fmt::{self, Write as _};

use serde::{Deserialize, Serialize};

use crate::stats::{MechanicStats, SystemStats};
use crate::tags::UNTAGGED;

/// A snapshot of the collector. `systems` and `mechanics` are sorted by
/// whole-run total time descending (ties by name). `frames` counts `end_frame` calls
/// since the last reset.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PerfReport {
    pub systems: Vec<SystemStats>,
    pub mechanics: Vec<MechanicStats>,
    pub frames: u64,
}

impl PerfReport {
    /// The stats of the system named exactly `name`.
    pub fn system(&self, name: &str) -> Option<&SystemStats> {
        self.systems.iter().find(|s| s.name == name)
    }

    /// The stats of `mechanic`.
    pub fn mechanic(&self, mechanic: &str) -> Option<&MechanicStats> {
        self.mechanics.iter().find(|m| m.mechanic == mechanic)
    }

    /// The `n` systems with the highest whole-run total.
    pub fn top_systems(&self, n: usize) -> &[SystemStats] {
        &self.systems[..n.min(self.systems.len())]
    }

    /// The `n` mechanics with the highest whole-run total.
    pub fn top_mechanics(&self, n: usize) -> &[MechanicStats] {
        &self.mechanics[..n.min(self.mechanics.len())]
    }

    /// Mechanics whose rolling p95 is over budget.
    pub fn over_budget(&self) -> impl Iterator<Item = &MechanicStats> {
        self.mechanics.iter().filter(|m| m.over_budget)
    }

    /// Names of systems that no tag matched.
    pub fn untagged_systems(&self) -> Vec<&str> {
        self.systems
            .iter()
            .filter(|s| s.mechanic == UNTAGGED)
            .map(|s| s.name.as_str())
            .collect()
    }

    /// The report as a JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("PerfReport is always serializable")
    }

    /// A fixed-width text table with the top `top` mechanics and systems.
    /// Over-budget mechanics are marked with `!`. Times are µs (written `us`).
    pub fn to_table(&self, top: usize) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "frames: {}", self.frames);
        let _ = writeln!(
            out,
            "{:<24} {:>9} {:>9} {:>9} {:>11} {:>8} {:>8} {:>7}",
            "MECHANIC", "p50 us", "p95 us", "max us", "total us", "samples", "calls", "budget"
        );
        for m in self.top_mechanics(top) {
            let budget = m
                .budget_us
                .map_or_else(|| "-".to_owned(), |b| b.to_string());
            let flag = if m.over_budget { "!" } else { " " };
            let _ = writeln!(
                out,
                "{flag}{:<23} {:>9.1} {:>9.1} {:>9.1} {:>11.1} {:>8} {:>8} {:>7}",
                clip(&m.mechanic, 23),
                m.p50_us,
                m.p95_us,
                m.max_us,
                m.total_us,
                m.samples,
                m.calls,
                budget
            );
        }
        let _ = writeln!(
            out,
            "{:<48} {:<14} {:>9} {:>9} {:>9} {:>11} {:>8}",
            "SYSTEM", "mechanic", "p50 us", "p95 us", "max us", "total us", "calls"
        );
        for s in self.top_systems(top) {
            let _ = writeln!(
                out,
                "{:<48} {:<14} {:>9.1} {:>9.1} {:>9.1} {:>11.1} {:>8}",
                clip(&s.name, 48),
                clip(&s.mechanic, 14),
                s.p50_us,
                s.p95_us,
                s.max_us,
                s.total_us,
                s.calls
            );
        }
        out
    }
}

impl fmt::Display for PerfReport {
    /// [`PerfReport::to_table`] with the top 20 rows.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_table(20))
    }
}

/// Shortens `s` to at most `max` chars by keeping its tail (the most specific
/// part of a type path), prefixed with `..`.
pub fn clip(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        return s.to_owned();
    }
    let keep = max.saturating_sub(2);
    let tail: String = s.chars().skip(len - keep).collect();
    format!("..{tail}")
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn clip_keeps_tail() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("npc_sim::needs::decay", 10), "..s::decay");
    }
}
