//! Detached explorer worlds (spec G7, plan Task 8): `npcctl serve
//! --explorer-home` children the menu started, and live worlds it found in
//! an agent folder's `run.json` (adopted on the Resume screen, `--detached
//! AGENT`, or from a window that moved its world to detached).
//!
//! **The Detached screen** shows one world: the status line "<agent> in
//! <world> running detached on port N, tick T" (`session/info` every
//! [`POLL_EVERY`], on a background thread), the URL, the prompt (from the
//! prompt file `npcctl serve` named, without its frontmatter) with **Copy**
//! (the OS clipboard, through `testbed_content::explorer::clipboard` on a
//! thread), **Open in window**, **Stop** and **Back** (the world keeps
//! running).
//!
//! **Stop** and **Open in window** run on a worker thread, never on the
//! frame: `session/stop` (the world saves first; a failed save refuses the
//! stop), then the exit is waited for, at most [`STOP_WAIT`]. Stop goes back
//! to Resume with "<agent> stopped at tick T and was saved.". Open in window
//! (spec §8): a refused stop, or one that did not end the process in time,
//! aborts and the world keeps running detached; a process that exited
//! non-zero, or wrote no newer save, is restarted detached from its last
//! save on the same port; else `actor_testbed --resume --exact-port N` opens
//! it in the window, and a window that cannot take the port back (exit 4, or
//! 2) restarts it detached on `N` ([`crate::run`]).
//!
//! Back and the Detached screen leave a world running; the menu's Quit
//! stops every tracked world ([`crate::quit`], plan Task 29: it supersedes
//! G7's "a detached world outlives the menu"): `session/stop` through
//! [`OpKind::Quit`], and only a world that saved but did not exit, or one
//! the dev chose **Quit anyway** for, is killed by its own pid. A world
//! that exits on its own is dropped from the list.

use std::io::{BufRead as _, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use serde::Deserialize;
use serde_json::Value;
use testbed_content::explorer::home::{AgentDir, read_lossy, slug};
use testbed_content::explorer::host::{
    DetachedLine, RemoteError, RunFile, detach_command, latest_prompt, parse_rfc3339, pid_alive,
    prompt_text, remote_call, remote_call_checked, session_call,
};

use crate::explorer::{ResumeInfo, ResumeNotice};
use crate::launch::{self, Entry};
use crate::paste::ClipboardTools;
use crate::run::{self, KillError, Spawner, StderrTail};
use crate::screens::{Action, MainNotice, Screen, button};
use crate::ui::{self, ERROR, OK, TEXT};
use crate::{AppSettings, Paths};

/// How often the Detached screen asks the world for its tick.
pub const POLL_EVERY: Duration = Duration::from_secs(2);
/// How long Stop and Open in window wait for the process to exit after
/// `session/stop`.
pub const STOP_WAIT: Duration = Duration::from_secs(15);
/// `session/info`'s answer time.
const INFO_TIMEOUT: Duration = Duration::from_secs(5);
/// `session/stop`'s answer time (it saves first).
const STOP_TIMEOUT: Duration = Duration::from_secs(60);
/// How often a Quit asks an unanswering world again (one that moved here
/// from a window may still be taking the port over, `--wait-port`).
const STOP_RETRY: Duration = Duration::from_millis(250);
/// `app/quit`'s answer time (it answers before it saves).
const QUIT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long Copy reads "Copied".
const COPIED_FOR: Duration = Duration::from_secs(2);

/// A channel's receiving end a resource can hold (`Receiver` is not
/// `Sync`): only the frame reads it, with `try_recv`.
pub struct Rx<T>(Mutex<Receiver<T>>);

impl<T> Rx<T> {
    pub fn new(rx: Receiver<T>) -> Self {
        Self(Mutex::new(rx))
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.0
            .lock()
            .map_err(|_| TryRecvError::Disconnected)?
            .try_recv()
    }
}

/// How the menu talks to a detached world (a trait so tests stand in).
/// Called on background threads only.
pub trait Control: Send + Sync + 'static {
    /// `session/info`'s tick.
    ///
    /// # Errors
    /// No answer, or no tick in it.
    fn tick(&self, port: u16) -> Result<u64, String>;
    /// `session/stop` (the world saves first): its answer.
    ///
    /// # Errors
    /// [`StopError::Refused`] (the world answered: its save failed), or
    /// [`StopError::NoAnswer`].
    fn stop(&self, port: u16) -> Result<Value, StopError>;
    /// A game window's `app/quit {pid}` (BRP on `port`): it answers, then
    /// exits (an explorer window saves first). Refused by a process whose
    /// pid is not `pid`.
    ///
    /// # Errors
    /// No answer (no remote interface, not listening yet), or refused.
    fn quit_window(&self, port: u16, pid: u32) -> Result<Value, String>;
}

/// Why `session/stop` did not stop a world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopError {
    /// The world answered with an error: its save failed (the reason).
    Refused(String),
    /// No answer: it does not listen (yet), or the connection failed.
    NoAnswer(String),
}

impl From<String> for StopError {
    fn from(e: String) -> Self {
        Self::Refused(e)
    }
}

impl From<&str> for StopError {
    fn from(e: &str) -> Self {
        Self::Refused(e.to_owned())
    }
}

/// The real [`Control`]: JSON-RPC on `127.0.0.1:port`.
struct RpcControl;

impl Control for RpcControl {
    fn tick(&self, port: u16) -> Result<u64, String> {
        session_call(port, "session/info", INFO_TIMEOUT)?
            .get("tick")
            .and_then(Value::as_u64)
            .ok_or_else(|| "session/info has no tick".to_owned())
    }

    fn stop(&self, port: u16) -> Result<Value, StopError> {
        remote_call_checked(
            port,
            "session/stop",
            Value::Object(serde_json::Map::new()),
            STOP_TIMEOUT,
        )
        .map_err(|e| match e {
            RemoteError::Refused(e) => StopError::Refused(e),
            RemoteError::NoAnswer(e) => StopError::NoAnswer(e),
        })
    }

    fn quit_window(&self, port: u16, pid: u32) -> Result<Value, String> {
        remote_call(
            port,
            "app/quit",
            serde_json::json!({ "pid": pid }),
            QUIT_TIMEOUT,
        )
    }
}

/// The [`Control`] in use.
#[derive(Resource, Clone)]
pub struct Controls(pub Arc<dyn Control>);

impl Default for Controls {
    fn default() -> Self {
        Self(Arc::new(RpcControl))
    }
}

/// A line under the status: what happened last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

impl Notice {
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }

    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }
}

/// The detached process: a child the menu started, or a pid it found.
#[derive(Debug)]
pub enum Proc {
    Child(Child),
    Pid(u32),
}

impl Proc {
    fn pid(&self) -> u32 {
        match self {
            Self::Child(c) => c.id(),
            Self::Pid(p) => *p,
        }
    }

    /// `Some(code)` once it exited (`None` code: no code, or not our child).
    /// For a found pid this asks the OS: background threads only.
    fn exited(&mut self) -> Option<Option<i32>> {
        match self {
            Self::Child(c) => match c.try_wait() {
                Ok(Some(status)) => Some(status.code()),
                Ok(None) | Err(_) => None,
            },
            Self::Pid(p) => (!pid_alive(*p)).then_some(None),
        }
    }

    /// `exited` for the frame: a child's `try_wait` only.
    fn child_exited(&mut self) -> Option<Option<i32>> {
        match self {
            Self::Child(_) => self.exited(),
            Self::Pid(_) => None,
        }
    }
}

/// What a Stop, Open in window or the menu's Quit does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Stop,
    Open,
    /// The menu quits ([`crate::quit`]): the stop (asked again while the
    /// world does not answer, within [`crate::quit::QuitWait`]), and a world
    /// that saved but did not exit within that wait is killed by its pid.
    Quit,
}

/// Kills `proc` by its own pid (never by image name): a child the menu
/// started through its handle, or a pid it adopted once
/// [`run::kill_adopted`] checked it is still that world's process
/// (`started`: its `run.json`'s). Background threads only (a found pid asks
/// the OS).
///
/// # Errors
/// [`KillError::NotOurs`]: a reused pid, never killed; or the OS refused.
pub fn kill_proc(proc: &mut Proc, started: Option<SystemTime>) -> Result<(), KillError> {
    match proc {
        Proc::Child(c) => {
            if matches!(c.try_wait(), Ok(None)) {
                c.kill().map_err(|e| KillError::Failed(e.to_string()))?;
            }
            c.wait()
                .map(|_| ())
                .map_err(|e| KillError::Failed(e.to_string()))
        }
        Proc::Pid(p) => run::kill_adopted(*p, started),
    }
}

/// Removes `dir`'s `run.json` when it names `pid` (a world killed by the
/// menu cannot remove it itself).
pub fn forget_run_file(dir: &AgentDir, pid: u32) {
    if let Some(run) = RunFile::read_any(dir).filter(|r| r.pid == pid)
        && let Err(e) = run.remove(dir)
    {
        warn!(
            "game_menu: cannot remove {}: {e}",
            RunFile::path(dir).display()
        );
    }
}

/// How `session/stop` ended.
#[derive(Debug)]
enum Outcome {
    /// Refused: the world answered that its save failed; still running.
    Refused(String),
    /// Not answered (for a Quit: within its wait): presumably running.
    NoAnswer(String),
    /// Accepted, but the process still ran after [`STOP_WAIT`].
    StillRunning,
    /// The process exited: its code (`None`: no code, or a found pid), the
    /// tick of the stop and whether the save is newer than before the stop.
    Exited {
        code: Option<i32>,
        tick: Option<u64>,
        saved_newer: bool,
    },
    /// [`OpKind::Quit`]: the stop was accepted (the world saved) but the
    /// process still ran after [`STOP_WAIT`], so it was killed by its pid
    /// (`Err`: the kill failed, or the pid was not the world's any more).
    Killed {
        tick: Option<u64>,
        kill: Result<(), KillError>,
    },
}

/// A finished Stop or Open in window, with the process handed back.
#[derive(Debug)]
struct OpDone {
    kind: OpKind,
    proc: Proc,
    outcome: Outcome,
}

/// What a `session/info` poll found.
struct Polled {
    tick: Result<u64, String>,
    /// False once a found pid is gone (checked when the call fails).
    alive: bool,
}

/// `npcctl serve`'s first stdout line.
#[derive(Clone, Debug, Deserialize)]
struct Listening {
    port: u16,
    world: String,
    prompt: Option<String>,
}

/// Where the world is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Started, its listening line not read yet.
    Starting,
    Running,
    Busy(OpKind),
}

/// One detached world.
pub struct DetachedWorld {
    pub agent: String,
    /// The world's name (empty until known).
    pub world: String,
    /// Its port (0 until known).
    pub port: u16,
    /// The last tick `session/info` answered.
    pub tick: Option<u64>,
    /// Why the last poll got no answer.
    pub poll_error: Option<String>,
    /// The prompt file and its text.
    pub prompt: Option<(PathBuf, String)>,
    /// The last thing that happened to it.
    pub notice: Option<Notice>,
    phase: Phase,
    /// `None` while a Stop or Open in window holds it.
    proc: Option<Proc>,
    pid: u32,
    /// An adopted world's `run.json` `started` (a kill checks the pid's
    /// process is not younger).
    started: Option<SystemTime>,
    /// The listening line, while starting.
    listen: Option<Rx<Listening>>,
    stderr: Option<StderrTail>,
    poll: Option<Rx<Polled>>,
    next_poll: Instant,
    op: Option<Rx<OpDone>>,
}

impl DetachedWorld {
    fn new(agent: String, proc: Proc) -> Self {
        Self {
            agent,
            world: String::new(),
            port: 0,
            tick: None,
            poll_error: None,
            prompt: None,
            notice: None,
            phase: Phase::Running,
            pid: proc.pid(),
            started: None,
            proc: Some(proc),
            listen: None,
            stderr: None,
            poll: None,
            next_poll: Instant::now(),
            op: None,
        }
    }

    /// Up and idle (Open and Stop can run).
    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    /// Started, not listening yet.
    pub fn is_starting(&self) -> bool {
        self.phase == Phase::Starting
    }

    /// The process id.
    pub fn pid(&self) -> Option<u32> {
        Some(self.pid)
    }

    /// The status line.
    pub fn status(&self) -> String {
        if self.phase == Phase::Starting {
            return format!("Starting {}'s world detached...", self.agent);
        }
        let tick = self
            .tick
            .map_or_else(|| "...".to_owned(), |t| t.to_string());
        let mut line = format!(
            "{} in {} running detached on port {}, tick {tick}",
            self.agent, self.world, self.port
        );
        if let Some(e) = &self.poll_error {
            line.push_str(&format!(" (no answer: {e})"));
        }
        line
    }

    /// What runs right now, for the notice line.
    fn activity(&self) -> Option<&'static str> {
        match self.phase {
            Phase::Busy(OpKind::Stop) => Some("Stopping: the world saves first..."),
            Phase::Busy(OpKind::Open) => {
                Some("Saving and stopping, then opening the world in the window...")
            }
            Phase::Busy(OpKind::Quit) => Some("Quitting: the world saves first..."),
            _ => None,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Reads the prompt file (the listening line's, else the newest one of
    /// the agent folder).
    fn load_prompt(&mut self, path: Option<PathBuf>, dir: &AgentDir) {
        let Some(path) = path.or_else(|| latest_prompt(dir)) else {
            return;
        };
        let text = read_lossy(&path);
        if !text.is_empty() {
            self.prompt = Some((path, prompt_text(&text).trim_end().to_owned()));
        }
    }
}

/// The detached worlds the menu knows, and the one the Detached screen
/// shows.
#[derive(Resource, Default)]
pub struct DetachedWorlds {
    worlds: Vec<DetachedWorld>,
    /// The agent the Detached screen shows.
    pub shown: Option<String>,
}

impl DetachedWorlds {
    /// `agent`'s world (by folder name, as `run.json` is).
    pub fn get(&self, agent: &str) -> Option<&DetachedWorld> {
        let s = slug(agent);
        self.worlds.iter().find(|w| slug(&w.agent) == s)
    }

    pub fn get_mut(&mut self, agent: &str) -> Option<&mut DetachedWorld> {
        let s = slug(agent);
        self.worlds.iter_mut().find(|w| slug(&w.agent) == s)
    }

    fn remove(&mut self, agent: &str) -> Option<DetachedWorld> {
        let s = slug(agent);
        let i = self.worlds.iter().position(|w| slug(&w.agent) == s)?;
        Some(self.worlds.remove(i))
    }

    fn insert(&mut self, world: DetachedWorld) {
        self.remove(&world.agent);
        self.worlds.push(world);
    }

    /// Every tracked world.
    pub fn iter(&self) -> impl Iterator<Item = &DetachedWorld> {
        self.worlds.iter()
    }

    /// A Stop or Open in window is in flight (no other launch meanwhile).
    pub fn busy(&self) -> bool {
        self.worlds
            .iter()
            .any(|w| matches!(w.phase, Phase::Busy(_)))
    }

    /// Something must be polled: a world starting or busy.
    pub fn wants_frames(&self) -> bool {
        self.worlds
            .iter()
            .any(|w| w.phase != Phase::Running || w.poll.is_some())
    }

    fn shown_world(&self) -> Option<&DetachedWorld> {
        self.shown.as_deref().and_then(|a| self.get(a))
    }
}

/// Copy's state on the Detached screen.
#[derive(Resource, Default)]
pub struct PromptCopy {
    pending: Option<Rx<Result<(), String>>>,
    copied_at: Option<Instant>,
    error: Option<String>,
}

impl PromptCopy {
    /// A copy is in flight.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// The detached worlds and their screen.
pub struct DetachedPlugin;

impl Plugin for DetachedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DetachedWorlds>()
            .init_resource::<Controls>()
            .init_resource::<Spawner>()
            .init_resource::<PromptCopy>()
            .add_systems(OnEnter(Screen::ExplorerDetached), spawn_detached)
            .add_systems(
                Update,
                (
                    track,
                    (finish_copy, refresh_detached).run_if(in_state(Screen::ExplorerDetached)),
                )
                    .chain(),
            );
    }
}

/// The agent folder of `agent` in the settings' agents folder.
fn agent_dir(world: &World, agent: &str) -> AgentDir {
    AgentDir {
        dir: world
            .resource::<AppSettings>()
            .0
            .agents_dir
            .join(slug(agent)),
    }
}

/// Starts `npcctl serve` for a detached `entry` (`exe ARGS`, its own
/// console and process group so it outlives the menu) and shows it on the
/// Detached screen; a start that fails goes back to Main with the reason.
pub fn start(world: &mut World, entry: Entry, exe: &Path, args: &[String]) {
    let Some(agent) = entry.agent().map(str::to_owned) else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    detach_command(&mut cmd);
    let spawner = world.get_resource::<Spawner>().cloned().unwrap_or_default();
    let mut child = match (spawner.0)(cmd) {
        Ok(child) => child,
        Err(e) => {
            world.insert_resource(MainNotice::error(format!(
                "cannot start {}: {e}",
                exe.display()
            )));
            run::set_screen(world, Screen::Main);
            return;
        }
    };
    let listen = child.stdout.take().map(read_listening);
    let stderr = child.stderr.take().map(StderrTail::spawn);
    let mut w = DetachedWorld::new(agent.clone(), Proc::Child(child));
    w.phase = Phase::Starting;
    w.listen = listen;
    w.stderr = stderr;
    if let Entry::ExplorerNew { world: name, .. } = &entry {
        w.world.clone_from(name);
    }
    if let Entry::ExplorerRestart { port, .. } = &entry {
        w.port = *port;
    }
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    worlds.insert(w);
    worlds.shown = Some(agent);
    run::set_screen(world, Screen::ExplorerDetached);
}

/// Reads `npcctl serve`'s stdout on a thread: the listening line is sent
/// (lines before it are skipped), the rest is drained.
fn read_listening(stream: impl Read + Send + 'static) -> Rx<Listening> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut sent = false;
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if !sent
                && line.starts_with('{')
                && line.contains("\"listening\"")
                && let Ok(l) = serde_json::from_str::<Listening>(line)
            {
                sent = tx.send(l).is_ok();
            }
        }
    });
    Rx::new(rx)
}

/// Tracks the live detached world of `run` (a `run.json` the menu found);
/// a world already tracked is kept.
pub fn adopt(worlds: &mut DetachedWorlds, run: &RunFile, dir: &AgentDir) {
    if worlds.get(&run.agent).is_some() {
        return;
    }
    let mut w = DetachedWorld::new(run.agent.clone(), Proc::Pid(run.pid));
    w.started = parse_rfc3339(&run.started);
    w.world.clone_from(&run.world);
    w.port = run.port;
    w.load_prompt(None, dir);
    worlds.insert(w);
}

/// A window moved its world to detached (its [`DetachedLine`]): tracks the
/// new host and shows it.
pub fn adopt_moved(world: &mut World, moved: DetachedLine) {
    let agent = moved.agent.clone();
    adopt_moved_quietly(world, moved);
    world.resource_mut::<DetachedWorlds>().shown = Some(agent);
    run::set_screen(world, Screen::ExplorerDetached);
}

/// [`adopt_moved`] without showing it (the menu is quitting: the world is
/// stopped next).
pub fn adopt_moved_quietly(world: &mut World, moved: DetachedLine) {
    let dir = agent_dir(world, &moved.agent);
    let run = RunFile {
        pid: moved.pid,
        port: moved.port,
        mode: testbed_content::explorer::host::RunMode::Detached,
        started: String::new(),
        agent: moved.agent.clone(),
        world: moved.world,
    };
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    worlds.remove(&moved.agent);
    adopt(&mut worlds, &run, &dir);
}

/// `--detached AGENT`: shows `agent`'s detached world (tracked, or found in
/// its `run.json`); the Resume screen says so when none runs.
pub fn watch_running(world: &mut World, agent: &str) {
    let dir = agent_dir(world, agent);
    let found = RunFile::read_live(&dir)
        .filter(|r| r.mode == testbed_content::explorer::host::RunMode::Detached);
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    if let Some(run) = &found {
        adopt(&mut worlds, run, &dir);
    }
    if worlds.get(agent).is_some() {
        worlds.shown = Some(agent.to_owned());
        run::set_screen(world, Screen::ExplorerDetached);
    } else {
        world.resource_mut::<ResumeNotice>().0 =
            Some(format!("no detached world runs for {agent}"));
        run::set_screen(world, Screen::ExplorerResume);
    }
}

/// Sets `agent`'s notice line.
pub fn set_notice(world: &mut World, agent: &str, notice: Notice) {
    if let Some(w) = world.resource_mut::<DetachedWorlds>().get_mut(agent) {
        w.notice = Some(notice);
    }
}

/// The Detached screen's (or a Resume row's) Watch: shows `agent`.
pub fn watch(world: &mut World, agent: &str) {
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    if worlds.get(agent).is_none() {
        return;
    }
    worlds.shown = Some(agent.to_owned());
    run::set_screen(world, Screen::ExplorerDetached);
}

/// The save file of `agent`'s world.
fn save_path(world: &World, w: &DetachedWorld) -> PathBuf {
    agent_dir(world, &w.agent).save(&w.world)
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Stop or Open in window for `agent`: `session/stop` and the wait for the
/// exit on a worker thread (see the module docs). Open in window first
/// checks that `actor_testbed` is built (the world is not stopped for a
/// window that cannot open).
pub fn begin_op(world: &mut World, agent: &str, kind: OpKind) {
    if run::busy(world) {
        return;
    }
    if kind == OpKind::Open {
        let exe_dir = world.resource::<Paths>().exe_dir.clone();
        if launch::find_exe("actor_testbed", &exe_dir).is_none() {
            let cmd = launch::command_line(&launch::build_args("actor_testbed", &exe_dir));
            set_notice(
                world,
                agent,
                Notice::error(format!(
                    "actor_testbed is not built ({cmd}): the world keeps running detached"
                )),
            );
            show(world, agent);
            return;
        }
    }
    start_op(world, agent, kind);
}

/// Starts `kind` for `agent`'s world when it runs idle (no busy check: the
/// menu's Quit stops every world at once); `false` when it does not. A
/// Quit's stop and exit share `wait` ([`crate::quit::QuitWait`]); Stop and
/// Open in window wait [`STOP_WAIT`] for the exit.
pub fn start_op(world: &mut World, agent: &str, kind: OpKind) -> bool {
    let wait = match kind {
        OpKind::Quit => {
            world
                .get_resource::<crate::quit::QuitWait>()
                .copied()
                .unwrap_or_default()
                .0
        }
        OpKind::Stop | OpKind::Open => STOP_WAIT,
    };
    let control = world.resource::<Controls>().0.clone();
    let save = match world.resource::<DetachedWorlds>().get(agent) {
        Some(w) if w.is_running() => save_path(world, w),
        _ => return false,
    };
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    let Some(w) = worlds.get_mut(agent) else {
        return false;
    };
    let Some(proc) = w.proc.take() else {
        return false;
    };
    w.phase = Phase::Busy(kind);
    w.notice = None;
    let (port, started) = (w.port, w.started);
    info!(
        "game_menu: {kind:?} for {agent}'s detached world (pid {}, port {port})",
        w.pid
    );
    let (tx, rx) = mpsc::channel();
    w.op = Some(Rx::new(rx));
    std::thread::spawn(move || {
        let stop = Stop {
            kind,
            port,
            wait,
            started,
        };
        let _ = tx.send(stop_and_wait(&stop, control.as_ref(), proc, &save));
    });
    true
}

/// What [`stop_and_wait`] stops and how long it waits.
struct Stop {
    kind: OpKind,
    port: u16,
    wait: Duration,
    /// An adopted world's `run.json` `started` (for the kill's check).
    started: Option<SystemTime>,
}

/// The menu's Quit anyway: takes `agent`'s idle world's process (to be
/// killed by its pid), its folder and its `run.json` `started` (an adopted
/// world's); the world shows busy meanwhile.
pub fn take_proc(world: &mut World, agent: &str) -> Option<(Proc, AgentDir, Option<SystemTime>)> {
    let dir = agent_dir(world, agent);
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    let w = worlds.get_mut(agent).filter(|w| w.is_running())?;
    let proc = w.proc.take()?;
    w.phase = Phase::Busy(OpKind::Quit);
    Some((proc, dir, w.started))
}

/// Hands a process [`take_proc`] took back (its kill failed): idle again.
pub fn give_back(world: &mut World, agent: &str, proc: Proc) {
    if let Some(w) = world.resource_mut::<DetachedWorlds>().get_mut(agent) {
        w.proc = Some(proc);
        w.phase = Phase::Running;
    }
}

/// Drops `agent`'s world from the list (it was killed).
pub fn forget(world: &mut World, agent: &str) {
    world.resource_mut::<DetachedWorlds>().remove(agent);
}

/// The menu quits while `agent`'s world still starts (no port to stop it
/// on yet, and past the wait): kills the child the menu started (through
/// its handle) and drops it, with the `run.json` it may have written. Its
/// pid, when there was one to kill.
pub fn kill_starting(world: &mut World, agent: &str) -> Option<u32> {
    let dir = agent_dir(world, agent);
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    let w = worlds.get_mut(agent).filter(|w| w.is_starting())?;
    let pid = w.pid;
    if let Some(Proc::Child(mut child)) = w.proc.take() {
        run::kill(&mut child);
    }
    worlds.remove(agent);
    forget_run_file(&dir, pid);
    Some(pid)
}

/// `session/stop`, then the exit, waited for at most `stop.wait` (a worker
/// thread). A Quit asks again while the world does not answer (a world
/// moved here from a window may still be taking the port over), within
/// the same `wait`, and kills a world that saved but did not exit.
fn stop_and_wait(stop: &Stop, control: &dyn Control, mut proc: Proc, save: &Path) -> OpDone {
    let Stop {
        kind,
        port,
        wait,
        started,
    } = *stop;
    let before = modified(save);
    let budget = Instant::now() + wait;
    let answer = loop {
        match control.stop(port) {
            Err(StopError::NoAnswer(e))
                if kind == OpKind::Quit
                    && Instant::now() + STOP_RETRY < budget
                    && proc.exited().is_none() =>
            {
                debug!("game_menu: no answer to session/stop on port {port} yet: {e}");
                std::thread::sleep(STOP_RETRY);
            }
            other => break other,
        }
    };
    let outcome = match answer {
        Err(StopError::Refused(e)) => Outcome::Refused(e),
        Err(StopError::NoAnswer(e)) => Outcome::NoAnswer(e),
        Ok(reply) => {
            let tick = reply.get("tick").and_then(Value::as_u64);
            let deadline = Instant::now() + wait;
            loop {
                if let Some(code) = proc.exited() {
                    let after = modified(save);
                    let saved_newer = after.is_some() && (before.is_none() || after > before);
                    break Outcome::Exited {
                        code,
                        tick,
                        saved_newer,
                    };
                }
                if Instant::now() >= deadline {
                    if kind == OpKind::Quit {
                        break Outcome::Killed {
                            tick,
                            kill: kill_proc(&mut proc, started),
                        };
                    }
                    break Outcome::StillRunning;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    };
    OpDone {
        kind,
        proc,
        outcome,
    }
}

/// Every frame: starting worlds' listening lines, exits, polls of the shown
/// world, finished Stops and Opens.
pub(crate) fn track(world: &mut World) {
    let on_screen = *world.resource::<State<Screen>>().get() == Screen::ExplorerDetached;
    let control = world.resource::<Controls>().0.clone();
    let agents: Vec<String> = world
        .resource::<DetachedWorlds>()
        .worlds
        .iter()
        .map(|w| w.agent.clone())
        .collect();
    for agent in agents {
        let dir = agent_dir(world, &agent);
        let event = {
            let mut worlds = world.resource_mut::<DetachedWorlds>();
            let shown = worlds
                .shown
                .as_deref()
                .is_some_and(|s| slug(s) == slug(&agent));
            let Some(w) = worlds.get_mut(&agent) else {
                continue;
            };
            step(w, &dir, on_screen && shown, &control)
        };
        match event {
            Event::None => {}
            Event::Done(done) => finish_op(world, &agent, done),
            Event::Gone {
                code,
                why,
                starting,
            } => {
                world.resource_mut::<DetachedWorlds>().remove(&agent);
                gone(world, &agent, code, why.as_deref(), starting);
            }
        }
    }
}

/// What a frame found for one world.
enum Event {
    None,
    /// A Stop or Open in window finished.
    Done(OpDone),
    /// The process ended on its own.
    Gone {
        code: Option<i32>,
        why: Option<String>,
        starting: bool,
    },
}

/// One frame of one world (see [`track`]); `poll`: it is on screen.
fn step(w: &mut DetachedWorld, dir: &AgentDir, poll: bool, control: &Arc<dyn Control>) -> Event {
    if let Some(rx) = &w.op {
        return match rx.try_recv() {
            Ok(done) => {
                w.op = None;
                Event::Done(done)
            }
            Err(TryRecvError::Empty) => Event::None,
            Err(TryRecvError::Disconnected) => {
                w.op = None;
                w.phase = Phase::Running;
                w.notice = Some(Notice::error("the stop's worker ended without an answer"));
                Event::None
            }
        };
    }
    if let Some(code) = w.proc.as_mut().and_then(Proc::child_exited) {
        return Event::Gone {
            code,
            why: w.stderr.as_ref().and_then(StderrTail::last_line),
            starting: w.phase == Phase::Starting,
        };
    }
    if w.phase == Phase::Starting
        && let Some(l) = w.listen.as_ref().and_then(|rx| rx.try_recv().ok())
    {
        w.phase = Phase::Running;
        w.listen = None;
        w.port = l.port;
        w.world = l.world;
        w.load_prompt(l.prompt.map(PathBuf::from), dir);
        w.next_poll = Instant::now();
    }
    if let Some(rx) = &w.poll {
        match rx.try_recv() {
            Ok(polled) => {
                w.poll = None;
                match polled.tick {
                    Ok(t) => {
                        w.tick = Some(t);
                        w.poll_error = None;
                        if w.prompt.is_none() {
                            w.load_prompt(None, dir);
                        }
                    }
                    Err(e) => w.poll_error = Some(e),
                }
                if !polled.alive {
                    return Event::Gone {
                        code: None,
                        why: None,
                        starting: false,
                    };
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => w.poll = None,
        }
    }
    if poll && w.is_running() && w.poll.is_none() && Instant::now() >= w.next_poll {
        w.next_poll = Instant::now() + POLL_EVERY;
        let (tx, rx) = mpsc::channel();
        w.poll = Some(Rx::new(rx));
        let (control, port) = (Arc::clone(control), w.port);
        let pid = match &w.proc {
            Some(Proc::Pid(p)) => Some(*p),
            _ => None,
        };
        std::thread::spawn(move || {
            let tick = control.tick(port);
            let alive = tick.is_ok() || pid.is_none_or(pid_alive);
            let _ = tx.send(Polled { tick, alive });
        });
    }
    Event::None
}

/// The title errors use for `agent`'s detached world.
fn title(agent: &str) -> String {
    format!("AI explorer: {agent} (detached)")
}

/// A world that ended on its own: dropped (by the caller), and said where
/// the dev looks when it was on screen (Main for one that never came up,
/// else Resume).
fn gone(world: &mut World, agent: &str, code: Option<i32>, why: Option<&str>, starting: bool) {
    let title = title(agent);
    let text = launch::exit_error(&title, code, why).unwrap_or_else(|| {
        if code == Some(0) {
            format!("{title} stopped")
        } else {
            format!("{title} is no longer running")
        }
    });
    warn!("game_menu: {text}");
    let shown = {
        let mut worlds = world.resource_mut::<DetachedWorlds>();
        let shown = worlds
            .shown
            .as_deref()
            .is_some_and(|s| slug(s) == slug(agent));
        if shown {
            worlds.shown = None;
        }
        shown
    };
    let on_screen = *world.resource::<State<Screen>>().get() == Screen::ExplorerDetached;
    if !(shown && on_screen) {
        return;
    }
    if starting {
        world.insert_resource(MainNotice::error(text));
        run::set_screen(world, Screen::Main);
    } else {
        world.resource_mut::<ResumeNotice>().0 = Some(text);
        run::set_screen(world, Screen::ExplorerResume);
    }
}

/// A Stop or Open in window finished (see the module docs).
fn finish_op(world: &mut World, agent: &str, done: OpDone) {
    if done.kind == OpKind::Quit || crate::quit::active(world) {
        quit_finished(world, agent, done);
        return;
    }
    let OpDone {
        kind,
        proc,
        outcome,
    } = done;
    let adopted = matches!(proc, Proc::Pid(_));
    let Some(save) = world
        .resource::<DetachedWorlds>()
        .get(agent)
        .map(|w| save_path(world, w))
    else {
        return;
    };
    let mut worlds = world.resource_mut::<DetachedWorlds>();
    let Some(w) = worlds.get_mut(agent) else {
        return;
    };
    let port = w.port;
    let keep = |w: &mut DetachedWorld, proc: Proc, text: String| {
        w.proc = Some(proc);
        w.phase = Phase::Running;
        w.notice = Some(Notice::error(text));
        w.next_poll = Instant::now();
    };
    let what = match kind {
        OpKind::Stop | OpKind::Quit => "Stop",
        OpKind::Open => "Open in window",
    };
    match outcome {
        Outcome::Refused(e) => {
            keep(
                w,
                proc,
                format!("{what} aborted: {e}; the world keeps running detached"),
            );
            show(world, agent);
        }
        Outcome::NoAnswer(e) => {
            keep(
                w,
                proc,
                format!("{what} aborted: the world does not answer ({e}); nothing was stopped"),
            );
            show(world, agent);
        }
        // `Killed` comes from a Quit only, which `quit_finished` takes.
        Outcome::StillRunning | Outcome::Killed { .. } => {
            keep(
                w,
                proc,
                format!(
                    "{what} aborted: the world did not exit within {} s of the stop; it keeps \
                     running detached",
                    STOP_WAIT.as_secs()
                ),
            );
            show(world, agent);
        }
        Outcome::Exited {
            code,
            tick,
            saved_newer,
        } => {
            let clean = exit_was_clean(code, adopted, saved_newer);
            // Only now: the reason may wait a moment for the stream's end.
            let why = w.stderr.as_ref().and_then(StderrTail::last_line);
            worlds.remove(agent);
            match kind {
                OpKind::Stop | OpKind::Quit => {
                    if clean {
                        let at = tick.map_or_else(String::new, |t| format!(" at tick {t}"));
                        world.resource_mut::<ResumeInfo>().0 =
                            Some(format!("{agent} stopped{at} and was saved."));
                    } else if adopted && code.is_none() {
                        world.resource_mut::<ResumeNotice>().0 = Some(format!(
                            "{agent} stopped, but its save is not newer than before the stop: check {}",
                            save.display()
                        ));
                    } else {
                        let code = code.map_or_else(|| "?".to_owned(), |c| c.to_string());
                        let head = format!(
                            "{agent}'s detached world exited with code {code} after the stop"
                        );
                        world.resource_mut::<ResumeNotice>().0 =
                            Some(launch::with_reason(head, why.as_deref()));
                    }
                    world.resource_mut::<DetachedWorlds>().shown = None;
                    run::set_screen(world, Screen::ExplorerResume);
                }
                OpKind::Open if clean && saved_newer => {
                    run::start_game(
                        world,
                        Entry::ExplorerMove {
                            agent: agent.to_owned(),
                            port,
                        },
                    );
                }
                OpKind::Open => {
                    let head = if clean {
                        "Open in window aborted: the world wrote no new save at the stop".to_owned()
                    } else {
                        let code = code.map_or_else(|| "?".to_owned(), |c| c.to_string());
                        launch::with_reason(
                            format!(
                                "Open in window aborted: the world exited with code {code} at the stop"
                            ),
                            why.as_deref(),
                        )
                    };
                    let text =
                        format!("{head}; restarted detached from the last save on port {port}");
                    run::start_game(
                        world,
                        Entry::ExplorerRestart {
                            agent: agent.to_owned(),
                            port,
                        },
                    );
                    set_notice(world, agent, Notice::error(text));
                }
            }
        }
    }
}

/// A stop finished while the menu quits (its own [`OpKind::Quit`], or a
/// Stop or Open in window that was in flight when Quit was pressed): no
/// screen changes, no window and no restart; the Quitting screen gets the
/// result. A world still running is kept: after its own refused stop the
/// Quitting screen offers Quit anyway; after a Stop or Open in flight the
/// quit stops it next.
fn quit_finished(world: &mut World, agent: &str, done: OpDone) {
    let OpDone {
        kind,
        proc,
        outcome,
    } = done;
    let adopted = matches!(proc, Proc::Pid(_));
    let pid = proc.pid();
    let dir = agent_dir(world, agent);
    let save = world
        .resource::<DetachedWorlds>()
        .get(agent)
        .map(|w| save_path(world, w));
    let keep = |world: &mut World, proc: Proc, why: Option<&str>| {
        if let Some(w) = world.resource_mut::<DetachedWorlds>().get_mut(agent) {
            w.proc = Some(proc);
            w.phase = Phase::Running;
            if let Some(why) = why {
                w.notice = Some(Notice::error(format!("Quit: {why}")));
            }
        }
    };
    let secs = world
        .get_resource::<crate::quit::QuitWait>()
        .copied()
        .unwrap_or_default()
        .0
        .as_secs();
    let result = match outcome {
        Outcome::Refused(e) if kind == OpKind::Quit => {
            let e = format!("refused the stop, its save failed: {e}");
            keep(world, proc, Some(&e));
            Err(e)
        }
        Outcome::NoAnswer(e) if kind == OpKind::Quit => {
            let e = format!("does not answer session/stop (within {secs} s): {e}");
            keep(world, proc, Some(&e));
            Err(e)
        }
        Outcome::Killed {
            kill: Err(KillError::Failed(e)),
            ..
        } => {
            let e = format!(
                "it saved but still ran {secs} s after the stop, and cannot be killed: {e}"
            );
            keep(world, proc, Some(&e));
            Err(e)
        }
        Outcome::Killed {
            kill: Err(KillError::NotOurs(e)),
            ..
        } => {
            // The world's pid now belongs to another process: never killed;
            // its world is gone, so is its stale run.json.
            forget_run_file(&dir, pid);
            world.resource_mut::<DetachedWorlds>().remove(agent);
            Ok(format!("not killed: {e}; its stale run.json was removed"))
        }
        Outcome::Refused(_) | Outcome::NoAnswer(_) | Outcome::StillRunning => {
            // A Stop or Open in window in flight that did not end it: the
            // quit's own stop comes next.
            keep(world, proc, None);
            return;
        }
        Outcome::Killed { tick, kill: Ok(()) } => {
            forget_run_file(&dir, pid);
            world.resource_mut::<DetachedWorlds>().remove(agent);
            let at = tick.map_or_else(String::new, |t| format!(" at tick {t}"));
            Ok(format!(
                "saved{at}, but still running {secs} s after the stop: killed (pid {pid})"
            ))
        }
        Outcome::Exited {
            code,
            tick,
            saved_newer,
        } => {
            let why = world
                .resource::<DetachedWorlds>()
                .get(agent)
                .and_then(|w| w.stderr.as_ref())
                .and_then(StderrTail::last_line);
            world.resource_mut::<DetachedWorlds>().remove(agent);
            Ok(if exit_was_clean(code, adopted, saved_newer) {
                let at = tick.map_or_else(String::new, |t| format!(" at tick {t}"));
                format!("stopped{at} and saved")
            } else if adopted && code.is_none() {
                format!(
                    "stopped, but its save is not newer than before the stop: check {}",
                    save.map_or_else(String::new, |p| p.display().to_string())
                )
            } else {
                let code = code.map_or_else(|| "?".to_owned(), |c| c.to_string());
                launch::with_reason(
                    format!("exited with code {code} after the stop"),
                    why.as_deref(),
                )
            })
        }
    };
    crate::quit::world_result(world, agent, result);
}

/// Whether a world that exited after `session/stop` ended well: our child
/// with code 0, or a found pid (no exit code) whose save is newer than
/// before the stop.
fn exit_was_clean(code: Option<i32>, adopted: bool, saved_newer: bool) -> bool {
    code == Some(0) || (code.is_none() && adopted && saved_newer)
}

/// Shows `agent` on the Detached screen (an op from a Resume row that
/// ended there says why on that screen).
fn show(world: &mut World, agent: &str) {
    world.resource_mut::<DetachedWorlds>().shown = Some(agent.to_owned());
    if *world.resource::<State<Screen>>().get() != Screen::ExplorerDetached {
        run::set_screen(world, Screen::ExplorerDetached);
    }
}

/// Copy: the shown world's prompt to the OS clipboard, on a thread.
pub fn copy_prompt(world: &mut World) {
    let Some(text) = world
        .resource::<DetachedWorlds>()
        .shown_world()
        .and_then(|w| w.prompt.as_ref().map(|(_, t)| t.clone()))
    else {
        return;
    };
    if world.resource::<PromptCopy>().is_pending() {
        return;
    }
    let copy = world
        .get_resource::<ClipboardTools>()
        .copied()
        .unwrap_or_default()
        .copy;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(copy(&text));
    });
    let mut state = world.resource_mut::<PromptCopy>();
    state.pending = Some(Rx::new(rx));
    state.error = None;
}

fn finish_copy(mut copy: ResMut<PromptCopy>) {
    let Some(rx) = &copy.pending else {
        return;
    };
    match rx.try_recv() {
        Ok(Ok(())) => {
            copy.pending = None;
            copy.copied_at = Some(Instant::now());
        }
        Ok(Err(e)) => {
            copy.pending = None;
            copy.error = Some(e);
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            copy.pending = None;
            copy.error = Some("the copy did not finish".into());
        }
    }
}

/// The Detached screen's lines and buttons ([`refresh_detached`] rewrites
/// them).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Line {
    Status,
    Url,
    Folder,
    Prompt,
    Notice,
}

/// The Detached screen's Open in window and Stop.
#[derive(Component)]
struct OpButton;

/// The Detached screen's Copy.
#[derive(Component)]
struct CopyButton;

fn spawn_detached(mut commands: Commands, worlds: Res<DetachedWorlds>, settings: Res<AppSettings>) {
    let Some(w) = worlds.shown_world() else {
        commands.queue(|w: &mut World| run::set_screen(w, Screen::ExplorerResume));
        return;
    };
    let agent = w.agent.clone();
    let dir = settings.0.agents_dir.join(slug(&agent));
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::ExplorerDetached),
        "Running detached",
    );
    commands.entity(panel).with_children(|p| {
        p.spawn((Line::Status, ui::label(w.status(), 18.0, TEXT)));
        p.spawn((Line::Url, ui::label("", 14.0, ui::DIM)));
        p.spawn((
            Line::Folder,
            ui::label(
                format!(
                    "Open your AI tool in {} and paste this prompt:",
                    dir.display()
                ),
                14.0,
                ui::DIM,
            ),
        ));
        p.spawn((
            Node {
                overflow: Overflow::scroll_y(),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: px(60),
                padding: UiRect::all(px(8)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            ScrollArea,
            BackgroundColor(ui::FIELD),
        ))
        .with_child((Line::Prompt, ui::label("", 13.0, TEXT)));
        ui::row(p, |r| {
            let copy = button(r, "Copy", None, false, 0, Action::CopyPrompt);
            r.commands().entity(copy).insert(CopyButton);
        });
        p.spawn((Line::Notice, ui::label("", 14.0, ERROR)));
        ui::row(p, |r| {
            let open = button(
                r,
                "Open in window",
                None,
                false,
                1,
                Action::OpenInWindow(agent.clone()),
            );
            r.commands().entity(open).insert((OpButton, AutoFocus));
            let stop = button(
                r,
                "Stop",
                None,
                false,
                2,
                Action::StopDetached(agent.clone()),
            );
            r.commands().entity(stop).insert(OpButton);
            button(r, "Back", None, true, 3, Action::Go(Screen::ExplorerResume));
        });
    });
}

/// What [`refresh_detached`] writes.
type LineQuery<'w, 's> = Query<'w, 's, (&'static Line, &'static mut Text, &'static mut TextColor)>;

/// The buttons' children and their label text and colour.
type ButtonLabels<'w, 's> = (
    Query<'w, 's, &'static Children>,
    Query<'w, 's, &'static mut TextColor, Without<Line>>,
    Query<'w, 's, &'static mut Text, Without<Line>>,
);

/// The Detached screen from the shown world: status, URL, prompt, notice,
/// Copy's label, and Open / Stop enabled while it runs idle.
fn refresh_detached(
    mut commands: Commands,
    worlds: Res<DetachedWorlds>,
    copy: Res<PromptCopy>,
    mut lines: LineQuery,
    ops: Query<(Entity, Has<bevy::ui::InteractionDisabled>), With<OpButton>>,
    copies: Query<(Entity, Has<bevy::ui::InteractionDisabled>), With<CopyButton>>,
    (children, mut colors, mut labels): ButtonLabels,
) {
    let Some(w) = worlds.shown_world() else {
        return;
    };
    let copied = copy.copied_at.is_some_and(|at| at.elapsed() < COPIED_FOR);
    for (line, mut text, mut color) in &mut lines {
        let (want, tint) = match line {
            Line::Status => (w.status(), TEXT),
            Line::Url => (
                if w.phase == Phase::Starting {
                    String::new()
                } else {
                    format!("URL: {}", w.url())
                },
                ui::DIM,
            ),
            Line::Folder => (text.0.clone(), ui::DIM),
            Line::Prompt => (
                w.prompt.as_ref().map_or_else(
                    || "(the prompt shows once the world is up)".to_owned(),
                    |(_, t)| t.clone(),
                ),
                TEXT,
            ),
            Line::Notice => match (w.activity(), &w.notice, &copy.error) {
                (Some(a), _, _) => (a.to_owned(), ui::NOTE),
                (None, Some(n), _) => (n.text.clone(), if n.error { ERROR } else { OK }),
                (None, None, Some(e)) => (
                    format!(
                        "copy failed ({e}): select the text in {} instead",
                        w.prompt
                            .as_ref()
                            .map_or_else(String::new, |(p, _)| p.display().to_string())
                    ),
                    ERROR,
                ),
                (None, None, None) => (String::new(), TEXT),
            },
        };
        if text.0 != want {
            text.0 = want;
        }
        color.set_if_neq(TextColor(tint));
    }
    let idle = w.is_running();
    for (b, off) in &ops {
        if off == idle {
            ui::set_enabled(&mut commands, b, idle, &children, &mut colors);
        }
    }
    let can_copy = w.prompt.is_some() && !copy.is_pending();
    let label = if copied { "Copied" } else { "Copy" };
    for (b, off) in &copies {
        if off == can_copy {
            ui::set_enabled(&mut commands, b, can_copy, &children, &mut colors);
        }
        if let Some(first) = children.get(b).ok().and_then(|c| c.first().copied())
            && let Ok(mut text) = labels.get_mut(first)
            && text.0 != label
        {
            label.clone_into(&mut text.0);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use bevy::ui::InteractionDisabled;
    use serde_json::{Value, json};
    use testbed_content::explorer::home::create_agent;
    use testbed_content::explorer::host::{RunFile, RunMode};
    use testbed_content::explorer::names::NameKind;

    use super::*;
    use crate::explorer::{NameField, ResumeInfo, ResumeNotice};
    use crate::launch::Entry;
    use crate::run::{self, Running, Spawner};
    use crate::screens::tests::{app, button_for, press, screen, test_child_until, tmp};
    use crate::screens::{Action, Screen};

    /// One child the fake spawner starts: prints `stdout` / `stderr`, waits
    /// for `until` to exist (when set), then exits with `code`.
    #[derive(Clone)]
    pub(crate) struct Plan {
        pub(crate) code: i32,
        pub(crate) stdout: String,
        pub(crate) stderr: String,
        pub(crate) until: Option<PathBuf>,
    }

    /// What the fake spawner was asked to start: the program's stem and
    /// its arguments.
    pub(crate) type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

    /// A [`Spawner`] that records each command and starts the next planned
    /// fake child (this test binary) instead.
    pub(crate) fn fake_spawner(app: &mut App, plans: Vec<Plan>) -> Calls {
        let calls: Calls = Arc::default();
        let log = Arc::clone(&calls);
        let plans = Arc::new(Mutex::new(VecDeque::from(plans)));
        app.insert_resource(Spawner(Arc::new(move |cmd: Command| {
            let stem = Path::new(cmd.get_program())
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let args = cmd
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            log.lock().unwrap().push((stem, args));
            let plan = plans.lock().unwrap().pop_front().expect("a planned child");
            test_child_until(
                Some(plan.code),
                &plan.stdout,
                &plan.stderr,
                plan.until.as_deref(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        })));
        calls
    }

    /// A [`Control`] whose `session/info` counts up from 1 and whose
    /// `session/stop` is `stop`.
    pub(crate) struct FakeControl {
        ticks: AtomicU64,
        stop: Box<dyn Fn(u16) -> Result<Value, StopError> + Send + Sync>,
        quit: Box<QuitFn>,
    }

    /// A fake `app/quit`: the port and the pid it names.
    pub(crate) type QuitFn = dyn Fn(u16, u32) -> Result<Value, String> + Send + Sync;

    impl Control for FakeControl {
        fn tick(&self, _port: u16) -> Result<u64, String> {
            Ok(self.ticks.fetch_add(1, Ordering::SeqCst) + 1)
        }
        fn stop(&self, port: u16) -> Result<Value, StopError> {
            (self.stop)(port)
        }
        fn quit_window(&self, port: u16, pid: u32) -> Result<Value, String> {
            (self.quit)(port, pid)
        }
    }

    /// A [`FakeControl`] with `stop`; windows have no remote interface.
    pub(crate) fn fake_control(
        app: &mut App,
        stop: impl Fn(u16) -> Result<Value, StopError> + Send + Sync + 'static,
    ) {
        fake_control_with(app, stop, |port, _| {
            Err(format!("http://127.0.0.1:{port} (connection refused)"))
        });
    }

    /// A [`FakeControl`] with `stop` and the windows' `app/quit`.
    pub(crate) fn fake_control_with(
        app: &mut App,
        stop: impl Fn(u16) -> Result<Value, StopError> + Send + Sync + 'static,
        quit: impl Fn(u16, u32) -> Result<Value, String> + Send + Sync + 'static,
    ) {
        app.insert_resource(Controls(Arc::new(FakeControl {
            ticks: AtomicU64::new(0),
            stop: Box::new(stop),
            quit: Box::new(quit),
        })));
    }

    /// The menu over `d/exe` (with stand-ins for `npcctl` and the games, so
    /// none is "missing") and `d/agents`.
    pub(crate) fn menu(d: &Path) -> App {
        let exe = d.join("exe");
        std::fs::create_dir_all(&exe).unwrap();
        for stem in ["npcctl", "actor_testbed", "village", "rts_units_demo"] {
            std::fs::write(
                exe.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX)),
                "",
            )
            .unwrap();
        }
        app(&exe, &d.join("agents"))
    }

    pub(crate) fn texts(app: &mut App) -> Vec<String> {
        let mut q = app.world_mut().query::<&Text>();
        q.iter(app.world()).map(|t| t.0.clone()).collect()
    }

    pub(crate) fn has_text(app: &mut App, want: &str) -> bool {
        texts(app).iter().any(|t| t.contains(want))
    }

    /// Updates (a few ms apart) until `done`, at most `within`.
    pub(crate) fn until(
        app: &mut App,
        within: Duration,
        mut done: impl FnMut(&mut App) -> bool,
    ) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            app.update();
            if done(app) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    pub(crate) fn listening(port: u16, world: &str, prompt: &Path) -> String {
        format!(
            "{}\n",
            json!({
                "listening": format!("http://127.0.0.1:{port}"),
                "port": port,
                "agent": "Wren Ashdown",
                "world": world,
                "prompt": prompt.display().to_string(),
            })
        )
    }

    /// A detached child that announces `port` and runs until `marker`
    /// exists, then exits with `code`.
    pub(crate) fn served(port: u16, prompt: &Path, marker: &Path, code: i32) -> Plan {
        Plan {
            code,
            stdout: listening(port, "Moor", prompt),
            stderr: String::new(),
            until: Some(marker.to_path_buf()),
        }
    }

    pub(crate) fn prompt_file(d: &Path) -> PathBuf {
        let p = d.join("start.md");
        std::fs::write(
            &p,
            "---\ntype: prompt\nagent: Wren Ashdown\nworld: Moor\ngenerated: true\n---\nYou are Wren.\n",
        )
        .unwrap();
        p
    }

    /// New, names typed, **Start detached**: the world starts and the
    /// Detached screen shows it.
    fn start_new_detached(app: &mut App) {
        run::set_screen(app.world_mut(), Screen::ExplorerNew);
        app.update();
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&NameField, &mut bevy::text::EditableText)>();
        for (f, mut t) in q.iter_mut(app.world_mut()) {
            *t = bevy::text::EditableText::new(match f.0 {
                NameKind::Agent => "Wren Ashdown",
                NameKind::World => "Moor",
            });
        }
        app.update();
        app.update();
        let start = button_for(app, |a| matches!(a, Action::StartNew(RunMode::Detached)));
        press(app, start);
        app.update();
    }

    /// Lets every fake child still waiting for its marker exit.
    pub(crate) fn release(markers: &[&Path]) {
        for m in markers {
            let _ = std::fs::write(m, "");
        }
    }

    pub(crate) fn status_line(port: u16, tick: u64) -> String {
        format!("Wren Ashdown in Moor running detached on port {port}, tick {tick}")
    }

    #[test]
    fn detached_screen_polls_tick() {
        let d = tmp("d-poll");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        let calls = fake_spawner(&mut app, vec![served(15790, &prompt, &marker, 0)]);
        fake_control(&mut app, |_| Err("no stop in this test".into()));
        start_new_detached(&mut app);
        assert_eq!(screen(&app), Screen::ExplorerDetached);
        // npcctl serve with the typed names.
        let (stem, args) = calls.lock().unwrap()[0].clone();
        assert_eq!(stem, "npcctl");
        assert_eq!(&args[..1], ["serve"]);
        assert!(
            args.windows(2).any(|w| w == ["--world", "Moor"]),
            "{args:?}"
        );
        // The prompt (without its frontmatter), with Copy, and the URL.
        assert!(
            until(&mut app, Duration::from_secs(30), |a| has_text(
                a,
                "You are Wren."
            )),
            "{:?}",
            texts(&mut app)
        );
        assert!(!has_text(&mut app, "generated: true"));
        assert!(has_text(&mut app, "http://127.0.0.1:15790"));
        button_for(&mut app, |a| matches!(a, Action::CopyPrompt));
        // The first answer, then the next one about 2 s later (not every frame).
        assert!(
            until(&mut app, Duration::from_secs(30), |a| has_text(
                a,
                &status_line(15790, 1)
            )),
            "{:?}",
            texts(&mut app)
        );
        let first = Instant::now();
        assert!(
            until(&mut app, Duration::from_secs(30), |a| has_text(
                a,
                &status_line(15790, 2)
            )),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            first.elapsed() >= Duration::from_millis(1500),
            "polled after {:?}",
            first.elapsed()
        );
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn stop_saves_and_returns_to_resume() {
        let d = tmp("d-stop");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        fake_spawner(&mut app, vec![served(15791, &prompt, &marker, 0)]);
        let m = marker.clone();
        fake_control(&mut app, move |port| {
            assert_eq!(port, 15791);
            // The save, then the exit.
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 77, "saved": "moor.world.json"}))
        });
        start_new_detached(&mut app);
        assert!(until(&mut app, Duration::from_secs(30), |a| {
            has_text(a, &status_line(15791, 1))
        }));
        let stop = button_for(&mut app, |a| matches!(a, Action::StopDetached(_)));
        press(&mut app, stop);
        assert!(
            until(&mut app, Duration::from_secs(30), |a| screen(a)
                == Screen::ExplorerResume),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_none()
        );
        let info = texts(&mut app);
        assert!(
            info.iter()
                .any(|t| t == "Wren Ashdown stopped at tick 77 and was saved."),
            "{info:?}"
        );
        assert_eq!(app.world().resource::<ResumeInfo>().0, None, "shown once");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn open_in_window_aborts_on_failed_save() {
        let d = tmp("d-abort");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        let calls = fake_spawner(&mut app, vec![served(15792, &prompt, &marker, 0)]);
        fake_control(&mut app, |_| {
            Err("the save failed: disk full; the session keeps running".into())
        });
        start_new_detached(&mut app);
        assert!(until(&mut app, Duration::from_secs(30), |a| {
            has_text(a, &status_line(15792, 1))
        }));
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        press(&mut app, open);
        assert!(
            until(&mut app, Duration::from_secs(30), |a| has_text(
                a,
                "Open in window aborted: the save failed: disk full"
            )),
            "{:?}",
            texts(&mut app)
        );
        // Still detached and tracked; no window was started.
        assert_eq!(screen(&app), Screen::ExplorerDetached);
        assert!(has_text(&mut app, "the world keeps running detached"));
        assert!(!app.world().contains_resource::<Running>());
        assert_eq!(calls.lock().unwrap().len(), 1, "only the first start");
        let world = app.world().resource::<DetachedWorlds>();
        assert!(
            world
                .get("Wren Ashdown")
                .is_some_and(DetachedWorld::is_running)
        );
        // Its buttons work again.
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        assert!(!app.world().entity(open).contains::<InteractionDisabled>());
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The stop was accepted but the process exited with an error: the
    /// world restarts detached from the last save, on the same port.
    #[test]
    fn open_in_window_restarts_a_world_that_died_at_stop() {
        let d = tmp("d-died");
        let mut app = menu(&d);
        let (prompt, m1, m2) = (prompt_file(&d), d.join("stop-1"), d.join("stop-2"));
        let calls = fake_spawner(
            &mut app,
            vec![
                Plan {
                    stderr: "ERROR npcctl serve: cannot write the save\n".into(),
                    ..served(15793, &prompt, &m1, 101)
                },
                served(15793, &prompt, &m2, 0),
            ],
        );
        let m = m1.clone();
        fake_control(&mut app, move |_| {
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 5}))
        });
        start_new_detached(&mut app);
        assert!(until(&mut app, Duration::from_secs(30), |a| {
            has_text(a, &status_line(15793, 1))
        }));
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        press(&mut app, open);
        assert!(
            until(&mut app, Duration::from_secs(30), |_| calls
                .lock()
                .unwrap()
                .len()
                == 2),
            "{:?}",
            texts(&mut app)
        );
        let (stem, args) = calls.lock().unwrap()[1].clone();
        assert_eq!(stem, "npcctl", "restarted detached, not the window");
        assert!(
            args.windows(2).any(|w| w == ["--port", "15793"]),
            "{args:?}"
        );
        assert!(args.contains(&"--resume".to_owned()), "{args:?}");
        assert!(until(&mut app, Duration::from_secs(30), |a| has_text(
            a,
            "restarted detached from the last save"
        )));
        assert!(has_text(&mut app, "exited with code 101"));
        assert_eq!(screen(&app), Screen::ExplorerDetached);
        release(&[&m1, &m2]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn open_in_window_falls_back_when_port_busy() {
        let d = tmp("d-busy");
        let mut app = menu(&d);
        let (prompt, m1, m2) = (prompt_file(&d), d.join("stop-1"), d.join("stop-2"));
        let calls = fake_spawner(
            &mut app,
            vec![
                served(15794, &prompt, &m1, 0),
                // The window: the port stayed taken.
                Plan {
                    code: 4,
                    stdout: String::new(),
                    stderr: "actor_testbed: port 15794 is still in use\n".into(),
                    until: None,
                },
                served(15794, &prompt, &m2, 0),
            ],
        );
        let (m, save) = (
            m1.clone(),
            d.join("agents")
                .join("wren-ashdown")
                .join("moor.world.json"),
        );
        fake_control(&mut app, move |_| {
            // A newer save, then the exit.
            std::fs::write(&save, "{}").unwrap();
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 9}))
        });
        start_new_detached(&mut app);
        assert!(until(&mut app, Duration::from_secs(30), |a| {
            has_text(a, &status_line(15794, 1))
        }));
        std::fs::create_dir_all(d.join("agents").join("wren-ashdown")).unwrap();
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        press(&mut app, open);
        assert!(
            until(&mut app, Duration::from_secs(30), |_| calls
                .lock()
                .unwrap()
                .len()
                == 3),
            "{:?}",
            texts(&mut app)
        );
        let calls = calls.lock().unwrap().clone();
        assert_eq!(calls[1].0, "actor_testbed");
        assert!(
            calls[1]
                .1
                .windows(2)
                .any(|w| w == ["--exact-port", "15794"])
        );
        assert_eq!(calls[2].0, "npcctl");
        assert!(calls[2].1.windows(2).any(|w| w == ["--port", "15794"]));
        assert!(calls[2].1.contains(&"--wait-port".to_owned()));
        assert!(
            until(&mut app, Duration::from_secs(30), |a| screen(a)
                == Screen::ExplorerDetached
                && has_text(a, "port 15794 is still in use")),
            "{:?}",
            texts(&mut app)
        );
        assert!(has_text(&mut app, "runs detached again on port 15794"));
        release(&[&m1, &m2]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A window closed with "keep it running detached" (or F9) prints its
    /// detached host: the menu tracks it and shows its Detached screen.
    #[test]
    fn a_window_that_moved_to_detached_is_tracked() {
        let d = tmp("d-moved");
        let mut app = menu(&d);
        let moved = testbed_content::explorer::host::DetachedLine {
            // Alive, and never stopped by this test.
            pid: std::process::id(),
            port: 15796,
            agent: "Wren Ashdown".into(),
            world: "Moor".into(),
        };
        let calls = fake_spawner(
            &mut app,
            vec![Plan {
                code: 0,
                stdout: format!("{}\n", moved.line()),
                stderr: String::new(),
                until: None,
            }],
        );
        fake_control(&mut app, |_| Err("not in this test".into()));
        let entry = Entry::ExplorerResume {
            agent: "Wren Ashdown".into(),
            choice: None,
            mode: RunMode::Window,
        };
        run::start_game(app.world_mut(), entry);
        assert_eq!(calls.lock().unwrap()[0].0, "actor_testbed");
        assert!(
            until(&mut app, Duration::from_secs(30), |a| screen(a)
                == Screen::ExplorerDetached),
            "{:?}",
            texts(&mut app)
        );
        let w = app
            .world()
            .resource::<DetachedWorlds>()
            .get("Wren Ashdown")
            .unwrap();
        assert_eq!((w.port, w.pid()), (15796, Some(std::process::id())));
        assert!(until(&mut app, Duration::from_secs(30), |a| has_text(
            a,
            "moved here from the window: running detached on port 15796"
        )));
        assert!(until(&mut app, Duration::from_secs(30), |a| has_text(
            a,
            &status_line(15796, 1)
        )));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A `run.json` whose process is clearly gone is deleted by the check
    /// and its row is a plain one again.
    #[test]
    fn resume_drops_a_stale_run_file() {
        let d = tmp("d-stale");
        let mut app = menu(&d);
        let agents = d.join("agents");
        let wren = create_agent(&agents, "Wren Ashdown", "Moor", "2026-10-26").unwrap();
        std::fs::write(
            wren.save("Moor"),
            r#"{"header":{"saved":"2026-10-26T01:00:00Z"}}"#,
        )
        .unwrap();
        // A pid that was ours and has exited.
        let mut gone = test_child_until(Some(0), "", "", None)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = gone.id();
        gone.wait().unwrap();
        RunFile {
            pid,
            port: 15798,
            mode: RunMode::Detached,
            started: "2026-10-26T01:00:00Z".into(),
            agent: "Wren Ashdown".into(),
            world: "Moor".into(),
        }
        .write(&wren)
        .unwrap();
        run::set_screen(app.world_mut(), Screen::ExplorerResume);
        assert!(
            until(&mut app, Duration::from_secs(60), |a| has_text(
                a,
                "Moor  -  last saved"
            )),
            "{:?}",
            texts(&mut app)
        );
        assert!(!RunFile::path(&wren).exists(), "the stale run.json is gone");
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An Open in window pressed on a Resume row that aborts shows its
    /// reason on the world's Detached screen.
    #[test]
    fn an_abort_from_a_resume_row_shows_its_reason() {
        let d = tmp("d-row-abort");
        let mut app = menu(&d);
        let agents = d.join("agents");
        let wren = create_agent(&agents, "Wren Ashdown", "Moor", "2026-10-26").unwrap();
        RunFile {
            pid: std::process::id(),
            port: 15799,
            mode: RunMode::Detached,
            started: "2026-10-26T01:00:00Z".into(),
            agent: "Wren Ashdown".into(),
            world: "Moor".into(),
        }
        .write(&wren)
        .unwrap();
        fake_control(&mut app, |_| Err("the save failed: disk full".into()));
        run::set_screen(app.world_mut(), Screen::ExplorerResume);
        assert!(until(&mut app, Duration::from_secs(60), |a| has_text(
            a,
            "running detached on port 15799"
        )));
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        press(&mut app, open);
        assert!(
            until(&mut app, Duration::from_secs(30), |a| screen(a)
                == Screen::ExplorerDetached
                && has_text(
                    a,
                    "Open in window aborted: the save failed: disk full"
                )),
            "{:?}",
            texts(&mut app)
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Quit while an Open in window or Stop is in flight waits for it on
    /// the Quitting screen (Task 29), then quits once the world is gone.
    #[test]
    fn quit_waits_for_a_stop_in_flight() {
        let d = tmp("d-quit");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        fake_spawner(&mut app, vec![served(15800, &prompt, &marker, 0)]);
        // A stop that takes its time.
        let m = marker.clone();
        fake_control(&mut app, move |_| {
            std::thread::sleep(Duration::from_millis(1500));
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 3}))
        });
        start_new_detached(&mut app);
        assert!(until(&mut app, Duration::from_secs(30), |a| {
            has_text(a, &status_line(15800, 1))
        }));
        let stop = button_for(&mut app, |a| matches!(a, Action::StopDetached(_)));
        press(&mut app, stop);
        assert!(app.world().resource::<DetachedWorlds>().busy());
        crate::screens::apply_action(app.world_mut(), Action::Quit);
        app.update();
        app.update();
        assert!(
            app.world().resource::<Messages<AppExit>>().is_empty(),
            "not while the stop runs"
        );
        assert_eq!(screen(&app), Screen::Quitting);
        assert!(
            has_text(&mut app, "an Open in window or Stop is in progress"),
            "{:?}",
            texts(&mut app)
        );
        // Once the stop ended (the world is gone), the menu quits.
        assert!(
            until(&mut app, Duration::from_secs(30), |a| !a
                .world()
                .resource::<Messages<AppExit>>()
                .is_empty()),
            "{:?}",
            texts(&mut app)
        );
        assert!(has_text(&mut app, "stopped at tick 3 and saved"));
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_found_world_stops_clean_only_with_a_newer_save() {
        assert!(exit_was_clean(Some(0), false, false), "our child, code 0");
        assert!(!exit_was_clean(Some(101), false, true));
        assert!(!exit_was_clean(None, false, true), "our child, no code");
        assert!(exit_was_clean(None, true, true), "found pid, newer save");
        assert!(!exit_was_clean(None, true, false), "found pid, old save");
    }

    #[test]
    fn resume_list_adopts_running_worlds() {
        let d = tmp("d-adopt");
        let mut app = menu(&d);
        let agents = d.join("agents");
        let wren = create_agent(&agents, "Wren Ashdown", "Moor", "2026-10-26").unwrap();
        std::fs::write(
            wren.save("Moor"),
            r#"{"header":{"saved":"2026-10-26T01:00:00Z"}}"#,
        )
        .unwrap();
        let brin = create_agent(&agents, "Brin Marlow", "Fen", "2026-10-26").unwrap();
        std::fs::write(
            brin.save("Fen"),
            r#"{"header":{"saved":"2026-10-26T01:00:00Z"}}"#,
        )
        .unwrap();
        // Wren runs detached (this process stands in for npcctl: alive, and
        // never stopped by the test), Brin in a window.
        for (dir, agent, world, port, mode) in [
            (&wren, "Wren Ashdown", "Moor", 15795, RunMode::Detached),
            (&brin, "Brin Marlow", "Fen", 15797, RunMode::Window),
        ] {
            RunFile {
                pid: std::process::id(),
                port,
                mode,
                started: "2026-10-26T01:00:00Z".into(),
                agent: agent.into(),
                world: world.into(),
            }
            .write(dir)
            .unwrap();
        }
        fake_control(&mut app, |_| Err("not in this test".into()));
        run::set_screen(app.world_mut(), Screen::ExplorerResume);
        app.update();
        app.update();
        // The pids are asked off the frame: the rows say so meanwhile.
        assert!(
            has_text(&mut app, "Moor  -  checking whether it runs...")
                || has_text(&mut app, "running detached on port 15795"),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            until(&mut app, Duration::from_secs(60), |a| has_text(
                a,
                "Moor  -  running detached on port 15795 - Open / Stop"
            ) && has_text(
                a,
                "Fen  -  running in a window"
            )),
            "{:?}",
            texts(&mut app)
        );
        // Open and Stop for Wren; Brin cannot be started twice.
        button_for(
            &mut app,
            |a| matches!(a, Action::OpenInWindow(n) if n == "Wren Ashdown"),
        );
        button_for(
            &mut app,
            |a| matches!(a, Action::StopDetached(n) if n == "Wren Ashdown"),
        );
        let mut q = app
            .world_mut()
            .query::<(&Action, Has<InteractionDisabled>)>();
        let brin_launch: Vec<bool> = q
            .iter(app.world())
            .filter_map(|(a, off)| match a {
                Action::Launch(Entry::ExplorerResume { agent, .. }) if agent == "Brin Marlow" => {
                    Some(off)
                }
                _ => None,
            })
            .collect();
        assert!(brin_launch.iter().all(|off| *off), "{brin_launch:?}");
        // Adopted: tracked by pid, its port and world from run.json.
        let worlds = app.world().resource::<DetachedWorlds>();
        let w = worlds.get("Wren Ashdown").expect("adopted");
        assert_eq!((w.port, w.world.as_str()), (15795, "Moor"));
        assert_eq!(w.pid(), Some(std::process::id()));
        assert!(worlds.get("Brin Marlow").is_none());
        // The row opens its Detached screen.
        let watch = button_for(
            &mut app,
            |a| matches!(a, Action::Watch(n) if n == "Wren Ashdown"),
        );
        press(&mut app, watch);
        app.update();
        assert_eq!(screen(&app), Screen::ExplorerDetached);
        assert!(until(&mut app, Duration::from_secs(30), |a| has_text(
            a,
            &status_line(15795, 1)
        )));
        let _ = ResumeNotice::default();
        let _ = std::fs::remove_dir_all(&d);
    }
}
