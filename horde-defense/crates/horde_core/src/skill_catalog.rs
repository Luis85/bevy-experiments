//! Validated, extensible skill DAG. Ownership stores stable string IDs, never node indices.
use crate::catalog::{self, ContentError};
use crate::kind_map::KindMap;
use crate::weapons::WeaponClass;
use crate::{campaign::BodyAttribute, content::StructureKind};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillNode {
    #[serde(default)]
    pub minimum_liberated: u32,
    #[serde(default)]
    pub minimum_victories: u64,
    pub id: String,
    pub name: String,
    pub description: String,
    pub price: u64,
    pub prerequisites: Vec<String>,
    pub branch: String,
    pub position: [f32; 2],
    pub icon: String,
    pub effects: Vec<SkillEffect>,
    /// Enemy archetype ids this node is authored to answer (for example
    /// `"shield-bearer"`). Descriptive only: no rule reads it. At most 8
    /// distinct ids of lowercase ASCII letters, digits, `-` or `_`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<String>,
}
/// A data-keyed capability a skill grants (`Unlock("grenade")`). Unlocks carry
/// no value; the simulation asks `SkillModifiers::unlocked` for the IDs it
/// implements, so new nodes can grant existing unlocks without a new variant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnlockId(pub String);
impl UnlockId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl From<&str> for UnlockId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}
impl std::fmt::Display for UnlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
/// Unlock IDs the simulation implements. Catalogues may only grant these, so a
/// typo is a load error rather than a silently dead skill.
pub mod unlocks {
    pub const PIERCING: &str = "piercing";
    pub const SWEEP: &str = "sweep";
    pub const HEAL: &str = "heal";
    pub const BRACE: &str = "brace";
    pub const GRENADE: &str = "grenade";
    pub const MOLOTOV: &str = "molotov";
    pub const ALL: [&str; 6] = [PIERCING, SWEEP, HEAL, BRACE, GRENADE, MOLOTOV];
}

/// Typed unlocks with campaign-side rules (blueprints, ranks, features,
/// weapon classes) keep their variants; numeric variants are stat modifiers;
/// plain capability unlocks are `Unlock(UnlockId)` data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum SkillEffect {
    BodyDevelopment {
        attribute: BodyAttribute,
        rank: u32,
    },
    Blueprint(StructureKind),
    StructureRank {
        kind: StructureKind,
        rank: u32,
    },
    FeatureUnlock(CampaignFeature),
    DamageAdd(i32),
    DamageMultiplier(f32),
    ReloadMultiplier(f32),
    CadenceMultiplier(f32),
    HealthAdd(i32),
    StaminaAdd(i32),
    ArmorAdd(i32),
    MeleeAdd(i32),
    EngineeringMultiplier(f32),
    WeaponTraining(WeaponClass),
    Unlock(UnlockId),
    GrenadeCharges(u32),
    MolotovCharges(u32),
    OrdnanceDamageMultiplier(f32),
    OrdnanceRadiusMultiplier(f32),
    BurnDurationMultiplier(f32),
    OrdnanceCooldownMultiplier(f32),
    /// A personal perk for one weapon class: Damage 1–2, Reload and Cadence
    /// 0.5–1 (multipliers), WeakBonus 0.01–1 (added to the weak-spot bonus).
    ClassModifier {
        class: WeaponClass,
        stat: ClassStat,
        value: f32,
    },
    /// Enemy armor ignored by personal shots, 1–20.
    ArmorPenetration(i32),
    /// Multiplier on hazard (blast and spit) damage taken, 0.5–1.
    HazardDamageMultiplier(f32),
}
/// The per-class stat a [`SkillEffect::ClassModifier`] changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClassStat {
    Damage,
    Reload,
    Cadence,
    WeakBonus,
}
impl ClassStat {
    pub const ALL: [Self; 4] = [Self::Damage, Self::Reload, Self::Cadence, Self::WeakBonus];
}
/// Folded per-class perks for one weapon class. Multipliers default to 1.0
/// and the weak-spot addition to 0.0; every field is always written.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClassModifiers {
    pub damage_multiplier: f32,
    pub reload_multiplier: f32,
    pub cadence_multiplier: f32,
    pub weak_bonus_add: f32,
}
impl Default for ClassModifiers {
    fn default() -> Self {
        Self {
            damage_multiplier: 1.0,
            reload_multiplier: 1.0,
            cadence_multiplier: 1.0,
            weak_bonus_add: 0.0,
        }
    }
}
impl ClassModifiers {
    /// The folded value of `stat`.
    pub fn get(&self, stat: ClassStat) -> f32 {
        match stat {
            ClassStat::Damage => self.damage_multiplier,
            ClassStat::Reload => self.reload_multiplier,
            ClassStat::Cadence => self.cadence_multiplier,
            ClassStat::WeakBonus => self.weak_bonus_add,
        }
    }
    fn within_bounds(&self) -> bool {
        [
            self.damage_multiplier,
            self.reload_multiplier,
            self.cadence_multiplier,
        ]
        .into_iter()
        .all(|v| v.is_finite() && (0.01..=100.0).contains(&v))
            && self.weak_bonus_add.is_finite()
            && (0.0..=1.0).contains(&self.weak_bonus_add)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CampaignFeature {
    Construction,
    Companion,
    Recruitment,
    Parallel,
    Operations,
}
impl CampaignFeature {
    pub const ALL: [Self; 5] = [
        Self::Construction,
        Self::Companion,
        Self::Recruitment,
        Self::Parallel,
        Self::Operations,
    ];
    /// Player-facing name of the unlocked feature, also its upgrade-goal label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Construction => "Fortifications",
            Self::Companion => "Mara",
            Self::Recruitment => "Recruitment",
            Self::Parallel => "Parallel defenses",
            Self::Operations => "Country operations",
        }
    }
    /// The feature's stable id. Presentation saves it (settings.json
    /// `seen_callouts` holds "feature:<id>"), so an id never changes and
    /// the list is append-only.
    pub fn id(self) -> &'static str {
        match self {
            Self::Construction => "construction",
            Self::Companion => "companion",
            Self::Recruitment => "recruitment",
            Self::Parallel => "parallel",
            Self::Operations => "operations",
        }
    }
}

/// Player-facing grouping of skill effects, used for search and filtering.
/// A node may belong to several categories (one per distinct effect group).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectCategory {
    /// Personal firearm damage, reload, fire rate and piercing.
    Firearms,
    /// Permission to equip an owned weapon class.
    WeaponClass,
    /// Health, stamina, armor and the personal heal and brace actives.
    Survival,
    /// Melee damage and the contact sweep.
    Melee,
    /// Grenades, Molotovs and their upgrades.
    Ordnance,
    /// Structure building, engineering, blueprints and mastery ranks.
    Fortification,
    /// Body-development attribute ranks.
    Body,
    /// Mara, recruitment, parallel defenses and country operations.
    Companions,
}

impl EffectCategory {
    pub const ALL: [Self; 8] = [
        Self::Firearms,
        Self::WeaponClass,
        Self::Survival,
        Self::Melee,
        Self::Ordnance,
        Self::Fortification,
        Self::Body,
        Self::Companions,
    ];
    /// Sentence-case label for chips and filters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Firearms => "Firearms",
            Self::WeaponClass => "Weapon proficiency",
            Self::Survival => "Survival",
            Self::Melee => "Melee",
            Self::Ordnance => "Ordnance",
            Self::Fortification => "Fortification",
            Self::Body => "Body development",
            Self::Companions => "Companions and world",
        }
    }
}

/// Player wording for an implemented capability unlock id (see [`unlocks`]);
/// unknown ids fall back to the id itself.
pub fn unlock_label(id: &str) -> &str {
    match id {
        unlocks::PIERCING => "Piercing shots",
        unlocks::SWEEP => "Contact sweep",
        unlocks::HEAL => "Emergency aid",
        unlocks::BRACE => "Brace",
        unlocks::GRENADE => "Grenades",
        unlocks::MOLOTOV => "Molotovs",
        other => other,
    }
}

impl SkillEffect {
    /// The player-facing category this effect belongs to.
    pub fn category(&self) -> EffectCategory {
        use EffectCategory as C;
        match self {
            Self::BodyDevelopment { .. } => C::Body,
            Self::Blueprint(_) | Self::StructureRank { .. } | Self::EngineeringMultiplier(_) => {
                C::Fortification
            }
            Self::FeatureUnlock(CampaignFeature::Construction) => C::Fortification,
            Self::FeatureUnlock(_) => C::Companions,
            Self::DamageAdd(_)
            | Self::DamageMultiplier(_)
            | Self::ReloadMultiplier(_)
            | Self::CadenceMultiplier(_)
            | Self::ClassModifier { .. }
            | Self::ArmorPenetration(_) => C::Firearms,
            Self::HealthAdd(_)
            | Self::StaminaAdd(_)
            | Self::ArmorAdd(_)
            | Self::HazardDamageMultiplier(_) => C::Survival,
            Self::MeleeAdd(_) => C::Melee,
            Self::WeaponTraining(_) => C::WeaponClass,
            Self::Unlock(id) => match id.as_str() {
                unlocks::PIERCING => C::Firearms,
                unlocks::SWEEP => C::Melee,
                unlocks::GRENADE | unlocks::MOLOTOV => C::Ordnance,
                _ => C::Survival,
            },
            Self::GrenadeCharges(_)
            | Self::MolotovCharges(_)
            | Self::OrdnanceDamageMultiplier(_)
            | Self::OrdnanceRadiusMultiplier(_)
            | Self::BurnDurationMultiplier(_)
            | Self::OrdnanceCooldownMultiplier(_) => C::Ordnance,
        }
    }
    /// Lowercase search keywords for this effect (for example `"shotgun"`,
    /// `"health"`, `"reload"`, `"grenade"`, `"mara"`).
    pub fn keywords(&self) -> Vec<&'static str> {
        let structure = |kind: StructureKind| match kind {
            StructureKind::Trap => "trap",
            StructureKind::Wall => "wall",
            StructureKind::Turret => "turret",
        };
        match self {
            Self::BodyDevelopment { attribute, .. } => match attribute {
                BodyAttribute::Vitality => vec!["vitality", "health"],
                BodyAttribute::Strength => vec!["strength", "melee"],
                BodyAttribute::Coordination => vec!["coordination", "reload"],
                BodyAttribute::Endurance => vec!["endurance", "stamina"],
                BodyAttribute::Perception => vec!["perception", "weak spot"],
            },
            Self::Blueprint(kind) => vec![structure(*kind), "blueprint", "structure"],
            Self::StructureRank { kind, .. } => vec![structure(*kind), "mastery", "structure"],
            Self::FeatureUnlock(feature) => match feature {
                CampaignFeature::Construction => vec!["construction", "build", "structure"],
                CampaignFeature::Companion => vec!["mara", "companion"],
                CampaignFeature::Recruitment => vec!["recruit", "recruitment", "survivor"],
                CampaignFeature::Parallel => vec!["parallel"],
                CampaignFeature::Operations => vec!["operations"],
            },
            Self::DamageAdd(_) | Self::DamageMultiplier(_) => vec!["damage"],
            Self::ReloadMultiplier(_) => vec!["reload"],
            Self::CadenceMultiplier(_) => vec!["fire rate", "cadence"],
            Self::HealthAdd(_) => vec!["health"],
            Self::StaminaAdd(_) => vec!["stamina"],
            Self::ArmorAdd(_) => vec!["armor"],
            Self::MeleeAdd(_) => vec!["melee", "damage"],
            Self::EngineeringMultiplier(_) => vec!["engineering", "structure"],
            Self::WeaponTraining(class) => vec![class_keyword(*class), "weapon", "proficiency"],
            Self::ClassModifier { class, stat, .. } => vec![
                class_keyword(*class),
                "perk",
                match stat {
                    ClassStat::Damage => "damage",
                    ClassStat::Reload => "reload",
                    ClassStat::Cadence => "fire rate",
                    ClassStat::WeakBonus => "weak spot",
                },
            ],
            Self::ArmorPenetration(_) => vec!["armor", "shield", "penetration", "perk"],
            Self::HazardDamageMultiplier(_) => vec!["blast", "spit", "hazard", "perk"],
            Self::Unlock(id) => match id.as_str() {
                unlocks::PIERCING => vec!["piercing", "penetration"],
                unlocks::SWEEP => vec!["sweep", "melee"],
                unlocks::HEAL => vec!["heal", "health"],
                unlocks::BRACE => vec!["brace", "stamina"],
                unlocks::GRENADE => vec!["grenade"],
                unlocks::MOLOTOV => vec!["molotov", "incendiary"],
                _ => vec!["ability"],
            },
            Self::GrenadeCharges(_) => vec!["grenade", "charges"],
            Self::MolotovCharges(_) => vec!["molotov", "charges"],
            Self::OrdnanceDamageMultiplier(_)
            | Self::OrdnanceRadiusMultiplier(_)
            | Self::OrdnanceCooldownMultiplier(_) => vec!["grenade", "molotov", "ordnance"],
            Self::BurnDurationMultiplier(_) => vec!["molotov", "burn", "incendiary"],
        }
    }
}

/// Lowercase search keyword for a weapon class.
fn class_keyword(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::Pistol => "pistol",
        WeaponClass::Smg => "smg",
        WeaponClass::Shotgun => "shotgun",
        WeaponClass::Rifle => "rifle",
        WeaponClass::Marksman => "marksman",
        WeaponClass::MachineGun => "machine gun",
    }
}

impl SkillNode {
    /// Distinct categories of this node's effects, in [`EffectCategory`] order.
    pub fn categories(&self) -> Vec<EffectCategory> {
        let set: BTreeSet<_> = self.effects.iter().map(SkillEffect::category).collect();
        set.into_iter().collect()
    }
    /// Whether `query` (lowercase, trimmed, non-empty) matches the name,
    /// description, branch, category labels or effect keywords.
    fn matches(&self, query: &str) -> bool {
        [&self.name, &self.description, &self.branch]
            .into_iter()
            .any(|text| text.to_lowercase().contains(query))
            || self
                .categories()
                .into_iter()
                .any(|c| c.label().to_lowercase().contains(query))
            || self
                .effects
                .iter()
                .flat_map(SkillEffect::keywords)
                .any(|keyword| keyword.contains(query))
    }
}

/// A named destination in the tree ("Shotgun proficiency", "Mara") that the
/// UI can offer as "Path to…". Derived from effects, never from fixed ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeGoal {
    /// Player-facing goal name.
    pub label: String,
    /// The first catalogue node granting the goal.
    pub node_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillModifiers {
    pub body_development: KindMap<BodyAttribute, u32>,
    pub blueprints: KindMap<StructureKind, bool>,
    pub structure_ranks: KindMap<StructureKind, u32>,
    pub features: BTreeSet<CampaignFeature>,
    pub damage_add: i32,
    pub damage_multiplier: f32,
    pub reload_multiplier: f32,
    pub cadence_multiplier: f32,
    pub health_add: i32,
    pub stamina_add: i32,
    pub armor_add: i32,
    pub melee_add: i32,
    pub engineering: bool,
    pub engineering_multiplier: f32,
    pub weapon_classes: BTreeSet<WeaponClass>,
    pub unlocks: BTreeSet<UnlockId>,
    pub grenade_charges: u32,
    pub molotov_charges: u32,
    pub ordnance_damage_multiplier: f32,
    pub ordnance_radius_multiplier: f32,
    pub burn_duration_multiplier: f32,
    pub ordnance_cooldown_multiplier: f32,
    /// Folded [`SkillEffect::ClassModifier`] perks; a class without a perk
    /// has no entry. Absent when empty.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub class_modifiers: BTreeMap<WeaponClass, ClassModifiers>,
    /// Enemy armor ignored by personal shots (0–1000). Absent at 0.
    #[serde(skip_serializing_if = "crate::schema::is_default")]
    pub armor_penetration: i32,
    /// Multiplier on hazard damage taken (0.01–1). Absent at exactly 1.0.
    #[serde(skip_serializing_if = "crate::schema::is_one_f32")]
    pub hazard_damage_multiplier: f32,
}
impl Default for SkillModifiers {
    fn default() -> Self {
        Self {
            body_development: KindMap::default(),
            blueprints: KindMap::default(),
            structure_ranks: KindMap::default(),
            features: BTreeSet::new(),
            damage_add: 0,
            damage_multiplier: 1.0,
            reload_multiplier: 1.0,
            cadence_multiplier: 1.0,
            health_add: 0,
            stamina_add: 0,
            armor_add: 0,
            melee_add: 0,
            engineering: false,
            engineering_multiplier: 1.0,
            weapon_classes: BTreeSet::new(),
            unlocks: BTreeSet::new(),
            grenade_charges: 2,
            molotov_charges: 2,
            ordnance_damage_multiplier: 1.0,
            ordnance_radius_multiplier: 1.0,
            burn_duration_multiplier: 1.0,
            ordnance_cooldown_multiplier: 1.0,
            class_modifiers: BTreeMap::new(),
            armor_penetration: 0,
            hazard_damage_multiplier: 1.0,
        }
    }
}
impl SkillModifiers {
    /// Whether a skill granted the capability `id` (see [`unlocks`]).
    pub fn unlocked(&self, id: &str) -> bool {
        self.unlocks.iter().any(|unlock| unlock.as_str() == id)
    }
    /// Validate frozen save snapshots independently of the current catalogue.
    pub fn validate(&self) -> Result<(), String> {
        if self.body_development.values().any(|rank| *rank > 1000)
            || self.structure_ranks.values().any(|rank| *rank > 10)
            || [
                self.damage_add,
                self.health_add,
                self.stamina_add,
                self.armor_add,
                self.melee_add,
            ]
            .into_iter()
            .any(|v| !(0..=1_000_000).contains(&v))
            || self.unlocks.iter().any(|id| !valid_unlock(id))
            || self.grenade_charges > 32768
            || self.molotov_charges > 32768
            || [
                self.damage_multiplier,
                self.reload_multiplier,
                self.cadence_multiplier,
                self.engineering_multiplier,
                self.ordnance_damage_multiplier,
                self.ordnance_radius_multiplier,
                self.burn_duration_multiplier,
                self.ordnance_cooldown_multiplier,
            ]
            .into_iter()
            .any(|v| !v.is_finite() || !(0.01..=100.0).contains(&v))
            || self.class_modifiers.len() > WeaponClass::ALL.len()
            || self.class_modifiers.values().any(|c| !c.within_bounds())
            || !(0..=1000).contains(&self.armor_penetration)
            || !self.hazard_damage_multiplier.is_finite()
            || !(0.01..=1.0).contains(&self.hazard_damage_multiplier)
        {
            return Err("Saved skill modifiers exceed supported finite bounds".into());
        }
        Ok(())
    }
    pub fn apply(&mut self, effect: &SkillEffect) {
        match *effect {
            SkillEffect::BodyDevelopment { attribute, rank } => {
                self.body_development[attribute] = self.body_development[attribute].max(rank)
            }
            SkillEffect::Blueprint(kind) => self.blueprints[kind] = true,
            SkillEffect::StructureRank { kind, rank } => {
                self.structure_ranks[kind] = self.structure_ranks[kind].max(rank)
            }
            SkillEffect::FeatureUnlock(feature) => {
                self.features.insert(feature);
            }
            SkillEffect::DamageAdd(n) => self.damage_add += n,
            SkillEffect::DamageMultiplier(n) => self.damage_multiplier *= n,
            SkillEffect::ReloadMultiplier(n) => self.reload_multiplier *= n,
            SkillEffect::CadenceMultiplier(n) => self.cadence_multiplier *= n,
            SkillEffect::HealthAdd(n) => self.health_add += n,
            SkillEffect::StaminaAdd(n) => self.stamina_add += n,
            SkillEffect::ArmorAdd(n) => self.armor_add += n,
            SkillEffect::MeleeAdd(n) => self.melee_add += n,
            SkillEffect::EngineeringMultiplier(n) => {
                self.engineering = true;
                self.engineering_multiplier *= n;
            }
            SkillEffect::WeaponTraining(class) => {
                self.weapon_classes.insert(class);
            }
            SkillEffect::Unlock(ref id) => {
                self.unlocks.insert(id.clone());
            }
            SkillEffect::GrenadeCharges(n) => self.grenade_charges += n,
            SkillEffect::MolotovCharges(n) => self.molotov_charges += n,
            SkillEffect::OrdnanceDamageMultiplier(n) => self.ordnance_damage_multiplier *= n,
            SkillEffect::OrdnanceRadiusMultiplier(n) => self.ordnance_radius_multiplier *= n,
            SkillEffect::BurnDurationMultiplier(n) => self.burn_duration_multiplier *= n,
            SkillEffect::OrdnanceCooldownMultiplier(n) => self.ordnance_cooldown_multiplier *= n,
            SkillEffect::ClassModifier { class, stat, value } => {
                let perk = self.class_modifiers.entry(class).or_default();
                match stat {
                    ClassStat::Damage => perk.damage_multiplier *= value,
                    ClassStat::Reload => perk.reload_multiplier *= value,
                    ClassStat::Cadence => perk.cadence_multiplier *= value,
                    ClassStat::WeakBonus => perk.weak_bonus_add += value,
                }
            }
            SkillEffect::ArmorPenetration(n) => self.armor_penetration += n,
            SkillEffect::HazardDamageMultiplier(n) => self.hazard_damage_multiplier *= n,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillCatalog {
    pub nodes: Vec<SkillNode>,
}
impl SkillCatalog {
    /// Pure parse + validate, so tests can build alternative catalogues in-process.
    pub fn from_json_str(text: &str) -> Result<Self, ContentError> {
        catalog::parse_validated("skills", text, 4 * catalog::MIB, Self::validate)
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        Self::from_json_str(text).map_err(ContentError::into_message)
    }
    /// Bundled default, or the `LASTLIGHT_SKILLS` override.
    pub fn try_load() -> Result<Self, ContentError> {
        catalog::load(
            "skills",
            4 * catalog::MIB,
            include_str!("../skills.json"),
            Self::from_json_str,
        )
    }
    pub fn node(&self, id: &str) -> Option<&SkillNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    /// Nodes matching `query` case-insensitively against name, description,
    /// branch, category labels and effect keywords, in catalogue order. An
    /// empty or whitespace-only query returns nothing.
    pub fn search(&self, query: &str) -> Vec<&SkillNode> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.nodes.iter().filter(|n| n.matches(&query)).collect()
    }
    /// One goal per weapon-class training node (the innate free root
    /// excluded), then grenades, Molotovs and each [`CampaignFeature`]
    /// (fortifications, Mara, recruitment, parallel defenses, country
    /// operations), in that order. Each goal names the first catalogue node
    /// granting it; goals the catalogue does not grant are omitted.
    pub fn upgrade_goals(&self) -> Vec<UpgradeGoal> {
        let first = |grants: &dyn Fn(&SkillEffect) -> bool| {
            self.nodes
                .iter()
                .find(|n| n.price > 0 && n.effects.iter().any(grants))
                .map(|n| n.id.clone())
        };
        let mut goals = Vec::new();
        let mut push = |label: String, node_id: Option<String>| {
            if let Some(node_id) = node_id {
                goals.push(UpgradeGoal { label, node_id });
            }
        };
        for class in WeaponClass::ALL {
            push(
                format!("{} proficiency", class.name()),
                first(&|e| *e == SkillEffect::WeaponTraining(class)),
            );
        }
        for id in [unlocks::GRENADE, unlocks::MOLOTOV] {
            push(
                unlock_label(id).to_owned(),
                first(&|e| matches!(e, SkillEffect::Unlock(u) if u.as_str() == id)),
            );
        }
        for feature in CampaignFeature::ALL {
            push(
                feature.label().to_owned(),
                first(&|e| *e == SkillEffect::FeatureUnlock(feature)),
            );
        }
        goals
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.is_empty() || self.nodes.len() > 256 {
            return Err("Skill catalogue needs 1â€“256 nodes".into());
        }
        let mut ids = BTreeSet::new();
        let mut positions = BTreeSet::new();
        for n in &self.nodes {
            if n.id.is_empty()
                || n.id.len() > 64
                || !n
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || !ids.insert(n.id.as_str())
                || n.name.trim().is_empty()
                || n.name.len() > 80
                || n.description.trim().is_empty()
                || n.description.len() > 1000
                || n.branch.trim().is_empty()
                || n.branch.len() > 48
                || n.icon.trim().is_empty()
                || n.icon.len() > 48
                || n.price > 1_000_000
                || n.effects.is_empty()
                || n.effects.len() > 16
                || n.position
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 10_000.0)
                || !positions.insert((n.position[0].to_bits(), n.position[1].to_bits()))
                || n.minimum_liberated > 250
                || n.minimum_victories > 10_000
                || n.prerequisites.len() > 16
                || (n.price == 0 && n.id != "pistol_training")
            {
                return Err(format!("Invalid or duplicate skill node: {}", n.id));
            }
            for effect in &n.effects {
                let valid = match effect {
                    SkillEffect::BodyDevelopment { rank, .. } => (1..=1000).contains(rank),
                    SkillEffect::StructureRank { rank, .. } => (1..=10).contains(rank),
                    SkillEffect::DamageAdd(v)
                    | SkillEffect::HealthAdd(v)
                    | SkillEffect::StaminaAdd(v)
                    | SkillEffect::MeleeAdd(v) => (1..=100).contains(v),
                    SkillEffect::ArmorAdd(v) => (1..=20).contains(v),
                    SkillEffect::ReloadMultiplier(v)
                    | SkillEffect::CadenceMultiplier(v)
                    | SkillEffect::OrdnanceCooldownMultiplier(v) => {
                        v.is_finite() && (0.5..=1.0).contains(v)
                    }
                    SkillEffect::DamageMultiplier(v)
                    | SkillEffect::EngineeringMultiplier(v)
                    | SkillEffect::OrdnanceDamageMultiplier(v)
                    | SkillEffect::OrdnanceRadiusMultiplier(v)
                    | SkillEffect::BurnDurationMultiplier(v) => {
                        v.is_finite() && (1.0..=2.0).contains(v)
                    }
                    SkillEffect::GrenadeCharges(v) | SkillEffect::MolotovCharges(v) => {
                        (1..=5).contains(v)
                    }
                    SkillEffect::Unlock(id) => valid_unlock(id),
                    SkillEffect::ClassModifier { stat, value, .. } => {
                        let range = match stat {
                            ClassStat::Damage => 1.0..=2.0,
                            ClassStat::Reload | ClassStat::Cadence => 0.5..=1.0,
                            ClassStat::WeakBonus => 0.01..=1.0,
                        };
                        value.is_finite() && range.contains(value)
                    }
                    SkillEffect::ArmorPenetration(v) => (1..=20).contains(v),
                    SkillEffect::HazardDamageMultiplier(v) => {
                        v.is_finite() && (0.5..=1.0).contains(v)
                    }
                    _ => true,
                };
                if !valid {
                    return Err(format!("Invalid effect in skill {}", n.id));
                }
            }
            let mut counters = BTreeSet::new();
            if n.counters.len() > 8
                || !n.counters.iter().all(|id| {
                    !id.is_empty()
                        && id.len() <= 48
                        && id.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                        })
                        && counters.insert(id.as_str())
                })
            {
                return Err(format!("Invalid skill counters: {}", n.id));
            }
        }
        let mut indegree = BTreeMap::new();
        for n in &self.nodes {
            let mut parents = BTreeSet::new();
            for parent in &n.prerequisites {
                if parent == &n.id || !ids.contains(parent.as_str()) || !parents.insert(parent) {
                    return Err(format!("Invalid prerequisite in skill {}", n.id));
                }
            }
            indegree.insert(n.id.as_str(), n.prerequisites.len());
        }
        let mut queue: BTreeSet<_> = indegree
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        if queue.len() != 1 || !queue.contains("pistol_training") {
            return Err(
                "The entire skill graph must share the single survivor-foundation root".into(),
            );
        }
        let mut visited = 0;
        while let Some(id) = queue.pop_first() {
            visited += 1;
            for n in &self.nodes {
                if n.prerequisites.iter().any(|p| p == id) {
                    let count = indegree.get_mut(n.id.as_str()).expect("validated ID");
                    *count -= 1;
                    if *count == 0 {
                        queue.insert(n.id.as_str());
                    }
                }
            }
        }
        if visited != self.nodes.len() {
            return Err("Skill prerequisites contain a cycle".into());
        }
        // Keep the six historical IDs and starter training available for save adapters.
        for id in [
            "pistol_training",
            "piercing",
            "quick_reload",
            "sweep",
            "second_wind",
            "brace",
            "engineering",
        ] {
            if !ids.contains(id) {
                return Err(format!("Missing compatibility skill: {id}"));
            }
        }
        let starter = self
            .node("pistol_training")
            .expect("checked compatibility node");
        if starter.price != 0
            || starter.minimum_liberated != 0
            || starter.minimum_victories != 0
            || !starter.prerequisites.is_empty()
            || starter.effects != [SkillEffect::WeaponTraining(WeaponClass::Pistol)]
        {
            return Err(
                "Pistol training must remain innate, free and without prerequisites".into(),
            );
        }
        // Bound the combined kernel as well as each individual effect. Even a
        // deliberately broad catalogue must not overflow stacked float products.
        let mut aggregate = SkillModifiers::default();
        for node in &self.nodes {
            for effect in &node.effects {
                aggregate.apply(effect);
            }
        }
        aggregate.validate()?;
        for factor in [
            aggregate.damage_multiplier,
            aggregate.reload_multiplier,
            aggregate.cadence_multiplier,
            aggregate.engineering_multiplier,
            aggregate.ordnance_damage_multiplier,
            aggregate.ordnance_radius_multiplier,
            aggregate.burn_duration_multiplier,
            aggregate.ordnance_cooldown_multiplier,
            aggregate.hazard_damage_multiplier,
        ]
        .into_iter()
        .chain(aggregate.class_modifiers.values().flat_map(|c| {
            [
                c.damage_multiplier,
                c.reload_multiplier,
                c.cadence_multiplier,
            ]
        })) {
            if !factor.is_finite() || !(0.01..=100.0).contains(&factor) {
                return Err("Combined skill multipliers exceed supported bounds".into());
            }
        }
        Ok(())
    }
}
/// Saved and authored unlocks must name an implemented capability.
fn valid_unlock(id: &UnlockId) -> bool {
    unlocks::ALL.contains(&id.as_str())
}
static SKILLS: OnceLock<SkillCatalog> = OnceLock::new();
pub(crate) fn install() -> Result<&'static SkillCatalog, ContentError> {
    catalog::install(&SKILLS, SkillCatalog::try_load)
}
pub fn skills() -> &'static SkillCatalog {
    catalog::lazy(&SKILLS, "skills", SkillCatalog::try_load, || {
        SkillCatalog::from_json_str(include_str!("../skills.json"))
    })
}
/// [`SkillCatalog::search`] over the installed catalogue.
pub fn search_nodes(query: &str) -> Vec<&'static SkillNode> {
    skills().search(query)
}
/// [`SkillCatalog::upgrade_goals`] over the installed catalogue.
pub fn upgrade_goals() -> Vec<UpgradeGoal> {
    skills().upgrade_goals()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_dag_is_extensible_and_rejects_cycles_references_and_bad_numbers() {
        let catalog = skills().clone();
        assert!(catalog.nodes.len() >= 25);
        catalog.validate().unwrap();
        let mut bad = catalog.clone();
        bad.nodes[0].prerequisites = vec![bad.nodes[1].id.clone()];
        bad.nodes[1].prerequisites = vec![bad.nodes[0].id.clone()];
        assert!(bad.validate().is_err());
        let mut bad = catalog.clone();
        bad.nodes[1].prerequisites = vec!["missing".into()];
        assert!(bad.validate().is_err());
        let mut bad = catalog.clone();
        bad.nodes[1].effects = vec![SkillEffect::DamageMultiplier(f32::NAN)];
        assert!(bad.validate().is_err());
        let mut extended = catalog;
        let mut node = extended.nodes.last().unwrap().clone();
        node.id = "new_authored_skill".into();
        node.position[0] += 200.0;
        extended.nodes.push(node);
        extended.validate().unwrap();
    }

    /// The shipped catalogue plus one probe node under shotgun_training.
    fn with_probe(effects: Vec<SkillEffect>, counters: Vec<&str>) -> SkillCatalog {
        let mut catalog = skills().clone();
        let mut node = catalog.node("shotgun_training").unwrap().clone();
        node.id = "perk_probe".into();
        node.prerequisites = vec!["shotgun_training".into()];
        node.position = [9_000.0, 9_000.0];
        node.effects = effects;
        node.counters = counters.into_iter().map(str::to_owned).collect();
        catalog.nodes.push(node);
        catalog
    }

    #[test]
    fn accepts_class_modifier_and_rejects_out_of_range() {
        use ClassStat::{Cadence, Damage, Reload, WeakBonus};
        use WeaponClass::{Marksman, Pistol, Rifle, Shotgun, Smg};
        let perk = |class, stat, value| SkillEffect::ClassModifier { class, stat, value };
        // The decision-19 shapes at their authored values validate and
        // round-trip through the tagged JSON form.
        let accepted = with_probe(
            vec![
                perk(Shotgun, Cadence, 0.8),
                perk(Shotgun, Reload, 0.8),
                perk(Marksman, Damage, 1.15),
                perk(Marksman, WeakBonus, 0.15),
                SkillEffect::ArmorPenetration(4),
                SkillEffect::HazardDamageMultiplier(0.6),
            ],
            vec!["shield-bearer", "sprinter-swarm"],
        );
        accepted.validate().unwrap();
        let text = serde_json::to_string(&accepted).unwrap();
        assert!(text.contains(
            r#"{"kind":"ClassModifier","value":{"class":"Shotgun","stat":"Cadence","value":0.8}}"#
        ));
        assert!(text.contains(r#"{"kind":"ArmorPenetration","value":4}"#));
        assert!(text.contains(r#"{"kind":"HazardDamageMultiplier","value":0.6}"#));
        assert!(text.contains(r#""counters":["shield-bearer","sprinter-swarm"]"#));
        assert_eq!(SkillCatalog::from_json(&text).unwrap(), accepted);
        // Folding: per-class multipliers multiply, the weak-spot bonus adds;
        // classes without a perk get no entry.
        let mut modifiers = SkillModifiers::default();
        for effect in &accepted.node("perk_probe").unwrap().effects {
            modifiers.apply(effect);
        }
        assert_eq!(
            modifiers.class_modifiers[&Shotgun],
            ClassModifiers {
                reload_multiplier: 0.8,
                cadence_multiplier: 0.8,
                ..ClassModifiers::default()
            }
        );
        assert_eq!(
            modifiers.class_modifiers[&Marksman],
            ClassModifiers {
                damage_multiplier: 1.15,
                weak_bonus_add: 0.15,
                ..ClassModifiers::default()
            }
        );
        assert_eq!(modifiers.class_modifiers.len(), 2);
        assert_eq!(modifiers.armor_penetration, 4);
        assert_eq!(modifiers.hazard_damage_multiplier, 0.6);
        modifiers.validate().unwrap();
        let neutral = ClassModifiers::default();
        assert_eq!(
            [
                neutral.damage_multiplier.to_bits(),
                neutral.reload_multiplier.to_bits(),
                neutral.cadence_multiplier.to_bits(),
                neutral.weak_bonus_add.to_bits(),
            ],
            [1.0f32.to_bits(), 1.0f32.to_bits(), 1.0f32.to_bits(), 0]
        );
        // Every out-of-range value fails on the effect rule itself.
        let rejected = [
            ("damage below 1", perk(Rifle, Damage, 0.99)),
            ("damage above 2", perk(Rifle, Damage, 2.01)),
            ("damage infinite", perk(Rifle, Damage, f32::INFINITY)),
            ("reload below 0.5", perk(Smg, Reload, 0.49)),
            ("reload above 1", perk(Smg, Reload, 1.01)),
            ("cadence NaN", perk(Pistol, Cadence, f32::NAN)),
            ("cadence below 0.5", perk(Pistol, Cadence, 0.4)),
            ("weak bonus 0", perk(Marksman, WeakBonus, 0.0)),
            ("weak bonus above 1", perk(Marksman, WeakBonus, 1.01)),
            ("weak bonus NaN", perk(Marksman, WeakBonus, f32::NAN)),
            ("penetration 0", SkillEffect::ArmorPenetration(0)),
            ("penetration negative", SkillEffect::ArmorPenetration(-1)),
            ("penetration 21", SkillEffect::ArmorPenetration(21)),
            (
                "hazard below 0.5",
                SkillEffect::HazardDamageMultiplier(0.49),
            ),
            ("hazard above 1", SkillEffect::HazardDamageMultiplier(1.01)),
            ("hazard NaN", SkillEffect::HazardDamageMultiplier(f32::NAN)),
        ];
        let passed: Vec<String> = rejected
            .into_iter()
            .filter_map(
                |(row, effect)| match with_probe(vec![effect], vec![]).validate() {
                    Err(error) if error == "Invalid effect in skill perk_probe" => None,
                    Err(error) => Some(format!("{row}: {error}")),
                    Ok(()) => Some(format!("{row}: validated")),
                },
            )
            .collect();
        assert_eq!(passed, Vec::<String>::new(), "rows that did not fail");
        // Counters name archetype ids: at most 8 distinct lowercase ids of
        // 1-48 bytes.
        let longest = "a".repeat(48);
        let too_long = "a".repeat(49);
        with_probe(vec![perk(Shotgun, Damage, 1.1)], vec![longest.as_str()])
            .validate()
            .unwrap();
        let counters = [
            ("empty", vec![""]),
            ("49 bytes", vec![too_long.as_str()]),
            ("uppercase", vec!["Shield-bearer"]),
            ("space", vec!["shield bearer"]),
            ("duplicate", vec!["exploder", "exploder"]),
            ("nine", vec!["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
        ];
        for (row, counters) in counters {
            let catalog = with_probe(vec![perk(Shotgun, Damage, 1.1)], counters);
            assert_eq!(
                catalog.validate(),
                Err("Invalid skill counters: perk_probe".into()),
                "{row}"
            );
        }
        // The shipped catalogue authors each of them since the decision-19
        // nodes (W2-41, tests/progression_tree.rs).
        let shipped = serde_json::to_string(skills()).unwrap();
        for key in [
            "ClassModifier",
            "ArmorPenetration",
            "HazardDamage",
            "counters",
        ] {
            assert!(shipped.contains(key), "{key}");
        }
    }

    #[test]
    fn unlocks_are_data_keyed_and_unknown_ids_are_rejected() {
        let catalog = skills().clone();
        let granted: BTreeSet<_> = catalog
            .nodes
            .iter()
            .flat_map(|n| &n.effects)
            .filter_map(|e| match e {
                SkillEffect::Unlock(id) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(granted, BTreeSet::from(unlocks::ALL));
        let grenade = catalog.node("grenade_training").unwrap();
        assert_eq!(grenade.effects, [SkillEffect::Unlock("grenade".into())]);
        let mut modifiers = SkillModifiers::default();
        modifiers.apply(&grenade.effects[0]);
        assert!(modifiers.unlocked(unlocks::GRENADE));
        assert!(!modifiers.unlocked(unlocks::MOLOTOV));
        modifiers.validate().unwrap();
        // A second node may grant an existing unlock with no new code.
        let mut extended = catalog.clone();
        let mut node = grenade.clone();
        node.id = "ordnance_drill".into();
        node.position[0] += 300.0;
        extended.nodes.push(node);
        extended.validate().unwrap();
        let mut bad = catalog;
        let index = bad.nodes.iter().position(|n| n.id == "grenade_training");
        bad.nodes[index.unwrap()].effects = vec![SkillEffect::Unlock("granade".into())];
        assert!(bad.validate().is_err());
        modifiers.unlocks.insert("granade".into());
        assert!(modifiers.validate().is_err());
    }

    #[test]
    fn every_effect_has_a_category_and_nodes_list_distinct_categories() {
        for category in EffectCategory::ALL {
            assert!(!category.label().is_empty());
        }
        let catalog = skills();
        let category = |id: &str| catalog.node(id).unwrap().categories();
        assert_eq!(category("quick_reload"), [EffectCategory::Firearms]);
        assert_eq!(category("piercing"), [EffectCategory::Firearms]);
        assert_eq!(category("engineering"), [EffectCategory::Fortification]);
        assert_eq!(category("trap_rank_2"), [EffectCategory::Fortification]);
        assert_eq!(category("shotgun_training"), [EffectCategory::WeaponClass]);
        assert_eq!(category("body_vitality_3"), [EffectCategory::Body]);
        assert_eq!(category("companion_contact"), [EffectCategory::Companions]);
        assert_eq!(category("grenade_training"), [EffectCategory::Ordnance]);
        assert_eq!(category("slow_burn"), [EffectCategory::Ordnance]);
        assert_eq!(category("sweep"), [EffectCategory::Melee]);
        assert_eq!(category("field_fitness"), [EffectCategory::Survival]);
        assert_eq!(category("brace"), [EffectCategory::Survival]);
        for node in &catalog.nodes {
            assert!(!node.categories().is_empty(), "{}", node.id);
            for effect in &node.effects {
                assert!(!effect.keywords().is_empty(), "{}", node.id);
            }
        }
        for id in unlocks::ALL {
            assert_ne!(unlock_label(id), id);
        }
        for feature in CampaignFeature::ALL {
            assert!(!feature.label().is_empty());
        }
    }

    #[test]
    fn search_matches_names_descriptions_branches_categories_and_keywords() {
        assert!(search_nodes("").is_empty());
        assert!(search_nodes("   ").is_empty());
        let ids = |query: &str| -> Vec<&str> {
            search_nodes(query).iter().map(|n| n.id.as_str()).collect()
        };
        assert!(ids("SHOTGUN").contains(&"shotgun_training"));
        assert!(ids("mara").contains(&"companion_contact"));
        let health = ids("Health");
        for id in ["field_fitness", "veteran_vitality", "body_vitality_1"] {
            assert!(health.contains(&id), "{id}");
        }
        let reload = ids("reload");
        assert!(reload.contains(&"quick_reload") && reload.contains(&"combat_reload"));
        assert!(reload.contains(&"body_coordination_4"));
        let grenade = ids(" grenade ");
        assert!(grenade.contains(&"grenade_training") && grenade.contains(&"grenade_bandolier"));
        assert!(ids("fortification").contains(&"trap_rank_3"));
        assert!(ids("weapon proficiency").contains(&"rifle_training"));
        assert!(ids("body development").contains(&"body_perception_20"));
        assert!(ids("zzzz-no-match").is_empty());
        // Catalogue order, no duplicates.
        let positions: Vec<_> = search_nodes("a")
            .iter()
            .map(|n| skills().nodes.iter().position(|m| m.id == n.id).unwrap())
            .collect();
        assert!(positions.len() > 10);
        assert!(positions.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn upgrade_goals_are_derived_from_effects() {
        let goals = upgrade_goals();
        let find = |label: &str| {
            goals
                .iter()
                .find(|g| g.label == label)
                .unwrap_or_else(|| panic!("missing goal {label}: {goals:?}"))
                .node_id
                .as_str()
        };
        assert_eq!(find("Shotgun proficiency"), "shotgun_training");
        assert_eq!(find("SMG proficiency"), "smg_training");
        assert_eq!(find("Rifle proficiency"), "rifle_training");
        assert_eq!(find("Marksman proficiency"), "marksman_training");
        assert_eq!(find("Machine gun proficiency"), "machine_gun_training");
        assert_eq!(find("Grenades"), "grenade_training");
        assert_eq!(find("Molotovs"), "molotov_training");
        assert_eq!(find("Fortifications"), "engineering");
        assert_eq!(find("Mara"), "companion_contact");
        assert_eq!(find("Recruitment"), "recruitment_network");
        assert_eq!(find("Parallel defenses"), "parallel_command");
        assert_eq!(find("Country operations"), "country_operations");
        // The innate pistol root is never a goal.
        assert!(goals.iter().all(|g| g.node_id != "pistol_training"));
        assert_eq!(goals.len(), 5 + 2 + 5);
        // A catalogue without a goal's effect omits it.
        let mut trimmed = skills().clone();
        trimmed
            .nodes
            .retain(|n| !n.effects.contains(&SkillEffect::Unlock("molotov".into())));
        assert!(
            trimmed
                .upgrade_goals()
                .iter()
                .all(|g| g.label != "Molotovs")
        );
    }
    /// The feature ids are saved in settings.json (`seen_callouts`, as
    /// "feature:<id>"), so each is pinned here; the list is append-only.
    #[test]
    fn campaign_feature_ids_are_stable_and_unique() {
        let ids: Vec<&str> = CampaignFeature::ALL.iter().map(|f| f.id()).collect();
        assert_eq!(
            ids,
            [
                "construction",
                "companion",
                "recruitment",
                "parallel",
                "operations"
            ]
        );
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), CampaignFeature::ALL.len());
        for id in ids {
            assert!(
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase()),
                "{id}"
            );
        }
    }
}
