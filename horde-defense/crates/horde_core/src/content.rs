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
    /// Player-facing slot name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ranged => "Ranged",
            Self::Melee => "Melee",
            Self::Outfit => "Outfit",
            Self::Accessory => "Accessory",
        }
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
/// Shared, so cloning an item (a save snapshot of a 20,000-item stash, R58)
/// counts a reference instead of copying its ID. Serialized as a plain string.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemDefId(pub std::sync::Arc<str>);
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
        Self(id.into())
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
    /// Recovered only as loot: never offered in the shop or bought. Requires
    /// a droppable tier (1..=4) on a non-starter that is not NPC-only.
    #[serde(default, skip_serializing_if = "crate::schema::is_false")]
    pub loot_only: bool,
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
/// Why a loot-only definition is never offered or bought.
pub const LOOT_ONLY_REASON: &str = "This equipment is only recovered as loot";
impl ItemDefinition {
    /// Sold in the shop: a non-starter tier that is not loot-only. The shop
    /// rows, the buy preflight, the recommender and the smoke and fixture
    /// buyers all filter on it.
    pub fn purchasable(&self) -> bool {
        self.tier > 0 && !self.loot_only
    }
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
    /// Extra trap charges per structure mastery rank (traps only; 0 elsewhere).
    /// Absent means 0. Placement reads it through [`Self::charges_at_rank`].
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub charges_per_rank: u32,
    pub blueprint_price: u64,
    pub unlock_clear: usize,
    pub npc: NpcProfile,
    #[serde(default = "default_structure_npc_skill")]
    pub npc_skill: i32,
}
impl StructureDefinition {
    /// Charges a structure placed at structure mastery `rank` starts with:
    /// `charges + charges_per_rank × rank` (ruling E12). Unchanged at every
    /// rank while `charges_per_rank` is 0, as for the shipped wall and
    /// turret; the shipped shrapnel trap authors 1 (W4-03).
    pub fn charges_at_rank(&self, rank: u32) -> u32 {
        self.charges
            .saturating_add(self.charges_per_rank.saturating_mul(rank))
    }
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
    /// The highest tier the drop pool holds (non-starter, not NPC issue): a
    /// saved `Attempt::drop_tier` lies in `1..=max_drop_tier()`.
    pub fn max_drop_tier(&self) -> u8 {
        self.items
            .iter()
            .skip(4)
            .filter(|item| !item.npc_only)
            .map(|item| u8::try_from(item.tier).unwrap_or(u8::MAX))
            .max()
            .unwrap_or(0)
    }
    /// The v7 drop pool (`drop_tier` Some): ascending catalogue indices from
    /// 4 on whose tier lies in `1..=max_tier`, loot-only items included and
    /// NPC issue never (design-loot-economy section 3). The legacy pool
    /// (`drop_tier` None, ruling E11) stays combat.rs's verbatim filter over
    /// the 29 v1 definitions.
    pub fn drop_pool(&self, max_tier: usize) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(n, i)| *n >= 4 && !i.npc_only && (1..=max_tier).contains(&i.tier))
            .map(|(n, _)| n)
            .collect()
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
                // Rank 10 (the StructureRank bound) stays within the trap limit.
                || (d.kind != StructureKind::Trap && d.charges_per_rank != 0)
                || (d.kind == StructureKind::Trap && d.charges_at_rank(10) > 1000)
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
            if i.loot_only && (index < 4 || i.npc_only || !(1..=4).contains(&i.tier)) {
                return Err(format!("Loot-only item needs a drop tier: {}", i.name));
            }
        }
        // The opening shop sells a tier-1 item for every slot; a loot-only
        // item never counts, since it is never offered (W2-08), nor does NPC
        // issue, which the drop pool skips. A purchasable non-NPC tier-1 item
        // from index 4 on is also in the tier-1 drop pool.
        for slot in Slot::ALL {
            if !self.items.iter().skip(4).any(|item| {
                item.slot == slot && item.tier == 1 && item.purchasable() && !item.npc_only
            }) {
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
    fn loot_only_items_need_a_drop_tier_and_are_never_purchasable() {
        // Exactly the 39 appended items (29-67, W2-08) are loot-only; the
        // v1 rows keep the key skipped at false and their tier gate.
        for (index, item) in content().items.iter().enumerate() {
            assert_eq!(item.loot_only, index >= 29, "{}", item.id);
            assert_eq!(
                item.purchasable(),
                item.tier > 0 && index < 29,
                "{}",
                item.id
            );
        }
        assert_eq!(content().max_drop_tier(), 4);
        let row = |n: usize| serde_json::to_string(&content().items[n]).unwrap();
        assert!(!row(5).contains("loot_only"), "skipped while false");
        assert!(row(29).contains(r#""loot_only":true"#));
        // A loot-only item beside a purchasable tier-1 item of its slot
        // validates and round-trips (tier-1 Ranged keeps 4 and 22-28).
        let mut data = content().clone();
        data.items[21].loot_only = true;
        assert!(!data.items[21].purchasable());
        assert!(data.validate().is_ok());
        let text = serde_json::to_string(&data).unwrap();
        assert_eq!(Content::from_json(&text).unwrap().items[21], data.items[21]);
        // The opening shop needs a purchasable tier-1 item per slot: with
        // fire_axe (5) loot-only, every tier-1 Melee (5, 29, 30) is loot-only,
        // so the shop would offer no Melee. Validation refuses it at load.
        let mut data = content().clone();
        data.items[5].loot_only = true;
        assert!([5, 29, 30].iter().all(|&n| data.items[n].loot_only));
        match data.validate() {
            Err(error) => assert!(error.contains("Missing guaranteed opening shop"), "{error}"),
            Ok(()) => panic!("a slot with no purchasable tier-1 item validated"),
        }
        // NPC issue is purchasable yet never in the drop pool, so it never
        // counts: with fire_axe (5) NPC-only and the loot-only Melee (29, 30)
        // moved to tier 2, the tier-1 drop pool holds no Melee.
        let mut data = content().clone();
        data.items[5].npc_only = true;
        data.items[29].tier = 2;
        data.items[30].tier = 2;
        assert!(data.items[5].purchasable());
        assert!(
            data.drop_pool(1)
                .iter()
                .all(|&n| data.items[n].slot != Slot::Melee)
        );
        match data.validate() {
            Err(error) => assert!(error.contains("Missing guaranteed opening shop"), "{error}"),
            Ok(()) => panic!("an NPC-only tier-1 item counted toward the guarantee"),
        }
        // Loot-only gear must be droppable: tier 1..=4, never a starter and
        // never NPC-only (the drop pool skips both). Each row keeps the
        // per-slot tier-1 guarantee, so only the loot-only rule can fire:
        // a starter, a tier-0 non-starter (worn_service_pistol), a tier-5
        // item (checkpoint_rifle) and an NPC-only drop-tier item.
        for (index, tier, npc_only) in
            [(0, 0, false), (20, 0, false), (16, 5, false), (16, 4, true)]
        {
            let mut data = content().clone();
            data.items[index].loot_only = true;
            data.items[index].tier = tier;
            data.items[index].npc_only = npc_only;
            let row = format!("item {index} at tier {tier}, npc_only {npc_only}");
            match data.validate() {
                Err(error) => assert!(
                    error.contains("Loot-only item needs a drop tier"),
                    "{row}: {error}"
                ),
                Ok(()) => panic!("{row}: validated"),
            }
        }
    }
    #[test]
    fn rejects_charges_per_rank_on_a_wall() {
        // Since W4-03 only the shrapnel trap gains charges per rank (1); the
        // key is skipped at 0, so walls and turrets serialize without it.
        for d in &content().structures {
            let expected = u32::from(d.kind == StructureKind::Trap);
            assert_eq!(d.charges_per_rank, expected, "{}", d.id);
        }
        let text = serde_json::to_string(content()).unwrap();
        assert_eq!(text.matches("charges_per_rank").count(), 1, "skipped at 0");
        let mut flat = content().clone();
        for d in &mut flat.structures {
            d.charges_per_rank = 0;
        }
        let text = serde_json::to_string(&flat).unwrap();
        assert!(!text.contains("charges_per_rank"), "skipped at 0");
        let index = |kind| {
            content()
                .structures
                .iter()
                .position(|d| d.kind == kind)
                .unwrap()
        };
        // A trap may gain charges per rank, bounded so rank 10 stays at or
        // below the 1000-charge trap limit; the value round-trips.
        let trap = index(StructureKind::Trap);
        let mut data = content().clone();
        data.structures[trap].charges_per_rank = 1;
        assert!(data.validate().is_ok());
        let text = serde_json::to_string(&data).unwrap();
        assert!(text.contains(r#""charges_per_rank":1"#));
        assert_eq!(
            Content::from_json(&text).unwrap().structures[trap],
            data.structures[trap]
        );
        let charges = data.structures[trap].charges;
        data.structures[trap].charges_per_rank = (1000 - charges) / 10;
        assert!(data.validate().is_ok());
        data.structures[trap].charges_per_rank += 1;
        assert_eq!(
            data.validate(),
            Err(format!(
                "Invalid structure profile: {}",
                data.structures[trap].name
            ))
        );
        // Walls and turrets have no charges to scale.
        for kind in [StructureKind::Wall, StructureKind::Turret] {
            let mut data = content().clone();
            let n = index(kind);
            data.structures[n].charges_per_rank = 1;
            assert_eq!(
                data.validate(),
                Err(format!(
                    "Invalid structure profile: {}",
                    data.structures[n].name
                )),
                "{kind:?}"
            );
        }
    }
    #[test]
    fn every_slot_has_protected_starter_profile() {
        for (n, slot) in Slot::ALL.into_iter().enumerate() {
            assert_eq!(content().items[n].slot, slot);
            assert_eq!(content().items[n].tier, 0);
        }
    }

    /// Every catalogue id in order: the 29 v1 definitions (save format v1
    /// stored `definition: n` against this order) and the 39 `loot_only`
    /// items appended at indices 29-67 (design-loot-economy §5).
    const CATALOGUE_IDS: [&str; 68] = [
        "service_carbine",
        "crowbar",
        "work_jacket",
        "field_webbing",
        "marksman_rifle",
        "fire_axe",
        "padded_vest",
        "first_aid_rig",
        "patrol_rifle",
        "forged_machete",
        "scavenged_ballistic_vest",
        "rangefinder",
        "longwatch_rifle",
        "breach_hammer",
        "riot_armor",
        "survival_harness",
        "checkpoint_rifle",
        "tempered_saber",
        "fortress_armor",
        "veteran_kit",
        "worn_service_pistol",
        "compact_service_pistol",
        "compact_smg",
        "pump_shotgun",
        "patrol_rifle_tier1",
        "scout_marksman_rifle",
        "belt_fed_machine_gun",
        "heavy_revolver",
        "double_barrel_shotgun",
        "field_hatchet",
        "pipe_wrench",
        "canvas_coat",
        "leather_jacket",
        "ammo_pouch",
        "signal_radio",
        "duty_pistol",
        "patrol_smg",
        "riot_shotgun",
        "designated_marksman_rifle",
        "squad_machine_gun",
        "reinforced_axe",
        "sledgehammer",
        "kevlar_jacket",
        "plated_vest",
        "medic_satchel",
        "scout_satchel",
        "match_pistol",
        "suppressed_smg",
        "combat_shotgun",
        "precision_marksman_rifle",
        "general_purpose_machine_gun",
        "war_machete",
        "breaching_axe",
        "tactical_vest",
        "ceramic_plate_carrier",
        "trauma_kit",
        "command_radio",
        "custom_pistol",
        "elite_smg",
        "auto_shotgun",
        "longshot_marksman_rifle",
        "heavy_machine_gun",
        "officer_saber",
        "demolition_hammer",
        "bastion_armor",
        "reactive_vest",
        "quartermaster_rig",
        "expedition_satchel",
    ];
    fn catalogue_order(data: &Content) -> Result<(), String> {
        let ids: Vec<&str> = data.items.iter().map(|item| item.id.as_str()).collect();
        if ids != CATALOGUE_IDS {
            return Err(format!("catalogue ids moved: {ids:?}"));
        }
        for (index, item) in data.items.iter().enumerate() {
            if item.loot_only != (index >= 29) {
                return Err(format!("{} loot_only {}", item.id, item.loot_only));
            }
        }
        Ok(())
    }
    #[test]
    fn appended_ids_keep_v1_order() {
        // Ids never change and content is append-only (ISO-3): the v1 prefix
        // keeps its order and exactly the appended items are loot-only.
        assert_eq!(catalogue_order(content()), Ok(()));
        // Mutation proof: swapping two appended ids, or dropping the
        // loot-only flag from one appended item, fails the pin.
        let mut swapped = content().clone();
        swapped.items.swap(35, 36);
        assert!(catalogue_order(&swapped).is_err());
        let mut flagged = content().clone();
        flagged.items[29].loot_only = false;
        assert!(catalogue_order(&flagged).is_err());
    }
    #[test]
    fn every_tier_1_to_4_has_one_ranged_per_class_and_three_per_other_slot() {
        // The drop pool the Some path draws from (`Content::drop_pool`: index
        // 4 on, never NPC issue) offers, at every tier, each weapon class and
        // at least three items of every other slot.
        let items = &content().items;
        let droppable = |tier: usize| {
            content()
                .drop_pool(tier)
                .into_iter()
                .map(|n| &items[n])
                .filter(move |item| item.tier == tier)
        };
        for tier in 1..=4 {
            for class in crate::weapons::WeaponClass::ALL {
                assert!(
                    droppable(tier).any(|item| item.slot == Slot::Ranged
                        && crate::weapons::WeaponSpec::from_item(item).class == class),
                    "tier {tier} has no {class:?}"
                );
            }
            for slot in [Slot::Melee, Slot::Outfit, Slot::Accessory] {
                assert!(
                    droppable(tier).filter(|item| item.slot == slot).count() >= 3,
                    "tier {tier} has under three {slot:?} items"
                );
            }
        }
        // Droppable definitions per drop tier (12 / 16 / 20 / 24 before
        // W2-08), counted on the pool itself.
        let pool = |drop_tier: usize| content().drop_pool(drop_tier).len();
        assert_eq!([1, 2, 3, 4].map(pool), [18, 33, 48, 63]);
        assert_eq!(content().max_drop_tier(), 4);
        // The pool skips NPC issue and starters: an NPC-only tier-1 item
        // from index 4 on leaves it, and tier 0 never enters.
        let mut edited = content().clone();
        edited.items[29].npc_only = true;
        let tier1 = edited.drop_pool(1);
        assert_eq!(tier1.len(), 17);
        assert!(!tier1.contains(&29));
        assert!(
            content()
                .drop_pool(4)
                .iter()
                .all(|&n| n >= 4 && items[n].tier > 0)
        );
    }
    #[test]
    fn loot_only_ranged_items_are_their_exemplar_at_the_tier_quality() {
        use crate::quality::Quality;
        // (index, exemplar, Personal damage, NPC dice, NPC adds, sell, buy):
        // `quality::apply(exemplar, B_T)` with B_T Rare / Epic / Legendary
        // for tiers 2 / 3 / 4; sell = round(exemplar sell x {36, 56, 80} / 23)
        // and buy = round(exemplar buy x {120, 220, 340} / 70). The JSON
        // holds what `apply` returns: Squad machine gun is 33, since 25 x
        // 1.3 is 32.5 in f32 and rounds away from zero (the design table
        // rounded it down to 32).
        let rows: [(usize, usize, i32, u8, i32, u64, u64); 15] = [
            (35, 21, 26, 2, 3, 17, 60),
            (36, 22, 21, 3, 3, 28, 94),
            (37, 23, 13, 5, 5, 31, 103),
            (38, 25, 108, 7, 9, 44, 146),
            (39, 26, 33, 5, 5, 49, 163),
            (46, 21, 31, 2, 5, 27, 110),
            (47, 22, 25, 3, 6, 44, 173),
            (48, 23, 16, 5, 10, 49, 189),
            (49, 25, 129, 7, 15, 68, 267),
            (50, 26, 39, 5, 10, 75, 299),
            (57, 21, 38, 2, 8, 38, 170),
            (58, 22, 30, 3, 9, 63, 267),
            (59, 23, 19, 5, 16, 70, 291),
            (60, 25, 158, 7, 24, 97, 413),
            (61, 26, 48, 5, 16, 108, 461),
        ];
        for (index, exemplar, damage, dice, adds, sell, buy) in rows {
            let item = &content().items[index];
            let (base, rarity, sell_ratio, buy_ratio) = match item.tier {
                2 => (Quality::Rare, "Rare", 36.0, 120.0),
                3 => (Quality::Epic, "Exceptional", 56.0, 220.0),
                4 => (Quality::Legendary, "Masterwork", 80.0, 340.0),
                tier => panic!("{} at tier {tier}", item.id),
            };
            let from = &content().items[exemplar];
            let mut expected = crate::quality::apply(from.clone(), Some(base));
            expected.id = item.id.clone();
            expected.name = item.name.clone();
            expected.tier = item.tier;
            expected.rarity = rarity.into();
            expected.base_quality = base;
            expected.loot_only = true;
            expected.sell_value = (from.sell_value as f64 * sell_ratio / 23.0).round() as u64;
            expected.buy_value = (from.buy_value as f64 * buy_ratio / 70.0).round() as u64;
            assert_eq!(item, &expected, "{}", item.id);
            assert_eq!(
                (
                    item.personal.damage,
                    item.npc.damage_dice,
                    item.npc.damage_add,
                    item.sell_value,
                    item.buy_value
                ),
                (damage, dice, adds, sell, buy),
                "{}",
                item.id
            );
        }
    }
    #[test]
    fn loot_only_non_ranged_items_are_sidegrades_of_their_slot_anchor() {
        // Each copies its anchor (the lowest-index item of its slot and tier)
        // in quality, rarity and price, and is Pareto non-dominated against
        // it: better in one stat, worse in another.
        let stats = |item: &ItemDefinition| {
            let p = &item.personal;
            let mut values = vec![
                p.melee_damage as f32,
                p.armor as f32,
                p.health as f32,
                p.stamina as f32,
                p.weak_bonus,
                item.npc.armor as f32,
                item.npc.accuracy as f32,
            ];
            if item.slot == Slot::Melee {
                values.push(crate::campaign::mean_basic_damage(crate::rules::Damage {
                    dice: item.npc.damage_dice,
                    adds: item.npc.damage_add,
                    kind: item.npc.damage_kind,
                }));
            }
            values
        };
        let mut count = 0;
        for item in content()
            .items
            .iter()
            .filter(|i| i.loot_only && i.slot != Slot::Ranged)
        {
            let anchor = content()
                .items
                .iter()
                .skip(4)
                .find(|a| a.slot == item.slot && a.tier == item.tier)
                .unwrap();
            assert!(!anchor.loot_only, "{}", item.id);
            assert_eq!(
                (
                    item.base_quality,
                    &item.rarity,
                    item.sell_value,
                    item.buy_value
                ),
                (
                    anchor.base_quality,
                    &anchor.rarity,
                    anchor.sell_value,
                    anchor.buy_value
                ),
                "{}",
                item.id
            );
            let (mine, theirs) = (stats(item), stats(anchor));
            let better = mine.iter().zip(&theirs).any(|(m, t)| m > t);
            let worse = mine.iter().zip(&theirs).any(|(m, t)| m < t);
            assert!(better && worse, "{} against {}", item.id, anchor.id);
            assert!(!item.visual_style.is_empty(), "{}", item.id);
            count += 1;
        }
        assert_eq!(count, 24);
    }
}
