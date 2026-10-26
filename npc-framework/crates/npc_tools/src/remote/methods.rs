//! The `npc/*` method handlers. Every handler is an exclusive system
//! `fn(In<Option<Value>>, &mut World) -> BrpResult` run in `RemoteLast`.
//! They validate their params and return a [`BrpError`] for anything wrong;
//! they never panic on bad input.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::remote::{BrpError, BrpResult, error_codes};
use npc_kernel::commands::{CommandError, SimCommand, submit};
use npc_kernel::hash::state_hash;
use npc_kernel::inspect;
use npc_kernel::journal::{Journal, JournalFilter, query};
use npc_kernel::replay::{Replay, ScenarioInfo};
use npc_kernel::{NpcSimTick, SimClock, SimControl, SimId};
use npc_perf::PerfHandle;

use crate::HeadlessDriver;
use crate::out_path::{self, OutputRoot};
use crate::recorder::Recorder;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most ticks one `npc/step` may run.
pub const MAX_STEP: u64 = 1_000_000;
/// Highest `npc/speed` multiplier.
pub const MAX_SPEED: u32 = 1_000;
/// Default checkpoint interval of `npc/record/start`, in ticks.
pub const DEFAULT_RECORD_EVERY: u64 = 100;

/// Every `npc/*` method this crate registers (`npc/screenshot` is added by
/// the game, see `village`).
pub const METHODS: [&str; 13] = [
    "npc/pause",
    "npc/resume",
    "npc/step",
    "npc/speed",
    "npc/state",
    "npc/unit",
    "npc/command",
    "npc/journal",
    "npc/perf",
    "npc/perf/reset",
    "npc/record/start",
    "npc/record/stop",
    "npc/methods",
];

/// Result of the control methods (`npc/pause`, `npc/resume`, `npc/speed`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlState {
    /// Ticks completed (the next tick to run).
    pub tick: u64,
    pub paused: bool,
    pub multiplier: u32,
    /// `"headless"` (frame-stepped [`HeadlessDriverPlugin`](crate::HeadlessDriverPlugin))
    /// or `"view"` (any other driver, e.g. a game's real-time one).
    pub driver: String,
}

/// Result of `npc/step`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepResult {
    /// Ticks completed after the step.
    pub tick: u64,
    /// Ticks run by this call.
    pub ticks: u64,
    /// `state_hash` after the step (a JSON number; may exceed 2^53).
    pub hash: u64,
    /// The same hash as 16 hex digits, as the F1 overlay shows it.
    pub hash_hex: String,
}

/// Result of `npc/command`: where the command was queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAccepted {
    /// The tick it applies at (start of that tick).
    pub tick: u64,
    /// Its position among the commands of that tick.
    pub seq: u32,
}

/// Result of `npc/record/start`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordStatus {
    pub recording: bool,
    /// Tick recording started at (its first checkpoint).
    pub started_at: u64,
    /// Checkpoint interval in ticks.
    pub every: u64,
}

/// A replay being recorded live, by the shared [`Recorder`] with the live
/// cadence: checkpoints are taken in `Last` whenever at least `every` ticks
/// passed since the previous one (live ticks do not land on exact
/// multiples), and at start and stop. Commands come from the `CommandLog` at
/// stop, so they cover the whole run from tick 0.
#[derive(Resource, Clone, Debug, Default)]
pub struct Recording {
    pub active: Option<ActiveRecording>,
}

/// The live recording: the shared recorder (`crate::recorder`).
pub type ActiveRecording = Recorder;

/// Write the recording to this path when the app exits (`village --record`).
#[derive(Resource, Clone, Debug)]
pub struct RecordOnExit(pub PathBuf);

// ---------------------------------------------------------------- errors

fn invalid_params(message: impl Into<String>) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message: message.into(),
        data: None,
    }
}

fn not_ready(what: &str) -> BrpError {
    BrpError {
        code: error_codes::RESOURCE_NOT_PRESENT,
        message: format!("sim not ready: missing {what}"),
        data: None,
    }
}

/// A rejected command: `UnknownUnit` → entity-not-found, `NotReady` →
/// resource-not-present, the rest → invalid-params. `data` is the
/// [`CommandError`] JSON, e.g. `{"error":"UnknownUnit","unit":999}`.
pub fn command_error(err: &CommandError) -> BrpError {
    let code = match err {
        CommandError::UnknownUnit { .. } => error_codes::ENTITY_NOT_FOUND,
        CommandError::NotReady { .. } => error_codes::RESOURCE_NOT_PRESENT,
        _ => error_codes::INVALID_PARAMS,
    };
    BrpError {
        code,
        message: err.to_string(),
        data: serde_json::to_value(err).ok(),
    }
}

fn to_value<T: Serialize>(value: &T) -> BrpResult {
    serde_json::to_value(value).map_err(BrpError::internal)
}

/// Params as `T`; absent params mean `T::default()`.
fn params_or_default<T: DeserializeOwned + Default>(params: Option<Value>) -> BrpResult<T> {
    match params {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => {
            serde_json::from_value(v).map_err(|e| invalid_params(format!("bad params: {e}")))
        }
    }
}

/// Required params as `T`.
fn params<T: DeserializeOwned>(params: Option<Value>, shape: &str) -> BrpResult<T> {
    match params {
        None | Some(Value::Null) => {
            Err(invalid_params(format!("missing params, expected {shape}")))
        }
        Some(v) => serde_json::from_value(v)
            .map_err(|e| invalid_params(format!("bad params (expected {shape}): {e}"))),
    }
}

fn tick(world: &World) -> BrpResult<u64> {
    world
        .get_resource::<SimClock>()
        .map(|c| c.tick)
        .ok_or_else(|| not_ready("SimClock (NpcPlugins)"))
}

// ---------------------------------------------------------------- control

/// The current pause/speed: the kernel's `SimControl` (created on first use).
fn control_state(world: &mut World) -> BrpResult<ControlState> {
    let tick = tick(world)?;
    let headless = world.contains_resource::<HeadlessDriver>();
    let c = *world.get_resource_or_init::<SimControl>();
    Ok(ControlState {
        tick,
        paused: c.paused,
        multiplier: c.multiplier,
        driver: if headless { "headless" } else { "view" }.into(),
    })
}

fn set_control(world: &mut World, f: impl FnOnce(&mut bool, &mut u32)) -> BrpResult {
    tick(world)?;
    {
        let mut c = world.get_resource_or_init::<SimControl>();
        let c = &mut *c;
        f(&mut c.paused, &mut c.multiplier);
    }
    to_value(&control_state(world)?)
}

/// `npc/pause` → [`ControlState`].
pub fn pause(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    set_control(world, |paused, _| *paused = true)
}

/// `npc/resume` → [`ControlState`].
pub fn resume(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    set_control(world, |paused, _| *paused = false)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepParams {
    #[serde(default = "one")]
    ticks: u64,
}

impl Default for StepParams {
    fn default() -> Self {
        Self { ticks: 1 }
    }
}

fn one() -> u64 {
    1
}

/// Runs `NpcSimTick` `ticks` times now and returns the new tick and hash.
/// With the headless driver in charge, each tick also closes a profiler
/// frame (as the driver does); in the game the frame closes in `Last`.
pub fn step_ticks(world: &mut World, ticks: u64) -> BrpResult<StepResult> {
    tick(world)?;
    npc_kernel::ensure_frozen(world);
    let headless = world.contains_resource::<HeadlessDriver>();
    let perf = world.get_resource::<PerfHandle>().map(|h| h.0.clone());
    for _ in 0..ticks {
        if let Err(e) = world.try_run_schedule(NpcSimTick) {
            return Err(BrpError::internal(format!("cannot run NpcSimTick: {e}")));
        }
        if headless && let Some(perf) = &perf {
            perf.end_frame();
        }
    }
    let hash = state_hash(world);
    Ok(StepResult {
        tick: tick(world)?,
        ticks,
        hash,
        hash_hex: format!("{hash:016x}"),
    })
}

/// `npc/step {ticks = 1}` → [`StepResult`]. Works while paused.
pub fn step(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let StepParams { ticks } = params_or_default(p)?;
    if ticks > MAX_STEP {
        return Err(invalid_params(format!("ticks {ticks} exceeds {MAX_STEP}")));
    }
    to_value(&step_ticks(world, ticks)?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeedParams {
    multiplier: u32,
}

/// `npc/speed {multiplier}` → [`ControlState`]. `0` stops the sim like a
/// pause; the pause flag is left as it is.
pub fn speed(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let SpeedParams { multiplier } = params(p, "{\"multiplier\": u32}")?;
    if multiplier > MAX_SPEED {
        return Err(invalid_params(format!(
            "multiplier {multiplier} exceeds {MAX_SPEED}"
        )));
    }
    set_control(world, |_, m| *m = multiplier)
}

// ---------------------------------------------------------------- state

/// `npc/state` → the world as JSON (the installed inspector; for the
/// village `village_content::inspect::WorldSnapshot`).
pub fn state(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    tick(world)?;
    Ok(inspect::world_json(world))
}

/// Methods registered with feature `rules` (besides [`METHODS`]).
#[cfg(feature = "rules")]
pub const RULES_METHODS: [&str; 1] = ["npc/sheet"];

#[cfg(feature = "rules")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetParams {
    unit: u32,
}

/// `npc/sheet {unit}` (feature `rules`) → `{"unit", "tick", "sheet"}`, the
/// unit's `npc_rules::SheetView` (stats, traits, skills with effective
/// levels, points). Unknown unit → entity-not-found; a unit without a
/// character sheet → invalid-params.
#[cfg(feature = "rules")]
pub fn sheet(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let SheetParams { unit } = params(p, "{\"unit\": u32}")?;
    let now = tick(world)?;
    let known = world
        .get_resource::<npc_kernel::SimIndex>()
        .and_then(|i| i.get(SimId(unit)))
        .is_some();
    if !known {
        return Err(command_error(&CommandError::UnknownUnit { unit }));
    }
    match npc_rules::sheet_view(world, SimId(unit)) {
        Some(view) => Ok(json!({"unit": unit, "tick": now, "sheet": view})),
        None => Err(invalid_params(format!(
            "unit {unit} has no character sheet (npc_rules::CharacterSheet)"
        ))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitParams {
    id: u32,
}

/// `npc/unit {id}` → one unit as JSON with its recent journal (the
/// installed inspector; for the village `UnitSnapshot`).
pub fn unit(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let UnitParams { id } = params(p, "{\"id\": u32}")?;
    match inspect::unit_json(world, SimId(id)) {
        Some(snapshot) => Ok(snapshot),
        None => Err(command_error(&CommandError::UnknownUnit { unit: id })),
    }
}

// ---------------------------------------------------------------- input

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandParams {
    cmd: SimCommand,
    #[serde(default)]
    at_tick: Option<u64>,
}

/// `npc/command {cmd, at_tick?}` → [`CommandAccepted`]. Goes through
/// `npc_kernel::submit`, so it is validated, journaled and replayable.
pub fn command(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let CommandParams { cmd, at_tick } = params(
        p,
        "{\"cmd\": SimCommand, \"at_tick\"?: u64}, e.g. {\"cmd\":{\"type\":\"Order\",\"unit\":5,\"order\":{\"type\":\"GoTo\",\"pos\":[1000,2000]}}}",
    )?;
    match submit(world, at_tick, cmd) {
        Ok((tick, seq)) => to_value(&CommandAccepted { tick, seq }),
        Err(err) => Err(command_error(&err)),
    }
}

// ---------------------------------------------------------------- data

/// `npc/journal` params: `{"filter": JournalFilter}` or the filter fields
/// directly (`{"unit":5,"limit":20}`); absent = everything.
fn journal_filter(p: Option<Value>) -> BrpResult<JournalFilter> {
    match p {
        Some(Value::Object(mut map)) if map.contains_key("filter") => {
            let filter = map.remove("filter").unwrap_or(Value::Null);
            if !map.is_empty() {
                return Err(invalid_params(
                    "give either {\"filter\": {..}} or the filter fields",
                ));
            }
            params_or_default(Some(filter))
        }
        other => params_or_default(other),
    }
}

/// `npc/journal {filter}` → `[JournalEntry]` in `(tick, seq)` order.
pub fn journal(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let filter = journal_filter(p)?;
    let journal = world
        .get_resource::<Journal>()
        .ok_or_else(|| not_ready("Journal (SimPlugin)"))?;
    if !journal.enabled && journal.entries.is_empty() {
        return Err(BrpError {
            code: error_codes::RESOURCE_ERROR,
            message: "the journal is off (enable it with JournalPlugin, `village` debug-tools, or `npc/record/start`)"
                .into(),
            data: None,
        });
    }
    to_value(&query(journal, &filter))
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PerfParams {
    #[serde(default)]
    top: Option<usize>,
}

fn perf_handle(world: &World) -> BrpResult<PerfHandle> {
    world
        .get_resource::<PerfHandle>()
        .cloned()
        .ok_or_else(|| BrpError {
            code: error_codes::RESOURCE_NOT_PRESENT,
            message: "the profiler is not installed (no PerfHandle; build with debug-tools)".into(),
            data: None,
        })
}

/// `npc/perf {top?}` → `npc_perf::PerfReport` (with `top`, only the top
/// `top` systems and mechanics).
pub fn perf(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let PerfParams { top } = params_or_default(p)?;
    let collector = perf_handle(world)?.0;
    // `report_top` only summarises the rows it returns.
    let report = top.map_or_else(|| collector.report(), |n| collector.report_top(n));
    to_value(&report)
}

/// `npc/perf/reset` → `{"reset": true}`.
pub fn perf_reset(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    perf_handle(world)?.0.reset();
    Ok(json!({"reset": true}))
}

/// `npc/methods` → the `npc/*` method names (`rpc.discover` lists all).
pub fn methods(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let mut names: Vec<String> = world
        .get_resource::<bevy::remote::RemoteMethods>()
        .map(|m| m.methods())
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.starts_with("npc/"))
        .collect();
    names.sort();
    Ok(json!(names))
}

// ---------------------------------------------------------------- recording

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordStartParams {
    #[serde(default = "default_every")]
    every: u64,
}

impl Default for RecordStartParams {
    fn default() -> Self {
        Self {
            every: DEFAULT_RECORD_EVERY,
        }
    }
}

fn default_every() -> u64 {
    DEFAULT_RECORD_EVERY
}

/// Starts recording (and turns the journal on, keeping its verbosity).
/// Errors when already recording or the app has no `ScenarioInfo`.
pub fn start_recording(world: &mut World, every: u64) -> BrpResult<RecordStatus> {
    let now = tick(world)?;
    if !world.contains_resource::<ScenarioInfo>() {
        return Err(not_ready(
            "ScenarioInfo (the app must be built by the content's app factory for replays)",
        ));
    }
    if world
        .get_resource::<Recording>()
        .is_some_and(|r| r.active.is_some())
    {
        return Err(invalid_params("already recording (npc/record/stop first)"));
    }
    let verbose = world.get_resource::<Journal>().is_some_and(|j| j.verbose);
    npc_kernel::journal::set_journal(world, true, verbose);
    let recorder = Recorder::live(world, every);
    let status = RecordStatus {
        recording: true,
        started_at: recorder.started_at(),
        every: recorder.every(),
    };
    debug_assert_eq!(status.started_at, now);
    world.get_resource_or_init::<Recording>().active = Some(recorder);
    Ok(status)
}

/// Stops recording and returns the replay: the whole run from tick 0 to
/// now, with every command in the `CommandLog`.
pub fn stop_recording(world: &mut World) -> BrpResult<Replay> {
    tick(world)?;
    let Some(active) = world
        .get_resource_mut::<Recording>()
        .and_then(|mut r| r.active.take())
    else {
        return Err(invalid_params("not recording (npc/record/start first)"));
    };
    active.finish(world).map_err(|message| BrpError {
        code: error_codes::RESOURCE_NOT_PRESENT,
        message,
        data: None,
    })
}

/// Writes `replay` as pretty JSON to `path`, creating parent directories.
pub fn write_replay(path: &Path, replay: &Replay) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(replay).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// `npc/record/start {every = 100}` → [`RecordStatus`].
pub fn record_start(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let RecordStartParams { every } = params_or_default(p)?;
    to_value(&start_recording(world, every)?)
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RecordStopParams {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    overwrite: bool,
}

/// `npc/record/stop {path?, overwrite?}` → `npc_kernel::replay::Replay`
/// (also written to `path` when given). `path` is relative and confined under
/// the [`OutputRoot`] (`crate::out_path`: no absolute paths, no `..`,
/// extension `.json`, no overwrite unless `overwrite: true`); a refused path
/// is an invalid-params error *before* the recording stops. On a write
/// error the replay is in the error's `data`.
pub fn record_stop(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let RecordStopParams { path, overwrite } = params_or_default(p)?;
    let path = match path {
        None => None,
        Some(req) => {
            let root = world.get_resource_or_init::<OutputRoot>().clone();
            Some(out_path::resolve(&root.0, &req, "json", overwrite).map_err(invalid_params)?)
        }
    };
    let replay = stop_recording(world)?;
    let root = world.get_resource_or_init::<OutputRoot>().0.clone();
    if let Some(path) = path
        && let Err(e) = out_path::prepare(&root, &path).and_then(|()| write_replay(&path, &replay))
    {
        return Err(BrpError {
            code: error_codes::INTERNAL_ERROR,
            message: e,
            data: serde_json::to_value(&replay).ok(),
        });
    }
    to_value(&replay)
}

/// `Last`: takes a checkpoint when recording and `every` ticks have passed.
pub fn checkpoint_system(world: &mut World) {
    let Some(now) = world.get_resource::<SimClock>().map(|c| c.tick) else {
        return;
    };
    let due = world
        .get_resource::<Recording>()
        .and_then(|r| r.active.as_ref())
        .is_some_and(|a| a.due(now));
    if !due {
        return;
    }
    let Some(mut active) = world.resource_mut::<Recording>().active.take() else {
        return;
    };
    active.after_tick(world);
    world.resource_mut::<Recording>().active = Some(active);
}

/// `Last`: on `AppExit`, writes the active recording to [`RecordOnExit`].
pub fn record_on_exit_system(world: &mut World) {
    let exiting = world
        .get_resource::<Messages<AppExit>>()
        .is_some_and(|m| !m.is_empty());
    if !exiting {
        return;
    }
    let Some(RecordOnExit(path)) = world.remove_resource::<RecordOnExit>() else {
        return;
    };
    match stop_recording(world) {
        Ok(replay) => match write_replay(&path, &replay) {
            Ok(()) => info!(
                "npc_tools: wrote replay ({} ticks, {} commands) -> {}",
                replay.ticks,
                replay.commands.len(),
                path.display()
            ),
            Err(e) => error!("npc_tools: {e}"),
        },
        Err(e) => error!("npc_tools: no replay written: {}", e.message),
    }
}

#[cfg(all(test, feature = "rules"))]
mod sheet_tests {
    use super::*;
    use npc_kernel::{NpcKernelPlugin, spawn_sim};
    use npc_rules::{CharacterSheet, RulesPlugin};

    #[test]
    fn npc_sheet_returns_the_sheet_view() {
        let mut app = App::new();
        app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
        app.finish();
        let sheet = CharacterSheet {
            unspent: 7,
            ..Default::default()
        };
        let (unit, _) = spawn_sim(app.world_mut(), sheet);
        let (bare, _) = spawn_sim(app.world_mut(), ());
        let world = app.world_mut();
        let out = super::sheet(In(Some(json!({"unit": unit.0}))), world).expect("sheet");
        assert_eq!(out["unit"], json!(unit.0));
        assert_eq!(out["sheet"]["points"]["unspent"], json!(7));
        assert_eq!(out["sheet"]["stats"][0]["name"], json!("ST"));
        let err = super::sheet(In(Some(json!({"unit": bare.0}))), world).unwrap_err();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
        let err = super::sheet(In(Some(json!({"unit": 999}))), world).unwrap_err();
        assert_eq!(err.code, error_codes::ENTITY_NOT_FOUND);
        assert!(super::sheet(In(None), world).is_err());
    }
}
