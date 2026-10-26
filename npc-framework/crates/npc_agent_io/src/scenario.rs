//! Scenario files and session records (spec §3.3).
//!
//! A scenario is JSON with a schema `version` (currently
//! [`SCENARIO_VERSION`]):
//!
//! ```json
//! {"version":1,"content":"village","name":"agent_demo","seed":1,
//!  "world":{"layout":"village"},
//!  "actors":[{"kind":"villager","pos":[2000,0],"needs":{"Hunger":30000}}],
//!  "controllers":[{"actor":0,"controller":"agent","mode":"Direct","on_idle":"Idle"}]}
//! ```
//!
//! `content` picks a loader from a [`ScenarioLoaders`] registry the host
//! assembles (each content crate provides its loader, e.g.
//! `village_content::agent::register_scenario_loader`). The loader
//! interprets `world` and `actors` and returns a fresh app at tick 0 with
//! `AgentIoPlugin` installed; [`load`] then pre-attaches `controllers`
//! (by `unit` id or by index into `actors`) directly, before tick 0, so a
//! replay rebuilt from the same scenario starts identically.
//!
//! A [`SessionRecord`] (`npcctl serve --record`) is the scenario, the seed
//! and the session's replay (every command, agent actions included, plus
//! state-hash checkpoints); [`verify_session`] re-runs it without any agent.

use std::collections::BTreeMap;

use bevy_app::App;
use npc_kernel::commands::{CommandQueue, ContentCommand};
use npc_kernel::hash::state_hash;
use npc_kernel::registry::content_hash;
use npc_kernel::replay::{REPLAY_VERSION, Replay, VerifyReport};
use npc_kernel::{SimId, SimIndex, run_ticks};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::control::{AttachController, ControlMode, IdlePolicy, attach_now};

/// Current scenario schema version.
pub const SCENARIO_VERSION: u32 = 1;
/// Current session record version.
pub const SESSION_VERSION: u32 = 1;

/// A scenario file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDoc {
    pub version: u32,
    /// Which loader builds it (`"village"`).
    pub content: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Default seed (a session's `--seed` / `world/reset {seed}` overrides it).
    #[serde(default)]
    pub seed: Option<u64>,
    /// Content-specific world layout.
    #[serde(default)]
    pub world: Value,
    /// Extra actors, content-specific beyond `kind` and `pos`.
    #[serde(default)]
    pub actors: Vec<ActorSpec>,
    /// Controllers to pre-attach.
    #[serde(default)]
    pub controllers: Vec<ControllerSpec>,
}

/// One actor of a scenario: `{"kind": "villager", "pos": [x, z], ..}`; the
/// other fields (`home`, `needs`, ..) are for the loader, in `extra`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Value", into = "Value")]
pub struct ActorSpec {
    pub kind: String,
    pub pos: Option<(i32, i32)>,
    pub extra: serde_json::Map<String, Value>,
}

impl TryFrom<Value> for ActorSpec {
    type Error = String;
    fn try_from(v: Value) -> Result<Self, String> {
        let Value::Object(mut extra) = v else {
            return Err("an actor must be an object".into());
        };
        let kind = match extra.remove("kind") {
            Some(Value::String(k)) => k,
            _ => return Err("an actor needs a string `kind`".into()),
        };
        let pos = match extra.remove("pos") {
            None | Some(Value::Null) => None,
            Some(p) => Some(
                serde_json::from_value::<(i32, i32)>(p)
                    .map_err(|e| format!("actor `pos` must be [x, z] in mm: {e}"))?,
            ),
        };
        Ok(Self { kind, pos, extra })
    }
}

impl From<ActorSpec> for Value {
    fn from(a: ActorSpec) -> Value {
        let mut m = a.extra;
        m.insert("kind".into(), Value::String(a.kind));
        if let Some((x, z)) = a.pos {
            m.insert("pos".into(), serde_json::json!([x, z]));
        }
        Value::Object(m)
    }
}

/// A controller to pre-attach: to `unit` (a SimId) or to `actor` (an index
/// into `actors`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerSpec {
    #[serde(default)]
    pub unit: Option<u32>,
    #[serde(default)]
    pub actor: Option<usize>,
    pub controller: String,
    #[serde(default)]
    pub mode: ControlMode,
    #[serde(default)]
    pub on_idle: IdlePolicy,
}

impl ScenarioDoc {
    /// Parses and checks the version.
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: Self = serde_json::from_str(text).map_err(|e| format!("bad scenario: {e}"))?;
        doc.check()?;
        Ok(doc)
    }

    /// From a JSON value (e.g. `world/load_scenario {scenario}`).
    pub fn from_value(v: Value) -> Result<Self, String> {
        let doc: Self = serde_json::from_value(v).map_err(|e| format!("bad scenario: {e}"))?;
        doc.check()?;
        Ok(doc)
    }

    fn check(&self) -> Result<(), String> {
        if self.version != SCENARIO_VERSION {
            return Err(format!(
                "scenario version {} is not supported (expected {SCENARIO_VERSION})",
                self.version
            ));
        }
        Ok(())
    }

    /// The name replays record (`ScenarioInfo.name`): `scenario:<name>`.
    pub fn info_name(&self) -> String {
        format!(
            "scenario:{}",
            self.name.as_deref().unwrap_or(self.content.as_str())
        )
    }
}

/// A freshly loaded scenario.
pub struct LoadedScenario {
    /// At tick 0, content frozen, not finished, without a driver.
    pub app: App,
    /// The SimId of each entry of `actors`.
    pub actors: Vec<SimId>,
}

/// Builds a scenario's app for `seed`. It must add `AgentIoPlugin`, freeze
/// content, spawn the world and actors and insert `ScenarioInfo`.
pub type ScenarioLoaderFn = fn(&ScenarioDoc, u64) -> Result<LoadedScenario, String>;

/// Scenario loaders by `content` name (assembled by the host).
#[derive(Clone, Default)]
pub struct ScenarioLoaders {
    pub loaders: BTreeMap<String, ScenarioLoaderFn>,
}

impl ScenarioLoaders {
    /// Adds the loader of `content`.
    ///
    /// # Panics
    /// On a duplicate name.
    pub fn add(&mut self, content: &str, f: ScenarioLoaderFn) -> &mut Self {
        assert!(
            self.loaders.insert(content.to_owned(), f).is_none(),
            "scenario loader `{content}` registered twice"
        );
        self
    }

    pub fn names(&self) -> Vec<String> {
        self.loaders.keys().cloned().collect()
    }
}

/// Loads `doc` with `seed` (default: the doc's, else 1) and pre-attaches its
/// controllers.
pub fn load(
    loaders: &ScenarioLoaders,
    doc: &ScenarioDoc,
    seed: Option<u64>,
) -> Result<LoadedScenario, String> {
    let f = loaders.loaders.get(&doc.content).ok_or_else(|| {
        format!(
            "no scenario loader for content `{}` (known: {:?})",
            doc.content,
            loaders.names()
        )
    })?;
    let seed = seed.or(doc.seed).unwrap_or(1);
    let mut loaded = f(doc, seed)?;
    attach_controllers(doc, &mut loaded)?;
    Ok(loaded)
}

/// Pre-attaches `doc.controllers` to `loaded` (by `unit` id or by index into
/// `actors`) directly, before tick 0, so a replay rebuilt from the same
/// scenario starts identically. [`load`] calls it; hosts that build the app
/// themselves (a windowed testbed with its own plugins) call it after the
/// content loader.
pub fn attach_controllers(doc: &ScenarioDoc, loaded: &mut LoadedScenario) -> Result<(), String> {
    for (i, spec) in doc.controllers.iter().enumerate() {
        let unit = match (spec.unit, spec.actor) {
            (Some(u), None) => SimId(u),
            (None, Some(a)) => *loaded
                .actors
                .get(a)
                .ok_or_else(|| format!("controllers[{i}]: actor {a} does not exist"))?,
            _ => return Err(format!("controllers[{i}]: give exactly one of unit, actor")),
        };
        let world = loaded.app.world_mut();
        let cmd = AttachController {
            unit: unit.0,
            controller: spec.controller.clone(),
            mode: spec.mode,
            on_idle: spec.on_idle,
            force: false,
        };
        cmd.validate(world)
            .map_err(|e| format!("controllers[{i}]: {e}"))?;
        let entity = world
            .resource::<SimIndex>()
            .get(unit)
            .ok_or_else(|| format!("controllers[{i}]: unknown unit {}", unit.0))?;
        attach_now(world, entity, &spec.controller, spec.mode, spec.on_idle);
    }
    Ok(())
}

/// An agent session: the scenario, the seed and the replay.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub version: u32,
    pub scenario: ScenarioDoc,
    pub seed: u64,
    pub replay: Replay,
}

/// Re-runs a session from its scenario and commands, without any agent, and
/// compares the content hash and every checkpoint.
pub fn verify_session(loaders: &ScenarioLoaders, record: &SessionRecord) -> VerifyReport {
    let failed = |error: String| VerifyReport {
        ok: false,
        checked: 0,
        first_mismatch: None,
        error: Some(error),
    };
    if record.version != SESSION_VERSION || record.replay.version != REPLAY_VERSION {
        return failed(format!(
            "unsupported session record version {} / replay version {}",
            record.version, record.replay.version
        ));
    }
    let mut loaded = match load(loaders, &record.scenario, Some(record.seed)) {
        Ok(l) => l,
        Err(e) => return failed(e),
    };
    let world = loaded.app.world_mut();
    let actual = content_hash(world);
    if record.replay.content_hash != Some(actual) {
        return failed(format!(
            "content hash mismatch: record {:?}, this build {actual:016x}",
            record.replay.content_hash
        ));
    }
    {
        let mut queue = world.get_resource_or_init::<CommandQueue>();
        for (tick, seq, cmd) in &record.replay.commands {
            queue.pending.insert((*tick, *seq), cmd.clone());
        }
    }
    let mut checkpoints = record.replay.checkpoints.clone();
    checkpoints.sort_by_key(|c| c.0);
    let mut done = 0;
    let mut checked = 0;
    for (tick, expected) in checkpoints {
        run_ticks(world, tick.saturating_sub(done));
        done = done.max(tick);
        checked += 1;
        let actual = state_hash(world);
        if actual != expected {
            return VerifyReport {
                ok: false,
                checked,
                first_mismatch: Some((tick, expected, actual)),
                error: None,
            };
        }
    }
    VerifyReport {
        ok: true,
        checked,
        first_mismatch: None,
        error: None,
    }
}
