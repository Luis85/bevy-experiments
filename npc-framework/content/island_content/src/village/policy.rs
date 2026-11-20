//! Immutable, JSON-authored life policy. Hosts freeze this with the world preset.
//! Safety guards and player orders stay ahead of the editable daily routine.
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum RoutineGoal {
    Thirst,
    Hunger,
    Recovery,
    Home,
    Work,
}

#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct IslandAiPolicy {
    pub decision_ticks: u64,
    pub critical_need: i32,
    pub thirst_threshold: i32,
    pub hunger_threshold: i32,
    pub recovery_threshold: i32,
    pub night_recovery_threshold: i32,
    pub routine: Vec<RoutineGoal>,
    pub goap: GoapPolicy,
}

impl Default for IslandAiPolicy {
    fn default() -> Self {
        Self {
            decision_ticks: 20,
            critical_need: 15_000,
            thirst_threshold: 55_000,
            hunger_threshold: 50_000,
            recovery_threshold: 30_000,
            night_recovery_threshold: 85_000,
            routine: vec![
                RoutineGoal::Thirst,
                RoutineGoal::Hunger,
                RoutineGoal::Recovery,
                RoutineGoal::Home,
                RoutineGoal::Work,
            ],
            goap: GoapPolicy::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct GoapPolicy {
    pub max_expansions: u32,
    pub max_steps: u32,
    /// Positive cost multipliers for physical action duration and travel.
    pub buy_cost: u32,
    pub gather_cost: u32,
    pub craft_cost: u32,
    /// Recipe levels a plan may chain below its goal (raw -> intermediate
    /// -> final is two).
    pub max_depth: u32,
    /// States one decision may search over all the goals it tries.
    pub decision_expansions: u32,
    /// Funded units over which the time a missing gathering tool saves is
    /// weighed against the trip to buy it.
    pub tool_horizon: u32,
    /// Plan for funded demand no maker is planned to meet (`life.pursue_goal
    /// elsewhere`): residents' purchases, settlements without a maker, the
    /// boat's order.
    pub serve_elsewhere: bool,
}

impl Default for GoapPolicy {
    fn default() -> Self {
        Self {
            max_expansions: 128,
            max_steps: 12,
            buy_cost: 1,
            gather_cost: 1,
            craft_cost: 1,
            max_depth: 4,
            decision_expansions: 512,
            tool_horizon: 8,
            serve_elsewhere: true,
        }
    }
}

impl IslandAiPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=300).contains(&self.decision_ticks) {
            return Err("island_ai.decision_ticks must be 1..=300".into());
        }
        if !(1..=40_000).contains(&self.critical_need)
            || [
                self.thirst_threshold,
                self.hunger_threshold,
                self.recovery_threshold,
            ]
            .iter()
            .any(|threshold| *threshold <= self.critical_need || *threshold > 90_000)
            || !(self.recovery_threshold..=100_000).contains(&self.night_recovery_threshold)
        {
            return Err("island_ai thresholds must preserve critical < routine <= 90000 and recovery <= night recovery <= 100000".into());
        }
        let expected = [
            RoutineGoal::Thirst,
            RoutineGoal::Hunger,
            RoutineGoal::Recovery,
            RoutineGoal::Home,
            RoutineGoal::Work,
        ];
        if self.routine.len() != expected.len()
            || self.routine.last() != Some(&RoutineGoal::Work)
            || expected
                .iter()
                .any(|goal| self.routine.iter().filter(|entry| *entry == goal).count() != 1)
        {
            return Err("island_ai.routine needs thirst, hunger, recovery, home and work exactly once; work must be last".into());
        }
        if self
            .routine
            .iter()
            .position(|goal| *goal == RoutineGoal::Recovery)
            > self
                .routine
                .iter()
                .position(|goal| *goal == RoutineGoal::Home)
        {
            return Err("island_ai.routine must put recovery before home so night-time shelter cannot suppress needed rest".into());
        }
        if !(8..=512).contains(&self.goap.max_expansions)
            || !(1..=24).contains(&self.goap.max_steps)
            || [
                self.goap.buy_cost,
                self.goap.gather_cost,
                self.goap.craft_cost,
            ]
            .iter()
            .any(|cost| !(1..=100).contains(cost))
        {
            return Err(
                "island_ai.goap requires 8..=512 expansions, 1..=24 steps and action costs 1..=100"
                    .into(),
            );
        }
        if !(1..=12).contains(&self.goap.max_depth)
            || !(self.goap.max_expansions..=4096).contains(&self.goap.decision_expansions)
            || !(1..=16).contains(&self.goap.tool_horizon)
        {
            return Err(
                "island_ai.goap requires max_depth 1..=12, decision_expansions from max_expansions to 4096 and tool_horizon 1..=16"
                    .into(),
            );
        }
        Ok(())
    }
}

impl IslandAiPolicy {
    /// The regional profile orders its branches in the resident tree file, so
    /// an authored `routine` would be silently ignored there. Refuse it.
    pub fn validate_regional(&self) -> Result<(), String> {
        if self.routine != Self::default().routine {
            return Err(
                "island_ai.routine is not used by the regional living world: order the branches                  in the resident tree (trees/resident.json or living_world.behavior.trees) and                  leave routine out"
                    .into(),
            );
        }
        Ok(())
    }
}

pub fn active(world: &World) -> Option<&IslandAiPolicy> {
    world.get_resource::<IslandAiPolicy>()
}

pub(super) fn register(app: &mut App) {
    let configured = app
        .world()
        .get_resource::<crate::world_runtime::ActiveWorld>()
        .and_then(|world| world.0.island_ai.clone());
    if let Some(configured) = configured {
        app.insert_resource(configured)
            .register_sim_resource::<IslandAiPolicy>()
            .register_sim_component::<super::routing::RouteRecovery>();
    }
}

impl SimHashResource for IslandAiPolicy {
    const NAME: &'static str = "island_ai_policy";
    fn sim_hash(&self, hash: &mut Fnv64) {
        hash.u64(self.decision_ticks);
        for value in [
            self.critical_need,
            self.thirst_threshold,
            self.hunger_threshold,
            self.recovery_threshold,
            self.night_recovery_threshold,
        ] {
            hash.i32(value);
        }
        hash.u32(self.routine.len() as u32);
        for goal in &self.routine {
            hash.u8(*goal as u8);
        }
        for value in [
            self.goap.max_expansions,
            self.goap.max_steps,
            self.goap.buy_cost,
            self.goap.gather_cost,
            self.goap.craft_cost,
            self.goap.max_depth,
            self.goap.decision_expansions,
            self.goap.tool_horizon,
        ] {
            hash.u32(value);
        }
        hash.u8(u8::from(self.goap.serve_elsewhere));
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_json_policy_is_usable_and_invalid_priority_trees_are_rejected() {
        let mut policy: IslandAiPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(policy, IslandAiPolicy::default());
        policy.validate().unwrap();
        policy.routine.swap(0, 1);
        policy.validate().unwrap();
        policy.routine.swap(0, 4);
        assert!(policy.validate().is_err());
        policy = IslandAiPolicy::default();
        policy.routine.swap(2, 3);
        assert!(
            policy
                .validate()
                .unwrap_err()
                .contains("recovery before home")
        );
        policy = IslandAiPolicy::default();
        policy.goap.max_expansions = u32::MAX;
        assert!(policy.validate().is_err());
        assert!(serde_json::from_str::<IslandAiPolicy>(r#"{"hunger_theshold":50000}"#).is_err());
    }
}
