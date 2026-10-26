//! Quitting the game stops every process it started (plan Task 29: the
//! dev's decision "quitting the game stops all processes", which supersedes
//! spec G7's "a detached world outlives the menu" for the menu's Quit;
//! closing one game window still asks "Keep it running detached?").
//!
//! **Quit** (Main's button, the menu window's close button, or Ctrl+C on the
//! menu's console, see [`gate_exit`]) stops, in parallel on worker threads
//! (never on the frame):
//!
//! - every detached world the menu tracks (started by it, moved to detached
//!   from a window, or adopted from a live `run.json`): `session/stop` (the
//!   world saves first; asked again while it does not answer, e.g. a world
//!   still taking the port over from a window), then its exit is waited
//!   for; one that saved but still runs after [`QuitWait`] is killed by its
//!   pid ([`OpKind::Quit`]);
//! - the explorer window it started: `app/quit {pid}` over its BRP (the port
//!   from its `run.json`): it saves, removes `run.json` and exits 0, as when
//!   closed with "No". Once it answered it gets [`ANSWERED_GRACE`] more to
//!   exit; one that has not quit in time is killed through the menu's own
//!   child handle (and its `run.json` removed);
//! - the Village, a testbed scenario or the RTS units demo: nothing to
//!   save, killed through the menu's child handle at once (ruling R17: the
//!   menu passes them no `--remote`);
//! - a build or a save check: killed at once.
//!
//! An Open in window or Stop in flight is waited for first; its world is
//! then stopped like the others (no window opens, nothing restarts). A world
//! still starting detached is stopped once it is up, or killed through its
//! child handle when it is not up within [`QuitWait`] (its `run.json` is
//! removed).
//!
//! An adopted world (a pid from `run.json`, not a child) is killed only
//! after [`run::kill_adopted`] checked the pid still is that world's
//! process (`npcctl` / `actor_testbed`, not started after the `run.json`):
//! a reused pid is never killed; the world is forgotten and its stale
//! `run.json` removed.
//!
//! The **Quitting** screen lists each process and its state; the menu exits
//! once all are gone. A world that could not be stopped (its stop was
//! refused because its save failed, or it does not answer; the screen says
//! which) keeps the menu on that screen with the world and the reason:
//! **Quit anyway** kills it by its pid (its save stays at the last
//! autosave; its `run.json` is removed) and quits; **Back** returns to Main
//! and leaves it running. Back is not offered after Ctrl+C (Bevy's handler
//! asks to exit on every frame from then on); a second Ctrl+C exits the
//! menu at once and leaves running whatever was not stopped yet (worlds
//! still starting included).
//!
//! The menu touches only processes it started or adopted: never a world
//! whose `run.json` it did not adopt, and never an `npcctl mcp` server (it
//! belongs to the AI tool's session and exits with it).

use std::sync::Arc;
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use testbed_content::explorer::home::{AgentDir, slug};
use testbed_content::explorer::host::{RunFile, RunMode};

use crate::AppSettings;
use crate::detached::{self, Control, Controls, DetachedWorlds, OpKind, Proc, Rx};
use crate::explorer::SaveCheck;
use crate::run::{self, Job, KillError, Running};
use crate::screens::{Action, MainNotice, Screen, button};
use crate::ui::{self, ERROR, TEXT};

/// How long an explorer window gets to answer `app/quit` and exit, a world
/// starting detached to come up, and a world to answer `session/stop` and
/// exit, before it is killed.
pub const WINDOW_WAIT: Duration = Duration::from_secs(15);

/// How long a window that answered `app/quit` (late, maybe) still gets to
/// save and exit.
pub const ANSWERED_GRACE: Duration = Duration::from_secs(10);

/// How often a window's `app/quit` is tried again while it does not answer
/// (not listening yet, or its `run.json` not written yet).
const ASK_RETRY: Duration = Duration::from_millis(250);

/// [`WINDOW_WAIT`] in use (a test shortens it).
#[derive(Resource, Clone, Copy, Debug)]
pub struct QuitWait(pub Duration);

impl Default for QuitWait {
    fn default() -> Self {
        Self(WINDOW_WAIT)
    }
}

/// What one line of the Quitting screen stops.
#[derive(Clone, Debug, PartialEq, Eq)]
enum What {
    /// A detached world, by agent.
    World(String),
    /// The game window (the [`Running`] game).
    Window,
}

/// Where a process stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Not asked yet: why.
    Waiting(String),
    /// Asked (or being killed): what runs.
    Busy(String),
    /// Gone: how.
    Done(String),
    /// Its stop was refused (the save failed): it still runs.
    Refused(String),
}

impl State {
    fn is_final(&self) -> bool {
        matches!(self, Self::Done(_) | Self::Refused(_))
    }

    fn text(&self) -> &str {
        match self {
            Self::Waiting(t) | Self::Busy(t) | Self::Done(t) | Self::Refused(t) => t,
        }
    }
}

/// How Quit anyway's kill ended.
type KillDone = Result<Option<String>, (Proc, String)>;

/// One process the quit stops.
pub struct Item {
    /// Shown on the Quitting screen.
    pub name: String,
    what: What,
    pub pid: u32,
    pub state: State,
    /// When a window or a starting world is killed.
    deadline: Instant,
    /// A window's `app/quit` was sent (on its worker).
    asked: bool,
    ask: Option<Rx<Result<(), String>>>,
    /// Quit anyway's kill of a refused world: `Some` why it was not killed
    /// (a reused pid); the process back when the kill failed.
    kill: Option<Rx<KillDone>>,
}

impl Item {
    fn new(name: String, what: What, pid: u32, wait: Duration) -> Self {
        Self {
            name,
            what,
            pid,
            state: State::Waiting("waiting".into()),
            deadline: Instant::now() + wait,
            asked: false,
            ask: None,
            kill: None,
        }
    }

    fn is_world(&self, agent: &str, pid: Option<u32>) -> bool {
        matches!(&self.what, What::World(a) if slug(a) == slug(agent))
            && pid.is_none_or(|p| p == self.pid)
    }
}

/// The quit in progress (the Quitting screen's model).
#[derive(Resource)]
pub struct Quitting {
    pub items: Vec<Item>,
    /// Started (or joined) by Ctrl+C: no Back.
    pub ctrl_c: bool,
    /// Everything is gone: the menu's own `AppExit` passes [`gate_exit`].
    done: bool,
    wait: Duration,
}

impl Quitting {
    /// Nothing runs any more but what refused to stop.
    fn finished(&self) -> bool {
        self.items.iter().all(|i| i.state.is_final())
    }

    /// The worlds whose stop was refused.
    fn refused(&self) -> impl Iterator<Item = &Item> {
        self.items
            .iter()
            .filter(|i| matches!(i.state, State::Refused(_)))
    }

    /// Stopped on a refused stop: the Quitting screen offers Quit anyway.
    pub fn stuck(&self) -> bool {
        self.finished() && self.refused().next().is_some()
    }
}

/// A quit is in progress (no launch, and a finished Stop or Open in window
/// reports here instead of changing screens).
pub fn active(world: &World) -> bool {
    world.contains_resource::<Quitting>()
}

/// No game window and no detached world: the menu can exit at once.
fn nothing_to_stop(world: &World) -> bool {
    let game = world
        .get_resource::<Running>()
        .is_some_and(|r| !r.is_build());
    let worlds = world
        .get_resource::<DetachedWorlds>()
        .is_some_and(|w| w.iter().next().is_some());
    !game && !worlds
}

/// The Quitting screen and its systems.
pub struct QuitPlugin;

impl Plugin for QuitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuitWait>()
            .add_systems(OnEnter(Screen::Quitting), spawn_quitting)
            .add_systems(
                Update,
                (drive, refresh_quitting.run_if(in_state(Screen::Quitting)))
                    .chain()
                    .after(detached::track)
                    .run_if(resource_exists::<Quitting>),
            )
            .add_systems(
                Last,
                gate_exit
                    .after(bevy::window::ExitSystems)
                    .before(run::stop_build_on_exit),
            );
    }
}

/// Quit: kills a build or a save check, then exits at once when nothing
/// else runs; else starts stopping everything on the Quitting screen (see
/// the module docs). A second Quit while one runs does nothing.
pub fn start(world: &mut World, ctrl_c: bool) {
    if let Some(mut q) = world.get_resource_mut::<Quitting>() {
        q.ctrl_c |= ctrl_c;
        return;
    }
    // Dropping them kills them.
    world.remove_resource::<SaveCheck>();
    if world
        .get_resource::<Running>()
        .is_some_and(Running::is_build)
    {
        world.remove_resource::<Running>();
    }
    if nothing_to_stop(world) {
        world.write_message(AppExit::Success);
        return;
    }
    let wait = world
        .get_resource::<QuitWait>()
        .copied()
        .unwrap_or_default()
        .0;
    let mut q = Quitting {
        items: Vec::new(),
        ctrl_c,
        done: false,
        wait,
    };
    collect(world, &mut q);
    info!(
        "game_menu: quitting: stopping {}",
        q.items
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    world.insert_resource(q);
    run::show_window(world, true);
    run::set_screen(world, Screen::Quitting);
}

/// Adds what runs and is not listed yet: tracked detached worlds and the
/// game window.
fn collect(world: &World, q: &mut Quitting) {
    let wait = q.wait;
    if let Some(worlds) = world.get_resource::<DetachedWorlds>() {
        for w in worlds.iter() {
            let pid = w.pid().unwrap_or(0);
            if !q.items.iter().any(|i| i.is_world(&w.agent, Some(pid))) {
                q.items.push(Item::new(
                    format!("{}'s world (detached, pid {pid})", w.agent),
                    What::World(w.agent.clone()),
                    pid,
                    wait,
                ));
            }
        }
    }
    if let Some(running) = world.get_resource::<Running>()
        && let Job::Game(entry) = &running.job
    {
        let pid = running.child.id();
        if !q
            .items
            .iter()
            .any(|i| i.what == What::Window && i.pid == pid)
        {
            q.items.push(Item::new(
                format!("{} (window, pid {pid})", entry.title()),
                What::Window,
                pid,
                wait,
            ));
        }
    }
}

/// Every frame while quitting: each process one step on, newly started or
/// adopted ones listed, and the exit once all are gone.
fn drive(world: &mut World) {
    let control = world.resource::<Controls>().0.clone();
    let agents_dir = world.resource::<AppSettings>().0.agents_dir.clone();
    world.resource_scope(|world, mut q: Mut<Quitting>| {
        let wait = q.wait;
        for item in &mut q.items {
            match item.what.clone() {
                What::World(agent) => step_world(world, item, &agent, wait),
                What::Window => step_window(world, item, &control, &agents_dir, wait),
            }
        }
        collect(world, &mut q);
        if !q.done && q.finished() && q.refused().next().is_none() {
            q.done = true;
            info!("game_menu: quitting: everything stopped");
            world.write_message(AppExit::Success);
        }
    });
}

/// One step of a detached world (see the module docs).
fn step_world(world: &mut World, item: &mut Item, agent: &str, wait: Duration) {
    if let Some(rx) = &item.kill {
        match rx.try_recv() {
            Ok(Ok(None)) => {
                item.kill = None;
                detached::forget(world, agent);
                item.state = State::Done(format!(
                    "killed (pid {}); its save stays at the last autosave",
                    item.pid
                ));
            }
            Ok(Ok(Some(not_ours))) => {
                item.kill = None;
                detached::forget(world, agent);
                item.state = State::Done(format!(
                    "not killed: {not_ours}; its stale run.json was removed"
                ));
            }
            Ok(Err((proc, e))) => {
                item.kill = None;
                detached::give_back(world, agent, proc);
                item.state = State::Refused(format!("cannot kill pid {}: {e}", item.pid));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                item.kill = None;
                item.state = State::Refused("the kill did not finish".into());
            }
        }
        return;
    }
    if item.state.is_final() {
        return;
    }
    let found = world
        .resource::<DetachedWorlds>()
        .get(agent)
        .filter(|w| w.pid() == Some(item.pid))
        .map(|w| (w.is_running(), w.is_starting()));
    match found {
        None => item.state = State::Done("no longer running".into()),
        Some((true, _)) => {
            if matches!(item.state, State::Waiting(_))
                && detached::start_op(world, agent, OpKind::Quit)
            {
                item.state = State::Busy("saving, then stopping (session/stop)...".into());
            }
        }
        Some((_, true)) => {
            if Instant::now() >= item.deadline {
                if let Some(pid) = detached::kill_starting(world, agent) {
                    item.state = State::Done(format!(
                        "killed (pid {pid}): it did not come up within {} s",
                        wait.as_secs()
                    ));
                }
            } else {
                item.state = State::Waiting("starting: it is stopped once it is up".into());
            }
        }
        Some((false, false)) => {
            if matches!(item.state, State::Waiting(_)) {
                item.state = State::Waiting(
                    "an Open in window or Stop is in progress: waiting for it".into(),
                );
            }
        }
    }
}

/// Where an explorer window's `app/quit` goes: the port in its agent
/// folder's `run.json`.
struct Target(AgentDir);

/// One step of the game window (see the module docs).
fn step_window(
    world: &mut World,
    item: &mut Item,
    control: &Arc<dyn Control>,
    agents_dir: &std::path::Path,
    wait: Duration,
) {
    if item.state.is_final() {
        return;
    }
    let pid = item.pid;
    let exited = {
        let Some(mut running) = world
            .get_resource_mut::<Running>()
            .filter(|r| r.child.id() == pid)
        else {
            item.state = State::Done("exited".into());
            return;
        };
        match running.child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => Some(Ok(status.code())),
            Err(e) => Some(Err(e.to_string())),
        }
    };
    if let Some(exited) = exited {
        let Some(running) = world.remove_resource::<Running>() else {
            return;
        };
        item.state = State::Done(match exited {
            Ok(Some(0)) if item.asked => "quit when asked".to_owned(),
            Ok(Some(0)) => "exited".to_owned(),
            Ok(Some(code)) => format!("exited with code {code}"),
            Ok(None) => "exited".to_owned(),
            Err(e) => format!("cannot wait for it: {e}"),
        });
        // A window that moved its world to detached meanwhile (F9): that
        // world is stopped next.
        if let Some(moved) = running.moved() {
            detached::adopt_moved_quietly(world, moved);
        }
        return;
    }
    if !item.asked {
        item.asked = true;
        let Some(target) = window_dir(world, agents_dir).map(Target) else {
            // Ruling R17: the Village and the testbed scenarios have
            // nothing to save.
            kill_window(world, item, agents_dir, "nothing to save");
            return;
        };
        let (tx, rx) = mpsc::channel();
        let (control, deadline) = (Arc::clone(control), item.deadline);
        std::thread::spawn(move || {
            let _ = tx.send(ask_window(control.as_ref(), &target, pid, deadline));
        });
        item.ask = Some(Rx::new(rx));
        item.state = State::Busy("asked to quit (app/quit)...".into());
    }
    if let Some(rx) = &item.ask {
        match rx.try_recv() {
            Ok(Ok(())) => {
                item.ask = None;
                // A late answer still gets its time to save and exit.
                item.deadline = item.deadline.max(Instant::now() + wait.min(ANSWERED_GRACE));
                item.state = State::Busy("saving and quitting: it answered app/quit...".into());
            }
            Ok(Err(e)) => {
                item.ask = None;
                let why = format!("no answer to app/quit within {} s ({e})", wait.as_secs());
                kill_window(world, item, agents_dir, &why);
                return;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => item.ask = None,
        }
    }
    if Instant::now() >= item.deadline {
        let why = format!("it did not quit in time ({} s)", wait.as_secs());
        kill_window(world, item, agents_dir, &why);
    }
}

/// The running game's agent folder, for an explorer window.
fn window_dir(world: &World, agents_dir: &std::path::Path) -> Option<AgentDir> {
    match &world.get_resource::<Running>()?.job {
        Job::Game(entry) => entry.agent().map(|agent| AgentDir {
            dir: agents_dir.join(slug(agent)),
        }),
        Job::Build { .. } => None,
    }
}

/// Kills the game window through the menu's own child handle and drops it;
/// an explorer window's `run.json` goes too (it could not remove it).
fn kill_window(world: &mut World, item: &mut Item, agents_dir: &std::path::Path, why: &str) {
    let dir = window_dir(world, agents_dir);
    if let Some(mut running) = world.remove_resource::<Running>() {
        run::kill(&mut running.child);
    }
    if let Some(dir) = dir {
        detached::forget_run_file(&dir, item.pid);
    }
    info!("game_menu: quitting: {} killed: {why}", item.name);
    item.state = State::Done(format!("killed (pid {}): {why}", item.pid));
}

/// `app/quit {pid}` to the window until it answers or `deadline` passes (a
/// worker thread): the port is looked up again on every try (an explorer
/// window writes its `run.json` once it is up).
fn ask_window(
    control: &dyn Control,
    target: &Target,
    pid: u32,
    deadline: Instant,
) -> Result<(), String> {
    loop {
        let Target(dir) = target;
        let port = RunFile::read_any(dir)
            .filter(|r| r.pid == pid && r.mode == RunMode::Window)
            .map(|r| r.port)
            .ok_or_else(|| format!("no run.json of pid {pid} in {}", dir.dir.display()));
        let err = match port.and_then(|p| control.quit_window(p, pid)) {
            Ok(_) => return Ok(()),
            Err(e) => e,
        };
        if Instant::now() + ASK_RETRY >= deadline {
            return Err(err);
        }
        std::thread::sleep(ASK_RETRY);
    }
}

/// A stop the quit started (or a Stop / Open in window in flight) ended for
/// `agent`'s world: `Ok` how it went (it is gone), `Err` why it could not be
/// stopped (refused: its save failed; or no answer; it still runs).
pub fn world_result(world: &mut World, agent: &str, result: Result<String, String>) {
    let Some(mut q) = world.get_resource_mut::<Quitting>() else {
        return;
    };
    if let Some(item) = q
        .items
        .iter_mut()
        .find(|i| i.is_world(agent, None) && !i.state.is_final())
    {
        item.state = match result {
            Ok(text) => State::Done(text),
            Err(e) => State::Refused(e),
        };
    }
}

/// The Quitting screen's **Quit anyway**: every world whose stop was
/// refused is killed by its pid on a worker (its `run.json` removed; its
/// save stays at the last autosave); the menu exits once they are gone.
pub fn quit_anyway(world: &mut World) {
    if !world
        .get_resource::<Quitting>()
        .is_some_and(Quitting::stuck)
    {
        return;
    }
    world.resource_scope(|world, mut q: Mut<Quitting>| {
        for item in &mut q.items {
            let (State::Refused(_), What::World(agent)) = (&item.state, &item.what) else {
                continue;
            };
            let agent = agent.clone();
            let Some((mut proc, dir, started)) = detached::take_proc(world, &agent) else {
                item.state = State::Done("no longer running".into());
                continue;
            };
            let pid = item.pid;
            info!("game_menu: quit anyway: killing {} (pid {pid})", item.name);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(match detached::kill_proc(&mut proc, started) {
                    Ok(()) => {
                        detached::forget_run_file(&dir, pid);
                        Ok(None)
                    }
                    // A reused pid: never killed; the world is gone, so is
                    // its run.json (removed only while it names this pid).
                    Err(KillError::NotOurs(e)) => {
                        detached::forget_run_file(&dir, pid);
                        Ok(Some(e))
                    }
                    Err(KillError::Failed(e)) => Err((proc, e)),
                });
            });
            item.kill = Some(Rx::new(rx));
            item.state = State::Busy(format!("killing pid {pid}..."));
        }
    });
}

/// The Quitting screen's **Back**: the quit ends, the refused worlds keep
/// running and Main says so (not after Ctrl+C).
pub fn back(world: &mut World) {
    let Some(q) = world.get_resource::<Quitting>() else {
        return;
    };
    if !q.stuck() || q.ctrl_c {
        return;
    }
    let names: Vec<String> = q
        .refused()
        .map(|i| format!("{} keeps running ({})", i.name, i.state.text()))
        .collect();
    world.remove_resource::<Quitting>();
    world.insert_resource(MainNotice::error(format!(
        "Quit stopped: {}",
        names.join("; ")
    )));
    run::set_screen(world, Screen::Main);
}

/// `Last`: an exit the menu did not decide (Ctrl+C through Bevy's
/// `TerminalCtrlCHandlerPlugin`, which asks on every frame from then on)
/// becomes the quit while anything still runs; the menu's own exit, and any
/// exit when nothing runs, passes.
pub fn gate_exit(world: &mut World) {
    if world.resource::<Messages<AppExit>>().is_empty()
        || world.get_resource::<Quitting>().is_some_and(|q| q.done)
        || (!active(world) && nothing_to_stop(world))
    {
        return;
    }
    let ctrl_c = world
        .resource_mut::<Messages<AppExit>>()
        .drain()
        .any(|e| e == AppExit::from_code(130));
    if !active(world) {
        info!(
            "game_menu: exit asked{}: stopping every process first",
            if ctrl_c { " (Ctrl+C)" } else { "" }
        );
    }
    start(world, ctrl_c);
}

/// The Quitting screen's list.
#[derive(Component)]
struct QuitList;

/// The Quitting screen's line under the list (a refused stop).
#[derive(Component)]
struct QuitNote;

/// **Quit anyway** and **Back** (shown once a stop was refused).
#[derive(Component)]
struct StuckButton {
    back: bool,
}

fn spawn_quitting(mut commands: Commands) {
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::Quitting),
        "Quitting...",
    );
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(
            "Stopping everything the game started. Detached worlds and explorer windows save first.",
            14.0,
            ui::DIM,
        ));
        p.spawn((QuitList, ui::label("", 16.0, TEXT)));
        p.spawn((QuitNote, ui::label("", 14.0, ERROR)));
        ui::row(p, |r| {
            let anyway = button(
                r,
                "Quit anyway",
                Some("kill it by its pid: its save stays at the last autosave"),
                true,
                0,
                Action::QuitAnyway,
            );
            r.commands()
                .entity(anyway)
                .insert(StuckButton { back: false });
            let back = button(
                r,
                "Back",
                Some("keep it running"),
                true,
                1,
                Action::QuitBack,
            );
            r.commands().entity(back).insert(StuckButton { back: true });
        });
    });
}

/// The Quitting screen from [`Quitting`]: one line per process, the refused
/// stops' note and the buttons while stuck.
fn refresh_quitting(
    q: Res<Quitting>,
    mut list: Query<&mut Text, (With<QuitList>, Without<QuitNote>)>,
    mut note: Query<&mut Text, (With<QuitNote>, Without<QuitList>)>,
    mut buttons: Query<(&StuckButton, &mut Node)>,
) {
    let lines: Vec<String> = q
        .items
        .iter()
        .map(|i| format!("{}: {}", i.name, i.state.text()))
        .collect();
    let lines = lines.join("\n");
    for mut t in &mut list {
        if t.0 != lines {
            t.0.clone_from(&lines);
        }
    }
    let stuck = q.stuck();
    let want = if stuck {
        let names: Vec<&str> = q.refused().map(|i| i.name.as_str()).collect();
        let mut text = format!(
            "{} could not be stopped (the reason is above), so it still runs. Quit anyway kills \
             it (its save stays at the last autosave).",
            names.join(", ")
        );
        if q.ctrl_c {
            text.push_str(" Ctrl+C again exits the menu at once and leaves it running.");
        }
        text
    } else {
        String::new()
    };
    for mut t in &mut note {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
    for (b, mut node) in &mut buttons {
        let display = if stuck && !(b.back && q.ctrl_c) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use bevy::ui::InteractionDisabled;
    use serde_json::json;
    use testbed_content::explorer::home::{AgentDir, create_agent};
    use testbed_content::explorer::host::{RunFile, RunMode, pid_alive};

    use super::*;
    use crate::detached::DetachedWorlds;
    use crate::detached::tests::{
        Plan, fake_control, fake_control_with, fake_spawner, has_text, menu, prompt_file, release,
        served, texts, until,
    };
    use crate::launch::Entry;
    use crate::run::{self, Running};
    use crate::screens::tests::{button_for, press, screen, test_child, tmp};
    use crate::screens::{Action, Screen, apply_action};

    fn exiting(app: &mut App) -> bool {
        !app.world().resource::<Messages<AppExit>>().is_empty()
    }

    /// Starts `agent`'s world `name` detached, as New's Start detached
    /// does, and waits until it is up on `port`.
    fn start_detached(app: &mut App, agent: &str, name: &str, port: u16) {
        run::start_game(
            app.world_mut(),
            Entry::ExplorerNew {
                agent: agent.into(),
                world: name.into(),
                mode: RunMode::Detached,
            },
        );
        assert!(
            until(app, Duration::from_secs(30), |a| a
                .world()
                .resource::<DetachedWorlds>()
                .get(agent)
                .is_some_and(|w| w.is_running() && w.port == port)),
            "{:?}",
            texts(app)
        );
    }

    /// The agent folder of `agent` under `d/agents`.
    fn folder(d: &Path, agent: &str) -> AgentDir {
        AgentDir {
            dir: d
                .join("agents")
                .join(testbed_content::explorer::home::slug(agent)),
        }
    }

    /// Quit stops every detached world in parallel: `session/stop` (the
    /// world saves), then the wait for its exit; the menu exits once all
    /// are gone, never before.
    #[test]
    fn quit_stops_detached_worlds_and_waits_for_their_save() {
        let d = tmp("q-worlds");
        let mut app = menu(&d);
        let prompt = prompt_file(&d);
        let (m1, m2) = (d.join("stop-1"), d.join("stop-2"));
        let calls = fake_spawner(
            &mut app,
            vec![
                served(15801, &prompt, &m1, 0),
                served(15802, &prompt, &m2, 0),
            ],
        );
        let (wren, brin) = (folder(&d, "Wren Ashdown"), folder(&d, "Brin Marlow"));
        let stops: Arc<Mutex<Vec<u16>>> = Arc::default();
        let (log, saves) = (
            Arc::clone(&stops),
            [
                (15801, wren.save("Moor"), m1.clone()),
                (15802, brin.save("Fen"), m2.clone()),
            ],
        );
        fake_control(&mut app, move |port| {
            log.lock().unwrap().push(port);
            // Saving takes a while; then the save, then the exit.
            std::thread::sleep(Duration::from_millis(1500));
            let (_, save, marker) = saves.iter().find(|(p, _, _)| *p == port).unwrap();
            std::fs::create_dir_all(save.parent().unwrap()).unwrap();
            std::fs::write(save, "{}").unwrap();
            std::fs::write(marker, "").unwrap();
            Ok(json!({"stopped": true, "tick": 42}))
        });
        start_detached(&mut app, "Wren Ashdown", "Moor", 15801);
        start_detached(&mut app, "Brin Marlow", "Fen", 15802);
        assert_eq!(calls.lock().unwrap().len(), 2);
        let started = Instant::now();
        apply_action(app.world_mut(), Action::Quit);
        app.update();
        app.update();
        assert!(!exiting(&mut app), "not before the worlds are gone");
        assert_eq!(screen(&app), Screen::Quitting);
        assert!(has_text(&mut app, "Wren Ashdown"), "{:?}", texts(&mut app));
        assert!(has_text(&mut app, "Brin Marlow"), "{:?}", texts(&mut app));
        assert!(
            until(&mut app, Duration::from_secs(30), exiting),
            "{:?}",
            texts(&mut app)
        );
        // In parallel: both stops took 1.5 s, together well under 3 s.
        assert!(
            started.elapsed() < Duration::from_millis(2900),
            "{:?}",
            started.elapsed()
        );
        let mut ports = stops.lock().unwrap().clone();
        ports.sort_unstable();
        assert_eq!(ports, [15801, 15802]);
        assert!(wren.save("Moor").exists() && brin.save("Fen").exists());
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_none()
        );
        assert!(
            has_text(&mut app, "stopped at tick 42 and saved"),
            "{:?}",
            texts(&mut app)
        );
        release(&[&m1, &m2]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Writes `agent`'s `run.json` naming `pid` (as the world or window
    /// would) and returns the agent folder.
    fn write_run(
        d: &Path,
        agent: &str,
        pid: u32,
        port: u16,
        mode: RunMode,
        started: &str,
    ) -> AgentDir {
        let dir = folder(d, agent);
        std::fs::create_dir_all(&dir.dir).unwrap();
        RunFile {
            pid,
            port,
            mode,
            started: started.into(),
            agent: agent.into(),
            world: "Moor".into(),
        }
        .write(&dir)
        .unwrap();
        dir
    }

    /// Now as `run.json` writes it.
    fn now_rfc3339() -> String {
        testbed_content::explorer::host::rfc3339(std::time::SystemTime::now())
    }

    /// A copy of this test binary named `stem` (once per test run), so a fake
    /// world passes the kill's identity check (`npcctl.exe`).
    fn fake_exe(stem: &str) -> std::path::PathBuf {
        static LOCK: Mutex<()> = Mutex::new(());
        let _held = LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("game-menu-fake-exe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
        if !exe.exists() {
            std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        }
        exe
    }

    /// A fake world process (this test binary, as `program`) that runs until
    /// `until` exists, then exits 0.
    fn world_process(program: Option<&str>, until: &Path) -> std::process::Child {
        let model = crate::screens::tests::test_child_until(Some(0), "", "", Some(until));
        let exe = program.map_or_else(|| std::env::current_exe().unwrap(), fake_exe);
        let mut cmd = std::process::Command::new(exe);
        cmd.args(model.get_args());
        for (k, v) in model.get_envs() {
            if let Some(v) = v {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    /// Tracks `agent`'s world run by `pid` as the Resume screen adopts a live
    /// `run.json` (`started` its date).
    fn adopt_pid(
        app: &mut App,
        d: &Path,
        agent: &str,
        pid: u32,
        port: u16,
        started: &str,
    ) -> AgentDir {
        let dir = write_run(d, agent, pid, port, RunMode::Detached, started);
        let run = RunFile::read_any(&dir).unwrap();
        crate::detached::adopt(
            &mut app.world_mut().resource_mut::<DetachedWorlds>(),
            &run,
            &dir,
        );
        dir
    }

    /// An explorer window gets `app/quit {pid}` on the port of its
    /// `run.json`: one that answers and exits is done at once; one that
    /// answers late gets a fresh grace to save and exit; one that answers
    /// but does not exit, or never answers, is killed through the menu's
    /// child handle after the wait, and its `run.json` removed.
    #[test]
    fn quit_asks_explorer_windows_then_kills_after_timeout() {
        for case in ["exits", "late", "stays", "silent"] {
            let d = tmp(&format!("q-window-{case}"));
            let mut app = menu(&d);
            app.insert_resource(QuitWait(Duration::from_secs(2)));
            let marker = d.join("quit-1");
            let calls = fake_spawner(
                &mut app,
                vec![Plan {
                    code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    until: Some(marker.clone()),
                }],
            );
            let asked: Arc<Mutex<Vec<(u16, u32)>>> = Arc::default();
            let (log, m) = (Arc::clone(&asked), marker.clone());
            let first: Mutex<Option<Instant>> = Mutex::new(None);
            fake_control_with(
                &mut app,
                |_| Err("no stop in this test".into()),
                move |port, pid| {
                    log.lock().unwrap().push((port, pid));
                    let t0 = *first.lock().unwrap().get_or_insert_with(Instant::now);
                    match case {
                        "exits" => std::fs::write(&m, "").unwrap(),
                        "silent" => return Err("connection refused".into()),
                        // Answers 1.2 s after the first ask (of the 2 s
                        // wait), then takes 1.5 s to save and exit.
                        "late" if t0.elapsed() < Duration::from_millis(1200) => {
                            return Err("connection refused".into());
                        }
                        "late" => {
                            let m = m.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(1500));
                                std::fs::write(&m, "").unwrap();
                            });
                        }
                        _ => {}
                    }
                    Ok(json!({"quitting": true, "pid": pid}))
                },
            );
            run::start_game(
                app.world_mut(),
                Entry::ExplorerNew {
                    agent: "Wren Ashdown".into(),
                    world: "Moor".into(),
                    mode: RunMode::Window,
                },
            );
            let pid = app.world().resource::<Running>().child.id();
            assert_eq!(calls.lock().unwrap()[0].0, "actor_testbed");
            // What the window writes once it is up.
            let dir = write_run(
                &d,
                "Wren Ashdown",
                pid,
                15807,
                RunMode::Window,
                &now_rfc3339(),
            );
            let started = Instant::now();
            apply_action(app.world_mut(), Action::Quit);
            assert!(
                until(&mut app, Duration::from_secs(30), exiting),
                "{:?}",
                texts(&mut app)
            );
            assert_eq!(asked.lock().unwrap().first(), Some(&(15807, pid)));
            assert!(!app.world().contains_resource::<Running>());
            if case == "late" {
                assert!(started.elapsed() > Duration::from_secs(2), "past the wait");
                assert!(
                    has_text(&mut app, "quit when asked"),
                    "{:?}",
                    texts(&mut app)
                );
            } else if case == "exits" {
                assert!(started.elapsed() < Duration::from_secs(2));
                assert!(
                    has_text(&mut app, "quit when asked"),
                    "{:?}",
                    texts(&mut app)
                );
            } else {
                assert!(started.elapsed() >= Duration::from_millis(1500), "waited");
                assert!(
                    has_text(&mut app, &format!("killed (pid {pid})")),
                    "{:?}",
                    texts(&mut app)
                );
                assert!(!pid_alive(pid), "killed");
                assert!(!RunFile::path(&dir).exists(), "its run.json removed");
            }
            release(&[&marker]);
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// Ruling R17: the Village (and a testbed scenario, the RTS demo) has
    /// nothing to save: no `--remote` from the menu, and Quit kills it
    /// through the menu's child handle at once, without asking.
    #[test]
    fn quit_kills_windows_without_a_save_at_once() {
        let d = tmp("q-village");
        let mut app = menu(&d);
        let marker = d.join("quit-1");
        let calls = fake_spawner(
            &mut app,
            vec![Plan {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
                until: Some(marker.clone()),
            }],
        );
        let asked: Arc<Mutex<u32>> = Arc::default();
        let log = Arc::clone(&asked);
        fake_control_with(
            &mut app,
            |_| Err("no stop in this test".into()),
            move |_, _| {
                *log.lock().unwrap() += 1;
                Ok(json!({}))
            },
        );
        run::start_game(app.world_mut(), Entry::Village);
        let pid = app.world().resource::<Running>().child.id();
        let args = calls.lock().unwrap()[0].1.clone();
        assert!(!args.iter().any(|a| a.starts_with("--remote")), "{args:?}");
        let started = Instant::now();
        apply_action(app.world_mut(), Action::Quit);
        assert!(until(&mut app, Duration::from_secs(30), exiting));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(*asked.lock().unwrap(), 0, "not asked");
        assert!(
            has_text(&mut app, &format!("killed (pid {pid}): nothing to save")),
            "{:?}",
            texts(&mut app)
        );
        assert!(!pid_alive(pid));
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Quit while Open in window is in flight: the world's stop ends (it
    /// saved), and no window opens; the menu exits.
    #[test]
    fn quit_during_open_in_window_opens_no_window() {
        let d = tmp("q-open");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        let calls = fake_spawner(&mut app, vec![served(15808, &prompt, &marker, 0)]);
        let (m, save) = (marker.clone(), folder(&d, "Wren Ashdown").save("Moor"));
        fake_control(&mut app, move |_| {
            std::thread::sleep(Duration::from_millis(1000));
            std::fs::create_dir_all(save.parent().unwrap()).unwrap();
            std::fs::write(&save, "{}").unwrap();
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 5}))
        });
        start_detached(&mut app, "Wren Ashdown", "Moor", 15808);
        let open = button_for(&mut app, |a| matches!(a, Action::OpenInWindow(_)));
        press(&mut app, open);
        assert!(app.world().resource::<DetachedWorlds>().busy());
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(30), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert_eq!(calls.lock().unwrap().len(), 1, "no window was started");
        assert!(!app.world().contains_resource::<Running>());
        assert!(has_text(&mut app, "stopped at tick 5 and saved"));
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// F9 during the quit: the window moved its world to detached and exits
    /// when asked; the new host (still taking the port over: no answer at
    /// first) is asked again until it answers, saves and exits.
    #[test]
    fn quit_stops_a_world_moved_to_detached_meanwhile() {
        let d = tmp("q-f9");
        let mut app = menu(&d);
        app.insert_resource(QuitWait(Duration::from_secs(10)));
        let (mw, mn) = (d.join("window-exit"), d.join("host-exit"));
        let mut host = world_process(Some("npcctl"), &mn);
        let moved = testbed_content::explorer::host::DetachedLine {
            pid: host.id(),
            port: 15809,
            agent: "Wren Ashdown".into(),
            world: "Moor".into(),
        };
        fake_spawner(
            &mut app,
            vec![Plan {
                code: 0,
                stdout: format!("{}\n", moved.line()),
                stderr: String::new(),
                until: Some(mw.clone()),
            }],
        );
        let stops: Arc<Mutex<u32>> = Arc::default();
        let (log, save, m) = (
            Arc::clone(&stops),
            folder(&d, "Wren Ashdown").save("Moor"),
            mn.clone(),
        );
        let w = mw.clone();
        fake_control_with(
            &mut app,
            move |port| {
                assert_eq!(port, 15809);
                let mut n = log.lock().unwrap();
                *n += 1;
                if *n <= 3 {
                    return Err(crate::detached::StopError::NoAnswer(
                        "connection refused".into(),
                    ));
                }
                std::fs::create_dir_all(save.parent().unwrap()).unwrap();
                std::fs::write(&save, "{}").unwrap();
                std::fs::write(&m, "").unwrap();
                Ok(json!({"stopped": true, "tick": 8}))
            },
            move |_, _| {
                std::fs::write(&w, "").unwrap();
                Ok(json!({"quitting": true}))
            },
        );
        run::start_game(
            app.world_mut(),
            Entry::ExplorerResume {
                agent: "Wren Ashdown".into(),
                choice: None,
                mode: RunMode::Window,
            },
        );
        let pid = app.world().resource::<Running>().child.id();
        write_run(
            &d,
            "Wren Ashdown",
            pid,
            15809,
            RunMode::Window,
            &now_rfc3339(),
        );
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(60), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            has_text(&mut app, "quit when asked"),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            has_text(&mut app, "stopped at tick 8 and saved"),
            "{:?}",
            texts(&mut app)
        );
        assert!(*stops.lock().unwrap() >= 4, "asked again");
        assert!(
            matches!(host.try_wait(), Ok(Some(_))),
            "the new host exited"
        );
        release(&[&mw, &mn]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A world that never comes up (no listening line) is killed through its
    /// child handle after the wait, and the `run.json` it wrote goes too.
    #[test]
    fn quit_kills_a_world_that_never_comes_up() {
        let d = tmp("q-starting");
        let mut app = menu(&d);
        app.insert_resource(QuitWait(Duration::from_secs(1)));
        let marker = d.join("never");
        fake_spawner(
            &mut app,
            vec![Plan {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
                until: Some(marker.clone()),
            }],
        );
        fake_control(&mut app, |_| Err("no stop in this test".into()));
        run::start_game(
            app.world_mut(),
            Entry::ExplorerNew {
                agent: "Wren Ashdown".into(),
                world: "Moor".into(),
                mode: RunMode::Detached,
            },
        );
        let pid = app
            .world()
            .resource::<DetachedWorlds>()
            .get("Wren Ashdown")
            .filter(|w| w.is_starting())
            .and_then(crate::detached::DetachedWorld::pid)
            .unwrap();
        let dir = write_run(
            &d,
            "Wren Ashdown",
            pid,
            15810,
            RunMode::Detached,
            &now_rfc3339(),
        );
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(30), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            has_text(&mut app, &format!("killed (pid {pid}): it did not come up")),
            "{:?}",
            texts(&mut app)
        );
        assert!(!pid_alive(pid));
        assert!(!RunFile::path(&dir).exists(), "its run.json removed");
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// An adopted world (a pid from `run.json`) that saved at the stop but
    /// still runs after the wait is killed once its pid is checked to be
    /// the world's `npcctl`; its `run.json` goes.
    #[test]
    fn an_adopted_world_that_saved_but_runs_on_is_killed() {
        let d = tmp("q-adopted-killed");
        let mut app = menu(&d);
        app.insert_resource(QuitWait(Duration::from_secs(1)));
        let never = d.join("never");
        let mut host = world_process(Some("npcctl"), &never);
        let pid = host.id();
        fake_control(&mut app, |_| Ok(json!({"stopped": true, "tick": 4})));
        let dir = adopt_pid(&mut app, &d, "Wren Ashdown", pid, 15811, &now_rfc3339());
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(60), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            has_text(&mut app, &format!("killed (pid {pid})")),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            until(&mut app, Duration::from_secs(10), |_| matches!(
                host.try_wait(),
                Ok(Some(_))
            )),
            "killed"
        );
        assert!(!RunFile::path(&dir).exists(), "its run.json removed");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Quit anyway on adopted worlds: the one whose pid is still its
    /// `npcctl` is killed; a pid now running another program, or an
    /// `npcctl` that started after the `run.json` was written (a reused
    /// pid), is never killed: the world is forgotten and its stale
    /// `run.json` removed. The screen tells a refused stop (failed save)
    /// from one that got no answer.
    #[test]
    fn quit_anyway_kills_an_adopted_pid_only_when_it_is_the_worlds() {
        let d = tmp("q-adopted-anyway");
        let mut app = menu(&d);
        app.insert_resource(QuitWait(Duration::from_secs(1)));
        let never = d.join("never");
        let mut wren = world_process(Some("npcctl"), &never);
        let mut brin = world_process(Some("npcctl"), &never);
        let mut cara = world_process(None, &never);
        fake_control(&mut app, |port| {
            if port == 15814 {
                Err(crate::detached::StopError::NoAnswer(
                    "connection refused".into(),
                ))
            } else {
                Err("the save failed: disk full; the session keeps running".into())
            }
        });
        let now = now_rfc3339();
        let wren_dir = adopt_pid(&mut app, &d, "Wren Ashdown", wren.id(), 15812, &now);
        // A reused pid: this npcctl started long after that run.json.
        let brin_dir = adopt_pid(
            &mut app,
            &d,
            "Brin Marlow",
            brin.id(),
            15813,
            "2000-01-01T00:00:00Z",
        );
        // A reused pid: another program (this test binary).
        let cara_dir = adopt_pid(&mut app, &d, "Cara Vell", cara.id(), 15814, &now);
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(60), |a| a
                .world()
                .resource::<Quitting>()
                .stuck()),
            "{:?}",
            texts(&mut app)
        );
        assert!(has_text(
            &mut app,
            "refused the stop, its save failed: the save failed: disk full"
        ));
        assert!(
            has_text(&mut app, "does not answer session/stop"),
            "{:?}",
            texts(&mut app)
        );
        let anyway = button_for(&mut app, |a| matches!(a, Action::QuitAnyway));
        press(&mut app, anyway);
        assert!(
            until(&mut app, Duration::from_secs(60), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            until(&mut app, Duration::from_secs(10), |_| matches!(
                wren.try_wait(),
                Ok(Some(_))
            )),
            "the world's own npcctl is killed"
        );
        assert!(matches!(brin.try_wait(), Ok(None)), "never killed");
        assert!(matches!(cara.try_wait(), Ok(None)), "never killed");
        assert!(
            has_text(&mut app, &format!("not killed: pid {} (", brin.id())),
            "{:?}",
            texts(&mut app)
        );
        assert!(
            has_text(&mut app, &format!("not killed: pid {} is now", cara.id())),
            "{:?}",
            texts(&mut app)
        );
        for dir in [&wren_dir, &brin_dir, &cara_dir] {
            assert!(!RunFile::path(dir).exists(), "{}", dir.dir.display());
        }
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .iter()
                .next()
                .is_none()
        );
        run::kill(&mut brin);
        run::kill(&mut cara);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A refused stop (the save failed) keeps the menu on the Quitting
    /// screen with the world and the reason; Back leaves it running, Quit
    /// anyway kills it by its pid and quits.
    #[test]
    fn quit_with_a_refused_save_offers_quit_anyway() {
        let d = tmp("q-refused");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        fake_spawner(&mut app, vec![served(15803, &prompt, &marker, 0)]);
        fake_control(&mut app, |_| {
            Err("the save failed: disk full; the session keeps running".into())
        });
        start_detached(&mut app, "Wren Ashdown", "Moor", 15803);
        let pid = app
            .world()
            .resource::<DetachedWorlds>()
            .get("Wren Ashdown")
            .and_then(crate::detached::DetachedWorld::pid)
            .unwrap();
        apply_action(app.world_mut(), Action::Quit);
        let refused = |a: &mut App| has_text(a, "the save failed: disk full");
        assert!(
            until(&mut app, Duration::from_secs(30), refused),
            "{:?}",
            texts(&mut app)
        );
        assert_eq!(screen(&app), Screen::Quitting);
        assert!(has_text(&mut app, "Wren Ashdown"));
        assert!(!exiting(&mut app));
        let anyway = button_for(&mut app, |a| matches!(a, Action::QuitAnyway));
        assert!(!app.world().entity(anyway).contains::<InteractionDisabled>());
        let back = button_for(&mut app, |a| matches!(a, Action::QuitBack));
        // Back: the menu stays, the world runs on.
        press(&mut app, back);
        app.update();
        assert_eq!(screen(&app), Screen::Main);
        assert!(!exiting(&mut app));
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_some_and(crate::detached::DetachedWorld::is_running)
        );
        // Again, then Quit anyway: killed by its pid, the menu exits.
        apply_action(app.world_mut(), Action::Quit);
        assert!(until(&mut app, Duration::from_secs(30), refused));
        let anyway = button_for(&mut app, |a| matches!(a, Action::QuitAnyway));
        press(&mut app, anyway);
        assert!(
            until(&mut app, Duration::from_secs(30), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert!(!pid_alive(pid), "killed");
        assert!(
            app.world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_none()
        );
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Only what the menu started or adopted is stopped: a process it did
    /// not start (and a detached world's `run.json` it never adopted) is
    /// left alone.
    #[test]
    fn quit_leaves_untracked_processes_alone() {
        let d = tmp("q-untracked");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        fake_spawner(&mut app, vec![served(15804, &prompt, &marker, 0)]);
        let stops: Arc<Mutex<Vec<u16>>> = Arc::default();
        let (log, m) = (Arc::clone(&stops), marker.clone());
        fake_control(&mut app, move |port| {
            log.lock().unwrap().push(port);
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 1}))
        });
        // Someone else's world: a live process with a run.json the menu
        // never adopted (no Resume screen visit).
        let mut other = test_child(None, "", "")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let cara = create_agent(&d.join("agents"), "Cara Vell", "Dune", "2026-10-26").unwrap();
        RunFile {
            pid: other.id(),
            port: 15805,
            mode: RunMode::Detached,
            started: "2026-10-26T01:00:00Z".into(),
            agent: "Cara Vell".into(),
            world: "Dune".into(),
        }
        .write(&cara)
        .unwrap();
        start_detached(&mut app, "Wren Ashdown", "Moor", 15804);
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(30), exiting),
            "{:?}",
            texts(&mut app)
        );
        assert_eq!(*stops.lock().unwrap(), [15804], "only the tracked world");
        assert!(matches!(other.try_wait(), Ok(None)), "still running");
        assert!(RunFile::path(&cara).exists(), "its run.json untouched");
        run::kill(&mut other);
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Ctrl+C on the menu's console (Bevy's handler writes exit code 130 on
    /// every frame from then on) runs the same quit: the worlds stop and
    /// save first, the menu exits after; the refused-stop screen would
    /// offer no Back.
    #[test]
    fn ctrl_c_runs_the_same_quit() {
        let d = tmp("q-ctrl-c");
        let mut app = menu(&d);
        let (prompt, marker) = (prompt_file(&d), d.join("stop-1"));
        fake_spawner(&mut app, vec![served(15806, &prompt, &marker, 0)]);
        let m = marker.clone();
        fake_control(&mut app, move |_| {
            std::thread::sleep(Duration::from_millis(500));
            std::fs::write(&m, "").unwrap();
            Ok(json!({"stopped": true, "tick": 9}))
        });
        start_detached(&mut app, "Wren Ashdown", "Moor", 15806);
        let ctrl_c = AppExit::from_code(130);
        app.world_mut().write_message(ctrl_c.clone());
        app.update();
        assert!(!exiting(&mut app), "held back while the world runs");
        assert!(app.world().resource::<Quitting>().ctrl_c);
        app.update();
        assert_eq!(screen(&app), Screen::Quitting);
        let mut done = false;
        for _ in 0..3000 {
            app.world_mut().write_message(ctrl_c.clone());
            app.update();
            let exits: Vec<AppExit> = app
                .world()
                .resource::<Messages<AppExit>>()
                .iter_current_update_messages()
                .cloned()
                .collect();
            if exits.contains(&AppExit::Success) {
                done = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(done, "{:?}", texts(&mut app));
        assert!(has_text(&mut app, "stopped at tick 9 and saved"));
        release(&[&marker]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The hands-off check with the real binaries (`cargo build -p npcctl
    /// -p actor_testbed` first; it opens a game window): the menu's own code
    /// paths start a detached world (`npcctl serve`) and an explorer window
    /// (`actor_testbed`), then a scenario window, and Quit stops them all
    /// (the scenario window is killed at once: it has nothing to save).
    /// Prints the pids it started, alive before and gone after, the saves'
    /// times and the `run.json` files. Run with `cargo test -p game_menu
    /// hands_off -- --ignored --nocapture`.
    #[test]
    #[ignore = "starts the real npcctl and actor_testbed (a window); run by hand"]
    fn hands_off_quit_with_the_real_binaries() {
        use crate::launch::RunMode as Mode;
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .to_path_buf();
        for stem in ["npcctl", "actor_testbed"] {
            assert!(
                crate::launch::find_exe(stem, &exe_dir).is_some(),
                "build {stem} first"
            );
        }
        let d = tmp("q-hands-off");
        let agents = d.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let mut app = crate::screens::tests::app(&exe_dir, &agents);
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        // As New's Start detached and Start in window do.
        run::start_game(
            app.world_mut(),
            Entry::ExplorerNew {
                agent: "Wren Ashdown".into(),
                world: "Moor".into(),
                mode: Mode::Detached,
            },
        );
        assert!(
            until(&mut app, Duration::from_secs(120), |a| a
                .world()
                .resource::<DetachedWorlds>()
                .get("Wren Ashdown")
                .is_some_and(crate::detached::DetachedWorld::is_running)),
            "{:?}",
            texts(&mut app)
        );
        run::set_screen(app.world_mut(), Screen::Main);
        app.update();
        run::start_game(
            app.world_mut(),
            Entry::ExplorerNew {
                agent: "Brin Marlow".into(),
                world: "Fen".into(),
                mode: Mode::Window,
            },
        );
        let (wren, brin) = (folder(&d, "Wren Ashdown"), folder(&d, "Brin Marlow"));
        assert!(
            until(&mut app, Duration::from_secs(180), |_| {
                RunFile::read_any(&brin).is_some() && brin.save("Fen").exists()
            }),
            "the window did not come up"
        );
        let world_pid = app
            .world()
            .resource::<DetachedWorlds>()
            .get("Wren Ashdown")
            .and_then(crate::detached::DetachedWorld::pid)
            .unwrap();
        let window_pid = app.world().resource::<Running>().child.id();
        let before = [
            (wren.save("Moor"), modified(&wren.save("Moor"))),
            (brin.save("Fen"), modified(&brin.save("Fen"))),
        ];
        println!("BEFORE quit:");
        for (what, pid) in [
            ("npcctl serve (Wren, detached)", world_pid),
            ("actor_testbed (Brin, window)", window_pid),
        ] {
            println!("  pid {pid} {what}: alive={}", pid_alive(pid));
        }
        for dir in [&wren, &brin] {
            println!(
                "  {}: {:?}",
                RunFile::path(dir).display(),
                RunFile::read_any(dir)
            );
        }
        for (p, t) in &before {
            println!("  save {} modified {t:?}", p.display());
        }
        std::thread::sleep(Duration::from_secs(2));
        let started = Instant::now();
        apply_action(app.world_mut(), Action::Quit);
        assert!(
            until(&mut app, Duration::from_secs(60), exiting),
            "{:?}",
            texts(&mut app)
        );
        println!("Quitting screen after {:?}:", started.elapsed());
        for t in texts(&mut app) {
            if t.contains("pid") {
                println!("  {t}");
            }
        }
        println!("AFTER quit:");
        for (what, pid) in [
            ("npcctl serve (Wren, detached)", world_pid),
            ("actor_testbed (Brin, window)", window_pid),
        ] {
            println!("  pid {pid} {what}: alive={}", pid_alive(pid));
            assert!(!pid_alive(pid), "{what} still runs");
        }
        for dir in [&wren, &brin] {
            println!(
                "  {} exists: {}",
                RunFile::path(dir).display(),
                RunFile::path(dir).exists()
            );
            assert!(!RunFile::path(dir).exists());
        }
        for (p, t) in &before {
            let now = modified(p);
            println!(
                "  save {} modified {now:?} (newer: {})",
                p.display(),
                now > *t
            );
            assert!(now > *t, "{} not newer", p.display());
        }
        // A testbed scenario window: nothing to save, killed at once.
        let mut app = crate::screens::tests::app(&exe_dir, &agents);
        run::start_game(app.world_mut(), Entry::Testbed("doors"));
        let pid = app.world().resource::<Running>().child.id();
        println!(
            "BEFORE quit: pid {pid} actor_testbed --scenario doors: alive={}",
            pid_alive(pid)
        );
        std::thread::sleep(Duration::from_secs(5));
        let started = Instant::now();
        apply_action(app.world_mut(), Action::Quit);
        assert!(until(&mut app, Duration::from_secs(60), exiting));
        for t in texts(&mut app) {
            if t.contains("pid") {
                println!("  {t}");
            }
        }
        println!(
            "AFTER quit ({:?}): pid {pid} alive={}",
            started.elapsed(),
            pid_alive(pid)
        );
        assert!(!pid_alive(pid));
        let _ = std::fs::remove_dir_all(&d);
    }
}
