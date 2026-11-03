//! Validated next-deployment controls. Active encounters retain a complete copy.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BalancePreset {
    Easy,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalancePresetDefinition {
    pub id: BalancePreset,
    pub name: String,
    pub description: String,
    pub settings: BalanceSettings,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BalanceCatalog {
    pub default_preset: BalancePreset,
    pub presets: Vec<BalancePresetDefinition>,
}
impl BalanceCatalog {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let v: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        v.validate()?;
        Ok(v)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.default_preset == BalancePreset::Custom || self.presets.len() != 3 {
            return Err("Require Easy, Normal and Hard presets".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.presets {
            p.settings.validate()?;
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
pub fn catalog() -> &'static BalanceCatalog {
    static VALUE: OnceLock<BalanceCatalog> = OnceLock::new();
    VALUE.get_or_init(|| {
        let text = if let Some(path) = std::env::var_os("LASTLIGHT_BALANCE") {
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1_048_576) {
                panic!("Balance catalogue exceeds 1 MiB");
            }
            std::fs::read_to_string(path).expect("Could not read LASTLIGHT_BALANCE")
        } else {
            include_str!("../balance.json").to_owned()
        };
        BalanceCatalog::from_json(&text)
            .unwrap_or_else(|e| panic!("Invalid balance catalogue: {e}"))
    })
}
