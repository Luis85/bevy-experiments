//! Replays (spec §3.3): seed + scenario + applied commands + state-hash
//! checkpoints, plus the content hash of the registered content. Not
//! snapshots: a replay is re-run from tick 0.
//!
//! The kernel knows no scenario: building the app for a replay is the job of
//! an [`AppFactory`] supplied by content (e.g. `village_content::try_new_sim_app`).
//!
//! - [`Recorder`] is the one replay recorder: checkpoints aligned to
//!   multiples of an interval (headless) or by elapsed ticks (live, BRP),
//!   then the header and the [`CommandLog`]. `npc_tools::recorder`
//!   re-exports it for `npcctl` and the BRP `npc/record/*` methods.
//! - [`record`] runs an app built by a factory with an aligned [`Recorder`].
//! - [`verify`] re-runs a replay and compares the content hash, then every
//!   checkpoint.
//! - [`diverge`] runs two replays side by side and reports the first tick
//!   whose state hashes differ, with a per-entity, per-component diff.
//!
//! JSON: `{"version":2,"content_hash":123..,"seed":1,"scenario":"village",
//! "villagers":1,"ticks":3000,"commands":[[100,0,{"type":"SetFood",..}]],
//! "checkpoints":[[500,123..]]}`.
//!
//! Versions: 2 (current) adds `content_hash` and hashes registered
//! components by name. Version 1 files still load (header and commands are
//! unchanged), but their checkpoints use the pre-registry hash encoding, so
//! [`verify`] reports them as needing a re-record instead of a mismatch.

use std::collections::BTreeSet;

use bevy_app::App;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::clock::SimClock;
use crate::commands::{CommandLog, CommandQueue, SimCommand};
use crate::hash::{components_json, entity_hashes, state_hash};
use crate::ids::{SimId, SimIndex};
use crate::registry::content_hash;
use crate::rng::WorldSeed;
use crate::schedule::run_ticks;

/// Current replay format version.
pub const REPLAY_VERSION: u32 = 2;
/// The first format (no content hash, pre-registry hash encoding).
pub const REPLAY_VERSION_V1: u32 = 1;

/// Builds a fresh app at tick 0 for `(seed, scenario, villagers)`; content
/// provides it. An unknown scenario is an `Err` with a readable message.
pub type AppFactory = fn(u64, &str, u32) -> Result<App, String>;

/// Which scenario an app was built from (read by replays). Inserted by the
/// content's app factory.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioInfo {
    /// The scenario name the factory understands (e.g. `"village"`).
    pub name: String,
    /// Unit-count parameter (1 when the scenario has a fixed size).
    pub villagers: u32,
}

fn one() -> u32 {
    1
}

/// A recorded run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replay {
    pub version: u32,
    /// Hash over every registered content name and tree (absent in v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<u64>,
    pub seed: u64,
    /// Scenario name for the app factory.
    pub scenario: String,
    /// Unit count for the factory. Defaults to 1 when absent from the JSON.
    #[serde(default = "one")]
    pub villagers: u32,
    /// Tick the run ends at (ticks completed from tick 0).
    pub ticks: u64,
    /// Applied commands as `(tick, seq, cmd)`; re-applied in `(tick, seq)`
    /// order whatever the order of this list.
    pub commands: Vec<(u64, u32, SimCommand)>,
    /// `(tick, state_hash)` after `tick` ticks completed, ascending.
    pub checkpoints: Vec<(u64, u64)>,
}

impl Replay {
    /// A replay without commands or checkpoints, e.g. to [`diverge`] two seeds.
    pub fn new(seed: u64, scenario: &str, villagers: u32, ticks: u64) -> Self {
        Self {
            version: REPLAY_VERSION,
            content_hash: None,
            seed,
            scenario: scenario.to_owned(),
            villagers,
            ticks,
            commands: Vec::new(),
            checkpoints: Vec::new(),
        }
    }
}

/// Runs `app` for `ticks` more ticks and returns its replay. A checkpoint is
/// taken at every tick that is a multiple of `every` (`every == 0`: none
/// besides the last) and at the final tick. Commands are everything in the
/// app's [`CommandLog`] (applied since tick 0); commands still pending after
/// the run are not part of the replay.
///
/// The app must come from an [`AppFactory`] and be changed only through
/// commands, or the replay will not verify.
///
/// # Panics
/// When the app has no [`ScenarioInfo`] (not built by an app factory).
pub fn record(app: &mut App, ticks: u64, every: u64) -> Replay {
    let world = app.world_mut();
    assert!(
        world.contains_resource::<ScenarioInfo>(),
        "replay::record needs an app built by an app factory (no ScenarioInfo)"
    );
    let start = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let end = start + ticks;
    let mut recorder = Recorder::aligned(every, end);
    let mut t = start;
    while t < end {
        // Run straight to the next checkpoint tick (the recorder would
        // ignore the ticks in between).
        let next = match every {
            0 => end,
            e => (t / e + 1).saturating_mul(e).min(end),
        };
        run_ticks(world, next - t);
        t = next;
        recorder.after_tick(world);
    }
    recorder.finish(world).expect("ScenarioInfo checked above")
}

/// When a [`Recorder`] takes checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    /// After every tick that is a multiple of `every` (`every == 0`: none)
    /// and after tick `end`. Headless runs that step tick by tick: [`record`],
    /// `npcctl run --record` / `replay record`.
    Aligned { end: u64 },
    /// Whenever at least `every` ticks passed since the previous checkpoint
    /// (a live game does not land on exact multiples), plus one when started
    /// and one when finished. The BRP `npc/record/*` methods and
    /// `village --record`.
    Live,
}

/// The one replay recorder (headless and live): checkpoints as `(tick,
/// state_hash)` by its [`Cadence`]; [`Recorder::finish`] adds the header
/// (seed, [`ScenarioInfo`], content hash) and every command in the app's
/// [`CommandLog`] (applied since tick 0). `npc_tools::recorder` re-exports
/// it for the CLI and BRP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorder {
    every: u64,
    cadence: Cadence,
    started_at: u64,
    /// `(tick, state_hash)`, ascending.
    pub checkpoints: Vec<(u64, u64)>,
}

impl Recorder {
    /// [`Cadence::Aligned`]: checkpoints at multiples of `every` and at `end`.
    /// Call [`Recorder::after_tick`] after each tick.
    pub fn aligned(every: u64, end: u64) -> Self {
        Self {
            every,
            cadence: Cadence::Aligned { end },
            started_at: 0,
            checkpoints: Vec::new(),
        }
    }

    /// [`Cadence::Live`] from the world's current tick, with its first
    /// checkpoint taken now. `every` is at least 1.
    pub fn live(world: &mut World, every: u64) -> Self {
        let now = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
        Self {
            every: every.max(1),
            cadence: Cadence::Live,
            started_at: now,
            checkpoints: vec![(now, state_hash(world))],
        }
    }

    /// Checkpoint interval in ticks.
    pub fn every(&self) -> u64 {
        self.every
    }

    /// Tick a live recording started at (0 for an aligned one).
    pub fn started_at(&self) -> u64 {
        self.started_at
    }

    pub fn cadence(&self) -> Cadence {
        self.cadence
    }

    /// Whether a checkpoint is due at `tick` (ticks completed).
    pub fn due(&self, tick: u64) -> bool {
        match self.cadence {
            Cadence::Aligned { end } => {
                tick == end || (self.every > 0 && tick.is_multiple_of(self.every))
            }
            Cadence::Live => self
                .checkpoints
                .last()
                .is_none_or(|&(t, _)| tick >= t + self.every),
        }
    }

    /// Takes a checkpoint if one is due at the world's current tick. Call
    /// after each tick (headless) or once per frame (live).
    pub fn after_tick(&mut self, world: &mut World) {
        let Some(tick) = world.get_resource::<SimClock>().map(|c| c.tick) else {
            return;
        };
        if self.due(tick) && self.checkpoints.last().is_none_or(|&(t, _)| t != tick) {
            self.checkpoints.push((tick, state_hash(world)));
        }
    }

    /// The replay of the run from tick 0 to now. A live recording also takes
    /// a final checkpoint now (when it has none at this tick). `Err` when the
    /// app has no [`ScenarioInfo`] (not built by an app factory).
    pub fn finish(mut self, world: &mut World) -> Result<Replay, String> {
        let now = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
        if self.cadence == Cadence::Live && self.checkpoints.last().is_none_or(|&(t, _)| t != now) {
            self.checkpoints.push((now, state_hash(world)));
        }
        let info = world
            .get_resource::<ScenarioInfo>()
            .cloned()
            .ok_or_else(|| {
                "no ScenarioInfo (the app must be built by the content's app factory for replays)"
                    .to_owned()
            })?;
        Ok(Replay {
            version: REPLAY_VERSION,
            content_hash: Some(content_hash(world)),
            seed: world.get_resource::<WorldSeed>().map_or(0, |s| s.0),
            scenario: info.name,
            villagers: info.villagers,
            ticks: now,
            commands: world
                .get_resource::<CommandLog>()
                .map(|log| log.entries.clone())
                .unwrap_or_default(),
            checkpoints: self.checkpoints,
        })
    }
}

/// A fresh app at tick 0 for `replay`, with its commands queued.
pub fn replay_app(replay: &Replay, factory: AppFactory) -> Result<App, String> {
    let mut app = factory(replay.seed, &replay.scenario, replay.villagers)?;
    let mut queue = app.world_mut().get_resource_or_init::<CommandQueue>();
    for (tick, seq, cmd) in &replay.commands {
        queue.pending.insert((*tick, *seq), cmd.clone());
    }
    Ok(app)
}

/// Result of [`verify`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Every checkpoint matched (and the replay could be run at all).
    pub ok: bool,
    /// Checkpoints compared, including a mismatching one.
    pub checked: usize,
    /// `(tick, expected_hash, actual_hash)` of the first mismatch.
    pub first_mismatch: Option<(u64, u64, u64)>,
    /// Why the replay could not be run (unknown version or scenario).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Re-runs `replay` from tick 0 and compares every checkpoint (in tick
/// order), stopping at the first mismatch. Fails without running when the
/// replay's content hash differs from the app's (different registered
/// content), or for a version 1 file (old hash encoding).
pub fn verify(replay: &Replay, factory: AppFactory) -> VerifyReport {
    let failed = |error: String| VerifyReport {
        ok: false,
        checked: 0,
        first_mismatch: None,
        error: Some(error),
    };
    if replay.version == REPLAY_VERSION_V1 {
        return failed(format!(
            "replay version 1 uses the pre-registry state-hash encoding; its commands still \
             load, but re-record it (npcctl replay record --commands <file>) to get version \
             {REPLAY_VERSION} checkpoints"
        ));
    }
    if replay.version != REPLAY_VERSION {
        return failed(format!(
            "replay version {} is not supported (expected {REPLAY_VERSION})",
            replay.version
        ));
    }
    let mut app = match replay_app(replay, factory) {
        Ok(app) => app,
        Err(e) => return failed(e),
    };
    let world = app.world_mut();
    let actual_content = content_hash(world);
    match replay.content_hash {
        None => return failed("replay has no content_hash".to_owned()),
        Some(expected) if expected != actual_content => {
            return failed(format!(
                "content hash mismatch: replay {expected:016x}, this build {actual_content:016x} \
                 (registered actions, conditions, trees, needs, components or commands differ)"
            ));
        }
        Some(_) => {}
    }
    let mut checkpoints = replay.checkpoints.clone();
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

/// First point where two runs differ.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    /// The first tick count (ticks completed) at which the state hashes
    /// differ: the runs agree after `tick - 1` ticks and differ after `tick`
    /// (0 when they differ from the start).
    pub tick: u64,
    /// Entities whose per-entity hashes differ, ascending `SimId`.
    pub entities: Vec<EntityDiff>,
}

/// How one entity differs at the divergence tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityDiff {
    pub sim_id: SimId,
    /// Names (as in [`components_json`]) of the components that differ.
    pub components: Vec<String>,
    /// Run `a`'s values of just those components (`null` when the entity
    /// does not exist in `a`).
    pub a: Value,
    /// Run `b`'s values, likewise.
    pub b: Value,
}

/// Runs `a` and `b` side by side for `min(a.ticks, b.ticks)` ticks,
/// comparing state hashes after every tick (and before the first).
/// `None` when they never differ.
///
/// # Panics
/// When either replay cannot be run (unknown scenario); see [`try_diverge`].
pub fn diverge(a: &Replay, b: &Replay, factory: AppFactory) -> Option<Divergence> {
    try_diverge(a, b, factory).unwrap_or_else(|e| panic!("{e}"))
}

/// [`diverge`] returning an error for a replay that cannot be run.
pub fn try_diverge(
    a: &Replay,
    b: &Replay,
    factory: AppFactory,
) -> Result<Option<Divergence>, String> {
    let mut app_a = replay_app(a, factory)?;
    let mut app_b = replay_app(b, factory)?;
    let (wa, wb) = (app_a.world_mut(), app_b.world_mut());
    let end = a.ticks.min(b.ticks);
    let mut tick = 0;
    loop {
        if state_hash(wa) != state_hash(wb) {
            return Ok(Some(Divergence {
                tick,
                entities: entity_diffs(wa, wb),
            }));
        }
        if tick >= end {
            return Ok(None);
        }
        run_ticks(wa, 1);
        run_ticks(wb, 1);
        tick += 1;
    }
}

/// Per-entity, per-component differences between two worlds.
pub fn entity_diffs(wa: &mut World, wb: &mut World) -> Vec<EntityDiff> {
    let (ha, hb) = (entity_hashes(wa), entity_hashes(wb));
    let ids: BTreeSet<SimId> = ha.keys().chain(hb.keys()).copied().collect();
    ids.into_iter()
        .filter(|id| ha.get(id) != hb.get(id))
        .map(|id| {
            let ca = components_of(wa, id);
            let cb = components_of(wb, id);
            let empty = Map::new();
            let (ma, mb) = (ca.as_ref().unwrap_or(&empty), cb.as_ref().unwrap_or(&empty));
            let keys: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
            let components: Vec<String> = keys
                .into_iter()
                .filter(|k| ma.get(*k) != mb.get(*k))
                .cloned()
                .collect();
            let pick = |m: &Option<Map<String, Value>>| match m {
                None => Value::Null,
                Some(m) => Value::Object(
                    components
                        .iter()
                        .filter_map(|k| m.get(k).map(|v| (k.clone(), v.clone())))
                        .collect(),
                ),
            };
            EntityDiff {
                sim_id: id,
                a: pick(&ca),
                b: pick(&cb),
                components,
            }
        })
        .collect()
}

fn components_of(world: &World, id: SimId) -> Option<Map<String, Value>> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    Some(components_json(world, entity))
}

#[cfg(test)]
mod recorder_tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(crate::NpcKernelPlugin::default())
            .insert_resource(ScenarioInfo {
                name: "test".into(),
                villagers: 1,
            });
        crate::ensure_frozen(app.world_mut());
        app
    }

    fn ticks_of(r: &Recorder) -> Vec<u64> {
        r.checkpoints.iter().map(|c| c.0).collect()
    }

    #[test]
    fn aligned_checkpoints_at_multiples_and_end() {
        let mut app = app();
        let world = app.world_mut();
        let mut r = Recorder::aligned(5, 12);
        for _ in 0..12 {
            run_ticks(world, 1);
            r.after_tick(world);
        }
        assert_eq!(ticks_of(&r), [5, 10, 12]);
        let replay = r.finish(world).unwrap();
        assert_eq!((replay.ticks, replay.scenario.as_str()), (12, "test"));
        assert_eq!(replay.checkpoints.len(), 3);
        // `record` gives the same replay from a fresh app.
        let mut again = self::app();
        assert_eq!(record(&mut again, 12, 5), replay);
        // every == 0: only the end; no ticks: nothing.
        let mut r0 = Recorder::aligned(0, 15);
        let world = again.world_mut();
        for _ in 0..3 {
            run_ticks(world, 1);
            r0.after_tick(world);
        }
        assert_eq!(ticks_of(&r0), [15]);
        assert!(ticks_of(&Recorder::aligned(1, 0)).is_empty());
    }

    #[test]
    fn live_checkpoints_by_interval_with_start_and_stop() {
        let mut app = app();
        let world = app.world_mut();
        run_ticks(world, 2);
        let mut r = Recorder::live(world, 3);
        assert_eq!((r.started_at(), r.every()), (2, 3));
        // A live frame may run several ticks or none.
        for step in [1, 0, 2, 1, 4, 1] {
            run_ticks(world, step);
            r.after_tick(world);
        }
        assert_eq!(ticks_of(&r), [2, 5, 10]);
        // Finishing adds the stop checkpoint.
        let replay = r.finish(world).unwrap();
        assert_eq!(replay.ticks, 11);
        assert_eq!(
            replay.checkpoints.iter().map(|c| c.0).collect::<Vec<_>>(),
            [2, 5, 10, 11]
        );
        let mut r = Recorder::live(world, 0);
        assert_eq!(r.every(), 1);
        run_ticks(world, 1);
        let replay = r.clone().finish(world).unwrap();
        assert_eq!(
            replay.checkpoints.iter().map(|c| c.0).collect::<Vec<_>>(),
            [11, 12]
        );
        r.after_tick(world);
        assert_eq!(ticks_of(&r), [11, 12]);
    }

    #[test]
    fn finish_needs_scenario_info() {
        let mut app = App::new();
        app.add_plugins(crate::NpcKernelPlugin::default());
        assert!(Recorder::aligned(1, 1).finish(app.world_mut()).is_err());
    }
}
