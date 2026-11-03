//! Validated, engine-free enemy composition. Resolved profiles are save snapshots.
use crate::content::{BossKind, EnemyKind};
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
    pub visual: EnemyVisual,
    pub unlocked_by: Option<BossKind>,
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
            || !(0..=1000).contains(&self.armor)
            || !(0..=100).contains(&self.npc_armor)
            || !(0..=20).contains(&self.npc_dodge)
            || !(1..=1_000_000).contains(&self.reward)
            || self.xp > 1_000_000
            || !finite_range(self.visual.scale, 0.25, 3.0)
            || self.visual.tint.iter().any(|v| !finite_range(*v, 0.0, 1.0))
        {
            return Err(format!(
                "Invalid resolved enemy profile: {}",
                self.archetype_id
            ));
        }
        Ok(())
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
    defaults: [String; 5],
    waves: Vec<Vec<WeightedEnemy>>,
}

impl EnemyCatalog {
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
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > MAX_JSON_BYTES {
            return Err("Enemy JSON exceeds the supported 4 MiB authoring limit".into());
        }
        let authored: AuthoredCatalog =
            serde_json::from_str(text).map_err(|e| format!("Enemy JSON: {e}"))?;
        authored.resolve()
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
        &self.defaults[kind as usize]
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

pub fn enemy_catalog() -> &'static EnemyCatalog {
    static CATALOG: OnceLock<EnemyCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let (text, source) = match std::env::var_os("LASTLIGHT_ENEMIES") {
            Some(path) => {
                let metadata = std::fs::metadata(&path).unwrap_or_else(|e| {
                    panic!("Could not inspect LASTLIGHT_ENEMIES {path:?}: {e}")
                });
                if metadata.len() > MAX_JSON_BYTES as u64 {
                    panic!(
                        "LASTLIGHT_ENEMIES {path:?} exceeds the supported 4 MiB authoring limit"
                    );
                }
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("Could not read LASTLIGHT_ENEMIES {path:?}: {e}"));
                (text, format!("{path:?}"))
            }
            None => (
                include_str!("../enemy_catalog.json").to_owned(),
                "bundled enemy_catalog.json".to_owned(),
            ),
        };
        EnemyCatalog::from_json(&text)
            .unwrap_or_else(|error| panic!("Invalid enemy catalogue from {source}: {error}"))
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
}

fn valid_id(id: &str) -> bool {
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
            {
                return Err(format!("Invalid attack component: {}", a.id));
            }
        }
        for d in &self.defenses {
            if !(0..=1000).contains(&d.armor)
                || !(0..=100).contains(&d.npc_armor)
                || !(0..=20).contains(&d.npc_dodge)
                || !finite_range(d.contact_armor_fraction, 0.0, 1.0)
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
        for (id, a) in archetypes {
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
            };
            enemy.validate()?;
            resolved.insert(id.to_owned(), enemy);
        }
        if self.defaults.len() != EnemyKind::ALL.len() {
            return Err(
                "Exactly one unrestricted default per legacy enemy kind is required".into(),
            );
        }
        let mut defaults: [String; 5] = std::array::from_fn(|_| String::new());
        for default in self.defaults {
            let enemy = resolved
                .get(&default.archetype_id)
                .ok_or_else(|| format!("Unknown enemy default: {}", default.archetype_id))?;
            let target = &mut defaults[default.kind as usize];
            if !target.is_empty() || enemy.kind != default.kind || enemy.unlocked_by.is_some() {
                return Err("Duplicate, mismatched or locked legacy enemy default".into());
            }
            *target = default.archetype_id;
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
        Ok(EnemyCatalog {
            resolved,
            defaults,
            waves,
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
        assert_eq!(pool, catalog.wave_pool(5, &[]));
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
}
