//! Explorer mode (game menu and AI explorer spec §5.2–5.4): one agent's
//! world in the testbed, started New or resumed from its save, in the game
//! window.
//!
//! The window-free part (build or resume the world, saves, `--check-save`,
//! the agent folder's workspace, the notes resources) is
//! [`testbed_content::explorer::host`], shared with the detached host. This
//! module keeps what only the windowed app has: the flags, the remote port
//! probe, the prompt modal, the wall clock (the New seed, the `created` and
//! `saved` dates, the autosave interval), the save on window close and the
//! close dialog that keeps the world running detached
//! ([`install_close_to_detached`], F9).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use npc_framework::kernel::driver::NpcDriverSet;
use npc_framework::prelude::SimControl;
use npc_framework::view_helpers::ViewSystems;
use testbed_content::explorer::home::{AgentDir, slug};
pub use testbed_content::explorer::host::{ExplorerSession, Mismatch, Start};
use testbed_content::explorer::host::{
    HostArgs, RunFile, RunMode, build_world_with, detach_command, detached_resume_args,
    insert_notes, rfc3339, save_now, write_session_files,
};
use testbed_content::explorer::prompt::PromptKind;
use testbed_view::{ExplorerModalPlugin, ExplorerModalSystems, PromptModal, PromptModalClosed};

/// The explorer flags, validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerArgs {
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
    /// `--check-save`: report whether the save fits this build, no window.
    pub check_only: bool,
    /// `--exact-port N` (with `--resume`): a detached world moves into the
    /// window on its port `N`, waited for up to [`EXACT_PORT_WAIT`]; no
    /// prompt modal.
    pub exact_port: Option<u16>,
    /// `--close-dialog`: the "Keep it running detached?" dialog opens at
    /// start (visual checks, as a close request would open it).
    pub show_close_dialog: bool,
    /// `--preset ID`: a new agent's actor preset (default `human`); a
    /// resume keeps the agent's own (another one is an error).
    pub preset: Option<String>,
    /// `--actors-dir DIR`: the actor presets folder (default `npc-actors`
    /// beside the agents folder).
    pub actors_dir: Option<PathBuf>,
}

impl ExplorerArgs {
    /// The host's arguments.
    pub fn host_args(&self) -> HostArgs {
        HostArgs {
            home: self.home.clone(),
            agent: self.agent.clone(),
            world: self.world.clone(),
            resume: self.resume,
            on_mismatch: self.on_mismatch,
            preset: self.preset.clone(),
            actors_dir: self.actors_dir.clone(),
        }
    }
}

/// The explorer flags as `parse_args` collects them, before validation.
#[derive(Default)]
pub struct ExplorerFlags {
    home: Option<PathBuf>,
    agent: Option<String>,
    world: Option<String>,
    resume: bool,
    load_anyway: bool,
    fresh_world: bool,
    check_save: bool,
    close_dialog: bool,
    exact_port: Option<String>,
    preset: Option<String>,
    actors_dir: Option<PathBuf>,
}

impl ExplorerFlags {
    /// Takes `flag` when it is an explorer switch.
    pub fn switch(&mut self, flag: &str) -> bool {
        let slot = match flag {
            "--resume" => &mut self.resume,
            "--load-anyway" => &mut self.load_anyway,
            "--fresh-world" => &mut self.fresh_world,
            "--check-save" => &mut self.check_save,
            "--close-dialog" => &mut self.close_dialog,
            _ => return false,
        };
        *slot = true;
        true
    }

    /// Takes `flag value` when `flag` is an explorer flag with a value.
    pub fn value(&mut self, flag: &str, value: &str) -> bool {
        match flag {
            "--explorer-home" => self.home = Some(PathBuf::from(value)),
            "--agent" => self.agent = Some(value.to_owned()),
            "--world" => self.world = Some(value.to_owned()),
            "--exact-port" => self.exact_port = Some(value.to_owned()),
            "--preset" => self.preset = Some(value.to_owned()),
            "--actors-dir" => self.actors_dir = Some(PathBuf::from(value)),
            _ => return false,
        }
        true
    }

    /// The explorer mode, or `None` without `--explorer-home`.
    ///
    /// # Errors
    /// A usage message.
    pub fn finish(
        self,
        scenario_given: bool,
        seed_given: bool,
    ) -> Result<Option<ExplorerArgs>, String> {
        let Some(home) = self.home else {
            let any = self.agent.is_some()
                || self.world.is_some()
                || self.resume
                || self.load_anyway
                || self.fresh_world
                || self.check_save
                || self.close_dialog
                || self.exact_port.is_some()
                || self.preset.is_some()
                || self.actors_dir.is_some();
            return if any {
                Err("--agent, --world, --resume, --load-anyway, --fresh-world, \
                     --check-save, --preset, --actors-dir and --exact-port need \
                     --explorer-home DIR"
                    .into())
            } else {
                Ok(None)
            };
        };
        if scenario_given {
            return Err("--scenario cannot be used with --explorer-home \
                        (the explorer builds its own world)"
                .into());
        }
        if seed_given {
            return Err("--seed cannot be used with --explorer-home \
                        (a new world picks its seed, a resumed one keeps it)"
                .into());
        }
        let agent = match self.agent {
            Some(a) if !a.trim().is_empty() => a,
            Some(_) => return Err("--agent: the name is blank".into()),
            None => return Err("--explorer-home needs --agent NAME".into()),
        };
        if self.world.as_ref().is_some_and(|w| w.trim().is_empty()) {
            return Err("--world: the name is blank".into());
        }
        if self.preset.as_ref().is_some_and(|p| p.trim().is_empty()) {
            return Err("--preset: the name is blank".into());
        }
        if self.resume && self.world.is_some() {
            return Err(
                "--world cannot be used with --resume: the world comes from the \
                        agent's folder (its agent.json)"
                    .into(),
            );
        }
        if self.load_anyway && self.fresh_world {
            return Err("--load-anyway and --fresh-world exclude each other".into());
        }
        if (self.load_anyway || self.fresh_world) && !self.resume {
            return Err("--load-anyway and --fresh-world need --resume".into());
        }
        if !self.resume && !self.check_save && self.world.is_none() {
            return Err("a new agent needs --world NAME (or --resume an existing one)".into());
        }
        let exact_port = match self.exact_port {
            None => None,
            Some(v) => Some(
                v.parse::<u16>()
                    .ok()
                    .filter(|&p| p != 0)
                    .ok_or_else(|| format!("--exact-port: `{v}` is not a port"))?,
            ),
        };
        if exact_port.is_some() && self.check_save {
            return Err("--exact-port cannot be used with --check-save".into());
        }
        if exact_port.is_some() && !self.resume {
            return Err(
                "--exact-port needs --resume (it moves a running world into the window)".into(),
            );
        }
        let on_mismatch = if self.load_anyway {
            Some(Mismatch::LoadAnyway)
        } else if self.fresh_world {
            Some(Mismatch::FreshWorld)
        } else {
            None
        };
        Ok(Some(ExplorerArgs {
            home,
            agent,
            world: self.world,
            resume: self.resume,
            on_mismatch,
            check_only: self.check_save,
            exact_port,
            show_close_dialog: self.close_dialog,
            preset: self.preset,
            actors_dir: self.actors_dir,
        }))
    }
}

use testbed_content::explorer::host::wait_for_handover_until;
/// The handover wait, shared with `npcctl serve --wait-port`.
pub use testbed_content::explorer::host::{Handover, running_elsewhere};

/// `true` once `within` has passed (the app reads the clock; the shared
/// host code takes it as this function).
fn after(within: Duration) -> impl FnMut() -> bool {
    let deadline = Instant::now() + within;
    move || Instant::now() >= deadline
}

/// `--exact-port`: whether `port` can be bound on 127.0.0.1 within
/// `within`, trying every 100 ms.
#[cfg(test)]
pub fn wait_for_port(port: u16, within: Duration) -> bool {
    testbed_content::explorer::host::wait_for_port_until(port, after(within))
}

/// `--exact-port N`: the port, then the detached host's exit, within one
/// budget of `within`.
pub fn wait_for_handover(dir: &AgentDir, port: u16, within: Duration) -> Handover {
    wait_for_handover_until(dir, port, after(within))
}
/// The explorer's port probe, shared with the detached host. Its probe
/// listeners are dropped before BRP binds, so another process can still
/// take a port in between ([`remote_ready`] catches that).
pub use testbed_content::explorer::host::{PORT_SPAN, pick_port};
/// Seconds between autosaves.
const AUTOSAVE_SECS: u64 = 60;
/// How long `--exact-port N` waits for a detached world to release `N`.
pub const EXACT_PORT_WAIT: Duration = testbed_content::explorer::host::HANDOVER_WAIT;
/// The exit code when `--exact-port N` stays taken.
pub const EXIT_PORT_IN_USE: i32 = 4;
/// How long the "moved here from detached" notice shows.
pub const MOVED_NOTICE_FOR: Duration = Duration::from_secs(5);

/// The message of a `--exact-port N` that stays taken.
pub fn port_in_use(port: u16) -> String {
    format!("port {port} is still in use")
}

/// The agent's folder (`<home>/<slug(agent)>`), whether or not it exists.
pub fn agent_dir(ex: &ExplorerArgs) -> AgentDir {
    AgentDir {
        dir: ex.home.join(slug(&ex.agent)),
    }
}

/// The refusal for an agent whose world runs elsewhere.
pub fn already_running(agent: &str, run: &RunFile) -> String {
    let mode = match run.mode {
        RunMode::Window => "window",
        RunMode::Detached => "detached",
    };
    format!(
        "agent {agent} is already running (pid {}, mode {mode}, port {})",
        run.pid, run.port
    )
}

/// `Err` when the app installed BRP but its server is not listening: the
/// picked port was taken between [`pick_port`] and the bind, so the prompt
/// would advertise a dead URL.
///
/// # Errors
/// A message naming the busy port.
pub fn remote_ready(app: &App) -> Result<(), String> {
    #[cfg(feature = "debug-tools")]
    if let Some(status) = app
        .world()
        .get_resource::<npc_framework::tools::remote::RemoteStatus>()
        && !status.listening
    {
        return Err(format!(
            "{BUSY_REMOTE} {} is busy (another program took it after it was picked): \
             try again",
            status.port
        ));
    }
    #[cfg(not(feature = "debug-tools"))]
    let _ = app;
    Ok(())
}

/// How [`remote_ready`]'s error starts.
const BUSY_REMOTE: &str = "remote port";

/// True for [`remote_ready`]'s error.
pub fn is_busy_remote(err: &str) -> bool {
    err.starts_with(BUSY_REMOTE) && err.contains(" is busy ")
}

/// `npcctl` next to the running exe (`npcctl.exe` on Windows), and whether
/// it is missing (not built yet).
pub fn npcctl_path() -> (PathBuf, bool) {
    let name = if cfg!(windows) {
        "npcctl.exe"
    } else {
        "npcctl"
    };
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let path = dir.join(name);
    let missing = !path.exists();
    (path, missing)
}

/// What the in-window prompt modal shows ([`ExplorerPrompt::modal`]).
#[derive(Resource, Clone, Debug)]
pub struct ExplorerPrompt {
    /// Which opening the prompt has (a title can be derived from it).
    pub kind: PromptKind,
    /// The rendered prompt, as written to `prompts/<kind>.md`.
    pub text: String,
    /// That file.
    pub path: PathBuf,
    /// The BRP port picked for this run.
    pub port: u16,
    /// `http://127.0.0.1:<port>`.
    pub url: String,
    /// `npcctl` is not built next to the game (the prompt says so).
    pub npcctl_missing: bool,
    /// Lenient load: names in the save this build does not know.
    pub skipped: Vec<String>,
    /// Load: resources the save lacked (they kept their built value).
    pub defaulted: Vec<String>,
    /// Why the agent folder's workspace could not be written; `None` when
    /// it was (then `path` holds `text`).
    pub write_error: Option<String>,
}

impl ExplorerPrompt {
    /// One line on what a load skipped or defaulted; `None` when nothing.
    pub fn load_summary(&self) -> Option<String> {
        if self.skipped.is_empty() && self.defaulted.is_empty() {
            return None;
        }
        Some(format!(
            "{} skipped, {} defaulted",
            self.skipped.len(),
            self.defaulted.len()
        ))
    }

    /// The modal's title (the default font has ASCII only: no dashes).
    pub fn title(&self) -> &'static str {
        match self.kind {
            PromptKind::Start => "Your agent's start prompt",
            PromptKind::Resume => "Resume prompt",
            PromptKind::ResumeChanged => "Resume prompt - your world may have changed",
            PromptKind::ResumeRebuilt => "Resume prompt - your world was rebuilt",
        }
    }

    /// The notes under the prompt: the remote URL and port, a missing
    /// `npcctl`, what a load skipped or defaulted, and the prompt file (or
    /// why the agent folder could not be written).
    pub fn notes(&self) -> Vec<String> {
        let mut notes = vec![format!(
            "The game serves the agent at {} (port {}).",
            self.url, self.port
        )];
        if self.npcctl_missing {
            notes.push(
                "npcctl is not built next to the game yet: run `cargo build -p npcctl` before the agent starts."
                    .into(),
            );
        }
        if let Some(summary) = self.load_summary() {
            let lists: String = [("skipped", &self.skipped), ("defaulted", &self.defaulted)]
                .into_iter()
                .filter(|(_, names)| !names.is_empty())
                .map(|(what, names)| format!("; {what}: {}", names.join(", ")))
                .collect();
            notes.push(format!("The save loaded with {summary}{lists}."));
        }
        match &self.write_error {
            None => notes.push(format!("The prompt is also in {}", self.path.display())),
            Some(e) => notes.push(format!(
                "The agent folder was not prepared ({e}): copy the prompt from here; AGENTS.md \
                 and .mcp.json there may be missing or name an old port."
            )),
        }
        notes
    }

    /// The open modal for this prompt.
    pub fn modal(&self) -> PromptModal {
        PromptModal {
            title: self.title().to_owned(),
            text: self.text.clone(),
            notes: self.notes(),
            open: true,
            // Only a written file holds the text.
            file: self.write_error.is_none().then(|| self.path.clone()),
        }
    }
}

/// Shows the prompt modal for the stored [`ExplorerPrompt`] (call after
/// [`prepare_session`]); OK unpauses the sim ([`unpause_on_close`]).
pub fn show_prompt_modal(app: &mut App) {
    let Some(modal) = app
        .world()
        .get_resource::<ExplorerPrompt>()
        .map(ExplorerPrompt::modal)
    else {
        return;
    };
    app.add_plugins(ExplorerModalPlugin)
        .insert_resource(modal)
        .add_systems(Update, unpause_on_close.after(ExplorerModalSystems));
}

/// The world waits behind the prompt modal: OK unpauses the sim (app code;
/// the view never writes `SimControl`).
pub fn unpause_on_close(
    mut closed: MessageReader<PromptModalClosed>,
    control: Option<ResMut<SimControl>>,
) {
    if closed.read().count() == 0 {
        return;
    }
    if let Some(mut control) = control {
        control.paused = false;
    }
}

/// Autosave settings; [`autosave`] writes the session's save every `every`
/// of wall-clock time and [`save_on_exit`] once more on `AppExit`.
#[derive(Resource, Clone, Debug)]
pub struct Autosave {
    /// The agent folder ([`save_now`] writes its save).
    pub dir: AgentDir,
    /// The save file.
    pub path: PathBuf,
    /// Wall-clock interval.
    pub every: Duration,
    /// When the last save was taken.
    pub last: Instant,
}

impl Autosave {
    /// Every 60 s into the session's save file.
    pub fn new(session: &ExplorerSession) -> Self {
        Self {
            dir: session.dir.clone(),
            path: session.save_path(),
            every: Duration::from_secs(AUTOSAVE_SECS),
            last: Instant::now(),
        }
    }
}

/// Saves now (header `saved` is now) and restarts the interval.
///
/// # Errors
/// As [`save_now`].
pub fn autosave_now(world: &mut World) -> Result<(), String> {
    let Some(mut auto) = world.get_resource_mut::<Autosave>() else {
        return Err("no autosave configured".into());
    };
    auto.last = Instant::now();
    let dir = auto.dir.clone();
    save_now(world, &dir, &now_rfc3339())
}

/// `Update`, after the driver set (never mid-tick): saves every
/// [`Autosave::every`]. A failed save is a warning.
pub fn autosave(world: &mut World) {
    let due = world
        .get_resource::<Autosave>()
        .is_some_and(|a| a.last.elapsed() >= a.every);
    if due {
        run_save(world, "autosave");
    }
}

/// `Last`, in `OnAppExitSystems`: a final save when the app is exiting
/// (also when the last window closed in this frame).
pub fn save_on_exit(world: &mut World) {
    let exiting = world
        .get_resource::<Messages<AppExit>>()
        .is_some_and(|m| !m.is_empty());
    if exiting && world.get_resource::<Autosave>().is_some() {
        run_save(world, "save on exit");
        world.remove_resource::<Autosave>();
    }
    if exiting
        && let Some(run) = world.remove_resource::<Running>()
        && let Err(e) = run.file.remove(&run.dir)
    {
        warn!(
            "actor_testbed: cannot remove {}: {e}",
            RunFile::path(&run.dir).display()
        );
    }
}

/// This window's `run.json` ([`RunFile`], mode window), removed on exit.
#[derive(Resource, Clone, Debug)]
pub struct Running {
    /// The agent folder.
    pub dir: AgentDir,
    /// What was written.
    pub file: RunFile,
}

/// `--exact-port`: the world moved here from a detached host, so no prompt
/// modal (the agent is already playing): the sim runs at once and a notice
/// says where the world came from for [`MOVED_NOTICE_FOR`]. Call after
/// [`prepare_session`].
pub fn show_moved_notice(app: &mut App) {
    if let Some(mut control) = app.world_mut().get_resource_mut::<SimControl>() {
        control.paused = false;
    }
    let text = app.world().get_resource::<ExplorerPrompt>().map_or_else(
        || "moved here from detached".to_owned(),
        |p| format!("moved here from detached (port {})", p.port),
    );
    app.insert_resource(MovedNoticeText(text))
        .add_systems(Startup, spawn_moved_notice)
        .add_systems(Update, expire_moved_notice);
}

/// The notice's text.
#[derive(Resource, Clone, Debug)]
struct MovedNoticeText(String);

/// The "moved here from detached" notice and how long it still shows.
#[derive(Component, Clone, Debug)]
pub struct MovedNotice {
    /// Time left (real time, view only).
    pub left: Duration,
}

fn spawn_moved_notice(mut commands: Commands, text: Res<MovedNoticeText>) {
    commands.spawn((
        Name::new("moved notice"),
        MovedNotice {
            left: MOVED_NOTICE_FOR,
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Percent(35.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.85)),
        Text::new(text.0.clone()),
        TextFont::from_font_size(18.0),
        TextColor(Color::WHITE),
    ));
}

/// Counts the notice down on real time (view only) and removes it.
fn expire_moved_notice(
    mut commands: Commands,
    time: Option<Res<Time<Real>>>,
    mut notices: Query<(Entity, &mut MovedNotice)>,
) {
    let delta = time.map_or(Duration::ZERO, |t| t.delta());
    for (entity, mut notice) in &mut notices {
        notice.left = notice.left.saturating_sub(delta);
        if notice.left.is_zero() {
            commands.entity(entity).despawn();
        }
    }
}

fn run_save(world: &mut World, what: &str) {
    match autosave_now(world) {
        Ok(()) => {
            let tick = world.resource::<npc_framework::prelude::SimClock>().tick;
            let path = world.resource::<Autosave>().path.display().to_string();
            info!("actor_testbed: {what}: tick {tick} -> {path}");
        }
        Err(e) => warn!("actor_testbed: {what} failed: {e}"),
    }
}

/// After the app is built: writes the agent folder's workspace for `port`
/// ([`write_session_files`]: `AGENTS.md`, `CLAUDE.md`, `prompts/<kind>.md`,
/// the Claude Code settings, `.mcp.json`; on every start and resume, never
/// the notes or the save), stores [`ExplorerPrompt`], inserts the notes
/// resources ([`insert_notes`]) and sets up the autosave systems. A
/// workspace that cannot be written is logged and named in the modal
/// ([`ExplorerPrompt::write_error`]); the world still starts.
///
/// # Errors
/// A message when the agent's unit is missing.
pub fn prepare_session(
    app: &mut App,
    start: &Start,
    dir: &AgentDir,
    port: u16,
) -> Result<(), String> {
    let (npcctl, npcctl_missing) = npcctl_path();
    let url = format!("http://127.0.0.1:{port}");
    // A folder that cannot be written does not stop the world: the modal
    // still shows the prompt and says what went wrong.
    let files = write_session_files(app.world(), start, dir, &url, npcctl, npcctl_missing)?;
    if let Some(e) = &files.write_error {
        warn!("actor_testbed: {e}");
    }
    let session = app.world().resource::<ExplorerSession>().clone();
    let (skipped, defaulted) = match start {
        Start::Resumed(r) | Start::ResumedChanged(r) => (r.skipped.clone(), r.defaulted.clone()),
        _ => (Vec::new(), Vec::new()),
    };
    // The F7 editor and the notes methods share the one `NotesStores`.
    insert_notes(app.world_mut(), files.unit, dir, now_rfc3339);
    // Findable while it runs (spec §8); a failure is only a warning.
    let run = RunFile {
        pid: std::process::id(),
        port,
        mode: RunMode::Window,
        started: now_rfc3339(),
        agent: session.meta.agent.clone(),
        world: session.meta.world.clone(),
    };
    if let Err(e) = run.write(dir) {
        warn!(
            "actor_testbed: cannot write {}: {e}",
            RunFile::path(dir).display()
        );
    }
    app.insert_resource(Running {
        dir: dir.clone(),
        file: run,
    });
    app.insert_resource(ExplorerPrompt {
        kind: files.kind,
        text: files.text,
        path: files.path,
        port,
        url,
        npcctl_missing,
        skipped,
        defaulted,
        write_error: files.write_error,
    })
    .insert_resource(Autosave::new(&session))
    .add_systems(Update, autosave.after(NpcDriverSet))
    // `OnAppExitSystems` runs after `ExitSystems` (`exit_on_all_closed`
    // writes `AppExit` in `Last`), so closing the window saves too.
    .add_systems(Last, save_on_exit.in_set(bevy::app::OnAppExitSystems));
    Ok(())
}

/// Starts the detached host (`npcctl serve`): the program and its
/// arguments, the new process id back. A test stands in for it.
#[derive(Resource, Clone)]
pub struct DetachSpawner(pub DetachFn);

/// [`DetachSpawner`]'s function.
pub type DetachFn = Arc<dyn Fn(&Path, &[String]) -> std::io::Result<u32> + Send + Sync>;

impl Default for DetachSpawner {
    /// `npcctl` in its own console and process group, no stdio: it
    /// outlives this window.
    fn default() -> Self {
        Self(Arc::new(|program, args| {
            let mut cmd = std::process::Command::new(program);
            cmd.args(args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            detach_command(&mut cmd);
            cmd.spawn().map(|c| c.id())
        }))
    }
}

/// Where the world moves when it goes detached.
#[derive(Resource, Clone, Debug)]
pub struct DetachTarget {
    /// The agents folder.
    pub home: PathBuf,
    /// The agent.
    pub agent: String,
    /// The window's port: the detached host takes it over.
    pub port: u16,
    /// `npcctl` next to the game ([`npcctl_path`]) and whether it is
    /// missing (not built: Yes and F9 then say so and keep the window).
    pub npcctl: (PathBuf, bool),
}

/// The "Keep it running detached?" dialog of a closing explorer window.
#[derive(Resource, Clone, Debug, Default)]
pub struct CloseDialog {
    /// Shown.
    pub open: bool,
    /// Why the last Yes could not start the detached host.
    pub error: Option<String>,
    /// The answer, applied after this frame's input.
    choice: Option<CloseChoice>,
    /// The spawned panel.
    root: Option<Entity>,
}

/// An answer to [`CloseDialog`] (or F9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseChoice {
    /// Save, start the detached host on the same port, exit.
    Detach,
    /// Save and exit.
    Exit,
    /// Back to the game.
    Cancel,
}

/// The dialog's systems (after the game's input, so the click or key that
/// answers it never reaches the world).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CloseDialogSystems;

/// Run condition: the close dialog is open.
pub fn close_dialog_open(dialog: Option<Res<CloseDialog>>) -> bool {
    dialog.is_some_and(|d| d.open)
}

#[derive(Component)]
struct CloseDialogRoot;

#[derive(Component, Clone, Copy)]
struct CloseButton(CloseChoice);

#[derive(Component)]
struct CloseErrorText;

/// The detached host's arguments for `target`.
pub fn detach_args(target: &DetachTarget) -> Vec<String> {
    detached_resume_args(&target.home, &target.agent, target.port)
}

/// Explorer windows (spec §8, G7): closing the window asks "Keep it running
/// detached?" (the window plugin must not close windows on request: the
/// app sets `close_when_requested: false`). **Yes** saves, starts `npcctl
/// serve --resume --port N --wait-port` for this agent on this window's
/// port (it waits for the window to exit), prints the
/// [`testbed_content::explorer::host::DetachedLine`] for the menu and
/// exits; **No** exits (the exit saves); **Cancel** (Escape) goes back. F9
/// moves the world without asking. While the dialog is open the game's
/// input ([`ViewSystems::Input`]) does not run and no text field has the
/// focus, so typing never reaches a game key; Y, N and Escape answer it.
pub fn install_close_to_detached(app: &mut App, home: &Path, agent: &str, port: u16) {
    if !app.is_plugin_added::<bevy::ui_widgets::ButtonPlugin>() {
        app.add_plugins(bevy::ui_widgets::ButtonPlugin);
    }
    app.init_resource::<DetachSpawner>()
        .init_resource::<CloseDialog>()
        .insert_resource(DetachTarget {
            home: home.to_path_buf(),
            agent: agent.to_owned(),
            port,
            npcctl: npcctl_path(),
        })
        .configure_sets(Update, ViewSystems::Input.run_if(not(close_dialog_open)))
        .configure_sets(Update, CloseDialogSystems.after(ViewSystems::Input))
        // The prompt modal behind the dialog is frozen too.
        .configure_sets(
            Update,
            testbed_view::ExplorerModalSystems.run_if(not(close_dialog_open)),
        )
        .add_systems(Update, f9_detaches.in_set(ViewSystems::Input))
        .add_systems(
            Update,
            (
                open_on_close_request,
                dialog_keys,
                apply_close_choice,
                sync_close_dialog,
            )
                .chain()
                .in_set(CloseDialogSystems),
        );
}

/// Opens the close dialog as a close request would (`--close-dialog`).
pub fn open_close_dialog(app: &mut App) {
    if let Some(mut dialog) = app.world_mut().get_resource_mut::<CloseDialog>() {
        dialog.open = true;
    }
}

/// F9 (the app's key): the world moves to detached on purpose, no dialog.
fn f9_detaches(keys: Option<Res<ButtonInput<KeyCode>>>, mut dialog: ResMut<CloseDialog>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::F9)) {
        dialog.choice = Some(CloseChoice::Detach);
    }
}

/// A close request (the window's X, Alt+F4) opens the dialog instead of
/// closing; the focused text field (F7) loses the focus.
fn open_on_close_request(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    mut dialog: ResMut<CloseDialog>,
    focus: Option<ResMut<bevy::input_focus::InputFocus>>,
) {
    if requests.read().count() == 0 || dialog.open {
        return;
    }
    dialog.open = true;
    dialog.error = None;
    if let Some(mut focus) = focus {
        focus.clear();
    }
}

/// Y, N and Escape answer the open dialog.
fn dialog_keys(keys: Option<Res<ButtonInput<KeyCode>>>, mut dialog: ResMut<CloseDialog>) {
    let Some(keys) = keys else { return };
    if !dialog.open {
        return;
    }
    let choice = if keys.just_pressed(KeyCode::KeyY) {
        CloseChoice::Detach
    } else if keys.just_pressed(KeyCode::KeyN) {
        CloseChoice::Exit
    } else if keys.just_pressed(KeyCode::Escape) {
        CloseChoice::Cancel
    } else {
        return;
    };
    dialog.choice = Some(choice);
}

/// Carries out the answer: Yes (or F9) pauses the sim, saves, starts the
/// detached host and exits; a save or a start that fails keeps the window
/// and shows the reason. No exits ([`save_on_exit`] saves).
fn apply_close_choice(world: &mut World) {
    let Some(choice) = world.resource_mut::<CloseDialog>().choice.take() else {
        return;
    };
    match choice {
        CloseChoice::Cancel => world.resource_mut::<CloseDialog>().open = false,
        CloseChoice::Exit => {
            world.write_message(AppExit::Success);
        }
        CloseChoice::Detach => match move_to_detached(world) {
            Ok(()) => {
                world.write_message(AppExit::Success);
            }
            Err(e) => {
                warn!("actor_testbed: {e}");
                let mut dialog = world.resource_mut::<CloseDialog>();
                dialog.open = true;
                dialog.error = Some(e);
            }
        },
    }
}

/// Saves (the sim paused, so the exit's save is the same tick) and starts
/// the detached host; prints its [`testbed_content::explorer::host::DetachedLine`].
fn move_to_detached(world: &mut World) -> Result<(), String> {
    let target = world
        .get_resource::<DetachTarget>()
        .cloned()
        .ok_or("this window has no explorer world")?;
    let (npcctl, missing) = target.npcctl.clone();
    if missing {
        return Err(format!(
            "{} is not built (cargo build -p npcctl): the world stays in the window",
            npcctl.display()
        ));
    }
    // Paused, so the exit's save is this tick; a failure puts the earlier
    // state back.
    let was_paused = world.get_resource::<SimControl>().map(|c| c.paused);
    let restore = |world: &mut World| {
        if let (Some(paused), Some(mut control)) =
            (was_paused, world.get_resource_mut::<SimControl>())
        {
            control.paused = paused;
        }
    };
    if let Some(mut control) = world.get_resource_mut::<SimControl>() {
        control.paused = true;
    }
    if world.get_resource::<Autosave>().is_some()
        && let Err(e) = autosave_now(world)
    {
        restore(world);
        return Err(format!(
            "the save failed: {e}; the world stays in the window"
        ));
    }
    let args = detach_args(&target);
    let spawn = world.resource::<DetachSpawner>().0.clone();
    let pid = match spawn(&npcctl, &args) {
        Ok(pid) => pid,
        Err(e) => {
            restore(world);
            return Err(format!(
                "cannot start {}: {e}; the world stays in the window",
                npcctl.display()
            ));
        }
    };
    let world_name = world
        .get_resource::<ExplorerSession>()
        .map(|s| s.meta.world.clone())
        .unwrap_or_default();
    let line = testbed_content::explorer::host::DetachedLine {
        pid,
        port: target.port,
        agent: target.agent.clone(),
        world: world_name,
    };
    info!(
        "actor_testbed: the world keeps running detached: {} {} (pid {pid})",
        npcctl.display(),
        args.join(" ")
    );
    // A closed stdout (no menu reading it) must not stop the exit's save.
    let _ = writeln!(std::io::stdout().lock(), "{}", line.line());
    Ok(())
}

/// Spawns or despawns the dialog's panel and shows the last error.
fn sync_close_dialog(
    mut commands: Commands,
    mut dialog: ResMut<CloseDialog>,
    target: Res<DetachTarget>,
    mut errors: Query<&mut Text, With<CloseErrorText>>,
) {
    if !dialog.open {
        if let Some(root) = dialog.root.take() {
            commands.entity(root).despawn();
        }
        return;
    }
    if dialog.root.is_none() {
        dialog.root = Some(spawn_close_dialog(&mut commands, &target));
    }
    let want = dialog.error.clone().unwrap_or_default();
    for mut t in &mut errors {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

fn spawn_close_dialog(commands: &mut Commands, target: &DetachTarget) -> Entity {
    let font = |px: f32| TextFont::from_font_size(px);
    commands
        .spawn((
            Name::new("close dialog"),
            CloseDialogRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            GlobalZIndex(2000),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    width: Val::Percent(90.0),
                    max_width: Val::Px(620.0),
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(10.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.09, 0.11)),
                BorderColor::all(Color::srgb(0.35, 0.40, 0.48)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Keep it running detached?"),
                    font(20.0),
                    TextColor(Color::srgb(0.97, 0.95, 0.85)),
                ));
                panel.spawn((
                    Text::new(format!(
                        "Yes: {}'s world keeps running without a window (npcctl serve on port \
                         {}, the same URL), so the agent can keep playing; the game menu can \
                         open it in the window again. No: the world is saved and stops.",
                        target.agent, target.port
                    )),
                    font(14.0),
                    TextColor(Color::srgb(0.88, 0.90, 0.88)),
                ));
                panel.spawn((
                    Text::new("Y: Yes   N: No   Esc: back to the game   (F9 moves it at any time)"),
                    font(12.0),
                    TextColor(Color::srgb(0.62, 0.68, 0.74)),
                ));
                panel.spawn((
                    CloseErrorText,
                    Text::new(""),
                    font(13.0),
                    TextColor(Color::srgb(0.98, 0.55, 0.50)),
                ));
                panel
                    .spawn(Node {
                        justify_content: JustifyContent::FlexEnd,
                        column_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for (label, choice) in [
                            ("Yes", CloseChoice::Detach),
                            ("No", CloseChoice::Exit),
                            ("Cancel", CloseChoice::Cancel),
                        ] {
                            row.spawn((
                                Name::new(format!("close dialog {label}")),
                                CloseButton(choice),
                                bevy::ui_widgets::Button,
                                bevy::picking::hover::Hovered::default(),
                                Node {
                                    padding: UiRect::axes(Val::Px(18.0), Val::Px(6.0)),
                                    min_width: Val::Px(84.0),
                                    justify_content: JustifyContent::Center,
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.20, 0.24, 0.30)),
                            ))
                            .with_child((Text::new(label), font(14.0)))
                            .observe(on_close_button);
                        }
                    });
            });
        })
        .id()
}

/// A dialog button: its answer, applied after this frame's input.
fn on_close_button(
    ev: On<bevy::ui_widgets::Activate>,
    buttons: Query<&CloseButton>,
    mut dialog: ResMut<CloseDialog>,
) {
    if let Ok(b) = buttons.get(ev.entity) {
        dialog.choice = Some(b.0);
    }
}

/// `--record` and `--commands` cannot start mid-world: a replay is
/// scenario + seed + commands from tick 0.
///
/// # Errors
/// A usage message.
pub fn refuse_recording_on_resume(
    ex: &ExplorerArgs,
    record: bool,
    commands: bool,
) -> Result<(), String> {
    if ex.resume && (record || commands) {
        return Err(
            "--record and --commands cannot be used with --resume: a recording \
                    started mid-world cannot be replayed from scenario + seed"
                .into(),
        );
    }
    Ok(())
}

/// A world seed from the wall clock (app code: the sim only sees the number).
fn wall_clock_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let low = u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or_default();
    low ^ u64::from(std::process::id()).rotate_left(32)
}

/// [`rfc3339`] of now.
pub fn now_rfc3339() -> String {
    rfc3339(SystemTime::now())
}

/// Builds the explorer app (`host::build_world`) with a wall-clock seed and
/// date, refusing a remote port taken after it was picked
/// ([`remote_ready`]). `host` adds the window, driver and view (tests pass
/// `|_| {}`). The sim starts paused.
///
/// # Errors
/// A message; `host::needs_choice` is true for a save from another build
/// without [`ExplorerArgs::on_mismatch`].
pub fn build_explorer_app(
    args: &ExplorerArgs,
    host: impl FnOnce(&mut App),
) -> Result<(App, Start, AgentDir), String> {
    build_world_with(
        &args.host_args(),
        wall_clock_seed(),
        &now_rfc3339(),
        host,
        remote_ready,
    )
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;
    use npc_framework::prelude::{SimId, run_ticks, state_hash};
    use npc_framework::snapshot;
    use serde_json::Value;
    use testbed_content::TestbedIds;
    use testbed_content::explorer::workspace::GENERATED;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("npc-explorer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn new_args(home: &Path, agent: &str, world: &str) -> ExplorerArgs {
        ExplorerArgs {
            home: home.to_path_buf(),
            agent: agent.into(),
            world: Some(world.into()),
            resume: false,
            on_mismatch: None,
            check_only: false,
            exact_port: None,
            show_close_dialog: false,
            preset: None,
            actors_dir: Some(home.join("actors")),
        }
    }

    fn resume_args(home: &Path, agent: &str, on_mismatch: Option<Mismatch>) -> ExplorerArgs {
        ExplorerArgs {
            world: None,
            resume: true,
            on_mismatch,
            ..new_args(home, agent, "")
        }
    }

    fn actor(app: &App, agent: &str) -> SimId {
        app.world()
            .resource::<TestbedIds>()
            .get(agent)
            .unwrap_or_else(|| panic!("no actor named {agent}"))
    }

    /// Rewrites the save at `path` through `edit` (as JSON).
    fn edit_save(path: &Path, edit: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        edit(&mut v);
        std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    }

    #[test]
    fn explorer_flags_validate() {
        let parse = |flags: &[&str], scenario: bool| {
            let mut f = ExplorerFlags::default();
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                if !f.switch(flag) {
                    assert!(f.value(flag, it.next().unwrap()), "{flag}");
                }
            }
            f.finish(scenario, false)
        };
        assert_eq!(parse(&[], false), Ok(None));
        let a = parse(
            &["--explorer-home", "h", "--agent", "A", "--world", "W"],
            false,
        )
        .unwrap()
        .unwrap();
        assert!(!a.resume && a.world.as_deref() == Some("W"));
        let r = parse(
            &[
                "--explorer-home",
                "h",
                "--agent",
                "A",
                "--resume",
                "--fresh-world",
            ],
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.on_mismatch, Some(Mismatch::FreshWorld));
        let c = parse(
            &["--explorer-home", "h", "--agent", "A", "--check-save"],
            false,
        )
        .unwrap()
        .unwrap();
        assert!(c.check_only);
        // Task 11: the actor preset and its folder reach the host.
        let p = parse(
            &[
                "--explorer-home",
                "h",
                "--agent",
                "A",
                "--world",
                "W",
                "--preset",
                "rover",
                "--actors-dir",
                "acts",
            ],
            false,
        )
        .unwrap()
        .unwrap();
        let host = p.host_args();
        assert_eq!(host.preset.as_deref(), Some("rover"));
        assert_eq!(host.actors_dir, Some(PathBuf::from("acts")));
        assert_eq!(a.host_args().preset, None, "the default: human");
        for (flags, scenario, needle) in [
            (
                &["--preset", "rover"][..],
                false,
                "--actors-dir and --exact-port need --explorer-home",
            ),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--world",
                    "W",
                    "--preset",
                    " ",
                ][..],
                false,
                "--preset: the name is blank",
            ),
            (
                &["--explorer-home", "h", "--agent", "A", "--world", "W"][..],
                true,
                "--scenario",
            ),
            (
                &["--explorer-home", "h", "--world", "W"][..],
                false,
                "--agent",
            ),
            (
                &["--explorer-home", "h", "--agent", "A"][..],
                false,
                "--world",
            ),
            (
                &["--agent", "A", "--world", "W"][..],
                false,
                "--explorer-home",
            ),
            (
                &["--explorer-home", "h", "--agent", "A", "--load-anyway"][..],
                false,
                "--resume",
            ),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--resume",
                    "--load-anyway",
                    "--fresh-world",
                ][..],
                false,
                "--fresh-world",
            ),
            (
                &["--explorer-home", "h", "--agent", " ", "--world", "W"][..],
                false,
                "--agent",
            ),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--resume",
                    "--world",
                    "W",
                ][..],
                false,
                "the world comes from the agent's folder",
            ),
        ] {
            let e = parse(flags, scenario).unwrap_err();
            assert!(e.contains(needle), "{flags:?}: {e}");
        }
    }

    /// The port tests bind real ports: one at a time, so they cannot take
    /// each other's.
    static PORTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn hold(port: u16) -> TcpListener {
        TcpListener::bind(("127.0.0.1", port)).unwrap()
    }

    #[test]
    fn pick_port_skips_taken() {
        let _one = PORTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let last = 15702 + PORT_SPAN;
        let a = pick_port(15702, last).expect("a free port");
        let _held = hold(a);
        let b = pick_port(15702, last).expect("another free port");
        // Every port below `a` was taken, and `a` is now: the next is higher.
        assert!(b > a, "{a} then {b}");
        assert_eq!(pick_port(a, a), None);
    }

    #[test]
    fn pick_port_needs_the_render_port_free_too() {
        let _one = PORTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let a = pick_port(15702, 15702 + PORT_SPAN).expect("a free pair");
        let _render = hold(a + 1);
        assert_eq!(pick_port(a, a), None, "{a} is free but {} is not", a + 1);
        let b = pick_port(a, a + PORT_SPAN).expect("a free pair further on");
        assert!(b >= a + 2, "{a} then {b}");
        // The last port has no `+ 1`.
        assert_eq!(pick_port(u16::MAX, u16::MAX), None);
    }

    /// A host whose BRP did not get its port (as `NpcRemotePlugin` records
    /// a port taken after `pick_port`).
    #[cfg(feature = "debug-tools")]
    fn busy_remote(app: &mut App) {
        app.insert_resource(npc_framework::tools::remote::RemoteStatus {
            port: 15709,
            listening: false,
            render_port: None,
            errors: vec!["BRP port 15709 is already in use".into()],
        });
    }

    #[cfg(feature = "debug-tools")]
    #[test]
    fn a_busy_remote_port_stops_the_start_and_leaves_nothing() {
        let home = tmp("busy");
        let Err(e) = build_explorer_app(&new_args(&home, "Wren", "W"), busy_remote) else {
            panic!("started with a dead remote URL");
        };
        assert!(e.contains("15709") && e.contains("busy") && e.contains("try again"));
        assert!(!home.join("wren").exists(), "no folder left behind");
        // Resume says the same (not "unreadable save").
        let _ = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        let Err(e) = build_explorer_app(&resume_args(&home, "Wren", None), busy_remote) else {
            panic!("resumed with a dead remote URL");
        };
        assert!(e.contains("busy") && !e.contains("unreadable"), "{e}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Every file under `dir` (relative path, bytes), sorted.
    fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(dir).unwrap().to_string_lossy();
                    out.push((rel.replace('\\', "/"), std::fs::read(&p).unwrap()));
                }
            }
        }
        out.sort();
        out
    }

    /// The owned files of the folder: everything outside [`GENERATED`],
    /// but `run.json` (each run writes its own).
    fn owned(dir: &AgentDir) -> Vec<(String, Vec<u8>)> {
        files(&dir.dir)
            .into_iter()
            .filter(|(rel, _)| {
                rel != testbed_content::explorer::host::RUN_FILE
                    && !GENERATED
                        .iter()
                        .any(|g| rel == g || rel.starts_with(&format!("{g}/")))
            })
            .collect()
    }

    fn read(path: impl AsRef<Path>) -> String {
        let path = path.as_ref();
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    #[test]
    fn new_writes_the_workspace() {
        let home = tmp("ws-new");
        let (mut app, start, dir) =
            build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        for g in GENERATED {
            assert!(dir.dir.join(g).exists(), "{g} missing");
        }
        let p = app.world().resource::<ExplorerPrompt>();
        assert_eq!(p.path, dir.prompt(PromptKind::Start));
        // The file has a frontmatter, the copied text none.
        let file = read(&p.path);
        assert!(file.starts_with("---\ntype: prompt\n"), "{file}");
        assert!(file.ends_with(&p.text) && !p.text.starts_with("---"));
        assert_eq!(p.modal().file, Some(dir.prompt(PromptKind::Start)));
        assert!(read(dir.dir.join("AGENTS.md")).contains("http://127.0.0.1:15703"));
        assert!(read(dir.dir.join(".mcp.json")).contains("http://127.0.0.1:15703"));
        assert_eq!(read(dir.dir.join("CLAUDE.md")), "@AGENTS.md\n");
        assert!(!dir.dir.join("start_prompt.md").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn resume_rewrites_generated_files_only() {
        let home = tmp("ws-resume");
        let (mut app, start, dir) =
            build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        drop(app);
        // Hand edits in generated files, and owned files with content.
        std::fs::write(dir.dir.join("AGENTS.md"), "hand edit").unwrap();
        std::fs::write(dir.dir.join(".mcp.json"), "{}").unwrap();
        std::fs::write(
            dir.diary().join("Cold.md"),
            "---\nid: 1\ntitle: Cold\n---\nBrr.\n",
        )
        .unwrap();
        std::fs::write(dir.dir.join("memory.md"), "# Memory\n").unwrap();
        std::fs::write(dir.proposals().join("001-doors.md"), "# Doors\n").unwrap();
        let before = owned(&dir);
        assert!(
            before.iter().any(|(rel, _)| rel == "w.world.json"),
            "{before:?}"
        );

        let (mut app, start, dir) =
            build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15710).unwrap();
        assert_eq!(owned(&dir), before, "owned files untouched");
        let agents = read(dir.dir.join("AGENTS.md"));
        assert!(!agents.contains("hand edit") && agents.contains("http://127.0.0.1:15710"));
        let mcp = read(dir.dir.join(".mcp.json"));
        assert!(
            mcp.contains("http://127.0.0.1:15710") && !mcp.contains("15703"),
            "{mcp}"
        );
        let p = app.world().resource::<ExplorerPrompt>();
        assert_eq!(p.path, dir.prompt(PromptKind::Resume));
        // The file has a frontmatter, the copied text none.
        let file = read(&p.path);
        assert!(file.starts_with("---\ntype: prompt\n"), "{file}");
        assert!(file.ends_with(&p.text) && !p.text.starts_with("---"));
        // The start prompt of the New stays (it names no port).
        assert!(dir.prompt(PromptKind::Start).is_file());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_old_folder_gets_its_workspace_on_resume() {
        let home = tmp("ws-old");
        // A folder as an older build left it: owned files, the old prompt
        // files, no workspace.
        let (_, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        std::fs::write(dir.dir.join("start_prompt.md"), "old start").unwrap();
        std::fs::write(dir.dir.join("resume_prompt.md"), "old resume").unwrap();
        for g in GENERATED {
            assert!(!dir.dir.join(g).exists(), "{g} already there");
        }
        let before: Vec<_> = owned(&dir)
            .into_iter()
            .filter(|(rel, _)| !rel.ends_with("_prompt.md"))
            .collect();

        let (mut app, start, dir) =
            build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15704).unwrap();
        for g in GENERATED {
            assert!(dir.dir.join(g).exists(), "{g} missing after resume");
        }
        assert!(dir.prompt(PromptKind::Resume).is_file());
        assert!(!dir.dir.join("start_prompt.md").exists());
        assert!(!dir.dir.join("resume_prompt.md").exists());
        assert_eq!(owned(&dir), before);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_folder_that_cannot_be_written_still_starts_and_says_so() {
        let home = tmp("ws-fails");
        let (mut app, start, dir) =
            build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        // A file where the `prompts` folder goes: the workspace write fails.
        std::fs::write(dir.dir.join("prompts"), "in the way").unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        let p = app.world().resource::<ExplorerPrompt>();
        let e = p.write_error.clone().expect("the write failed");
        assert!(e.contains("cannot write the agent folder"), "{e}");
        // The modal still has the prompt, names the failure and no file.
        let modal = p.modal();
        assert!(modal.text.contains("AGENTS.md"), "{}", modal.text);
        assert_eq!(modal.file, None);
        assert!(
            modal.notes.iter().any(|n| n.contains(&e)),
            "{:?}",
            modal.notes
        );
        assert!(
            !modal
                .notes
                .iter()
                .any(|n| n.starts_with("The prompt is also in"))
        );
        // The session is set up as usual.
        assert!(app.world().get_resource::<Autosave>().is_some());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn npcctl_path_is_a_sibling_of_the_exe() {
        let (path, missing) = npcctl_path();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(path.parent(), exe.parent());
        assert_eq!(missing, !path.exists());
    }

    #[test]
    fn autosave_round_trips() {
        let home = tmp("autosave");
        let (mut app, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        let session = app.world().resource::<ExplorerSession>().clone();
        app.insert_resource(Autosave::new(&session));
        run_ticks(app.world_mut(), 200);
        let hash = state_hash(app.world_mut());
        autosave_now(app.world_mut()).unwrap();
        // Saving does not change the sim.
        assert_eq!(state_hash(app.world_mut()), hash);
        let snap = snapshot::read(&dir.save("W")).unwrap();
        assert_eq!(snap.tick, 200);
        assert_eq!(snap.header["agent"], "Wren");
        assert_eq!(snap.header["seed"], session.seed);
        assert_eq!(snapshot::check(app.world_mut(), &snap), Ok(()));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn prepare_session_writes_prompt_and_resource() {
        let home = tmp("prepare");
        let (mut app, start, dir) =
            build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        let p = app.world().resource::<ExplorerPrompt>();
        let unit = app.world().resource::<TestbedIds>().get("Wren").unwrap().0;
        assert_eq!(p.kind, PromptKind::Start);
        assert_eq!(p.port, 15703);
        assert_eq!(p.url, "http://127.0.0.1:15703");
        assert!(p.text.contains(&format!("actor #{unit}")), "{}", p.text);
        assert_eq!(p.path, dir.prompt(PromptKind::Start));
        let file = std::fs::read_to_string(&p.path).unwrap();
        assert!(file.starts_with("---\ntype: prompt\n") && file.ends_with(&p.text));
        assert!(app.world().get_resource::<Autosave>().is_some());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn explorer_maps_its_actor_to_the_agent_folder() {
        use npc_framework::agent_io::notes::{NotesClock, NotesDirs};
        let home = tmp("notes-dirs");
        let (mut app, start, dir) =
            build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        let unit = actor(&app, "Wren");
        let dirs = &app.world().resource::<NotesDirs>().0;
        assert_eq!(dirs.len(), 1, "{dirs:?}");
        assert_eq!(dirs.get(&unit), Some(&dir.dir));
        // The clock dates stamps with the wall clock, RFC 3339 UTC.
        let date = (app.world().resource::<NotesClock>().0)();
        assert_eq!(date.len(), "2026-10-24T13:05:09Z".len(), "{date}");
        assert!(date.ends_with('Z') && date.as_bytes()[10] == b'T', "{date}");
        // A player write reaches the agent folder's diary, dated.
        let added = npc_framework::agent_io::call(
            app.world_mut(),
            "notes/add",
            Some(serde_json::json!({
                "unit": unit.0, "book": "diary", "title": "First night", "body": "Cold."
            })),
        )
        .unwrap();
        assert_eq!(added["accepted"], true, "{added}");
        let file = dir.diary().join(format!("{} First night.md", &date[..10]));
        let diary = std::fs::read_to_string(&file).unwrap();
        assert!(diary.contains("\ntitle: First night\n") && diary.contains("Cold."));
        assert!(diary.contains(&format!("\ncreated: {date}\n")), "{diary}");
        assert!(diary.contains("\nagent: Wren\nworld: W\n"), "{diary}");
        let _ = std::fs::remove_dir_all(&home);
    }

    fn prompt(kind: PromptKind) -> ExplorerPrompt {
        ExplorerPrompt {
            kind,
            text: "the prompt".into(),
            path: PathBuf::from("C:/agents/wren/prompts/resume.md"),
            port: 15704,
            url: "http://127.0.0.1:15704".into(),
            npcctl_missing: false,
            skipped: Vec::new(),
            defaulted: Vec::new(),
            write_error: None,
        }
    }

    #[test]
    fn modal_titles_follow_kind_and_stay_ascii() {
        let title = |k| prompt(k).modal().title;
        assert_eq!(title(PromptKind::Start), "Your agent's start prompt");
        assert_eq!(title(PromptKind::Resume), "Resume prompt");
        assert!(title(PromptKind::ResumeChanged).contains("may have changed"));
        assert!(title(PromptKind::ResumeRebuilt).contains("rebuilt"));
        for k in [
            PromptKind::Start,
            PromptKind::Resume,
            PromptKind::ResumeChanged,
            PromptKind::ResumeRebuilt,
        ] {
            let m = prompt(k).modal();
            assert!(m.title.is_ascii() && m.notes.iter().all(|n| n.is_ascii()));
        }
    }

    #[test]
    fn modal_notes_name_port_tool_load_and_file() {
        let plain = prompt(PromptKind::Resume).modal();
        assert!(plain.open);
        assert_eq!(plain.text, "the prompt");
        assert_eq!(
            plain.file.as_deref(),
            Some(Path::new("C:/agents/wren/prompts/resume.md"))
        );
        assert_eq!(plain.notes.len(), 2, "{:?}", plain.notes);
        assert!(
            plain.notes[0].contains("http://127.0.0.1:15704") && plain.notes[0].contains("15704")
        );
        assert!(plain.notes[1].contains("C:/agents/wren/prompts/resume.md"));

        let mut p = prompt(PromptKind::ResumeChanged);
        p.npcctl_missing = true;
        p.skipped = vec!["content hash".into(), "Foo".into()];
        p.defaulted = vec!["Bar".into()];
        let notes = p.modal().notes;
        assert_eq!(notes.len(), 4, "{notes:?}");
        assert!(notes[1].contains("cargo build -p npcctl"), "{notes:?}");
        assert_eq!(
            notes[2],
            "The save loaded with 2 skipped, 1 defaulted; skipped: content hash, Foo; defaulted: Bar."
        );
    }

    #[test]
    fn closing_the_modal_unpauses() {
        let home = tmp("modal");
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins);
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        show_prompt_modal(&mut app);
        app.update();
        app.update();
        let modal = app.world().resource::<PromptModal>();
        assert!(modal.open);
        assert_eq!(modal.title, "Your agent's start prompt");
        // The copied text is the prompt file without its frontmatter.
        let file = std::fs::read_to_string(dir.prompt(PromptKind::Start)).unwrap();
        assert!(file.starts_with("---\ntype: prompt\n") && file.ends_with(&modal.text));
        assert!(!modal.text.contains("generated:") && !modal.text.starts_with("---"));
        assert!(
            app.world().resource::<SimControl>().paused,
            "waits while open"
        );
        app.world_mut().write_message(PromptModalClosed);
        app.update();
        assert!(!app.world().resource::<SimControl>().paused);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// End to end: the OK button's `Activate` (what a click or Enter on it
    /// sends) closes the modal and the sim runs.
    #[test]
    fn the_ok_button_unpauses_the_sim() {
        use bevy::ui_widgets::Activate;
        let home = tmp("modal-ok");
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins);
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        show_prompt_modal(&mut app);
        app.update();
        assert!(app.world().resource::<SimControl>().paused);
        let ok = app
            .world_mut()
            .query::<(Entity, &Name)>()
            .iter(app.world())
            .find(|(_, n)| n.as_str() == "prompt modal OK")
            .map(|(e, _)| e)
            .expect("the OK button");
        app.world_mut().trigger(Activate { entity: ok });
        app.world_mut().flush();
        app.update();
        assert!(!app.world().resource::<PromptModal>().open);
        assert!(!app.world().resource::<SimControl>().paused);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Closing the last window (not a hand-written `AppExit`) saves the
    /// world at the current tick: `save_on_exit` runs after
    /// `exit_on_all_closed` wrote the exit in the same `Last`, whichever
    /// was added first.
    #[test]
    fn closing_the_window_saves_the_current_tick() {
        for window_first in [true, false] {
            closing_the_window_saves(window_first);
        }
    }

    fn closing_the_window_saves(window_first: bool) {
        use bevy::window::{ExitCondition, WindowPlugin};
        let window_plugin = || WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::OnAllClosed,
            ..default()
        };
        let home = tmp(&format!("close-{window_first}"));
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins);
            if window_first {
                app.add_plugins(window_plugin());
            }
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15703).unwrap();
        if !window_first {
            app.add_plugins(window_plugin());
        }
        let window = app.world_mut().spawn(Window::default()).id();
        app.update();
        assert!(app.should_exit().is_none(), "a window is open");
        run_ticks(app.world_mut(), 300);
        let save = dir.save("W");
        assert_eq!(snapshot::read(&save).unwrap().tick, 0, "only the New save");
        edit_save(&save, |v| {
            v["header"]["saved"] = "2000-01-01T00:00:00Z".into()
        });

        app.world_mut().despawn(window);
        app.update();
        assert!(app.should_exit().is_some(), "the last window closed");
        let snap = snapshot::read(&save).unwrap();
        assert_eq!(
            snap.tick, 300,
            "saved when the window closed ({window_first})"
        );
        assert_ne!(snap.header["saved"], "2000-01-01T00:00:00Z");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn exact_port_flag_needs_resume() {
        let parse = |flags: &[&str]| {
            let mut f = ExplorerFlags::default();
            let mut it = flags.iter();
            while let Some(flag) = it.next() {
                if !f.switch(flag) {
                    assert!(f.value(flag, it.next().unwrap()), "{flag}");
                }
            }
            f.finish(false, false)
        };
        let a = parse(&[
            "--explorer-home",
            "h",
            "--agent",
            "A",
            "--resume",
            "--exact-port",
            "15710",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(a.exact_port, Some(15710));
        for (flags, needle) in [
            (&["--exact-port", "15710"][..], "--explorer-home"),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--world",
                    "W",
                    "--exact-port",
                    "15710",
                ][..],
                "--resume",
            ),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--resume",
                    "--exact-port",
                    "nope",
                ][..],
                "not a port",
            ),
            (
                &[
                    "--explorer-home",
                    "h",
                    "--agent",
                    "A",
                    "--resume",
                    "--check-save",
                    "--exact-port",
                    "15710",
                ][..],
                "--check-save",
            ),
        ] {
            let e = parse(flags).unwrap_err();
            assert!(e.contains(needle), "{flags:?}: {e}");
        }
    }

    #[test]
    fn wait_for_port_takes_a_port_freed_in_time() {
        let held = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!wait_for_port(port, Duration::from_millis(300)), "taken");
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            drop(held);
        });
        let start = Instant::now();
        // Generous: the release thread may start late on a busy machine.
        assert!(wait_for_port(port, Duration::from_secs(60)), "freed");
        assert!(start.elapsed() >= Duration::from_millis(400));
        release.join().unwrap();
    }

    /// A child this test owns that just waits (killed by pid, by the test).
    fn sleeper() -> std::process::Child {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("ping");
            c.args(["-n", "120", "127.0.0.1"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("120");
            c
        };
        cmd.stdout(std::process::Stdio::null())
            .spawn()
            .expect("a sleeping child")
    }

    fn write_run(dir: &AgentDir, pid: u32, port: u16) -> RunFile {
        let run = RunFile {
            pid,
            port,
            mode: RunMode::Detached,
            started: "2026-10-25T12:00:00Z".into(),
            agent: "Wren".into(),
            world: "W".into(),
        };
        run.write(dir).unwrap();
        run
    }

    fn free_port() -> u16 {
        TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// Fix round 1: the handover waits, within its budget, for the detached
    /// host to exit after it released the port; one that stays is named.
    #[test]
    fn the_handover_waits_for_the_detached_host_to_exit() {
        let home = tmp("handover");
        let dir = AgentDir {
            dir: home.join("wren"),
        };
        std::fs::create_dir_all(&dir.dir).unwrap();
        // Still alive when the budget ends: refused, naming it.
        let mut child = sleeper();
        let run = write_run(&dir, child.id(), free_port());
        assert_eq!(
            wait_for_handover(&dir, free_port(), Duration::from_millis(500)),
            Handover::Running(run.clone())
        );
        let msg = already_running("Wren", &run);
        assert!(
            msg.contains(&format!(
                "agent Wren is already running (pid {}, mode detached",
                run.pid
            )),
            "{msg}"
        );
        // Exits within the budget: the handover goes ahead.
        let killer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            let _ = child.kill();
            let _ = child.wait();
        });
        let start = Instant::now();
        // Generous: tasklist is slow on a busy machine.
        assert_eq!(
            wait_for_handover(&dir, free_port(), Duration::from_secs(60)),
            Handover::Ready
        );
        assert!(start.elapsed() >= Duration::from_millis(600));
        killer.join().unwrap();
        // A run.json that is our own, or gone, is no obstacle.
        let _ = write_run(&dir, std::process::id(), 1);
        assert_eq!(running_elsewhere(&dir), None);
        std::fs::remove_file(RunFile::path(&dir)).unwrap();
        assert_eq!(
            wait_for_handover(&dir, free_port(), Duration::from_millis(500)),
            Handover::Ready
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The handover path: no prompt modal (the agent is already playing),
    /// the sim runs, and a notice says where the world came from for 5 s.
    #[test]
    fn exact_port_skips_the_modal() {
        let home = tmp("moved");
        let (_, _, _) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
        let (mut app, start, dir) = build_explorer_app(&resume_args(&home, "Wren", None), |app| {
            app.add_plugins(MinimalPlugins);
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15711).unwrap();
        show_moved_notice(&mut app);
        app.update();
        app.update();
        assert!(
            app.world().get_resource::<PromptModal>().is_none(),
            "no modal"
        );
        assert!(!app.world().resource::<SimControl>().paused, "the sim runs");
        let texts = |app: &mut App| -> Vec<String> {
            app.world_mut()
                .query_filtered::<&Text, With<MovedNotice>>()
                .iter(app.world())
                .map(|t| t.0.clone())
                .collect()
        };
        let shown = texts(&mut app);
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert!(shown[0].contains("moved here from detached"), "{shown:?}");
        assert!(
            shown[0].contains("15711") && shown[0].is_ascii(),
            "{shown:?}"
        );
        // Gone once its 5 s are up.
        for mut notice in app
            .world_mut()
            .query::<&mut MovedNotice>()
            .iter_mut(app.world_mut())
        {
            notice.left = Duration::ZERO;
        }
        app.update();
        assert!(texts(&mut app).is_empty(), "the notice expired");
        assert_eq!(MOVED_NOTICE_FOR, Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `run.json` (mode window) while the window runs; removed on exit.
    #[test]
    fn the_window_writes_run_json_and_removes_it_on_exit() {
        use testbed_content::explorer::host::{RunFile, RunMode};
        let home = tmp("run-json");
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins);
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15712).unwrap();
        let run = RunFile::read_live(&dir).expect("run.json");
        assert_eq!(run.pid, std::process::id());
        assert_eq!((run.port, run.mode), (15712, RunMode::Window));
        assert_eq!((run.agent.as_str(), run.world.as_str()), ("Wren", "W"));
        app.update();
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(!RunFile::path(&dir).exists(), "removed on exit");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Task 29: the menu's `app/quit` takes the window's "No" path: it
    /// answers first, then the exit saves the current tick, removes
    /// `run.json` and exits 0. A request naming another pid is refused.
    #[cfg(feature = "debug-tools")]
    #[test]
    fn app_quit_saves_and_exits() {
        use npc_framework::view_helpers::remote::{AppQuitPlugin, QUIT_DELAY, app_quit};
        use testbed_content::explorer::host::RunFile;
        let home = tmp("app-quit");
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins);
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, 15714).unwrap();
        app.add_plugins(AppQuitPlugin);
        app.update();
        run_ticks(app.world_mut(), 120);
        let save = dir.save("W");
        edit_save(&save, |v| {
            v["header"]["saved"] = "2000-01-01T00:00:00Z".into()
        });
        assert!(RunFile::path(&dir).exists());
        // Another process's pid: refused, nothing happens.
        let other = json_pid(std::process::id().wrapping_add(4));
        assert!(app_quit(bevy::ecs::system::In(other), app.world_mut()).is_err());
        // Ours: answered at once, the exit comes a moment later.
        let answer = app_quit(
            bevy::ecs::system::In(json_pid(std::process::id())),
            app.world_mut(),
        )
        .expect("answered");
        assert_eq!(answer["quitting"], true, "{answer}");
        app.update();
        assert!(app.should_exit().is_none(), "answered before exiting");
        std::thread::sleep(QUIT_DELAY + Duration::from_millis(100));
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        let snap = snapshot::read(&save).unwrap();
        assert_eq!(snap.tick, 120, "saved at the quit");
        assert_ne!(snap.header["saved"], "2000-01-01T00:00:00Z");
        assert!(!RunFile::path(&dir).exists(), "run.json removed");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[cfg(feature = "debug-tools")]
    fn json_pid(pid: u32) -> Option<Value> {
        Some(serde_json::json!({ "pid": pid }))
    }

    /// What the fake detach spawner was asked to start.
    type Spawned = std::sync::Arc<std::sync::Mutex<Vec<(PathBuf, Vec<String>)>>>;

    /// A [`DetachSpawner`] that records the command instead of starting
    /// `npcctl` (pid 4242).
    fn fake_spawner(app: &mut App) -> Spawned {
        let spawned: Spawned = std::sync::Arc::default();
        let log = std::sync::Arc::clone(&spawned);
        app.insert_resource(DetachSpawner(std::sync::Arc::new(move |program, args| {
            log.lock()
                .unwrap()
                .push((program.to_path_buf(), args.to_vec()));
            Ok(4242)
        })));
        spawned
    }

    #[derive(Resource, Default)]
    struct InputRuns(u32);

    fn count_input(mut runs: ResMut<InputRuns>) {
        runs.0 += 1;
    }

    /// An explorer window (no renderer) with the close-to-detached dialog on
    /// `port`, a fake spawner, a probe in the game's input set and an open
    /// window; the sim is past tick 0.
    fn window_app(tag: &str, port: u16) -> (App, PathBuf, AgentDir, Spawned, Entity) {
        use bevy::window::{ExitCondition, WindowPlugin};
        use npc_framework::view_helpers::ViewSystems;
        let home = tmp(tag);
        let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |app| {
            app.add_plugins(MinimalPlugins).add_plugins(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::OnAllClosed,
                close_when_requested: false,
                ..default()
            });
        })
        .unwrap();
        prepare_session(&mut app, &start, &dir, port).unwrap();
        let spawned = fake_spawner(&mut app);
        install_close_to_detached(&mut app, &home, "Wren", port);
        // As if npcctl were built next to the test binary.
        app.world_mut().resource_mut::<DetachTarget>().npcctl.1 = false;
        app.init_resource::<InputRuns>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, count_input.in_set(ViewSystems::Input));
        let window = app.world_mut().spawn(Window::default()).id();
        app.update();
        run_ticks(app.world_mut(), 40);
        (app, home, dir, spawned, window)
    }

    fn dialog_button(app: &mut App, name: &str) -> Entity {
        app.world_mut()
            .query::<(Entity, &Name)>()
            .iter(app.world())
            .find(|(_, n)| n.as_str() == name)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn press_key(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.clear();
        keys.press(key);
    }

    /// Task 8: closing the explorer window asks "Keep it running detached?"
    /// (the game's keys do not run meanwhile); Yes saves, starts `npcctl
    /// serve --resume --port N --wait-port` (a fake spawner here) on the
    /// same port and exits.
    #[test]
    fn close_offers_detached() {
        use bevy::ui_widgets::Activate;
        use bevy::window::WindowCloseRequested;
        let (mut app, home, dir, spawned, window) = window_app("close-detached", 15714);
        let runs = app.world().resource::<InputRuns>().0;
        assert!(runs > 0, "the game's input runs while no dialog is open");
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        app.update();
        assert!(app.world().resource::<CloseDialog>().open, "it asks");
        assert!(
            app.should_exit().is_none(),
            "nothing closes before the answer"
        );
        let texts: Vec<String> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|t| t.0.clone())
            .collect();
        assert!(
            texts.iter().any(|t| t == "Keep it running detached?"),
            "{texts:?}"
        );
        // The game's keys are off while it asks.
        let runs = app.world().resource::<InputRuns>().0;
        app.update();
        assert_eq!(app.world().resource::<InputRuns>().0, runs);
        assert!(spawned.lock().unwrap().is_empty(), "not before Yes");

        let yes = dialog_button(&mut app, "close dialog Yes");
        app.world_mut().trigger(Activate { entity: yes });
        app.world_mut().flush();
        app.update();
        assert!(app.should_exit().is_some(), "the window exits");
        let calls = spawned.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert_eq!(calls[0].0, npcctl_path().0);
        assert_eq!(
            calls[0].1,
            testbed_content::explorer::host::detached_resume_args(&home, "Wren", 15714)
        );
        assert!(calls[0].1.windows(2).any(|w| w == ["--port", "15714"]));
        // Saved at the tick the world reached, for the detached host to load.
        assert_eq!(snapshot::read(&dir.save("W")).unwrap().tick, 40);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// No: save and exit, nothing detached; Escape cancels (back to the game).
    #[test]
    fn close_without_detaching_saves_and_exits() {
        use bevy::ui_widgets::Activate;
        use bevy::window::WindowCloseRequested;
        let (mut app, home, dir, spawned, window) = window_app("close-no", 15715);
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        press_key(&mut app, KeyCode::Escape);
        app.update();
        assert!(
            !app.world().resource::<CloseDialog>().open,
            "Escape cancels"
        );
        assert!(app.should_exit().is_none());
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.clear();
        }
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        app.update();
        let no = dialog_button(&mut app, "close dialog No");
        app.world_mut().trigger(Activate { entity: no });
        app.world_mut().flush();
        app.update();
        assert!(app.should_exit().is_some());
        assert!(spawned.lock().unwrap().is_empty());
        assert_eq!(snapshot::read(&dir.save("W")).unwrap().tick, 40);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// F9 moves the world to detached on purpose, without asking.
    #[test]
    fn f9_moves_to_detached_without_asking() {
        let (mut app, home, _dir, spawned, _window) = window_app("f9", 15716);
        press_key(&mut app, KeyCode::F9);
        app.update();
        assert!(!app.world().resource::<CloseDialog>().open, "no dialog");
        assert!(app.should_exit().is_some());
        let calls = spawned.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert!(calls[0].1.windows(2).any(|w| w == ["--port", "15716"]));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A failed start puts the pause state back (here: running).
    #[test]
    fn a_failed_detach_restores_the_pause_state() {
        let (mut app, home, _dir, _spawned, _window) = window_app("detach-unpause", 15718);
        app.world_mut().resource_mut::<SimControl>().paused = false;
        app.insert_resource(DetachSpawner(std::sync::Arc::new(|_, _| {
            Err(std::io::Error::other("no such file"))
        })));
        press_key(&mut app, KeyCode::F9);
        app.update();
        assert!(app.should_exit().is_none());
        assert!(
            !app.world().resource::<SimControl>().paused,
            "running again"
        );
        // Paused before: paused after.
        app.world_mut().resource_mut::<SimControl>().paused = true;
        press_key(&mut app, KeyCode::F9);
        app.update();
        assert!(app.world().resource::<SimControl>().paused);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// While the close dialog is open the prompt modal behind it does not
    /// act (its OK applies only after the dialog closed).
    #[test]
    fn the_close_dialog_freezes_the_prompt_modal() {
        use bevy::ui_widgets::Activate;
        use bevy::window::WindowCloseRequested;
        let (mut app, home, _dir, _spawned, window) = window_app("dialog-modal", 15719);
        show_prompt_modal(&mut app);
        app.update();
        assert!(app.world().resource::<PromptModal>().open);
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        let ok = dialog_button(&mut app, "prompt modal OK");
        app.world_mut().trigger(Activate { entity: ok });
        app.world_mut().flush();
        app.update();
        app.update();
        assert!(app.world().resource::<PromptModal>().open, "frozen");
        let cancel = dialog_button(&mut app, "close dialog Cancel");
        app.world_mut().trigger(Activate { entity: cancel });
        app.world_mut().flush();
        app.update();
        app.update();
        assert!(!app.world().resource::<CloseDialog>().open);
        assert!(
            !app.world().resource::<PromptModal>().open,
            "OK applies now"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A start that fails keeps the window: the dialog names the error.
    #[test]
    fn a_failed_detach_keeps_the_window() {
        use bevy::ui_widgets::Activate;
        use bevy::window::WindowCloseRequested;
        let (mut app, home, _dir, _spawned, window) = window_app("detach-fails", 15717);
        app.insert_resource(DetachSpawner(std::sync::Arc::new(|_, _| {
            Err(std::io::Error::other("no such file"))
        })));
        app.world_mut()
            .write_message(WindowCloseRequested { window });
        app.update();
        app.update();
        let yes = dialog_button(&mut app, "close dialog Yes");
        app.world_mut().trigger(Activate { entity: yes });
        app.world_mut().flush();
        app.update();
        app.update();
        assert!(app.should_exit().is_none(), "the window stays");
        let dialog = app.world().resource::<CloseDialog>();
        assert!(dialog.open);
        let e = dialog.error.clone().unwrap_or_default();
        assert!(e.contains("no such file"), "{e}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn record_with_resume_is_refused() {
        let ex = resume_args(Path::new("h"), "Wren", None);
        assert!(
            refuse_recording_on_resume(&ex, true, false)
                .unwrap_err()
                .contains("--resume")
        );
        assert!(refuse_recording_on_resume(&ex, false, true).is_err());
        assert!(refuse_recording_on_resume(&ex, false, false).is_ok());
        assert!(
            refuse_recording_on_resume(&new_args(Path::new("h"), "W", "X"), true, true).is_ok()
        );
    }
}
