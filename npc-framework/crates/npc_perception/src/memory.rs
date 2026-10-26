//! What an agent perceived: [`PerceptionMemory`].

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash};
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Full confidence (permille).
pub const CONFIDENCE_FULL: u16 = 1000;

/// The sense a percept came through. When several sense the same target in
/// one scan, vision wins, then camera, radar, proximity and reach (hearing
/// never replaces a sensed percept). `Reach`, `Camera` and `Radar` were
/// appended, so the existing variants keep their hash tags.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum SenseKind {
    Vision,
    Proximity,
    Hearing,
    /// The opt-in [`ReachSense`](crate::ReachSense): all round, with line of sight.
    Reach,
    /// A robot's [`Camera`](crate::Camera) (plan Task 12).
    Camera,
    /// A robot's [`Radar`](crate::Radar) (plan Task 12).
    Radar,
}

impl SenseKind {
    /// Lower-case name (journal `Perceived.via`, inspect).
    pub const fn name(self) -> &'static str {
        match self {
            SenseKind::Vision => "vision",
            SenseKind::Proximity => "proximity",
            SenseKind::Hearing => "hearing",
            SenseKind::Reach => "reach",
            SenseKind::Camera => "camera",
            SenseKind::Radar => "radar",
        }
    }

    const fn tag(self) -> u8 {
        match self {
            SenseKind::Vision => 0,
            SenseKind::Proximity => 1,
            SenseKind::Hearing => 2,
            SenseKind::Reach => 3,
            SenseKind::Camera => 4,
            SenseKind::Radar => 5,
        }
    }
}

/// One remembered thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Percept {
    /// Last tick it was sensed (by any sense).
    pub last_seen_tick: u64,
    /// Where it was then (a noise's position for hearing).
    pub pos: SimPos,
    /// The sense of that last observation.
    pub via: SenseKind,
    /// Confidence of that observation, permille (from the detection check).
    pub confidence: u16,
    /// Its `Perceivable` kinds when observed (0 if it had none, e.g. heard).
    pub kinds: u32,
    /// Sensed by vision, camera, radar, proximity or reach at the agent's
    /// latest update.
    pub sensed: bool,
}

impl Percept {
    /// `confidence` decayed linearly to 0 at `forget_after_ticks` of age.
    pub fn confidence_at(&self, now: u64, forget_after_ticks: u64) -> u16 {
        let age = now.saturating_sub(self.last_seen_tick);
        if forget_after_ticks == 0 || age >= forget_after_ticks {
            return if age == 0 { self.confidence } else { 0 };
        }
        let left = u128::from(forget_after_ticks - age);
        (u128::from(self.confidence) * left / u128::from(forget_after_ticks)) as u16
    }
}

/// The latest noise an agent heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct HeardNoise {
    pub tick: u64,
    pub pos: SimPos,
    pub source: Option<SimId>,
    pub kind: u32,
}

/// Bounded memory of percepts, by `SimId` (at most
/// `PerceptionConfig::memory_capacity`; the oldest are evicted first), plus
/// the latest heard noise. Forgets a percept `forget_after_ticks` after it
/// was last sensed. Written only by the perception system.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct PerceptionMemory {
    pub percepts: BTreeMap<SimId, Percept>,
    pub last_noise: Option<HeardNoise>,
}

impl PerceptionMemory {
    pub fn get(&self, id: SimId) -> Option<&Percept> {
        self.percepts.get(&id)
    }

    /// Percepts currently sensed (vision, proximity or reach), ascending `SimId`.
    pub fn sensed(&self) -> impl Iterator<Item = (SimId, &Percept)> + '_ {
        self.percepts
            .iter()
            .filter(|(_, p)| p.sensed)
            .map(|(&id, p)| (id, p))
    }

    /// Whether a currently sensed percept has a kind in `mask`.
    pub fn senses_kind(&self, mask: u32) -> bool {
        self.sensed().any(|(_, p)| p.kinds & mask != 0)
    }

    /// Whether any remembered percept has a kind in `mask`.
    pub fn remembers_kind(&self, mask: u32) -> bool {
        self.percepts.values().any(|p| p.kinds & mask != 0)
    }

    /// The remembered percept with a kind in `mask` closest to `from`
    /// (ties: lower `SimId`); only currently sensed ones if `sensed_only`.
    pub fn nearest(&self, from: SimPos, mask: u32, sensed_only: bool) -> Option<(SimId, Percept)> {
        self.percepts
            .iter()
            .filter(|(_, p)| p.kinds & mask != 0 && (p.sensed || !sensed_only))
            .min_by_key(|(id, p)| (from.dist2(p.pos), **id))
            .map(|(&id, &p)| (id, p))
    }

    /// Inspect JSON: `{"percepts":[{"target":..,..}],"last_noise":..}`.
    pub fn to_json(&self) -> Value {
        let percepts: Vec<Value> = self
            .percepts
            .iter()
            .map(|(id, p)| {
                json!({
                    "target": id,
                    "last_seen_tick": p.last_seen_tick,
                    "pos": p.pos,
                    "via": p.via.name(),
                    "confidence": p.confidence,
                    "kinds": p.kinds,
                    "sensed": p.sensed,
                })
            })
            .collect();
        json!({ "percepts": percepts, "last_noise": self.last_noise })
    }
}

/// Facts the perception system writes to the agent's `Blackboard` at each of
/// its updates: the nearest **sensed** percept with a kind in `kinds` as
/// `target_slot = U32(sim_id)` and `pos_slot = Pos(x, z)`, or `None` in both
/// when there is none. Slots are `Blackboard` indices (0..8).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct PerceptionFacts {
    pub kinds: u32,
    pub target_slot: u8,
    pub pos_slot: u8,
}

impl SimHash for PerceptionMemory {
    const NAME: &'static str = "perception_memory";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.percepts.len()).expect("too many percepts"));
        for (id, p) in &self.percepts {
            h.u32(id.0);
            h.u64(p.last_seen_tick);
            p.pos.sim_hash(h);
            h.u8(p.via.tag());
            h.u16(p.confidence);
            h.u32(p.kinds);
            h.u8(u8::from(p.sensed));
        }
        h.opt(self.last_noise.as_ref(), |h, n| {
            h.u64(n.tick);
            n.pos.sim_hash(h);
            h.opt(n.source.as_ref(), |h, s| h.u32(s.0));
            h.u32(n.kind);
        });
    }
}

impl SimHash for PerceptionFacts {
    const NAME: &'static str = "perception_facts";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.kinds);
        h.u8(self.target_slot);
        h.u8(self.pos_slot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_decays_linearly() {
        let p = Percept {
            last_seen_tick: 100,
            pos: SimPos::default(),
            via: SenseKind::Vision,
            confidence: 1000,
            kinds: 1,
            sensed: false,
        };
        assert_eq!(p.confidence_at(100, 200), 1000);
        assert_eq!(p.confidence_at(200, 200), 500);
        assert_eq!(p.confidence_at(300, 200), 0);
        assert_eq!(p.confidence_at(100, 0), 1000);
    }
}
