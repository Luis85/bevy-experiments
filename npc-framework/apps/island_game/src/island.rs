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
        backup(path)?;
    }
    save(app.world_mut())?;
    Ok(app)
}

/// The title's words for an island this build cannot continue: an older
/// one, as the coordinator's brief words it; any other refusal is the
/// simulation's own sentence, with the same promise about the file.
pub(crate) const OLDER_SAVE: &str = "This island was saved by an older version and cannot be opened. It is kept as a backup; start a new island.";

/// Why a save does not continue, for the title: the simulation's check in
/// the player's words. The file is always kept.
pub(crate) fn refusal(check: &island_content::village::island_save::IslandSaveCheck) -> String {
    use island_content::village::island_save::IslandSaveCheck;
    match check {
        IslandSaveCheck::OlderIsland { .. } => OLDER_SAVE.to_owned(),
        other => format!("{} It is kept as a backup.", other.sentence()),
    }
}

/// Why a saved island could not be restored.
#[derive(Debug)]
pub(crate) enum RestoreError {
    /// The simulation refuses this save (older, newer, unreadable): say its
    /// sentence and keep the file.
    Refused(String),
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
            Self::Refused(text) | Self::Failed(text) => f.write_str(text),
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
    Ok((doc, seed))
}

fn fresh_construction(rules: Option<&Path>) -> Result<ScenarioDoc, String> {
    let preset = if let Some(path) = rules {
        if !path.exists()
            && path
                .file_stem()
                .is_some_and(|name| name == "island_colony" || name == "living_world")
        {
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
        island_content::worlds::builtin_preset("island_colony").expect("shipped island rules")
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

fn shipped_preset(rules: Option<&Path>) -> island_content::worlds::WorldPreset {
    let key = if rules
        .and_then(Path::file_stem)
        .is_some_and(|name| name == "living_world")
    {
        "living_world"
    } else {
        "island_colony"
    };
    island_content::worlds::builtin_preset(key).expect("shipped island profile")
}

fn construction_from_preset(
    preset: island_content::worlds::WorldPreset,
) -> Result<ScenarioDoc, String> {
    let mut doc = island_content::scenario::scenario_doc("island_colony")?;
    island_content::worlds::validate(&preset).map_err(|errors| errors.join("; "))?;
    if !preset.colony {
        return Err("Local island rules must enable the colony simulation.".into());
    }
    if let Some(region) = &preset.living_world {
        doc.actors
            .first_mut()
            .ok_or("Island construction needs an arrival actor")?
            .pos = Some((region.actor_start.x, region.actor_start.z));
    }
    island_content::explorer::context::ExplorerContext::for_world(
        "human",
        "human",
        &island_content::actors::human_preset(),
        if preset.living_world.is_some() {
            "living_world"
        } else {
            "island_colony"
        },
        &preset,
    )
    .write_to(&mut doc)?;
    Ok(doc)
}

/// Restore `saved` into a fresh world of this build, after the
/// simulation's own check: only a current save is restored (strictly, then
/// validated); anything else is refused with its sentence and kept.
fn restore_validated(
    world: &mut World,
    saved: &snapshot::WorldSnapshot,
) -> Result<(), RestoreError> {
    use island_content::village::island_save::{self, IslandSaveCheck};
    match island_save::check_island_save(world, saved) {
        IslandSaveCheck::Current => Ok(restore_current(world, saved)?),
        refused => Err(RestoreError::Refused(refusal(&refused))),
    }
}

fn restore_current(world: &mut World, saved: &snapshot::WorldSnapshot) -> Result<(), String> {
    let expected = snapshot::save(world)?;
    island_content::village::integrity::validate_snapshot_structure(&expected, saved)
        .map_err(|reason| format!("{reason}; the original island save has been preserved."))?;
    let report = snapshot::load(world, saved, snapshot::LoadMode::Strict).map_err(|error| {
        format!("Island save could not load: {error}. The file has been preserved.")
    })?;
    if !report.defaulted.is_empty() || !report.skipped.is_empty() {
        return Err("Island save is incomplete; the original file has been preserved.".into());
    }
    validate_loaded(world)
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
        message: "Island saves automatically | Ctrl+S save now".into(),
        failed: false,
        routine: false,
        _lock: lock,
    })
    .add_systems(Startup, spawn_label)
    .add_systems(Update, (save_input, close_requested, refresh_label).chain())
    .add_systems(Last, save_on_exit.in_set(bevy::app::OnAppExitSystems));
}

fn backup(path: &Path) -> Result<PathBuf, String> {
    for number in 1..=10000 {
        let next = path.with_extension(format!("previous-{number}.json"));
        match File::options().write(true).create_new(true).open(&next) {
            Ok(mut destination) => {
                let mut source = File::open(path).map_err(|e| e.to_string())?;
                std::io::copy(&mut source, &mut destination)
                    .map_err(|e| format!("Could not back up the current island: {e}"))?;
                destination.sync_all().map_err(|e| e.to_string())?;
                return Ok(next);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not create an island backup: {error}")),
        }
    }
    Err("Island backup limit reached; choose another --island-save path.".into())
}

fn validate_loaded(world: &World) -> Result<(), String> {
    island_content::village::integrity::validate_colony(world)
        .map_err(|reason| format!("{reason}; the original island save has been preserved."))
}

pub(crate) fn save(world: &mut World) -> Result<(), String> {
    let local = world.resource::<LocalIsland>();
    if !local.active {
        return Ok(());
    }
    if local._lock.is_none() {
        return Err("This session does not own the save file".into());
    }
    // Stamped with the island's save version and content: a later build
    // tells an older save from a damaged one.
    let mut snapshot = island_content::village::island_save::save_island(world)?;
    let local = world.resource::<LocalIsland>();
    snapshot.header.insert("local_island".into(), 1.into());
    snapshot.header.insert(
        "scenario".into(),
        serde_json::to_value(&local.doc).map_err(|e| e.to_string())?,
    );
    snapshot.header.insert("seed".into(), local.seed.into());
    snapshot::write_atomic(&local.path, &snapshot)
        .map_err(|e| format!("Could not save island: {e}"))?;
    let frontend = world.contains_resource::<crate::frontend::FrontendState>();
    let mut local = world.resource_mut::<LocalIsland>();
    local.last = Instant::now();
    local.failed = false;
    // Whoever asked for the save; the timer marks its own afterwards.
    local.routine = false;
    local.message = if frontend {
        "Island saved · Ctrl+S"
    } else {
        "Island saved | Ctrl+S to save again"
    }
    .into();
    Ok(())
}
fn report_save(world: &mut World) -> bool {
    if let Err(error) = save(world) {
        error!("{error}");
        let mut local = world.resource_mut::<LocalIsland>();
        local.last = Instant::now();
        local.failed = true;
        local.message =
            format!("{error} | Free disk space or restore access, then Ctrl+S to retry");
        false
    } else {
        true
    }
}
fn save_input(world: &mut World) {
    if world
        .get_resource::<island_view::FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let due = world.resource::<LocalIsland>().last.elapsed() >= Duration::from_secs(60);
    let key = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| {
            keys.just_pressed(KeyCode::F9)
                || (keys.just_pressed(KeyCode::KeyS)
                    && [KeyCode::ControlLeft, KeyCode::ControlRight]
                        .into_iter()
                        .any(|key| keys.pressed(key) || keys.just_pressed(key)))
        });
    if key {
        report_save(world);
    } else if due && report_save(world) {
        // A failed autosave is still reported; a completed one is routine.
        world.resource_mut::<LocalIsland>().routine = true;
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
        if report_save(world) {
            world.write_message(AppExit::Success);
        }
    }
}
pub(crate) fn save_on_exit(world: &mut World) {
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
    for (mut label, mut node) in &mut labels {
        label.0.clone_from(&local.message);
        node.display = if gate.as_ref().is_some_and(|gate| gate.0) {
            Display::None
        } else {
            Display::Flex
        };
    }
}

#[cfg(test)]
mod tests {
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
        assert!(island_content::village::frontier::enabled(fallback.world()));
        assert!(
            session::summary(fallback.world())
                .issue
                .unwrap()
                .contains("The Living Island")
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
    fn incomplete_or_inconsistent_island_saves_are_preserved() {
        let dir =
            std::env::temp_dir().join(format!("npc-local-island-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("island.json");
        drop(open(&path, Some(7), None, false, |_| {}).unwrap());
        let original = snapshot::read(&path).unwrap();
        for invalid in [
            {
                let mut value = original.clone();
                value.entities.clear();
                value
            },
            {
                let mut value = original.clone();
                value
                    .entities
                    .retain(|entity| !entity.components.contains_key("island_awaiting_arrival_v1"));
                value
            },
            {
                let mut value = original.clone();
                value
                    .entities
                    .iter_mut()
                    .find(|entity| entity.components.contains_key("island_villager"))
                    .unwrap()
                    .components
                    .remove("island_vitality");
                value
            },
            {
                let mut value = original.clone();
                value.resources.remove("island_colony_terrain_v1");
                value
            },
            {
                let mut value = original.clone();
                value.resources.get_mut("island_colony_terrain_v1").unwrap()["cells"] =
                    serde_json::json!([]);
                value
            },
            {
                let mut value = original.clone();
                value.resources.get_mut("island_colony_terrain_v1").unwrap()["cells"][0]["surface"] =
                    "grass".into();
                value
            },
        ] {
            snapshot::write_atomic(&path, &invalid).unwrap();
            let before = std::fs::read(&path).unwrap();
            assert!(open(&path, None, None, false, |_| {}).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
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
        let mut preset = island_content::worlds::builtin_preset("island_colony").unwrap();
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
                .contains("Could not save")
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
        let quiet = save_notice(app.world()).unwrap();
        assert_eq!((quiet.text, quiet.routine), (before.text, false));

        // The minute timer: saved, and marked as nobody's request.
        run_ticks(app.world_mut(), 9);
        overdue(&mut app);
        save_input(app.world_mut());
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
        let asked = save_notice(app.world()).unwrap();
        assert!(!asked.routine && !asked.failed);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        // So is a save from the pause menu, which calls `save` directly.
        overdue(&mut app);
        save_input(app.world_mut());
        assert!(save_notice(app.world()).unwrap().routine);
        save(app.world_mut()).unwrap();
        assert!(!save_notice(app.world()).unwrap().routine);

        // A failed autosave is a failure, never routine.
        overdue(&mut app);
        app.world_mut().resource_mut::<LocalIsland>().path =
            dir.join("missing-parent").join("save.json");
        save_input(app.world_mut());
        let failed = save_notice(app.world()).unwrap();
        assert!(failed.failed && !failed.routine);
        assert!(failed.text.contains("Could not save"), "{}", failed.text);
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
