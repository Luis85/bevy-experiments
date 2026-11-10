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
/// A stage's place on its country's front: axis 2 of the threat (design
/// enemy-balance §1, ruling W-9). Derived from the attempt's stage and the
/// country's stage count at deploy and never saved; only the snapshot's
/// `front_ramp` and `boss_rank` copies are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrontPosition {
    /// 0-based stage on the front.
    pub stage: u32,
    /// The country's stage count.
    pub count: u32,
}
impl FrontPosition {
    /// A regional checkpoint every this many stages.
    pub const CHECKPOINT_EVERY: u32 = 6;
    /// `stage / (count - 1)` in 0..=1, or 0 on a front of one stage.
    pub fn depth(self) -> f32 {
        if self.count <= 1 {
            return 0.0;
        }
        self.stage.min(self.count - 1) as f32 / (self.count - 1) as f32
    }
    /// Capstone on the final stage, else Regional on every sixth stage,
    /// else Sector.
    pub fn boss_rank(self) -> BossRank {
        let ordinal = self.stage.saturating_add(1);
        if ordinal == self.count {
            BossRank::Capstone
        } else if ordinal.is_multiple_of(Self::CHECKPOINT_EVERY) {
            BossRank::Regional
        } else {
            BossRank::Sector
        }
    }
    /// A regional checkpoint stage (a Regional boss).
    pub fn is_checkpoint(self) -> bool {
        self.boss_rank() == BossRank::Regional
    }
}
impl BossRank {
    /// The boss phase transitions this rank adds (ruling W-9): one on a
    /// Capstone, none otherwise.
    pub fn extra_transitions(self) -> u8 {
        u8::from(self == BossRank::Capstone)
    }
}
/// The most phase transitions one boss may enter (schema v7 §3.10).
pub const MAX_BOSS_PHASES: u8 = 2;
/// Boss scaling starts above this rank: the boss damage rank term and the
/// boss phase gate (design enemy-balance §6, ruling W-6) ...
const BOSS_SCALING_FROM_RANK: u32 = 4;
/// ... and the rank term stops growing at this one.
const BOSS_SCALING_TO_RANK: u32 = 50;
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
    /// The front ramp at a country's final stage before the preset's
    /// percent (0..=0.5). Absent (0) keeps every stage of a front equal.
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub front_ramp_max: f32,
    /// Boss damage growth per threat rank above 4 (0..=0.05). Absent (0)
    /// keeps bosses at the shared enemy damage scaling.
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub boss_damage_per_rank: f32,
    /// The growth knee (design enemy-balance §2): enemy health and damage
    /// grow by `health_per_rank` and `damage_per_rank` up to this rank and
    /// by the `_late` slopes past it. Absent (`u32::MAX`) keeps one slope.
    #[serde(default = "no_growth_knee", skip_serializing_if = "is_no_growth_knee")]
    pub growth_knee_rank: u32,
    /// Enemy health growth per threat rank past the knee (0..=0.25).
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub health_per_rank_late: f32,
    /// Enemy damage growth per threat rank past the knee (0..=0.25).
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub damage_per_rank_late: f32,
    /// Personal boss armor growth per threat rank above 4 (0..=0.05).
    /// Absent (0) keeps the level's boss armor.
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub boss_armor_per_rank: f32,
    /// NPC bosses gain 1 armor per this many threat ranks above 4
    /// (0..=100). Absent (0) keeps the level's NPC boss armor.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub npc_boss_armor_rank_step: u32,
    /// The elite spawn curve (design enemy-balance §2 "Elite curve").
    /// Absent (the zero curve) means no elites at any rank.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub elite: EliteCurve,
    /// Level-up grant, rank milestones and camp prices. Absent means the
    /// neutral block: no grant, no milestones, flat camp prices.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub rewards: ProgressionRewardCatalog,
}
/// The elite spawn chance by threat rank, in basis points (design
/// enemy-balance §2 "Elite curve"): 0 below `min_rank`, `start_bp` at it,
/// linear to `end_bp` at `end_rank`, then held. The zero curve (`end_bp`
/// 0, the serde default) fields no elites, so a legacy override keeps none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EliteCurve {
    pub min_rank: u32,
    pub start_bp: u32,
    pub end_rank: u32,
    pub end_bp: u32,
}
/// Elites never spawn below this threat rank (schema v7 §3.10).
pub const ELITE_MIN_RANK: u32 = 3;
impl EliteCurve {
    /// The curve is the zero curve, or starts at rank 3 or later with a
    /// positive rate, ends later, never falls, stays within 10000 bp and
    /// rises by a whole number of basis points per rank.
    fn validate(&self) -> Result<(), String> {
        if *self == Self::default() {
            return Ok(());
        }
        let ranks = self.end_rank.saturating_sub(self.min_rank);
        if self.min_rank < ELITE_MIN_RANK
            || self.end_rank > 10_000
            || ranks == 0
            || self.start_bp == 0
            || self.end_bp < self.start_bp
            || self.end_bp > 10_000
            || !(self.end_bp - self.start_bp).is_multiple_of(ranks)
        {
            return Err("Invalid elite curve: it must be linear from rank 3 or later".into());
        }
        Ok(())
    }
    /// The elite chance in basis points at threat `rank`. Total on any
    /// curve: one that never went through validate (no rank span, ranks or
    /// rates reversed) holds `start_bp` from `min_rank` rather than panic.
    pub fn bp(&self, rank: u32) -> u32 {
        if self.end_bp == 0 || rank < self.min_rank {
            return 0;
        }
        let step = self
            .end_bp
            .saturating_sub(self.start_bp)
            .checked_div(self.end_rank.saturating_sub(self.min_rank))
            .unwrap_or(0);
        let ranks = rank.min(self.end_rank).saturating_sub(self.min_rank);
        self.start_bp.saturating_add(step.saturating_mul(ranks))
    }
}
/// The elite spawn chance in basis points at threat `rank`, before the
/// preset's `elite_rate_pct` (design enemy-balance §2): with the shipped
/// curve 0 below rank 3, else `200 + 65 x (min(rank, 23) - 3)`.
pub fn elite_bp(cfg: &ProgressionCatalog, rank: u32) -> u32 {
    cfg.elite.bp(rank)
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
/// The reward block progression settlement pays from: the installed
/// catalogue's `rewards`, or, in a `test-support` build, the calling thread's
/// [`override_rewards`] block.
pub fn rewards() -> &'static ProgressionRewardCatalog {
    #[cfg(feature = "test-support")]
    if let Some(rewards) = REWARDS_OVERRIDE.with(std::cell::Cell::get) {
        return rewards;
    }
    &catalog().rewards
}
#[cfg(feature = "test-support")]
thread_local! {
    static REWARDS_OVERRIDE: std::cell::Cell<Option<&'static ProgressionRewardCatalog>> =
        const { std::cell::Cell::new(None) };
}
/// Test-support only: [`rewards`] answers `rewards` on the calling thread
/// until the guard drops, so a test can author a level grant or milestones
/// without touching the shared catalogue. Panics on an invalid block.
#[cfg(feature = "test-support")]
pub fn override_rewards(rewards: ProgressionRewardCatalog) -> RewardsOverride {
    if let Err(error) = rewards.validate() {
        panic!("override_rewards: {error}");
    }
    let rewards: &'static ProgressionRewardCatalog = Box::leak(Box::new(rewards));
    RewardsOverride {
        previous: REWARDS_OVERRIDE.with(|cell| cell.replace(Some(rewards))),
    }
}
/// Restores the previous [`rewards`] answer when dropped.
#[cfg(feature = "test-support")]
#[must_use = "the override ends when the guard drops"]
pub struct RewardsOverride {
    previous: Option<&'static ProgressionRewardCatalog>,
}
#[cfg(feature = "test-support")]
impl Drop for RewardsOverride {
    fn drop(&mut self) {
        REWARDS_OVERRIDE.with(|cell| cell.set(self.previous));
    }
}
/// [`ProgressionRewardCatalog::level_grant`] over [`rewards`].
pub fn level_grant(level: u32) -> u64 {
    rewards().level_grant(level)
}
/// [`ProgressionRewardCatalog::camp_price`] over [`rewards`].
pub fn camp_price(base: u64, rank: u32) -> u64 {
    rewards().camp_price(base, rank)
}
fn no_growth_knee() -> u32 {
    u32::MAX
}
fn is_no_growth_knee(rank: &u32) -> bool {
    *rank == u32::MAX
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
    /// The load-time cross-check against the enemy catalogue: every pressure
    /// floor's `wave_tier` names an authored wave tier. `wave_pool` wraps a
    /// level past the last tier silently, so a floor above it would field an
    /// early tier's composition; [`MAX_WAVE_TIER`] alone does not catch it.
    pub fn check_wave_tiers(
        &self,
        enemies: &crate::enemy_catalog::EnemyCatalog,
    ) -> Result<(), String> {
        match self
            .pressure_floors
            .iter()
            .find(|floor| floor.wave_tier >= enemies.wave_count())
        {
            Some(floor) => Err(format!(
                "Pressure floor wave tier {} has no enemy wave tier (the catalogue has {})",
                floor.wave_tier,
                enemies.wave_count()
            )),
            None => Ok(()),
        }
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
                self.health_per_rank_late,
                self.damage_per_rank_late,
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
            || !finite(self.front_ramp_max, 0.0, 0.5)
            || !finite(self.boss_damage_per_rank, 0.0, 0.05)
            || !finite(self.boss_armor_per_rank, 0.0, 0.05)
            || self.npc_boss_armor_rank_step > 100
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
        self.elite.validate()?;
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
    /// The front ramp of a stage at `front` with the preset's
    /// `front_ramp_pct`: `1 + front_ramp_max × pct/100 × depth`, at most 2
    /// (ruling W-5). None, the literal neutral 1.0, without a country, at
    /// depth 0, at percent 0 or with no ramp in the catalogue.
    pub fn front_ramp(&self, front_ramp_pct: u16, front: Option<FrontPosition>) -> Option<f32> {
        let front = front?;
        if front.stage == 0 || front.count <= 1 || front_ramp_pct == 0 || self.front_ramp_max <= 0.0
        {
            return None;
        }
        let ramp = 1.0 + self.front_ramp_max * (f32::from(front_ramp_pct) / 100.0) * front.depth();
        Some(ramp.min(2.0))
    }
    /// The boss damage multiplier at threat `rank`:
    /// `1 + boss_damage_per_rank × (min(rank, 50) - 4)`, clamped to 1..=4
    /// (ruling W-6; the boss-rank factor arrives in wave 4). The literal 1.0
    /// through rank 4 and without the catalogue key.
    pub fn boss_damage_multiplier(&self, rank: u32) -> f32 {
        let steps = rank
            .min(BOSS_SCALING_TO_RANK)
            .saturating_sub(BOSS_SCALING_FROM_RANK);
        if steps == 0 || self.boss_damage_per_rank <= 0.0 {
            return 1.0;
        }
        (1.0 + self.boss_damage_per_rank * steps as f32).clamp(1.0, 4.0)
    }
    /// `1 + early × min(rank, knee) + late × (rank - knee)` past the knee:
    /// the enemy health and damage growth (design enemy-balance §2). Up to
    /// the knee, or with no knee (`u32::MAX`), it is the single legacy slope
    /// bit for bit. With a knee and no late slope it is flat past the knee
    /// (`1 + early × knee`), not the legacy slope; the `late == 0` return
    /// only skips adding zero.
    fn growth(&self, rank: u32, early: f32, late: f32) -> f32 {
        let knee = self.growth_knee_rank;
        let value = 1.0 + rank.min(knee) as f32 * early;
        if rank <= knee || late == 0.0 {
            return value;
        }
        value + (rank - knee) as f32 * late
    }
    /// The Personal boss armor multiplier at threat `rank`:
    /// `1 + boss_armor_per_rank × (min(rank, 50) - 4)`, clamped to 1..=4
    /// (design enemy-balance §2, "Boss armor"; the boss-rank factor arrives
    /// in wave 4). The literal 1.0 through rank 4 and without the key.
    pub fn boss_armor_multiplier(&self, rank: u32) -> f32 {
        let steps = rank
            .min(BOSS_SCALING_TO_RANK)
            .saturating_sub(BOSS_SCALING_FROM_RANK);
        if steps == 0 || self.boss_armor_per_rank <= 0.0 {
            return 1.0;
        }
        (1.0 + self.boss_armor_per_rank * steps as f32).clamp(1.0, 4.0)
    }
    /// The NPC boss armor added at threat `rank`: one per
    /// `npc_boss_armor_rank_step` ranks above 4, at most 20. 0 through rank
    /// 4 and without the key.
    pub fn npc_boss_armor_add(&self, rank: u32) -> i32 {
        if self.npc_boss_armor_rank_step == 0 {
            return 0;
        }
        (rank.saturating_sub(BOSS_SCALING_FROM_RANK) / self.npc_boss_armor_rank_step).min(20) as i32
    }
    pub fn snapshot(&self, rank: u32) -> ProgressionSnapshot {
        let bounded = rank.min(self.max_rank);
        let n = bounded as f32;
        let floor = &self.pressure_floors[(rank as usize).min(self.pressure_floors.len() - 1)];
        ProgressionSnapshot {
            enemy_cap: (self.population_base + bounded as usize * self.population_per_rank)
                .min(self.population_cap),
            rank,
            batch_size: (1 + bounded / self.batch_rank_step).min(self.max_batch),
            item_level: (1 + rank / self.item_level_rank_step).min(self.max_item_level),
            enemy_health: self.growth(rank, self.health_per_rank, self.health_per_rank_late),
            enemy_damage: self.growth(rank, self.damage_per_rank, self.damage_per_rank_late),
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
            boss_damage_multiplier: self.boss_damage_multiplier(rank),
            boss_armor_multiplier: self.boss_armor_multiplier(rank),
            npc_boss_armor_add: self.npc_boss_armor_add(rank),
            elite_rate_bp: self.elite.bp(rank),
            // The rank term of the transition count (design enemy-balance
            // §6): one transition past rank 4. The preset bonus and the
            // boss-rank term are folded in at deploy.
            boss_phases: u8::from(rank > BOSS_SCALING_FROM_RANK),
            // The other enemy-side fields stay neutral until their wave-2
            // writers.
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
    /// Folds a preset's catalogue extras into this rank snapshot (ruling
    /// E3), in integer math: the elite rate scales by `elite_rate_pct`
    /// (at most 10000 bp), and past the rank-4 phase gate the boss gains
    /// `boss_phase_bonus` transitions (at most 2). Normal's extras (100,
    /// 100, 0) change nothing, and the bonus adds no phase at ranks 0-4 on
    /// any preset (plan §2.5, the W2-26 pin reading). The rank term is the
    /// rank snapshot's; the front-ramp percent and the boss-rank term
    /// ([`BossRank::extra_transitions`]) are folded with the front position
    /// in `Campaign::progression_for`.
    pub fn fold_extras(&mut self, extras: crate::balance::PresetExtras) {
        if self.elite_rate_bp != 0 && extras.elite_rate_pct != 100 {
            let scaled = u64::from(self.elite_rate_bp) * u64::from(extras.elite_rate_pct) / 100;
            self.elite_rate_bp = scaled.min(10_000) as u32;
        }
        if self.rank > BOSS_SCALING_FROM_RANK && extras.boss_phase_bonus != 0 {
            self.boss_phases = self
                .boss_phases
                .saturating_add(extras.boss_phase_bonus)
                .min(MAX_BOSS_PHASES);
        }
    }
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
            || self.boss_phases > MAX_BOSS_PHASES
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

    /// The design's authored values, which progression.json ships (W2-18
    /// the camp prices, W2-54 the level grant and the milestones).
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
        // The shipped rewards block is the design's: the camp prices (W2-18),
        // the level grant and the four milestones (W2-54).
        assert_eq!(catalog().rewards, authored());
        let neutral = ProgressionCatalog {
            rewards: ProgressionRewardCatalog::default(),
            ..catalog().clone()
        };
        let neutral = serde_json::to_string(&neutral).unwrap();
        assert!(!neutral.contains("rewards"), "skipped at default");
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

    /// W2-54 (decision 14, PT7, PT8; R32): the shipped `progression.json`
    /// authors the design's level grant (§6, fraction 0.25) and the four
    /// rank milestones (§7), with the W2-18 camp prices unchanged.
    #[test]
    fn shipped_catalogue_authors_the_design_grant_and_milestones() {
        assert_eq!(level_grant(2), 5);
        let shipped = &catalog().rewards;
        assert_eq!(shipped.level_grant_xp_fraction, 0.25);
        assert_eq!(
            shipped.milestones,
            [
                milestone(5, 500, Quality::Uncommon),
                milestone(10, 1000, Quality::Uncommon),
                milestone(15, 1500, Quality::Rare),
                milestone(20, 2000, Quality::Rare),
            ]
        );
        assert_eq!(*shipped, authored());
        assert_eq!((2..=6).map(level_grant).sum::<u64>(), 77, "run 1 (L1->6)");
    }

    #[test]
    fn level_grant_and_camp_price_ship_the_design_tables() {
        // Shipped values: the level grant (W2-54) and the camp prices
        // (W2-18) follow the design tables; the neutral block pays no grant.
        let neutral = ProgressionRewardCatalog::default();
        for level in [0, 1, 2, 6, 12, 50] {
            assert_eq!(level_grant(level), authored().level_grant(level), "{level}");
            assert_eq!(neutral.level_grant(level), 0, "neutral level {level}");
        }
        let shipped =
            [0, 1, 10, 20, 50, 1000].map(|rank| [camp_price(25, rank), camp_price(100, rank)]);
        assert_eq!(
            shipped,
            [
                [25, 100],
                [30, 110],
                [50, 200],
                [75, 300],
                [150, 600],
                [150, 600]
            ]
        );
        // The neutral block keeps every price flat and exact.
        for rank in [0, 1, 10, 20, 50, 1000] {
            assert_eq!(neutral.camp_price(25, rank), 25, "rank {rank}");
            assert_eq!(neutral.camp_price(100, rank), 100, "rank {rank}");
            assert_eq!(
                neutral.camp_price(27, rank),
                27,
                "rank {rank}: neutral is exact"
            );
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

    /// W2-33 (design enemy-balance §2 "Elite curve", acceptance G6.3): 0
    /// below rank 3, 200 bp at 3, linear (+65 a rank) to 1500 at 23, then
    /// held; the snapshot writes it; a catalogue without the block has no
    /// elites; validation keeps the curve linear and bounded.
    #[test]
    fn elite_bp_pins() {
        let cfg = catalog();
        assert_eq!(
            cfg.elite,
            EliteCurve {
                min_rank: 3,
                start_bp: 200,
                end_rank: 23,
                end_bp: 1500
            }
        );
        let pins = [0, 1, 2, 3, 4, 13, 22, 23, 24, 50, 1000, u32::MAX].map(|r| elite_bp(cfg, r));
        assert_eq!(
            pins,
            [0, 0, 0, 200, 265, 850, 1435, 1500, 1500, 1500, 1500, 1500]
        );
        for rank in 3..=23 {
            assert_eq!(elite_bp(cfg, rank), 200 + 65 * (rank - 3), "rank {rank}");
        }
        for rank in [0, 2, 3, 13, 23, 60] {
            assert_eq!(cfg.snapshot(rank).elite_rate_bp, elite_bp(cfg, rank));
        }
        // A legacy override without the block: no elites at any rank.
        let mut legacy = serde_json::to_value(cfg).unwrap();
        legacy.as_object_mut().unwrap().remove("elite");
        let legacy = ProgressionCatalog::from_json(&legacy.to_string()).unwrap();
        assert_eq!(legacy.elite, EliteCurve::default());
        for rank in [0, 3, 13, 23, 1000] {
            assert_eq!(elite_bp(&legacy, rank), 0, "legacy rank {rank}");
            assert_eq!(legacy.snapshot(rank).elite_rate_bp, 0);
        }
        // Validation: linear, starting at rank 3 or later, bounded.
        let edit = |change: &dyn Fn(&mut EliteCurve)| {
            let mut c = cfg.clone();
            change(&mut c.elite);
            c.validate()
        };
        type Edit<'a> = &'a dyn Fn(&mut EliteCurve);
        let rows: [(&str, Edit); 7] = [
            ("not linear", &|e| e.end_bp = 1501),
            ("below rank 3", &|e| e.min_rank = 2),
            ("end before start", &|e| e.end_rank = 3),
            ("falling", &|e| e.start_bp = 1600),
            ("above 10000 bp", &|e| e.end_bp = 10_200),
            ("zero start", &|e| e.start_bp = 0),
            ("start without end", &|e| e.end_bp = 0),
        ];
        let passed: Vec<&str> = rows
            .into_iter()
            .filter_map(|(row, change)| edit(change).is_ok().then_some(row))
            .collect();
        assert_eq!(passed, Vec::<&str>::new(), "rows that validated");
        edit(&|e| *e = EliteCurve::default()).unwrap();
    }
    /// W2 review: `bp` is pub, so a curve that never went through validate
    /// (no rank span, ranks or rates reversed, extreme values) never panics
    /// or wraps; it holds `start_bp` from `min_rank` and never leaves the
    /// curve's own range.
    #[test]
    fn elite_bp_never_panics_on_an_unvalidated_curve() {
        let curve = |min_rank, start_bp, end_rank, end_bp| EliteCurve {
            min_rank,
            start_bp,
            end_rank,
            end_bp,
        };
        let curves = [
            curve(5, 300, 5, 900),
            curve(9, 300, 4, 900),
            curve(3, 1500, 23, 200),
            curve(0, u32::MAX, u32::MAX, u32::MAX),
            curve(0, u32::MAX - 1, 1, u32::MAX),
            curve(0, 1, 2, u32::MAX),
        ];
        for c in curves {
            for rank in [0, 1, 4, 5, 9, 23, 1000, u32::MAX] {
                let bp = std::panic::catch_unwind(|| c.bp(rank))
                    .unwrap_or_else(|_| panic!("{c:?} panics at rank {rank}"));
                let expected_floor = if rank < c.min_rank { 0 } else { c.start_bp };
                assert!(
                    (expected_floor.min(c.end_bp)..=c.start_bp.max(c.end_bp)).contains(&bp),
                    "{c:?} rank {rank}: {bp}"
                );
            }
        }
        assert_eq!(curve(5, 300, 5, 900).bp(5), 300, "no span holds start_bp");
        assert_eq!(curve(5, 300, 5, 900).bp(50), 300);
        assert_eq!(curve(9, 300, 4, 900).bp(9), 300, "reversed ranks");
        assert_eq!(
            curve(3, 1500, 23, 200).bp(23),
            1500,
            "a falling curve holds"
        );
    }
}
