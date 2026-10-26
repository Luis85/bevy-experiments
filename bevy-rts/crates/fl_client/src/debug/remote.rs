//! The remote debug endpoint: Bevy Remote Protocol (JSON-RPC over localhost HTTP) with the
//! `fl.*` methods, also published as agent tools through `bevy_brp_extras` (spec §15 C,
//! ADR-0008).
//!
//! The sim is reachable only through these methods. Reads are [`SimQuery`]s; writes are
//! commands issued through the [`LocalDriver`] or driver controls (pause, resume, step, speed,
//! load). The built-in `world.*` methods see only the client World.
//!
//! A method that needs the runner's answer waits for it on the main thread for at most
//! [`REPLY_TIMEOUT`]. Replies are matched by query id; tick outputs and failures read meanwhile
//! go to the [`RunnerBacklog`], which `drive_runner` handles next, so nothing is dropped. Reads
//! of the newest tick, hash or snapshot count that backlog.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::remote::http::RemoteHttpPlugin;
use bevy::remote::{BrpError, BrpResult, RemotePlugin, error_codes};
use bevy::window::ExitCondition;
use bevy_brp_extras::{AgentTool, AppAgentToolExt, BrpExtrasPlugin};
use fl_content::{ContentDb, ScenarioDef, ScenarioUnit};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{DebugSpawnCmd, PlayerCommand, PlayerId, SimId, Tick};
use fl_view::{FromRunner, SimQuery, SimReply, SimSnapshot, TickDriver, ToRunner};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{
    CurrentScenario, LoadRequest, OverlayRegistry, ProfilerSource, frame_stats, issue_in_world,
    issue_json, load_match, screenshot,
};
use crate::driver::{LocalDriver, MAX_SPEED, MIN_SPEED};
use crate::link::{
    Deadline, InputLog, PendingReplies, RunnerBacklog, RunnerLink, RunnerStatus, SnapshotBuffer,
    WaitError, failure_text, read_runner, send_query, wait_for_reply,
};
use crate::view::ClientContent;

/// The default BRP port; every agent running the game picks its own (CLAUDE.md).
pub const DEFAULT_PORT: u16 = 15702;
/// The longest a method waits for the sim runner.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
/// Journal entries `fl.journal_since` returns when no limit is given.
const DEFAULT_JOURNAL_LIMIT: u32 = 1000;

/// Serves BRP on `127.0.0.1:port` with the `fl.*` methods and the `bevy_brp_extras` methods,
/// and publishes every `fl.*` method as an agent tool. Compiled only with the `debug_remote`
/// feature in debug builds, never in release builds (spec §15 C).
pub struct RemoteDebugPlugin {
    pub port: u16,
}

impl Default for RemoteDebugPlugin {
    fn default() -> Self {
        RemoteDebugPlugin { port: DEFAULT_PORT }
    }
}

/// A BRP method handler.
type Method = fn(In<Option<Value>>, &mut World) -> BrpResult;

/// Every `fl.*` method: name, agent tool description, handler.
const METHODS: &[(&str, &str, Method)] = &[
    (
        "fl.sim_state",
        "Sim world summary (tick, seed, state_hash, entities, players, map, last_seq) plus \
         last_hash, paused and speed. No params. Poll it for readiness (tick >= T).",
        sim_state,
    ),
    (
        "fl.entity",
        "One unit's saved sim components by SimId. Params {sim_id}. null if it does not exist.",
        entity,
    ),
    (
        "fl.hash",
        "The newest state hash received from the sim (hex; the sim hashes every 20 ticks), \
         and the last tick received. No params.",
        hash,
    ),
    (
        "fl.issue_command",
        "Issue a command through the driver. Params {player, units: [sim_id], command: name \
         such as \"core.move\", payload: JSON such as {\"target\": [x, y]}}. Returns \
         {exec_tick, seq}.",
        issue_command,
    ),
    (
        "fl.pause",
        "Pause the sim (driver control). No params.",
        pause,
    ),
    (
        "fl.resume",
        "Resume the sim (driver control). No params.",
        resume,
    ),
    (
        "fl.step",
        "Pause if running, then run n more ticks (default 1, at most 600) and wait for them, \
         at most 2 s. Params {n}. Returns {tick: the next tick the sim runs, completed, \
         requested, finished}; when finished is false the rest stay queued and run over the \
         next frames.",
        step,
    ),
    (
        "fl.set_speed",
        "Set the game speed (driver control). Params {x} in 0.25..=8.",
        set_speed,
    ),
    (
        "fl.spawn",
        "Spawn a unit by issuing debug.spawn as the local player. Params {unit: content id \
         such as \"base.de.pz3\", player, x, y, heading_deg (default 0)}. Returns \
         {exec_tick, seq}.",
        spawn,
    ),
    (
        "fl.journal_since",
        "Sim journal entries from a tick on. Params {tick, limit (default 1000)}.",
        journal_since,
    ),
    (
        "fl.perf",
        "Perf data: {profiler: per feature/phase/system p50/p95/max, sim: work counters of \
         the last tick, frame: fps, frame time, entity count, render stats}. No params.",
        perf,
    ),
    (
        "fl.screenshot",
        "Render the game camera's view with an offscreen camera and save it as a PNG. Params \
         {path}. The file appears a few frames after the reply; poll for it.",
        screenshot_method,
    ),
    (
        "fl.debug_query",
        "Run a read-only sim debug query (ADR-0015). Params {name, args}, such as \
         {name: \"core.debug_queries\"} to list every name, or {name: \"core.paths\", \
         args: {units: [sim_id]}}. Returns the query's JSON (Fx as raw i64), or an error \
         with the query's message.",
        debug_query,
    ),
    (
        "fl.toggle_overlay",
        "Toggle a client debug overlay. Params {name}, such as \"grid\". Returns {name, on}.",
        toggle_overlay,
    ),
    (
        "fl.load_scenario",
        "Replace the match with a scenario of the loaded content; blocks until the new sim \
         is built. Params {id}, such as \"base.testbed\". Returns {tick}.",
        load_scenario,
    ),
    (
        "fl.load",
        "Replace the match with a save written by fl.save; blocks until the new sim is \
         built. Params {path}. Returns {tick}.",
        load,
    ),
    (
        "fl.save_scenario",
        "Write the current snapshot's units, with the map and players of the scenario the \
         match started from, as a *.scenario.ron. Params {path, id (default: the file stem)}.",
        save_scenario,
    ),
    (
        "fl.select",
        "Replace the selection with the local player's units among the given ids; other ids \
         are ignored. Params {sim_ids: [sim_id]}. Returns {selected: [sim_id]}.",
        select,
    ),
    (
        "fl.selection",
        "The selected units. No params. Returns {selected: [sim_id]}.",
        selection,
    ),
    (
        "fl.save",
        "Save the match (sim state and pending commands) to a file. Params {path}. Returns \
         {path, bytes}.",
        save,
    ),
];

impl Plugin for RemoteDebugPlugin {
    fn build(&self, app: &mut App) {
        // The extras' input methods need the input and window messages, which a headless app
        // lacks: give it those plugins, without a window and without exiting for lack of one.
        if !app.is_plugin_added::<InputPlugin>() {
            app.add_plugins(InputPlugin);
        }
        if !app.is_plugin_added::<WindowPlugin>() {
            app.add_plugins(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                close_when_requested: false,
                ..default()
            });
        }
        let mut remote = RemotePlugin::default();
        for &(name, _, handler) in METHODS {
            remote = remote.with_method_main(name, handler);
        }
        app.add_plugins(remote)
            .add_plugins(BrpExtrasPlugin::with_http_plugin(
                RemoteHttpPlugin::default().with_port(self.port),
            ))
            .init_resource::<PendingReplies>()
            .init_resource::<RunnerBacklog>();
        for &(name, description, _) in METHODS {
            app.register_agent_tool(AgentTool::new(name, name, description));
        }
        screenshot::build(app);
        tracing::info!(port = self.port, "remote debug endpoint (BRP) on 127.0.0.1");
    }
}

const NO_MATCH: &str = "no match is running";

fn invalid_params(message: impl std::fmt::Display) -> BrpError {
    BrpError {
        code: error_codes::INVALID_PARAMS,
        message: format!("invalid params: {message}"),
        data: None,
    }
}

fn failed(message: impl Into<String>) -> BrpError {
    BrpError {
        code: error_codes::INTERNAL_ERROR,
        message: message.into(),
        data: None,
    }
}

/// The params as `T`; missing params read as `{}`.
fn params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, BrpError> {
    let value = match params {
        None | Some(Value::Null) => Value::Object(serde_json::Map::new()),
        Some(value) => value,
    };
    serde_json::from_value(value).map_err(invalid_params)
}

/// The match's driver, mutably.
fn driver_mut(world: &mut World) -> Result<Mut<'_, LocalDriver>, BrpError> {
    world
        .get_resource_mut::<LocalDriver>()
        .ok_or_else(|| failed(NO_MATCH))
}

/// Why the runner can no longer answer, if it can't.
fn runner_failure(world: &World) -> Option<String> {
    let from_status = world
        .get_resource::<RunnerStatus>()
        .and_then(|s| s.failed.clone());
    let from_backlog = || {
        world.get_resource::<RunnerBacklog>().and_then(|b| {
            b.0.iter().find_map(|m| match m {
                FromRunner::Failed(failure) => Some(failure.clone()),
                _ => None,
            })
        })
    };
    from_status.or_else(from_backlog).map(|f| failure_text(&f))
}

/// The common checks of a method that talks to the runner: a match runs and its runner works.
fn check_runner(world: &World) -> Result<(), BrpError> {
    if !world.contains_resource::<RunnerLink>() || !world.contains_resource::<LocalDriver>() {
        return Err(failed(NO_MATCH));
    }
    runner_failure(world).map_or(Ok(()), |failure| Err(failed(failure)))
}

/// Sends `q` and waits for its reply, at most [`REPLY_TIMEOUT`] ([`wait_for_reply`]); a
/// [`SimReply::Error`] is an error. A query given up on is abandoned: its late reply is
/// dropped.
fn query(world: &mut World, q: SimQuery) -> Result<SimReply, BrpError> {
    check_runner(world)?;
    world.init_resource::<PendingReplies>();
    let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
        send_query(world.resource::<RunnerLink>(), &mut replies, q)
    });
    match wait_for_reply(world, id, REPLY_TIMEOUT) {
        Ok(SimReply::Error(message)) | Err(message) => Err(failed(message)),
        Ok(reply) => Ok(reply),
    }
}

fn unexpected(reply: &SimReply) -> BrpError {
    failed(format!("unexpected reply from the sim runner: {reply:?}"))
}

fn hash_hex(hash: Option<u64>) -> Value {
    hash.map_or(Value::Null, |h| Value::String(format!("{h:016x}")))
}

fn sim_state(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let reply = query(world, SimQuery::WorldSummary)?;
    let SimReply::WorldSummary(mut summary) = reply else {
        return Err(unexpected(&reply));
    };
    let (_, last_hash) = latest_received(world);
    let driver = world.get_resource::<LocalDriver>();
    if let Value::Object(map) = &mut summary {
        map.insert("last_hash".to_owned(), hash_hex(last_hash));
        map.insert(
            "paused".to_owned(),
            json!(driver.map(LocalDriver::is_paused)),
        );
        map.insert("speed".to_owned(), json!(driver.map(LocalDriver::speed)));
    }
    Ok(summary)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityParams {
    sim_id: u32,
}

fn entity(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let EntityParams { sim_id } = params(p)?;
    match query(world, SimQuery::Entity(SimId(sim_id)))? {
        SimReply::Entity(value) => Ok(value.unwrap_or(Value::Null)),
        other => Err(unexpected(&other)),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DebugQueryParams {
    name: String,
    #[serde(default)]
    args: Value,
}

fn debug_query(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let DebugQueryParams { name, args } = params(p)?;
    let args = if args.is_null() { json!({}) } else { args };
    match query(world, SimQuery::Debug { name, args })? {
        SimReply::Debug(value) => Ok(value),
        other => Err(unexpected(&other)),
    }
}

fn hash(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    if !world.contains_resource::<RunnerStatus>() {
        return Err(failed(NO_MATCH));
    }
    let (last_tick, hash) = latest_received(world);
    Ok(json!({
        "hash": hash_hex(hash),
        "last_tick": last_tick.map(|t| t.0),
    }))
}

fn issued(command: &PlayerCommand) -> Value {
    json!({
        "exec_tick": command.exec_tick.0,
        "seq": command.seq,
        "player": command.player.0,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueParams {
    player: u8,
    #[serde(default)]
    units: Vec<u32>,
    command: String,
    #[serde(default)]
    payload: Value,
}

fn issue_command(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let IssueParams {
        player,
        units,
        command,
        payload,
    } = params(p)?;
    let units = units.into_iter().map(SimId).collect();
    let command = issue_json(world, PlayerId(player), units, &command, payload).map_err(failed)?;
    Ok(issued(&command))
}

fn driver_state(driver: &LocalDriver) -> Value {
    json!({
        "paused": driver.is_paused(),
        "speed": driver.speed(),
        "next_tick": driver.next_tick().0,
    })
}

fn pause(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let mut driver = driver_mut(world)?;
    driver.pause();
    Ok(driver_state(&driver))
}

fn resume(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let mut driver = driver_mut(world)?;
    driver.resume();
    Ok(driver_state(&driver))
}

fn one() -> u32 {
    1
}

/// The most ticks one `fl.step` runs: 30 s of sim (ruling).
pub const MAX_STEP: u32 = 600;
/// Ticks `fl.step` keeps in flight ahead of the last one received.
const STEP_WINDOW: u32 = 8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepParams {
    #[serde(default = "one")]
    n: u32,
}

/// The tick and hash of the newest tick output received, counting the backlog `drive_runner`
/// has not handled yet (right after `fl.step`). The hash is the newest one sent, which may be
/// an older tick's (the sim hashes every `interval_ticks`).
fn latest_received(world: &World) -> (Option<Tick>, Option<u64>) {
    let status = world.get_resource::<RunnerStatus>();
    let mut tick = status.and_then(|s| s.last_tick);
    let mut hash = status.and_then(|s| s.last_hash);
    if let Some(backlog) = world.get_resource::<RunnerBacklog>() {
        for message in &backlog.0 {
            if let FromRunner::Tick(output) = message {
                tick = Some(output.tick);
                hash = output.hash.or(hash);
            }
        }
    }
    (tick, hash)
}

/// Sends the next tick input of the driver (one more step while paused); returns its tick.
fn send_one_step(world: &mut World) -> Result<Tick, BrpError> {
    world.resource_scope(|world, mut driver: Mut<LocalDriver>| {
        driver.step(1);
        let input = driver
            .poll()
            .ok_or_else(|| failed("the driver gave no input for a step"))?;
        let tick = input.tick;
        if let Some(mut log) = world.get_resource_mut::<InputLog>() {
            log.record(&input);
        }
        world
            .resource::<RunnerLink>()
            .port
            .tx
            .send(ToRunner::Tick(input))
            .map_err(|_| failed("the sim runner is gone"))?;
        Ok(tick)
    })
}

/// Pauses, then runs `n` ticks, at most [`STEP_WINDOW`] in flight, until all are received or
/// [`REPLY_TIMEOUT`] passes. Replies `{tick, completed, requested, finished}`, where `tick` is
/// the next tick the sim runs after the completed ones. When the time is up, the steps not yet
/// sent stay queued in the driver, which runs them over the next frames (`finished: false`).
fn step(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let StepParams { n } = params(p)?;
    if n > MAX_STEP {
        return Err(invalid_params(format!(
            "n = {n} is more than {MAX_STEP} ticks; step in parts"
        )));
    }
    check_runner(world)?;
    world.init_resource::<PendingReplies>();
    world.init_resource::<RunnerBacklog>();
    let first = {
        let mut driver = world.resource_mut::<LocalDriver>();
        driver.pause();
        driver.next_tick()
    };
    let deadline = Deadline::after(REPLY_TIMEOUT);
    let (mut sent, mut completed) = (0_u32, 0_u32);
    let finished = loop {
        while sent < n && sent - completed < STEP_WINDOW {
            send_one_step(world)?;
            sent += 1;
        }
        if completed == n {
            break true;
        }
        match read_runner(world, &deadline) {
            Ok(Some(tick)) if tick.0 >= first.0 => completed = tick.0 - first.0 + 1,
            Ok(_) => {}
            Err(WaitError::Timeout) => break false,
            Err(WaitError::Failed(message)) => return Err(failed(message)),
        }
    };
    if !finished {
        world.resource_mut::<LocalDriver>().step(n - sent);
    }
    Ok(json!({
        "tick": first.0 + completed,
        "completed": completed,
        "requested": n,
        "finished": finished,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeedParams {
    x: f32,
}

fn set_speed(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let SpeedParams { x } = params(p)?;
    if !(MIN_SPEED..=MAX_SPEED).contains(&x) {
        return Err(invalid_params(format!(
            "the speed must be in {MIN_SPEED}..={MAX_SPEED}"
        )));
    }
    let mut driver = driver_mut(world)?;
    driver.set_speed(x);
    Ok(driver_state(&driver))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpawnParams {
    unit: String,
    player: u8,
    x: f64,
    y: f64,
    #[serde(default)]
    heading_deg: f64,
}

fn metres(value: f64, what: &str) -> Result<Fx, BrpError> {
    Fx::from_f64(value).map_err(|err| invalid_params(format!("{what}: {err}")))
}

fn spawn(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let SpawnParams {
        unit,
        player,
        x,
        y,
        heading_deg,
    } = params(p)?;
    if let Some(content) = world.get_resource::<ClientContent>()
        && content.0.unit_id(&unit).is_none()
    {
        return Err(invalid_params(format!("no unit named '{unit}'")));
    }
    let payload = DebugSpawnCmd {
        unit,
        owner: PlayerId(player),
        pos: FxVec2::new(metres(x, "x")?, metres(y, "y")?),
        heading: Angle::from_degrees(metres(heading_deg, "heading_deg")?),
    };
    let command = issue_in_world(world, Vec::new(), &payload).map_err(failed)?;
    Ok(issued(&command))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalParams {
    #[serde(default)]
    tick: u32,
    limit: Option<u32>,
}

fn journal_since(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let JournalParams { tick, limit } = params(p)?;
    let q = SimQuery::JournalSince {
        tick: Tick(tick),
        limit: limit.unwrap_or(DEFAULT_JOURNAL_LIMIT),
    };
    match query(world, q)? {
        SimReply::Journal(entries) => serde_json::to_value(entries).map_err(failed_display),
        other => Err(unexpected(&other)),
    }
}

fn failed_display(err: impl std::fmt::Display) -> BrpError {
    failed(err.to_string())
}

fn perf(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let profiler = world
        .get_resource::<ProfilerSource>()
        .map_or(Value::Null, |source| (source.0)());
    let sim = match query(world, SimQuery::Perf)? {
        SimReply::Perf(value) => value,
        other => return Err(unexpected(&other)),
    };
    Ok(json!({
        "profiler": profiler,
        "sim": sim,
        "frame": frame_stats(world),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathParams {
    path: PathBuf,
}

fn screenshot_method(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let PathParams { path } = params(p)?;
    let size = screenshot::request_screenshot(world, &path).map_err(failed)?;
    Ok(json!({ "path": path, "width": size.x, "height": size.y }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverlayParams {
    name: String,
}

fn toggle_overlay(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let OverlayParams { name } = params(p)?;
    let on = OverlayRegistry::toggle(world, &name).map_err(invalid_params)?;
    Ok(json!({ "name": name, "on": on }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioParams {
    id: String,
}

fn load_scenario(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let ScenarioParams { id } = params(p)?;
    let tick = load_match(world, LoadRequest::Scenario(id.clone())).map_err(failed)?;
    Ok(json!({ "scenario": id, "tick": tick.0 }))
}

fn load(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let PathParams { path } = params(p)?;
    let bytes = std::fs::read(&path)
        .map_err(|err| failed(format!("cannot read {}: {err}", path.display())))?;
    let tick = load_match(world, LoadRequest::Save(bytes)).map_err(failed)?;
    Ok(json!({ "path": path, "tick": tick.0 }))
}

fn save(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let PathParams { path } = params(p)?;
    let pending = world
        .get_resource::<LocalDriver>()
        .ok_or_else(|| failed(NO_MATCH))?
        .pending();
    let bytes = match query(world, SimQuery::Save { pending })? {
        SimReply::Save(bytes) => bytes,
        other => return Err(unexpected(&other)),
    };
    std::fs::write(&path, &bytes)
        .map_err(|err| failed(format!("cannot write {}: {err}", path.display())))?;
    Ok(json!({ "path": path, "bytes": bytes.len() }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveScenarioParams {
    path: PathBuf,
    id: Option<String>,
}

/// The file name suffix of scenario files.
const SCENARIO_SUFFIX: &str = ".scenario.ron";

fn save_scenario(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let SaveScenarioParams { path, id } = params(p)?;
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(SCENARIO_SUFFIX))
        .ok_or_else(|| invalid_params(format!("the path must end in '{SCENARIO_SUFFIX}'")))?;
    let id = id.unwrap_or_else(|| stem.to_owned());
    let content = world
        .get_resource::<ClientContent>()
        .ok_or_else(|| failed(NO_MATCH))?;
    let scenario = world
        .get_resource::<CurrentScenario>()
        .and_then(|c| c.0.clone())
        .ok_or_else(|| failed("the match did not start from a scenario; nothing to base it on"))?;
    let template = content
        .0
        .kind::<ScenarioDef>()
        .get(&scenario)
        .ok_or_else(|| failed(format!("scenario '{scenario}' is not loaded")))?;
    let snapshot = latest_snapshot(world).ok_or_else(|| failed("no snapshot received yet"))?;
    let def =
        scenario_from_snapshot(&content.0, template, id.clone(), &snapshot).map_err(failed)?;
    let text = scenario_ron(&def).map_err(failed)?;
    std::fs::write(&path, text)
        .map_err(|err| failed(format!("cannot write {}: {err}", path.display())))?;
    Ok(json!({ "path": path, "id": id, "units": def.units.len() }))
}

fn selected_json(ids: impl IntoIterator<Item = SimId>) -> Value {
    json!({ "selected": ids.into_iter().map(|id| id.0).collect::<Vec<_>>() })
}

fn select(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    #[derive(Deserialize)]
    struct Params {
        sim_ids: Vec<u32>,
    }
    let Params { sim_ids } = params(p)?;
    let snapshot = latest_snapshot(world).ok_or_else(|| failed("no snapshot has arrived yet"))?;
    let chosen = crate::selection::select_own(world, &snapshot, sim_ids.into_iter().map(SimId))
        .ok_or_else(|| failed("selection is not available: no local player or plugin"))?;
    Ok(selected_json(chosen))
}

fn selection(In(_): In<Option<Value>>, world: &mut World) -> BrpResult {
    let selection = world
        .get_resource::<crate::selection::Selection>()
        .ok_or_else(|| failed("selection is not available: SelectionPlugin is not added"))?;
    Ok(selected_json(selection.0.iter().copied()))
}

/// The newest snapshot received, counting the backlog that `drive_runner` has not handled yet
/// (right after `fl.step`).
fn latest_snapshot(world: &World) -> Option<Arc<SimSnapshot>> {
    let backlog = world.get_resource::<RunnerBacklog>().and_then(|b| {
        b.0.iter().rev().find_map(|m| match m {
            FromRunner::Tick(output) => Some(output.snapshot.clone()),
            _ => None,
        })
    });
    backlog.or_else(|| world.get_resource::<SnapshotBuffer>()?.curr.clone())
}

/// `template` with `id` and the units of `snapshot`, in snapshot order.
///
/// # Errors
///
/// If a record's unit type is not in `content`.
pub fn scenario_from_snapshot(
    content: &ContentDb,
    template: &ScenarioDef,
    id: String,
    snapshot: &SimSnapshot,
) -> Result<ScenarioDef, String> {
    let units = snapshot
        .entities
        .iter()
        .map(|record| {
            let unit = content
                .unit(record.unit)
                .ok_or_else(|| format!("unit {} has an unknown type", record.sim_id))?;
            Ok(ScenarioUnit {
                unit: unit.id.clone(),
                player: record.owner.0,
                pos: record.pos,
                heading_deg: record.heading.to_degrees(),
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(ScenarioDef {
        id,
        map: template.map.clone(),
        seed: template.seed,
        players: template.players.clone(),
        units,
    })
}

/// `def` as the text of a `*.scenario.ron` file.
///
/// # Errors
///
/// If it does not serialize.
pub fn scenario_ron(def: &ScenarioDef) -> Result<String, String> {
    let config = ron::ser::PrettyConfig::new().struct_names(true);
    ron::ser::to_string_pretty(def, config)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_is_named_fl_and_described() {
        for &(name, description, _) in METHODS {
            assert!(name.starts_with("fl."), "{name}");
            assert!(
                !name.ends_with("+watch"),
                "{name}: agent tools can't be watching"
            );
            assert!(description.len() > 10, "{name}");
        }
        let mut names: Vec<&str> = METHODS.iter().map(|m| m.0).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), METHODS.len(), "duplicate method names");
    }

    #[test]
    fn params_default_to_an_empty_object_and_reject_unknown_fields() {
        let StepParams { n } = params(None).expect("defaults");
        assert_eq!(n, 1);
        let err = params::<StepParams>(Some(json!({"n": 2, "m": 3})))
            .err()
            .expect("unknown field");
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
    }
}
