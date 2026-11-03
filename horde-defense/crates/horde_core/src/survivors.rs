//! Cosmetic personal survivor identity. Neither choice changes combat numbers.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentVisualItem {
    pub definition: usize,
    pub level: u32,
    pub quality: crate::quality::Quality,
    pub style: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EquipmentVisual {
    pub ranged: Option<EquipmentVisualItem>,
    pub outfit: Option<EquipmentVisualItem>,
    pub accessory: Option<EquipmentVisualItem>,
    pub melee: Option<EquipmentVisualItem>,
    pub weapon_class: Option<crate::weapons::WeaponClass>,
}
impl EquipmentVisual {
    pub fn validate(&self) -> Result<(), String> {
        for (slot, item) in [
            (crate::content::Slot::Ranged, &self.ranged),
            (crate::content::Slot::Outfit, &self.outfit),
            (crate::content::Slot::Accessory, &self.accessory),
            (crate::content::Slot::Melee, &self.melee),
        ] {
            let Some(item) = item else {
                continue;
            };
            if !(1..=64).contains(&item.level)
                || crate::content::content()
                    .items
                    .get(item.definition)
                    .is_none_or(|definition| definition.slot != slot)
                || item.style.len() > 64
                || !item
                    .style
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_')
            {
                return Err("Invalid frozen equipped appearance".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    Male,
    Female,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurvivorId {
    Rook,
    June,
}
impl SurvivorId {
    pub const ALL: [Self; 2] = [Self::Rook, Self::June];
    pub fn name(self) -> &'static str {
        match self {
            Self::Rook => "Rook",
            Self::June => "June",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Rook => "A road-worn survivor with a steady resolve.",
            Self::June => "A resourceful survivor ready to hold the line.",
        }
    }
    pub fn appearance(self) -> Appearance {
        match self {
            Self::Rook => Appearance::Male,
            Self::June => Appearance::Female,
        }
    }
}
pub fn legacy_selection() -> Option<SurvivorId> {
    Some(SurvivorId::Rook)
}
