//! Personal ordnance is an authored, enemy-only area attack. Flight, fire and
//! charges belong to the attempt; presentation never resolves damage or refills.
use crate::catalog::{self as shared, ContentError};
use crate::{content::Mode, skill_catalog::SkillModifiers};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

pub const MAX_ACTIVE_ORDNANCE: usize = 32;
pub const MAX_CHARGES: u32 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrdnanceKind {
    Grenade,
    Molotov,
}
impl OrdnanceKind {
    pub fn index(self) -> usize {
        match self {
            Self::Grenade => 0,
            Self::Molotov => 1,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Grenade => "Grenade",
            Self::Molotov => "Molotov",
        }
    }
    /// The skill unlock ID that grants this throw.
    pub fn unlock(self) -> &'static str {
        match self {
            Self::Grenade => crate::skill_catalog::unlocks::GRENADE,
            Self::Molotov => crate::skill_catalog::unlocks::MOLOTOV,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThrowRequest {
    pub kind: OrdnanceKind,
    pub cell: [u8; 2],
}
pub fn valid_cell(cell: [u8; 2]) -> bool {
    valid_cell_for_lanes(cell, crate::sim::LANES)
}
pub fn valid_cell_for_lanes(cell: [u8; 2], lanes: u8) -> bool {
    valid_cell_for_arena(cell, lanes, 30.0)
}
pub fn valid_cell_for_arena(cell: [u8; 2], lanes: u8, width: f32) -> bool {
    f32::from(cell[0]) + 0.5 <= width && cell[1] < lanes
}
pub fn cell_center(cell: [u8; 2]) -> [f32; 2] {
    [f32::from(cell[0]) + 0.5, f32::from(cell[1]) + 0.5]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrdnanceDefinition {
    pub flight_ticks: u32,
    pub cooldown_ticks: u32,
    pub damage: i32,
    pub radius: f32,
    pub edge_damage_fraction: f32,
    pub burn_damage: i32,
    pub burn_ticks: u32,
    pub burn_interval: u32,
}
impl OrdnanceDefinition {
    fn valid(&self) -> bool {
        (1..=600).contains(&self.flight_ticks)
            && (1..=3600).contains(&self.cooldown_ticks)
            && (1..=100_000).contains(&self.damage)
            && valid_radius(self.radius)
            && self.edge_damage_fraction.is_finite()
            && (0.0..=1.0).contains(&self.edge_damage_fraction)
            && (0..=100_000).contains(&self.burn_damage)
            && self.burn_ticks <= 3600
            && (1..=600).contains(&self.burn_interval)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrdnanceCatalog {
    pub grenade: OrdnanceDefinition,
    pub molotov: OrdnanceDefinition,
}
impl OrdnanceCatalog {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        shared::parse_validated("ordnance", text, shared::MIB, Self::validate)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_ORDNANCE` override.
    pub fn try_load() -> Result<Self, ContentError> {
        shared::load(
            "ordnance",
            shared::MIB,
            include_str!("../ordnance.json"),
            Self::from_json_str,
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.grenade.valid()
            || !self.molotov.valid()
            || self.grenade.burn_ticks != 0
            || self.grenade.burn_damage != 0
            || self.molotov.burn_ticks == 0
            || self.molotov.burn_damage == 0
        {
            return Err("Invalid grenade/Molotov definition".into());
        }
        Ok(())
    }
    pub fn definition(&self, kind: OrdnanceKind) -> &OrdnanceDefinition {
        match kind {
            OrdnanceKind::Grenade => &self.grenade,
            OrdnanceKind::Molotov => &self.molotov,
        }
    }
}
static CATALOG: OnceLock<OrdnanceCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static OrdnanceCatalog, ContentError> {
    shared::install(&CATALOG, OrdnanceCatalog::try_load)
}
pub fn catalog() -> &'static OrdnanceCatalog {
    shared::lazy(&CATALOG, "ordnance", OrdnanceCatalog::try_load, || {
        OrdnanceCatalog::from_json_str(include_str!("../ordnance.json"))
    })
}
pub fn validate_catalog() -> Result<(), String> {
    catalog().validate()
}

/// Bounded facts about the successful release, independent of rendering and RNG.
/// Transient battle events may expire before an airborne projectile is resumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseCues {
    pub tick: u64,
    pub brace: bool,
    pub last_shot_tick: Option<u64>,
    pub last_hit_tick: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThrownOrdnance {
    pub id: u64,
    pub kind: OrdnanceKind,
    pub cell: [u8; 2],
    pub flight_left: u32,
    pub flight_ticks: u32,
    pub damage: i32,
    pub radius: f32,
    pub edge_damage_fraction: f32,
    pub burn_damage: i32,
    pub burn_ticks: u32,
    pub burn_interval: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_cues: Option<ReleaseCues>,
}
impl ThrownOrdnance {
    pub fn validate_release_cues(&self, current_tick: u64) -> Result<(), String> {
        if let Some(cues) = self.release_cues {
            let elapsed = self.flight_ticks.checked_sub(self.flight_left);
            if cues.tick == 0
                || cues.tick > current_tick
                || elapsed.is_none()
                || current_tick.checked_sub(cues.tick) != elapsed.map(u64::from)
                || cues.last_shot_tick.is_some_and(|tick| tick > cues.tick)
                || cues.last_hit_tick.is_some_and(|tick| tick > cues.tick)
            {
                return Err("Invalid saved ordnance release cue clock".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FireZone {
    pub id: u64,
    pub cell: [u8; 2],
    pub radius: f32,
    pub damage: i32,
    pub ticks_left: u32,
    pub tick_left: u32,
    pub tick_interval: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrdnanceState {
    pub projectiles: Vec<ThrownOrdnance>,
    pub fire_zones: Vec<FireZone>,
    pub cooldowns: [u32; 2],
    pub charges: [u32; 2],
    pub issued: [u32; 2],
    pub spent: [u32; 2],
    /// Number of throws issued in this attempt; actor IDs use Attempt.next_id.
    pub next_id: u64,
}
fn valid_radius(radius: f32) -> bool {
    radius.is_finite() && (0.1..=15.0).contains(&radius)
}
pub fn allowance(mode: Mode, modifiers: &SkillModifiers) -> [u32; 2] {
    if mode != Mode::Personal {
        return [0; 2];
    }
    [
        if modifiers.unlocked(OrdnanceKind::Grenade.unlock()) {
            modifiers.grenade_charges.min(MAX_CHARGES)
        } else {
            0
        },
        if modifiers.unlocked(OrdnanceKind::Molotov.unlock()) {
            modifiers.molotov_charges.min(MAX_CHARGES)
        } else {
            0
        },
    ]
}
impl OrdnanceState {
    pub fn for_deployment(mode: Mode, modifiers: &SkillModifiers) -> Self {
        let issued = allowance(mode, modifiers);
        Self {
            issued,
            charges: issued,
            ..Self::default()
        }
    }
    pub fn validate(
        &self,
        mode: Mode,
        modifiers: &SkillModifiers,
        next_actor_id: u64,
        lane_count: u8,
        width_yards: f32,
    ) -> Result<(), String> {
        let allowed = allowance(mode, modifiers);
        let mut ids = BTreeSet::new();
        if [
            modifiers.ordnance_damage_multiplier,
            modifiers.ordnance_radius_multiplier,
            modifiers.burn_duration_multiplier,
            modifiers.ordnance_cooldown_multiplier,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0 || value > 10_000.0)
            || self.projectiles.len() + self.fire_zones.len() > MAX_ACTIVE_ORDNANCE
            || (0..2).any(|i| {
                self.issued[i] > allowed[i]
                    || self.charges[i].checked_add(self.spent[i]) != Some(self.issued[i])
                    || self.cooldowns[i] > 3600
            })
            || self.next_id != u64::from(self.spent[0]) + u64::from(self.spent[1])
            || self.projectiles.len() + self.fire_zones.len() > self.next_id as usize
            || (0..2).any(|index| {
                self.projectiles
                    .iter()
                    .filter(|p| p.kind.index() == index)
                    .count()
                    + if index == 1 { self.fire_zones.len() } else { 0 }
                    > self.spent[index] as usize
            })
            || (mode != Mode::Personal && self != &Self::default())
        {
            return Err("Invalid saved ordnance allowance or clock".into());
        }
        for p in &self.projectiles {
            if p.id == 0
                || p.id >= next_actor_id
                || !ids.insert(p.id)
                || !valid_cell_for_arena(p.cell, lane_count, width_yards)
                || !(1..=600).contains(&p.flight_ticks)
                || p.flight_left == 0
                || p.flight_left > p.flight_ticks
                || !(1..=100_000).contains(&p.damage)
                || !valid_radius(p.radius)
                || !p.edge_damage_fraction.is_finite()
                || !(0.0..=1.0).contains(&p.edge_damage_fraction)
                || !(0..=100_000).contains(&p.burn_damage)
                || p.burn_ticks > 3600
                || !(1..=600).contains(&p.burn_interval)
                || (p.kind == OrdnanceKind::Grenade && (p.burn_ticks != 0 || p.burn_damage != 0))
                || (p.kind == OrdnanceKind::Molotov && (p.burn_ticks == 0 || p.burn_damage == 0))
            {
                return Err("Invalid saved ordnance projectile".into());
            }
        }
        for z in &self.fire_zones {
            if z.id == 0
                || z.id >= next_actor_id
                || !ids.insert(z.id)
                || !valid_cell_for_arena(z.cell, lane_count, width_yards)
                || !valid_radius(z.radius)
                || !(1..=100_000).contains(&z.damage)
                || !(1..=3600).contains(&z.ticks_left)
                || !(1..=600).contains(&z.tick_interval)
                || z.tick_left == 0
                || z.tick_left > z.tick_interval
            {
                return Err("Invalid saved ordnance fire zone".into());
            }
        }
        Ok(())
    }
}
