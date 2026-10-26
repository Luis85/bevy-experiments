//! Child processes: a game (the menu window hides until it exits) or a
//! `cargo build` (the window stays and says "building...", with Cancel).
//!
//! A game's stderr is piped through a [`StderrTail`]: every line still
//! reaches the menu's own stderr (the terminal shows it), and the last
//! non-empty one is the reason Main shows when the game exits non-zero. An
//! explorer window's stdout is read by a [`StdoutWatch`] for the line it
//! prints when it moved its world to detached (the menu then tracks that
//! world, [`crate::detached`]). A detached entry (`npcctl serve`) is no
//! game: the menu stays and shows the Detached screen.

use std::io::{BufRead as _, BufReader, Read, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use testbed_content::explorer::host::DetachedLine;

use crate::detached::{self, DetachedWorlds, Notice};
use crate::explorer::SaveCheck;
use crate::launch::{self, Entry};
use crate::screens::{MainNotice, Missing, Screen};
use crate::{AppSettings, Paths};

/// Starts a child from its prepared command (a test stands in for the
/// games and `npcctl`; builds always run `cargo`).
#[derive(Resource, Clone)]
pub struct Spawner(pub Arc<dyn Fn(Command) -> std::io::Result<Child> + Send + Sync>);

impl Default for Spawner {
    fn default() -> Self {
        Self(Arc::new(|mut cmd: Command| cmd.spawn()))
    }
}

/// What a running child is.
#[derive(Clone, Debug)]
pub enum Job {
    /// A game started for `entry`; the menu window is hidden.
    Game(Entry),
    /// `cargo build -p <stem>` for `entry`, which starts when it succeeds.
    Build { entry: Entry, stem: &'static str },
}

/// The child the Running screen waits for. Dropped while a build still runs
/// (Cancel, or the menu closing), it kills the build; a game is left alone.
#[derive(Resource, Debug)]
pub struct Running {
    pub child: Child,
    pub job: Job,
    /// The child's stderr, when it was piped.
    stderr: Option<StderrTail>,
    /// An explorer window's stdout, when it was piped.
    stdout: Option<StdoutWatch>,
}

impl Running {
    /// Waits on `child`; a piped stderr is read by a [`StderrTail`], a piped
    /// stdout by a [`StdoutWatch`].
    pub fn new(mut child: Child, job: Job) -> Self {
        let stderr = child.stderr.take().map(StderrTail::spawn);
        let stdout = child.stdout.take().map(StdoutWatch::spawn);
        Self {
            child,
            job,
            stderr,
            stdout,
        }
    }

    /// The line of an explorer window that moved its world to detached
    /// (call after it exited; waits at most [`TAIL_GRACE`]).
    pub fn moved(&self) -> Option<DetachedLine> {
        self.stdout.as_ref().and_then(StdoutWatch::moved)
    }

    /// The Running screen's title.
    pub fn heading(&self) -> &'static str {
        match self.job {
            Job::Game(_) => "Running",
            Job::Build { .. } => "Building",
        }
    }

    /// The Running screen's line.
    pub fn status(&self) -> String {
        match &self.job {
            Job::Game(entry) => format!("{} is running", entry.title()),
            Job::Build { stem, .. } => {
                format!("building {stem}... (cargo writes its progress to the terminal)")
            }
        }
    }

    /// A build (Cancel and closing the menu stop it).
    pub fn is_build(&self) -> bool {
        matches!(self.job, Job::Build { .. })
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.is_build() {
            kill(&mut self.child);
        }
    }
}

/// Why [`kill_adopted`] did not kill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KillError {
    /// The pid is no longer the world's process (reused by another
    /// program, or started after the world's `run.json`): never killed.
    NotOurs(String),
    /// The kill (or the identity check) could not run or failed.
    Failed(String),
}

/// The programs an adopted world's pid may belong to.
const WORLD_PROGRAMS: [&str; 2] = ["npcctl", "actor_testbed"];

/// How much later than its `run.json`'s `started` (whole seconds) a world's
/// process may seem to have started (rounding, clock reads).
const START_SLACK: Duration = Duration::from_secs(5);

/// Kills `pid`, a process the menu adopted from a `run.json` (not its own
/// child), once it checked the pid is still that world's: its program is
/// `npcctl` or `actor_testbed`, and it started no later than `started` (the
/// `run.json`'s, when known): a process that started after the world wrote
/// its `run.json` cannot be that world's. A pid the OS reused is never
/// killed. Background threads only (it asks the OS).
///
/// # Errors
/// [`KillError::NotOurs`] (not killed), or [`KillError::Failed`].
pub fn kill_adopted(pid: u32, started: Option<SystemTime>) -> Result<(), KillError> {
    let Some((name, began)) = process_identity(pid).map_err(KillError::Failed)? else {
        // Already gone: nothing to kill.
        return Ok(());
    };
    let stem = Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !WORLD_PROGRAMS.contains(&stem.as_str()) {
        return Err(KillError::NotOurs(format!(
            "pid {pid} is now {name}, not the world's npcctl or actor_testbed"
        )));
    }
    if let (Some(started), Some(began)) = (started, began)
        && began > started + START_SLACK
    {
        return Err(KillError::NotOurs(format!(
            "pid {pid} ({name}) started after the world's run.json was written: another process"
        )));
    }
    kill_pid(pid).map_err(KillError::Failed)
}

/// The program name of process `pid` and, when the OS says it cheaply, its
/// start time; `Ok(None)` when no such process runs.
///
/// # Errors
/// The question could not be asked, or its answer not read.
pub fn process_identity(pid: u32) -> Result<Option<(String, Option<SystemTime>)>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("cannot run tasklist: {e}"))?;
        if !out.status.success() {
            return Err(format!("tasklist failed for pid {pid}"));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let want = pid.to_string();
        // `"image","pid",..` (file names cannot contain `"`); anything else
        // is tasklist's localized "no tasks" line.
        let row = text.lines().find_map(|l| {
            let row = l.trim().strip_prefix('"')?.strip_suffix('"')?;
            let fields: Vec<&str> = row.split("\",\"").collect();
            (fields.get(1) == Some(&want.as_str())).then(|| fields[0].to_owned())
        });
        let Some(name) = row else {
            return Ok(None);
        };
        // The start time, best effort (PowerShell answers in about a second).
        let script = format!(
            "(Get-Process -Id {pid}).StartTime.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')"
        );
        let began = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                testbed_content::explorer::host::parse_rfc3339(&String::from_utf8_lossy(&o.stdout))
            });
        Ok(Some((name, began)))
    }
    #[cfg(not(windows))]
    {
        let out = Command::new("ps")
            .args(["-o", "etimes=,comm=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| format!("cannot run ps: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let Some(line) = text.lines().map(str::trim).find(|l| !l.is_empty()) else {
            return Ok(None);
        };
        let (secs, name) = line.split_once(char::is_whitespace).unwrap_or(("", line));
        let began = secs
            .trim()
            .parse::<u64>()
            .ok()
            .and_then(|s| SystemTime::now().checked_sub(Duration::from_secs(s)));
        Ok(Some((name.trim().to_owned(), began)))
    }
}

/// Kills process `pid` by its pid, never by image name (callers check it is
/// theirs first: [`kill_adopted`]). `taskkill /PID N /F` on Windows, `kill
/// -9 N` on Unix. Background threads only.
///
/// # Errors
/// The kill could not run or failed (its output).
fn kill_pid(pid: u32) -> Result<(), String> {
    if pid == 0 || pid == std::process::id() {
        return Err(format!("refusing to kill pid {pid}"));
    }
    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    };
    #[cfg(not(windows))]
    let out = Command::new("kill").args(["-9", &pid.to_string()]).output();
    let out = out.map_err(|e| format!("cannot run the kill: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stderr);
        Err(format!("kill of pid {pid} failed: {}", text.trim()))
    }
}

/// Kills and reaps `child` when it still runs.
pub fn kill(child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        if let Err(e) = child.kill() {
            warn!("game_menu: cannot stop child {}: {e}", child.id());
        }
        let _ = child.wait();
    }
}

/// How long the tail of an exited child's stderr is waited for (a
/// grandchild holding the pipe must not stall the menu).
const TAIL_GRACE: Duration = Duration::from_millis(500);

/// A child's stderr, read on a thread: each line is copied to the menu's
/// stderr as it comes, the last non-empty one ([`launch::reason_line`]) is
/// kept.
#[derive(Debug)]
pub struct StderrTail {
    last: Arc<Mutex<Option<String>>>,
    /// Signalled (or dropped) when the stream ends.
    done: Mutex<Receiver<()>>,
}

impl StderrTail {
    /// Starts reading `stream`.
    pub fn spawn(stream: impl Read + Send + 'static) -> Self {
        let last = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel();
        let keep = Arc::clone(&last);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let _ = std::io::stderr().lock().write_all(&line);
                        if let Some(reason) = launch::reason_line(&String::from_utf8_lossy(&line))
                            && let Ok(mut l) = keep.lock()
                        {
                            *l = Some(reason);
                        }
                    }
                }
            }
            let _ = tx.send(());
        });
        Self {
            last,
            done: Mutex::new(rx),
        }
    }

    /// The last non-empty line, once the stream ended (waited for at most
    /// [`TAIL_GRACE`]; call it after the child exited).
    pub fn last_line(&self) -> Option<String> {
        if let Ok(done) = self.done.lock() {
            let _ = done.recv_timeout(TAIL_GRACE);
        }
        self.last.lock().ok().and_then(|l| l.clone())
    }
}

/// An explorer window's stdout, read on a thread: each line is copied to the
/// menu's stdout, except the [`DetachedLine`] the window prints when it
/// moved its world to detached, which is kept.
#[derive(Debug)]
pub struct StdoutWatch {
    moved: Arc<Mutex<Option<DetachedLine>>>,
    done: Mutex<Receiver<()>>,
}

impl StdoutWatch {
    /// Starts reading `stream`.
    pub fn spawn(stream: impl Read + Send + 'static) -> Self {
        let moved = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel();
        let keep = Arc::clone(&moved);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let text = String::from_utf8_lossy(&line);
                        if let Some(d) = DetachedLine::parse(&text) {
                            if let Ok(mut m) = keep.lock() {
                                *m = Some(d);
                            }
                        } else {
                            let _ = std::io::stdout().lock().write_all(&line);
                        }
                    }
                }
            }
            let _ = tx.send(());
        });
        Self {
            moved,
            done: Mutex::new(rx),
        }
    }

    /// The window's "moved to detached" line, once the stream ended (waited
    /// for at most [`TAIL_GRACE`]; call it after the child exited).
    pub fn moved(&self) -> Option<DetachedLine> {
        if let Ok(done) = self.done.lock() {
            let _ = done.recv_timeout(TAIL_GRACE);
        }
        self.moved.lock().ok().and_then(|m| m.clone())
    }
}

/// A child is running or a save check is in flight: a second launch, build
/// or check is ignored (two activations in one frame would otherwise orphan
/// the first child).
pub fn busy(world: &World) -> bool {
    crate::quit::active(world)
        || world.contains_resource::<Running>()
        || world.contains_resource::<SaveCheck>()
        || world
            .get_resource::<DetachedWorlds>()
            .is_some_and(DetachedWorlds::busy)
}

/// Starts `entry`: the game when its executable is next to the menu, else
/// the Missing screen. A plain explorer Resume checks the save first
/// ([`crate::explorer::start_check`]). Does nothing while [`busy`].
pub fn launch(world: &mut World, entry: Entry) {
    if busy(world) {
        return;
    }
    if let Entry::ExplorerResume {
        agent,
        choice: None,
        mode,
    } = &entry
    {
        crate::explorer::start_check(world, agent.clone(), *mode);
        return;
    }
    start_game(world, entry);
}

/// Starts `entry`'s game without [`launch`]'s save check (the check's `ok`
/// path); the Missing screen when it is not built. Does nothing while
/// [`busy`].
pub fn start_game(world: &mut World, entry: Entry) {
    if busy(world) {
        return;
    }
    let settings = world.resource::<AppSettings>().0.clone();
    let (stem, args) = launch::command_for(&entry, &settings);
    let exe_dir = world.resource::<Paths>().exe_dir.clone();
    let Some(exe) = launch::find_exe(stem, &exe_dir) else {
        world.insert_resource(Missing::new(stem, entry, &exe_dir));
        set_screen(world, Screen::Missing);
        return;
    };
    if entry.is_detached() {
        info!(
            "game_menu: starting {} {}",
            exe.display(),
            launch::command_line(&args)
        );
        detached::start(world, entry, &exe, &args);
        return;
    }
    info!(
        "game_menu: starting {} {}",
        exe.display(),
        launch::command_line(&args)
    );
    let mut cmd = Command::new(&exe);
    cmd.args(&args).stderr(Stdio::piped());
    if entry.agent().is_some() {
        // The line of a window that moved its world to detached.
        cmd.stdout(Stdio::piped());
    }
    let spawner = world.get_resource::<Spawner>().cloned().unwrap_or_default();
    match (spawner.0)(cmd) {
        Ok(child) => {
            world.insert_resource(Running::new(child, Job::Game(entry)));
            show_window(world, false);
            set_screen(world, Screen::Running);
        }
        Err(e) => {
            world.insert_resource(MainNotice::error(format!(
                "cannot start {}: {e}",
                exe.display()
            )));
            set_screen(world, Screen::Main);
        }
    }
}

/// Runs `cargo build -p <stem>` in `root` for the Missing screen's entry
/// (its stderr stays the terminal's: cargo's progress bar and colours).
/// Does nothing while [`busy`].
pub fn build(world: &mut World, entry: Entry, stem: &'static str, cargo: &Path, root: &Path) {
    if busy(world) {
        return;
    }
    let exe_dir = world.resource::<Paths>().exe_dir.clone();
    let args = launch::build_args(stem, &exe_dir);
    info!(
        "game_menu: {} (in {})",
        launch::command_line(&args),
        root.display()
    );
    match Command::new(cargo)
        .args(&args[1..])
        .current_dir(root)
        .spawn()
    {
        Ok(child) => {
            world.insert_resource(Running::new(child, Job::Build { entry, stem }));
            set_screen(world, Screen::Running);
        }
        Err(e) => {
            if let Some(mut missing) = world.get_resource_mut::<Missing>() {
                missing.error = Some(format!("cannot run cargo: {e}"));
            }
            set_screen(world, Screen::Missing);
        }
    }
}

/// The Building screen's Cancel: kills the build and goes back to the
/// Missing screen ("the build was cancelled"). Does nothing for a game.
pub fn cancel_build(world: &mut World) {
    if !world
        .get_resource::<Running>()
        .is_some_and(Running::is_build)
    {
        return;
    }
    // Dropping it kills and reaps cargo.
    world.remove_resource::<Running>();
    if let Some(mut missing) = world.get_resource_mut::<Missing>() {
        missing.error = Some("the build was cancelled".to_owned());
    }
    set_screen(world, Screen::Missing);
}

/// Stops a running build when the menu exits (its window closed): the
/// build would otherwise run on without anyone waiting for it.
pub fn stop_build_on_exit(world: &mut World) {
    let exiting = !world.resource::<Messages<AppExit>>().is_empty();
    if exiting
        && world
            .get_resource::<Running>()
            .is_some_and(Running::is_build)
    {
        world.remove_resource::<Running>();
    }
}

/// Polls the child; on exit shows the window again and moves on: a game
/// back to Main (a non-zero exit as Main's error line, with the last line
/// the game wrote to stderr), a successful build starts its game, a failed
/// one back to Missing with the exit code.
pub fn poll_child(world: &mut World) {
    let Some(mut running) = world.get_resource_mut::<Running>() else {
        return;
    };
    let status = match running.child.try_wait() {
        Ok(None) => return,
        Ok(Some(status)) => Ok(status.code()),
        Err(e) => Err(e.to_string()),
    };
    let Some(running) = world.remove_resource::<Running>() else {
        return;
    };
    match &running.job {
        Job::Game(entry) => {
            show_window(world, true);
            if let Ok(code) = status
                && explorer_window_done(world, entry, code, &running)
            {
                return;
            }
            let notice = match status {
                Ok(code) => {
                    let why = running.stderr.as_ref().and_then(StderrTail::last_line);
                    launch::exit_error(&entry.title(), code, why.as_deref()).map(MainNotice::error)
                }
                Err(e) => Some(MainNotice::error(format!("{}: {e}", entry.title()))),
            };
            world.insert_resource(notice.unwrap_or_default());
            set_screen(world, Screen::Main);
        }
        Job::Build { entry, stem } => match status {
            Ok(Some(0)) => {
                let exe_dir = world.resource::<Paths>().exe_dir.clone();
                if launch::find_exe(stem, &exe_dir).is_some() {
                    launch(world, entry.clone());
                } else {
                    // Built into the workspace's target folder, which is not
                    // the menu's (a copied or installed menu).
                    if let Some(mut missing) = world.get_resource_mut::<Missing>() {
                        missing.error = Some(format!(
                            "built, but {stem} is still not in {}",
                            exe_dir.display()
                        ));
                    }
                    set_screen(world, Screen::Missing);
                }
            }
            other => {
                let why = match other {
                    Ok(code) => launch::exit_error("cargo build", code, None).unwrap_or_default(),
                    Err(e) => e,
                };
                if let Some(mut missing) = world.get_resource_mut::<Missing>() {
                    missing.error = Some(format!("{why}; see the terminal for {stem}'s errors"));
                }
                set_screen(world, Screen::Missing);
            }
        },
    }
}

/// An explorer window exited: one that could not take its port back from a
/// detached world (`--exact-port`: exit 4, or 2 when the old host stayed)
/// restarts the world detached on that port (spec §8); one that moved its
/// world to detached (its [`DetachedLine`]) hands it to the Detached
/// screen. `false`: neither (back to Main, as for any game).
fn explorer_window_done(
    world: &mut World,
    entry: &Entry,
    code: Option<i32>,
    running: &Running,
) -> bool {
    if let Entry::ExplorerMove { agent, port } = entry
        && matches!(code, Some(2 | 4))
    {
        let why = running.stderr.as_ref().and_then(StderrTail::last_line);
        let head = format!("the window could not open the world on port {port}");
        let text = format!(
            "{}: it runs detached again on port {port}",
            launch::with_reason(head, why.as_deref())
        );
        start_game(
            world,
            Entry::ExplorerRestart {
                agent: agent.clone(),
                port: *port,
            },
        );
        detached::set_notice(world, agent, Notice::error(text));
        return true;
    }
    let Some(moved) = running.stdout.as_ref().and_then(StdoutWatch::moved) else {
        return false;
    };
    let (agent, port) = (moved.agent.clone(), moved.port);
    detached::adopt_moved(world, moved);
    detached::set_notice(
        world,
        &agent,
        Notice::info(format!(
            "moved here from the window: running detached on port {port}"
        )),
    );
    true
}

/// Hides or shows the menu window (hidden, [`crate::power`] lets the menu
/// wake only to poll); shown again, it asks for focus.
pub(crate) fn show_window(world: &mut World, visible: bool) {
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    if let Ok(mut window) = windows.single_mut(world) {
        window.visible = visible;
        if visible {
            window.focused = true;
        }
    }
}

pub fn set_screen(world: &mut World, screen: Screen) {
    world.resource_mut::<NextState<Screen>>().set(screen);
}

/// Where the menu looks for the games and whether it can build them.
#[derive(Resource, Clone, Debug)]
pub struct BuildTools {
    /// `cargo` on `PATH`.
    pub cargo: Option<PathBuf>,
    /// The workspace the menu was built in.
    pub root: Option<PathBuf>,
}

impl BuildTools {
    pub fn detect(exe_dir: &Path) -> Self {
        Self {
            cargo: std::env::var_os("PATH").and_then(|p| launch::find_on_path("cargo", &p)),
            root: launch::workspace_root(exe_dir),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_tail_keeps_the_last_reason_line() {
        let text = "INFO starting\r\n\u{1b}[31mERROR\u{1b}[0m the world file is gone\r\n\r\n\
                    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n";
        let tail = StderrTail::spawn(std::io::Cursor::new(text.as_bytes().to_vec()));
        assert_eq!(
            tail.last_line().as_deref(),
            Some("ERROR the world file is gone")
        );
        let empty = StderrTail::spawn(std::io::empty());
        assert_eq!(empty.last_line(), None);
    }
}
