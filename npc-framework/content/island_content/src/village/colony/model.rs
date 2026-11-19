//! Authoritative colony state. Stable names and byte encodings are save/replay contracts.
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct ColonyState {
    pub food_reserve_days: u32,
    pub gatherings: bool,
    pub social_events: u64,
    pub projects: BTreeMap<SimId, HousingProject>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct HousingProject {
    pub builder: SimId,
    pub ordered_at: u64,
}
impl Default for ColonyState {
    fn default() -> Self {
        Self {
            food_reserve_days: 2,
            gatherings: true,
            social_events: 0,
            projects: BTreeMap::new(),
        }
    }
}
impl SimHashResource for ColonyState {
    const NAME: &'static str = "island_colony_policy_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.food_reserve_days);
        h.u8(u8::from(self.gatherings));
        h.u64(self.social_events);
        h.u32(self.projects.len() as u32);
        for (plot, project) in &self.projects {
            h.u32(plot.0);
            h.u32(project.builder.0);
            h.u64(project.ordered_at);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Household {
    pub label: String,
    pub capacity: u32,
    pub residents: Vec<SimId>,
    pub door: SimPos,
    pub heated_until: u64,
    pub logs_burned: u64,
}
impl SimHash for Household {
    const NAME: &'static str = "island_household_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.label);
        h.u32(self.capacity);
        h.u32(self.residents.len() as u32);
        for resident in &self.residents {
            h.u32(resident.0);
        }
        self.door.sim_hash(h);
        h.u64(self.heated_until);
        h.u64(self.logs_burned);
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct SocialLife {
    /// 0..100000 satisfaction from health, provisions, a bed and companionship.
    pub morale: i32,
    pub affinity: BTreeMap<SimId, i32>,
    pub last_social_tick: u64,
    /// Day of the last finished meeting with each partner: a bond fades only
    /// after `economy.live.resident_neglect_after_days` without one.
    #[serde(default)]
    pub met_day: BTreeMap<SimId, u64>,
    /// Whom `life.socialise` last set out to meet; cleared when a meeting
    /// starts. Read by the inspector instead of a second search.
    #[serde(default)]
    pub seeking: Option<SimId>,
}
impl Default for SocialLife {
    fn default() -> Self {
        Self {
            morale: 75_000,
            affinity: BTreeMap::new(),
            last_social_tick: 0,
            met_day: BTreeMap::new(),
            seeking: None,
        }
    }
}
impl SimHash for SocialLife {
    const NAME: &'static str = "island_social_life_v2";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.morale);
        h.u64(self.last_social_tick);
        h.u32(self.affinity.len() as u32);
        for (other, affinity) in &self.affinity {
            h.u32(other.0);
            h.i32(*affinity);
        }
        h.u32(self.met_day.len() as u32);
        for (other, day) in &self.met_day {
            h.u32(other.0);
            h.u64(*day);
        }
        h.opt(self.seeking.as_ref(), |h, id| h.u32(id.0));
    }
}

/// A reserved visitor identity has not begun a life. It has no survival needs
/// until the recorded arrival; once joined it can never use this to reset life.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct AwaitingArrival {
    pub name: String,
    pub home: SimPos,
}
impl SimHash for AwaitingArrival {
    const NAME: &'static str = "island_awaiting_arrival_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.name);
        self.home.sim_hash(h);
    }
}
