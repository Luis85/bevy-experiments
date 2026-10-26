//! Actor presets (spec G9–G11, plan Task 11): what an actor is, as data.
//!
//! A preset is one JSON file in the actors folder (`Settings::actors_dir`,
//! default `<agents folder>/../npc-actors`): a name and description, a body
//! (`human` or `robot`) and a 3D model ([`MODELS`]), the character sheet
//! ([`SheetSpec`], point-buy validated with the rules registry within
//! [`PRESET_POINT_BUDGET`]), movement speed, carrying capacity, the actions
//! it may take and its sensors ([`SensorSpec`]). Memory span and capacity
//! are world settings (map/memory spec M1), not preset fields.
//!
//! The game ships [`BUILTIN_PRESETS`] (human, rover, sentinel), copied into
//! an actors folder that has no preset yet ([`load_presets`]). A preset's id
//! is its file name without `.json` (`human`); `--preset` and `agent.json`
//! name it by id.
//!
//! - [`validate`] checks a preset against a built world (its rules, verbs and
//!   detection check) and gives one reason per problem; an invalid preset is
//!   never spawned. The robot sensors (LIDAR, camera, radar) are real
//!   sensors since Task 12 (E2): the built-in rover and sentinel are valid.
//! - [`apply_preset`] gives an actor the preset: sheet, speed, carrying
//!   capacity, [`Sensors`], [`ReachSense`], [`AllowedActions`] and
//!   [`ActorModel`]. A scenario actor with `"preset": "<id>"` gets it at
//!   spawn ([`crate::layout::spawn_unit`], from the [`ActorPresets`]
//!   resource or the built-ins).
//! - [`AllowedActions`] and [`ActorModel`] are snapshot-only (never hashed,
//!   ADR 0013). The verb gate [`verb_gate`] refuses every known action
//!   ([`known_actions`]) outside the set: its affordances say "this actor
//!   cannot <verb>" and `agent/act` rejects it.
//!
//! Content is sim code: integers only; no clock; the folder is read only by
//! the host (`explorer::host`), never by the sim.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::*;
use npc_framework::glue::{CRAFT, DRIVE_TO, UNLOAD};
use npc_framework::interaction::{InteractionRegistry, Inventory};
use npc_framework::perception::{ALL_KINDS, DetectionChecks, ReachSense, SensorReadings, Sensors};
use npc_framework::prelude::{CharacterSheet, Commandable};
use npc_framework::rules::{Attributes, with_rules};
use serde::{Deserialize, Serialize};

use crate::world::TestbedUnit;

/// The built-in presets `(id, JSON)`, by id.
pub const BUILTIN_PRESETS: [(&str, &str); 3] = [
    ("human", include_str!("../actors/human.json")),
    ("rover", include_str!("../actors/rover.json")),
    ("sentinel", include_str!("../actors/sentinel.json")),
];

/// The default preset (the human scout of every explorer before presets).
pub const HUMAN: &str = "human";

/// Most character points a preset's sheet may be worth (spent plus
/// unspent). The scout is worth 64.
pub const PRESET_POINT_BUDGET: i32 = 100;

/// The models a preset may name: the testbed's characters and the robots
/// (drawn from primitives from Task 13 on).
pub const MODELS: [&str; 6] = [
    "rogue",
    "knight",
    "skeleton_minion",
    "skeleton_warrior",
    "rover",
    "sentinel",
];

/// Longest sensor range, mm.
pub const RANGE_MAX_MM: i32 = 50_000;
/// Longest reach, mm (0: none).
pub const REACH_MAX_MM: i32 = 5_000;
/// Fastest speed, mm per tick (10 m/s at 20 Hz).
pub const SPEED_MAX: i32 = 200;
/// Heaviest carrying capacity, g.
pub const CARRY_MAX_G: u32 = 1_000_000;

/// What a preset's actor is built as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    /// A person: senses with eyes and ears.
    Human,
    /// A robot: senses only through its sensors.
    Robot,
}

impl Body {
    /// `human` or `robot`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Robot => "robot",
        }
    }
}

/// The four attributes (bought levels; the rule set's base costs nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributesSpec {
    #[serde(default = "ten")]
    pub st: i32,
    #[serde(default = "ten")]
    pub dx: i32,
    #[serde(default = "ten")]
    pub iq: i32,
    #[serde(default = "ten")]
    pub ht: i32,
}

fn ten() -> i32 {
    10
}

impl Default for AttributesSpec {
    fn default() -> Self {
        Self {
            st: 10,
            dx: 10,
            iq: 10,
            ht: 10,
        }
    }
}

/// The character sheet by name: attributes, traits (name → level), skills
/// (name → points) and banked points (the rules' point-buy).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetSpec {
    #[serde(default)]
    pub attributes: AttributesSpec,
    #[serde(default)]
    pub traits: BTreeMap<String, u8>,
    #[serde(default)]
    pub skills: BTreeMap<String, i32>,
    #[serde(default)]
    pub unspent: i32,
}

impl SheetSpec {
    /// The rules' sheet (no secondary buys).
    pub fn to_sheet(&self) -> CharacterSheet {
        let a = self.attributes;
        CharacterSheet {
            attributes: Attributes {
                st: a.st,
                dx: a.dx,
                iq: a.iq,
                ht: a.ht,
            },
            secondary: Default::default(),
            traits: self.traits.clone(),
            skills: self.skills.clone(),
            unspent: self.unspent,
        }
    }
}

/// One sensor (`{"type": "vision", ..}`). Distances in mm, angles in
/// degrees (`half_fov_deg`: to each side of the facing), `noise_ppm` parts
/// per million, `bearing_res_mdeg` millidegrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SensorSpec {
    /// A vision cone with line of sight.
    Vision { range_mm: i32, half_fov_deg: u16 },
    /// Hearing all around.
    Hearing { radius_mm: i32 },
    /// A planar laser scan (`npc_perception::Lidar`).
    Lidar {
        range_mm: i32,
        fov_deg: u16,
        beams: u16,
        noise_ppm: u32,
    },
    /// Object detections, no images (`npc_perception::Camera`).
    Camera { range_mm: i32, half_fov_deg: u16 },
    /// Radar tracks (`npc_perception::Radar`).
    Radar {
        range_mm: i32,
        half_fov_deg: u16,
        range_res_mm: i32,
        bearing_res_mdeg: i32,
    },
    /// The vicinity sense all round, through nothing solid; lifts fog
    /// ([`ReachSense`]; 0: none).
    Reach { radius_mm: i32 },
}

/// Perception settings of a preset. Memory span and capacity are world
/// settings (map/memory spec M1); the detection check is too (one per
/// world), so `detection` may only name the world's active check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerceptionSpec {
    /// The detection check by name; `null`: the world's.
    #[serde(default)]
    pub detection: Option<String>,
}

/// One actor preset (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorPreset {
    /// The name shown to the dev and the agent (`Human scout`).
    pub name: String,
    /// What the body can do, a sentence or two.
    pub description: String,
    pub body: Body,
    /// The 3D model ([`MODELS`]).
    pub model: String,
    pub sheet: SheetSpec,
    pub speed_mm_per_tick: i32,
    /// Most weight carried, g.
    pub carry_capacity: u32,
    /// The actions it may take (verb names, [`known_actions`]).
    pub actions: BTreeSet<String>,
    pub sensors: Vec<SensorSpec>,
    #[serde(default)]
    pub perception: PerceptionSpec,
}

impl ActorPreset {
    /// The reach radius (the last `reach` sensor), `None` without one or
    /// at 0.
    pub fn reach_mm(&self) -> Option<i32> {
        self.sensors
            .iter()
            .rev()
            .find_map(|s| match s {
                SensorSpec::Reach { radius_mm } => Some(*radius_mm),
                _ => None,
            })
            .filter(|&r| r > 0)
    }
}

/// The built-in preset `id`, parsed.
pub fn builtin_preset(id: &str) -> Option<ActorPreset> {
    let (_, text) = BUILTIN_PRESETS.iter().find(|(n, _)| *n == id)?;
    // A built-in that does not parse is a bug the tests catch.
    serde_json::from_str(text).ok()
}

/// The built-in human (the default preset).
pub fn human_preset() -> ActorPreset {
    builtin_preset(HUMAN).unwrap_or_else(|| panic!("the built-in `{HUMAN}` preset parses"))
}

/// The id of a preset file: its name without `.json`.
pub fn preset_id(path: &Path) -> Option<String> {
    path.file_stem().map(|s| s.to_string_lossy().into_owned())
}

fn is_preset_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        && !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

/// The preset files in `dir`, sorted by file name.
fn preset_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_preset_file(p))
        .collect();
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(files)
}

/// Creates `dir` if needed and, when it holds no preset file, writes the
/// built-ins into it (each atomically, a file that appeared meanwhile is
/// kept).
///
/// # Errors
/// When the folder cannot be created, read or written.
pub fn install_builtins(dir: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("the actors folder {}: {e}", dir.display());
    std::fs::create_dir_all(dir).map_err(err)?;
    if !preset_files(dir).map_err(err)?.is_empty() {
        return Ok(());
    }
    for (id, text) in BUILTIN_PRESETS {
        let path = dir.join(format!("{id}.json"));
        if !path.exists() {
            crate::explorer::workspace::write_atomic(&path, text).map_err(err)?;
        }
    }
    Ok(())
}

/// Every preset file in `dir`, sorted by file name, parsed (a leading BOM is
/// tolerated): the preset or why it does not read. The built-ins are copied
/// first into a folder without presets ([`install_builtins`]). A folder that
/// cannot be read gives one entry: `dir` and the error.
pub fn load_presets(dir: &Path) -> Vec<(PathBuf, Result<ActorPreset, String>)> {
    if let Err(e) = install_builtins(dir) {
        return vec![(dir.to_path_buf(), Err(e))];
    }
    let files = match preset_files(dir) {
        Ok(f) => f,
        Err(e) => {
            let e = format!("the actors folder {}: {e}", dir.display());
            return vec![(dir.to_path_buf(), Err(e))];
        }
    };
    files
        .into_iter()
        .map(|path| {
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    serde_json::from_str::<ActorPreset>(t.trim_start_matches('\u{feff}'))
                        .map_err(|e| e.to_string())
                });
            (path, parsed)
        })
        .collect()
}

/// Preset `id` from `dir` ([`load_presets`]), not validated yet.
///
/// # Errors
/// An unknown id (with the ids that read), or why its file does not read.
pub fn find_preset(dir: &Path, id: &str) -> Result<ActorPreset, String> {
    let loaded = load_presets(dir);
    let mut readable = Vec::new();
    for (path, preset) in loaded {
        if !is_preset_file(&path) {
            if let Err(e) = preset {
                return Err(format!("unknown actor preset `{id}`: {e}"));
            }
            continue;
        }
        let this = preset_id(&path).unwrap_or_default();
        if this == id {
            return preset.map_err(|e| format!("actor preset `{id}` does not read: {e}"));
        }
        if preset.is_ok() {
            readable.push(this);
        }
    }
    Err(format!(
        "unknown actor preset `{id}` in {} (there: {})",
        dir.display(),
        if readable.is_empty() {
            "none".to_owned()
        } else {
            readable.join(", ")
        }
    ))
}

/// The actions a preset may allow or withhold: the interaction verbs
/// registered in the app plus `craft` and the vehicle verbs `drive_to` and
/// `unload`.
pub fn known_actions(world: &World) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = world
        .get_resource::<InteractionRegistry>()
        .map(|r| r.verbs.keys().map(|v| (*v).to_owned()).collect())
        .unwrap_or_default();
    out.extend([CRAFT, DRIVE_TO, UNLOAD].map(String::from));
    out
}

fn is_known_action(world: &World, verb: &str) -> bool {
    [CRAFT, DRIVE_TO, UNLOAD].contains(&verb)
        || world
            .get_resource::<InteractionRegistry>()
            .is_some_and(|r| r.verb(verb).is_some())
}

fn check_range(out: &mut Vec<String>, what: &str, v: i32, lo: i32, hi: i32) {
    if !(lo..=hi).contains(&v) {
        out.push(format!("{what} {v} is out of {lo}..={hi}"));
    }
}

fn check_sensor(out: &mut Vec<String>, s: &SensorSpec) {
    let range = |out: &mut Vec<String>, kind: &str, v: i32| {
        check_range(out, &format!("{kind} range_mm"), v, 1, RANGE_MAX_MM);
    };
    let fov = |out: &mut Vec<String>, kind: &str, field: &str, v: u16, hi: i32| {
        check_range(out, &format!("{kind} {field}"), i32::from(v), 1, hi);
    };
    match *s {
        SensorSpec::Vision {
            range_mm,
            half_fov_deg,
        } => {
            range(out, "vision", range_mm);
            fov(out, "vision", "half_fov_deg", half_fov_deg, 180);
        }
        SensorSpec::Hearing { radius_mm } => {
            check_range(out, "hearing radius_mm", radius_mm, 1, RANGE_MAX_MM);
        }
        SensorSpec::Reach { radius_mm } => {
            check_range(out, "reach radius_mm", radius_mm, 0, REACH_MAX_MM);
        }
        SensorSpec::Lidar {
            range_mm,
            fov_deg,
            beams,
            noise_ppm,
        } => {
            range(out, "lidar", range_mm);
            fov(out, "lidar", "fov_deg", fov_deg, 360);
            check_range(out, "lidar beams", i32::from(beams), 1, 720);
            if noise_ppm > 1_000_000 {
                out.push(format!("lidar noise_ppm {noise_ppm} is over 1000000"));
            }
        }
        SensorSpec::Camera {
            range_mm,
            half_fov_deg,
        } => {
            range(out, "camera", range_mm);
            fov(out, "camera", "half_fov_deg", half_fov_deg, 180);
        }
        SensorSpec::Radar {
            range_mm,
            half_fov_deg,
            range_res_mm,
            bearing_res_mdeg,
        } => {
            range(out, "radar", range_mm);
            fov(out, "radar", "half_fov_deg", half_fov_deg, 180);
            check_range(out, "radar range_res_mm", range_res_mm, 1, range_mm.max(1));
            check_range(out, "radar bearing_res_mdeg", bearing_res_mdeg, 1, 180_000);
        }
    }
}

fn check_sheet(out: &mut Vec<String>, sheet: &SheetSpec, world: &World) {
    with_rules(world, |r| {
        let limits = r.set.limits;
        let a = sheet.attributes;
        for (name, v) in [("ST", a.st), ("DX", a.dx), ("IQ", a.iq), ("HT", a.ht)] {
            check_range(
                out,
                &format!("attribute {name}"),
                v,
                1,
                limits.attribute_max,
            );
        }
        for (name, &level) in &sheet.traits {
            match r.registry.trait_def(name) {
                None => out.push(format!(
                    "unknown trait `{name}` (known: {})",
                    r.registry.trait_names()
                )),
                Some(d) => check_range(
                    out,
                    &format!("level of `{name}`"),
                    i32::from(level),
                    1,
                    i32::from(d.max_level),
                ),
            }
        }
        for (name, &points) in &sheet.skills {
            if r.registry.skill(name).is_none() {
                out.push(format!(
                    "unknown skill `{name}` (known: {})",
                    r.registry.skill_names()
                ));
            } else {
                check_range(
                    out,
                    &format!("points in `{name}`"),
                    points,
                    1,
                    limits.skill_points_max,
                );
            }
        }
        if sheet.unspent < 0 {
            out.push(format!("unspent points {} are negative", sheet.unspent));
        }
        let total = r.points(&sheet.to_sheet()).total;
        if total > PRESET_POINT_BUDGET {
            out.push(format!(
                "the sheet is worth {total} points, over the budget of {PRESET_POINT_BUDGET}"
            ));
        }
    });
}

/// Every problem of `p` in `world` (a built, frozen world: its rules, verbs
/// and detection check), one reason each: the sheet (known traits and
/// skills, limits, [`PRESET_POINT_BUDGET`]), speed `1..=`[`SPEED_MAX`],
/// carrying capacity, sensor ranges `1..=`[`RANGE_MAX_MM`] mm (reach
/// `0..=`[`REACH_MAX_MM`]), fields of view `1..=180` (a LIDAR's full field
/// `1..=360`), beams `1..=720`, at most one `reach`, `lidar`, `camera` and
/// `radar` each (an actor reports one of each), known actions, a known
/// model, the detection check.
///
/// # Errors
/// The reasons, in field order.
pub fn validate(p: &ActorPreset, world: &World) -> Result<(), Vec<String>> {
    let mut out = Vec::new();
    if p.name.trim().is_empty() {
        out.push("the name is empty".to_owned());
    }
    if !MODELS.contains(&p.model.as_str()) {
        out.push(format!(
            "unknown model `{}` (known: {})",
            p.model,
            MODELS.join(", ")
        ));
    }
    check_sheet(&mut out, &p.sheet, world);
    check_range(
        &mut out,
        "speed_mm_per_tick",
        p.speed_mm_per_tick,
        1,
        SPEED_MAX,
    );
    if p.carry_capacity > CARRY_MAX_G {
        out.push(format!(
            "carry_capacity {} g is over {CARRY_MAX_G} g",
            p.carry_capacity
        ));
    }
    let known = known_actions(world);
    for a in p.actions.iter().filter(|a| !known.contains(*a)) {
        out.push(format!(
            "unknown action `{a}` (known: {})",
            known.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    for s in &p.sensors {
        check_sensor(&mut out, s);
    }
    let reaches = p
        .sensors
        .iter()
        .filter(|s| matches!(s, SensorSpec::Reach { .. }))
        .count();
    if reaches > 1 {
        out.push(format!("{reaches} reach sensors: give at most one"));
    }
    for (kind, is) in [
        (
            "lidar",
            (|s| matches!(s, SensorSpec::Lidar { .. })) as fn(&SensorSpec) -> bool,
        ),
        ("camera", |s| matches!(s, SensorSpec::Camera { .. })),
        ("radar", |s| matches!(s, SensorSpec::Radar { .. })),
    ] {
        let n = p.sensors.iter().filter(|s| is(s)).count();
        if n > 1 {
            out.push(format!("{n} {kind} sensors: give at most one"));
        }
    }
    if let Some(d) = &p.perception.detection {
        let active = world
            .get_resource::<DetectionChecks>()
            .map_or("automatic", |c| c.active);
        if d != active {
            out.push(format!(
                "detection `{d}` is not this world's (`{active}`): the detection check is a \
                 world setting"
            ));
        }
    }
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// The actions an actor made from a preset may take; every other known
/// action ([`known_actions`]) is refused ([`verb_gate`]). Snapshot-only
/// (`allowed_actions`), never hashed. An actor without it may do everything.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedActions(pub BTreeSet<String>);

/// The model the view draws an actor made from a preset with ([`MODELS`]).
/// Snapshot-only (`actor_model`), never hashed.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorModel(pub String);

/// Snapshot name of [`AllowedActions`].
pub const ALLOWED_ACTIONS: &str = "allowed_actions";
/// Snapshot name of [`ActorModel`].
pub const ACTOR_MODEL: &str = "actor_model";
/// Name of the verb gate ([`verb_gate`]).
pub const ALLOWED_ACTIONS_GATE: &str = "testbed.allowed_actions";

/// The presets a scenario's actors may name besides the built-ins, by id
/// (the host inserts the one it chose before the world spawns). Not sim
/// state: neither hashed nor saved.
#[derive(Resource, Clone, Debug, Default)]
pub struct ActorPresets(pub BTreeMap<String, ActorPreset>);

impl ActorPresets {
    /// Just `preset` as `id`.
    pub fn one(id: &str, preset: ActorPreset) -> Self {
        Self(BTreeMap::from([(id.to_owned(), preset)]))
    }
}

/// Preset `id` for a scenario actor: the [`ActorPresets`] entry, else the
/// built-in.
pub fn resolve_preset(world: &World, id: &str) -> Option<ActorPreset> {
    world
        .get_resource::<ActorPresets>()
        .and_then(|p| p.0.get(id).cloned())
        .or_else(|| builtin_preset(id))
}

/// Gives `entity` preset `p` (validated by the caller): its sheet, speed
/// (`TestbedUnit` and `Commandable`), carrying capacity (`Inventory`), its
/// sensors ([`Sensors`]: vision, hearing, LIDAR, camera and radar, interest
/// kept), its reach ([`ReachSense`]; none at 0), [`AllowedActions`] and
/// [`ActorModel`]. A robot has no eyes or ears unless its preset lists
/// them: it senses through its LIDAR, camera, radar and reach only, so its
/// memory and affordances follow them. Leaves everything else (fog of war,
/// memory) as it is.
pub fn apply_preset(world: &mut World, entity: Entity, p: &ActorPreset) {
    let mut e = world.entity_mut(entity);
    e.insert(p.sheet.to_sheet());
    if let Some(mut unit) = e.get_mut::<TestbedUnit>() {
        unit.speed_mm_per_tick = p.speed_mm_per_tick;
    }
    if let Some(mut c) = e.get_mut::<Commandable>() {
        c.speed_mm_per_tick = p.speed_mm_per_tick;
    }
    if let Some(mut inv) = e.get_mut::<Inventory>() {
        inv.max_weight_g = p.carry_capacity;
    }
    let interest = e.get::<Sensors>().map_or(ALL_KINDS, |s| s.interest);
    let mut sensors = Sensors::new().interest(interest);
    for s in &p.sensors {
        sensors = match *s {
            SensorSpec::Vision {
                range_mm,
                half_fov_deg,
            } => sensors.vision(range_mm, half_fov_deg),
            SensorSpec::Hearing { radius_mm } => sensors.hearing(radius_mm),
            SensorSpec::Lidar {
                range_mm,
                fov_deg,
                beams,
                noise_ppm,
            } => sensors.lidar(range_mm, fov_deg, beams, noise_ppm),
            SensorSpec::Camera {
                range_mm,
                half_fov_deg,
            } => sensors.camera(range_mm, half_fov_deg),
            SensorSpec::Radar {
                range_mm,
                half_fov_deg,
                range_res_mm,
                bearing_res_mdeg,
            } => sensors.radar(range_mm, half_fov_deg, range_res_mm, bearing_res_mdeg),
            // The reach is its own component.
            SensorSpec::Reach { .. } => sensors,
        };
    }
    if !sensors.has_robot_sensors() {
        e.remove::<SensorReadings>();
    }
    e.insert(sensors);
    match p.reach_mm() {
        Some(radius_mm) => {
            e.insert(ReachSense { radius_mm });
        }
        None => {
            e.remove::<ReachSense>();
        }
    }
    e.insert((
        AllowedActions(p.actions.clone()),
        ActorModel(p.model.clone()),
    ));
}

/// The verb gate (`npc_agent_io::verb_gate`): an actor with
/// [`AllowedActions`] cannot use a known action outside them ("this actor
/// cannot pick_up"); other verbs (`walk`, `order`, `interact`, `wait`, ..)
/// are never refused here (`interact` and `order` ask about the verb they
/// run).
pub fn verb_gate(world: &World, entity: Entity, verb: &str) -> Option<String> {
    let allowed = world.get::<AllowedActions>(entity)?;
    (!allowed.0.contains(verb) && is_known_action(world, verb))
        .then(|| format!("this actor cannot {verb}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        for (id, _) in BUILTIN_PRESETS {
            assert!(builtin_preset(id).is_some(), "{id}");
        }
        assert_eq!(human_preset().reach_mm(), Some(1500));
    }

    #[test]
    fn unknown_sensor_and_field_are_parse_errors() {
        let mut v = serde_json::to_value(human_preset()).unwrap();
        v["sensors"] = serde_json::json!([{"type": "sonar", "range_mm": 1}]);
        assert!(serde_json::from_value::<ActorPreset>(v).is_err());
        let mut v = serde_json::to_value(human_preset()).unwrap();
        v["wings"] = serde_json::json!(2);
        assert!(serde_json::from_value::<ActorPreset>(v).is_err());
    }
}
