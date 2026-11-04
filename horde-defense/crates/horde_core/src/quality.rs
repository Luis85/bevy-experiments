//! Item quality is independent of level and definition unlock tier. Quality
//! generation uses its own counter key and never consumes the combat RNG.
use crate::catalog::{self as shared, ContentError};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quality {
    Poor,
    #[default]
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}
impl Quality {
    pub const ALL: [Self; 6] = [
        Self::Poor,
        Self::Common,
        Self::Uncommon,
        Self::Rare,
        Self::Epic,
        Self::Legendary,
    ];
    pub fn name(self) -> &'static str {
        &catalog().definition(self).name
    }
    pub fn color(self) -> [f32; 4] {
        catalog().definition(self).color
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityDefinition {
    pub quality: Quality,
    pub name: String,
    pub color: [f32; 4],
    pub weight: u32,
    pub weight_per_rank: u32,
    pub min_rank: u32,
    pub stat_multiplier: f32,
    pub value_multiplier: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityCatalog {
    pub tiers: Vec<QualityDefinition>,
}
impl QualityCatalog {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        shared::parse_validated("quality", text, shared::MIB, Self::validate)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_QUALITY` override.
    pub fn try_load() -> Result<Self, ContentError> {
        shared::load(
            "quality",
            shared::MIB,
            include_str!("../quality.json"),
            Self::from_json_str,
        )
    }
    pub fn definition(&self, quality: Quality) -> &QualityDefinition {
        &self.tiers[quality as usize]
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.tiers.len() != 6 {
            return Err("Quality catalogue requires exactly six named tiers".into());
        }
        let mut seen = BTreeSet::new();
        for (index, tier) in self.tiers.iter().enumerate() {
            if tier.quality != Quality::ALL[index]
                || !seen.insert(tier.quality)
                || tier.name.trim().is_empty()
                || tier.name.len() > 40
                || tier
                    .color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || tier.color[3] < 0.5
                || tier.weight > 1_000_000
                || tier.weight_per_rank > 10_000
                || tier.min_rank > 1000
                || !tier.stat_multiplier.is_finite()
                || !(0.25..=4.0).contains(&tier.stat_multiplier)
                || !tier.value_multiplier.is_finite()
                || !(0.1..=20.0).contains(&tier.value_multiplier)
            {
                return Err("Invalid quality tier identity, palette, weight or multiplier".into());
            }
        }
        if self.tiers.windows(2).any(|p| {
            p[1].stat_multiplier <= p[0].stat_multiplier
                || p[1].value_multiplier <= p[0].value_multiplier
        }) || !self.tiers.iter().any(|q| q.min_rank == 0 && q.weight > 0)
        {
            return Err(
                "Quality multipliers must increase and rank zero needs an eligible weighted tier"
                    .into(),
            );
        }
        Ok(())
    }
    pub fn roll(&self, seed: u64, attempt: u64, enemy: u64, rank: u32) -> Quality {
        // Counter-based SplitMix avalanche with an explicit quality-purpose tag.
        fn mix(mut value: u64) -> u64 {
            value = value.wrapping_add(0x9E3779B97F4A7C15);
            value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
            value ^ (value >> 31)
        }
        let weight = |q: &QualityDefinition| {
            if rank >= q.min_rank {
                u64::from(q.weight) + u64::from(q.weight_per_rank) * u64::from(rank.min(1000))
            } else {
                0
            }
        };
        let total = self.tiers.iter().map(weight).sum::<u64>();
        let mut roll = mix(seed ^ mix(attempt) ^ mix(enemy) ^ 0x5155_414C_4954_5901) % total;
        for tier in &self.tiers {
            let n = weight(tier);
            if roll < n {
                return tier.quality;
            }
            roll -= n;
        }
        Quality::Common
    }
}
static VALUE: OnceLock<QualityCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static QualityCatalog, ContentError> {
    shared::install(&VALUE, QualityCatalog::try_load)
}
pub fn catalog() -> &'static QualityCatalog {
    shared::lazy(&VALUE, "quality", QualityCatalog::try_load, || {
        QualityCatalog::from_json_str(include_str!("../quality.json"))
    })
}
pub fn stat_ratio(base: Quality, rolled: Option<Quality>) -> f32 {
    catalog().definition(rolled.unwrap_or(base)).stat_multiplier
        / catalog().definition(base).stat_multiplier
}
pub fn value_ratio(base: Quality, rolled: Option<Quality>) -> f64 {
    f64::from(
        catalog()
            .definition(rolled.unwrap_or(base))
            .value_multiplier,
    ) / f64::from(catalog().definition(base).value_multiplier)
}
pub fn apply(
    mut item: crate::content::ItemDefinition,
    quality: Option<Quality>,
) -> crate::content::ItemDefinition {
    let ratio = stat_ratio(item.base_quality, quality);
    if ratio == 1.0 {
        return item;
    }
    let scale = |value: i32| (value as f32 * ratio).round().clamp(0.0, 100_000.0) as i32;
    item.personal.damage =
        scale(item.personal.damage).max(i32::from(item.slot == crate::content::Slot::Ranged));
    item.personal.melee_damage =
        scale(item.personal.melee_damage).max(i32::from(item.slot == crate::content::Slot::Melee));
    item.personal.armor = scale(item.personal.armor);
    item.personal.health = scale(item.personal.health);
    item.personal.stamina = scale(item.personal.stamina);
    item.npc.armor = scale(item.npc.armor);
    if matches!(
        item.slot,
        crate::content::Slot::Ranged | crate::content::Slot::Melee
    ) {
        let expected = crate::campaign::mean_basic_damage(crate::rules::Damage {
            dice: item.npc.damage_dice,
            adds: item.npc.damage_add,
            kind: item.npc.damage_kind,
        });
        item.npc.damage_add = (item.npc.damage_add as f32 + expected * (ratio - 1.0))
            .round()
            .clamp(-30.0, 100.0) as i32;
    }
    item
}

/// Informational receipt only. Reward/item insertion happened before emission;
/// UI, audio and replay must never grant the item a second time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LootEvent {
    #[serde(default = "legacy_loot_origin")]
    pub at: [f32; 2],
    pub item_id: u64,
    pub definition: usize,
    pub level: u32,
    pub quality: Quality,
    pub name: String,
    pub tick: u64,
}
fn legacy_loot_origin() -> [f32; 2] {
    [1.5, 5.5]
}
pub const MAX_LOOT_EVENTS: usize = 128;
pub fn validate_loot_events(
    events: &[LootEvent],
    next_item_id: u64,
    max_tick: Option<u64>,
    lane_count: u8,
) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    if events.len() > MAX_LOOT_EVENTS
        || events.iter().any(|event| {
            event.item_id == 0
                || event.item_id >= next_item_id
                || !ids.insert(event.item_id)
                || event.definition >= crate::content::content().items.len()
                || !(1..=64).contains(&event.level)
                || event.name.trim().is_empty()
                || event.name.len() > 120
                || event.at.iter().any(|v| !v.is_finite())
                || event.at[0] < 1.5
                || !(0.5..=f32::from(lane_count) - 0.5).contains(&event.at[1])
                || max_tick.is_some_and(|tick| event.tick > tick)
        })
    {
        return Err("Invalid bounded loot feedback receipt".into());
    }
    Ok(())
}
