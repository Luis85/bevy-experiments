//! The window-free explorer host (spec G7, plan Task 6): one agent's world
//! in the testbed, started New or resumed from its save, plus what a running
//! world writes into the agent folder (the generated workspace, `run.json`)
//! and the notes resources. The windowed `actor_testbed` and the detached
//! host (`npcctl serve`) both start explorer worlds through here.
//!
//! No wall clock in here: the caller passes the New seed and `now` (RFC 3339,
//! [`rfc3339`]) for the `created` / `saved` / backup dates and
//! [`RunFile::started`]. The agent folder and the save never enter sim
//! state: the sim only sees the seed and the scenario document, and a save
//! is loaded before the first frame (ADR 0013).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::notes::{NotesClock, NotesDirs, NotesOwner, NotesStores};
use npc_framework::agent_io::scenario::{ScenarioDoc, attach_controllers};
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::perception::{FogResets, Perceivable, PerceptionConfig, ReachSense, RevealsFog};
use npc_framework::prelude::{SimControl, SimId};
use npc_framework::snapshot::{self, LoadError, LoadMode, LoadReport, WorldSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::home::{AgentDir, AgentMeta, create_agent_as, read_lossy, slug};
use super::prompt::{PromptKind, PromptVars, render_prompt};
use super::settings::default_actors_dir;
use super::workspace::{ActorSummary, WorkspaceVars, write_atomic, write_workspace};
use crate::TestbedIds;
use crate::actors::{ActorPreset, ActorPresets, builtin_preset, find_preset, validate};
use crate::world::{Prop, PropKind, kinds};

/// The embedded scenario every explorer world is built from.
pub const SCENARIO: &str = "explorer";
/// The scenario name of the actor the agent plays (renamed to the agent).
pub const ACTOR: &str = "explorer";
/// The default actor preset (`crate::actors::HUMAN`).
pub const HUMAN_PRESET: &str = crate::actors::HUMAN;
/// Start of the error for a save that needs `--load-anyway` / `--fresh-world`.
const NEEDS_CHOICE: &str = "save from another build";
/// How long an explorer actor remembers a thing after it last sensed it:
/// 60 s at 20 Hz (spec M1). Other scenarios keep the framework default.
pub const EXPLORER_FORGET_AFTER: u64 = 1200;
/// How many things an explorer actor's perception memory holds (and one scan
/// senses, nearest first).
pub const EXPLORER_MEMORY_CAPACITY: u32 = 128;
/// The explorer actor's all-round reach ([`ReachSense`], spec F3) before
/// actor presets: 1.5 m, the interaction reach, the built-in presets' `reach`
/// (a preset's own `reach` sensor sets it now). Other scenarios' actors have
/// none.
pub const EXPLORER_REACH_MM: i32 = 1500;
/// The save header's actor preset id.
pub const HEADER_PRESET: &str = "preset";
/// The save header's actor preset as the world was built with it (a resume
/// rebuilds from it, not from the actors folder).
pub const HEADER_ACTOR_PRESET: &str = "actor_preset";
/// The running world's file in the agent folder (owned, never generated).
pub const RUN_FILE: &str = "run.json";

/// How to start an explorer world (validated by the caller's flag parser).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostArgs {
    /// The agents folder (`--explorer-home`).
    pub home: PathBuf,
    /// The agent's name (`--agent`).
    pub agent: String,
    /// The world's name (`--world`; New only).
    pub world: Option<String>,
    /// `--resume`: rebuild from the save instead of creating the agent.
    pub resume: bool,
    /// What to do with a save from another build.
    pub on_mismatch: Option<Mismatch>,
    /// The actor preset's id (`--preset`; New only, default
    /// [`HUMAN_PRESET`]): any preset in [`HostArgs::actors_dir`] that reads
    /// and validates. A resume keeps the agent's preset (`agent.json`); a
    /// different one here is an error.
    pub preset: Option<String>,
    /// The actor presets folder; `None`:
    /// [`default_actors_dir`](super::settings::default_actors_dir) of
    /// [`HostArgs::home`].
    pub actors_dir: Option<PathBuf>,
}

impl HostArgs {
    /// The actor presets folder ([`HostArgs::actors_dir`]).
    pub fn actors_dir(&self) -> PathBuf {
        self.actors_dir
            .clone()
            .unwrap_or_else(|| default_actors_dir(&self.home))
    }
}

/// The choice for a save another build wrote (spec §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// `--load-anyway`: load what still fits, report the rest.
    LoadAnyway,
    /// `--fresh-world`: back up the old save, start a new world for the same agent.
    FreshWorld,
}

/// How the explorer world started.
#[derive(Debug)]
pub enum Start {
    /// A new agent and world.
    New,
    /// The save loaded as it was.
    Resumed(LoadReport),
    /// A save from another build, loaded leniently (the report lists what did not fit).
    ResumedChanged(LoadReport),
    /// A fresh world for an existing agent; the old save was backed up.
    Rebuilt,
}

/// The world's session: what each save writes into its header.
#[derive(Resource, Clone, Debug)]
pub struct ExplorerSession {
    /// `agent.json`.
    pub meta: AgentMeta,
    /// The agent folder.
    pub dir: AgentDir,
    /// The scenario document the world was built from, as JSON.
    pub scenario: Value,
    /// The world seed.
    pub seed: u64,
    /// The actor preset the agent's actor was built with (`meta.preset`;
    /// the built-in human for a save from before presets).
    pub preset: ActorPreset,
    /// What the host decided on its own while starting (an `agent.json`
    /// that disagreed with the save, a preset file that was gone), one
    /// line each; also printed to stderr.
    pub notes: Vec<String>,
}

impl ExplorerSession {
    /// The save file.
    pub fn save_path(&self) -> PathBuf {
        self.dir.save(&self.meta.world)
    }

    /// The header for a save taken at `saved` (RFC 3339): [`save_header`]
    /// plus the actor preset (its id, [`HEADER_PRESET`], and the preset
    /// itself, [`HEADER_ACTOR_PRESET`]).
    pub fn header(&self, saved: &str) -> Map<String, Value> {
        let mut header = save_header(&self.meta, &self.scenario, self.seed, saved);
        header.insert(HEADER_PRESET.into(), self.meta.preset.clone().into());
        // A preset always serializes.
        let preset = serde_json::to_value(&self.preset).unwrap_or(Value::Null);
        header.insert(HEADER_ACTOR_PRESET.into(), preset);
        header
    }

    /// The presets the world's scenario may name: this one.
    fn presets(&self) -> ActorPresets {
        ActorPresets::one(&self.meta.preset, self.preset.clone())
    }
}

/// A save's header: `agent`, `world`, `scenario` (the scenario document),
/// `seed` and `saved` (RFC 3339).
pub fn save_header(meta: &AgentMeta, doc: &Value, seed: u64, saved: &str) -> Map<String, Value> {
    let mut header = Map::new();
    header.insert("agent".into(), meta.agent.clone().into());
    header.insert("world".into(), meta.world.clone().into());
    header.insert("scenario".into(), doc.clone());
    header.insert("seed".into(), seed.into());
    header.insert("saved".into(), saved.into());
    header
}

/// Saves the sim state of `world` (its [`ExplorerSession`]'s world) into
/// `dir` atomically, the header's `saved` set to `now`. Call it between
/// ticks.
///
/// # Errors
/// When the world has no session, cannot be saved or the file not written.
pub fn save_now(world: &mut World, dir: &AgentDir, now: &str) -> Result<(), String> {
    let session = world
        .get_resource::<ExplorerSession>()
        .ok_or("no explorer session in this world")?;
    let (path, header) = (dir.save(&session.meta.world), session.header(now));
    write_save(world, &path, header)
}

/// Saves the sim state of `world` with `header` to `path` (atomically).
fn write_save(world: &mut World, path: &Path, header: Map<String, Value>) -> Result<(), String> {
    let mut snap = snapshot::save(world)?;
    snap.header = header;
    snapshot::write_atomic(path, &snap)
        .map_err(|e| format!("cannot write the save {}: {e}", path.display()))
}

/// The prompt kind of a [`Start`].
pub fn prompt_kind(start: &Start) -> PromptKind {
    match start {
        Start::New => PromptKind::Start,
        Start::Resumed(_) => PromptKind::Resume,
        Start::ResumedChanged(_) => PromptKind::ResumeChanged,
        Start::Rebuilt => PromptKind::ResumeRebuilt,
    }
}

/// True for the error of a save that needs `--load-anyway` or `--fresh-world`.
pub fn needs_choice(err: &str) -> bool {
    err.starts_with(NEEDS_CHOICE)
}

/// `t` as RFC 3339 UTC, whole seconds (`2026-10-24T13:05:09Z`). Pure: the
/// caller reads the clock.
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (H. Hinnant's `civil_from_days`;
    // unsigned is enough from the epoch on).
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The time an [`rfc3339`] string names (`2026-10-24T13:05:09Z`, fractional
/// seconds allowed and dropped; UTC `Z` only): `run.json`'s `started`.
pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let t = text.trim().strip_suffix('Z')?;
    let (date, time) = t.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<u64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.split('.').next()?;
    let mut h = time.split(':').map(|p| p.parse::<u64>().ok());
    let (hour, min, sec) = (h.next()??, h.next()??, h.next()??);
    if d.next().is_some()
        || h.next().is_some()
        || !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || min > 59
        || sec > 60
    {
        return None;
    }
    // H. Hinnant's `days_from_civil` (the inverse of `rfc3339`'s).
    let y = if month <= 2 { year - 1 } else { year };
    let (era, yoe) = (y / 400, y % 400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe).checked_sub(719_468)?;
    let secs = days * 86_400 + hour * 3600 + min * 60 + sec;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

/// Builds the explorer world: New creates the agent folder and a world with
/// `seed` (and saves it at tick 0); Resume rebuilds from the save's scenario
/// and seed and loads it. `plugins` adds the host's plugins (window, driver,
/// view; tests pass `|_| {}`). The sim starts paused; `now` dates the
/// folder, the saves and a backup.
///
/// # Errors
/// A message; [`needs_choice`] is true for a save from another build
/// without [`HostArgs::on_mismatch`].
pub fn build_world(
    args: &HostArgs,
    seed: u64,
    now: &str,
    plugins: impl FnOnce(&mut App),
) -> Result<(App, Start, AgentDir), String> {
    build_world_with(args, seed, now, plugins, |_| Ok(()))
}

/// [`build_world`] with a host check run on the built app before anything
/// is written (New) or loaded (Resume): the windowed app refuses a remote
/// port that was taken after it was picked, and leaves no folder behind.
///
/// # Errors
/// As [`build_world`], or `ready`'s message.
pub fn build_world_with(
    args: &HostArgs,
    seed: u64,
    now: &str,
    plugins: impl FnOnce(&mut App),
    ready: impl Fn(&App) -> Result<(), String>,
) -> Result<(App, Start, AgentDir), String> {
    if !args.resume {
        return new_agent(args, seed, now, plugins, &ready, write_save);
    }
    let (dir, mut meta) = read_agent(&args.home, &args.agent)?;
    if args.on_mismatch == Some(Mismatch::FreshWorld) {
        let (id, preset, notes) = fresh_world_preset(args, &dir, &meta)?;
        check_asked(args, &meta.agent, &id)?;
        meta.preset = id;
        return rebuild(dir, meta, preset, notes, seed, now, plugins, &ready);
    }
    let mut opened = open_save(&dir, &meta, plugins, &ready)?;
    check_asked(args, &meta.agent, &opened.preset_id)?;
    let mut notes = Vec::new();
    if opened.preset_id != meta.preset {
        // The save wins: it holds the actor the world was built with.
        notes.push(format!(
            "agent.json names the actor preset `{}`, the save `{}`: the save's is kept \
             (agent.json updated)",
            meta.preset, opened.preset_id
        ));
        meta.preset = opened.preset_id.clone();
        write_meta(&dir, &meta);
    }
    let start = match snapshot::check(opened.app.world_mut(), &opened.snap) {
        Ok(()) => Start::Resumed(load(&mut opened, LoadMode::Strict)?),
        Err(e @ (LoadError::Version { .. } | LoadError::ContentHash { .. })) => {
            if args.on_mismatch != Some(Mismatch::LoadAnyway) {
                return Err(format!(
                    "{NEEDS_CHOICE}: {e}; use --load-anyway (load what still fits) or \
                     --fresh-world (a new world, the diary kept)"
                ));
            }
            Start::ResumedChanged(load(&mut opened, LoadMode::Lenient)?)
        }
        Err(e) => return Err(unreadable(&dir.save(&meta.world), &e)),
    };
    // After the load: a snapshot restores the resource it was saved with,
    // and a save from before the reach sense has none.
    apply_memory(&mut opened.app);
    apply_senses(&mut opened.app, &meta.agent, &opened.preset)?;
    let session = ExplorerSession {
        meta,
        dir: dir.clone(),
        scenario: opened.scenario,
        seed: opened.seed,
        preset: opened.preset,
        notes,
    };
    Ok((begin(opened.app, session), start, dir))
}

/// A resume keeps the world's actor preset: `--preset` naming another one is
/// an error.
fn check_asked(args: &HostArgs, agent: &str, id: &str) -> Result<(), String> {
    match args.preset.as_deref() {
        Some(asked) if asked != id => Err(format!(
            "`{agent}` plays the actor preset `{id}`; a resumed world keeps it (asked for `{asked}`)"
        )),
        _ => Ok(()),
    }
}

/// Writes `agent.json` again (atomically); a failure is printed, not fatal.
fn write_meta(dir: &AgentDir, meta: &AgentMeta) {
    let written = serde_json::to_string_pretty(meta)
        .map_err(|e| e.to_string())
        .and_then(|t| write_atomic(&dir.meta(), &t).map_err(|e| e.to_string()));
    if let Err(e) = written {
        eprintln!("explorer: cannot update {}: {e}", dir.meta().display());
    }
}

/// The actor preset a save was built with: its id (the header's
/// [`HEADER_PRESET`], else the scenario actor `agent`'s `preset`, else
/// `human`: a save from before presets) and its copy (the header's
/// [`HEADER_ACTOR_PRESET`], else the built-in of that id).
fn saved_preset(header: &Map<String, Value>, agent: &str) -> Result<(String, ActorPreset), String> {
    let from_doc = || {
        header
            .get("scenario")?
            .get("actors")?
            .as_array()?
            .iter()
            .find(|a| a.get("name").and_then(Value::as_str) == Some(agent))?
            .get("preset")?
            .as_str()
            .map(str::to_owned)
    };
    let id = header
        .get(HEADER_PRESET)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(from_doc)
        .unwrap_or_else(|| HUMAN_PRESET.to_owned());
    let preset = match header.get(HEADER_ACTOR_PRESET) {
        Some(v) => serde_json::from_value::<ActorPreset>(v.clone())
            .map_err(|e| format!("its actor preset: {e}"))?,
        None => builtin_preset(&id)
            .ok_or_else(|| format!("it names actor preset `{id}` but holds no copy of it"))?,
    };
    Ok((id, preset))
}

/// `--fresh-world`'s actor preset: the id the old save names (else
/// `agent.json`'s), read again from the actors folder (an edited file
/// applies); a file that is gone falls back to the old save's copy, with a
/// note.
fn fresh_world_preset(
    args: &HostArgs,
    dir: &AgentDir,
    meta: &AgentMeta,
) -> Result<(String, ActorPreset, Vec<String>), String> {
    let saved = snapshot::read(&dir.save(&meta.world))
        .ok()
        .and_then(|snap| saved_preset(&snap.header, &meta.agent).ok());
    let id = saved
        .as_ref()
        .map_or_else(|| meta.preset.clone(), |(id, _)| id.clone());
    let actors = args.actors_dir();
    let file = actors.join(format!("{id}.json"));
    if let (false, Some((_, copy))) = (file.is_file(), saved) {
        let note = format!(
            "the actor preset file {} is gone: the fresh world uses the old save's copy of `{id}`",
            file.display()
        );
        return Ok((id, copy, vec![note]));
    }
    let preset = find_preset(&actors, &id)?;
    Ok((id, preset, Vec::new()))
}

/// `world/reset` of a running explorer world (the detached host): the
/// world `session` was built from (its scenario, the agent's actor), new
/// for `seed`, with the explorer's memory settings and the session (its
/// seed now `seed`), paused like [`build_world`]'s. Nothing is written
/// here: the host keeps the old save first (`AgentDir::backup_save`, as
/// `--fresh-world` does) before the reset world is saved over it, and
/// re-inserts the notes resources ([`insert_notes`]).
///
/// # Errors
/// When the scenario no longer builds.
pub fn reset_world(
    session: &ExplorerSession,
    seed: u64,
    plugins: impl FnOnce(&mut App),
) -> Result<App, String> {
    let doc = ScenarioDoc::from_value(session.scenario.clone())?;
    let mut app = build(&doc, seed, session.presets(), plugins)?;
    apply_memory(&mut app);
    apply_senses(&mut app, &session.meta.agent, &session.preset)?;
    let session = ExplorerSession {
        seed,
        ..session.clone()
    };
    Ok(begin(app, session))
}

/// How many ports past the first an explorer world tries
/// (`first..=first + 19`, [`pick_port`]).
pub const PORT_SPAN: u16 = 19;

/// The first port `p` in `first..=last` such that both `p` and `p + 1` (the
/// window's render-app BRP server) can be bound on 127.0.0.1, so a detached
/// world's port can later move into the window. The probe listeners are
/// dropped before the caller binds, so another process can still take the
/// port in between (the caller checks its own bind).
pub fn pick_port(first: u16, last: u16) -> Option<u16> {
    let free = |p: u16| std::net::TcpListener::bind(("127.0.0.1", p));
    (first..=last).find(|&p| {
        let Some(next) = p.checked_add(1) else {
            return false;
        };
        // Held together, so the two probes cannot be the same listener.
        free(p).is_ok_and(|_held| free(next).is_ok())
    })
}

/// How long a handover waits for its port and for the old host to exit
/// (`actor_testbed --exact-port`, `npcctl serve --wait-port`; spec §8).
pub const HANDOVER_WAIT: Duration = Duration::from_secs(10);

/// How often a handover tries the port again.
const PORT_RETRY: Duration = Duration::from_millis(100);
/// How often a handover asks whether the old host is gone.
const HOST_RETRY: Duration = Duration::from_millis(200);

/// Whether `port` can be bound on 127.0.0.1 before `expired()` says the
/// caller's budget is spent, trying every 100 ms (the probe is dropped
/// again; the caller binds next). Never another port: the agent's settings
/// pin the URL. The caller owns the clock (this crate reads none).
pub fn wait_for_port_until(port: u16, mut expired: impl FnMut() -> bool) -> bool {
    loop {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if expired() {
            return false;
        }
        std::thread::sleep(PORT_RETRY);
    }
}

/// The world running in `dir` in another live process (its `run.json`):
/// one owner per world (spec §8), so this process must not start it too.
pub fn running_elsewhere(dir: &AgentDir) -> Option<RunFile> {
    RunFile::read_live(dir).filter(|run| run.pid != std::process::id())
}

/// What a handover found when its wait ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handover {
    /// The port is free and no other process runs the world.
    Ready,
    /// The port stayed taken.
    PortInUse,
    /// The port came free but `run.json` still names this live process.
    Running(RunFile),
}

/// A handover onto `port` (window to detached, or back): within one budget
/// (until `expired()`, the caller's clock: [`HANDOVER_WAIT`]), waits for
/// `port` to come free and then for the old host to be gone (its `run.json`
/// removed or its pid exited); it releases the port a moment before it
/// exits.
pub fn wait_for_handover_until(
    dir: &AgentDir,
    port: u16,
    mut expired: impl FnMut() -> bool,
) -> Handover {
    if !wait_for_port_until(port, &mut expired) {
        return Handover::PortInUse;
    }
    loop {
        let Some(run) = running_elsewhere(dir) else {
            return Handover::Ready;
        };
        if expired() {
            return Handover::Running(run);
        }
        std::thread::sleep(HOST_RETRY);
    }
}

/// The `npcctl serve` arguments that resume `agent`'s world detached on
/// exactly `port`, waiting for the port and the old host first
/// (`--wait-port`): the window's "keep it running detached" and F9, and the
/// menu's restart after a failed move into the window.
pub fn detached_resume_args(home: &Path, agent: &str, port: u16) -> Vec<String> {
    [
        "serve",
        "--explorer-home",
        &home.display().to_string(),
        "--agent",
        agent,
        "--resume",
        "--port",
        &port.to_string(),
        "--wait-port",
    ]
    .map(str::to_owned)
    .to_vec()
}

/// The line the window prints on stdout when it moved its world to
/// detached (`{"detached":{"pid","port","agent","world"}}`), so the menu
/// that started the window tracks the new host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetachedLine {
    /// The detached host (`npcctl serve`).
    pub pid: u32,
    /// Its port (the window's).
    pub port: u16,
    /// The agent's name.
    pub agent: String,
    /// The world's name.
    pub world: String,
}

impl DetachedLine {
    /// The stdout line.
    pub fn line(&self) -> String {
        serde_json::json!({ "detached": self }).to_string()
    }

    /// The line read back; `None` for any other line.
    pub fn parse(line: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Wrapped {
            detached: DetachedLine,
        }
        let line = line.trim();
        if !line.starts_with("{\"detached\"") {
            return None;
        }
        serde_json::from_str::<Wrapped>(line)
            .ok()
            .map(|w| w.detached)
    }
}

/// Makes `cmd` a detached world that outlives the process starting it:
/// on Windows its own hidden console (closing the starter's console does
/// not end it) in a new process group (the starter's Ctrl+C does not reach
/// it); on Unix its own process group.
pub fn detach_command(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(any(windows, unix)))]
    let _ = cmd;
}

/// Calls `method` (no params) on the explorer world served on
/// `127.0.0.1:port` (the menu's Detached screen: `session/info`,
/// `session/stop`), waiting at most `timeout` for the answer.
///
/// # Errors
/// The transport error or the method's error, one line.
pub fn session_call(port: u16, method: &str, timeout: Duration) -> Result<Value, String> {
    remote_call(port, method, Value::Object(Map::new()), timeout)
}

/// Calls `method` with `params` on the JSON-RPC server on
/// `127.0.0.1:port` (a detached world, or a game window's BRP: the menu's
/// `app/quit`), waiting at most `timeout` for the answer.
///
/// # Errors
/// The transport error or the method's error, one line.
pub fn remote_call(
    port: u16,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    remote_call_checked(port, method, params, timeout).map_err(|e| match e {
        RemoteError::Refused(e) | RemoteError::NoAnswer(e) => e,
    })
}

/// Why a [`remote_call_checked`] failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteError {
    /// The server answered with an error (`session/stop`: the save failed).
    Refused(String),
    /// No answer: nothing listens on the port, or the connection failed.
    NoAnswer(String),
}

/// [`remote_call`] that tells a refusal from no answer.
///
/// # Errors
/// [`RemoteError::Refused`] with the method's error message, or
/// [`RemoteError::NoAnswer`] with the transport error.
pub fn remote_call_checked(
    port: u16,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, RemoteError> {
    use npc_framework::agent_io::http::{ClientError, RpcClient};
    let mut client =
        RpcClient::new(&format!("http://127.0.0.1:{port}")).map_err(RemoteError::NoAnswer)?;
    client.timeout = timeout;
    client.call(method, params).map_err(|e| match e {
        ClientError::Rpc(e) => RemoteError::Refused(e.message),
        ClientError::Transport(e) => RemoteError::NoAnswer(e),
    })
}

/// The newest prompt file of the agent folder (`prompts/<kind>.md` by
/// modification time): the one the running world wrote last.
pub fn latest_prompt(dir: &AgentDir) -> Option<PathBuf> {
    [
        PromptKind::Start,
        PromptKind::Resume,
        PromptKind::ResumeChanged,
        PromptKind::ResumeRebuilt,
    ]
    .into_iter()
    .map(|k| dir.prompt(k))
    .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
    .max_by_key(|(t, _)| *t)
    .map(|(_, p)| p)
}

/// The prompt a prompt file holds: the file without its generated
/// frontmatter (the text the game copies).
pub fn prompt_text(file: &str) -> &str {
    let file = file.trim_start_matches('\u{feff}');
    file.strip_prefix("---\n")
        .or_else(|| file.strip_prefix("---\r\n"))
        .and_then(|rest| {
            let end = rest
                .find("\n---\n")
                .map(|i| i + 5)
                .or_else(|| rest.find("\n---\r\n").map(|i| i + 6))?;
            Some(&rest[end..])
        })
        .unwrap_or(file)
}

/// The host's check on a built app ([`build_world_with`]).
type Ready<'a> = &'a dyn Fn(&App) -> Result<(), String>;

/// How the tick-0 save is written ([`write_save`]; a test injects a failure).
type SaveFn = fn(&mut World, &Path, Map<String, Value>) -> Result<(), String>;

/// New: the folder must not exist yet; the world is built (and the name
/// checked) before the folder is created, and a failed tick-0 `save` removes
/// the folder again, so a failure leaves nothing behind and the name can be
/// used again.
fn new_agent(
    args: &HostArgs,
    seed: u64,
    now: &str,
    plugins: impl FnOnce(&mut App),
    ready: Ready,
    save: SaveFn,
) -> Result<(App, Start, AgentDir), String> {
    let world = args
        .world
        .as_deref()
        .ok_or("a new agent needs a world name")?;
    let folder = agent_dir(&args.home, &args.agent);
    if folder.dir.exists() {
        return Err(format!(
            "an agent folder already exists: {} (resume it instead)",
            folder.dir.display()
        ));
    }
    let id = args.preset.as_deref().unwrap_or(HUMAN_PRESET);
    let preset = find_preset(&args.actors_dir(), id)?;
    let (mut app, scenario) = fresh_world(&args.agent, seed, (id, &preset), plugins, ready)?;
    let dir = create_agent_as(&args.home, &args.agent, world, now, id)?;
    let session = ExplorerSession {
        meta: AgentMeta {
            agent: args.agent.clone(),
            world: world.to_owned(),
            created: now.to_owned(),
            preset: id.to_owned(),
        },
        dir: dir.clone(),
        scenario,
        seed,
        preset,
        notes: Vec::new(),
    };
    if let Err(e) = save(app.world_mut(), &session.save_path(), session.header(now)) {
        // `create_agent` refuses an existing folder, so this one is ours.
        return Err(match std::fs::remove_dir_all(&dir.dir) {
            Ok(()) => e,
            Err(rm) => format!(
                "{e}; the new agent folder {} could not be removed either: {rm}",
                dir.dir.display()
            ),
        });
    }
    Ok((begin(app, session), Start::New, dir))
}

/// Fresh world, keep the diary: the old save (if any) is renamed to
/// `<world>.world.<date>.json`, and a new world is built and saved.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    dir: AgentDir,
    meta: AgentMeta,
    preset: ActorPreset,
    notes: Vec<String>,
    seed: u64,
    now: &str,
    plugins: impl FnOnce(&mut App),
    ready: Ready,
) -> Result<(App, Start, AgentDir), String> {
    let chosen = (meta.preset.as_str(), &preset);
    let (mut app, scenario) = fresh_world(&meta.agent, seed, chosen, plugins, ready)?;
    let save = dir.save(&meta.world);
    if save.exists() {
        dir.backup_save(&meta.world, now)
            .map_err(|e| format!("cannot back up the save {}: {e}", save.display()))?;
    }
    write_meta(&dir, &meta);
    let session = ExplorerSession {
        meta,
        dir: dir.clone(),
        scenario,
        seed,
        preset,
        notes,
    };
    write_save(app.world_mut(), &session.save_path(), session.header(now))?;
    Ok((begin(app, session), Start::Rebuilt, dir))
}

/// Explorer worlds remember longer than the shared scenario default: sets
/// [`EXPLORER_FORGET_AFTER`] and [`EXPLORER_MEMORY_CAPACITY`], keeping the
/// rest of the world's [`PerceptionConfig`].
fn apply_memory(app: &mut App) {
    let mut config = app
        .world()
        .get_resource::<PerceptionConfig>()
        .copied()
        .unwrap_or_default();
    config.forget_after_ticks = EXPLORER_FORGET_AFTER;
    config.memory_capacity = EXPLORER_MEMORY_CAPACITY;
    app.insert_resource(config);
}

/// Explorer worlds sense what is in reach (spec F3, proposals 007 and 011):
/// the actor named `agent` gets [`ReachSense`] (its actor preset's `reach`;
/// none at 0) and [`RevealsFog`] (fog of war, spec G8: its explored cells
/// and last-seen statics, snapshot-only; every preset keeps it), and
/// every chest without [`Perceivable`] becomes perceivable
/// ([`kinds::CONTAINER`]), so it can be sensed and remembered. Only here:
/// the shared scenarios keep their hashes. Idempotent (a resumed save that
/// already has both is unchanged).
///
/// The chests become perceivable to **every** NPC of an explorer world,
/// not only to the agent's actor (any unit's vision, proximity or reach
/// can sense them). Only the chests present when the world is built,
/// resumed or reset get it: a chest spawned at runtime stays imperceptible
/// until the next resume or reset.
///
/// # Errors
/// When the world has no actor named `agent` (it would silently get no
/// reach sense otherwise).
fn apply_senses(app: &mut App, agent: &str, preset: &ActorPreset) -> Result<(), String> {
    let world = app.world_mut();
    let e = world
        .get_resource::<TestbedIds>()
        .and_then(|ids| ids.get(agent))
        .and_then(|id| world.resource::<npc_framework::prelude::SimIndex>().get(id))
        .ok_or_else(|| format!("the explorer world has no actor named `{agent}`"))?;
    // Fog of war (plan Task 9, spec G8): the played actor reveals fog and
    // keeps its explored ground in the save, whatever its actor preset.
    // Inserting it again keeps the explored cells a resumed save brought.
    let mut actor = world.entity_mut(e);
    actor.insert(RevealsFog);
    match preset.reach_mm() {
        Some(radius_mm) => {
            actor.insert(ReachSense { radius_mm });
        }
        None => {
            actor.remove::<ReachSense>();
        }
    }
    let mut chests = world.query_filtered::<(Entity, &Prop), Without<Perceivable>>();
    let chests: Vec<Entity> = chests
        .iter(world)
        .filter(|(_, p)| p.kind == PropKind::Chest)
        .map(|(e, _)| e)
        .collect();
    // Order does not matter: every one gets the same component.
    for e in chests {
        world
            .entity_mut(e)
            .insert(Perceivable::new(kinds::CONTAINER));
    }
    Ok(())
}

/// Pauses the sim (the host unpauses it) and stores the session (its
/// notes printed to stderr).
fn begin(mut app: App, session: ExplorerSession) -> App {
    for note in &session.notes {
        eprintln!("explorer: {note}");
    }
    let world = app.world_mut();
    match world.get_resource_mut::<SimControl>() {
        Some(mut control) => control.paused = true,
        None => world.insert_resource(SimControl {
            paused: true,
            ..Default::default()
        }),
    }
    world.insert_resource(session);
    app
}

/// `<home>/<slug(agent)>`.
fn agent_dir(home: &Path, agent: &str) -> AgentDir {
    AgentDir {
        dir: home.join(slug(agent)),
    }
}

/// The agent folder and its `agent.json` (a leading BOM is tolerated).
fn read_agent(home: &Path, agent: &str) -> Result<(AgentDir, AgentMeta), String> {
    let dir = agent_dir(home, agent);
    if !dir.dir.is_dir() {
        return Err(format!(
            "no agent folder {} (start a new agent first)",
            dir.dir.display()
        ));
    }
    let path = dir.meta();
    let bad = |e: &dyn std::fmt::Display| format!("unreadable {}: {e}", path.display());
    let text = std::fs::read_to_string(&path).map_err(|e| bad(&e))?;
    let meta = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| bad(&e))?;
    Ok((dir, meta))
}

/// The `explorer` scenario with its actor named `agent` and made from
/// actor preset `chosen` (`(id, preset)`, validated here first), built for
/// `seed`: the app and the scenario as JSON (the actor's `preset` set).
fn fresh_world(
    agent: &str,
    seed: u64,
    chosen: (&str, &ActorPreset),
    plugins: impl FnOnce(&mut App),
    ready: Ready,
) -> Result<(App, Value), String> {
    let (id, preset) = chosen;
    let mut doc = crate::scenario::scenario_doc(SCENARIO)?;
    // The world's own names (`gate`, `tree_0`, ..) share the namespace.
    let bare = ScenarioDoc {
        actors: Vec::new(),
        controllers: Vec::new(),
        ..doc.clone()
    };
    let probe = crate::scenario::load_with(&bare, 1, |_| {})?;
    if probe
        .app
        .world()
        .resource::<TestbedIds>()
        .get(agent)
        .is_some()
    {
        return Err(format!(
            "`{agent}` names a thing in the explorer world: pick another agent name"
        ));
    }
    validate(preset, probe.app.world())
        .map_err(|reasons| format!("actor preset `{id}` is invalid: {}", reasons.join("; ")))?;
    let actor = doc
        .actors
        .iter_mut()
        .find(|a| a.extra.get("name").and_then(Value::as_str) == Some(ACTOR))
        .ok_or_else(|| format!("the `{SCENARIO}` scenario has no actor named `{ACTOR}`"))?;
    actor
        .extra
        .insert("name".into(), Value::String(agent.to_owned()));
    actor
        .extra
        .insert("preset".into(), Value::String(id.to_owned()));
    let presets = ActorPresets::one(id, preset.clone());
    let mut app = build(&doc, seed, presets, plugins)?;
    ready(&app)?;
    apply_memory(&mut app);
    apply_senses(&mut app, agent, preset)?;
    let json = serde_json::to_value(&doc).map_err(|e| format!("scenario to JSON: {e}"))?;
    Ok((app, json))
}

/// `doc` built for `seed` with its controllers attached (content frozen),
/// its actors' presets from `presets` (or the built-ins).
fn build(
    doc: &ScenarioDoc,
    seed: u64,
    presets: ActorPresets,
    plugins: impl FnOnce(&mut App),
) -> Result<App, String> {
    let mut loaded = crate::scenario::load_with(doc, seed, |app| {
        plugins(app);
        app.insert_resource(presets);
    })?;
    attach_controllers(doc, &mut loaded)?;
    loaded.app.insert_resource(ScenarioInfo {
        name: doc.name.clone().unwrap_or_else(|| SCENARIO.into()),
        villagers: u32::try_from(loaded.actors.len()).unwrap_or(u32::MAX),
    });
    Ok(loaded.app)
}

/// A save read and its world rebuilt from the header, not loaded yet.
struct Opened {
    app: App,
    snap: WorldSnapshot,
    scenario: Value,
    seed: u64,
    /// The save's actor preset id and copy ([`saved_preset`]).
    preset_id: String,
    preset: ActorPreset,
}

fn unreadable(path: &Path, e: &dyn std::fmt::Display) -> String {
    format!("unreadable save {}: {e}", path.display())
}

/// Reads the save and builds its scenario and seed (`header.scenario`,
/// `header.seed`). Every error but `ready`'s starts `unreadable`.
fn open_save(
    dir: &AgentDir,
    meta: &AgentMeta,
    plugins: impl FnOnce(&mut App),
    ready: Ready,
) -> Result<Opened, String> {
    let path = dir.save(&meta.world);
    let snap = snapshot::read(&path)?;
    let scenario = snap
        .header
        .get("scenario")
        .cloned()
        .ok_or_else(|| unreadable(&path, &"the header has no scenario"))?;
    let seed = snap
        .header
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or_else(|| unreadable(&path, &"the header has no seed"))?;
    let (preset_id, preset) =
        saved_preset(&snap.header, &meta.agent).map_err(|e| unreadable(&path, &e))?;
    let doc = ScenarioDoc::from_value(scenario.clone()).map_err(|e| unreadable(&path, &e))?;
    let presets = ActorPresets::one(&preset_id, preset.clone());
    let app = build(&doc, seed, presets, plugins).map_err(|e| unreadable(&path, &e))?;
    ready(&app)?;
    Ok(Opened {
        app,
        snap,
        scenario,
        seed,
        preset_id,
        preset,
    })
}

fn load(opened: &mut Opened, mode: LoadMode) -> Result<LoadReport, String> {
    let world = opened.app.world_mut();
    let mut report = snapshot::load(world, &opened.snap, mode)
        .map_err(|e| format!("cannot load the save: {e}"))?;
    // The fog of war's explored ground saved for another grid started over.
    if let Some(FogResets(ids)) = world.remove_resource::<FogResets>() {
        report.skipped.push(fog_reset_note(&ids));
    }
    Ok(report)
}

/// The [`LoadReport::skipped`] line of explored records a load started over.
pub fn fog_reset_note(ids: &[SimId]) -> String {
    let ids: Vec<String> = ids.iter().map(|id| format!("#{}", id.0)).collect();
    format!(
        "explored_cells of {}: saved for another grid, the explored ground starts over",
        ids.join(", ")
    )
}

/// The `--check-save` result: `Ok` when the save fits this build, else
/// `(kind, error)` with `kind` one of `content_hash`, `version`,
/// `unreadable`. [`check_json`] prints it.
pub type CheckJson = Result<(), (&'static str, String)>;

/// `--check-save`: whether the agent's save fits this build. Builds
/// headless; writes nothing.
pub fn check_save(home: &Path, agent: &str) -> CheckJson {
    let as_unreadable = |e: String| ("unreadable", e);
    let (dir, meta) = read_agent(home, agent).map_err(as_unreadable)?;
    let mut opened = open_save(&dir, &meta, |_| {}, &|_| Ok(())).map_err(as_unreadable)?;
    snapshot::check(opened.app.world_mut(), &opened.snap).map_err(|e| {
        let kind = match e {
            LoadError::Version { .. } => "version",
            LoadError::ContentHash { .. } => "content_hash",
            LoadError::Bad(_) => "unreadable",
        };
        (kind, e.to_string())
    })
}

/// The `--check-save` output line: `{"ok":true}` or
/// `{"ok":false,"error":"..","kind":".."}`.
pub fn check_json(result: &CheckJson) -> String {
    match result {
        Ok(()) => r#"{"ok":true}"#.to_owned(),
        Err((kind, error)) => format!(
            r#"{{"ok":false,"error":{},"kind":{}}}"#,
            Value::String(error.clone()),
            Value::String((*kind).to_owned())
        ),
    }
}

/// What [`write_session_files`] wrote for this start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFiles {
    /// Which opening the prompt has.
    pub kind: PromptKind,
    /// The agent's unit.
    pub unit: SimId,
    /// The rendered prompt, as written to `prompts/<kind>.md`.
    pub text: String,
    /// That file.
    pub path: PathBuf,
    /// Why the workspace could not be written; `None` when it was (then
    /// `path` holds `text`). The world still starts: the host shows the
    /// prompt and this message instead.
    pub write_error: Option<String>,
}

/// Writes the agent folder's workspace for this start (`AGENTS.md`,
/// `CLAUDE.md`, `prompts/<kind>.md`, the Claude Code settings,
/// `.mcp.json`: [`write_workspace`]; on every start and resume, never the
/// notes or the save) for the game at `url`. `npcctl` is the CLI the
/// prompt names (`npcctl_missing`: not built yet). Call it once the URL is
/// known.
///
/// # Errors
/// A message when `world` has no session or the agent's unit is missing; a
/// folder that cannot be written is [`SessionFiles::write_error`].
pub fn write_session_files(
    world: &World,
    start: &Start,
    dir: &AgentDir,
    url: &str,
    npcctl: PathBuf,
    npcctl_missing: bool,
) -> Result<SessionFiles, String> {
    let session = world
        .get_resource::<ExplorerSession>()
        .ok_or("no explorer session in this world")?;
    let unit = world
        .resource::<TestbedIds>()
        .get(&session.meta.agent)
        .ok_or_else(|| format!("no unit named {}", session.meta.agent))?;
    let kind = prompt_kind(start);
    let vars = WorkspaceVars {
        prompt: PromptVars {
            agent: session.meta.agent.clone(),
            world: session.meta.world.clone(),
            url: url.to_owned(),
            unit: unit.0,
            npcctl,
            dir: dir.dir.clone(),
            npcctl_missing,
        },
        actor: ActorSummary::from_preset(&session.preset),
    };
    let text = render_prompt(kind, &vars.prompt);
    let write_error = write_workspace(dir, kind, &vars)
        .map_err(|e| format!("cannot write the agent folder {}: {e}", dir.dir.display()))
        .err();
    Ok(SessionFiles {
        kind,
        unit,
        path: dir.prompt(kind),
        text,
        write_error,
    })
}

/// The explorer's notes live in its agent folder (one agent, one actor),
/// dated by `clock` (the host's wall clock, RFC 3339 UTC, [`rfc3339`]):
/// inserts [`NotesDirs`] mapping `unit` to the folder and [`NotesClock`],
/// and tells [`NotesStores`] (created if missing) the folder's owner, the
/// agent and world names the entry files carry (from the world's
/// [`ExplorerSession`], else the folder's `agent.json`; the generic store
/// never reads content files). The notes methods and the F7 editor share
/// that one `NotesStores`.
pub fn insert_notes(world: &mut World, unit: SimId, dir: &AgentDir, clock: fn() -> String) {
    let meta = world
        .get_resource::<ExplorerSession>()
        .map(|s| s.meta.clone())
        .or_else(|| serde_json::from_str::<AgentMeta>(&read_lossy(&dir.meta())).ok());
    let owner = meta.map_or_else(NotesOwner::default, |m| NotesOwner {
        agent: m.agent,
        world: m.world,
    });
    world
        .get_resource_or_init::<NotesStores>()
        .set_owner(&dir.dir, owner);
    world.insert_resource(NotesDirs(BTreeMap::from([(unit, dir.dir.clone())])));
    world.insert_resource(NotesClock(clock));
}

/// Where a running world is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    /// In the game window.
    Window,
    /// Without a window (`npcctl serve`).
    Detached,
}

/// `run.json` in the agent folder: the world running there, so another
/// menu can find, stop or open it (spec §8). Owned, never generated: the
/// running game writes it on start and removes it on exit; a stale one (a
/// dead pid) is ignored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFile {
    /// The running process.
    pub pid: u32,
    /// Its remote port.
    pub port: u16,
    /// Window or detached.
    pub mode: RunMode,
    /// When it started (RFC 3339, from the caller).
    pub started: String,
    /// The agent's name.
    pub agent: String,
    /// The world's name.
    pub world: String,
}

impl RunFile {
    /// `<agent folder>/run.json`.
    pub fn path(dir: &AgentDir) -> PathBuf {
        dir.dir.join(RUN_FILE)
    }

    /// Writes `run.json` atomically (a temp file, then a rename), replacing
    /// any earlier one.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn write(&self, dir: &AgentDir) -> std::io::Result<()> {
        let mut text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        text.push('\n');
        write_atomic(&Self::path(dir), &text)
    }

    /// Removes `run.json` when it names this run (its pid); one another
    /// process wrote since is left alone. No file is fine.
    ///
    /// # Errors
    /// When the file is ours and cannot be removed.
    pub fn remove(&self, dir: &AgentDir) -> std::io::Result<()> {
        // Read, then remove, without a lock: a run that writes its own
        // run.json between the two loses it. The window is a few
        // microseconds, and handovers are stop -> save -> start (spec §8).
        if Self::read(dir).is_none_or(|on_disk| on_disk.pid != self.pid) {
            return Ok(());
        }
        match std::fs::remove_file(Self::path(dir)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// The world running in `dir`: `None` without a readable `run.json` or
    /// when its process is gone ([`pid_alive`]). Never changes the file.
    pub fn read_live(dir: &AgentDir) -> Option<RunFile> {
        Self::read(dir).filter(|run| pid_alive(run.pid))
    }

    /// `run.json` as written, live or not.
    pub fn read_any(dir: &AgentDir) -> Option<RunFile> {
        Self::read(dir)
    }

    fn read(dir: &AgentDir) -> Option<RunFile> {
        let text = std::fs::read_to_string(Self::path(dir)).ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    }
}

/// Whether process `pid` is running: this process is; otherwise the OS is
/// asked without any new dependency (`tasklist` on Windows, `kill -0` on
/// Unix). When the check cannot run or its answer is unclear, the process
/// counts as alive, so a running world is never taken for a stale one.
///
/// Known limits: a pid the OS has reused for another process looks alive;
/// on Unix, another user's process (`kill` fails with EPERM) looks dead,
/// which is fine because agent folders belong to one user.
pub fn pid_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    if pid == 0 {
        return false;
    }
    os_pid_alive(pid).unwrap_or(true)
}

/// Whether process `pid` is clearly gone: only a confident answer from the
/// OS (the opposite of [`pid_alive`]'s "unclear counts as alive"), so a
/// stale `run.json` may be deleted on it.
pub fn pid_dead(pid: u32) -> bool {
    pid != std::process::id() && (pid == 0 || os_pid_alive(pid) == Some(false))
}

#[cfg(windows)]
fn os_pid_alive(pid: u32) -> Option<bool> {
    /// tasklist's no-match line for a pid that never exists, as this
    /// machine's language words it; kept once read.
    static NO_MATCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    /// Windows hands out only multiples of 4 as pids.
    const NEVER_A_PID: u32 = 3;
    let out = tasklist(pid)?;
    let no_match = match NO_MATCH.get() {
        Some(m) => Some(m.as_str()),
        None => tasklist(NEVER_A_PID).map(|m| NO_MATCH.get_or_init(|| m).as_str()),
    };
    tasklist_says(&out, pid, no_match)
}

/// `tasklist /FI "PID eq <pid>" /NH /FO CSV`'s stdout, `None` when it
/// cannot run or fails.
#[cfg(windows)]
fn tasklist(pid: u32) -> Option<String> {
    use std::os::windows::process::CommandExt;
    /// No console window flashes up when a windowed app (the menu) asks.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reads `tasklist` output for `pid`: `Some(true)` for a CSV row whose pid
/// field is `pid`; `Some(false)` only for a clear no-match, the same text
/// tasklist printed for a pid that never exists (`no_match`; its wording is
/// localized, e.g. `INFO: No tasks are running ..` or `INFORMATION: Es
/// werden keine Aufgaben ..`); `None` (unknown) for anything else. File
/// names cannot contain `"`, so the fields split exactly on `","` even when
/// the image name has a comma.
#[cfg(any(windows, test))]
fn tasklist_says(stdout: &str, pid: u32, no_match: Option<&str>) -> Option<bool> {
    let want = pid.to_string();
    let is_row_of = |line: &str, want: Option<&str>| {
        let row = line
            .trim()
            .strip_prefix('"')
            .and_then(|l| l.strip_suffix('"'));
        row.is_some_and(|r| want.is_none_or(|w| r.split("\",\"").nth(1) == Some(w)))
    };
    if stdout.lines().any(|l| is_row_of(l, Some(&want))) {
        return Some(true);
    }
    // A reference that is empty or a process row says nothing about `pid`.
    let no_match = no_match
        .map(str::trim)
        .filter(|m| !m.is_empty() && !m.lines().any(|l| is_row_of(l, None)))?;
    (stdout.trim() == no_match).then_some(false)
}

#[cfg(unix)]
fn os_pid_alive(pid: u32) -> Option<bool> {
    // `kill -0 -N` would ask about a process group: such pids do not exist.
    if i32::try_from(pid).is_err() {
        return Some(false);
    }
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|s| s.success())
}

#[cfg(not(any(windows, unix)))]
fn os_pid_alive(_pid: u32) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_back() {
        for secs in [0, 86_399, 951_782_400, 1_793_019_909, 4_102_444_799] {
            let t = UNIX_EPOCH + Duration::from_secs(secs);
            assert_eq!(parse_rfc3339(&rfc3339(t)), Some(t), "{}", rfc3339(t));
        }
        assert_eq!(
            parse_rfc3339("2026-10-26T14:00:56.1234567Z"),
            parse_rfc3339("2026-10-26T14:00:56Z")
        );
        for bad in [
            "",
            "2026-10-26",
            "2026-13-01T00:00:00Z",
            "2026-10-26T14:00:56+02:00",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }

    fn failing_save(_: &mut World, path: &Path, _: Map<String, Value>) -> Result<(), String> {
        Err(format!(
            "cannot write the save {}: disk full",
            path.display()
        ))
    }

    /// Moved from actor_testbed: the tick-0 save's failure path needs the
    /// private [`new_agent`].
    #[test]
    fn a_failed_first_save_removes_the_new_agent_folder() {
        let home = std::env::temp_dir().join(format!(
            "npc-explorer-host-save-fails-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let (seed, now) = (7, "2026-10-25T12:00:00Z");
        let args = HostArgs {
            home: home.clone(),
            agent: "Wren".into(),
            world: Some("W".into()),
            resume: false,
            on_mismatch: None,
            preset: None,
            actors_dir: Some(home.join("actors")),
        };
        let Err(e) = new_agent(&args, seed, now, |_| {}, &|_| Ok(()), failing_save) else {
            panic!("a New without its tick-0 save started");
        };
        assert!(e.contains("disk full"), "{e}");
        assert!(!home.join("wren").exists(), "the folder was removed");
        // The name can be used again.
        let (_, start, dir) = build_world(&args, seed, now, |_| {}).unwrap();
        assert!(matches!(start, Start::New));
        assert!(dir.save("W").is_file());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Review fix 5: a missing explorer actor is an error, not a world
    /// whose actor silently has no reach.
    #[test]
    fn apply_senses_needs_the_actor() {
        let mut app = crate::scenario::try_new_sim_app(1, SCENARIO, 0).unwrap();
        let human = crate::actors::human_preset();
        let err = apply_senses(&mut app, "Nobody", &human).unwrap_err();
        assert!(err.contains("no actor named `Nobody`"), "{err}");
        apply_senses(&mut app, ACTOR, &human).unwrap();
        let id = app.world().resource::<TestbedIds>().get(ACTOR).unwrap();
        let e = app
            .world()
            .resource::<npc_framework::prelude::SimIndex>()
            .get(id)
            .unwrap();
        assert!(app.world().get::<ReachSense>(e).is_some());
    }

    /// `world/reset` of a detached explorer world: the New world of that
    /// seed, with the explorer's memory settings and its session.
    #[test]
    fn reset_world_is_the_new_world_of_its_seed() {
        use npc_framework::prelude::{run_ticks, state_hash};
        let home =
            std::env::temp_dir().join(format!("npc-explorer-host-reset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let args = HostArgs {
            home: home.clone(),
            agent: "Wren".into(),
            world: Some("W".into()),
            resume: false,
            on_mismatch: None,
            preset: None,
            actors_dir: Some(home.join("actors")),
        };
        let (mut app, _, _) = build_world(&args, 7, "2026-10-25T12:00:00Z", |_| {}).unwrap();
        let session = app.world().resource::<ExplorerSession>().clone();
        let new_hash = state_hash(app.world_mut());
        run_ticks(app.world_mut(), 50);
        let mut reset = reset_world(&session, 7, |_| {}).unwrap();
        assert_eq!(state_hash(reset.world_mut()), new_hash);
        let world = reset.world();
        assert!(world.resource::<SimControl>().paused, "starts paused");
        assert_eq!(
            world.resource::<PerceptionConfig>().forget_after_ticks,
            EXPLORER_FORGET_AFTER
        );
        let other = reset_world(&session, 9, |_| {}).unwrap();
        let s = other.world().resource::<ExplorerSession>();
        assert_eq!((s.seed, s.meta.agent.as_str()), (9, "Wren"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn tasklist_output_is_read_strictly() {
        let info = "INFO: No tasks are running which match the specified criteria.\r\n";
        let says = |out: &str, pid| tasklist_says(out, pid, Some(info));
        let row = r#""npcctl.exe","1234","Console","1","12,345 K""#;
        assert_eq!(says(&format!("\r\n{row}\r\n"), 1234), Some(true));
        assert_eq!(says(row, 123), None, "another pid's row is no answer");
        // A comma in the image name does not shift the pid field.
        let comma = r#""a,b.exe","1234","Console","1","1,024 K""#;
        assert_eq!(says(comma, 1234), Some(true));
        assert_eq!(says(comma, 1), None);
        // The no-match line is a clear no; anything else is unknown.
        assert_eq!(says(info, 1234), Some(false));
        assert_eq!(says("", 1234), None, "empty output is unknown");
        assert_eq!(says("Access denied\r\n", 1234), None);
        // Another language's no-match line, read against its own reference.
        let de =
            "INFORMATION: Es werden keine Aufgaben mit den angegebenen Kriterien ausgefuehrt.\r\n";
        assert_eq!(tasklist_says(de, 1234, Some(de)), Some(false));
        assert_eq!(says(de, 1234), None);
        // Without a usable reference, nothing is a clear no.
        assert_eq!(tasklist_says(info, 1234, None), None);
        assert_eq!(tasklist_says(info, 1234, Some("")), None);
        assert_eq!(
            tasklist_says(row, 99, Some(row)),
            None,
            "a row is no reference"
        );
    }

    /// Task 8: what the window, the menu and `npcctl serve` share for a
    /// move between window and detached.
    #[test]
    fn handover_helpers() {
        let line = DetachedLine {
            pid: 42,
            port: 15702,
            agent: "Wren \"W\"".into(),
            world: "Moor".into(),
        };
        let text = line.line();
        assert!(text.starts_with("{\"detached\":"), "{text}");
        assert_eq!(DetachedLine::parse(&format!("{text}\r\n")), Some(line));
        assert_eq!(DetachedLine::parse("{\"listening\":\"x\"}"), None);
        assert_eq!(DetachedLine::parse("INFO detached"), None);
        let args = detached_resume_args(Path::new("/agents"), "Wren", 15703);
        let home = Path::new("/agents").display().to_string();
        assert_eq!(
            args,
            [
                "serve",
                "--explorer-home",
                home.as_str(),
                "--agent",
                "Wren",
                "--resume",
                "--port",
                "15703",
                "--wait-port"
            ]
        );
        assert_eq!(
            prompt_text("---\ntype: prompt\ngenerated: true\n---\nYou are Wren.\n"),
            "You are Wren.\n"
        );
        assert_eq!(
            prompt_text("\u{feff}---\r\ntype: prompt\r\n---\r\nHi\r\n"),
            "Hi\r\n"
        );
        assert_eq!(prompt_text("no frontmatter"), "no frontmatter");
    }

    #[test]
    fn latest_prompt_is_the_newest_file() {
        let home =
            std::env::temp_dir().join(format!("npc-explorer-host-prompt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let dir = AgentDir {
            dir: home.join("wren"),
        };
        assert_eq!(latest_prompt(&dir), None);
        std::fs::create_dir_all(dir.dir.join("prompts")).unwrap();
        std::fs::write(dir.prompt(PromptKind::Start), "start").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(dir.prompt(PromptKind::Resume), "resume").unwrap();
        assert_eq!(latest_prompt(&dir), Some(dir.prompt(PromptKind::Resume)));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn this_process_is_alive() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(0));
    }
}
