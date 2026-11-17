//! Validated next-deployment controls. Active encounters retain a complete copy.
use crate::catalog::{self as shared, ContentError};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BalancePreset {
    Easy,
    /// The default is only the neutral serde value of saved fields such as
    /// `Campaign.balance_extras_basis`; `BalanceSettings::default()` is separate.
    #[default]
    Normal,
    Hard,
    Custom,
}
impl BalancePreset {
    pub const ALL: [Self; 4] = [Self::Easy, Self::Normal, Self::Hard, Self::Custom];
    pub fn name(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
            Self::Custom => "Custom",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceSettings {
    pub preset: BalancePreset,
    pub enemy_health: f32,
    pub enemy_damage: f32,
    pub enemy_speed: f32,
    pub spawn_rate: f32,
    pub player_damage: f32,
    pub defender_health: f32,
    pub reward: f32,
    pub boss_time: f32,
    pub threat_growth: f32,
    pub npc_spawn_rate: f32,
    pub supply: i32,
    pub start_lanes: u8,
    pub max_lanes: u8,
    pub lanes_per_step: u8,
    pub lane_level_step: u32,
    pub start_width: u8,
    pub max_width: u8,
    pub width_per_step: u8,
    pub width_level_step: u32,
}
impl Default for BalanceSettings {
    fn default() -> Self {
        Self::for_preset(catalog().default_preset)
    }
}
impl BalanceSettings {
    pub fn for_preset(preset: BalancePreset) -> Self {
        let mut s = catalog()
            .presets
            .iter()
            .find(|p| {
                p.id == if preset == BalancePreset::Custom {
                    BalancePreset::Normal
                } else {
                    preset
                }
            })
            .expect("validated preset")
            .settings
            .clone();
        s.preset = preset;
        s
    }
    pub fn legacy() -> Self {
        Self {
            preset: BalancePreset::Custom,
            enemy_health: 1.0,
            enemy_damage: 1.0,
            enemy_speed: 1.0,
            spawn_rate: 1.0,
            player_damage: 1.0,
            defender_health: 1.0,
            reward: 1.0,
            boss_time: 1.0,
            threat_growth: 1.0,
            npc_spawn_rate: 1.0,
            supply: 0,
            start_lanes: 11,
            max_lanes: 17,
            lanes_per_step: 2,
            lane_level_step: 10,
            start_width: 30,
            max_width: 30,
            width_per_step: 0,
            width_level_step: 10,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.enemy_health,
            self.enemy_damage,
            self.enemy_speed,
            self.spawn_rate,
            self.player_damage,
            self.defender_health,
            self.reward,
            self.boss_time,
            self.threat_growth,
            self.npc_spawn_rate,
        ]
        .into_iter()
        .any(|v| !v.is_finite() || !(0.25..=3.0).contains(&v))
            || !(-30..=100).contains(&self.supply)
            || !(3..=17).contains(&self.start_lanes)
            || self.start_lanes.is_multiple_of(2)
            || !(self.start_lanes..=17).contains(&self.max_lanes)
            || self.max_lanes.is_multiple_of(2)
            || self.lanes_per_step > 6
            || !self.lanes_per_step.is_multiple_of(2)
            || !(1..=100).contains(&self.lane_level_step)
            || !(8..=30).contains(&self.start_width)
            || !(self.start_width..=30).contains(&self.max_width)
            || self.width_per_step > 8
            || !(1..=100).contains(&self.width_level_step)
        {
            return Err("Balance controls exceed supported finite ranges".into());
        }
        Ok(())
    }
    pub fn geometry(&self, level: u32) -> (u8, f32) {
        let level = level
            .min(crate::campaign::Player::level_cap())
            .saturating_sub(1);
        (
            (u32::from(self.start_lanes)
                + level / self.lane_level_step * u32::from(self.lanes_per_step))
            .min(u32::from(self.max_lanes)) as u8,
            (u32::from(self.start_width)
                + level / self.width_level_step * u32::from(self.width_per_step))
            .min(u32::from(self.max_width)) as f32,
        )
    }
}
/// Catalogue-only rules a preset adds beyond its `BalanceSettings` (never
/// saved, never in the digest). Integer percents and a phase count, so the
/// wave-2 fold stays integer math; the default is Normal's neutral values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetExtras {
    /// Elite spawn-rate scale in percent, 0..=300.
    pub elite_rate_pct: u16,
    /// Country front-ramp scale in percent, 0..=300.
    pub front_ramp_pct: u16,
    /// Extra boss phase transitions, 0..=2.
    pub boss_phase_bonus: u8,
}
impl Default for PresetExtras {
    fn default() -> Self {
        Self {
            elite_rate_pct: 100,
            front_ramp_pct: 100,
            boss_phase_bonus: 0,
        }
    }
}
impl PresetExtras {
    pub const MAX_PCT: u16 = 300;
    pub const MAX_BOSS_PHASE_BONUS: u8 = 2;
    /// The catalogue extras of `preset`; Custom has Normal's.
    pub fn for_preset(preset: BalancePreset) -> Self {
        let id = if preset == BalancePreset::Custom {
            BalancePreset::Normal
        } else {
            preset
        };
        catalog()
            .presets
            .iter()
            .find(|p| p.id == id)
            .expect("validated preset")
            .extras
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.elite_rate_pct > Self::MAX_PCT
            || self.front_ramp_pct > Self::MAX_PCT
            || self.boss_phase_bonus > Self::MAX_BOSS_PHASE_BONUS
        {
            return Err("Balance preset extras exceed supported ranges".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalancePresetDefinition {
    pub id: BalancePreset,
    pub name: String,
    pub description: String,
    pub settings: BalanceSettings,
    /// Absent in older catalogues, which then give every preset Normal's.
    #[serde(default)]
    pub extras: PresetExtras,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceCatalog {
    pub default_preset: BalancePreset,
    pub presets: Vec<BalancePresetDefinition>,
}
impl BalanceCatalog {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        shared::parse_validated("balance", text, shared::MIB, Self::validate)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_BALANCE` override.
    pub fn try_load() -> Result<Self, ContentError> {
        shared::load(
            "balance",
            shared::MIB,
            include_str!("../balance.json"),
            Self::from_json_str,
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.default_preset == BalancePreset::Custom || self.presets.len() != 3 {
            return Err("Require Easy, Normal and Hard presets".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.presets {
            p.settings.validate()?;
            p.extras.validate()?;
            if p.id == BalancePreset::Custom
                || p.id != p.settings.preset
                || !seen.insert(p.id as u8)
                || p.name.is_empty()
                || p.name.len() > 40
                || p.description.len() > 500
            {
                return Err("Invalid or duplicate balance preset".into());
            }
        }
        Ok(())
    }
}
static VALUE: OnceLock<BalanceCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static BalanceCatalog, ContentError> {
    shared::install(&VALUE, BalanceCatalog::try_load)
}
pub fn catalog() -> &'static BalanceCatalog {
    shared::lazy(&VALUE, "balance", BalanceCatalog::try_load, || {
        BalanceCatalog::from_json_str(include_str!("../balance.json"))
    })
}
