//! Simulation-clock metrics and compact persistent run records. Never pays rewards.
use crate::{
    balance::BalanceSettings,
    content::{Mode, Slot},
    quality::Quality,
    sim::Outcome,
    survivors::SurvivorId,
    weapons::WeaponClass,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageSource {
    Defender,
    Structures,
    Ordnance,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageTotals {
    pub defender: u64,
    pub structures: u64,
    pub ordnance: u64,
}
impl DamageTotals {
    pub fn total(self) -> u64 {
        self.defender
            .saturating_add(self.structures)
            .saturating_add(self.ordnance)
    }
    fn add(&mut self, source: DamageSource, damage: u64) {
        let value = match source {
            DamageSource::Defender => &mut self.defender,
            DamageSource::Structures => &mut self.structures,
            DamageSource::Ordnance => &mut self.ordnance,
        };
        *value = value.saturating_add(damage);
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageBucket {
    pub tick: u64,
    pub damage: DamageTotals,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DpsTracker {
    pub version: u8,
    pub totals: DamageTotals,
    pub buckets: VecDeque<DamageBucket>,
    pub peak: f32,
}
impl DpsTracker {
    pub fn new() -> Self {
        Self {
            version: 1,
            ..Self::default()
        }
    }
    pub fn record(&mut self, tick: u64, source: DamageSource, damage: u64) {
        if damage == 0 {
            return;
        }
        self.version = 1;
        self.totals.add(source, damage);
        while self
            .buckets
            .front()
            .is_some_and(|b| tick.saturating_sub(b.tick) >= 300)
        {
            self.buckets.pop_front();
        }
        if self.buckets.back().is_none_or(|b| b.tick != tick) {
            self.buckets.push_back(DamageBucket {
                tick,
                damage: DamageTotals::default(),
            });
        }
        self.buckets
            .back_mut()
            .expect("bucket")
            .damage
            .add(source, damage);
        self.peak = self.peak.max(self.live(tick));
    }
    pub fn live(&self, tick: u64) -> f32 {
        self.buckets
            .iter()
            .filter(|b| tick.saturating_sub(b.tick) < 300)
            .map(|b| b.damage.total() as f64)
            .sum::<f64>() as f32
            / 5.0
    }
    pub fn average(&self, tick: u64) -> f32 {
        if tick == 0 {
            0.0
        } else {
            self.totals.total() as f32 / (tick as f32 / 60.0)
        }
    }
    pub fn validate(&self, tick: u64) -> Result<(), String> {
        let mut last = None;
        let mut sums = DamageTotals::default();
        if self.version > 1 || self.buckets.len() > 300 || !self.peak.is_finite() || self.peak < 0.0
        {
            return Err("Invalid simulation-clock DPS state".into());
        }
        for b in &self.buckets {
            if b.tick > tick || last.is_some_and(|v| b.tick <= v) {
                return Err("Invalid DPS bucket order".into());
            }
            last = Some(b.tick);
            sums.defender = sums
                .defender
                .checked_add(b.damage.defender)
                .ok_or("DPS defender sum overflow")?;
            sums.structures = sums
                .structures
                .checked_add(b.damage.structures)
                .ok_or("DPS structure sum overflow")?;
            sums.ordnance = sums
                .ordnance
                .checked_add(b.damage.ordnance)
                .ok_or("DPS ordnance sum overflow")?;
        }
        if sums.defender > self.totals.defender
            || sums.structures > self.totals.structures
            || sums.ordnance > self.totals.ordnance
        {
            return Err("DPS window exceeds cumulative injury".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GearRecord {
    pub slot: Slot,
    pub definition: crate::content::ItemDefId,
    pub name: String,
    pub level: u32,
    pub quality: Quality,
    pub weapon_class: Option<WeaponClass>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: u64,
    pub attempt_id: u64,
    pub worker_id: Option<u64>,
    pub recruit_id: Option<u64>,
    pub outcome: Outcome,
    pub mode: Mode,
    pub country_id: Option<String>,
    pub country_name: String,
    pub defender_name: String,
    pub cause: String,
    pub stage: usize,
    /// Catalogue level position of the encounter; saved as the level's stable ID.
    #[serde(with = "crate::content::level_ref")]
    pub template: usize,
    pub survivor: Option<SurvivorId>,
    pub lane_count: u8,
    pub width_yards: f32,
    pub threat_rank: u32,
    pub balance: BalanceSettings,
    pub gear: Vec<GearRecord>,
    pub duration_ticks: u64,
    pub kills: u32,
    pub shots: u32,
    pub hits: u32,
    pub misses: u32,
    pub loot: u32,
    pub damage_by_source: DamageTotals,
    pub average_dps: f32,
    pub peak_dps: f32,
    pub coins: u64,
    pub kill_xp: u64,
    pub completion_xp: u64,
    pub loot_by_quality: crate::kind_map::KindMap<crate::quality::Quality, u32>,
    pub end_hp: i32,
    pub max_hp: i32,
    pub legacy: bool,
}
impl RunRecord {
    pub fn duration_seconds(&self) -> f32 {
        self.duration_ticks as f32 / 60.0
    }
    pub fn validate(&self) -> Result<(), String> {
        self.balance.validate()?;
        if self.attempt_id == 0
            || self.template >= crate::content::content().levels.len()
            || !(3..=17).contains(&self.lane_count)
            || self.lane_count.is_multiple_of(2)
            || !self.width_yards.is_finite()
            || !(8.0..=30.0).contains(&self.width_yards)
            || self.max_hp <= 0
            || self.end_hp > self.max_hp
            || self.country_name.len() > 120
            || self.defender_name.len() > 120
            || self.cause.len() > 1000
            || self.gear.len() > 4
            || self.gear.iter().any(|g| {
                g.definition.index().is_none() || g.name.len() > 120 || !(1..=64).contains(&g.level)
            })
            || [self.average_dps, self.peak_dps]
                .into_iter()
                .any(|v| !v.is_finite() || v < 0.0)
            || self
                .loot_by_quality
                .iter()
                .map(|v| u64::from(*v))
                .sum::<u64>()
                > u64::from(self.loot)
        {
            return Err("Invalid persistent run record".into());
        }
        Ok(())
    }
}

pub fn first_run_id() -> u64 {
    1
}
