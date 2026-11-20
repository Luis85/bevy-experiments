//! Runtime-only AI scheduling. Physical motion and domain clocks are never skipped.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcologyScheduleConfig {
    pub near_radius_m: f64,
    pub far_interval_seconds: f64,
    pub max_far_decisions_per_pass: usize,
}

impl Default for EcologyScheduleConfig {
    fn default() -> Self {
        Self {
            near_radius_m: 160.,
            far_interval_seconds: 2.,
            max_far_decisions_per_pass: 16,
        }
    }
}

impl EcologyScheduleConfig {
    pub fn validate(&self) -> Result<(), SandboxError> {
        if !finite_range(self.near_radius_m, 20., 5000.)
            || !finite_range(self.far_interval_seconds, 0.25, 30.)
            || !(1..=MAX_LIVING_UNITS).contains(&self.max_far_decisions_per_pass)
        {
            return Err(SandboxError::Invalid(
                "ecology schedule configuration".into(),
            ));
        }
        Ok(())
    }
}

/// Latest behavior-pass work plus lifetime counters. These are measured jobs,
/// not elapsed CPU/GPU timings or inferred world population.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcologyScheduleStats {
    pub eligible: usize,
    pub near_decisions: usize,
    pub far_decisions: usize,
    pub evaluated: usize,
    pub deferred: usize,
    pub threat_candidates: usize,
    pub observer_candidates: usize,
    pub passes: u64,
    pub evaluated_total: u64,
    pub deferred_total: u64,
    pub threat_candidates_total: u64,
}

/// Transient host resource, never client authority or persistent population.
/// Reset when replacing a saved world, including loading the same clock value.
#[derive(Debug, Clone)]
pub struct EcologyScheduler {
    config: EcologyScheduleConfig,
    elapsed_seconds: f64,
    last_world_seconds: Option<f64>,
    last_evaluated: BTreeMap<u64, f64>,
    far_cursor: Option<u64>,
    stats: EcologyScheduleStats,
}

impl Default for EcologyScheduler {
    fn default() -> Self {
        Self::new(EcologyScheduleConfig::default()).expect("valid default ecology schedule")
    }
}

impl EcologyScheduler {
    pub fn new(config: EcologyScheduleConfig) -> Result<Self, SandboxError> {
        config.validate()?;
        Ok(Self {
            config,
            elapsed_seconds: 0.,
            last_world_seconds: None,
            last_evaluated: BTreeMap::new(),
            far_cursor: None,
            stats: EcologyScheduleStats::default(),
        })
    }

    pub fn config(&self) -> &EcologyScheduleConfig {
        &self.config
    }

    pub fn stats(&self) -> &EcologyScheduleStats {
        &self.stats
    }

    pub fn reset(&mut self) {
        self.elapsed_seconds = 0.;
        self.last_world_seconds = None;
        self.last_evaluated.clear();
        self.far_cursor = None;
        self.stats = EcologyScheduleStats::default();
    }

    pub(super) fn advance_clock(&mut self, real_dt: f64, world_seconds: f64) {
        if self.last_world_seconds.is_some_and(|t| world_seconds < t) {
            self.reset();
        }
        self.last_world_seconds = Some(world_seconds);
        self.elapsed_seconds += real_dt;
    }

    pub(super) fn select(
        &mut self,
        candidates: &[(u64, bool)],
        threat_candidates: usize,
        observer_candidates: usize,
    ) -> BTreeSet<u64> {
        let current: BTreeSet<_> = candidates.iter().map(|(id, _)| *id).collect();
        self.last_evaluated.retain(|id, _| current.contains(id));
        let mut selected = BTreeSet::new();
        let mut far = Vec::new();
        let mut near_count = 0;
        for &(id, near) in candidates {
            if near {
                selected.insert(id);
                self.last_evaluated.insert(id, self.elapsed_seconds);
                near_count += 1;
            } else if self.last_evaluated.get(&id).is_none_or(|last| {
                self.elapsed_seconds - last + 1e-9 >= self.config.far_interval_seconds
            }) {
                far.push(id);
            }
        }
        // Rotate due work by identity. Budget pressure cannot repeatedly prefer
        // low IDs and starve the tail of a growing population.
        far.sort_unstable();
        let split = self
            .far_cursor
            .map_or(0, |last| far.partition_point(|id| *id <= last));
        far.rotate_left(split);
        let mut far_count = 0;
        for id in far.into_iter().take(self.config.max_far_decisions_per_pass) {
            selected.insert(id);
            self.last_evaluated.insert(id, self.elapsed_seconds);
            self.far_cursor = Some(id);
            far_count += 1;
        }
        let deferred = candidates.len().saturating_sub(selected.len());
        self.stats = EcologyScheduleStats {
            eligible: candidates.len(),
            near_decisions: near_count,
            far_decisions: far_count,
            evaluated: selected.len(),
            deferred,
            threat_candidates,
            observer_candidates,
            passes: self.stats.passes.saturating_add(1),
            evaluated_total: self
                .stats
                .evaluated_total
                .saturating_add(selected.len() as u64),
            deferred_total: self.stats.deferred_total.saturating_add(deferred as u64),
            threat_candidates_total: self
                .stats
                .threat_candidates_total
                .saturating_add(threat_candidates as u64),
        };
        selected
    }
}
