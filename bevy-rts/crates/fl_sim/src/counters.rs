//! [`SimCounters`]: deterministic per-tick work counters (spec §12.6, §15; ADR-0010).
//!
//! Systems count the algorithmic work they do (entities processed, query candidates, spawns,
//! ...). The sim is deterministic, so the counts are exact, and the perf gate asserts budgets
//! on them. Observe-only: sim logic never reads them, and they are never hashed.

use std::collections::BTreeMap;

use bevy_ecs::resource::Resource;

/// The work done in the current tick, by counter name (`"<feature>.<what>"`). Adds commute,
/// so systems that only count need no order among themselves: the schedule allows their
/// ambiguities on this resource. Transient
/// `"core.counters"`: reset to empty at the start of every tick, in
/// [`InputSet::Housekeeping`](crate::InputSet::Housekeeping).
#[derive(Resource, Default, Debug)]
pub struct SimCounters {
    counts: BTreeMap<&'static str, u64>,
}

impl SimCounters {
    /// Add `n` to counter `name`.
    pub fn add(&mut self, name: &'static str, n: u64) {
        let count = self.counts.entry(name).or_default();
        *count = count.saturating_add(n);
    }

    /// The count of `name` this tick; 0 if nothing was counted.
    pub fn get(&self, name: &str) -> u64 {
        self.counts.get(name).copied().unwrap_or(0)
    }

    /// Set every counter back to zero: the Transient reset of `"core.counters"`.
    pub fn clear(&mut self) {
        self.counts.clear();
    }

    /// Every counter this tick, sorted by name.
    pub fn snapshot(&self) -> Vec<(&'static str, u64)> {
        self.counts.iter().map(|(&name, &n)| (name, n)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_add_up_and_snapshot_is_sorted() {
        let mut counters = SimCounters::default();
        counters.add("b.n", 2);
        counters.add("a.n", 1);
        counters.add("b.n", 3);
        assert_eq!(counters.get("b.n"), 5);
        assert_eq!(counters.get("missing"), 0);
        assert_eq!(counters.snapshot(), [("a.n", 1), ("b.n", 5)]);
    }
}
