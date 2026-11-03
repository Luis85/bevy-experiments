//! Validated, extensible skill DAG. Ownership stores stable string IDs, never node indices.
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
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum SkillEffect {
    BodyDevelopment { attribute: BodyAttribute, rank: u32 },
    Blueprint(StructureKind),
    StructureRank { kind: StructureKind, rank: u32 },
    FeatureUnlock(CampaignFeature),
    DamageAdd(i32),
    DamageMultiplier(f32),
    ReloadMultiplier(f32),
    CadenceMultiplier(f32),
    HealthAdd(i32),
    StaminaAdd(i32),
    ArmorAdd(i32),
    MeleeAdd(i32),
    Piercing,
    Sweep,
    Heal,
    Brace,
    EngineeringMultiplier(f32),
    WeaponTraining(WeaponClass),
    GrenadeUnlock,
    MolotovUnlock,
    GrenadeCharges(u32),
    MolotovCharges(u32),
    OrdnanceDamageMultiplier(f32),
    OrdnanceRadiusMultiplier(f32),
    BurnDurationMultiplier(f32),
    OrdnanceCooldownMultiplier(f32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CampaignFeature {
    Construction,
    Companion,
    Recruitment,
    Parallel,
    Operations,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillModifiers {
    pub body_development: [u32; 5],
    pub blueprints: [bool; 3],
    pub structure_ranks: [u32; 3],
    pub features: BTreeSet<CampaignFeature>,
    pub damage_add: i32,
    pub damage_multiplier: f32,
    pub reload_multiplier: f32,
    pub cadence_multiplier: f32,
    pub health_add: i32,
    pub stamina_add: i32,
    pub armor_add: i32,
    pub melee_add: i32,
    pub piercing: bool,
    pub sweep: bool,
    pub heal: bool,
    pub brace: bool,
    pub engineering: bool,
    pub engineering_multiplier: f32,
    pub weapon_classes: BTreeSet<WeaponClass>,
    pub grenade_unlocked: bool,
    pub molotov_unlocked: bool,
    pub grenade_charges: u32,
    pub molotov_charges: u32,
    pub ordnance_damage_multiplier: f32,
    pub ordnance_radius_multiplier: f32,
    pub burn_duration_multiplier: f32,
    pub ordnance_cooldown_multiplier: f32,
}
impl Default for SkillModifiers {
    fn default() -> Self {
        Self {
            body_development: [0; 5],
            blueprints: [false; 3],
            structure_ranks: [0; 3],
            features: BTreeSet::new(),
            damage_add: 0,
            damage_multiplier: 1.0,
            reload_multiplier: 1.0,
            cadence_multiplier: 1.0,
            health_add: 0,
            stamina_add: 0,
            armor_add: 0,
            melee_add: 0,
            piercing: false,
            sweep: false,
            heal: false,
            brace: false,
            engineering: false,
            engineering_multiplier: 1.0,
            weapon_classes: BTreeSet::new(),
            grenade_unlocked: false,
            molotov_unlocked: false,
            grenade_charges: 2,
            molotov_charges: 2,
            ordnance_damage_multiplier: 1.0,
            ordnance_radius_multiplier: 1.0,
            burn_duration_multiplier: 1.0,
            ordnance_cooldown_multiplier: 1.0,
        }
    }
}
impl SkillModifiers {
    /// Validate frozen save snapshots independently of the current catalogue.
    pub fn validate(&self) -> Result<(), String> {
        if self.body_development.iter().any(|rank| *rank > 1000)
            || self.structure_ranks.iter().any(|rank| *rank > 10)
            || [
                self.damage_add,
                self.health_add,
                self.stamina_add,
                self.armor_add,
                self.melee_add,
            ]
            .into_iter()
            .any(|v| !(0..=1_000_000).contains(&v))
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
        {
            return Err("Saved skill modifiers exceed supported finite bounds".into());
        }
        Ok(())
    }
    pub fn apply(&mut self, effect: &SkillEffect) {
        match *effect {
            SkillEffect::BodyDevelopment { attribute, rank } => {
                self.body_development[attribute.index()] =
                    self.body_development[attribute.index()].max(rank)
            }
            SkillEffect::Blueprint(kind) => self.blueprints[kind.index()] = true,
            SkillEffect::StructureRank { kind, rank } => {
                self.structure_ranks[kind.index()] = self.structure_ranks[kind.index()].max(rank)
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
            SkillEffect::Piercing => self.piercing = true,
            SkillEffect::Sweep => self.sweep = true,
            SkillEffect::Heal => self.heal = true,
            SkillEffect::Brace => self.brace = true,
            SkillEffect::EngineeringMultiplier(n) => {
                self.engineering = true;
                self.engineering_multiplier *= n;
            }
            SkillEffect::WeaponTraining(class) => {
                self.weapon_classes.insert(class);
            }
            SkillEffect::GrenadeUnlock => self.grenade_unlocked = true,
            SkillEffect::MolotovUnlock => self.molotov_unlocked = true,
            SkillEffect::GrenadeCharges(n) => self.grenade_charges += n,
            SkillEffect::MolotovCharges(n) => self.molotov_charges += n,
            SkillEffect::OrdnanceDamageMultiplier(n) => self.ordnance_damage_multiplier *= n,
            SkillEffect::OrdnanceRadiusMultiplier(n) => self.ordnance_radius_multiplier *= n,
            SkillEffect::BurnDurationMultiplier(n) => self.burn_duration_multiplier *= n,
            SkillEffect::OrdnanceCooldownMultiplier(n) => self.ordnance_cooldown_multiplier *= n,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillCatalog {
    pub nodes: Vec<SkillNode>,
}
impl SkillCatalog {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let catalog: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn node(&self, id: &str) -> Option<&SkillNode> {
        self.nodes.iter().find(|n| n.id == id)
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
                    _ => true,
                };
                if !valid {
                    return Err(format!("Invalid effect in skill {}", n.id));
                }
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
        ] {
            if !factor.is_finite() || !(0.01..=100.0).contains(&factor) {
                return Err("Combined skill multipliers exceed supported bounds".into());
            }
        }
        Ok(())
    }
}
pub fn skills() -> &'static SkillCatalog {
    static SKILLS: OnceLock<SkillCatalog> = OnceLock::new();
    SKILLS.get_or_init(|| {
        let (text, source) = match std::env::var_os("LASTLIGHT_SKILLS") {
            Some(path) => {
                if std::fs::metadata(&path).is_ok_and(|m| m.len() > 4 * 1024 * 1024) {
                    panic!("LASTLIGHT_SKILLS exceeds 4 MiB authoring limit");
                }
                (
                    std::fs::read_to_string(&path).unwrap_or_else(|e| {
                        panic!("Could not read LASTLIGHT_SKILLS {path:?}: {e}")
                    }),
                    format!("{path:?}"),
                )
            }
            None => (
                include_str!("../skills.json").to_owned(),
                "bundled skills.json".to_owned(),
            ),
        };
        SkillCatalog::from_json(&text)
            .unwrap_or_else(|e| panic!("Invalid skills from {source}: {e}"))
    })
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
}
