//! Bounded world-clear progression, frozen per encounter. Retries never add rank.
use crate::catalog::{self as shared, ContentError};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GearScaling {
    pub damage_per_level: f32,
    pub protection_per_level: f32,
    pub vitality_per_level: f32,
    pub npc_damage_add_per_level: i32,
    pub npc_health_per_level: f32,
    pub price_per_level: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureFloor {
    pub start_rate: f32,
    pub rate_cap: f32,
    pub boss_health: i32,
    pub npc_boss_health: i32,
    pub wave_tier: usize,
    pub boss_time: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressionCatalog {
    pub max_rank: u32,
    pub threat_guard_rank: u32,
    pub player_level_cap: u32,
    pub base_lanes: u8,
    pub lane_level_step: u32,
    pub lanes_per_step: u8,
    pub max_lanes: u8,
    pub population_global_cap: usize,
    pub victory_player_xp: u64,
    pub population_base: usize,
    pub population_per_rank: usize,
    pub population_cap: usize,
    pub npc_arrival_factor: f32,
    pub batch_rank_step: u32,
    pub max_batch: u32,
    pub item_level_rank_step: u32,
    pub max_item_level: u32,
    pub health_per_rank: f32,
    pub damage_per_rank: f32,
    pub speed_per_rank: f32,
    pub arrival_per_rank: f32,
    pub reward_per_rank: f32,
    pub fortification_per_rank: f32,
    pub supply_per_rank: u32,
    pub max_supply_bonus: u32,
    pub pressure_floors: Vec<PressureFloor>,
    pub gear: GearScaling,
}
fn finite(v: f32, min: f32, max: f32) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
impl ProgressionCatalog {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        shared::parse_validated("progression", text, shared::MIB, Self::validate)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_PROGRESSION` override.
    pub fn try_load() -> Result<Self, ContentError> {
        shared::load(
            "progression",
            shared::MIB,
            include_str!("../progression.json"),
            Self::from_json_str,
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=10_000).contains(&self.threat_guard_rank)
            || self.max_rank > self.threat_guard_rank
            || !(1..=1000).contains(&self.player_level_cap)
            || self.base_lanes != 11
            || self.max_lanes < self.base_lanes
            || self.max_lanes > crate::sim::MAX_LANES
            || self.max_lanes.is_multiple_of(2)
            || !self.lanes_per_step.is_multiple_of(2)
            || self.lanes_per_step > 6
            || self.lane_level_step == 0
            || self.lane_level_step > 1000
            || self.population_global_cap > 224
            || self.population_global_cap < self.population_cap
            || self.victory_player_xp > 1_000_000
            || self.health_per_rank <= 0.0
            || self.damage_per_rank <= 0.0
            || !(1..=224).contains(&self.population_base)
            || self.population_per_rank > 16
            || !(self.population_base..=224).contains(&self.population_cap)
            || !finite(self.npc_arrival_factor, 0.5, 2.0)
            || !(1..=1000).contains(&self.max_rank)
            || !(1..=1000).contains(&self.batch_rank_step)
            || !(1..=8).contains(&self.max_batch)
            || !(1..=1000).contains(&self.item_level_rank_step)
            || !(1..=64).contains(&self.max_item_level)
            || self.supply_per_rank > 10
            || self.max_supply_bonus > 100
            || self.pressure_floors.is_empty()
            || self.pressure_floors.len() > 64
            || [
                self.health_per_rank,
                self.damage_per_rank,
                self.speed_per_rank,
                self.arrival_per_rank,
                self.reward_per_rank,
                self.fortification_per_rank,
            ]
            .into_iter()
            .any(|v| !finite(v, 0.0, 0.25))
            || [
                self.gear.damage_per_level,
                self.gear.protection_per_level,
                self.gear.vitality_per_level,
                self.gear.npc_health_per_level,
                self.gear.price_per_level,
            ]
            .into_iter()
            .any(|v| !finite(v, 0.0, 1.0))
            || !(0..=2).contains(&self.gear.npc_damage_add_per_level)
        {
            return Err("Invalid bounded progression configuration".into());
        }
        for f in &self.pressure_floors {
            if !finite(f.start_rate, 0.0, 10.0)
                || !finite(f.rate_cap, f.start_rate, 20.0)
                || !(0..=100_000).contains(&f.boss_health)
                || !(0..=1000).contains(&f.npc_boss_health)
                || f.wave_tier > 4
                || !finite(f.boss_time, 0.0, 600.0)
            {
                return Err("Invalid progression pressure floor".into());
            }
        }
        if self.pressure_floors.windows(2).any(|p| {
            p[1].start_rate < p[0].start_rate
                || p[1].rate_cap < p[0].rate_cap
                || p[1].boss_health < p[0].boss_health
                || p[1].npc_boss_health < p[0].npc_boss_health
                || p[1].wave_tier < p[0].wave_tier
                || p[1].boss_time < p[0].boss_time
        }) {
            return Err("Progression pressure floors must be monotone".into());
        }
        Ok(())
    }
    pub fn lanes_for_level(&self, level: u32) -> u8 {
        (u32::from(self.base_lanes)
            + (level.min(self.player_level_cap).saturating_sub(1) / self.lane_level_step)
                * u32::from(self.lanes_per_step))
        .min(u32::from(self.max_lanes)) as u8
    }
    pub fn population_for_lanes(&self, base: usize, lanes: u8) -> usize {
        base.saturating_mul(usize::from(lanes))
            .div_ceil(usize::from(self.base_lanes))
            .min(self.population_global_cap)
    }
    pub fn snapshot(&self, rank: u32) -> ProgressionSnapshot {
        let bounded = rank.min(self.max_rank);
        let n = bounded as f32;
        let threat = rank as f32;
        let floor = &self.pressure_floors[(rank as usize).min(self.pressure_floors.len() - 1)];
        ProgressionSnapshot {
            enemy_cap: (self.population_base + bounded as usize * self.population_per_rank)
                .min(self.population_cap),
            rank,
            batch_size: (1 + bounded / self.batch_rank_step).min(self.max_batch),
            item_level: (1 + rank / self.item_level_rank_step).min(self.max_item_level),
            enemy_health: 1.0 + threat * self.health_per_rank,
            enemy_damage: 1.0 + threat * self.damage_per_rank,
            enemy_speed: (1.0 + n * self.speed_per_rank).min(4.0),
            arrival_multiplier: (1.0 + n * self.arrival_per_rank).min(4.0),
            reward_multiplier: (1.0 + n * self.reward_per_rank).min(8.0),
            fortification_multiplier: (1.0 + n * self.fortification_per_rank).min(8.0),
            supply_bonus: bounded
                .saturating_mul(self.supply_per_rank)
                .min(self.max_supply_bonus),
            start_rate_floor: floor.start_rate,
            rate_cap_floor: floor.rate_cap,
            boss_health_floor: floor.boss_health,
            npc_boss_health_floor: floor.npc_boss_health,
            wave_tier: floor.wave_tier,
            boss_time_floor: floor.boss_time,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProgressionSnapshot {
    pub enemy_cap: usize,
    pub rank: u32,
    pub batch_size: u32,
    pub item_level: u32,
    pub enemy_health: f32,
    pub enemy_damage: f32,
    pub enemy_speed: f32,
    pub arrival_multiplier: f32,
    pub reward_multiplier: f32,
    pub fortification_multiplier: f32,
    pub supply_bonus: u32,
    pub start_rate_floor: f32,
    pub rate_cap_floor: f32,
    pub boss_time_floor: f32,
    pub boss_health_floor: i32,
    pub npc_boss_health_floor: i32,
    pub wave_tier: usize,
}
impl Default for ProgressionSnapshot {
    fn default() -> Self {
        Self {
            enemy_cap: 0,
            rank: 0,
            batch_size: 1,
            item_level: 1,
            enemy_health: 1.0,
            enemy_damage: 1.0,
            enemy_speed: 1.0,
            arrival_multiplier: 1.0,
            reward_multiplier: 1.0,
            fortification_multiplier: 1.0,
            supply_bonus: 0,
            start_rate_floor: 0.0,
            rate_cap_floor: 0.0,
            boss_time_floor: 0.0,
            boss_health_floor: 0,
            npc_boss_health_floor: 0,
            wave_tier: 0,
        }
    }
}
impl ProgressionSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.enemy_cap > 224
            || (self.enemy_cap == 0 && self.rank != 0)
            || self.rank > 10_000
            || !finite(self.boss_time_floor, 0.0, 600.0)
            || self.batch_size > self.rank.saturating_add(1)
            || self.item_level > self.rank.saturating_add(1)
            || !(1..=8).contains(&self.batch_size)
            || !(1..=64).contains(&self.item_level)
            || self.supply_bonus > 100
            || self.wave_tier > 4
            || !(0..=100_000).contains(&self.boss_health_floor)
            || !(0..=1000).contains(&self.npc_boss_health_floor)
            || !finite(self.start_rate_floor, 0.0, 10.0)
            || !finite(self.rate_cap_floor, self.start_rate_floor, 20.0)
            || !finite(self.arrival_multiplier, 0.5, 8.0)
            || !finite(self.enemy_health, 1.0, 4096.0)
            || !finite(self.enemy_damage, 1.0, 4096.0)
            || [
                self.enemy_speed,
                self.reward_multiplier,
                self.fortification_multiplier,
            ]
            .into_iter()
            .any(|v| !finite(v, 1.0, 8.0))
        {
            return Err("Invalid saved progression snapshot".into());
        }
        Ok(())
    }
}
static VALUE: OnceLock<ProgressionCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static ProgressionCatalog, ContentError> {
    shared::install(&VALUE, ProgressionCatalog::try_load)
}
pub fn catalog() -> &'static ProgressionCatalog {
    shared::lazy(&VALUE, "progression", ProgressionCatalog::try_load, || {
        ProgressionCatalog::from_json_str(include_str!("../progression.json"))
    })
}
pub fn level_one() -> u32 {
    1
}
pub fn scale_item(
    mut item: crate::content::ItemDefinition,
    level: u32,
) -> crate::content::ItemDefinition {
    let delta = level.saturating_sub(1).min(63);
    let n = delta as f32;
    let gear = &catalog().gear;
    let scale = |value: i32, per: f32| (value as f32 * (1.0 + n * per).min(8.0)).round() as i32;
    item.personal.damage = scale(item.personal.damage, gear.damage_per_level);
    item.personal.melee_damage = scale(item.personal.melee_damage, gear.damage_per_level);
    item.personal.armor = scale(item.personal.armor, gear.protection_per_level);
    item.personal.health = scale(item.personal.health, gear.vitality_per_level);
    item.personal.stamina = scale(item.personal.stamina, gear.vitality_per_level);
    item.npc.armor = scale(item.npc.armor, gear.protection_per_level);
    if matches!(
        item.slot,
        crate::content::Slot::Ranged | crate::content::Slot::Melee
    ) {
        item.npc.damage_add =
            (item.npc.damage_add + delta as i32 * gear.npc_damage_add_per_level).clamp(-30, 100);
    }
    item
}
pub fn price(base: u64, level: u32) -> u64 {
    (base as f64
        * (1.0
            + f64::from(level.saturating_sub(1).min(63))
                * f64::from(catalog().gear.price_per_level)))
    .round() as u64
}
