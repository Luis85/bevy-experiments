//! Validated, engine-free enemy composition. Resolved profiles are save snapshots.
use crate::catalog::{self, ContentError};
use crate::content::{BossKind, EnemyKind};
use crate::kind_map::KindMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORDS: usize = 4096;
fn one_multiplier() -> f32 {
    1.0
}
fn one_structure_multiplier() -> u8 {
    1
}
const MAX_TARGET_URGENCY: f32 = 100.0;
/// Target urgency a legacy category implies when an archetype authors none,
/// so catalogues written before the field keep their targeting behaviour.
fn legacy_target_urgency(kind: EnemyKind) -> f32 {
    if kind == EnemyKind::Runner { -3.0 } else { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualSprite {
    Shambler,
    Runner,
    Brute,
    Armored,
    Demolisher,
    Breacher,
    Caller,
    SiegeBrute,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyVisual {
    pub sprite: VisualSprite,
    /// RGBA tint channels in 0..=1; interpreted by presentation.
    pub tint: [f32; 4],
    pub scale: f32,
}

/// A complete immutable-at-spawn profile. Keep this in live/save enemy state so
/// catalogue edits affect future spawns only, including across save/resume.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedEnemy {
    pub archetype_id: String,
    /// Legacy category, never an exhaustive list of authored archetypes.
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
    #[serde(default = "one_structure_multiplier")]
    pub structure_damage_multiplier: u8,
    #[serde(default = "one_multiplier")]
    pub contact_armor_fraction: f32,
    /// Added to an NPC defender's target urgency (yards of distance); negative
    /// values are engaged first. Saves before v3 derive it from `kind`.
    pub target_urgency: f32,
    pub visual: EnemyVisual,
    pub unlocked_by: Option<BossKind>,
    /// Ranged attack reach in yards beyond the defense line; 0 is melee.
    #[serde(default, skip_serializing_if = "crate::schema::is_zero_f32")]
    pub reach: f32,
    /// Personal armor added against shots and turret fire while not in contact.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub ranged_armor: i32,
    /// NPC damage resistance added against ranged fire while not in contact.
    #[serde(default, skip_serializing_if = "crate::schema::is_default")]
    pub npc_ranged_armor: i32,
    /// Hits never interrupt movement or attacks (no hit-stop, prone or stun).
    #[serde(default, skip_serializing_if = "crate::schema::is_false")]
    pub stagger_immune: bool,
    /// Members spawned by one weighted pick.
    #[serde(
        default = "crate::schema::one_u8",
        skip_serializing_if = "crate::schema::is_one_u8"
    )]
    pub group_size: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_effect: Option<DeathEffect>,
    /// Authored labels for perk targeting; never used for numbers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Set on an elite spawn; its affix stats are already baked into the
    /// other fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elite: Option<EliteMark>,
}

/// The largest death-burst radius in yards (schema v7 §3.9).
pub const MAX_BURST_RADIUS: f32 = 3.0;

/// What an archetype does when it dies (schema v7 §3.9). Saved inside the
/// spawn snapshot, so a split target need not exist in the current catalogue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DeathEffect {
    /// A blast around the body; with `fuse` it also detonates on contact.
    Burst {
        /// Yards, in (0, 3].
        radius: f32,
        damage: i32,
        structure_damage: i32,
        npc_dice: u8,
        npc_add: i32,
        fuse: bool,
    },
    /// Spawns `count` children of `archetype_id`.
    Split { archetype_id: String, count: u8 },
}

impl DeathEffect {
    fn valid(&self) -> bool {
        match self {
            Self::Burst {
                radius,
                damage,
                structure_damage,
                npc_dice,
                npc_add,
                fuse: _,
            } => {
                radius.is_finite()
                    && *radius > 0.0
                    && *radius <= MAX_BURST_RADIUS
                    && (1..=10_000).contains(damage)
                    && (0..=10_000).contains(structure_damage)
                    && (1..=20).contains(npc_dice)
                    && (-30..=100).contains(npc_add)
            }
            Self::Split {
                archetype_id,
                count,
            } => valid_id(archetype_id) && (1..=4).contains(count),
        }
    }
}

/// Marks an elite spawn: the affix it rolled and its display name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EliteMark {
    pub affix_id: String,
    pub name: String,
}

/// The most authored tags one archetype may carry.
pub const MAX_ENEMY_TAGS: usize = 8;
const MAX_ELITE_NAME_BYTES: usize = 60;
/// The most authored elite affixes.
pub const MAX_ELITE_AFFIXES: usize = 16;
const MAX_AFFIX_NAME_BYTES: usize = 24;

/// What every elite gains before its affix (design enemy-balance §5 "Base
/// multipliers"). The neutral default (all 1) is what a catalogue without
/// the key gets; it fields no elites anyway, because it has no affixes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EliteBase {
    /// Health and NPC hit points, 1..=4.
    pub health: f32,
    /// Coin reward factor, 1..=10.
    pub reward: u32,
    /// Experience factor, 1..=10.
    pub xp: u32,
    /// Visual scale, 1..=1.5.
    pub scale: f32,
}
impl Default for EliteBase {
    fn default() -> Self {
        Self {
            health: 1.0,
            reward: 1,
            xp: 1,
            scale: 1.0,
        }
    }
}

/// One elite affix (design enemy-balance §5 "Affixes"): a weighted pick
/// whose stats are baked into the spawn snapshot by
/// [`ResolvedEnemy::with_affix`]. Multipliers default to 1 and additions
/// to 0. Personal reads the multipliers and the Personal armor terms; NPC
/// battles read the `npc_` additions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EliteAffix {
    pub id: String,
    /// Display word, prefixed to the archetype's name in the mark.
    pub name: String,
    /// Pick weight, 1..=10000.
    pub weight: u32,
    /// Personal health, 1..=4.
    #[serde(default = "one_multiplier")]
    pub health: f32,
    /// Personal damage, 1..=4.
    #[serde(default = "one_multiplier")]
    pub damage: f32,
    /// Speed, 1..=2.
    #[serde(default = "one_multiplier")]
    pub speed: f32,
    /// Windup, 0.5..=1.
    #[serde(default = "one_multiplier")]
    pub windup: f32,
    /// Personal armor added, 0..=100.
    #[serde(default)]
    pub armor: i32,
    /// Personal ranged armor added, 0..=100.
    #[serde(default)]
    pub ranged_armor: i32,
    /// NPC hit points added, 0..=100.
    #[serde(default)]
    pub npc_hp: i32,
    /// NPC armor added, 0..=20.
    #[serde(default)]
    pub npc_armor: i32,
    /// NPC skill added, 0..=10.
    #[serde(default)]
    pub npc_skill: i32,
    /// NPC dodge added, 0..=10.
    #[serde(default)]
    pub npc_dodge: i32,
    /// NPC damage added, 0..=20.
    #[serde(default)]
    pub npc_damage_add: i32,
    /// NPC ranged armor added, 0..=20.
    #[serde(default)]
    pub npc_ranged_armor: i32,
    /// A death effect the affix adds; offered only to archetypes without
    /// one of their own.
    #[serde(default)]
    pub death_effect: Option<DeathEffect>,
}
impl EliteAffix {
    fn valid(&self) -> bool {
        valid_id(&self.id)
            && !self.name.trim().is_empty()
            && self.name.len() <= MAX_AFFIX_NAME_BYTES
            && (1..=10_000).contains(&self.weight)
            && finite_range(self.health, 1.0, 4.0)
            && finite_range(self.damage, 1.0, 4.0)
            && finite_range(self.speed, 1.0, 2.0)
            && finite_range(self.windup, 0.5, 1.0)
            && (0..=100).contains(&self.armor)
            && (0..=100).contains(&self.ranged_armor)
            && (0..=100).contains(&self.npc_hp)
            && (0..=20).contains(&self.npc_armor)
            && (0..=10).contains(&self.npc_skill)
            && (0..=10).contains(&self.npc_dodge)
            && (0..=20).contains(&self.npc_damage_add)
            && (0..=20).contains(&self.npc_ranged_armor)
            && self.death_effect.as_ref().is_none_or(DeathEffect::valid)
    }
    /// Whether `base` may roll this affix: never a second death effect.
    fn offered_to(&self, base: &ResolvedEnemy) -> bool {
        self.death_effect.is_none() || base.death_effect.is_none()
    }
}

impl ResolvedEnemy {
    /// Validate saved snapshots without consulting the current catalogue.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.archetype_id)
            || self.name.trim().is_empty()
            || self.name.len() > 120
            || !(1..=100_000).contains(&self.health)
            || !(1..=1000).contains(&self.npc_hp)
            || !(3..=30).contains(&self.npc_ht)
            || !finite_range(self.speed, 0.01, 10.0)
            || !(1..=10_000).contains(&self.damage)
            || !finite_range(self.windup, 0.5, 60.0)
            || !(3..=30).contains(&self.npc_skill)
            || !(1..=20).contains(&self.npc_damage_dice)
            || !(-30..=100).contains(&self.npc_damage_add)
            || !(1..=8).contains(&self.structure_damage_multiplier)
            || !finite_range(self.contact_armor_fraction, 0.0, 1.0)
            || !finite_range(self.target_urgency, -MAX_TARGET_URGENCY, MAX_TARGET_URGENCY)
            || !(0..=1000).contains(&self.armor)
            || !(0..=100).contains(&self.npc_armor)
            || !(0..=20).contains(&self.npc_dodge)
            || !(1..=1_000_000).contains(&self.reward)
            || self.xp > 1_000_000
            || !finite_range(self.visual.scale, 0.25, 3.0)
            || self.visual.tint.iter().any(|v| !finite_range(*v, 0.0, 1.0))
            || !finite_range(self.reach, 0.0, 12.0)
            || !(0..=1000).contains(&self.ranged_armor)
            || !(0..=100).contains(&self.npc_ranged_armor)
            || !(1..=6).contains(&self.group_size)
            || self.death_effect.as_ref().is_some_and(|e| !e.valid())
            || self.tags.len() > MAX_ENEMY_TAGS
            || self.tags.iter().any(|tag| !valid_id(tag))
            || self.tags.iter().collect::<BTreeSet<_>>().len() != self.tags.len()
            || self.elite.as_ref().is_some_and(|elite| {
                !valid_id(&elite.affix_id)
                    || elite.name.trim().is_empty()
                    || elite.name.len() > MAX_ELITE_NAME_BYTES
            })
        {
            return Err(format!(
                "Invalid resolved enemy profile: {}",
                self.archetype_id
            ));
        }
        Ok(())
    }

    /// This profile as an elite with `affix` (design enemy-balance §5
    /// "Bake"): the base multipliers and the affix are baked into the
    /// stats, so every sim read of the snapshot is unchanged, and `elite`
    /// marks it. Personal health is `round(health x base x affix)`, NPC hit
    /// points `round(npc_hp x base) + npc_hp add`; reward and xp multiply,
    /// the visual scales, and the affix's death effect is added. The mark
    /// names "<affix> <archetype>", or the affix alone past 60 bytes.
    pub fn with_affix(&self, base: &EliteBase, affix: &EliteAffix) -> ResolvedEnemy {
        let scaled = |value: i32, factor: f32| (value as f32 * factor).round().max(1.0) as i32;
        let full = format!("{} {}", affix.name, self.name);
        let name = if full.len() <= MAX_ELITE_NAME_BYTES {
            full
        } else {
            affix.name.clone()
        };
        let mut elite = self.clone();
        elite.health = scaled(self.health, base.health * affix.health);
        elite.npc_hp = scaled(self.npc_hp, base.health) + affix.npc_hp;
        elite.reward = self.reward.saturating_mul(u64::from(base.reward));
        elite.xp = self.xp.saturating_mul(u64::from(base.xp));
        elite.visual.scale = self.visual.scale * base.scale;
        elite.damage = scaled(self.damage, affix.damage);
        elite.speed = self.speed * affix.speed;
        elite.windup = self.windup * affix.windup;
        elite.armor += affix.armor;
        elite.ranged_armor += affix.ranged_armor;
        elite.npc_armor += affix.npc_armor;
        elite.npc_skill += affix.npc_skill;
        elite.npc_dodge += affix.npc_dodge;
        elite.npc_damage_add += affix.npc_damage_add;
        elite.npc_ranged_armor += affix.npc_ranged_armor;
        if affix.death_effect.is_some() {
            elite.death_effect = affix.death_effect.clone();
        }
        elite.elite = Some(EliteMark {
            affix_id: affix.id.clone(),
            name,
        });
        elite
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedEnemy {
    pub archetype_id: String,
    pub weight: u32,
}

/// Only constructed after every component, reference, default and wave validates.
#[derive(Clone, Debug)]
pub struct EnemyCatalog {
    resolved: BTreeMap<String, ResolvedEnemy>,
    defaults: KindMap<EnemyKind, String>,
    waves: Vec<Vec<WeightedEnemy>>,
    /// Archetypes authored `elite_eligible: false` (catalogue only).
    elite_ineligible: BTreeSet<String>,
    elite_base: EliteBase,
    /// Authored order; empty means no elites.
    elite_affixes: Vec<EliteAffix>,
    /// The largest health any elite can spawn with, (Personal, NPC); 0
    /// without affixes.
    elite_maximum_health: (i32, i32),
    /// One phase profile per boss kind, in authored order; empty means no
    /// boss phases.
    boss_phases: Vec<(BossKind, crate::sim::BossPhaseProfile)>,
}

impl EnemyCatalog {
    /// The number of authored wave tiers; `wave_pool` levels at or past it
    /// wrap, so every progression floor's tier must be below it (checked by
    /// [`crate::progression::ProgressionCatalog::check_wave_tiers`]).
    pub fn wave_count(&self) -> usize {
        self.waves.len()
    }

    /// Whether a director wave spawn of `id` may roll elite. Unknown ids are
    /// not eligible.
    pub fn elite_eligible(&self, id: &str) -> bool {
        self.resolved.contains_key(id) && !self.elite_ineligible.contains(id)
    }

    /// The phase profile of `kind`'s boss (design enemy-balance §6, one
    /// per kind); None when the catalogue authors no phases.
    pub fn boss_phase(&self, kind: BossKind) -> Option<&crate::sim::BossPhaseProfile> {
        self.boss_phases
            .iter()
            .find(|(boss, _)| *boss == kind)
            .map(|(_, profile)| profile)
    }

    /// The authored elite affix ids, in table order.
    pub fn elite_affix_ids(&self) -> impl Iterator<Item = &str> {
        self.elite_affixes.iter().map(|affix| affix.id.as_str())
    }

    /// `base` as an elite with the affix `key` picks (design enemy-balance
    /// §5 "Roll"): `key` modulo the weight of the affixes offered to it, in
    /// table order. None when nothing is offered or `base` is already an
    /// elite. Pure: the caller supplies the keyed AFFIX hash.
    pub fn elite_profile(&self, base: &ResolvedEnemy, key: u64) -> Option<ResolvedEnemy> {
        if base.elite.is_some() {
            return None;
        }
        let offered = || self.elite_affixes.iter().filter(|a| a.offered_to(base));
        let total: u64 = offered().map(|a| u64::from(a.weight)).sum();
        if total == 0 {
            return None;
        }
        let mut roll = key % total;
        for affix in offered() {
            if roll < u64::from(affix.weight) {
                return Some(base.with_affix(&self.elite_base, affix));
            }
            roll -= u64::from(affix.weight);
        }
        None
    }

    /// The largest health an elite of this catalogue can spawn with in
    /// `mode` (Personal health, NPC hit points), before encounter
    /// modifiers; 0 without affixes.
    pub(crate) fn maximum_elite_health(&self, mode: crate::content::Mode) -> i32 {
        if mode == crate::content::Mode::Personal {
            self.elite_maximum_health.0
        } else {
            self.elite_maximum_health.1
        }
    }

    /// The weighted mean hit points one pick from `wave_pool(tier, unlocked)`
    /// brings in `mode` (Personal health, NPC hit points). A pick counts
    /// every group member and every split child it will release. Pure: no
    /// RNG, no rank scaling.
    pub fn mean_pool_health(
        &self,
        tier: usize,
        unlocked: &[BossKind],
        mode: crate::content::Mode,
    ) -> f32 {
        let hp = |enemy: &ResolvedEnemy| {
            f64::from(if mode == crate::content::Mode::Personal {
                enemy.health
            } else {
                enemy.npc_hp
            })
        };
        let pick = |enemy: &ResolvedEnemy| {
            let children = match &enemy.death_effect {
                Some(DeathEffect::Split {
                    archetype_id,
                    count,
                }) => self
                    .resolved
                    .get(archetype_id)
                    .map_or(0.0, |child| f64::from(*count) * hp(child)),
                _ => 0.0,
            };
            f64::from(enemy.group_size) * hp(enemy) + children
        };
        let pool = self.wave_pool(tier, unlocked);
        let weight: f64 = pool.iter().map(|entry| f64::from(entry.weight)).sum();
        let total: f64 = pool
            .iter()
            .map(|entry| f64::from(entry.weight) * pick(&self.resolved[&entry.archetype_id]))
            .sum();
        (total / weight) as f32
    }

    /// The hostiles one pick from `wave_pool(tier, unlocked)` brings, as
    /// (members per unit of pick weight, profile) rows: every group member
    /// and every split child it will release, each child under its own
    /// profile. The second value is the pool's total pick weight. Borrowed,
    /// pure: no RNG, no rank scaling (the Npc `defender_power` model).
    pub(crate) fn pool_members(
        &self,
        tier: usize,
        unlocked: &[BossKind],
    ) -> (Vec<(f32, &ResolvedEnemy)>, f32) {
        let pool = self.wave_pool(tier, unlocked);
        let mut members = Vec::with_capacity(pool.len() + 2);
        let mut total = 0.0;
        for entry in &pool {
            let weight = entry.weight as f32;
            total += weight;
            let enemy = &self.resolved[&entry.archetype_id];
            members.push((weight * f32::from(enemy.group_size), enemy));
            if let Some(DeathEffect::Split {
                archetype_id,
                count,
            }) = &enemy.death_effect
                && let Some(child) = self.resolved.get(archetype_id)
            {
                members.push((weight * f32::from(*count), child));
            }
        }
        (members, total)
    }

    pub(crate) fn maximum_health(&self, mode: crate::content::Mode) -> i32 {
        self.resolved
            .values()
            .map(|enemy| {
                if mode == crate::content::Mode::Personal {
                    enemy.health
                } else {
                    enemy.npc_hp
                }
            })
            .max()
            .unwrap_or(1)
    }
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        if text.len() > MAX_JSON_BYTES {
            return Err(ContentError::TooLarge {
                catalog: "enemies",
                limit: MAX_JSON_BYTES as u64,
                actual: text.len() as u64,
            });
        }
        let authored: AuthoredCatalog =
            serde_json::from_str(text).map_err(|e| ContentError::Parse {
                catalog: "enemies",
                message: e.to_string(),
            })?;
        authored.resolve().map_err(|message| ContentError::Invalid {
            catalog: "enemies",
            message,
        })
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_ENEMIES` override.
    pub fn try_load() -> Result<Self, ContentError> {
        catalog::load(
            "enemies",
            MAX_JSON_BYTES as u64,
            include_str!("../enemy_catalog.json"),
            Self::from_json_str,
        )
    }

    /// Every resolved archetype ID, in stable sorted order.
    pub fn archetype_ids(&self) -> impl Iterator<Item = &str> {
        self.resolved.keys().map(String::as_str)
    }

    pub fn resolve(&self, id: &str) -> Result<ResolvedEnemy, String> {
        self.resolved
            .get(id)
            .cloned()
            .ok_or_else(|| format!("Unknown enemy archetype: {id}"))
    }

    pub fn default_profile(&self, kind: EnemyKind) -> &ResolvedEnemy {
        &self.resolved[self.default_for_kind(kind)]
    }

    pub fn default_for_kind(&self, kind: EnemyKind) -> &str {
        &self.defaults[kind]
    }

    /// Stable ID order, positive bounded weights, and at least one eligible entry.
    /// Levels cycle through the authored wave templates, like encounter templates.
    /// No RNG is consumed here; the caller performs one weighted selection.
    pub fn wave_pool(&self, level: usize, unlocked: &[BossKind]) -> Vec<WeightedEnemy> {
        self.waves[level % self.waves.len()]
            .iter()
            .filter(|entry| {
                self.resolved[&entry.archetype_id]
                    .unlocked_by
                    .is_none_or(|boss| unlocked.contains(&boss))
            })
            .cloned()
            .collect()
    }
}

static CATALOG: OnceLock<EnemyCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static EnemyCatalog, ContentError> {
    catalog::install(&CATALOG, EnemyCatalog::try_load)
}
pub fn enemy_catalog() -> &'static EnemyCatalog {
    catalog::lazy(&CATALOG, "enemies", EnemyCatalog::try_load, || {
        EnemyCatalog::from_json_str(include_str!("../enemy_catalog.json"))
    })
}

/// Compatibility path for pre-snapshot saves and explicit legacy-kind spawns.
pub fn resolve_kind(kind: EnemyKind) -> ResolvedEnemy {
    let catalog = enemy_catalog();
    catalog
        .resolve(catalog.default_for_kind(kind))
        .expect("validated enemy defaults")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    id: String,
    health: i32,
    npc_hp: i32,
    npc_ht: i32,
    /// See [`ResolvedEnemy::stagger_immune`].
    #[serde(default)]
    stagger_immune: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Movement {
    id: String,
    speed: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attack {
    id: String,
    #[serde(default = "one_structure_multiplier")]
    structure_damage_multiplier: u8,
    damage: i32,
    windup: f32,
    npc_skill: i32,
    npc_damage_dice: u8,
    npc_damage_add: i32,
    /// See [`ResolvedEnemy::reach`].
    #[serde(default)]
    reach: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Defense {
    id: String,
    #[serde(default = "one_multiplier")]
    contact_armor_fraction: f32,
    armor: i32,
    npc_armor: i32,
    npc_dodge: i32,
    /// See [`ResolvedEnemy::ranged_armor`].
    #[serde(default)]
    ranged_armor: i32,
    /// See [`ResolvedEnemy::npc_ranged_armor`].
    #[serde(default)]
    npc_ranged_armor: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reward {
    id: String,
    coins: u64,
    xp: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Visual {
    id: String,
    sprite: VisualSprite,
    tint: [f32; 4],
    scale: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Archetype {
    id: String,
    name: String,
    base_enemy_kind: EnemyKind,
    body: String,
    movement: String,
    attack: String,
    defense: String,
    reward: String,
    visual: String,
    unlocked_by: Option<BossKind>,
    /// See [`ResolvedEnemy::target_urgency`]; defaults from `base_enemy_kind`.
    #[serde(default)]
    target_urgency: Option<f32>,
    /// See [`ResolvedEnemy::death_effect`].
    #[serde(default)]
    death_effect: Option<DeathEffect>,
    /// See [`ResolvedEnemy::group_size`].
    #[serde(default = "crate::schema::one_u8")]
    group_size: u8,
    /// See [`ResolvedEnemy::tags`].
    #[serde(default)]
    tags: Vec<String>,
    /// Whether a director wave spawn may roll elite; catalogue only, never
    /// snapshotted (see [`EnemyCatalog::elite_eligible`]).
    #[serde(default = "elite_eligible_default")]
    elite_eligible: bool,
}
fn elite_eligible_default() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KindDefault {
    kind: EnemyKind,
    archetype_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wave {
    level: usize,
    entries: Vec<WeightedEnemy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredCatalog {
    bodies: Vec<Body>,
    movements: Vec<Movement>,
    attacks: Vec<Attack>,
    defenses: Vec<Defense>,
    rewards: Vec<Reward>,
    visuals: Vec<Visual>,
    archetypes: Vec<Archetype>,
    defaults: Vec<KindDefault>,
    waves: Vec<Wave>,
    /// See [`EliteBase`]; absent is neutral.
    #[serde(default)]
    elite_base: EliteBase,
    /// See [`EliteAffix`]; absent means no elites.
    #[serde(default)]
    elite_affixes: Vec<EliteAffix>,
    /// See [`AuthoredBossPhase`]; absent means no boss phases.
    #[serde(default)]
    boss_phases: Vec<AuthoredBossPhase>,
}

/// One boss kind's phase profile as authored: exactly one per
/// [`BossKind`] when the table is present (design enemy-balance §6).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthoredBossPhase {
    boss: BossKind,
    name: String,
    speed_mult: f32,
    armor_mult: f32,
    call_interval_mult: f32,
    siege_recovery_mult: f32,
    hold_ticks: u32,
}

/// Every profile within its bounds, and either no table or exactly one
/// profile for each boss kind.
fn check_boss_phases(
    authored: Vec<AuthoredBossPhase>,
) -> Result<Vec<(BossKind, crate::sim::BossPhaseProfile)>, String> {
    const KINDS: [BossKind; 3] = [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute];
    let mut phases: Vec<(BossKind, crate::sim::BossPhaseProfile)> = Vec::new();
    for phase in authored {
        let profile = crate::sim::BossPhaseProfile {
            name: phase.name,
            speed_mult: phase.speed_mult,
            armor_mult: phase.armor_mult,
            call_interval_mult: phase.call_interval_mult,
            siege_recovery_mult: phase.siege_recovery_mult,
            hold_ticks: phase.hold_ticks,
        };
        if !profile.valid() || phases.iter().any(|(boss, _)| *boss == phase.boss) {
            return Err(format!("Invalid or duplicate boss phase: {:?}", phase.boss));
        }
        phases.push((phase.boss, profile));
    }
    if !phases.is_empty() && phases.len() != KINDS.len() {
        return Err("Boss phases need exactly one profile per boss kind".into());
    }
    Ok(phases)
}

/// A catalogue id: 1-64 bytes of `[a-z0-9_.-]`. Saved per-archetype
/// telemetry keys use the same rule.
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b))
}
fn finite_range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}
fn index_by_id<T>(items: &[T], id: impl Fn(&T) -> &str) -> Result<BTreeMap<&str, &T>, String> {
    if items.is_empty() || items.len() > MAX_RECORDS {
        return Err("Component/archetype tables require 1..=4096 records".into());
    }
    let mut indexed = BTreeMap::new();
    for item in items {
        let key = id(item);
        if !valid_id(key) || indexed.insert(key, item).is_some() {
            return Err(format!(
                "Invalid or duplicate enemy component/archetype ID: {key}"
            ));
        }
    }
    Ok(indexed)
}
fn component<'a, T>(table: &BTreeMap<&str, &'a T>, id: &str) -> Result<&'a T, String> {
    table
        .get(id)
        .copied()
        .ok_or_else(|| format!("Unknown enemy component reference: {id}"))
}

/// The elite tables are bounded, the affix ids unique, and every eligible
/// archetype x offered affix pair passes [`ResolvedEnemy::validate`] (design
/// enemy-balance §5: there are no runtime clamps). Returns the largest
/// elite (Personal, NPC) health.
fn check_elites(
    base: &EliteBase,
    affixes: &[EliteAffix],
    resolved: &BTreeMap<String, ResolvedEnemy>,
    ineligible: &BTreeSet<String>,
) -> Result<(i32, i32), String> {
    if !finite_range(base.health, 1.0, 4.0)
        || !(1..=10).contains(&base.reward)
        || !(1..=10).contains(&base.xp)
        || !finite_range(base.scale, 1.0, 1.5)
    {
        return Err("Invalid elite base multipliers".into());
    }
    if affixes.len() > MAX_ELITE_AFFIXES {
        return Err(format!("At most {MAX_ELITE_AFFIXES} elite affixes"));
    }
    let mut ids = BTreeSet::new();
    let mut maximum = (0, 0);
    for affix in affixes {
        if !affix.valid() || !ids.insert(affix.id.as_str()) {
            return Err(format!("Invalid or duplicate elite affix: {}", affix.id));
        }
        for enemy in resolved.values() {
            if ineligible.contains(&enemy.archetype_id) || !affix.offered_to(enemy) {
                continue;
            }
            let elite = enemy.with_affix(base, affix);
            elite.validate().map_err(|error| {
                format!(
                    "Elite affix {} breaks {}: {error}",
                    affix.id, enemy.archetype_id
                )
            })?;
            maximum = (maximum.0.max(elite.health), maximum.1.max(elite.npc_hp));
        }
    }
    Ok(maximum)
}

impl AuthoredCatalog {
    fn resolve(self) -> Result<EnemyCatalog, String> {
        let bodies = index_by_id(&self.bodies, |v| &v.id)?;
        let movements = index_by_id(&self.movements, |v| &v.id)?;
        let attacks = index_by_id(&self.attacks, |v| &v.id)?;
        let defenses = index_by_id(&self.defenses, |v| &v.id)?;
        let rewards = index_by_id(&self.rewards, |v| &v.id)?;
        let visuals = index_by_id(&self.visuals, |v| &v.id)?;
        let archetypes = index_by_id(&self.archetypes, |v| &v.id)?;
        // Validate even unused reusable parts; authors cannot hide bad data until
        // a later archetype happens to reference it.
        for b in &self.bodies {
            if !(1..=100_000).contains(&b.health)
                || !(1..=1000).contains(&b.npc_hp)
                || !(3..=30).contains(&b.npc_ht)
            {
                return Err(format!("Invalid body component: {}", b.id));
            }
        }
        for m in &self.movements {
            if !finite_range(m.speed, 0.01, 10.0) {
                return Err(format!("Invalid movement component: {}", m.id));
            }
        }
        for a in &self.attacks {
            if !(1..=10_000).contains(&a.damage)
                || !finite_range(a.windup, 0.5, 60.0)
                || !(3..=30).contains(&a.npc_skill)
                || !(1..=20).contains(&a.npc_damage_dice)
                || !(-30..=100).contains(&a.npc_damage_add)
                || !(1..=8).contains(&a.structure_damage_multiplier)
                || !finite_range(a.reach, 0.0, 12.0)
            {
                return Err(format!("Invalid attack component: {}", a.id));
            }
        }
        for d in &self.defenses {
            if !(0..=1000).contains(&d.armor)
                || !(0..=100).contains(&d.npc_armor)
                || !(0..=20).contains(&d.npc_dodge)
                || !finite_range(d.contact_armor_fraction, 0.0, 1.0)
                || !(0..=1000).contains(&d.ranged_armor)
                || !(0..=100).contains(&d.npc_ranged_armor)
            {
                return Err(format!("Invalid defense component: {}", d.id));
            }
        }
        for r in &self.rewards {
            if !(1..=1_000_000).contains(&r.coins) || r.xp > 1_000_000 {
                return Err(format!("Invalid reward component: {}", r.id));
            }
        }
        for v in &self.visuals {
            if !finite_range(v.scale, 0.25, 3.0)
                || v.tint.iter().any(|v| !finite_range(*v, 0.0, 1.0))
            {
                return Err(format!("Invalid visual component: {}", v.id));
            }
        }
        let mut resolved = BTreeMap::new();
        let mut elite_ineligible = BTreeSet::new();
        for (id, a) in &archetypes {
            let id = *id;
            let b = component(&bodies, &a.body)?;
            let m = component(&movements, &a.movement)?;
            let attack = component(&attacks, &a.attack)?;
            let d = component(&defenses, &a.defense)?;
            let r = component(&rewards, &a.reward)?;
            let v = component(&visuals, &a.visual)?;
            let enemy = ResolvedEnemy {
                archetype_id: id.to_owned(),
                kind: a.base_enemy_kind,
                name: a.name.clone(),
                health: b.health,
                npc_hp: b.npc_hp,
                npc_ht: b.npc_ht,
                speed: m.speed,
                damage: attack.damage,
                windup: attack.windup,
                npc_skill: attack.npc_skill,
                npc_damage_dice: attack.npc_damage_dice,
                npc_damage_add: attack.npc_damage_add,
                structure_damage_multiplier: attack.structure_damage_multiplier,
                contact_armor_fraction: d.contact_armor_fraction,
                target_urgency: a
                    .target_urgency
                    .unwrap_or_else(|| legacy_target_urgency(a.base_enemy_kind)),
                armor: d.armor,
                npc_armor: d.npc_armor,
                npc_dodge: d.npc_dodge,
                reward: r.coins,
                xp: r.xp,
                visual: EnemyVisual {
                    sprite: v.sprite,
                    tint: v.tint,
                    scale: v.scale,
                },
                unlocked_by: a.unlocked_by,
                reach: attack.reach,
                ranged_armor: d.ranged_armor,
                npc_ranged_armor: d.npc_ranged_armor,
                stagger_immune: b.stagger_immune,
                group_size: a.group_size,
                death_effect: a.death_effect.clone(),
                tags: a.tags.clone(),
                elite: None,
            };
            enemy.validate()?;
            if !a.elite_eligible {
                elite_ineligible.insert(id.to_owned());
            }
            resolved.insert(id.to_owned(), enemy);
        }
        // A split target is checked at load (depth 1): it exists, is not the
        // splitter itself, needs no boss unlock, has no death effect of its
        // own and spawns alone.
        for enemy in resolved.values() {
            if let Some(DeathEffect::Split { archetype_id, .. }) = &enemy.death_effect {
                let valid = archetype_id != &enemy.archetype_id
                    && resolved.get(archetype_id).is_some_and(|child| {
                        child.unlocked_by.is_none()
                            && child.death_effect.is_none()
                            && child.group_size == 1
                    });
                if !valid {
                    return Err(format!(
                        "Invalid split target {archetype_id} of {}",
                        enemy.archetype_id
                    ));
                }
            }
        }
        let mut defaults = KindMap::<EnemyKind, String>::default();
        for default in self.defaults {
            let enemy = resolved
                .get(&default.archetype_id)
                .ok_or_else(|| format!("Unknown enemy default: {}", default.archetype_id))?;
            let target = &mut defaults[default.kind];
            if !target.is_empty() || enemy.kind != default.kind || enemy.unlocked_by.is_some() {
                return Err("Duplicate, mismatched or locked legacy enemy default".into());
            }
            *target = default.archetype_id;
        }
        if defaults.values().any(String::is_empty) {
            return Err(
                "Exactly one unrestricted default per legacy enemy kind is required".into(),
            );
        }
        if self.waves.is_empty() || self.waves.len() > MAX_RECORDS {
            return Err("Require 1..=4096 authored wave templates".into());
        }
        let mut authored_waves = self.waves;
        authored_waves.sort_by_key(|w| w.level);
        let mut waves = Vec::new();
        for (index, mut wave) in authored_waves.into_iter().enumerate() {
            if wave.level != index || wave.entries.is_empty() || wave.entries.len() > MAX_RECORDS {
                return Err(
                    "Wave levels must be contiguous from zero with 1..=4096 entries".into(),
                );
            }
            let mut seen = BTreeSet::new();
            let mut total = 0u32;
            let mut unrestricted = false;
            for entry in &wave.entries {
                let enemy = resolved
                    .get(&entry.archetype_id)
                    .ok_or_else(|| format!("Unknown wave enemy: {}", entry.archetype_id))?;
                if !seen.insert(&entry.archetype_id) || !(1..=1_000_000).contains(&entry.weight) {
                    return Err("Duplicate wave enemy or weight outside 1..=1000000".into());
                }
                total = total
                    .checked_add(entry.weight)
                    .ok_or("Wave weights overflow u32")?;
                unrestricted |= enemy.unlocked_by.is_none();
            }
            if total == 0 || !unrestricted {
                return Err("Every wave needs an unrestricted positive-weight enemy".into());
            }
            wave.entries
                .sort_by(|a, b| a.archetype_id.cmp(&b.archetype_id));
            waves.push(wave.entries);
        }
        let elite_maximum_health = check_elites(
            &self.elite_base,
            &self.elite_affixes,
            &resolved,
            &elite_ineligible,
        )?;
        let boss_phases = check_boss_phases(self.boss_phases)?;
        Ok(EnemyCatalog {
            resolved,
            defaults,
            waves,
            elite_ineligible,
            elite_base: self.elite_base,
            elite_affixes: self.elite_affixes,
            elite_maximum_health,
            boss_phases,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authored() -> serde_json::Value {
        serde_json::from_str(include_str!("../enemy_catalog.json")).unwrap()
    }
    fn parse(value: &serde_json::Value) -> Result<EnemyCatalog, String> {
        EnemyCatalog::from_json(&serde_json::to_string(value).unwrap())
    }

    #[test]
    fn shipped_components_resolve_complete_profiles_and_stable_pools() {
        let catalog = parse(&authored()).unwrap();
        let shambler = catalog.resolve("shambler").unwrap();
        assert_eq!(shambler.health, 32);
        assert_eq!(shambler.npc_hp, 9);
        assert_eq!(shambler.speed, 0.47);
        assert_eq!(shambler.reward, 3);
        assert_eq!(shambler.visual.sprite, VisualSprite::Shambler);
        for kind in EnemyKind::ALL {
            assert_eq!(
                catalog
                    .resolve(catalog.default_for_kind(kind))
                    .unwrap()
                    .kind,
                kind
            );
        }
        let pool = catalog.wave_pool(0, &[]);
        assert!(
            pool.windows(2)
                .all(|p| p[0].archetype_id < p[1].archetype_id)
        );
        // Levels past the 12 authored tiers (W2-31) wrap.
        assert_eq!(pool, catalog.wave_pool(12, &[]));
    }

    #[test]
    fn a_new_mix_and_match_archetype_needs_only_json() {
        let mut data = authored();
        let mut custom = data["archetypes"][0].clone();
        custom["id"] = "swift-plated-scavenger".into();
        custom["name"] = "Swift plated scavenger".into();
        custom["movement"] = "runner".into();
        custom["defense"] = "armored".into();
        data["archetypes"].as_array_mut().unwrap().push(custom);
        data["waves"][0]["entries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"archetype_id":"swift-plated-scavenger","weight":13}));
        let catalog = parse(&data).unwrap();
        let custom = catalog.resolve("swift-plated-scavenger").unwrap();
        assert_eq!(custom.health, catalog.resolve("shambler").unwrap().health);
        assert_eq!(custom.speed, catalog.resolve("runner").unwrap().speed);
        assert_eq!(custom.armor, catalog.resolve("armored").unwrap().armor);
        assert!(
            catalog
                .wave_pool(0, &[])
                .iter()
                .any(|w| w.archetype_id == custom.archetype_id)
        );
    }

    #[test]
    fn target_urgency_is_authored_data_with_a_legacy_category_default() {
        let catalog = parse(&authored()).unwrap();
        // Only the W2-31 exploder (-4, Shambler-based) and spitter (-2,
        // Runner-based) author an urgency other than their category default.
        // runner, caller-pack, skitter and plated-runner author the Runner -3
        // explicitly; sprinter and splitter-brood omit it and take that -3.
        let authored_urgency = [("exploder", -4.0), ("spitter", -2.0)];
        for id in catalog.archetype_ids() {
            let enemy = catalog.resolve(id).unwrap();
            let expected = authored_urgency
                .iter()
                .find(|(own, _)| *own == id)
                .map_or(legacy_target_urgency(enemy.kind), |(_, urgency)| *urgency);
            assert_eq!(enemy.target_urgency, expected, "{id}");
        }
        // Catalogues authored before the field keep the category's behaviour.
        let mut legacy = authored();
        for archetype in legacy["archetypes"].as_array_mut().unwrap() {
            archetype.as_object_mut().unwrap().remove("target_urgency");
        }
        let legacy = parse(&legacy).unwrap();
        for id in catalog.archetype_ids() {
            if authored_urgency.iter().all(|(own, _)| *own != id) {
                assert_eq!(legacy.resolve(id), catalog.resolve(id));
            }
        }
        // A data-only variant can reprioritise without a code change.
        let mut data = authored();
        data["archetypes"][0]["target_urgency"] = (-7.5).into();
        assert_eq!(
            parse(&data)
                .unwrap()
                .resolve("shambler")
                .unwrap()
                .target_urgency,
            -7.5
        );
        data["archetypes"][0]["target_urgency"] = 101.0.into();
        assert!(parse(&data).is_err());
    }

    #[test]
    fn boss_variants_need_matching_kill_unlock_and_keep_regular_stats() {
        let catalog = parse(&authored()).unwrap();
        for (id, boss) in [
            ("breacher-pack", BossKind::Breacher),
            ("caller-pack", BossKind::Caller),
            ("siege-pack", BossKind::SiegeBrute),
        ] {
            assert!(
                !catalog
                    .wave_pool(4, &[])
                    .iter()
                    .any(|w| w.archetype_id == id)
            );
            assert!(
                catalog
                    .wave_pool(4, &[boss])
                    .iter()
                    .any(|w| w.archetype_id == id)
            );
            let regular = catalog.resolve(id).unwrap();
            assert_eq!(regular.unlocked_by, Some(boss));
            assert!(regular.health < 480);
            assert!(regular.visual.scale < 1.5);
        }
    }

    #[test]
    fn reject_unknown_duplicate_unbounded_and_unsupported_authored_data() {
        let base = authored();
        let mutations: Vec<(&str, serde_json::Value)> = vec![
            ("/archetypes/0/body", "missing".into()),
            ("/archetypes/0/name", " ".into()),
            ("/bodies/0/health", 0.into()),
            ("/movements/0/speed", (-1).into()),
            ("/attacks/0/windup", 0.into()),
            ("/defenses/0/npc_dodge", 21.into()),
            ("/rewards/0/coins", 0.into()),
            ("/visuals/0/scale", 4.into()),
            ("/waves/0/entries/0/weight", 0.into()),
            ("/waves/0/entries/0/archetype_id", "missing".into()),
            ("/waves/0/level", 99.into()),
        ];
        for (pointer, value) in mutations {
            let mut data = base.clone();
            *data.pointer_mut(pointer).unwrap() = value;
            assert!(parse(&data).is_err(), "accepted {pointer}");
        }
        for table in [
            "bodies",
            "movements",
            "attacks",
            "defenses",
            "rewards",
            "visuals",
            "archetypes",
            "defaults",
        ] {
            let mut data = base.clone();
            let duplicate = data[table][0].clone();
            data[table].as_array_mut().unwrap().push(duplicate);
            assert!(parse(&data).is_err(), "accepted duplicate {table}");
        }
        let mut data = base.clone();
        data["bodies"][0]["inherits"] = "shambler".into();
        assert!(parse(&data).is_err());
        let mut data = base.clone();
        data["waves"][0]["entries"] = serde_json::json!([]);
        assert!(parse(&data).is_err());
        let mut data = base;
        data["waves"][0]["entries"] =
            serde_json::json!([{"archetype_id":"breacher-pack","weight":1}]);
        assert!(parse(&data).is_err());
        assert!(EnemyCatalog::from_json(&" ".repeat(MAX_JSON_BYTES + 1)).is_err());
        assert!(
            EnemyCatalog::from_json(
                &include_str!("../enemy_catalog.json").replacen("0.47", "NaN", 1)
            )
            .is_err()
        );
    }

    /// W2-31: the composition fields resolve into the snapshot, are bounded
    /// at load, and a split target is checked at depth 1.
    #[test]
    fn composition_fields_resolve_and_reject_out_of_bounds_or_bad_split_targets() {
        let catalog = parse(&authored()).unwrap();
        let spitter = catalog.resolve("spitter").unwrap();
        assert_eq!(spitter.reach, 4.0);
        assert_eq!(spitter.tags, ["ranged", "hazard"]);
        let shield = catalog.resolve("shield-bearer").unwrap();
        assert_eq!((shield.ranged_armor, shield.npc_ranged_armor), (14, 4));
        assert!(catalog.resolve("juggernaut").unwrap().stagger_immune);
        assert_eq!(
            catalog
                .resolve("juggernaut")
                .unwrap()
                .structure_damage_multiplier,
            3
        );
        assert_eq!(catalog.resolve("sprinter").unwrap().group_size, 4);
        assert_eq!(
            catalog.resolve("splitter").unwrap().death_effect,
            Some(DeathEffect::Split {
                archetype_id: "splitter-brood".into(),
                count: 3
            })
        );
        assert!(matches!(
            catalog.resolve("exploder").unwrap().death_effect,
            Some(DeathEffect::Burst { fuse: true, .. })
        ));
        assert!(!catalog.elite_eligible("splitter-brood"));
        assert!(catalog.elite_eligible("shambler") && !catalog.elite_eligible("missing"));

        let index = |table: &str, id: &str| {
            authored()[table]
                .as_array()
                .unwrap()
                .iter()
                .position(|v| v["id"] == id)
                .unwrap()
        };
        let splitter = index("archetypes", "splitter");
        let mutations: Vec<(String, serde_json::Value)> = vec![
            (
                format!("/attacks/{}/reach", index("attacks", "spitter")),
                12.5.into(),
            ),
            (
                format!(
                    "/defenses/{}/ranged_armor",
                    index("defenses", "shield-bearer")
                ),
                1001.into(),
            ),
            (
                format!(
                    "/defenses/{}/npc_ranged_armor",
                    index("defenses", "shield-bearer")
                ),
                101.into(),
            ),
            (
                format!("/archetypes/{}/group_size", index("archetypes", "sprinter")),
                7.into(),
            ),
            (
                format!("/archetypes/{}/group_size", index("archetypes", "sprinter")),
                0.into(),
            ),
            (
                format!("/archetypes/{}/tags/0", index("archetypes", "spitter")),
                "Bad Tag".into(),
            ),
            (
                format!("/archetypes/{splitter}/death_effect/Split/count"),
                5.into(),
            ),
            // Split targets: missing, itself, with a death effect of its own,
            // in a group, and behind a boss unlock.
            (
                format!("/archetypes/{splitter}/death_effect/Split/archetype_id"),
                "missing".into(),
            ),
            (
                format!("/archetypes/{splitter}/death_effect/Split/archetype_id"),
                "splitter".into(),
            ),
            (
                format!("/archetypes/{splitter}/death_effect/Split/archetype_id"),
                "exploder".into(),
            ),
            (
                format!("/archetypes/{splitter}/death_effect/Split/archetype_id"),
                "sprinter".into(),
            ),
            (
                format!("/archetypes/{splitter}/death_effect/Split/archetype_id"),
                "caller-pack".into(),
            ),
            (
                format!(
                    "/archetypes/{}/death_effect/Burst/radius",
                    index("archetypes", "exploder")
                ),
                3.5.into(),
            ),
        ];
        for (pointer, value) in mutations {
            let mut data = authored();
            *data.pointer_mut(&pointer).unwrap() = value;
            assert!(parse(&data).is_err(), "accepted {pointer}");
        }
        // An unused component is bounded too.
        let mut data = authored();
        let mut unused = data["bodies"][0].clone();
        unused["id"] = "unused-body".into();
        unused["stagger_immune"] = "yes".into();
        data["bodies"].as_array_mut().unwrap().push(unused);
        assert!(parse(&data).is_err());
    }

    /// W2-31: an unused attack or defense component is bounded on reach and
    /// ranged armor too; no archetype resolves it, so only the component
    /// loop can reject it.
    #[test]
    fn unused_components_still_bound_reach_and_ranged_armor() {
        let cases: [(&str, &str, serde_json::Value, &str); 6] = [
            ("attacks", "reach", (-0.5).into(), "attack"),
            ("attacks", "reach", 12.5.into(), "attack"),
            ("defenses", "ranged_armor", (-1).into(), "defense"),
            ("defenses", "ranged_armor", 1001.into(), "defense"),
            ("defenses", "npc_ranged_armor", (-1).into(), "defense"),
            ("defenses", "npc_ranged_armor", 101.into(), "defense"),
        ];
        for (table, field, value, kind) in cases {
            let mut data = authored();
            let mut unused = data[table][0].clone();
            unused["id"] = "unused-out-of-bounds".into();
            unused[field] = value.clone();
            data[table].as_array_mut().unwrap().push(unused);
            assert_eq!(
                parse(&data).err(),
                Some(format!("Invalid {kind} component: unused-out-of-bounds")),
                "{table}.{field} = {value}"
            );
        }
    }

    #[test]
    fn unused_attack_components_still_validate_demolition_multipliers() {
        for multiplier in [0, 9] {
            let mut data = authored();
            let mut unused = data["attacks"][0].clone();
            unused["id"] = "unused-invalid-attack".into();
            unused["structure_damage_multiplier"] = multiplier.into();
            data["attacks"].as_array_mut().unwrap().push(unused);
            assert!(parse(&data).is_err());
        }
    }

    #[test]
    fn snapshots_survive_catalogue_changes_and_validate_independently() {
        let old_catalog = parse(&authored()).unwrap();
        let snapshot = old_catalog.resolve("shambler").unwrap();
        let serialized = serde_json::to_string(&snapshot).unwrap();
        let mut data = authored();
        data["bodies"][0]["health"] = 99.into();
        let new_catalog = parse(&data).unwrap();
        let restored: ResolvedEnemy = serde_json::from_str(&serialized).unwrap();
        assert_eq!(restored.health, 32);
        assert_eq!(new_catalog.resolve("shambler").unwrap().health, 99);
        assert!(restored.validate().is_ok());
        let mut invalid = restored;
        invalid.speed = f32::NAN;
        assert!(invalid.validate().is_err());
    }

    /// W2-33 (design enemy-balance §5 "Base multipliers", "Affixes" and
    /// "Bake"): the shipped elite table, the stats one affix bakes into a
    /// spawn snapshot, and the load-time check of every archetype x affix
    /// pair.
    #[test]
    fn elite_affixes_bake_the_design_table_into_the_spawn_snapshot() {
        let catalog = parse(&authored()).unwrap();
        assert_eq!(
            catalog.elite_affix_ids().collect::<Vec<_>>(),
            ["hardened", "frenzied", "vicious", "shielded", "volatile"]
        );
        let shambler = catalog.resolve("shambler").unwrap();
        let bake = |id: &str| {
            let affix = catalog.elite_affixes.iter().find(|a| a.id == id).unwrap();
            shambler.with_affix(&catalog.elite_base, affix)
        };
        // Base: health x1.5 in both modes, reward x4, xp x3, scale x1.15.
        let hardened = bake("hardened");
        assert_eq!(
            hardened.elite,
            Some(EliteMark {
                affix_id: "hardened".into(),
                name: "Hardened Shambler".into()
            })
        );
        assert_eq!((hardened.health, hardened.npc_hp), (77, 18));
        assert_eq!((hardened.armor, hardened.npc_armor), (4, 2));
        assert_eq!((hardened.reward, hardened.xp), (12, 6));
        assert_eq!(hardened.visual.scale, 1.15);
        assert_eq!(hardened.name, shambler.name, "the record keeps its name");
        let frenzied = bake("frenzied");
        assert_eq!((frenzied.health, frenzied.npc_hp), (48, 14));
        assert_eq!(frenzied.speed, 0.47 * 1.3);
        assert_eq!(frenzied.windup, 1.25 * 0.75);
        assert_eq!((frenzied.npc_skill, frenzied.npc_dodge), (11, 8));
        let vicious = bake("vicious");
        assert_eq!((vicious.damage, vicious.npc_damage_add), (8, 3));
        let shielded = bake("shielded");
        assert_eq!((shielded.ranged_armor, shielded.npc_ranged_armor), (10, 3));
        let volatile = bake("volatile");
        assert_eq!((volatile.health, volatile.npc_hp), (58, 14));
        assert_eq!(
            volatile.death_effect,
            Some(DeathEffect::Burst {
                radius: 1.4,
                damage: 24,
                structure_damage: 30,
                npc_dice: 2,
                npc_add: 0,
                fuse: false
            })
        );
        // The pick is keyed: the key modulo the eligible weight (30, 25,
        // 20, 15, 10), and volatile is never offered to an archetype that
        // already has a death effect.
        let affix_of = |id: &str, key: u64| {
            catalog
                .elite_profile(&catalog.resolve(id).unwrap(), key)
                .and_then(|p| p.elite)
                .map(|m| m.affix_id)
        };
        let picks = [0, 29, 30, 54, 55, 74, 75, 89, 90, 99, 100].map(|k| affix_of("shambler", k));
        let expect = [
            "hardened", "hardened", "frenzied", "frenzied", "vicious", "vicious", "shielded",
            "shielded", "volatile", "volatile", "hardened",
        ];
        assert_eq!(picks, expect.map(|a| Some(a.to_owned())));
        for id in ["exploder", "splitter"] {
            let offered: BTreeSet<String> = (0..90).filter_map(|k| affix_of(id, k)).collect();
            assert!(!offered.contains("volatile"), "{id}");
            assert_eq!(affix_of(id, 89), Some("shielded".into()), "{id}: out of 90");
        }
        assert_eq!(catalog.elite_profile(&hardened, 0), None, "never twice");
        // Every eligible archetype x affix pair validates at load, or the
        // load is rejected: a windup affix of .5 takes the 0.8 s sprinter
        // below the 0.5 s bound.
        let mut data = authored();
        data["elite_affixes"][1]["windup"] = 0.5.into();
        let error = parse(&data).unwrap_err();
        assert!(error.contains("frenzied"), "{error}");
        // The splitter brood is not eligible, so it is not checked.
        let mut data = authored();
        data["elite_affixes"][0]["armor"] = 1000.into();
        assert!(parse(&data).is_err(), "armor above the bound");
        let rows: [(&str, serde_json::Value); 8] = [
            ("/elite_base/health", 0.9.into()),
            ("/elite_base/reward", 0.into()),
            ("/elite_affixes/0/weight", 0.into()),
            ("/elite_affixes/0/health", 4.5.into()),
            ("/elite_affixes/1/speed", 2.5.into()),
            ("/elite_affixes/2/damage", 0.5.into()),
            ("/elite_affixes/3/id", "Shielded!".into()),
            ("/elite_affixes/4/name", " ".into()),
        ];
        for (pointer, value) in rows {
            let mut data = authored();
            *data.pointer_mut(pointer).unwrap() = value;
            assert!(parse(&data).is_err(), "accepted {pointer}");
        }
        let mut data = authored();
        let duplicate = data["elite_affixes"][0].clone();
        data["elite_affixes"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(parse(&data).is_err(), "accepted a duplicate affix");
        // Without the tables there are no elites.
        let mut data = authored();
        data.as_object_mut().unwrap().remove("elite_affixes");
        data.as_object_mut().unwrap().remove("elite_base");
        let legacy = parse(&data).unwrap();
        assert_eq!(legacy.elite_affix_ids().count(), 0);
        assert_eq!(legacy.elite_profile(&shambler, 7), None);
    }

    /// Design enemy-balance §6: one phase profile per boss kind, within the
    /// profile bounds, and none without the table.
    #[test]
    fn boss_phase_table_has_one_bounded_profile_per_boss_kind() {
        let catalog = parse(&authored()).unwrap();
        let row = |kind| {
            let p = catalog.boss_phase(kind).expect("a profile per kind");
            (
                p.name.as_str(),
                p.speed_mult,
                p.armor_mult,
                p.call_interval_mult,
                p.siege_recovery_mult,
                p.hold_ticks,
            )
        };
        assert_eq!(row(BossKind::Breacher), ("Breach", 1.4, 0.6, 1.0, 1.0, 90));
        assert_eq!(row(BossKind::Caller), ("Chorus", 1.0, 1.0, 0.6, 1.0, 90));
        assert_eq!(
            row(BossKind::SiegeBrute),
            ("Enrage", 1.2, 1.0, 1.0, 0.6, 90)
        );
        let rows: [(&str, serde_json::Value); 7] = [
            ("/boss_phases/0/name", "".into()),
            ("/boss_phases/0/name", "x".repeat(41).into()),
            ("/boss_phases/0/speed_mult", 2.1.into()),
            ("/boss_phases/0/armor_mult", 0.2.into()),
            ("/boss_phases/1/call_interval_mult", 0.25.into()),
            ("/boss_phases/2/siege_recovery_mult", 1.1.into()),
            ("/boss_phases/2/hold_ticks", 181.into()),
        ];
        for (pointer, value) in rows {
            let mut data = authored();
            *data.pointer_mut(pointer).unwrap() = value;
            assert!(parse(&data).is_err(), "accepted {pointer}");
        }
        let mut data = authored();
        data["boss_phases"][1]["boss"] = "Breacher".into();
        assert!(parse(&data).is_err(), "accepted a duplicate kind");
        let mut data = authored();
        data["boss_phases"].as_array_mut().unwrap().pop();
        assert!(parse(&data).is_err(), "accepted a missing kind");
        let mut data = authored();
        data["boss_phases"][0]["phases"] = 3.into();
        assert!(parse(&data).is_err(), "accepted an unknown key");
        // Without the table no boss phases.
        let mut data = authored();
        data.as_object_mut().unwrap().remove("boss_phases");
        let legacy = parse(&data).unwrap();
        assert!(legacy.boss_phase(BossKind::Caller).is_none());
    }
}
