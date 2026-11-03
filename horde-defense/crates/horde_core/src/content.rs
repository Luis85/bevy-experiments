use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Personal,
    Npc,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slot {
    Ranged,
    Melee,
    Outfit,
    Accessory,
}
impl Slot {
    pub const ALL: [Self; 4] = [Self::Ranged, Self::Melee, Self::Outfit, Self::Accessory];
    pub fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyKind {
    Shambler,
    Runner,
    Brute,
    Armored,
    Demolisher,
}
impl EnemyKind {
    pub const ALL: [Self; 5] = [
        Self::Shambler,
        Self::Runner,
        Self::Brute,
        Self::Armored,
        Self::Demolisher,
    ];
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BossKind {
    Breacher,
    Caller,
    SiegeBrute,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureKind {
    Trap,
    Wall,
    Turret,
}
impl StructureKind {
    pub const ALL: [Self; 3] = [Self::Trap, Self::Wall, Self::Turret];
    pub fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Policy {
    Balanced,
    Marksman,
    Guardian,
    BossFocus,
}
impl Policy {
    pub const ALL: [Self; 4] = [
        Self::Balanced,
        Self::Marksman,
        Self::Guardian,
        Self::BossFocus,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersonalProfile {
    pub damage: i32,
    pub magazine: u32,
    pub cadence: f32,
    pub reload: f32,
    pub armor: i32,
    pub melee_damage: i32,
    pub health: i32,
    pub stamina: i32,
    pub weak_bonus: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NpcProfile {
    pub damage_dice: u8,
    pub damage_add: i32,
    pub damage_kind: crate::rules::DamageKind,
    pub armor: i32,
    pub accuracy: i32,
    pub magazine: u32,
    pub reload_turns: u32,
    pub skill_bonus: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDefinition {
    #[serde(default)]
    pub visual_style: String,
    #[serde(default)]
    pub base_quality: crate::quality::Quality,
    /// Equipment issued to NPC defenders; fresh personal loadouts must acquire
    /// their own alternative. Pre-proficiency saves retain grandfathered access.
    #[serde(default)]
    pub npc_only: bool,
    #[serde(default)]
    pub weapon: Option<crate::weapons::WeaponSpec>,
    pub name: String,
    pub slot: Slot,
    pub tier: usize,
    pub rarity: String,
    pub sell_value: u64,
    pub buy_value: u64,
    pub personal: PersonalProfile,
    pub npc: NpcProfile,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyDefinition {
    pub kind: EnemyKind,
    pub name: String,
    pub health: i32,
    pub speed: f32,
    pub damage: i32,
    pub armor: i32,
    pub windup: f32,
    pub reward: u64,
    pub xp: u64,
    pub npc_hp: i32,
    pub npc_ht: i32,
    pub npc_skill: i32,
    pub npc_dodge: i32,
    pub npc_armor: i32,
    pub npc_damage_dice: u8,
    pub npc_damage_add: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BossTiming {
    pub warning: f32,
    pub exposure_delay: f32,
    pub exposure_duration: f32,
    pub cycle: f32,
    pub call_warning: f32,
    pub call_interval: f32,
    pub initial_call: f32,
    pub siege_recovery: f32,
}
impl Default for BossTiming {
    fn default() -> Self {
        Self {
            warning: 5.0,
            exposure_delay: 7.0,
            exposure_duration: 4.0,
            cycle: 12.0,
            call_warning: 2.0,
            call_interval: 15.0,
            initial_call: 10.0,
            siege_recovery: 4.0,
        }
    }
}
impl BossTiming {
    pub fn validate(&self) -> bool {
        let positive = [
            self.warning,
            self.exposure_duration,
            self.cycle,
            self.call_warning,
            self.call_interval,
            self.initial_call,
            self.siege_recovery,
        ];
        positive
            .into_iter()
            .all(|v| v.is_finite() && (0.25..=600.0).contains(&v))
            && self.exposure_delay.is_finite()
            && self.exposure_delay >= 0.0
            && self.exposure_delay + self.exposure_duration <= self.cycle
            && self.call_warning < self.call_interval
            && self.call_warning < self.initial_call
            && self.siege_recovery <= self.cycle
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LevelDefinition {
    pub name: String,
    pub subtitle: String,
    pub boss: BossKind,
    pub boss_name: String,
    pub boss_time: f32,
    pub start_rate: f32,
    pub growth: f32,
    pub rate_cap: f32,
    pub enemy_cap: usize,
    pub enemies: Vec<u32>,
    pub supply: u32,
    pub boss_health: i32,
    pub boss_armor: i32,
    pub boss_damage: i32,
    pub boss_reward: u64,
    pub completion_bonus: u64,
    pub boss_npc_hp: i32,
    pub boss_npc_ht: i32,
    pub boss_npc_skill: i32,
    pub boss_npc_dodge: i32,
    pub boss_npc_armor: i32,
    pub boss_npc_damage_dice: u8,
    pub boss_npc_damage_add: i32,
    #[serde(default)]
    pub boss_timing: BossTiming,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructureDefinition {
    pub kind: StructureKind,
    pub name: String,
    pub supply: u32,
    pub health: i32,
    pub damage: i32,
    pub range: f32,
    pub cooldown: f32,
    pub charges: u32,
    pub blueprint_price: u64,
    pub unlock_clear: usize,
    pub npc: NpcProfile,
    #[serde(default = "default_structure_npc_skill")]
    pub npc_skill: i32,
}
fn default_structure_npc_skill() -> i32 {
    15
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Content {
    pub levels: Vec<LevelDefinition>,
    pub enemies: Vec<EnemyDefinition>,
    pub items: Vec<ItemDefinition>,
    pub structures: Vec<StructureDefinition>,
}
impl Content {
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > 32 * 1024 * 1024 {
            return Err(
                "Encounter/equipment JSON exceeds the supported 32 MiB authoring limit".into(),
            );
        }
        let data: Self = serde_json::from_str(text).map_err(|e| format!("Content JSON: {e}"))?;
        data.validate()?;
        Ok(data)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.levels.len() != 5
            || self.enemies.len() != EnemyKind::ALL.len()
            || self.structures.len() != StructureKind::ALL.len()
            || self.items.len() < 4
            || self.items.len() > 4096
        {
            return Err("Expected five encounter templates, canonical enemy/structure tables, and four protected starters".into());
        }
        let safe_float =
            |value: f32, min: f32, max: f32| value.is_finite() && (min..=max).contains(&value);
        let damage_valid = |dice: u8, adds: i32| {
            crate::rules::Damage {
                dice,
                adds,
                kind: crate::rules::DamageKind::Crushing,
            }
            .valid()
        };
        let npc_valid = |p: &NpcProfile| {
            damage_valid(p.damage_dice, p.damage_add)
                && (0..=100).contains(&p.armor)
                && (0..=20).contains(&p.accuracy)
                && (1..=60).contains(&p.reload_turns)
                && (-10..=20).contains(&p.skill_bonus)
                && p.magazine <= 1000
        };
        for l in &self.levels {
            let total = l.enemies.iter().try_fold(0u32, |n, w| n.checked_add(*w));
            if l.name.trim().is_empty()
                || !safe_float(l.boss_time, 10.0, 3600.0)
                || !(1..=512).contains(&l.enemy_cap)
                || !safe_float(l.start_rate, 0.001, 30.0)
                || !safe_float(l.rate_cap, l.start_rate, 30.0)
                || !safe_float(l.growth, 0.0, 100.0)
                || l.enemies.len() != 5
                || total.is_none_or(|n| n == 0)
                || !(1..=100_000).contains(&l.boss_health)
                || !(0..=1000).contains(&l.boss_armor)
                || !(1..=10_000).contains(&l.boss_damage)
                || !(1..=1000).contains(&l.boss_npc_hp)
                || !(3..=30).contains(&l.boss_npc_ht)
                || !(3..=30).contains(&l.boss_npc_skill)
                || !(0..=20).contains(&l.boss_npc_dodge)
                || !(0..=100).contains(&l.boss_npc_armor)
                || !damage_valid(l.boss_npc_damage_dice, l.boss_npc_damage_add)
                || l.supply > 1000
                || l.boss_reward > 1_000_000
                || l.completion_bonus > 1_000_000
                || !l.boss_timing.validate()
                || l.boss_timing.warning > l.boss_time
            {
                return Err(format!("Invalid or unsupported level profile: {}", l.name));
            }
        }
        for (index, e) in self.enemies.iter().enumerate() {
            if e.kind != EnemyKind::ALL[index]
                || e.name.trim().is_empty()
                || !(1..=100_000).contains(&e.health)
                || !safe_float(e.speed, 0.01, 10.0)
                || !safe_float(e.windup, 0.5, 60.0)
                || !(0..=1000).contains(&e.armor)
                || !(0..=100).contains(&e.npc_armor)
                || !(1..=10_000).contains(&e.damage)
                || !(1..=1000).contains(&e.npc_hp)
                || !(3..=30).contains(&e.npc_ht)
                || !(3..=30).contains(&e.npc_skill)
                || !(0..=20).contains(&e.npc_dodge)
                || !damage_valid(e.npc_damage_dice, e.npc_damage_add)
                || e.reward == 0
                || e.reward > 1_000_000
                || e.xp > 1_000_000
            {
                return Err(format!(
                    "Invalid enemy profile or canonical order: {}",
                    e.name
                ));
            }
        }
        for (index, d) in self.structures.iter().enumerate() {
            if (d.kind == StructureKind::Wall && !(0..=30).contains(&d.npc_skill))
                || (d.kind != StructureKind::Wall && !(3..=30).contains(&d.npc_skill))
            {
                return Err(format!("Invalid explicit NPC structure skill: {}", d.name));
            }
            if d.kind != StructureKind::ALL[index]
                || d.name.trim().is_empty()
                || !(1..=100_000).contains(&d.health)
                || !(1..=1000).contains(&d.supply)
                || !safe_float(d.cooldown, 0.1, 60.0)
                || !safe_float(d.range, 0.0, 60.0)
                || d.damage < 0
                || d.damage > 10000
                || !npc_valid(&d.npc)
                || d.unlock_clear > 5
                || d.blueprint_price > 1_000_000
                || (d.kind == StructureKind::Trap
                    && (d.charges == 0 || d.charges > 1000 || d.damage == 0))
                || (d.kind == StructureKind::Turret && (d.range <= 0.0 || d.damage == 0))
            {
                return Err(format!(
                    "Invalid structure profile or canonical order: {}",
                    d.name
                ));
            }
        }
        for (index, i) in self.items.iter().enumerate() {
            if i.visual_style.len() > 64
                || !i
                    .visual_style
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_')
            {
                return Err(format!("Invalid authored equipment style: {}", i.name));
            }
            if i.weapon
                .as_ref()
                .is_some_and(|w| !w.validate() || i.slot != Slot::Ranged)
            {
                return Err(format!(
                    "Invalid firearm class or projectile profile: {}",
                    i.name
                ));
            }
            if i.slot == Slot::Melee
                && (1..=20).any(|st| {
                    crate::rules::strength_adjusted_swing(
                        crate::rules::Damage {
                            dice: i.npc.damage_dice,
                            adds: i.npc.damage_add,
                            kind: i.npc.damage_kind,
                        },
                        st,
                    )
                    .is_none()
                })
            {
                return Err(format!(
                    "Melee profile cannot support the declared ST/fatigue range: {}",
                    i.name
                ));
            }
            let p = &i.personal;
            if i.name.trim().is_empty()
                || i.name.len() > 120
                || i.tier > 5
                || i.buy_value > 1_000_000
                || i.sell_value > i.buy_value
                || !npc_valid(&i.npc)
                || !(0..=1000).contains(&p.armor)
                || !(0..=1000).contains(&p.health)
                || !(0..=1000).contains(&p.stamina)
                || !safe_float(p.weak_bonus, 0.0, 1.0)
                || !safe_float(p.reload, 0.0, 60.0)
                || !safe_float(p.cadence, 0.0, 60.0)
                || p.magazine > 1000
                || !(0..=10000).contains(&p.damage)
                || !(0..=10000).contains(&p.melee_damage)
                || (i.slot == Slot::Ranged
                    && (p.magazine == 0
                        || i.npc.magazine == 0
                        || p.damage == 0
                        || p.cadence < 0.08
                        || p.reload < 0.1))
                || (i.slot == Slot::Melee && p.melee_damage == 0)
                || (index < 4
                    && (i.slot != Slot::ALL[index]
                        || i.tier != 0
                        || i.sell_value != 0
                        || i.buy_value != 0))
            {
                return Err(format!(
                    "Unsupported item or protected starter profile: {}",
                    i.name
                ));
            }
        }
        for slot in Slot::ALL {
            if !self
                .items
                .iter()
                .skip(4)
                .any(|item| item.slot == slot && item.tier == 1)
            {
                return Err(format!(
                    "Missing guaranteed opening shop/loot profile for {:?}",
                    slot
                ));
            }
        }
        Ok(())
    }
    pub fn enemy(&self, kind: EnemyKind) -> &EnemyDefinition {
        &self.enemies[kind as usize]
    }
    pub fn structure(&self, kind: StructureKind) -> &StructureDefinition {
        &self.structures[kind as usize]
    }
}
pub fn content() -> &'static Content {
    static CONTENT: OnceLock<Content> = OnceLock::new();
    CONTENT.get_or_init(|| {
        let (text, source) = match std::env::var_os("LASTLIGHT_CONTENT") {
            Some(path) => (
                {if std::fs::metadata(&path).is_ok_and(|m|m.len()>32*1024*1024){panic!("LASTLIGHT_CONTENT {path:?} exceeds the supported 32 MiB authoring limit");}std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("Could not read LASTLIGHT_CONTENT {path:?}: {error}")
                })},
                format!("{path:?}"),
            ),
            None => (
                include_str!("../content.json").to_owned(),
                "bundled content.json".to_owned(),
            ),
        };
        let data: Content = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!("Invalid encounter/equipment JSON from {source}: {error}")
        });
        data.validate().unwrap_or_else(|error| {
            panic!("Invalid encounter/equipment content from {source}: {error}")
        });
        data
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_overrides_reject_invalid_profiles_before_play() {
        let mut data = content().clone();
        data.items[0].personal.magazine = 0;
        let text = serde_json::to_string(&data).unwrap();
        assert!(Content::from_json(&text).is_err());
        data = content().clone();
        data.levels[0].boss_npc_hp = 0;
        assert!(data.validate().is_err());
    }
    #[test]
    fn authored_content_validates() {
        assert!(content().validate().is_ok());
    }
    #[test]
    fn authoring_rejects_missing_starters_reordered_profiles_and_overflow_weights() {
        let mut data = content().clone();
        data.items.clear();
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.enemies.swap(0, 1);
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.levels[0].enemies = vec![u32::MAX; 5];
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.levels[0].growth = f32::NAN;
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.items[0].personal.reload = f32::INFINITY;
        assert!(data.validate().is_err());
    }
    #[test]
    fn spawn_envelopes_are_bounded_and_increase() {
        for l in &content().levels {
            let end = (l.start_rate * (1.0 + l.growth)).min(l.rate_cap);
            assert!(end > l.start_rate);
            assert!(end <= l.rate_cap);
        }
    }
    #[test]
    fn every_slot_has_protected_starter_profile() {
        for (n, slot) in Slot::ALL.into_iter().enumerate() {
            assert_eq!(content().items[n].slot, slot);
            assert_eq!(content().items[n].tier, 0);
        }
    }
}
