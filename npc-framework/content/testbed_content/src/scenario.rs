//! Scenario files (`content: "testbed"`, agent-control spec §3.3), the
//! loader registered with `npc_agent_io`, and the headless constructors.
//!
//! ```json
//! {"version":1,"content":"testbed","name":"testbed","seed":1,
//!  "world":{"layout":"compound","furnish":true,"store_locked":true,
//!           "detection":"per_roll","stagger":4,
//!           "items":[{"kind":"book","pos":[-5500,6500],"name":"target"}],
//!           "open_doors":["workshop_door"]},
//!  "actors":[{"kind":"unit","template":"carpenter","pos":[-5500,500],
//!             "owner":1,"behaviour":"wander","equip":["axe"],
//!             "carry":[{"kind":"log","stack":3}],"name":"carpenter"}],
//!  "controllers":[{"actor":0,"controller":"agent","mode":"Advisor"}]}
//! ```
//!
//! `world` (all optional): `layout` `"compound"` (default; walls and doors,
//! [`crate::layout`]), `"open"` (no walls) or `"frontier"` (the compound plus
//! groves, ore, a ruin and scattered items outside, on a 128-cell grid); `furnish` (default `true`:
//! workstations, items, trees, ore, cart, wagon); `store_locked` (default
//! `true`); `detection` `"per_roll"` (default: Perception rolls,
//! `npc_rules` Per) or `"automatic"`; `stagger` (perception scans every N
//! ticks, default 4); extra `items` with `pos`; `open_doors` by name.
//! `actors` are `kind: "unit"` with the [`UnitSpec`] fields; `pos` is
//! required. The world spawns first (so its `SimId`s do not depend on the
//! actors), then the actors in order. Names land in [`TestbedIds`].
//!
//! The scenario files in `content/testbed_content/scenarios/` are embedded
//! ([`SCENARIO_FILES`]); [`try_new_sim_app`] builds one by name.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::scenario::{
    LoadedScenario, ScenarioDoc, ScenarioLoaders, attach_controllers,
};
use npc_framework::kernel::commands::check_pos;
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::kernel::spatial::GridBounds;
use npc_framework::prelude::*;
use serde::Deserialize;

use crate::layout::{
    FRONTIER_GRID_CELLS, GRID_CELLS, ItemSpec, UnitSpec, spawn_frontier, spawn_furnishing,
    spawn_item, spawn_unit, spawn_walls,
};
use crate::world::TestbedIds;
use crate::{TestbedAgentPlugin, TestbedPlugin};

/// The loader's `content` name.
pub const CONTENT: &str = "testbed";

/// The embedded scenario files: `(name, JSON)`. `testbed` is the default
/// showcase; the others focus on one feature each (see the README).
pub const SCENARIO_FILES: [(&str, &str); 11] = [
    ("testbed", include_str!("../scenarios/testbed.json")),
    ("doors", include_str!("../scenarios/doors.json")),
    (
        "locked_store",
        include_str!("../scenarios/locked_store.json"),
    ),
    ("chop", include_str!("../scenarios/chop.json")),
    ("craft", include_str!("../scenarios/craft.json")),
    ("perception", include_str!("../scenarios/perception.json")),
    ("vehicles", include_str!("../scenarios/vehicles.json")),
    ("formation", include_str!("../scenarios/formation.json")),
    ("agent", include_str!("../scenarios/agent.json")),
    ("smithy", include_str!("../scenarios/smithy.json")),
    ("explorer", include_str!("../scenarios/explorer.json")),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Compound,
    Open,
    /// The compound plus the outside features of [`crate::layout::spawn_frontier`]
    /// on a 128-cell grid.
    Frontier,
}

impl Layout {
    /// Grid size in cells (centred on the origin).
    pub const fn grid_cells(self) -> u32 {
        match self {
            Self::Frontier => FRONTIER_GRID_CELLS,
            Self::Compound | Self::Open => GRID_CELLS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Detection {
    #[default]
    PerRoll,
    Automatic,
}

/// The `world` object of a testbed scenario (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WorldSpec {
    pub layout: Layout,
    pub furnish: bool,
    pub store_locked: bool,
    pub detection: Detection,
    pub stagger: u32,
    pub items: Vec<ItemSpec>,
    pub open_doors: Vec<String>,
}

impl Default for WorldSpec {
    fn default() -> Self {
        Self {
            layout: Layout::Compound,
            furnish: true,
            store_locked: true,
            detection: Detection::PerRoll,
            stagger: npc_framework::perception::DEFAULT_STAGGER,
            items: Vec::new(),
            open_doors: Vec::new(),
        }
    }
}

impl WorldSpec {
    pub fn parse(v: &serde_json::Value) -> Result<Self, String> {
        if v.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(v.clone()).map_err(|e| format!("world: {e}"))
    }
}

/// The testbed's plugin set: `NpcPlugins` (no driver) with every capability
/// and its glue, the per-roll detection check active unless `detection` is
/// `automatic`, `npc_nav`, [`TestbedPlugin`] and [`TestbedAgentPlugin`].
pub fn testbed_plugins(seed: u64, detection: Detection) -> NpcPlugins {
    let plugins = NpcPlugins::new()
        .seed(seed)
        .without_driver()
        .with_perception()
        .with_rules()
        .with_agent_io()
        .with_interaction()
        .with_command()
        .with_vehicles()
        .with_progression()
        .with_crafting();
    match detection {
        Detection::PerRoll => plugins.with_per_roll_detection(),
        Detection::Automatic => plugins,
    }
}

/// A fresh app (tick 0, content frozen, not finished, no driver) for
/// `world`, with `plugins` added before the freeze.
pub fn build_app(seed: u64, world: &WorldSpec, plugins: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(
        1000,
        GridBounds::centered(world.layout.grid_cells(), world.layout.grid_cells()),
    ))
    .insert_resource(PerceptionConfig {
        stagger: world.stagger.max(1),
        ..Default::default()
    });
    app.add_plugins((
        testbed_plugins(seed, world.detection),
        NavPlugin,
        TestbedPlugin,
        TestbedAgentPlugin,
    ));
    plugins(&mut app);
    crate::finish_content(app.world_mut());
    app
}

fn spawn_world(world: &mut World, spec: &WorldSpec, ids: &mut TestbedIds) -> Result<(), String> {
    if spec.layout != Layout::Open {
        spawn_walls(world, ids, spec.store_locked);
    }
    if spec.furnish {
        spawn_furnishing(world, ids);
    }
    if spec.layout == Layout::Frontier {
        spawn_frontier(world, ids);
    }
    for (i, item) in spec.items.iter().enumerate() {
        let (x, z) = item
            .pos
            .ok_or_else(|| format!("world.items[{i}]: needs pos"))?;
        let pos = check_pos("pos", (x, z)).map_err(|e| format!("world.items[{i}]: {e}"))?;
        if npc_framework::interaction::items::kind_def(world, &item.kind).is_none() {
            return Err(format!(
                "world.items[{i}]: unknown item kind `{}`",
                item.kind
            ));
        }
        let id = spawn_item(world, &item.kind, item.stack, item.quality, pos);
        if let Some(name) = &item.name {
            ids.name(name.clone(), id);
        }
    }
    for name in &spec.open_doors {
        let id = ids
            .get(name)
            .ok_or_else(|| format!("world.open_doors: no door named `{name}`"))?;
        let e = world
            .resource::<SimIndex>()
            .get(id)
            .ok_or("door left the sim")?;
        let mut door = world
            .get_mut::<Door>(e)
            .ok_or_else(|| format!("world.open_doors: `{name}` is not a door"))?;
        door.locked_by = None;
        door.open = true;
    }
    Ok(())
}

/// Loads `doc` with `seed` and `plugins` added before the freeze.
pub fn load_with(
    doc: &ScenarioDoc,
    seed: u64,
    plugins: impl FnOnce(&mut App),
) -> Result<LoadedScenario, String> {
    let spec = WorldSpec::parse(&doc.world)?;
    let mut app = build_app(seed, &spec, plugins);
    let world = app.world_mut();
    let mut ids = TestbedIds::default();
    spawn_world(world, &spec, &mut ids)?;
    let mut actors = Vec::new();
    for (i, a) in doc.actors.iter().enumerate() {
        if a.kind != "unit" {
            return Err(format!(
                "actors[{i}]: unknown kind `{}` (testbed has: unit)",
                a.kind
            ));
        }
        let (x, z) = a.pos.ok_or_else(|| format!("actors[{i}]: needs pos"))?;
        let pos = check_pos("pos", (x, z)).map_err(|e| format!("actors[{i}]: {e}"))?;
        let unit: UnitSpec = serde_json::from_value(serde_json::Value::Object(a.extra.clone()))
            .map_err(|e| format!("actors[{i}]: {e}"))?;
        if unit.template.is_empty() {
            return Err(format!("actors[{i}]: needs a template"));
        }
        let id = spawn_unit(world, pos, &unit).map_err(|e| format!("actors[{i}]: {e}"))?;
        if let Some(name) = &unit.name {
            ids.name(name.clone(), id);
        }
        actors.push(id);
    }
    ids.units = actors.clone();
    world.insert_resource(ids);
    world.insert_resource(ScenarioInfo {
        name: doc.info_name(),
        villagers: u32::try_from(actors.len()).unwrap_or(u32::MAX),
    });
    Ok(LoadedScenario { app, actors })
}

/// The testbed's scenario loader (`npc_agent_io::scenario::ScenarioLoaderFn`).
pub fn load_scenario(doc: &ScenarioDoc, seed: u64) -> Result<LoadedScenario, String> {
    load_with(doc, seed, |_| {})
}

/// Adds the testbed loader (`content: "testbed"`) to `loaders`.
pub fn register_scenario_loader(loaders: &mut ScenarioLoaders) {
    loaders.add(CONTENT, load_scenario);
}

/// Every embedded scenario as `(name, one-line description)`, in
/// [`SCENARIO_FILES`] order (the README's scenario table, shortened; the
/// game menu lists them).
pub fn embedded_scenarios() -> &'static [(&'static str, &'static str)] {
    const LIST: [(&str, &str); SCENARIO_FILES.len()] = [
        (
            "testbed",
            "The showcase: 8 actors in two factions, a woodcutter, Per-roll detection.",
        ),
        (
            "doors",
            "An observer facing a book behind the closed workshop door.",
        ),
        (
            "locked_store",
            "A keeper near the store key and a warden at the locked store door.",
        ),
        (
            "chop",
            "A lumberjack with an axe and a bare-handed novice next to a tree.",
        ),
        (
            "craft",
            "A master carpenter and a novice at the workbench, 8 logs each.",
        ),
        (
            "perception",
            "Open field: a sharp-eyed scout and a dull minion, each 9 m from a book.",
        ),
        (
            "vehicles",
            "A driver and a passenger next to the cart; the wagon outside the gate.",
        ),
        (
            "formation",
            "Four faction-1 units in the yard and one enemy outside.",
        ),
        (
            "agent",
            "The doors scenario with an Advisor agent controller on the observer.",
        ),
        (
            "smithy",
            "The crafting chain: key, store, pickaxe, ore, forge, a smith and a joiner.",
        ),
        (
            "explorer",
            "The frontier map for an AI agent: the compound, groves, ore and a ruin.",
        ),
    ];
    &LIST
}

/// The embedded scenario file `name` (see [`SCENARIO_FILES`]).
pub fn scenario_text(name: &str) -> Option<&'static str> {
    SCENARIO_FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
}

/// The embedded scenario `name`, parsed.
pub fn scenario_doc(name: &str) -> Result<ScenarioDoc, String> {
    let text = scenario_text(name).ok_or_else(|| {
        format!(
            "unknown testbed scenario `{name}` (expected one of {:?})",
            SCENARIO_FILES.map(|(n, _)| n)
        )
    })?;
    ScenarioDoc::parse(text)
}

/// The headless constructor (tests, tools, replays): the embedded scenario
/// `scenario` built for `seed` with its controllers pre-attached, content
/// frozen, the app not finished, no driver; drive it with
/// `run_ticks(app.world_mut(), n)`. `ScenarioInfo.name` is `scenario`, so
/// the signature matches the replay `AppFactory` (`_units` is ignored: the
/// scenario files fix the actors).
pub fn try_new_sim_app(seed: u64, scenario: &str, _units: u32) -> Result<App, String> {
    try_new_sim_app_with(seed, scenario, |_| {})
}

/// [`try_new_sim_app`] with `plugins` added before the freeze.
pub fn try_new_sim_app_with(
    seed: u64,
    scenario: &str,
    plugins: impl FnOnce(&mut App),
) -> Result<App, String> {
    let doc = scenario_doc(scenario)?;
    let mut loaded = load_with(&doc, seed, plugins)?;
    attach_controllers(&doc, &mut loaded)?;
    loaded.app.insert_resource(ScenarioInfo {
        name: scenario.to_owned(),
        villagers: u32::try_from(loaded.actors.len()).unwrap_or(u32::MAX),
    });
    Ok(loaded.app)
}

/// [`try_new_sim_app`], panicking on an unknown scenario.
pub fn new_sim_app(seed: u64, scenario: &str) -> App {
    try_new_sim_app(seed, scenario, 0).unwrap_or_else(|e| panic!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_scenario_has_a_description() {
        let listed: Vec<&str> = embedded_scenarios().iter().map(|(n, _)| *n).collect();
        let files: Vec<&str> = SCENARIO_FILES.iter().map(|(n, _)| *n).collect();
        assert_eq!(listed, files, "same names, same order as SCENARIO_FILES");
        for (name, desc) in embedded_scenarios() {
            assert!(!desc.trim().is_empty(), "`{name}` has no description");
            assert!(!desc.contains('\n'), "`{name}`: one line");
        }
        assert!(
            listed.contains(&"explorer"),
            "Ruling R4: explorer is listed"
        );
    }
}
