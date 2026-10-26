//! Rolling windows, percentiles and the per-system / per-mechanic stat rows.

use serde::{Deserialize, Serialize};

/// Stats for one Bevy system (a `"system"` span), keyed by its name.
///
/// `calls` and `total_us` cover the whole run (since the last reset);
/// `p50_us`, `p95_us` and `max_us` cover the rolling window of the last
/// `window` calls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemStats {
    pub name: String,
    pub mechanic: String,
    pub calls: u64,
    pub total_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
}

/// Stats for one mechanic. A sample is the sum of the mechanic's time (all
/// its systems and `"mechanic"` spans) between two `end_frame` calls; frames
/// in which the mechanic did not run add no sample.
///
/// `samples`, `calls` and `total_us` cover the whole run; the percentiles
/// and `max_us` cover the rolling window of the last `window` samples.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MechanicStats {
    pub mechanic: String,
    pub samples: u64,
    /// Span entries (system runs plus mechanic spans) summed into the samples.
    pub calls: u64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
    pub total_us: f64,
    pub budget_us: Option<u64>,
    /// `p95_us > budget_us`, once the mechanic has at least the collector's
    /// `min_samples` samples (false before).
    pub over_budget: bool,
}

/// A fixed-capacity ring of nanosecond durations.
#[derive(Debug)]
pub(crate) struct Ring {
    buf: Vec<u64>,
    next: usize,
    cap: usize,
}

impl Ring {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            buf: Vec::new(),
            next: 0,
            cap: cap.max(1),
        }
    }

    pub(crate) fn push(&mut self, value: u64) {
        if self.buf.len() < self.cap {
            self.buf.push(value);
        } else {
            self.buf[self.next] = value;
        }
        self.next = (self.next + 1) % self.cap;
    }

    pub(crate) fn clear(&mut self) {
        self.buf.clear();
        self.next = 0;
    }

    /// `(p50, p95, max)` in ns, nearest-rank.
    #[cfg(test)]
    pub(crate) fn summary(&self) -> Summary {
        self.summary_with(&mut Vec::new())
    }

    /// [`Ring::summary`] in O(n) with a caller-owned scratch buffer
    /// (`report()` runs it for every system, so no per-row sort): select the
    /// median over the whole window, then the p95 within the upper half,
    /// then the max within the top 5 %. Exact (nearest rank), like a sort.
    pub(crate) fn summary_with(&self, scratch: &mut Vec<u64>) -> Summary {
        let n = self.buf.len();
        if n == 0 {
            return Summary::default();
        }
        scratch.clear();
        scratch.extend_from_slice(&self.buf);
        let (i50, i95) = (rank_index(n, 50), rank_index(n, 95));
        let (_, &mut p50, upper) = scratch.select_nth_unstable(i50);
        // `upper` holds ranks i50+1..n (i95 >= i50 always).
        let (p95, top) = match i95 - i50 {
            0 => (p50, upper),
            k => {
                let (_, &mut p95, top) = upper.select_nth_unstable(k - 1);
                (p95, top)
            }
        };
        let max = top.iter().copied().fold(p95, u64::max);
        Summary { p50, p95, max }
    }

    /// p95 in ns without a full sort (used every frame for budgets).
    pub(crate) fn p95(&self, scratch: &mut Vec<u64>) -> u64 {
        if self.buf.is_empty() {
            return 0;
        }
        scratch.clear();
        scratch.extend_from_slice(&self.buf);
        let idx = rank_index(scratch.len(), 95);
        *scratch.select_nth_unstable(idx).1
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Summary {
    pub(crate) p50: u64,
    pub(crate) p95: u64,
    pub(crate) max: u64,
}

/// 0-based index of the nearest-rank `pct` percentile in `n` sorted values.
fn rank_index(n: usize, pct: usize) -> usize {
    (pct * n).div_ceil(100).clamp(1, n) - 1
}

/// Nanoseconds to fractional microseconds.
pub(crate) fn us(ns: u64) -> f64 {
    ns as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_wraps_and_ranks() {
        let mut r = Ring::new(4);
        for v in 1..=6 {
            r.push(v);
        }
        let s = r.summary();
        assert_eq!((s.p50, s.p95, s.max), (4, 6, 6));
        assert_eq!(r.p95(&mut Vec::new()), 6);
        r.clear();
        assert_eq!(r.summary().max, 0);
    }

    #[test]
    fn summary_matches_sorted_nearest_rank() {
        // Uniform, skewed (one huge outlier), heavy-tailed and constant.
        let shapes: [fn(u64, usize) -> u64; 4] = [
            |x, _| x % 1000,
            |x, i| if i == 7 { u64::MAX / 3 } else { 200 + x % 300 },
            |x, _| (x % 1000).pow(3),
            |_, _| 42,
        ];
        for shape in shapes {
            for n in [1usize, 2, 3, 19, 20, 21, 64, 65, 100, 599, 600] {
                let mut r = Ring::new(600);
                let mut x: u64 = 88172645463325252;
                let mut all = Vec::new();
                for i in 0..n {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    let v = shape(x, i);
                    r.push(v);
                    all.push(v);
                }
                all.sort_unstable();
                let s = r.summary();
                assert_eq!(s.p50, all[rank_index(n, 50)], "p50 n={n}");
                assert_eq!(s.p95, all[rank_index(n, 95)], "p95 n={n}");
                assert_eq!(s.max, *all.last().unwrap(), "max n={n}");
            }
        }
    }

    #[test]
    fn rank_index_bounds() {
        assert_eq!(rank_index(1, 50), 0);
        assert_eq!(rank_index(100, 95), 94);
        assert_eq!(rank_index(100, 50), 49);
        assert_eq!(rank_index(2, 95), 1);
    }
}
