use crate::catalog::{self, ContentError};
use crate::kind_map::StableKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
impl StableKind for EnemyKind {
    const ALL: &'static [Self] = &Self::ALL;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BossKind {
    Breacher,
    Caller,
    SiegeBrute,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    /// Player-facing structure name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Trap => "Trap",
            Self::Wall => "Wall",
            Self::Turret => "Turret",
        }
    }
}
impl StableKind for StructureKind {
    const ALL: &'static [Self] = &Self::ALL;
}

/// Stable authored identity of an item definition. Saves reference items by
/// this ID, never by catalogue position; [`Content::item_index`] resolves it.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemDefId(pub String);
impl ItemDefId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// ID of the definition at `index` in the loaded catalogue.
    pub fn at(index: usize) -> Self {
        content().items[index].id.clone()
    }
    /// Position in the loaded catalogue, if this ID is authored there.
    pub fn index(&self) -> Option<usize> {
        content().item_index(self)
    }
    pub fn definition(&self) -> Option<&'static ItemDefinition> {
        content().item(self)
    }
}
impl From<&str> for ItemDefId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}
impl std::fmt::Display for ItemDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
/// Serde adapter for a level reference: a catalogue position in memory (the
/// simulation indexes `content().levels` with it), the level's stable authored
/// ID on disk, so saves survive level reordering.
pub mod level_ref {
    use super::content;
    use serde::{Deserialize, Deserializer, Serializer, de, ser};

    pub fn serialize<S: Serializer>(index: &usize, serializer: S) -> Result<S::Ok, S::Error> {
        let level = content()
            .levels
            .get(*index)
            .ok_or_else(|| ser::Error::custom(format!("unknown level position {index}")))?;
        serializer.serialize_str(&level.id)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<usize, D::Error> {
        let id = String::deserialize(deserializer)?;
        content()
            .level_index(&id)
            .ok_or_else(|| de::Error::custom(format!("unknown level id {id}")))
    }
}
/// Authored content IDs are short lowercase ASCII slugs.
fn valid_content_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
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
    /// Player-facing policy name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Marksman => "Marksman",
            Self::Guardian => "Guardian",
            Self::BossFocus => "Boss focus",
        }
    }
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
    pub id: ItemDefId,
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
    pub id: String,
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
    pub id: String,
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
    /// Item ID -> catalogue position, rebuilt whenever a catalogue is parsed.
    #[serde(skip)]
    item_index: HashMap<ItemDefId, usize>,
}
impl Content {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        let mut content: Self =
            catalog::parse_validated("content", text, 32 * catalog::MIB, Self::validate)?;
        content.item_index = content
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| (item.id.clone(), index))
            .collect();
        Ok(content)
    }
    /// Catalogue position of an item ID. The prebuilt index is verified so an
    /// in-process edited clone still resolves correctly.
    pub fn item_index(&self, id: &ItemDefId) -> Option<usize> {
        self.item_index
            .get(id)
            .copied()
            .filter(|&index| self.items.get(index).is_some_and(|item| item.id == *id))
            .or_else(|| self.items.iter().position(|item| item.id == *id))
    }
    pub fn item(&self, id: &ItemDefId) -> Option<&ItemDefinition> {
        self.item_index(id).map(|index| &self.items[index])
    }
    /// Catalogue position of a level ID (five levels; a scan is cheapest).
    pub fn level_index(&self, id: &str) -> Option<usize> {
        self.levels.iter().position(|level| level.id == id)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_CONTENT` override.
    pub fn try_load() -> Result<Self, ContentError> {
        catalog::load(
            "content",
            32 * catalog::MIB,
            include_str!("../content.json"),
            Self::from_json_str,
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.levels.len() != 5 || self.items.len() < 4 || self.items.len() > 4096 {
            return Err("Expected five encounter templates and four protected starters".into());
        }
        let enemy_kinds: BTreeSet<_> = self.enemies.iter().map(|e| e.kind).collect();
        let structure_kinds: BTreeSet<_> = self.structures.iter().map(|s| s.kind).collect();
        if enemy_kinds.len() != self.enemies.len()
            || enemy_kinds.len() != EnemyKind::ALL.len()
            || structure_kinds.len() != self.structures.len()
            || structure_kinds.len() != StructureKind::ALL.len()
        {
            return Err("Every enemy and structure kind needs exactly one profile".into());
        }
        for (catalog, ids) in [
            ("level", self.levels.iter().map(|l| l.id.as_str()).collect()),
            ("item", self.items.iter().map(|i| i.id.as_str()).collect()),
            (
                "structure",
                self.structures
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>(),
            ),
        ] {
            let mut seen = BTreeSet::new();
            if let Some(id) = ids
                .into_iter()
                .find(|id| !valid_content_id(id) || !seen.insert(*id))
            {
                return Err(format!(
                    "Missing, malformed or duplicate {catalog} ID: {id:?}"
                ));
            }
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
        for e in &self.enemies {
            if e.name.trim().is_empty()
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
                return Err(format!("Invalid enemy profile: {}", e.name));
            }
        }
        for d in &self.structures {
            if (d.kind == StructureKind::Wall && !(0..=30).contains(&d.npc_skill))
                || (d.kind != StructureKind::Wall && !(3..=30).contains(&d.npc_skill))
            {
                return Err(format!("Invalid explicit NPC structure skill: {}", d.name));
            }
            if d.name.trim().is_empty()
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
                return Err(format!("Invalid structure profile: {}", d.name));
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
        self.enemies
            .iter()
            .find(|e| e.kind == kind)
            .expect("validated: every enemy kind has a profile")
    }
    pub fn structure(&self, kind: StructureKind) -> &StructureDefinition {
        self.structures
            .iter()
            .find(|s| s.kind == kind)
            .expect("validated: every structure kind has a profile")
    }
}
static CONTENT: OnceLock<Content> = OnceLock::new();
pub(crate) fn install() -> Result<&'static Content, ContentError> {
    catalog::install(&CONTENT, Content::try_load)
}
pub fn content() -> &'static Content {
    catalog::lazy(&CONTENT, "content", Content::try_load, || {
        Content::from_json_str(include_str!("../content.json"))
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
    fn authoring_accepts_reordered_profiles_but_rejects_bad_starters_and_weights() {
        let mut data = content().clone();
        data.items.clear();
        assert!(data.validate().is_err());
        // Profiles are keyed by kind, so order is free but every kind is required once.
        let mut data = content().clone();
        data.enemies.swap(0, 1);
        assert!(data.validate().is_ok());
        assert_eq!(
            data.enemy(EnemyKind::Runner),
            content().enemy(EnemyKind::Runner)
        );
        data.enemies[1] = data.enemies[0].clone();
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.structures.pop();
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.items[5].id = data.items[4].id.clone();
        assert!(data.validate().is_err());
        let mut data = content().clone();
        data.levels[0].id = "Old Road".into();
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
    fn item_ids_resolve_through_the_index_and_survive_edited_clones() {
        let content = content();
        for (index, item) in content.items.iter().enumerate() {
            assert_eq!(content.item_index(&item.id), Some(index));
        }
        assert_eq!(content.item_index(&"no_such_item".into()), None);
        let mut edited = content.clone();
        edited.items.swap(4, 5);
        assert_eq!(edited.item_index(&content.items[4].id), Some(5));
    }
    #[test]
    fn every_slot_has_protected_starter_profile() {
        for (n, slot) in Slot::ALL.into_iter().enumerate() {
            assert_eq!(content().items[n].slot, slot);
            assert_eq!(content().items[n].tier, 0);
        }
    }
}
