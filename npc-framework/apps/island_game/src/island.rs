//! Local island persistence. Host files and timing never participate in simulation.
use bevy::prelude::*;
use island_view::IslandMode;
use npc_framework::{
    agent_io::scenario::{ScenarioDoc, attach_controllers},
    snapshot,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) mod session;

pub fn parse_mode(value: &str) -> Option<IslandMode> {
    match value {
        "inspector" => Some(IslandMode::Inspector),
        "character" => Some(IslandMode::Character),
        "editor" => Some(IslandMode::Editor),
        _ => None,
    }
}

#[derive(Resource)]
struct LocalIsland {
    path: PathBuf,
    doc: ScenarioDoc,
    seed: u64,
    last: Instant,
    message: String,
    /// The last save attempt failed; `message` says why.
    failed: bool,
    /// The last save was the minute timer's own, not one the player asked
    /// for. The player session announces only saves that were asked for.
    routine: bool,
    // OS lock survives for the host lifetime and releases even after a crash.
    _lock: Option<File>,
    active: bool,
    /// "Quit without saving" was confirmed: the exit writes nothing.
    leave_unsaved: bool,
    /// The last autosave failed and none has succeeded since.
    auto_failed: bool,
    /// When this session last wrote the autosave.
    last_auto: Option<Instant>,
    /// The minute timer's save, finishing beside the frame.
    writing: Option<BackgroundSave>,
}

/// A timer save whose file is being written on its own thread: the frame
/// reads the world (`island_save::begin_save`, about 20 ms instead of about
/// 290 ms for the whole save in release), the thread turns the two big
/// grids into JSON and writes the file, the same file a synchronous save
/// writes. Every other save, and anything that reads or removes the save
/// files, waits for it first ([`settle_saves`]).
struct BackgroundSave {
    /// The autosave (player session) or the save itself (developer host).
    auto: bool,
    thread: std::thread::JoinHandle<Result<(), String>>,
}
#[derive(Component)]
struct SaveLabel;

/// Where this session's world is saved; host files such as player settings
/// live beside it.
pub(crate) fn save_path(world: &World) -> Option<PathBuf> {
    world
        .get_resource::<LocalIsland>()
        .map(|island| island.path.clone())
}

/// The latest save outcome, for the player session's one notice.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SaveNotice {
    pub(crate) text: String,
    pub(crate) failed: bool,
    /// Written by the minute timer, not on the player's request.
    pub(crate) routine: bool,
    /// How long ago it was written.
    pub(crate) age: Duration,
}

pub(crate) fn save_notice(world: &World) -> Option<SaveNotice> {
    world
        .get_resource::<LocalIsland>()
        .filter(|island| island.active)
        .map(|island| SaveNotice {
            text: island.message.clone(),
            failed: island.failed,
            routine: island.routine,
            age: island.last.elapsed(),
        })
}

pub fn open(
    path: &Path,
    seed_override: Option<u64>,
    rules: Option<&Path>,
    fresh: bool,
    host: impl FnOnce(&mut App),
) -> Result<App, String> {
    let lock = lock_save(path)?;
    let saved = if path.exists() && !fresh {
        Some(snapshot::read(path)?)
    } else {
        None
    };
    let (doc, seed) = if let Some(saved) = &saved {
        saved_construction(saved, seed_override)?
    } else {
        (fresh_construction(rules)?, seed_override.unwrap_or(7))
    };
    let mut loaded = island_content::scenario::load_with(&doc, seed, host)?;
    attach_controllers(&doc, &mut loaded)?;
    let mut app = loaded.app;
    if let Some(saved) = &saved {
        restore_validated(app.world_mut(), saved).map_err(|error| error.to_string())?;
    }
    install(&mut app, path, doc, seed, Some(lock), true);
    if fresh && path.exists() {
        backup(path, false)?;
    }
    save(app.world_mut())?;
    Ok(app)
}

/// Why the title cannot simply continue: the card it shows above its
/// buttons. The save file is always kept as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Issue {
    /// Saved by an older version of the game.
    Older,
    /// Saved by another build of this version (its rules or names differ).
    OtherBuild,
    /// Saved by a newer version of the game.
    Newer,
    /// It could not be opened; the details, never in the main sentence.
    Unopened(String),
    /// The player's changed island rules could not be read; New game uses
    /// the island that comes with the game. The details.
    Rules(String),
}

/// Why a save does not continue, for the title. The file is always kept.
impl Issue {
    /// A save the game cannot recreate: its backup is never deleted.
    pub(crate) fn keeps_forever(&self) -> bool {
        // Every save the game refused, the unreadable one too: it is kept
        // as "kept-<n>", never pruned with the "previous" backups.
        matches!(
            self,
            Self::Older | Self::OtherBuild | Self::Newer | Self::Unopened(_)
        )
    }
}

pub(crate) fn refusal(check: &island_content::village::island_save::IslandSaveCheck) -> Issue {
    use island_content::village::island_save::IslandSaveCheck;
    match check {
        IslandSaveCheck::OlderIsland { .. } => Issue::Older,
        IslandSaveCheck::NewerIsland { .. } => Issue::Newer,
        IslandSaveCheck::OtherBuild => Issue::OtherBuild,
        other => Issue::Unopened(other.sentence()),
    }
}

/// Why a saved island could not be restored.
#[derive(Debug)]
pub(crate) enum RestoreError {
    /// The simulation refuses this save (older, newer, unreadable): the
    /// title says so and keeps the file.
    Refused(Issue),
    /// It should have loaded and did not.
    Failed(String),
}

impl From<String> for RestoreError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(Issue::Older) => {
                f.write_str("The island was saved by an older version of the game.")
            }
            Self::Refused(Issue::OtherBuild) => {
                f.write_str("The island was saved by another build of the game.")
            }
            Self::Refused(Issue::Newer) => {
                f.write_str("The island was saved by a newer version of the game.")
            }
            Self::Refused(Issue::Unopened(text) | Issue::Rules(text)) | Self::Failed(text) => {
                f.write_str(text)
            }
        }
    }
}

fn lock_save(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("world.lock"))
        .map_err(|error| format!("Cannot open island lock: {error}"))?;
    lock.try_lock().map_err(|_| {
        "This island is already open in another window. Close that window before continuing."
            .to_owned()
    })?;
    Ok(lock)
}

fn saved_construction(
    saved: &snapshot::WorldSnapshot,
    seed_override: Option<u64>,
) -> Result<(ScenarioDoc, u64), String> {
    if saved
        .header
        .get("local_island")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err(
            "This file is not a local Island Village save; choose a different --island-save path."
                .into(),
        );
    }
    let doc: ScenarioDoc = serde_json::from_value(
        saved
            .header
            .get("scenario")
            .cloned()
            .ok_or("Island save is missing its immutable scenario")?,
    )
    .map_err(|e| e.to_string())?;
    let seed = saved
        .header
        .get("seed")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Island save is missing its seed")?;
    if seed_override.is_some_and(|requested| requested != seed) {
        return Err("The saved island has a different seed. Use another --island-save path for a new island.".into());
    }
    // The world is built from the rules the save carries, never from the
    // world file installed beside it; the played character keeps the name
    // the save gave it. `load_island_save` then checks they agree.
    let rules = island_content::village::island_save::saved_rules(saved)
        .map_err(|refused| refused.sentence())?;
    let mut built = construction_from_preset(rules)?;
    let name = doc
        .world
        .get("construction")
        .and_then(|construction| construction.get("actor"))
        .cloned();
    if let Some(name) = name {
        if let Some(actor) = built.actors.first_mut() {
            actor.extra.insert("name".into(), name.clone());
        }
        if let Some(context) = built
            .world
            .get_mut("construction")
            .and_then(serde_json::Value::as_object_mut)
        {
            context.insert("actor".into(), name);
        }
    }
    Ok((built, seed))
}

fn fresh_construction(rules: Option<&Path>) -> Result<ScenarioDoc, String> {
    let preset = if let Some(path) = rules {
        if !path.exists() && path.file_stem().is_some_and(|name| name == "living_world") {
            island_content::worlds::install_builtins(
                path.parent().ok_or("World rules need a parent directory")?,
            )?;
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read island rules {}: {e}", path.display()))?;
        serde_json::from_str::<island_content::worlds::WorldPreset>(
            text.trim_start_matches('\u{feff}'),
        )
        .map_err(|e| e.to_string())?
    } else {
        island_content::worlds::builtin_preset("living_world").expect("shipped island rules")
    };
    construction_from_preset(preset)
}

/// A world file the player chose for a new game, such as one Shape
/// exported: read, parsed and checked exactly like the island rules.
pub(crate) fn world_file_preset(
    path: &Path,
) -> Result<island_content::worlds::WorldPreset, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("it cannot be read: {e}"))?;
    let preset = serde_json::from_str::<island_content::worlds::WorldPreset>(
        text.trim_start_matches('\u{feff}'),
    )
    .map_err(|e| format!("it is not a world file: {e}"))?;
    // The same guard every new island passes.
    construction_from_preset(preset.clone())?;
    Ok(preset)
}

/// The immutable construction of a chosen world file.
pub(crate) fn world_file_construction(path: &Path) -> Result<ScenarioDoc, String> {
    construction_from_preset(world_file_preset(path)?)
}

fn shipped_preset(_rules: Option<&Path>) -> island_content::worlds::WorldPreset {
    island_content::worlds::builtin_preset("living_world").expect("shipped island profile")
}

fn construction_from_preset(
    preset: island_content::worlds::WorldPreset,
) -> Result<ScenarioDoc, String> {
    island_content::construction::from_preset(&preset)
}

/// Restore `saved` into a fresh world of this build, after the
/// simulation's own check: only a current save is restored (strictly, then
/// validated); anything else is refused with its sentence and kept.
fn restore_validated(
    world: &mut World,
    saved: &snapshot::WorldSnapshot,
) -> Result<(), RestoreError> {
    // The simulation's load is the one gate: version, build, the rules the
    // save carries, structure and a strict load. The host only names the
    // result.
    match island_content::village::island_save::load_island_save(world, saved) {
        Ok(_) => Ok(validate_loaded(world)?),
        Err(refused) => Err(RestoreError::Refused(refusal(&refused))),
    }
}

fn install(
    app: &mut App,
    path: &Path,
    doc: ScenarioDoc,
    seed: u64,
    lock: Option<File>,
    active: bool,
) {
    app.insert_resource(LocalIsland {
        path: path.into(),
        doc,
        seed,
        last: Instant::now(),
        active,
        // Said in the pause menu, never as a toast (closing phase, defect 7).
        message: "Island saves automatically".into(),
        failed: false,
        routine: true,
        _lock: lock,
        leave_unsaved: false,
        auto_failed: false,
        last_auto: None,
        writing: None,
    })
    .add_systems(Startup, spawn_label)
    .init_resource::<island_view::BehaviourFileRequest>()
    .add_systems(Update, behaviour_file)
    .add_systems(
        Update,
        (finish_saves, save_input, close_requested, refresh_label).chain(),
    )
    .add_systems(Last, save_on_exit.in_set(bevy::app::OnAppExitSystems));
}

/// Backups New game keeps (by number); an island from an older version is
/// never deleted by the game.
pub(crate) const KEPT_BACKUPS: usize = 5;

/// The numbers of the backups of `path` of one kind (`previous`, `kept`):
/// files named exactly `<stem>.<kind>-<digits>.json`, ascending.
pub(crate) fn backup_numbers(path: &Path, kind: &str) -> Vec<u64> {
    let (Some(folder), Some(stem)) = (
        path.parent(),
        path.file_stem().and_then(|stem| stem.to_str()),
    ) else {
        return Vec::new();
    };
    let folder = if folder.as_os_str().is_empty() {
        Path::new(".")
    } else {
        folder
    };
    let prefix = format!("{stem}.{kind}-");
    let mut numbers: Vec<u64> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let digits = name.strip_prefix(&prefix)?.strip_suffix(".json")?;
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| digits.parse().ok())
                .flatten()
        })
        .collect();
    numbers.sort_unstable();
    numbers
}

/// The name the next backup of `path` gets: the highest number plus one, so
/// the highest is always the newest. Nothing is written.
pub(crate) fn next_backup(path: &Path, older: bool) -> PathBuf {
    // "kept" for any save the game refused (older, newer, another build or
    // unreadable): the name says nothing it cannot know.
    let kind = if older { "kept" } else { "previous" };
    let number = backup_numbers(path, kind).last().copied().unwrap_or(0) + 1;
    path.with_extension(format!("{kind}-{number}.json"))
}

fn backup(path: &Path, older: bool) -> Result<PathBuf, String> {
    let next = next_backup(path, older);
    let mut destination = File::options()
        .write(true)
        .create_new(true)
        .open(&next)
        .map_err(|error| format!("Could not create an island backup: {error}"))?;
    let mut source = File::open(path).map_err(|e| e.to_string())?;
    std::io::copy(&mut source, &mut destination)
        .map_err(|e| format!("Could not back up the current island: {e}"))?;
    destination.sync_all().map_err(|e| e.to_string())?;
    Ok(next)
}

/// After a new island's first save: keep the newest [`KEPT_BACKUPS`]
/// ordinary backups. Older-version islands are never touched; a delete that
/// fails is logged and ignored.
pub(crate) fn prune_backups(path: &Path) {
    let numbers = backup_numbers(path, "previous");
    let surplus = numbers.len().saturating_sub(KEPT_BACKUPS);
    for number in &numbers[..surplus] {
        let file = path.with_extension(format!("previous-{number}.json"));
        if let Err(error) = std::fs::remove_file(&file) {
            warn!("could not remove an old backup {}: {error}", file.display());
        }
    }
}

fn validate_loaded(world: &World) -> Result<(), String> {
    island_content::village::integrity::validate_colony(world)
        .map_err(|reason| format!("{reason}; the original island save has been preserved."))
}

/// The autosave beside a save: `<stem>.autosave.json`. Written every minute
/// of play in the player session, for crashes only; a clean exit removes it,
/// and Continue loads it only when the player chooses to.
pub(crate) fn autosave_path(path: &Path) -> PathBuf {
    path.with_extension("autosave.json")
}

/// Read the island in play for a save, with the host's header. `None`
/// while no island is in play (the title, or after a failed restore).
fn capture_island(
    world: &mut World,
) -> Result<Option<island_content::village::island_save::SaveInProgress>, String> {
    let local = world.resource::<LocalIsland>();
    if !local.active {
        return Ok(None);
    }
    if local._lock.is_none() {
        return Err(
            "The island is open in another window of the game. Close that one first.".into(),
        );
    }
    // Stamped with the island's save version and content: a later build
    // tells an older save from a damaged one.
    let mut capture = island_content::village::island_save::begin_save(world)?;
    let local = world.resource::<LocalIsland>();
    let header = capture.header_mut();
    header.insert("local_island".into(), 1.into());
    header.insert(
        "scenario".into(),
        serde_json::to_value(&local.doc).map_err(|e| e.to_string())?,
    );
    header.insert("seed".into(), local.seed.into());
    Ok(Some(capture))
}

/// The slow half of a save: the big grids to JSON, then the file.
fn write_capture(
    capture: island_content::village::island_save::SaveInProgress,
    target: &Path,
) -> Result<(), String> {
    let snapshot = capture.finish()?;
    snapshot::write_atomic(target, &snapshot).map_err(|e| e.to_string())
}

/// Write the island in play to `target`, now. Nothing is written while no
/// island is in play (the title, or after a failed restore).
fn write_island(world: &mut World, target: &Path) -> Result<bool, String> {
    settle_saves(world);
    match capture_island(world)? {
        Some(capture) => write_capture(capture, target).map(|()| true),
        None => Ok(false),
    }
}

/// The minute timer's save: read the island on this frame, write the file
/// on a thread ([`BackgroundSave`]); [`finish_saves`] takes its outcome.
fn begin_timer_save(world: &mut World, auto: bool) {
    settle_saves(world);
    let target = {
        let local = world.resource::<LocalIsland>();
        if auto {
            autosave_path(&local.path)
        } else {
            local.path.clone()
        }
    };
    world.resource_mut::<LocalIsland>().last = Instant::now();
    let capture = match capture_island(world) {
        Ok(Some(capture)) => capture,
        other => {
            let result = other.map(|_| false);
            timer_save_done(world, auto, result);
            return;
        }
    };
    let thread = std::thread::Builder::new()
        .name("island-save".into())
        .spawn(move || write_capture(capture, &target));
    match thread {
        Ok(thread) => {
            world.resource_mut::<LocalIsland>().writing = Some(BackgroundSave { auto, thread });
        }
        Err(error) => timer_save_done(world, auto, Err(error.to_string())),
    }
}

/// Takes the outcome of a finished timer save (each frame, before input).
fn finish_saves(world: &mut World) {
    let done = world
        .get_resource::<LocalIsland>()
        .and_then(|local| local.writing.as_ref())
        .is_some_and(|writing| writing.thread.is_finished());
    if done {
        settle_saves(world);
    }
}

/// Waits for a timer save still being written and takes its outcome.
pub(crate) fn settle_saves(world: &mut World) {
    let Some(writing) = world
        .get_resource_mut::<LocalIsland>()
        .and_then(|mut local| local.writing.take())
    else {
        return;
    };
    let result = writing
        .thread
        .join()
        .unwrap_or_else(|_| Err("the save thread stopped".into()))
        .map(|()| true);
    timer_save_done(world, writing.auto, result);
}

/// What a finished timer save leaves: the autosave's quiet bookkeeping, or
/// the developer host's routine save.
fn timer_save_done(world: &mut World, auto: bool, result: Result<bool, String>) {
    if auto {
        autosave_done(world, result);
    } else {
        match saved(world, result) {
            // A failed timer save is still reported; a completed one is routine.
            Ok(()) => world.resource_mut::<LocalIsland>().routine = true,
            Err(error) => note_failure(world, error),
        }
    }
}

/// The player session's autosave: the second file, never the player's own
/// save. A failure is said once in a lasting toast, retried every minute.
fn autosave_done(world: &mut World, result: Result<bool, String>) {
    let mut local = world.resource_mut::<LocalIsland>();
    local.last = Instant::now();
    match result {
        Ok(written) => {
            if written {
                local.last_auto = Some(Instant::now());
            }
            let failed = std::mem::take(&mut local.auto_failed);
            if failed {
                island_view::clear_island_toast(world);
            }
        }
        Err(error) => {
            error!("autosave failed: {error}");
            let first = !std::mem::replace(&mut local.auto_failed, true);
            if first {
                island_view::island_toast(
                    world,
                    island_view::ToastTone::Failed,
                    AUTOSAVE_FAILED,
                    true,
                    true,
                );
            }
        }
    }
}

/// The lasting toast after a failed autosave.
pub(crate) const AUTOSAVE_FAILED: &str = "The autosave failed. Your own save is not affected.";

/// Remove the autosave: after a clean exit, or quitting without saving.
pub(crate) fn remove_autosave(world: &mut World) {
    // An autosave still being written would come back after the removal.
    settle_saves(world);
    let Some(local) = world.get_resource::<LocalIsland>() else {
        return;
    };
    let target = autosave_path(&local.path);
    if target.exists()
        && let Err(error) = std::fs::remove_file(&target)
    {
        warn!(
            "could not remove the autosave {}: {error}",
            target.display()
        );
    }
}

/// Leave without writing anything: the save stays as it was, the autosave
/// goes, and the exit path skips its own save.
pub(crate) fn quit_unsaved(world: &mut World) {
    remove_autosave(world);
    if let Some(mut local) = world.get_resource_mut::<LocalIsland>() {
        local.leave_unsaved = true;
    }
    world.write_message(AppExit::Success);
}

/// How long ago the player's own save and the autosave were written, from
/// the files themselves (`None`: there is none).
pub(crate) fn save_ages(world: &World) -> (Option<Duration>, Option<Duration>) {
    let Some(local) = world.get_resource::<LocalIsland>() else {
        return (None, None);
    };
    let age = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| at.elapsed().ok())
    };
    let auto = local.last_auto.map(|at| at.elapsed()).or_else(|| {
        let target = autosave_path(&local.path);
        target.exists().then(|| age(&target)).flatten()
    });
    (age(&local.path), auto)
}

/// Whether the last autosave failed and none has succeeded since.
pub(crate) fn autosave_failed(world: &World) -> bool {
    world
        .get_resource::<LocalIsland>()
        .is_some_and(|local| local.auto_failed)
}

pub(crate) fn save(world: &mut World) -> Result<(), String> {
    let path = world.resource::<LocalIsland>().path.clone();
    let result = write_island(world, &path);
    saved(world, result)
}

/// The bookkeeping of a save of the player's file (`Ok(false)`: nothing
/// was in play to save).
fn saved(world: &mut World, result: Result<bool, String>) -> Result<(), String> {
    if !result.map_err(|error| format!("{SAVE_FAILED} Details: {error}"))? {
        return Ok(());
    }
    let frontend = world.contains_resource::<crate::frontend::FrontendState>();
    let mut local = world.resource_mut::<LocalIsland>();
    local.last = Instant::now();
    local.failed = false;
    // Whoever asked for the save; the timer marks its own afterwards.
    local.routine = false;
    local.message = if frontend {
        "Island saved"
    } else {
        "Island saved | Ctrl+S to save again"
    }
    .into();
    Ok(())
}

/// The behaviour document beside the save: `behaviour.json`.
pub(crate) fn behaviour_path(world: &World) -> Option<PathBuf> {
    save_path(world).map(|path| {
        path.parent().map_or_else(
            || PathBuf::from("behaviour.json"),
            |folder| folder.join("behaviour.json"),
        )
    })
}

/// "Save behaviour to file" / "Reload behaviour from file": the host's file
/// work; the view checks the document and sends the one command.
fn behaviour_file(world: &mut World) {
    let Some(what) = island_view::take_behaviour_request(world) else {
        return;
    };
    let Some(path) = behaviour_path(world) else {
        return;
    };
    let (tone, text) = match what {
        island_view::BehaviourFile::Save => match island_view::behaviour_text(world)
            .ok_or_else(|| "this island has no behaviour document".to_owned())
            .and_then(|text| std::fs::write(&path, text).map_err(|error| error.to_string()))
        {
            Ok(()) => (
                island_view::ToastTone::Done,
                format!("Behaviour saved to {}.", file_name(&path)),
            ),
            Err(error) => {
                warn!("could not save the behaviour: {error}");
                (
                    island_view::ToastTone::Failed,
                    format!("The behaviour could not be saved. Details: {error}"),
                )
            }
        },
        island_view::BehaviourFile::Reload => match std::fs::read_to_string(&path) {
            Err(error) => {
                warn!("could not read {}: {error}", path.display());
                (
                    island_view::ToastTone::Failed,
                    format!(
                        "Behaviour not changed: {} could not be read. Save the behaviour to a file first.",
                        file_name(&path)
                    ),
                )
            }
            Ok(text) => match island_view::reload_from_text(world, &text) {
                Ok(done) => (island_view::ToastTone::Done, done.to_owned()),
                Err(refused) => (island_view::ToastTone::Failed, refused),
            },
        },
    };
    island_view::island_toast(
        world,
        tone,
        text,
        tone == island_view::ToastTone::Failed,
        false,
    );
}

/// A path as players read it: the file's name, not its folders.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The first sentence of every failed save.
pub(crate) const SAVE_FAILED: &str =
    "The island could not be saved. Free some disk space or check the folder, then try again.";
fn report_save(world: &mut World) -> bool {
    if let Err(error) = save(world) {
        note_failure(world, error);
        false
    } else {
        true
    }
}

/// A failed save of the player's file, said where the save notice reads it.
fn note_failure(world: &mut World, error: String) {
    error!("{error}");
    let mut local = world.resource_mut::<LocalIsland>();
    local.last = Instant::now();
    local.failed = true;
    local.message = error;
}
fn save_input(world: &mut World) {
    if world
        .get_resource::<island_view::FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let due = world.resource::<LocalIsland>().last.elapsed() >= Duration::from_secs(60);
    // F9 is a developer key; Ctrl+S is the player's save key.
    let developer = crate::developer_keys_on(world);
    let key = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| {
            (developer && keys.just_pressed(KeyCode::F9))
                || (keys.just_pressed(KeyCode::KeyS)
                    && [KeyCode::ControlLeft, KeyCode::ControlRight]
                        .into_iter()
                        .any(|key| keys.pressed(key) || keys.just_pressed(key)))
        });
    let player = world.contains_resource::<crate::frontend::FrontendState>();
    if key {
        report_save(world);
    } else if due {
        // The player session's minute timer writes the autosave beside the
        // save; the player's own save is only what they asked for. The
        // developer host's timer writes the save itself.
        begin_timer_save(world, player);
    }
}
pub(crate) fn close_requested(world: &mut World) {
    let closing = world
        .get_resource::<Messages<bevy::window::WindowCloseRequested>>()
        .is_some_and(|requests| !requests.is_empty());
    if closing {
        world
            .resource_mut::<Messages<bevy::window::WindowCloseRequested>>()
            .clear();
        // A developer host whose save keeps failing: a second close within
        // five seconds of a failed one leaves without saving.
        let again = {
            let local = world.resource::<LocalIsland>();
            local.failed && local.last.elapsed() < Duration::from_secs(5)
        };
        if again {
            world.resource_mut::<LocalIsland>().leave_unsaved = true;
            world.write_message(AppExit::Success);
        } else if report_save(world) {
            world.write_message(AppExit::Success);
        }
    }
}
pub(crate) fn save_on_exit(world: &mut World) {
    let unsaved = world
        .get_resource::<LocalIsland>()
        .is_some_and(|local| local.leave_unsaved);
    if unsaved {
        return;
    }
    if world
        .get_resource::<Messages<AppExit>>()
        .is_some_and(|events| !events.is_empty())
        && !report_save(world)
    {
        world.resource_mut::<Messages<AppExit>>().clear();
        let error = world.resource::<LocalIsland>().message.clone();
        crate::frontend::exit_failed(world, error);
        #[cfg(feature = "debug-tools")]
        world.remove_resource::<npc_framework::view_helpers::remote::QuitRequested>();
    }
}

/// Persistent hosts acknowledge shutdown only after the exact world is durable.
#[cfg(feature = "debug-tools")]
pub fn app_quit(
    In(params): In<Option<serde_json::Value>>,
    world: &mut World,
) -> npc_framework::tools::remote::BrpResult {
    use npc_framework::prelude::SimControl;
    use npc_framework::tools::remote::BrpError;
    use npc_framework::view_helpers::remote::{self, QuitRequested};
    if !world.contains_resource::<LocalIsland>() {
        return Err(BrpError::internal("No persistent island is open"));
    }
    let mut answer = remote::app_quit(In(params), world)?;
    let paused = world
        .get_resource::<SimControl>()
        .map(|control| control.paused);
    if let Some(mut control) = world.get_resource_mut::<SimControl>() {
        control.paused = true;
    }
    if !report_save(world) {
        world.remove_resource::<QuitRequested>();
        if let Some(before) = paused {
            world.resource_mut::<SimControl>().paused = before;
        }
        return Err(BrpError::internal(
            world.resource::<LocalIsland>().message.clone(),
        ));
    }
    answer["saved"] = true.into();
    Ok(answer)
}
fn spawn_label(mut commands: Commands, frontend: Option<Res<crate::frontend::FrontendState>>) {
    // The player session shows saves in its own fading notice (`save_notice`).
    if frontend.is_some() {
        return;
    }
    commands.spawn((
        SaveLabel,
        BackgroundColor(Color::srgba(0.04, 0.06, 0.07, 0.9)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(164),
            max_width: percent(55),
            padding: UiRect::all(px(5)),
            ..default()
        },
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.),
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.91, 0.83)),
    ));
}
fn refresh_label(
    local: Res<LocalIsland>,
    gate: Option<Res<island_view::FrontendGate>>,
    mut labels: Query<(&mut Text, &mut Node), With<SaveLabel>>,
) {
    let display = if gate.as_ref().is_some_and(|gate| gate.0) {
        Display::None
    } else {
        Display::Flex
    };
    for (mut label, mut node) in &mut labels {
        // Written only when it differs: a written text is measured and
        // laid out again on the next frame.
        if label.0 != local.message {
            label.0.clone_from(&local.message);
        }
        if node.display != display {
            node.display = display;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_game_keeps_the_five_newest_backups_and_every_older_version_island() {
        let dir = std::env::temp_dir().join(format!("npc-backups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.world.json");
        std::fs::write(&path, b"save").unwrap();
        // A gap and a stray name: the next number follows the highest.
        for name in [
            "island.world.previous-1.json",
            "island.world.previous-4.json",
            "island.world.previous-x.json",
            "island.world.kept-2.json",
        ] {
            std::fs::write(dir.join(name), b"old").unwrap();
        }
        assert_eq!(
            super::next_backup(&path, false),
            dir.join("island.world.previous-5.json"),
            "a backup gets the number after the highest"
        );
        assert_eq!(
            super::next_backup(&path, true),
            dir.join("island.world.kept-3.json")
        );
        for _ in 0..6 {
            super::backup(&path, false).unwrap();
        }
        super::backup(&path, true).unwrap();
        super::prune_backups(&path);
        assert_eq!(
            super::backup_numbers(&path, "previous"),
            [6, 7, 8, 9, 10],
            "the five newest stay"
        );
        assert_eq!(super::backup_numbers(&path, "kept"), [2, 3]);
        assert!(
            dir.join("island.world.previous-x.json").exists(),
            "a file that is not a backup is never deleted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use npc_framework::{kernel::hash::state_hash, prelude::*};

    #[test]
    fn regional_host_uses_authored_arrival_and_resumes_frozen_construction() {
        let dir = std::env::temp_dir().join(format!("npc-regional-host-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.json");
        let rules = dir.join("living_world.json");
        let preset = island_content::worlds::builtin_preset("living_world").unwrap();
        let start = preset.living_world.as_ref().unwrap().actor_start;
        std::fs::write(&rules, serde_json::to_vec(&preset).unwrap()).unwrap();
        let mut app = open(&path, Some(7), Some(&rules), false, |_| {}).unwrap();
        let played = app
            .world()
            .resource::<island_content::village::VillageState>()
            .played;
        let actor = app.world().resource::<SimIndex>().get(played).unwrap();
        assert_eq!(app.world().get::<SimPos>(actor), Some(&start));
        assert_eq!(
            island_content::village::colony::summary(app.world()).residents,
            42
        );
        run_ticks(app.world_mut(), 17);
        save(app.world_mut()).unwrap();
        let hash = state_hash(app.world_mut());
        drop(app);
        std::fs::write(&rules, "invalid replacement rules").unwrap();
        let mut restored = open(&path, None, Some(&rules), false, |_| {}).unwrap();
        assert_eq!(state_hash(restored.world_mut()), hash);
        drop(restored);
        let fresh_path = dir.join("fallback.json");
        let fallback = session::open(&fresh_path, None, Some(&rules), |_| {}).unwrap();
        // The title falls back to the island that comes with the game (this
        // was `frontier::enabled` before the split removed profile gates).
        let shipped =
            island_content::worlds::builtin_preset("living_world").expect("shipped island rules");
        assert_eq!(
            fallback
                .world()
                .resource::<island_content::world_runtime::ActiveWorld>()
                .0
                .name,
            shipped.name
        );
        assert!(
            session::summary(fallback.world())
                .issue
                .is_some_and(|issue| matches!(
                    issue,
                    Issue::Rules(details) if details.contains("The Living Island")
                ))
        );
        assert!(
            !fresh_path.exists(),
            "opening the title must not silently create a fallback save"
        );
        drop(fallback);
        std::fs::remove_file(fresh_path.with_extension("world.lock")).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&rules).unwrap();
        std::fs::remove_file(path.with_extension("world.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn local_island_resumes_the_same_state_and_refuses_a_second_owner() {
        let dir = std::env::temp_dir().join(format!("npc-local-island-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        let mut app = open(&path, Some(7), None, false, |_| {}).unwrap();
        assert!(open(&path, None, None, false, |_| {}).is_err());
        run_ticks(app.world_mut(), 101);
        save(app.world_mut()).unwrap();
        let hash = state_hash(app.world_mut());
        drop(app);
        let mut resumed = open(&path, None, None, false, |_| {}).unwrap();
        assert_eq!(state_hash(resumed.world_mut()), hash);
        assert_eq!(resumed.world().resource::<SimClock>().tick, 101);
        drop(resumed);
        assert!(open(&path, Some(42), None, false, |_| {}).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join("island.world.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn new_local_island_uses_authored_rules_and_resume_keeps_the_frozen_copy() {
        let dir =
            std::env::temp_dir().join(format!("npc-local-island-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        let rules = dir.join("custom.json");
        let mut preset = island_content::worlds::builtin_preset("living_world").unwrap();
        preset.island_ai.as_mut().unwrap().decision_ticks = 41;
        std::fs::write(&rules, serde_json::to_vec(&preset).unwrap()).unwrap();
        let app = open(&path, None, Some(&rules), false, |_| {}).unwrap();
        assert_eq!(
            app.world()
                .resource::<island_content::village::policy::IslandAiPolicy>()
                .decision_ticks,
            41
        );
        drop(app);
        std::fs::write(&rules, "invalid edited file").unwrap();
        let resumed = open(&path, None, Some(&rules), false, |_| {}).unwrap();
        assert_eq!(
            resumed
                .world()
                .resource::<island_content::village::policy::IslandAiPolicy>()
                .decision_ticks,
            41
        );
        drop(resumed);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(rules).unwrap();
        std::fs::remove_file(dir.join("island.world.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn fresh_start_backs_up_old_progress_and_failed_exit_keeps_the_world_open() {
        let dir =
            std::env::temp_dir().join(format!("npc-local-island-fresh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        let mut old = open(&path, None, None, false, |_| {}).unwrap();
        run_ticks(old.world_mut(), 33);
        save(old.world_mut()).unwrap();
        drop(old);
        let before = std::fs::read(&path).unwrap();
        let mut new = open(&path, None, None, true, |_| {}).unwrap();
        assert_eq!(new.world().resource::<SimClock>().tick, 0);
        assert_eq!(
            std::fs::read(dir.join("island.previous-1.json")).unwrap(),
            before
        );
        new.init_resource::<Messages<AppExit>>();
        new.world_mut().resource_mut::<LocalIsland>().path =
            dir.join("missing-parent").join("save.json");
        new.world_mut().write_message(AppExit::Success);
        save_on_exit(new.world_mut());
        assert!(new.world().resource::<Messages<AppExit>>().is_empty());
        assert!(
            new.world()
                .resource::<LocalIsland>()
                .message
                .starts_with(SAVE_FAILED)
        );
        drop(new);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join("island.world.lock")).unwrap();
        std::fs::remove_file(dir.join("island.previous-1.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn only_the_timers_own_save_is_routine() {
        let dir =
            std::env::temp_dir().join(format!("npc-local-island-routine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        let mut app = open(&path, None, None, false, |_| {}).unwrap();
        app.init_resource::<ButtonInput<KeyCode>>();
        let overdue = |app: &mut App| {
            app.world_mut().resource_mut::<LocalIsland>().last =
                Instant::now() - Duration::from_secs(61);
        };
        // Not yet due and no key: nothing is written or said.
        let before = save_notice(app.world()).unwrap();
        save_input(app.world_mut());
        settle_saves(app.world_mut());
        let quiet = save_notice(app.world()).unwrap();
        assert_eq!((quiet.text, quiet.routine), (before.text, false));

        // The minute timer: saved, and marked as nobody's request.
        run_ticks(app.world_mut(), 9);
        overdue(&mut app);
        save_input(app.world_mut());
        settle_saves(app.world_mut());
        let auto = save_notice(app.world()).unwrap();
        assert!(auto.routine && !auto.failed);
        assert!(auto.age < Duration::from_secs(5), "the timer was reset");
        assert_eq!(snapshot::read(&path).unwrap().tick, 9);

        // Ctrl+S (or F9) is a request, also when the timer is due as well.
        overdue(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F9);
        save_input(app.world_mut());
        settle_saves(app.world_mut());
        let asked = save_notice(app.world()).unwrap();
        assert!(!asked.routine && !asked.failed);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        // So is a save from the pause menu, which calls `save` directly.
        overdue(&mut app);
        save_input(app.world_mut());
        settle_saves(app.world_mut());
        assert!(save_notice(app.world()).unwrap().routine);
        save(app.world_mut()).unwrap();
        assert!(!save_notice(app.world()).unwrap().routine);

        // A failed autosave is a failure, never routine.
        overdue(&mut app);
        app.world_mut().resource_mut::<LocalIsland>().path =
            dir.join("missing-parent").join("save.json");
        save_input(app.world_mut());
        settle_saves(app.world_mut());
        let failed = save_notice(app.world()).unwrap();
        assert!(failed.failed && !failed.routine);
        assert!(failed.text.starts_with(SAVE_FAILED), "{}", failed.text);
        drop(app);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join("island.world.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    #[cfg(feature = "debug-tools")]
    fn remote_quit_acknowledges_a_saved_paused_island_and_refuses_failed_save() {
        let dir =
            std::env::temp_dir().join(format!("npc-local-island-quit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        let mut app = open(&path, None, None, false, |_| {}).unwrap();
        app.init_resource::<SimControl>();
        run_ticks(app.world_mut(), 23);
        let reply = app_quit(In(None), app.world_mut()).unwrap();
        assert_eq!(reply["saved"], true);
        assert_eq!(reply["pid"], std::process::id());
        assert_eq!(snapshot::read(&path).unwrap().tick, 23);
        assert!(app.world().resource::<SimControl>().paused);
        app.world_mut().resource_mut::<SimControl>().paused = false;
        app.world_mut()
            .remove_resource::<npc_framework::view_helpers::remote::QuitRequested>();
        app.world_mut().resource_mut::<LocalIsland>().path =
            dir.join("missing-parent").join("save.json");
        assert!(app_quit(In(None), app.world_mut()).is_err());
        assert!(!app.world().resource::<SimControl>().paused);
        assert!(
            !app.world()
                .contains_resource::<npc_framework::view_helpers::remote::QuitRequested>()
        );
        drop(app);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join("island.world.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
