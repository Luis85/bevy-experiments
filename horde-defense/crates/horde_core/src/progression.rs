//! Bounded world-clear progression, frozen per encounter. Retries never add rank.
use crate::catalog::{self as shared, ContentError};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
/// The highest composition wave tier a pressure floor or a saved snapshot may
/// name (inclusive). The authored maximum is 11; the headroom is validation
/// only, so widening it never touches a save.
pub const MAX_WAVE_TIER: usize = 15;
/// The most regular enemies a production encounter keeps alive (one boss may
/// come on top). Saves and catalogues above it are rejected.
pub const GAMEPLAY_ENEMY_CAP: usize = 224;
/// Which boss a stage fields, derived from its place on the country's front
/// (schema v7 §3.10). Append-only.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum BossRank {
    #[default]
    Sector,
    Regional,
    Capstone,
}
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
    /// Level-up grant, rank milestones and camp prices. Absent means the
    /// neutral block: no grant, no milestones, flat camp prices.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub rewards: ProgressionRewardCatalog,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProgressionRewardCatalog {
    /// Level-up coin grant as a fraction (0..=1) of the XP needed to reach
    /// the new level. 0 pays nothing.
    pub level_grant_xp_fraction: f32,
    /// Rank milestones, ranks strictly increasing; at most
    /// [`MAX_RECEIPT_MILESTONES`](crate::sim::MAX_RECEIPT_MILESTONES), so one
    /// settlement receipt can hold them all.
    pub milestones: Vec<MilestoneDefinition>,
    pub camp_prices: CampPrices,
}
impl Default for ProgressionRewardCatalog {
    fn default() -> Self {
        Self {
            level_grant_xp_fraction: 0.0,
            milestones: Vec::new(),
            camp_prices: CampPrices::default(),
        }
    }
}
/// One rank milestone: coins and an item of a fixed authored quality, paid
/// once when the campaign's progression rank reaches `rank`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MilestoneDefinition {
    pub rank: u32,
    pub coins: u64,
    pub item_quality: crate::quality::Quality,
}
/// Threat-scaled camp prices: `base × min(1 + per_rank × rank, max_multiplier)`
/// rounded to 5 (see [`camp_price`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CampPrices {
    /// Base price of one NPC training point (the legacy flat 25).
    pub npc_training_base: u64,
    /// Price growth per threat rank (0..=1). 0 keeps prices flat.
    pub per_rank: f32,
    /// Cap on the price multiplier (1..=10).
    pub max_multiplier: f32,
}
impl Default for CampPrices {
    fn default() -> Self {
        Self {
            npc_training_base: 25,
            per_rank: 0.0,
            max_multiplier: 1.0,
        }
    }
}
impl ProgressionRewardCatalog {
    pub fn validate(&self) -> Result<(), String> {
        let prices = &self.camp_prices;
        if !finite(self.level_grant_xp_fraction, 0.0, 1.0)
            || self.milestones.len() > crate::sim::MAX_RECEIPT_MILESTONES
            || self
                .milestones
                .iter()
                .any(|m| !(1..=10_000).contains(&m.rank) || m.coins > 1_000_000)
            || self.milestones.windows(2).any(|m| m[1].rank <= m[0].rank)
            || !(1..=1_000_000).contains(&prices.npc_training_base)
            || !finite(prices.per_rank, 0.0, 1.0)
            || !finite(prices.max_multiplier, 1.0, 10.0)
        {
            return Err("Invalid progression rewards".into());
        }
        Ok(())
    }
    /// Coins granted for reaching `level` (0 at level 1 and below):
    /// `round(level_grant_xp_fraction × xp_to_advance_from(level - 1))`.
    pub fn level_grant(&self, level: u32) -> u64 {
        if level < 2 {
            return 0;
        }
        (f64::from(self.level_grant_xp_fraction) * xp_to_advance_from(level - 1) as f64).round()
            as u64
    }
    /// `base` scaled by threat `rank`, rounded to the nearest 5. A multiplier
    /// of 1 (the neutral block, or rank 0) returns `base` unchanged.
    pub fn camp_price(&self, base: u64, rank: u32) -> u64 {
        let prices = &self.camp_prices;
        let multiplier = (1.0 + f64::from(prices.per_rank) * f64::from(rank))
            .min(f64::from(prices.max_multiplier));
        if multiplier <= 1.0 {
            return base;
        }
        ((base as f64 * multiplier / 5.0).round() as u64).saturating_mul(5)
    }
}
/// XP needed to advance from `level` to the next level (the player's curve).
pub fn xp_to_advance_from(level: u32) -> u64 {
    (20.0 + 15.0 * (level.saturating_sub(1) as f64).powf(1.3)).round() as u64
}
/// [`ProgressionRewardCatalog::level_grant`] over the installed catalogue.
pub fn level_grant(level: u32) -> u64 {
    catalog().rewards.level_grant(level)
}
/// [`ProgressionRewardCatalog::camp_price`] over the installed catalogue.
pub fn camp_price(base: u64, rank: u32) -> u64 {
    catalog().rewards.camp_price(base, rank)
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
            || self.population_global_cap > GAMEPLAY_ENEMY_CAP
            || self.population_global_cap < self.population_cap
            || self.victory_player_xp > 1_000_000
            || self.health_per_rank <= 0.0
            || self.damage_per_rank <= 0.0
            || !(1..=GAMEPLAY_ENEMY_CAP).contains(&self.population_base)
            || self.population_per_rank > 16
            || !(self.population_base..=GAMEPLAY_ENEMY_CAP).contains(&self.population_cap)
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
                || f.wave_tier > MAX_WAVE_TIER
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
        self.rewards.validate()
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
            // The enemy-side fields stay neutral until their wave-2 writers.
            ..ProgressionSnapshot::default()
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
    /// Elite spawn chance in basis points (0..=10000).
    #[serde(skip_serializing_if = "crate::schema::is_default")]
    pub elite_rate_bp: u32,
    /// The front-depth ramp, already folded into `enemy_health` and
    /// `arrival_multiplier`; kept for display and audit.
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub front_ramp: f32,
    #[serde(skip_serializing_if = "crate::schema::is_default")]
    pub boss_rank: BossRank,
    /// Boss phase transitions this encounter's boss may enter (0..=2).
    #[serde(skip_serializing_if = "crate::schema::is_default")]
    pub boss_phases: u8,
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub boss_armor_multiplier: f32,
    #[serde(skip_serializing_if = "crate::schema::is_default")]
    pub npc_boss_armor_add: i32,
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub boss_health_multiplier: f32,
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub boss_reward_multiplier: f32,
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub boss_damage_multiplier: f32,
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
            elite_rate_bp: 0,
            front_ramp: 1.0,
            boss_rank: BossRank::Sector,
            boss_phases: 0,
            boss_armor_multiplier: 1.0,
            npc_boss_armor_add: 0,
            boss_health_multiplier: 1.0,
            boss_reward_multiplier: 1.0,
            boss_damage_multiplier: 1.0,
        }
    }
}
impl ProgressionSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.enemy_cap > GAMEPLAY_ENEMY_CAP {
            return Err(format!(
                "Saved enemy cap {} exceeds the gameplay enemy cap {GAMEPLAY_ENEMY_CAP}",
                self.enemy_cap
            ));
        }
        if (self.enemy_cap == 0 && self.rank != 0)
            || self.rank > 10_000
            || !finite(self.boss_time_floor, 0.0, 600.0)
            || self.batch_size > self.rank.saturating_add(1)
            || self.item_level > self.rank.saturating_add(1)
            || !(1..=8).contains(&self.batch_size)
            || !(1..=64).contains(&self.item_level)
            || self.supply_bonus > 100
            || self.wave_tier > MAX_WAVE_TIER
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
            || self.elite_rate_bp > 10_000
            || (self.rank < 3 && self.elite_rate_bp != 0)
            || !finite(self.front_ramp, 1.0, 2.0)
            || self.boss_phases > 2
            || !finite(self.boss_armor_multiplier, 1.0, 4.0)
            || !(0..=20).contains(&self.npc_boss_armor_add)
            || !finite(self.boss_health_multiplier, 1.0, 2.0)
            || !finite(self.boss_reward_multiplier, 1.0, 2.0)
            || !finite(self.boss_damage_multiplier, 1.0, 4.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::Quality;

    fn milestone(rank: u32, coins: u64, item_quality: Quality) -> MilestoneDefinition {
        MilestoneDefinition {
            rank,
            coins,
            item_quality,
        }
    }

    /// The design's authored values (wave 2 ships them in progression.json).
    fn authored() -> ProgressionRewardCatalog {
        ProgressionRewardCatalog {
            level_grant_xp_fraction: 0.25,
            milestones: vec![
                milestone(5, 500, Quality::Uncommon),
                milestone(10, 1000, Quality::Uncommon),
                milestone(15, 1500, Quality::Rare),
                milestone(20, 2000, Quality::Rare),
            ],
            camp_prices: CampPrices {
                npc_training_base: 25,
                per_rank: 0.10,
                max_multiplier: 6.0,
            },
        }
    }

    fn with_rewards(rewards: ProgressionRewardCatalog) -> ProgressionCatalog {
        ProgressionCatalog {
            rewards,
            ..catalog().clone()
        }
    }

    #[test]
    fn progression_rewards_reject_unsorted_milestones() {
        // The shipped catalogue has no rewards block: every reward is neutral
        // and nothing new is written back.
        assert_eq!(catalog().rewards, ProgressionRewardCatalog::default());
        let shipped = serde_json::to_string(catalog()).unwrap();
        assert!(!shipped.contains("rewards"), "skipped at default");
        // The authored block validates and round-trips.
        let catalog = with_rewards(authored());
        catalog.validate().unwrap();
        let text = serde_json::to_string(&catalog).unwrap();
        assert!(
            text.contains(r#""milestones":[{"rank":5,"coins":500,"item_quality":"Uncommon"},"#)
        );
        assert_eq!(ProgressionCatalog::from_json(&text).unwrap(), catalog);
        let edit = |change: &dyn Fn(&mut ProgressionRewardCatalog)| {
            let mut rewards = authored();
            change(&mut rewards);
            with_rewards(rewards).validate()
        };
        type Edit<'a> = &'a dyn Fn(&mut ProgressionRewardCatalog);
        let rows: [(&str, Edit); 17] = [
            ("unsorted", &|r| r.milestones.swap(1, 2)),
            ("duplicate rank", &|r| r.milestones[1].rank = 5),
            ("five milestones", &|r| {
                r.milestones.push(milestone(25, 1, Quality::Rare))
            }),
            ("rank 0", &|r| r.milestones[0].rank = 0),
            ("rank above 10000", &|r| r.milestones[3].rank = 10_001),
            ("coins above 1000000", &|r| {
                r.milestones[0].coins = 1_000_001
            }),
            ("negative fraction", &|r| r.level_grant_xp_fraction = -0.25),
            ("fraction above 1", &|r| r.level_grant_xp_fraction = 1.5),
            ("fraction NaN", &|r| r.level_grant_xp_fraction = f32::NAN),
            ("training base 0", &|r| r.camp_prices.npc_training_base = 0),
            ("training base above 1000000", &|r| {
                r.camp_prices.npc_training_base = 1_000_001
            }),
            ("per rank negative", &|r| r.camp_prices.per_rank = -0.25),
            ("per rank above 1", &|r| r.camp_prices.per_rank = 1.5),
            ("per rank NaN", &|r| r.camp_prices.per_rank = f32::NAN),
            ("max below 1", &|r| r.camp_prices.max_multiplier = 0.9),
            ("max above 10", &|r| r.camp_prices.max_multiplier = 10.5),
            ("max infinite", &|r| {
                r.camp_prices.max_multiplier = f32::INFINITY
            }),
        ];
        let passed: Vec<String> = rows
            .into_iter()
            .filter_map(|(row, change)| match edit(change) {
                Err(error) if error == "Invalid progression rewards" => None,
                Err(error) => Some(format!("{row}: {error}")),
                Ok(()) => Some(format!("{row}: validated")),
            })
            .collect();
        assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
        // The camp price bounds are inclusive.
        edit(&|r| {
            r.camp_prices.npc_training_base = 1_000_000;
            r.camp_prices.per_rank = 1.0;
            r.camp_prices.max_multiplier = 10.0;
        })
        .unwrap();
    }

    #[test]
    fn level_grant_and_camp_price_read_neutral_values_today() {
        // Shipped (neutral) values: no grant, flat legacy prices.
        for level in [0, 1, 2, 6, 12, 50] {
            assert_eq!(level_grant(level), 0, "level {level}");
        }
        for rank in [0, 1, 10, 20, 50, 1000] {
            assert_eq!(camp_price(25, rank), 25, "rank {rank}");
            assert_eq!(camp_price(100, rank), 100, "rank {rank}");
            assert_eq!(camp_price(27, rank), 27, "rank {rank}: neutral is exact");
        }
        // The authored values give the design tables.
        let rewards = authored();
        let grants: Vec<u64> = [1, 2, 3, 4, 5, 6, 8, 10, 12, 15, 20, 30, 40, 50]
            .into_iter()
            .map(|level| rewards.level_grant(level))
            .collect();
        assert_eq!(
            grants,
            [0, 5, 9, 14, 21, 28, 44, 61, 80, 110, 166, 290, 430, 580]
        );
        assert_eq!((2..=6).map(|l| rewards.level_grant(l)).sum::<u64>(), 77);
        let prices = |base| [0, 1, 10, 20, 50, 80].map(|rank| rewards.camp_price(base, rank));
        assert_eq!(
            prices(rewards.camp_prices.npc_training_base),
            [25, 30, 50, 75, 150, 150]
        );
        assert_eq!(prices(100), [100, 110, 200, 300, 600, 600]);
        // xp_to_advance_from is the player's level curve.
        let mut player = crate::campaign::Campaign::default().player;
        for level in 1..=60 {
            player.level = level;
            assert_eq!(xp_to_advance_from(level), player.next_level_xp(), "{level}");
        }
        assert_eq!([1, 2, 3].map(xp_to_advance_from), [20, 35, 57]);
    }
}
