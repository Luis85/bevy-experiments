//! Authored firearm identity. NPC attacks retain their independent pinned rules profile.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WeaponClass {
    Pistol,
    Smg,
    Shotgun,
    Rifle,
    Marksman,
    MachineGun,
}
impl WeaponClass {
    pub const ALL: [Self; 6] = [
        Self::Pistol,
        Self::Smg,
        Self::Shotgun,
        Self::Rifle,
        Self::Marksman,
        Self::MachineGun,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Pistol => "Pistol",
            Self::Smg => "SMG",
            Self::Shotgun => "Shotgun",
            Self::Rifle => "Rifle",
            Self::Marksman => "Marksman",
            Self::MachineGun => "Machine gun",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeaponSpec {
    pub class: WeaponClass,
    /// Damage in PersonalProfile is per pellet; one shell consumes one round.
    pub pellets: u32,
    /// Total fan width, in radians, centered on the aim ray.
    pub spread_radians: f32,
    /// Cap per target per trigger pull, including weak-point and skill bonuses.
    pub max_full_health_fraction_per_hit: f32,
}
impl Default for WeaponSpec {
    fn default() -> Self {
        Self {
            class: WeaponClass::Rifle,
            pellets: 1,
            spread_radians: 0.0,
            max_full_health_fraction_per_hit: 1.0,
        }
    }
}
impl WeaponSpec {
    pub fn hit_cap(&self, max_hp: i32) -> i32 {
        let cap = (max_hp.max(0) as f32 * self.max_full_health_fraction_per_hit).floor() as i32;
        if self.max_full_health_fraction_per_hit < 1.0 {
            cap.min(max_hp.saturating_sub(1)).max(0)
        } else {
            max_hp.max(0)
        }
    }
    pub fn validate(&self) -> bool {
        (1..=16).contains(&self.pellets)
            && self.spread_radians.is_finite()
            && (0.0..=1.2).contains(&self.spread_radians)
            && self.max_full_health_fraction_per_hit.is_finite()
            && (0.05..=1.0).contains(&self.max_full_health_fraction_per_hit)
            && (self.class != WeaponClass::Pistol || self.max_full_health_fraction_per_hit < 1.0)
            && (self.pellets == 1 || self.spread_radians > 0.0)
    }
    /// Old equipment without metadata remains a single-projectile rifle. No NPC
    /// rule changes or name heuristics are applied while restoring old saves.
    pub fn from_item(item: &crate::content::ItemDefinition) -> Self {
        item.weapon.clone().unwrap_or_default()
    }
}
