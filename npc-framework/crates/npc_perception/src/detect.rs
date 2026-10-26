//! The detection-check hook.
//!
//! Every candidate detection (a target in a vision cone with line of sight,
//! within proximity, or a heard noise) goes through the **active** check,
//! chosen by name. The built-in `"automatic"` check always detects with full
//! confidence. A rules plugin (spec §4.6, e.g. a Perception roll against a
//! target's stealth) registers its own function by name and makes it active;
//! `npc_perception` does not depend on it. The glue that wires the two
//! (testbed content, or a game) calls [`PerceptionAppExt`](crate::PerceptionAppExt).
//!
//! A check gets `&World` and must be deterministic (integers, counter RNG
//! keyed by `(seed, observer, tick, purpose)`, never iteration order).

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_kernel::SimId;

use crate::memory::{CONFIDENCE_FULL, SenseKind};

/// Name of the built-in check.
pub const AUTOMATIC: &str = "automatic";

/// One candidate detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detection {
    pub observer: Entity,
    pub observer_id: SimId,
    /// The sensed entity (`None` for a noise without a source).
    pub target: Option<Entity>,
    pub target_id: Option<SimId>,
    pub via: SenseKind,
    /// Squared distance observer → target (or noise) in mm².
    pub dist2: i64,
    /// The tick being run.
    pub tick: u64,
    /// For a heard noise: which of the tick's noises (`None` for vision and
    /// proximity).
    pub noise: Option<NoiseIndex>,
}

/// Which of a tick's noises a hearing detection is for. Noises are heard
/// sorted by [`Noise::sort_key`](crate::Noise::sort_key) (source first), so
/// both numbers are deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseIndex {
    /// Position in the tick's sorted noise list: unique within the tick.
    pub ordinal: u32,
    /// How many earlier noises in that list have the same source (or are
    /// sourceless too): 0 for a source's first noise of the tick.
    pub of_source: u32,
}

/// `Some(confidence permille)` when detected, `None` when not.
pub type DetectionCheckFn = fn(&World, &Detection) -> Option<u16>;

/// The built-in check: always detected, full confidence.
pub fn automatic(_world: &World, _detection: &Detection) -> Option<u16> {
    Some(CONFIDENCE_FULL)
}

/// Registered checks by name and the active one (default [`AUTOMATIC`]).
/// The names and the active choice are part of the content hash.
#[derive(Resource, Clone, Debug)]
pub struct DetectionChecks {
    pub checks: BTreeMap<&'static str, DetectionCheckFn>,
    pub active: &'static str,
}

impl Default for DetectionChecks {
    fn default() -> Self {
        Self {
            checks: BTreeMap::from([(AUTOMATIC, automatic as DetectionCheckFn)]),
            active: AUTOMATIC,
        }
    }
}

impl DetectionChecks {
    /// The active check.
    ///
    /// # Panics
    /// When the active name was never registered (also checked at freeze).
    pub fn active_fn(&self) -> DetectionCheckFn {
        *self.checks.get(self.active).unwrap_or_else(|| {
            panic!(
                "detection check `{}` is not registered (known: {:?})",
                self.active,
                self.checks.keys().collect::<Vec<_>>()
            )
        })
    }
}
