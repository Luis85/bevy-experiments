//! The AI explorer's New and Resume screens (game menu spec G3, §5.3,
//! Ruling R0).
//!
//! **New**: the agent and world names, prefilled with
//! `random_name(Agent / World, wall-clock nanos)`, a **Random** (dice) button
//! per field, and **Start**, disabled while a name is blank or
//! `slug(agent)` names an existing agent folder ([`launch::new_world_problem`];
//! Enter in a field presses it). **Resume**: the agents of the agents folder
//! ([`list_agents`]) with their world and last save date (read from the save
//! header once, when the screen opens); one without a save is disabled.
//! Resume runs `actor_testbed --check-save` as a child (no window), polled
//! on the "Checking" screen: `ok` starts the game; a save from another build
//! shows "This world was saved by another version of the game: <error>" with
//! **Load anyway** (`--load-anyway`) and **Fresh world, keep the diary**
//! (`--fresh-world`); an unreadable save shows the error with **Fresh world,
//! keep the diary** only. A check that exits neither 0 nor 1 shows "the save
//! check failed (exit N): <its last stderr line>" on the Resume screen; one
//! still running after [`CHECK_TIMEOUT`], or when the Checking screen is
//! left, is killed.
//!
//! **Window or detached** (spec G7): New has **Start in window** and **Start
//! detached**; each Resume row starts in the window, its **Start detached**
//! button detached (`npcctl serve`, the same check first; the mismatch
//! dialog keeps the choice). A row whose world runs detached (tracked, or a
//! live `run.json` of mode `detached`, adopted) says "running detached -
//! Open / Stop" and has those two buttons; the row itself opens its Detached
//! screen ([`crate::detached`]). One running in a window cannot be started
//! again.

use std::io::Read as _;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use bevy::ui_widgets::ScrollArea;
use testbed_content::explorer::home::{AgentEntry, list_agents};
use testbed_content::explorer::host::{RunFile, pid_dead};
use testbed_content::explorer::names::{NameKind, random_name};

use crate::detached::{self, DetachedWorlds, Rx};
use crate::launch::{self, CheckResult, Entry, Mismatch, RunMode};
use crate::run::{self, StderrTail};
use crate::screens::{Action, Missing, Screen, apply_action, back_button, button, on_activate};
use crate::ui::{self, ERROR, NOTE, OK, TEXT};
use crate::{AppSettings, Paths};

/// A name field on the New screen.
#[derive(Component, Clone, Copy, Debug)]
pub struct NameField(pub NameKind);

/// The New screen's "why Start is disabled" line.
#[derive(Component)]
struct NewProblemText;

/// The New screen's line naming the agent folder ([`launch::agent_folder_line`]).
#[derive(Component)]
struct NewFolderText;

/// The New screen's lines [`refresh_new`] rewrites.
type NewLine = Or<(With<NewProblemText>, With<NewFolderText>)>;

/// The New screen's Start buttons (in window, detached).
#[derive(Component)]
struct StartButton;

/// The names [`refresh_new`] last checked (cleared when New opens).
#[derive(Resource, Default)]
struct CheckedNames(Option<(String, String)>);

/// The Resume screen's error line (the save check could not run), shown
/// once: the screen takes it when it opens.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ResumeNotice(pub Option<String>);

/// The Resume screen's good news (a detached world stopped and saved),
/// shown once like [`ResumeNotice`].
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ResumeInfo(pub Option<String>);

/// The longest a `--check-save` may run before it is killed.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// How long an exited check's stdout is waited for.
const STDOUT_GRACE: Duration = Duration::from_millis(500);

/// A running `actor_testbed --check-save` for `agent`, polled by
/// [`poll_save_check`]. Dropped while the child still runs (Back, the
/// timeout, the menu closing), it kills the child.
#[derive(Resource)]
pub struct SaveCheck {
    pub agent: String,
    /// Where the world starts when the save fits.
    pub mode: RunMode,
    child: Child,
    /// The child's whole stdout, sent when it closes.
    stdout: Mutex<Receiver<String>>,
    stderr: Option<StderrTail>,
    deadline: Instant,
}

impl SaveCheck {
    /// Spawns `cmd` (stdout and stderr piped, no stdin) as the check for
    /// `agent`, killed after `timeout`.
    ///
    /// # Errors
    /// When the child cannot start.
    pub fn spawn(
        mut cmd: Command,
        agent: String,
        mode: RunMode,
        timeout: Duration,
    ) -> std::io::Result<Self> {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (tx, rx) = mpsc::channel();
        if let Some(mut out) = child.stdout.take() {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = out.read_to_end(&mut buf);
                let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            });
        }
        let stderr = child.stderr.take().map(StderrTail::spawn);
        Ok(Self {
            agent,
            mode,
            child,
            stdout: Mutex::new(rx),
            stderr,
            deadline: Instant::now() + timeout,
        })
    }

    /// The exited child's stdout (empty when it did not close in time).
    fn stdout(&self) -> String {
        self.stdout
            .lock()
            .ok()
            .and_then(|rx| rx.recv_timeout(STDOUT_GRACE).ok())
            .unwrap_or_default()
    }
}

impl Drop for SaveCheck {
    fn drop(&mut self) {
        run::kill(&mut self.child);
    }
}

/// The mismatch dialog: `agent`'s save and the check's error. `can_load` is
/// false for an unreadable save (only Fresh world is offered).
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct MismatchDialog {
    pub agent: String,
    pub mode: RunMode,
    pub error: String,
    pub can_load: bool,
}

/// The explorer's screens.
pub struct ExplorerPlugin;

impl Plugin for ExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ResumeNotice>()
            .init_resource::<ResumeInfo>()
            .init_resource::<RunScan>()
            .init_resource::<CheckedNames>()
            .add_systems(OnEnter(Screen::ExplorerNew), spawn_new)
            .add_systems(OnEnter(Screen::ExplorerResume), spawn_resume)
            .add_systems(OnEnter(Screen::ExplorerCheck), spawn_check)
            .add_systems(OnExit(Screen::ExplorerCheck), |mut commands: Commands| {
                // Back while checking: the check is killed (`SaveCheck`'s drop).
                commands.remove_resource::<SaveCheck>();
            })
            .add_systems(OnEnter(Screen::ExplorerMismatch), spawn_mismatch)
            .add_systems(
                Update,
                (
                    (refresh_new, enter_starts).run_if(in_state(Screen::ExplorerNew)),
                    poll_save_check.run_if(in_state(Screen::ExplorerCheck)),
                    finish_run_scan.run_if(in_state(Screen::ExplorerResume)),
                ),
            );
    }
}

/// A seed from the wall clock (app code: the menu is not sim code).
fn wall_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        // The low 64 bits of the nanoseconds: they change on every call.
        .map_or(0, |d| d.as_nanos() as u64)
}

fn spawn_new(
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut checked: ResMut<CheckedNames>,
) {
    checked.0 = None;
    let seed = wall_seed();
    let agent = random_name(NameKind::Agent, seed);
    let world = random_name(NameKind::World, seed.rotate_left(17) ^ 0x5DEE_CE66);
    let problem = launch::new_world_problem(&settings.0.agents_dir, &agent, &world);
    let folder = launch::agent_folder_line(&settings.0.agents_dir, &agent);
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::ExplorerNew),
        "New agent",
    );
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(
            "Name the agent and its world (or keep the random names).",
            16.0,
            ui::DIM,
        ));
        p.spawn(ui::label("Agent", 18.0, TEXT));
        name_row(p, NameKind::Agent, &agent, 0);
        p.spawn(ui::label("World", 18.0, TEXT));
        name_row(p, NameKind::World, &world, 2);
        p.spawn((
            NewProblemText,
            ui::label(problem.clone().unwrap_or_default(), 14.0, ERROR),
        ));
        p.spawn((NewFolderText, ui::label(folder, 12.0, ui::DIM)));
        ui::row(p, |r| {
            for (text, mode, tab) in [
                ("Start in window", RunMode::Window, 4),
                ("Start detached", RunMode::Detached, 5),
            ] {
                let start = button(
                    r,
                    text,
                    None,
                    problem.is_none(),
                    tab,
                    Action::StartNew(mode),
                );
                r.commands().entity(start).insert(StartButton);
            }
            back_button(r, Screen::ExplorerNew, 6);
        });
        p.spawn(ui::label(
            "Detached: no window; the menu shows the prompt, the port and the tick, and can \
             open it in the window later.",
            12.0,
            ui::DIM,
        ));
    });
}

/// A name field with its dice button (`tab`, `tab + 1`).
fn name_row(p: &mut ChildSpawnerCommands, kind: NameKind, name: &str, tab: i32) {
    p.spawn(Node {
        column_gap: px(10),
        align_items: AlignItems::Center,
        width: percent(100),
        ..default()
    })
    .with_children(|r| {
        let mut field = r.spawn((NameField(kind), ui::text_field(name, tab)));
        if kind == NameKind::Agent {
            field.insert(AutoFocus);
        }
        button(r, "Random", None, true, tab + 1, Action::RandomName(kind));
    });
}

/// The two names as typed.
fn names(fields: impl IntoIterator<Item = (NameKind, String)>) -> (String, String) {
    let (mut agent, mut world) = (String::new(), String::new());
    for (kind, text) in fields {
        match kind {
            NameKind::Agent => agent = text,
            NameKind::World => world = text,
        }
    }
    (agent, world)
}

/// Re-checks the names when they change: the problem line, the agent
/// folder line and Start.
fn refresh_new(
    mut commands: Commands,
    settings: Res<AppSettings>,
    mut checked: ResMut<CheckedNames>,
    fields: Query<(&NameField, &EditableText)>,
    (start, children, mut colors): (
        Query<Entity, With<StartButton>>,
        Query<&Children>,
        Query<&mut TextColor>,
    ),
    mut lines: Query<(&mut Text, Has<NewFolderText>), NewLine>,
) {
    let now = names(fields.iter().map(|(f, t)| (f.0, t.value().to_string())));
    if checked.0.as_ref() == Some(&now) {
        return;
    }
    let root = &settings.0.agents_dir;
    let problem = launch::new_world_problem(root, &now.0, &now.1);
    let folder = launch::agent_folder_line(root, &now.0);
    checked.0 = Some(now);
    for (mut t, is_folder) in &mut lines {
        t.0 = if is_folder {
            folder.clone()
        } else {
            problem.clone().unwrap_or_default()
        };
    }
    for b in &start {
        ui::set_enabled(&mut commands, b, problem.is_none(), &children, &mut colors);
    }
}

/// Enter in a name field presses Start in window (the fields take no
/// newlines).
fn enter_starts(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    fields: Query<(), With<NameField>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) && focus.get().is_some_and(|e| fields.contains(e)) {
        commands.queue(|world: &mut World| {
            apply_action(world, Action::StartNew(RunMode::Window));
        });
    }
}

/// The dice: another random name in the `kind` field.
pub fn reroll_name(world: &mut World, kind: NameKind) {
    let name = random_name(kind, wall_seed());
    let mut fields = world.query::<(&NameField, &mut EditableText)>();
    for (field, mut text) in fields.iter_mut(world) {
        if field.0 == kind {
            text.editor_mut().set_text(&name);
            text.pending_edits.clear();
            text.queue_edit(TextEdit::TextEnd(false));
        }
    }
}

/// Start in window / Start detached: launches the new agent when the names
/// are still fine.
pub fn start_new(world: &mut World, mode: RunMode) {
    let mut fields = world.query::<(&NameField, &EditableText)>();
    let (agent, world_name) = names(
        fields
            .iter(world)
            .map(|(f, t)| (f.0, t.value().to_string())),
    );
    let root = world.resource::<AppSettings>().0.agents_dir.clone();
    if launch::new_world_problem(&root, &agent, &world_name).is_some() {
        return;
    }
    run::launch(
        world,
        Entry::ExplorerNew {
            agent: agent.trim().to_owned(),
            world: world_name.trim().to_owned(),
            mode,
        },
    );
}

/// How a Resume row's world runs right now.
enum RowState {
    /// Not running: Start in window (the row) or detached.
    Idle,
    /// Running detached (tracked or adopted): Open / Stop.
    Detached {
        port: u16,
        starting: bool,
        busy: bool,
    },
    /// Running in a window (another menu's, or started by hand).
    Window { port: u16 },
    /// A `run.json` is there: whether its process lives is being asked
    /// (off the frame, [`RunScan`]).
    Checking,
}

/// The Resume screen's check of the `run.json` files of agents the menu
/// does not track: asking the OS about a pid can take a while, so it runs on
/// a thread and the rows are rebuilt when it answers ([`finish_run_scan`]).
/// A `run.json` whose pid is clearly dead is deleted there; one the OS
/// cannot answer for counts as running.
#[derive(Resource, Default)]
pub struct RunScan {
    pending: Option<Rx<Vec<ScanAnswer>>>,
    /// The listed agents and the name column, for the rebuilt rows.
    agents: Vec<AgentEntry>,
    name_px: f32,
}

impl RunScan {
    /// The check is in flight.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// One row's answer: its index and the live `run.json` (if any).
type ScanAnswer = (usize, Option<RunFile>);

/// A Resume row (its index in the list), rebuilt when the check answers.
#[derive(Component)]
struct ResumeRow(usize);

/// The live `run.json` in `dir`: `None` when there is none or its pid is
/// clearly dead (the stale file is deleted). A background thread only.
fn scan_run_file(dir: &testbed_content::explorer::home::AgentDir) -> Option<RunFile> {
    let run = RunFile::read_any(dir)?;
    if pid_dead(run.pid) {
        if let Err(e) = run.remove(dir) {
            warn!(
                "game_menu: cannot remove the stale {}: {e}",
                RunFile::path(dir).display()
            );
        }
        return None;
    }
    Some(run)
}

/// The state of a row once its `run.json` was checked.
fn checked_state(worlds: &mut DetachedWorlds, a: &AgentEntry, run: Option<RunFile>) -> RowState {
    match run {
        Some(run) if run.mode == RunMode::Window => RowState::Window { port: run.port },
        Some(run) => {
            detached::adopt(worlds, &run, &a.dir);
            row_state(worlds, a)
        }
        None => RowState::Idle,
    }
}

/// What runs where: tracked worlds first, then live `run.json` files
/// (a detached one is adopted).
fn row_state(worlds: &mut DetachedWorlds, a: &AgentEntry) -> RowState {
    if worlds.get(&a.meta.agent).is_none() {
        return if RunFile::path(&a.dir).exists() {
            RowState::Checking
        } else {
            RowState::Idle
        };
    }
    worlds
        .get(&a.meta.agent)
        .map_or(RowState::Idle, |w| RowState::Detached {
            port: w.port,
            starting: w.is_starting(),
            busy: !w.is_running(),
        })
}

fn spawn_resume(
    mut commands: Commands,
    settings: Res<AppSettings>,
    (mut notice, mut info): (ResMut<ResumeNotice>, ResMut<ResumeInfo>),
    (mut worlds, mut scan): (ResMut<DetachedWorlds>, ResMut<RunScan>),
) {
    let notice = notice.0.take();
    let info = info.0.take();
    let agents = list_agents(&settings.0.agents_dir);
    let dir = settings.0.agents_dir.display().to_string();
    let states: Vec<RowState> = agents.iter().map(|a| row_state(&mut worlds, a)).collect();
    let checks: Vec<(usize, testbed_content::explorer::home::AgentDir)> = states
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s, RowState::Checking))
        .map(|(i, _)| (i, agents[i].dir.clone()))
        .collect();
    scan.pending = None;
    if !checks.is_empty() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let found = checks
                .into_iter()
                .map(|(i, dir)| (i, scan_run_file(&dir)))
                .collect();
            let _ = tx.send(found);
        });
        scan.pending = Some(Rx::new(rx));
    }
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::ExplorerResume),
        "Resume an agent",
    );
    // Fira Mono at 18 px is ~11 px a character.
    let longest = agents.iter().map(|a| a.meta.agent.chars().count()).max();
    let name_px = longest.map_or(ui::NAME_COLUMN_PX, |n| {
        (n as f32 * 11.0 + 8.0).clamp(ui::NAME_COLUMN_PX, 320.0)
    });
    scan.agents.clone_from(&agents);
    scan.name_px = name_px;
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(format!("Agents folder: {dir}"), 14.0, ui::DIM));
        let mut focused = false;
        if agents.is_empty() {
            p.spawn(ui::label(
                "No agents in this folder yet: start a New agent.",
                16.0,
                NOTE,
            ));
        } else {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    overflow: Overflow::scroll_y(),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_height: px(0),
                    ..default()
                },
                ScrollArea,
            ))
            .with_children(|list| {
                for (i, (a, state)) in agents.iter().zip(&states).enumerate() {
                    let tab = i32::try_from(i * 3).unwrap_or(i32::MAX);
                    list.spawn((
                        ResumeRow(i),
                        Node {
                            column_gap: px(8),
                            align_items: AlignItems::Center,
                            flex_shrink: 0.0,
                            ..default()
                        },
                    ))
                    .with_children(|row| {
                        resume_row(row, a, state, (name_px, tab), &mut focused);
                    });
                }
            });
        }
        if let Some(m) = &info {
            p.spawn(ui::label(m.clone(), 14.0, OK));
        }
        if let Some(e) = &notice {
            p.spawn(ui::label(e.clone(), 14.0, ERROR));
        }
        ui::row(p, |r| {
            if let Some(back) = Screen::ExplorerResume.back() {
                let b = button(r, "Back", None, true, 10_000, Action::Go(back));
                if !focused {
                    r.commands().entity(b).insert(AutoFocus);
                }
            }
        });
    });
}

/// The `run.json` check answered: the rows it was about are rebuilt (a
/// detached world is adopted, a window's is disabled, a stale one is gone).
fn finish_run_scan(
    mut commands: Commands,
    mut scan: ResMut<RunScan>,
    mut worlds: ResMut<DetachedWorlds>,
    rows: Query<(Entity, &ResumeRow)>,
) {
    let Some(rx) = &scan.pending else {
        return;
    };
    let found = match rx.try_recv() {
        Ok(found) => found,
        Err(std::sync::mpsc::TryRecvError::Empty) => return,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
    };
    scan.pending = None;
    for (i, run) in found {
        let Some(a) = scan.agents.get(i).cloned() else {
            continue;
        };
        let state = checked_state(&mut worlds, &a, run);
        let tab = i32::try_from(i * 3).unwrap_or(i32::MAX);
        let name_px = scan.name_px;
        for (row, _) in rows.iter().filter(|(_, r)| r.0 == i) {
            commands
                .entity(row)
                .despawn_related::<Children>()
                .with_children(|r| {
                    // The focus stays where it is.
                    resume_row(r, &a, &state, (name_px, tab), &mut true);
                });
        }
    }
}

/// One Resume row: the agent's button (start in window, or its Detached
/// screen) and its small buttons. The first enabled agent button gets the
/// focus (`focused`).
fn resume_row(
    row: &mut ChildSpawnerCommands,
    a: &AgentEntry,
    state: &RowState,
    (name_px, tab): (f32, i32),
    focused: &mut bool,
) {
    let agent = a.meta.agent.clone();
    let world = &a.meta.world;
    let resume = |mode| {
        Action::Launch(Entry::ExplorerResume {
            agent: agent.clone(),
            choice: None,
            mode,
        })
    };
    let (detail, action, enabled) = match state {
        RowState::Idle => {
            let saved = a
                .has_save
                .then(|| launch::saved_date(&a.dir.save(world)))
                .flatten();
            (
                launch::resume_detail(world, saved.as_deref(), a.has_save),
                resume(RunMode::Window),
                a.has_save,
            )
        }
        RowState::Detached { starting: true, .. } => (
            format!("{world}  -  starting detached..."),
            Action::Watch(agent.clone()),
            true,
        ),
        RowState::Detached { port, .. } => (
            format!("{world}  -  running detached on port {port} - Open / Stop"),
            Action::Watch(agent.clone()),
            true,
        ),
        RowState::Window { port } => (
            format!("{world}  -  running in a window (port {port})"),
            resume(RunMode::Window),
            false,
        ),
        RowState::Checking => (
            format!("{world}  -  checking whether it runs..."),
            resume(RunMode::Window),
            false,
        ),
    };
    let id = ui::list_button(row, (&a.meta.agent, name_px), &detail, enabled, tab, action);
    row.commands().entity(id).observe(on_activate);
    ui::grow(&mut row.commands(), id);
    let mut small = |text: &str, on: bool, t: i32, action: Action| {
        let b = ui::small_button(row, text, on, t, action);
        row.commands().entity(b).observe(on_activate);
    };
    match state {
        RowState::Idle | RowState::Window { .. } | RowState::Checking => small(
            "Start detached",
            matches!(state, RowState::Idle) && a.has_save,
            tab + 1,
            resume(RunMode::Detached),
        ),
        RowState::Detached { starting, busy, .. } => {
            let on = !starting && !busy;
            small("Open", on, tab + 1, Action::OpenInWindow(agent.clone()));
            small("Stop", on, tab + 2, Action::StopDetached(agent.clone()));
        }
    }
    if enabled && !*focused {
        row.commands().entity(id).insert(AutoFocus);
        *focused = true;
    }
}

/// Resume's first step: runs `actor_testbed --check-save` for `agent` (no
/// window) with its output read on threads, and shows the Checking screen;
/// the Missing screen when `actor_testbed` is not built. Does nothing while
/// [`run::busy`] (a game, a build or another check).
pub fn start_check(world: &mut World, agent: String, mode: RunMode) {
    if run::busy(world) {
        return;
    }
    let settings = world.resource::<AppSettings>().0.clone();
    let exe_dir = world.resource::<Paths>().exe_dir.clone();
    let Some(exe) = launch::find_exe("actor_testbed", &exe_dir) else {
        let entry = Entry::ExplorerResume {
            agent,
            choice: None,
            mode,
        };
        world.insert_resource(Missing::new("actor_testbed", entry, &exe_dir));
        run::set_screen(world, Screen::Missing);
        return;
    };
    let args = launch::check_args(&agent, &settings);
    info!(
        "game_menu: checking the save: {} {}",
        exe.display(),
        launch::command_line(&args)
    );
    let mut cmd = Command::new(&exe);
    cmd.args(&args);
    match SaveCheck::spawn(cmd, agent, mode, CHECK_TIMEOUT) {
        Ok(check) => {
            world.insert_resource(check);
            run::set_screen(world, Screen::ExplorerCheck);
        }
        Err(e) => {
            world.resource_mut::<ResumeNotice>().0 =
                Some(format!("cannot run {} --check-save: {e}", exe.display()));
            run::set_screen(world, Screen::ExplorerResume);
        }
    }
}

fn spawn_check(mut commands: Commands, check: Option<Res<SaveCheck>>) {
    let Some(check) = check else {
        commands.queue(|w: &mut World| run::set_screen(w, Screen::ExplorerResume));
        return;
    };
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::ExplorerCheck),
        "Resume an agent",
    );
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(
            format!("Checking {}'s save...", check.agent),
            18.0,
            TEXT,
        ));
        p.spawn(ui::label(
            "Does the world fit this version of the game?",
            14.0,
            ui::DIM,
        ));
        ui::row(p, |r| back_button(r, Screen::ExplorerCheck, 0));
    });
}

/// Polls the check: exit 0 or 1 is its answer (`ok` starts the game,
/// anything else opens the mismatch dialog); another exit, a failed wait or
/// the timeout (the child is killed) goes back to Resume with the reason.
fn poll_save_check(world: &mut World) {
    let Some(mut check) = world.get_resource_mut::<SaveCheck>() else {
        return;
    };
    let status = match check.child.try_wait() {
        Ok(None) if Instant::now() < check.deadline => return,
        Ok(None) => Err("the save check timed out".to_owned()),
        Ok(Some(status)) => Ok(status.code()),
        Err(e) => Err(format!("the save check failed: {e}")),
    };
    let Some(check) = world.remove_resource::<SaveCheck>() else {
        return;
    };
    let stdout = match status {
        Ok(Some(0 | 1)) => check.stdout(),
        Ok(code) => {
            let why = check.stderr.as_ref().and_then(StderrTail::last_line);
            back_to_resume(world, launch::check_failed(code, why.as_deref()));
            return;
        }
        Err(e) => {
            // A timed-out child is killed here (`SaveCheck`'s drop).
            drop(check);
            back_to_resume(world, e);
            return;
        }
    };
    let (agent, mode) = (check.agent.clone(), check.mode);
    drop(check);
    let result = launch::parse_check(&stdout);
    let (error, can_load) = match result {
        CheckResult::Ok => {
            let entry = Entry::ExplorerResume {
                agent,
                choice: None,
                mode,
            };
            run::start_game(world, entry);
            return;
        }
        CheckResult::Mismatch(error) => (error, true),
        CheckResult::Unreadable(error) => (error, false),
    };
    world.insert_resource(MismatchDialog {
        agent,
        mode,
        error,
        can_load,
    });
    run::set_screen(world, Screen::ExplorerMismatch);
}

/// The Resume screen with `error` as its notice.
fn back_to_resume(world: &mut World, error: String) {
    world.resource_mut::<ResumeNotice>().0 = Some(error);
    run::set_screen(world, Screen::ExplorerResume);
}

fn spawn_mismatch(mut commands: Commands, dialog: Option<Res<MismatchDialog>>) {
    let Some(dialog) = dialog else {
        commands.queue(|w: &mut World| run::set_screen(w, Screen::ExplorerResume));
        return;
    };
    let title = if dialog.can_load {
        "The save is from another version"
    } else {
        "The save cannot be read"
    };
    let panel = ui::spawn_screen(
        &mut commands,
        DespawnOnExit(Screen::ExplorerMismatch),
        title,
    );
    let resume = |choice| {
        Action::Launch(Entry::ExplorerResume {
            agent: dialog.agent.clone(),
            choice: Some(choice),
            mode: dialog.mode,
        })
    };
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(format!("{}'s world", dialog.agent), 16.0, ui::DIM));
        let line = if dialog.can_load {
            format!(
                "This world was saved by another version of the game: {}",
                dialog.error
            )
        } else {
            dialog.error.clone()
        };
        p.spawn(ui::label(line, 16.0, ERROR));
        if dialog.can_load {
            p.spawn(ui::label(
                "Load anyway: load what still fits; what was skipped or reset is listed in the game.",
                14.0,
                ui::DIM,
            ));
        }
        p.spawn(ui::label(
            "Fresh world, keep the diary: a new world with the same names; the diary and \
             proposals stay, the old save is kept next to them.",
            14.0,
            ui::DIM,
        ));
        ui::row(p, |r| {
            let mut first = None;
            if dialog.can_load {
                first = Some(button(r, "Load anyway", None, true, 0, resume(Mismatch::LoadAnyway)));
            }
            let fresh = button(
                r,
                "Fresh world, keep the diary",
                None,
                true,
                1,
                resume(Mismatch::FreshWorld),
            );
            r.commands()
                .entity(first.unwrap_or(fresh))
                .insert(AutoFocus);
            back_button(r, Screen::ExplorerMismatch, 2);
        });
    });
}

#[cfg(test)]
mod tests {
    use bevy::ui::InteractionDisabled;
    use testbed_content::explorer::home::create_agent;

    use super::*;
    use crate::screens::tests::{app, button_for, press, screen, test_child, tmp};

    fn open(app: &mut App, s: Screen) {
        run::set_screen(app.world_mut(), s);
        app.update();
        app.update();
    }

    fn field_text(app: &mut App, kind: NameKind) -> String {
        let mut q = app.world_mut().query::<(&NameField, &EditableText)>();
        q.iter(app.world())
            .find(|(f, _)| f.0 == kind)
            .map(|(_, t)| t.value().to_string())
            .unwrap()
    }

    fn type_name(app: &mut App, kind: NameKind, text: &str) {
        let mut q = app.world_mut().query::<(&NameField, &mut EditableText)>();
        for (f, mut t) in q.iter_mut(app.world_mut()) {
            if f.0 == kind {
                *t = EditableText::new(text);
            }
        }
        app.update();
        app.update();
    }

    /// Both Start buttons are disabled (they always agree).
    fn start_disabled(app: &mut App) -> bool {
        let mut q = app
            .world_mut()
            .query_filtered::<Has<InteractionDisabled>, With<StartButton>>();
        let all: Vec<bool> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 2, "Start in window and Start detached");
        assert!(all.iter().all(|d| *d == all[0]), "{all:?}");
        all[0]
    }

    fn problem(app: &mut App) -> String {
        let mut q = app
            .world_mut()
            .query_filtered::<&Text, With<NewProblemText>>();
        q.single(app.world()).unwrap().0.clone()
    }

    #[test]
    fn new_screen_prefills_names_and_blocks_existing_agents() {
        let d = tmp("x-new");
        let agents = d.join("agents");
        let mut app = app(&d, &agents);
        open(&mut app, Screen::ExplorerNew);
        let agent = field_text(&mut app, NameKind::Agent);
        let world = field_text(&mut app, NameKind::World);
        assert!(
            agent.contains(' ') && !world.is_empty(),
            "{agent} / {world}"
        );
        // The agents folder does not exist yet: nothing blocks Start.
        assert!(!start_disabled(&mut app), "{}", problem(&mut app));
        create_agent(&agents, "Wren Ashdown", "Hollowmere", "2026-10-25").unwrap();
        type_name(&mut app, NameKind::Agent, "wren ashdown");
        assert!(start_disabled(&mut app));
        assert_eq!(
            problem(&mut app),
            "an agent with this name exists: resume it instead"
        );
        // Start does nothing then.
        apply_action(app.world_mut(), Action::StartNew(RunMode::Window));
        apply_action(app.world_mut(), Action::StartNew(RunMode::Detached));
        app.update();
        assert_eq!(screen(&app), Screen::ExplorerNew);
        type_name(&mut app, NameKind::Agent, "Brin Marlow");
        assert!(!start_disabled(&mut app));
        assert_eq!(problem(&mut app), "");
        type_name(&mut app, NameKind::World, "  ");
        assert!(start_disabled(&mut app));
        assert_eq!(problem(&mut app), "World name: enter a name");
        // A name without letters or digits has no folder name.
        type_name(&mut app, NameKind::World, "Hollowmere");
        type_name(&mut app, NameKind::Agent, "?!");
        assert!(start_disabled(&mut app));
        assert_eq!(
            problem(&mut app),
            "Agent name: the name has no letters or digits a folder can use"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn new_screen_names_the_agent_folder() {
        let d = tmp("x-folder");
        let agents = d.join("agents");
        let mut app = app(&d, &agents);
        open(&mut app, Screen::ExplorerNew);
        let line = |name: &str| format!("open your AI tool in {}", agents.join(name).display());
        let has = |app: &mut App, text: &str| texts(app).iter().any(|t| t.contains(text));
        type_name(&mut app, NameKind::Agent, " Brin Marlow ");
        // The folder actor_testbed will use: `<agents>/<slug(agent)>`.
        assert!(has(&mut app, &line("brin-marlow")), "{:?}", texts(&mut app));
        // It follows the name as it is typed.
        type_name(&mut app, NameKind::Agent, "Wren Ashdown");
        assert!(
            has(&mut app, &line("wren-ashdown")),
            "{:?}",
            texts(&mut app)
        );
        assert!(!has(&mut app, "brin-marlow"));
        // A name without a folder name names no folder.
        type_name(&mut app, NameKind::Agent, "?!");
        assert!(
            !has(&mut app, "open your AI tool in"),
            "{:?}",
            texts(&mut app)
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dice_rerolls_one_field() {
        let d = tmp("x-dice");
        let mut app = app(&d, &d.join("agents"));
        open(&mut app, Screen::ExplorerNew);
        type_name(&mut app, NameKind::Agent, "typed by hand");
        let world = field_text(&mut app, NameKind::World);
        let dice = button_for(&mut app, |a| {
            matches!(a, Action::RandomName(NameKind::Agent))
        });
        press(&mut app, dice);
        let agent = field_text(&mut app, NameKind::Agent);
        assert_ne!(agent, "typed by hand");
        assert!(agent.contains(' '), "{agent}");
        assert_eq!(field_text(&mut app, NameKind::World), world);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn start_launches_the_new_agent() {
        let d = tmp("x-start");
        let mut app = app(&d, &d.join("agents"));
        open(&mut app, Screen::ExplorerNew);
        type_name(&mut app, NameKind::Agent, " Brin Marlow ");
        type_name(&mut app, NameKind::World, "Gloomhollow");
        let start = button_for(&mut app, |a| matches!(a, Action::StartNew(RunMode::Window)));
        press(&mut app, start);
        app.update();
        // No actor_testbed next to the test binary: the Missing screen, with
        // the trimmed names.
        assert_eq!(screen(&app), Screen::Missing);
        let missing = app.world().resource::<Missing>();
        assert_eq!(
            missing.entry,
            Entry::ExplorerNew {
                agent: "Brin Marlow".into(),
                world: "Gloomhollow".into(),
                mode: RunMode::Window,
            }
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    fn resume_buttons(app: &mut App) -> Vec<(String, bool)> {
        let mut q = app
            .world_mut()
            .query::<(&Action, Has<InteractionDisabled>)>();
        q.iter(app.world())
            .filter_map(|(a, off)| match a {
                Action::Launch(Entry::ExplorerResume {
                    agent,
                    mode: RunMode::Window,
                    ..
                }) => Some((agent.clone(), !off)),
                _ => None,
            })
            .collect()
    }

    fn texts(app: &mut App) -> Vec<String> {
        let mut q = app.world_mut().query::<&Text>();
        q.iter(app.world()).map(|t| t.0.clone()).collect()
    }

    #[test]
    fn resume_lists_agents_and_disables_those_without_a_save() {
        let d = tmp("x-resume");
        let agents = d.join("agents");
        let wren = create_agent(&agents, "Wren Ashdown", "Hollowmere", "2026-10-24").unwrap();
        std::fs::write(
            wren.save("Hollowmere"),
            r#"{"header":{"saved":"2026-10-25T01:46:07Z"},"entities":[]}"#,
        )
        .unwrap();
        create_agent(&agents, "Brin Marlow", "Gloomhollow", "2026-10-24").unwrap();
        let mut app = app(&d, &agents);
        open(&mut app, Screen::ExplorerResume);
        let mut buttons = resume_buttons(&mut app);
        buttons.sort();
        assert_eq!(
            buttons,
            vec![("Brin Marlow".into(), false), ("Wren Ashdown".into(), true)]
        );
        let texts = texts(&mut app);
        assert!(
            texts.contains(&"Hollowmere  -  last saved 2026-10-25 01:46 UTC".to_owned()),
            "{texts:?}"
        );
        assert!(texts.contains(&"Gloomhollow  -  no save yet".to_owned()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resume_on_a_missing_folder_lists_nothing() {
        let d = tmp("x-empty");
        let mut app = app(&d, &d.join("not-created"));
        open(&mut app, Screen::ExplorerResume);
        assert!(resume_buttons(&mut app).is_empty());
        assert!(texts(&mut app).iter().any(|t| t.contains("No agents")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_resume_notice_shows_once() {
        let d = tmp("x-notice");
        let mut app = app(&d, &d.join("agents"));
        open(&mut app, Screen::ExplorerResume);
        app.world_mut().resource_mut::<ResumeNotice>().0 = Some("cannot run it".into());
        // Re-entering the same screen (start_check's failure path).
        open(&mut app, Screen::ExplorerResume);
        assert!(texts(&mut app).contains(&"cannot run it".to_owned()));
        open(&mut app, Screen::Explorer);
        open(&mut app, Screen::ExplorerResume);
        assert!(!texts(&mut app).contains(&"cannot run it".to_owned()));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Starts a check whose child prints `stdout` and `stderr`, then exits
    /// with `code` (or hangs, for `None`), and lands on the Checking screen.
    fn start_fake_check(app: &mut App, code: Option<i32>, stdout: &str, stderr: &str) -> u32 {
        start_check_with_timeout(app, code, stdout, stderr, CHECK_TIMEOUT)
    }

    fn start_check_with_timeout(
        app: &mut App,
        code: Option<i32>,
        stdout: &str,
        stderr: &str,
        timeout: Duration,
    ) -> u32 {
        let cmd = test_child(code, stdout, stderr);
        let check = SaveCheck::spawn(cmd, "Wren Ashdown".into(), RunMode::Window, timeout).unwrap();
        let pid = check.child.id();
        app.insert_resource(check);
        open(app, Screen::ExplorerCheck);
        pid
    }

    /// Updates until the screen leaves Checking.
    fn settle(app: &mut App) {
        for _ in 0..3000 {
            app.update();
            if screen(app) != Screen::ExplorerCheck {
                app.update();
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the check never finished");
    }

    /// The check finished with `stdout` (exit 0).
    fn checked(app: &mut App, stdout: &str) {
        start_fake_check(app, Some(0), stdout, "");
        settle(app);
    }

    fn launches(app: &mut App) -> Vec<Option<Mismatch>> {
        let mut q = app.world_mut().query::<&Action>();
        let mut v: Vec<Option<Mismatch>> = q
            .iter(app.world())
            .filter_map(|a| match a {
                Action::Launch(Entry::ExplorerResume { choice, .. }) => Some(*choice),
                _ => None,
            })
            .collect();
        v.sort_by_key(|c| format!("{c:?}"));
        v
    }

    #[test]
    fn a_mismatch_offers_both_choices() {
        let d = tmp("x-mismatch");
        let mut app = app(&d, &d.join("agents"));
        checked(
            &mut app,
            r#"{"ok":false,"error":"the content hash differs","kind":"content_hash"}"#,
        );
        assert_eq!(screen(&app), Screen::ExplorerMismatch);
        assert!(!app.world().contains_resource::<SaveCheck>());
        assert_eq!(
            launches(&mut app),
            vec![Some(Mismatch::FreshWorld), Some(Mismatch::LoadAnyway)]
        );
        let texts = texts(&mut app);
        assert!(
            texts.contains(
                &"This world was saved by another version of the game: the content hash differs"
                    .to_owned()
            )
        );
        // Load anyway starts actor_testbed with the flag (here: Missing).
        let load = button_for(&mut app, |a| {
            matches!(
                a,
                Action::Launch(Entry::ExplorerResume {
                    choice: Some(Mismatch::LoadAnyway),
                    ..
                })
            )
        });
        press(&mut app, load);
        app.update();
        assert_eq!(screen(&app), Screen::Missing);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_unreadable_save_offers_only_a_fresh_world() {
        let d = tmp("x-unreadable");
        let mut app = app(&d, &d.join("agents"));
        checked(
            &mut app,
            r#"{"ok":false,"error":"unreadable save x: EOF","kind":"unreadable"}"#,
        );
        assert_eq!(screen(&app), Screen::ExplorerMismatch);
        assert_eq!(launches(&mut app), vec![Some(Mismatch::FreshWorld)]);
        assert!(texts(&mut app).contains(&"unreadable save x: EOF".to_owned()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_ok_check_starts_the_game() {
        let d = tmp("x-ok");
        let mut app = app(&d, &d.join("agents"));
        checked(&mut app, "{\"ok\":true}\n");
        app.update();
        // Not built next to the test binary: Missing, for a plain resume.
        assert_eq!(screen(&app), Screen::Missing);
        assert_eq!(
            app.world().resource::<Missing>().entry,
            Entry::ExplorerResume {
                agent: "Wren Ashdown".into(),
                choice: None,
                mode: RunMode::Window,
            }
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_crashed_check_is_reported_on_resume() {
        let d = tmp("x-crash");
        let mut app = app(&d, &d.join("agents"));
        open(&mut app, Screen::ExplorerResume);
        start_fake_check(
            &mut app,
            Some(2),
            "not json\n",
            "INFO checking\nerror: --world cannot be used with --resume\n",
        );
        settle(&mut app);
        // No mismatch dialog: the exit code and the last stderr line.
        assert_eq!(screen(&app), Screen::ExplorerResume);
        assert!(!app.world().contains_resource::<MismatchDialog>());
        let texts = texts(&mut app);
        assert!(
            texts.contains(
                &"the save check failed (exit 2): error: --world cannot be used with --resume"
                    .to_owned()
            ),
            "{texts:?}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_check_that_exits_1_is_still_its_answer() {
        let d = tmp("x-exit1");
        let mut app = app(&d, &d.join("agents"));
        start_fake_check(
            &mut app,
            Some(1),
            r#"{"ok":false,"error":"the content hash differs","kind":"content_hash"}"#,
            "WARN mismatch\n",
        );
        settle(&mut app);
        assert_eq!(screen(&app), Screen::ExplorerMismatch);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_hung_check_is_killed_after_the_timeout() {
        let d = tmp("x-hung");
        let mut app = app(&d, &d.join("agents"));
        open(&mut app, Screen::ExplorerResume);
        let started = Instant::now();
        start_check_with_timeout(&mut app, None, "", "", Duration::from_millis(300));
        settle(&mut app);
        // `settle` returning means the child was reaped: killed, not waited out.
        assert!(started.elapsed() < Duration::from_secs(30));
        assert_eq!(screen(&app), Screen::ExplorerResume);
        assert!(texts(&mut app).contains(&"the save check timed out".to_owned()));
        assert!(!app.world().contains_resource::<SaveCheck>());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn leaving_the_checking_screen_kills_the_check() {
        let d = tmp("x-back");
        let mut app = app(&d, &d.join("agents"));
        let started = Instant::now();
        start_fake_check(&mut app, None, "", "");
        assert!(app.world().contains_resource::<SaveCheck>());
        // Back (Escape) while checking: the drop kills and reaps the child,
        // which would otherwise sleep for minutes.
        let back = button_for(&mut app, |a| {
            matches!(a, Action::Go(Screen::ExplorerResume))
        });
        press(&mut app, back);
        app.update();
        assert_eq!(screen(&app), Screen::ExplorerResume);
        assert!(!app.world().contains_resource::<SaveCheck>());
        assert!(started.elapsed() < Duration::from_secs(30));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_check_in_flight_blocks_other_launches() {
        let d = tmp("x-busy");
        let mut app = app(&d, &d.join("agents"));
        let pid = start_fake_check(&mut app, None, "", "");
        // A second Resume, another game, a build: all ignored.
        apply_action(app.world_mut(), Action::Launch(Entry::Village));
        start_check(app.world_mut(), "Brin Marlow".into(), RunMode::Window);
        run::build(
            app.world_mut(),
            Entry::Village,
            "village",
            std::path::Path::new("cargo"),
            &d,
        );
        app.update();
        assert_eq!(screen(&app), Screen::ExplorerCheck);
        assert_eq!(app.world().resource::<SaveCheck>().child.id(), pid);
        assert!(!app.world().contains_resource::<Missing>());
        assert!(!app.world().contains_resource::<run::Running>());
        app.world_mut().remove_resource::<SaveCheck>();
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resume_without_actor_testbed_shows_missing() {
        let d = tmp("x-noexe");
        let mut app = app(&d, &d.join("agents"));
        start_check(app.world_mut(), "Wren".into(), RunMode::Window);
        app.update();
        assert_eq!(screen(&app), Screen::Missing);
        assert!(!app.world().contains_resource::<SaveCheck>());
        let _ = std::fs::remove_dir_all(&d);
    }
}
