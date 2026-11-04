//! Immutable, validated explorer world rules. Files are read only by hosts.
use crate::actors::ActorPreset;
use crate::scenario::Detection;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::scenario::ScenarioDoc;
use npc_framework::prelude::SimId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const PLAIN_EXPLORER: &str = "plain_explorer";
pub const BUILTIN_PRESETS: [(&str, &str); 6] = [
    (
        PLAIN_EXPLORER,
        include_str!("../worlds/plain_explorer.json"),
    ),
    ("survival", include_str!("../worlds/survival.json")),
    ("night_shift", include_str!("../worlds/night_shift.json")),
    (
        "island_village",
        include_str!("../worlds/island_village.json"),
    ),
    (
        "living_island",
        include_str!("../worlds/living_island.json"),
    ),
    (
        "living_homestead",
        include_str!("../worlds/living_homestead.json"),
    ),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldPreset {
    #[serde(default, skip_serializing_if = "EconomyModel::is_fixed")]
    pub economy: EconomyModel,
    #[serde(default, skip_serializing_if = "IslandGeography::is_compact")]
    pub island_geography: IslandGeography,
    #[serde(
        default,
        skip_serializing_if = "npc_framework::agent_io::scheduler::SchedulerPolicy::is_default"
    )]
    pub ai_scheduler: npc_framework::agent_io::scheduler::SchedulerPolicy,
    #[serde(default, skip_serializing_if = "WorldScene::is_explorer")]
    pub scene: WorldScene,
    pub name: String,
    pub description: String,
    pub needs: NeedsSpec,
    pub day_night: DayNightSpec,
    pub detection: Detection,
    #[serde(default)]
    pub perception: PerceptionRulesSpec,
    pub systems: SystemsSpec,
    pub inhabitants: InhabitantsSpec,
}
/// Opt-in content model; existing presets keep their exact fixed-economy shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomyModel {
    #[default]
    Fixed,
    NeedsDriven,
}
impl EconomyModel {
    pub fn is_fixed(&self) -> bool {
        *self == Self::Fixed
    }
}
/// Immutable construction profile, retained in each saved world's preset copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IslandGeography {
    #[default]
    Compact,
    Expanded,
}
impl IslandGeography {
    pub fn is_compact(&self) -> bool {
        *self == Self::Compact
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldScene {
    #[default]
    Explorer,
    IslandVillage,
}
impl WorldScene {
    fn is_explorer(&self) -> bool {
        *self == Self::Explorer
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedsSpec {
    pub hunger: NeedSpec,
    pub thirst: NeedSpec,
    pub energy: NeedSpec,
    pub battery: NeedSpec,
}
/// Badness rises from zero (full) toward 100000 (depleted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeedSpec {
    pub on: bool,
    pub rise_milli_per_tick: i32,
    pub low_milli: i32,
    pub out_milli: i32,
    pub low_speed_pct: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayNightSpec {
    pub on: bool,
    pub cycle_ticks: u64,
    pub night_vision_pct: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PerceptionRulesSpec {
    pub forget_after_ticks: u64,
    pub memory_capacity: u32,
}
impl Default for PerceptionRulesSpec {
    fn default() -> Self {
        Self {
            forget_after_ticks: 1200,
            memory_capacity: 128,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemsSpec {
    #[serde(default = "map_default")]
    pub map: bool,
    pub crafting: bool,
    pub vehicles: bool,
    pub locked_doors: bool,
    pub progression: bool,
}
fn map_default() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InhabitantsSpec {
    pub workers: u8,
    pub skeletons: u8,
}

pub fn validate(p: &WorldPreset) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if p.island_geography == IslandGeography::Expanded
        && (p.scene != WorldScene::IslandVillage || p.economy != EconomyModel::NeedsDriven)
    {
        errors.push("Expanded island geography requires the Living Island Village economy.".into());
    }
    if p.economy == EconomyModel::NeedsDriven && p.scene != WorldScene::IslandVillage {
        errors.push("The needs-driven economy requires the Island Village scene.".into());
    }
    if let Err(error) = p.ai_scheduler.validate() {
        errors.push(error);
    }
    if p.scene == WorldScene::IslandVillage {
        if !p.needs.hunger.on || !p.needs.thirst.on || !p.needs.energy.on || p.needs.battery.on {
            errors.push(
                "Island Village requires Hunger, Thirst and Energy on, and Battery off.".into(),
            );
        }
        if !p.systems.crafting {
            errors.push("Island Village needs Crafting enabled for its production chains.".into());
        }
        if p.inhabitants.workers != 0 || p.inhabitants.skeletons != 0 {
            errors.push(if p.economy == EconomyModel::NeedsDriven {
                "Living Island Village supplies its own twelve residents; extra workers and skeletons must be zero.".into()
            } else { "Island Village supplies its own nine residents; extra workers and skeletons must be zero.".into() });
        }
    }
    if p.name.trim().is_empty() {
        errors.push("name must not be empty".into());
    }
    for (name, n) in need_specs(p) {
        if !(0..=1000).contains(&n.rise_milli_per_tick) {
            errors.push(format!("{name} rate must be 0..=1000"));
        }
        if !(0..=100_000).contains(&n.low_milli)
            || !(0..=100_000).contains(&n.out_milli)
            || n.low_milli >= n.out_milli
        {
            errors.push(format!(
                "{name} thresholds require 0 <= low < out <= 100000"
            ));
        }
        if n.low_speed_pct > 100 {
            errors.push(format!("{name} low_speed_pct must be 0..=100"));
        }
    }
    if !(1200..=144000).contains(&p.day_night.cycle_ticks) {
        errors.push("cycle_ticks must be 1200..=144000".into());
    }
    if p.day_night.night_vision_pct > 100 {
        errors.push("night_vision_pct must be 0..=100".into());
    }
    if !(200..=12000).contains(&p.perception.forget_after_ticks) {
        errors.push("forget_after_ticks must be 200..=12000".into());
    }
    if !(16..=512).contains(&p.perception.memory_capacity) {
        errors.push("memory_capacity must be 16..=512".into());
    }
    if p.inhabitants.workers > 8 || p.inhabitants.skeletons > 8 {
        errors.push("inhabitants must be at most 8 of each kind".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
pub(crate) fn need_specs(p: &WorldPreset) -> [(&'static str, NeedSpec); 4] {
    [
        ("Hunger", p.needs.hunger),
        ("Thirst", p.needs.thirst),
        ("Energy", p.needs.energy),
        ("Battery", p.needs.battery),
    ]
}
pub fn builtin_preset(id: &str) -> Option<WorldPreset> {
    serde_json::from_str(BUILTIN_PRESETS.iter().find(|(n, _)| *n == id)?.1).ok()
}
pub fn plain_preset() -> WorldPreset {
    builtin_preset(PLAIN_EXPLORER).expect("shipped Plain world parses")
}
pub fn preset_id(path: &Path) -> Option<String> {
    crate::actors::preset_id(path)
}
fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
pub fn install_builtins(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let existing = files(dir)?;
    if !existing.is_empty() {
        // Preserve an explicitly broken/disabled installation instead of masking its errors.
        let usable = existing.iter().any(|path| {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|text| {
                    serde_json::from_str::<WorldPreset>(text.trim_start_matches('\u{feff}')).ok()
                })
                .is_some_and(|preset| validate(&preset).is_ok())
        });
        if !usable {
            return Ok(());
        }
        // A new shipped scene is additive to existing installations; never replace a user's preset.
        for (id, text) in BUILTIN_PRESETS
            .iter()
            .filter(|(id, _)| matches!(*id, "island_village" | "living_island"))
        {
            let path = dir.join(format!("{id}.json"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        return Ok(());
    }
    for (id, text) in BUILTIN_PRESETS {
        let path = dir.join(format!("{id}.json"));
        if !path.exists() {
            crate::explorer::workspace::write_atomic(&path, text).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub fn load_worlds(dir: &Path) -> Vec<(PathBuf, Result<WorldPreset, String>)> {
    let paths = install_builtins(dir).and_then(|()| files(dir));
    match paths {
        Err(e) => vec![(dir.into(), Err(e))],
        Ok(paths) => paths
            .into_iter()
            .map(|path| {
                let parsed = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|text| {
                        serde_json::from_str(text.trim_start_matches('\u{feff}'))
                            .map_err(|e| e.to_string())
                    });
                (path, parsed)
            })
            .collect(),
    }
}
pub fn find_preset(dir: &Path, id: &str) -> Result<WorldPreset, String> {
    let mut names = Vec::new();
    for (path, result) in load_worlds(dir) {
        if path == dir {
            return Err(result.err().unwrap_or_default());
        }
        let name = preset_id(&path).unwrap_or_default();
        if name == id {
            return result.map_err(|e| format!("world preset `{id}`: {e}"));
        }
        if result.is_ok() {
            names.push(name);
        }
    }
    Err(format!(
        "unknown world preset `{id}` (there: {})",
        names.join(", ")
    ))
}
#[derive(Resource, Clone, Debug, Default)]
pub struct WorldPresets(pub BTreeMap<String, WorldPreset>);
impl WorldPresets {
    pub fn one(id: &str, preset: WorldPreset) -> Self {
        Self(BTreeMap::from([(id.into(), preset)]))
    }
}
pub fn resolve_preset(world: &World, id: &str) -> Option<WorldPreset> {
    world
        .get_resource::<WorldPresets>()
        .and_then(|p| p.0.get(id).cloned())
        .or_else(|| builtin_preset(id))
}
pub fn world_doc(p: &WorldPreset, actor: &ActorPreset) -> Result<ScenarioDoc, String> {
    validate(p).map_err(|e| e.join("; "))?;
    let mut doc = crate::scenario::scenario_doc("explorer")?;
    if p.scene == WorldScene::IslandVillage {
        if actor.body != crate::actors::Body::Human {
            return Err(
                "The island village requires a human actor with Hunger, Thirst and Energy.".into(),
            );
        }
        doc.world["layout"] = "island".into();
        doc.world["furnish"] = false.into();
        doc.actors[0].pos = Some((500, -1500));
    }
    doc.world["store_locked"] = p.systems.locked_doors.into();
    doc.world["detection"] = serde_json::to_value(p.detection).map_err(|e| e.to_string())?;
    doc.actors[0].extra.insert("preset".into(), "human".into());
    crate::explorer::context::ExplorerContext::for_world(
        "explorer",
        "human",
        actor,
        PLAIN_EXPLORER,
        p,
    )
    .write_to(&mut doc)?;
    Ok(doc)
}
pub fn prepare(app: &mut App, p: &WorldPreset) {
    crate::world_runtime::prepare(app, p);
}
pub fn apply(world: &mut World, played: SimId, p: &WorldPreset) {
    crate::world_runtime::apply(world, played, p);
}
