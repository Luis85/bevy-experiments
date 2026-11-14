//! In-process title/play lifecycle. Construction and restore are staged in an
//! isolated headless world before the live renderer's world is replaced.
use super::*;
use island_content::village::{VillageState, Villager, Vitality, colony::AwaitingArrival};
use npc_framework::agent_io::{api::AgentIoState, events::AgentEvents};
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::prelude::*;
use std::{collections::BTreeSet, path::Path};

#[derive(Resource)]
pub(crate) struct SessionState {
    pub(crate) available_save: bool,
    pub(crate) issue: Option<super::Issue>,
    /// A New game failed and the island in play could not be put back: no
    /// file may be written, and the title asks for a restart. The details.
    pub(crate) restart: Option<String>,
    /// The autosave is newer than the save (the game did not close
    /// properly): its age and the save's, in seconds.
    pub(crate) autosave_newer: Option<(u64, u64)>,
    // Reserved aliases from immutable construction, before a saved world's
    // historical visitor/estate aliases are restored.
    reserved_names: BTreeSet<String>,
    /// The world a new game uses unless the player chooses a world file:
    /// the construction this window opened with. Kept apart from
    /// `LocalIsland.doc`, which follows the game in play (a game started on
    /// an exported world saves that world's construction).
    default_doc: ScenarioDoc,
    /// That world's name and rules, for the New-game world list.
    default_world: island_content::worlds::WorldPreset,
}

pub(crate) struct Summary {
    pub(crate) can_continue: bool,
    pub(crate) can_start: bool,
    pub(crate) active: bool,
    pub(crate) player: String,
    pub(crate) day: u64,
    pub(crate) season: String,
    pub(crate) issue: Option<super::Issue>,
    pub(crate) restart: Option<String>,
    /// A save file exists (it can be continued or not): New game keeps it.
    pub(crate) has_file: bool,
    pub(crate) autosave_newer: Option<(u64, u64)>,
}

pub(crate) fn summary(world: &World) -> Summary {
    let session = world.resource::<SessionState>();
    let local = world.resource::<LocalIsland>();
    let actor = world
        .resource::<SimIndex>()
        .get(world.resource::<VillageState>().played);
    let player = actor
        .and_then(|e| {
            world
                .get::<Villager>(e)
                .map(|v| v.name.clone())
                .or_else(|| world.get::<AwaitingArrival>(e).map(|v| v.name.clone()))
        })
        .unwrap_or_else(|| "Traveller".into());
    let (day, season, _) = island_content::village::calendar(world);
    Summary {
        can_continue: (session.available_save || local.active) && session.restart.is_none(),
        can_start: local._lock.is_some() && session.restart.is_none(),
        active: local.active,
        player,
        day: day + 1,
        season: format!("{season:?}"),
        issue: session.issue.clone(),
        restart: session.restart.clone(),
        has_file: local.path.exists(),
        autosave_newer: session.autosave_newer.filter(|_| !local.active),
    }
}

/// "Continue from the autosave": put the newer autosave in place of the
/// save the title opened, after the same checks, then continue as usual.
pub(crate) fn adopt_autosave(world: &mut World) -> Result<(), String> {
    let auto = super::autosave_path(&world.resource::<LocalIsland>().path);
    let saved = snapshot::read(&auto)?;
    use island_content::village::island_save::{self, IslandSaveCheck};
    if island_save::check_island_save(world, &saved) != IslandSaveCheck::Current {
        return Err("The autosave cannot be opened. Continue from your save instead.".into());
    }
    let (doc, seed) = saved_construction(&saved, None).map_err(|error| error.to_string())?;
    let before = snapshot::save(world)?;
    let old_seed = world.resource::<WorldSeed>().0;
    let scenario = world.resource::<ScenarioInfo>().clone();
    if let Err(error) = replace_world(world, &saved, seed, scenario.clone()) {
        warn!("the autosave could not be put in place: {error}");
        replace_world(world, &before, old_seed, scenario)?;
        return Err("The autosave cannot be opened. Continue from your save instead.".into());
    }
    {
        let mut local = world.resource_mut::<LocalIsland>();
        local.doc = doc;
        local.seed = seed;
    }
    let mut session = world.resource_mut::<SessionState>();
    session.autosave_newer = None;
    session.available_save = true;
    Ok(())
}

/// What the New screen says when a new island did not start and the one in
/// play was put back as it was.
pub(crate) const NEW_GAME_FAILED: &str =
    "The new island could not start. Your island is as it was.";

/// A New game failed after the island in play was taken down: put it back,
/// or, when that fails too, stop every save and ask for a restart.
fn roll_back(
    world: &mut World,
    error: String,
    before: &snapshot::WorldSnapshot,
    seed: u64,
    scenario: ScenarioInfo,
) -> String {
    match replace_world(world, before, seed, scenario) {
        Ok(()) => {
            warn!("new game rolled back: {error}");
            NEW_GAME_FAILED.to_owned()
        }
        Err(restore) => {
            error!("new game failed ({error}) and the island could not be put back ({restore})");
            world.resource_mut::<LocalIsland>().active = false;
            world.resource_mut::<SessionState>().restart = Some(error.clone());
            NEW_GAME_FAILED.to_owned()
        }
    }
}

/// Opening the title screen never advances or rewrites a saved game.
pub(crate) fn open(
    path: &Path,
    seed_override: Option<u64>,
    rules: Option<&Path>,
    host: impl FnOnce(&mut App),
) -> Result<App, String> {
    let (lock, mut issue) = match lock_save(path) {
        Ok(lock) => (Some(lock), None),
        Err(error) => (None, Some(super::Issue::Unopened(error))),
    };
    // The autosave beside the save: when there is no save it is the save;
    // when it is newer the title offers it (the game did not close
    // properly); when it is older it is stale and goes.
    let auto = super::autosave_path(path);
    let modified = |file: &Path| {
        std::fs::metadata(file)
            .and_then(|meta| meta.modified())
            .ok()
    };
    let mut autosave_newer = None;
    let source = if lock.is_some() && !path.exists() && auto.exists() {
        auto.clone()
    } else {
        if lock.is_some()
            && auto.exists()
            && let (Some(own), Some(newer)) = (modified(path), modified(&auto))
        {
            if newer > own {
                let age = |at: std::time::SystemTime| at.elapsed().map_or(0, |age| age.as_secs());
                autosave_newer = Some((age(newer), age(own)));
            } else if let Err(error) = std::fs::remove_file(&auto) {
                warn!("could not remove a stale autosave: {error}");
            }
        }
        path.to_path_buf()
    };
    let mut resumed = None;
    if lock.is_some() && source.exists() {
        // Continue checks first, on a fresh world of this build: a save the
        // simulation refuses is named and kept untouched (New game backs it
        // up before writing; autosave writes nothing while no island is in
        // play).
        let prepared = (|| -> Result<_, super::RestoreError> {
            let saved = snapshot::read(&source)?;
            let (doc, seed) = saved_construction(&saved, seed_override)?;
            let mut staged = island_content::scenario::load_with(&doc, seed, |_| {})?;
            attach_controllers(&doc, &mut staged)?;
            restore_validated(staged.app.world_mut(), &saved)?;
            Ok((doc, seed, saved))
        })();
        match prepared {
            Ok(value) => resumed = Some(value),
            Err(super::RestoreError::Refused(refused)) => issue = Some(refused),
            Err(super::RestoreError::Failed(error)) => {
                warn!("the saved island could not be opened: {error}");
                issue = Some(super::Issue::Unopened(error));
            }
        }
    }
    let (doc, seed) = if let Some((doc, seed, _)) = &resumed {
        (doc.clone(), *seed)
    } else {
        let doc = match fresh_construction(rules) {
            Ok(doc) => doc,
            Err(error) => {
                let shipped = shipped_preset(rules);
                warn!(
                    "the island rules could not load ({error}); using {}",
                    shipped.name
                );
                issue = Some(super::Issue::Rules(format!("{error} ({})", shipped.name)));
                construction_from_preset(shipped)?
            }
        };
        (doc, seed_override.unwrap_or(7))
    };
    let mut loaded = island_content::scenario::load_with(&doc, seed, host)?;
    attach_controllers(&doc, &mut loaded)?;
    let mut app = loaded.app;
    let played = app.world().resource::<VillageState>().played;
    let reserved_names = app
        .world()
        .resource::<island_content::IslandIds>()
        .names
        .iter()
        .filter(|(_, id)| **id != played)
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    if let Some((_, _, saved)) = &resumed {
        restore_validated(app.world_mut(), saved).map_err(|error| error.to_string())?;
    }
    let default_world = app
        .world()
        .resource::<island_content::world_runtime::ActiveWorld>()
        .0
        .clone();
    let default_doc = doc.clone();
    install(&mut app, path, doc, seed, lock, false);
    app.insert_resource(SessionState {
        available_save: resumed.is_some(),
        issue,
        restart: None,
        autosave_newer,
        reserved_names,
        default_doc,
        default_world,
    });
    app.world_mut().get_resource_or_init::<SimControl>().paused = true;
    Ok(app)
}

pub(crate) fn name_problem(world: &World, value: &str) -> Option<String> {
    let name = value.trim();
    if name.is_empty() {
        return Some("Choose a name for your character.".into());
    }
    if name.chars().count() > 32 || name.chars().any(char::is_control) {
        return Some("Use at most 32 characters, without control characters.".into());
    }
    if world
        .resource::<SessionState>()
        .reserved_names
        .contains(&name.to_ascii_lowercase())
        || ["village_", "patch_", "wildlife_", "homestead_"]
            .iter()
            .any(|prefix| name.to_ascii_lowercase().starts_with(prefix))
    {
        return Some(
            "That name belongs to someone or somewhere on the island. Choose another.".into(),
        );
    }
    None
}

/// The world a new game uses when none is chosen.
pub(crate) fn default_world(world: &World) -> &island_content::worlds::WorldPreset {
    &world.resource::<SessionState>().default_world
}

fn named_construction(world: &World, base: ScenarioDoc, name: &str) -> Result<ScenarioDoc, String> {
    if let Some(error) = name_problem(world, name) {
        return Err(error);
    }
    let mut doc = base;
    let actor = doc
        .actors
        .first_mut()
        .ok_or("The island has no visiting character template")?;
    actor.extra.insert("name".into(), name.trim().into());
    let context = doc
        .world
        .get_mut("construction")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("This island is missing its immutable character construction")?;
    context.insert("actor".into(), name.trim().into());
    Ok(doc)
}

fn replace_world(
    world: &mut World,
    saved: &snapshot::WorldSnapshot,
    seed: u64,
    scenario: ScenarioInfo,
) -> Result<(), String> {
    // A new epoch invalidates host continuations; saved handle counters are then
    // restored by the normal snapshot decoder. No sim tick is manufactured.
    world.insert_resource(AgentIoState::default());
    world.insert_resource(AgentEvents::default());
    world.insert_resource(npc_framework::kernel::driver::SimTime::default());
    world.insert_resource(WorldSeed(seed));
    world.insert_resource(scenario);
    let report =
        snapshot::load(world, saved, snapshot::LoadMode::Strict).map_err(|e| e.to_string())?;
    if !report.defaulted.is_empty() || !report.skipped.is_empty() {
        return Err("World replacement was incomplete".into());
    }
    validate_loaded(world)
}

/// New game installs a checked world using the current immutable rules. The old
/// file is backed up; any write failure restores the previous live state.
#[cfg(test)]
pub(crate) fn new_game(world: &mut World, name: &str) -> Result<(), String> {
    new_game_on(world, name, None)
}

/// New game on a chosen world file (`None`: the world this window opened
/// with). The file is read once, here: the save keeps its whole
/// construction, so the game never depends on the file again.
pub(crate) fn new_game_on(
    world: &mut World,
    name: &str,
    world_file: Option<&Path>,
) -> Result<(), String> {
    if world.resource::<LocalIsland>()._lock.is_none() {
        return Err("This save is open in another window.".into());
    }
    let base = match world_file {
        Some(path) => super::world_file_construction(path).map_err(|error| {
            format!(
                "{} cannot be used: {error}",
                path.file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
            )
        })?,
        None => world.resource::<SessionState>().default_doc.clone(),
    };
    let doc = named_construction(world, base, name)?;
    let seed = world.resource::<LocalIsland>().seed;
    let mut staged = island_content::scenario::load_with(&doc, seed, |_| {})?;
    attach_controllers(&doc, &mut staged)?;
    validate_loaded(staged.app.world())?;
    // A chosen world's own residents and places keep their names too.
    if world_file.is_some() {
        let played = staged.app.world().resource::<VillageState>().played;
        let taken = staged
            .app
            .world()
            .resource::<island_content::IslandIds>()
            .names
            .iter()
            .any(|(known, id)| *id != played && known.eq_ignore_ascii_case(name.trim()));
        if taken {
            return Err(
                "That name belongs to someone or somewhere on the island. Choose another.".into(),
            );
        }
    }
    let prepared = snapshot::save(staged.app.world_mut())?;
    snapshot::check(world, &prepared)
        .map_err(|error| format!("These rules need a new host: {error}"))?;
    let scenario = staged.app.world().resource::<ScenarioInfo>().clone();
    // The world's rules as a runtime resource (not hashed, not saved): they
    // follow the construction, so a chosen world's name and rules show.
    use island_content::world_runtime::ActiveWorld as Rules;
    let rules = staged
        .app
        .world()
        .get_resource::<Rules>()
        .map(|r| r.0.clone());
    let old_rules = world.get_resource::<Rules>().map(|r| r.0.clone());
    let before = snapshot::save(world)?;
    let old_scenario = world.resource::<ScenarioInfo>().clone();
    let old_seed = world.resource::<WorldSeed>().0;
    let old_doc = world.resource::<LocalIsland>().doc.clone();
    let was_active = world.resource::<LocalIsland>().active;
    let path = world.resource::<LocalIsland>().path.clone();
    if path.exists() {
        let older = world
            .resource::<SessionState>()
            .issue
            .as_ref()
            .is_some_and(super::Issue::keeps_forever);
        backup(&path, older)?;
    }
    if let Err(error) = replace_world(world, &prepared, seed, scenario) {
        return Err(roll_back(world, error, &before, old_seed, old_scenario));
    }
    {
        let mut local = world.resource_mut::<LocalIsland>();
        local.doc = doc;
        local.active = true;
    }
    if let Some(rules) = rules {
        world.insert_resource(Rules(rules));
    }
    if let Err(error) = save(world) {
        {
            let mut local = world.resource_mut::<LocalIsland>();
            local.doc = old_doc;
            local.active = was_active;
        }
        if let Some(rules) = old_rules {
            world.insert_resource(Rules(rules));
        }
        return Err(roll_back(world, error, &before, old_seed, old_scenario));
    }
    super::prune_backups(&path);
    let mut session = world.resource_mut::<SessionState>();
    session.available_save = true;
    session.issue = None;
    Ok(())
}

pub(crate) fn continue_game(world: &mut World) -> Result<(), String> {
    if !summary(world).can_continue {
        return Err("There is no compatible saved island to continue.".into());
    }
    if world.resource::<LocalIsland>()._lock.is_none() {
        return Err("This save is open in another window.".into());
    }
    world.resource_mut::<LocalIsland>().active = true;
    Ok(())
}

pub(crate) fn living_player(world: &World) -> bool {
    let entity = world
        .resource::<SimIndex>()
        .get(world.resource::<VillageState>().played);
    entity.is_some_and(|entity| {
        world.get::<AwaitingArrival>(entity).is_some()
            || world
                .get::<Vitality>(entity)
                .is_some_and(|vitality| vitality.died_at.is_none())
    })
}

pub(crate) fn save_for_title(world: &mut World) -> Result<(), String> {
    save(world)?;
    if world.resource::<LocalIsland>().active {
        world.resource_mut::<SessionState>().available_save = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::kernel::hash::state_hash;

    fn path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("npc-frontend-{label}-{}", std::process::id()))
            .join("island.json")
    }
    fn cleanup(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("world.lock"));
        let _ = std::fs::remove_file(path.with_extension("previous-1.json"));
        let _ = std::fs::remove_dir(path.parent().unwrap());
    }

    #[test]
    fn title_has_no_save_or_life_and_new_game_uses_the_named_immutable_template() {
        let path = path("new");
        let mut app = open(&path, None, None, |_| {}).unwrap();
        assert!(!path.exists());
        assert!(app.world().resource::<SimControl>().paused);
        assert!(!summary(app.world()).can_continue);
        assert!(name_problem(app.world(), "Ada").is_some());
        let before = state_hash(app.world_mut());
        assert!(new_game(app.world_mut(), "Ada").is_err());
        assert_eq!(state_hash(app.world_mut()), before);
        new_game(app.world_mut(), "Zephyrine").unwrap();
        assert_eq!(summary(app.world()).player, "Zephyrine");
        assert_eq!(app.world().resource::<SimClock>().tick, 0);
        let saved = snapshot::read(&path).unwrap();
        assert_eq!(
            saved.header["scenario"]["world"]["construction"]["actor"],
            "Zephyrine"
        );
        drop(app);
        let resumed = open(&path, None, None, |_| {}).unwrap();
        assert!(summary(resumed.world()).can_continue);
        assert_eq!(summary(resumed.world()).player, "Zephyrine");
        drop(resumed);
        cleanup(&path);
    }

    #[test]
    fn new_name_validation_uses_fresh_aliases_instead_of_saved_history() {
        let path = path("historical-names");
        let mut app = open(&path, None, None, |_| {}).unwrap();
        new_game(app.world_mut(), "Zephyrine").unwrap();
        let played = app.world().resource::<VillageState>().played;
        // Host lookup fixture: saved aliases can describe historical actors.
        // Fresh-game validation must not inherit those aliases, or forget a
        // reserved construction name if a later lookup was renamed.
        app.world_mut()
            .resource_mut::<island_content::IslandIds>()
            .names
            .insert("Former visitor".into(), SimId(played.0 + 1));
        app.world_mut()
            .resource_mut::<island_content::IslandIds>()
            .names
            .remove("Ada");
        assert!(name_problem(app.world(), "Former visitor").is_none());
        assert!(name_problem(app.world(), "ada").is_some());
        new_game(app.world_mut(), "Former visitor").unwrap();
        assert_eq!(summary(app.world()).player, "Former visitor");
        drop(app);
        cleanup(&path);
    }

    #[test]
    fn failed_new_game_write_restores_previous_live_world_and_original_file() {
        let path = path("write-failure");
        let mut app = open(&path, None, None, |_| {}).unwrap();
        new_game(app.world_mut(), "Zephyrine").unwrap();
        run_ticks(app.world_mut(), 29);
        save(app.world_mut()).unwrap();
        let before = state_hash(app.world_mut());
        let bytes = std::fs::read(&path).unwrap();
        app.world_mut().resource_mut::<LocalIsland>().path = path
            .parent()
            .unwrap()
            .join("missing-parent")
            .join("island.json");
        assert!(
            new_game(app.world_mut(), "Ysolde")
                .unwrap_err()
                .contains("could not start")
        );
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(summary(app.world()).player, "Zephyrine");
        assert!(summary(app.world()).active);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        app.world_mut().resource_mut::<LocalIsland>().path = path.clone();
        drop(app);
        cleanup(&path);
    }

    fn profile(world: &World) -> String {
        world
            .resource::<island_content::world_runtime::ActiveWorld>()
            .0
            .name
            .clone()
    }

    /// A living-world game in which a logs patch was placed by a recorded
    /// command, exported to `<dir>/rules/<slug>-shaped-1.json`. Returns the
    /// file and the patch's position.
    fn shaped_world(app: &mut App, dir: &Path) -> (PathBuf, (i32, i32)) {
        use island_content::village::frontier::{PlaceResourcePatch, export_spec};
        new_game(app.world_mut(), "Tamsin").unwrap();
        let spec = export_spec(app.world()).unwrap();
        let anchor = spec.settlements[0].center;
        let place = (1..40)
            .flat_map(|ring| {
                [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, -1)]
                    .map(|(dx, dz)| (anchor.x + dx * ring * 3000, anchor.z + dz * ring * 3000))
            })
            .map(|(x, z)| PlaceResourcePatch {
                good: "log".into(),
                x,
                z,
                richness: 150,
            })
            .find(|place| place.check(app.world()).is_ok())
            .expect("somewhere near the first settlement takes a patch");
        let at = (place.x, place.z);
        npc_framework::kernel::commands::submit(app.world_mut(), None, SimCommand::content(&place))
            .unwrap();
        run_ticks(app.world_mut(), 2);
        assert!(
            export_spec(app.world())
                .unwrap()
                .resource_patches
                .iter()
                .any(|p| p.good == "log" && (p.x, p.z) == at),
            "the placed patch is part of the world"
        );
        app.insert_resource(crate::frontend::ExportFolder(dir.join("rules")));
        let file = crate::frontend::export_world(app.world()).unwrap();
        (file, at)
    }

    #[test]
    fn a_chosen_exported_world_reaches_the_new_game_and_its_save() {
        use island_content::village::frontier::export_spec;
        let path = path("chosen-world");
        let dir = path.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_dir_all(&dir);
        let rules = dir.join("rules").join("living_world.json");
        let mut app = open(&path, None, Some(&rules), |_| {}).unwrap();
        let (file, at) = shaped_world(&mut app, &dir);
        // The default world is still the shipped one.
        assert!(!default_world(app.world()).name.ends_with("(shaped)"));
        new_game_on(app.world_mut(), "Quillon", Some(&file)).unwrap();
        assert_eq!(summary(app.world()).player, "Quillon");
        assert_eq!(app.world().resource::<SimClock>().tick, 0);
        assert!(profile(app.world()).ends_with("(shaped)"));
        let has_patch = |world: &World| {
            export_spec(world)
                .unwrap()
                .resource_patches
                .iter()
                .any(|p| p.good == "log" && (p.x, p.z) == at)
        };
        assert!(has_patch(app.world()), "the new game has the placed patch");
        // The save holds the whole construction: the file can go.
        std::fs::remove_file(&file).unwrap();
        drop(app);
        let resumed = open(&path, None, Some(&rules), |_| {}).unwrap();
        assert!(
            summary(resumed.world()).can_continue,
            "{:?}",
            summary(resumed.world()).issue
        );
        assert_eq!(summary(resumed.world()).player, "Quillon");
        assert!(
            has_patch(resumed.world()),
            "Continue keeps the shaped world"
        );
        drop(resumed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unusable_world_file_is_refused_and_changes_nothing() {
        let path = path("bad-world");
        let dir = path.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_dir_all(&dir);
        let rules = dir.join("rules").join("living_world.json");
        let mut app = open(&path, None, Some(&rules), |_| {}).unwrap();
        new_game(app.world_mut(), "Tamsin").unwrap();
        let before = state_hash(app.world_mut());
        let bad = dir.join("rules").join("broken-shaped-1.json");
        std::fs::write(&bad, "{ not a world").unwrap();
        let error = new_game_on(app.world_mut(), "Quillon", Some(&bad)).unwrap_err();
        assert!(
            error.starts_with("broken-shaped-1.json cannot be used"),
            "{error}"
        );
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(summary(app.world()).player, "Tamsin");
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_on_a_chosen_world_restores_the_previous_game_and_rules() {
        let path = path("chosen-write-failure");
        let dir = path.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_dir_all(&dir);
        let rules = dir.join("rules").join("living_world.json");
        let mut app = open(&path, None, Some(&rules), |_| {}).unwrap();
        let (file, _) = shaped_world(&mut app, &dir);
        save(app.world_mut()).unwrap();
        let before = state_hash(app.world_mut());
        let rules_before = profile(app.world());
        let bytes = std::fs::read(&path).unwrap();
        app.world_mut().resource_mut::<LocalIsland>().path =
            dir.join("missing-parent").join("island.json");
        let error = new_game_on(app.world_mut(), "Quillon", Some(&file)).unwrap_err();
        assert!(error.contains("could not start"), "{error}");
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(summary(app.world()).player, "Tamsin");
        assert_eq!(
            profile(app.world()),
            rules_before,
            "the rules roll back too"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        app.world_mut().resource_mut::<LocalIsland>().path = path.clone();
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_save_is_named_kept_and_backed_up_by_new_game() {
        use super::super::Issue;
        // A save this build cannot continue: made before the island's stamp
        // and by different content (another content hash).
        let path = path("older");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut app = open(&path, None, None, |_| {}).unwrap();
        new_game(app.world_mut(), "Zephyrine").unwrap();
        drop(app);
        let mut saved = snapshot::read(&path).unwrap();
        // Stamped with an earlier island version (a save without a stamp
        // is unreadable now, not older: the sim's S1 rule).
        let version = island_content::village::island_save::ISLAND_SAVE_VERSION - 1;
        saved.header.insert(
            island_content::village::island_save::HEADER_VERSION.into(),
            version.into(),
        );
        snapshot::write_atomic(&path, &saved).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut app = open(&path, None, None, |_| {}).unwrap();
        assert!(!summary(app.world()).can_continue);
        assert_eq!(summary(app.world()).issue, Some(Issue::Older));
        // Nothing is in play: an autosave writes nothing over it.
        save(app.world_mut()).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "the file is untouched"
        );
        new_game(app.world_mut(), "Ysolde").unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("kept-1.json")).unwrap(),
            bytes,
            "New game keeps the older island as a backup of its own kind"
        );
        drop(app);
        cleanup(&path);
    }

    #[test]
    fn every_island_save_carries_the_islands_stamp() {
        let path = path("stamped");
        let mut app = open(&path, None, None, |_| {}).unwrap();
        new_game(app.world_mut(), "Zephyrine").unwrap();
        drop(app);
        let saved = snapshot::read(&path).unwrap();
        assert!(
            island_content::village::island_save::saved_version(&saved).is_some(),
            "{:?}",
            saved.header.keys().collect::<Vec<_>>()
        );
        cleanup(&path);
    }

    #[test]
    fn title_preserves_bad_save_until_explicit_new_and_keeps_a_backup() {
        let path = path("bad");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"unreadable old save").unwrap();
        let mut app = open(&path, None, None, |_| {}).unwrap();
        assert!(!summary(app.world()).can_continue);
        assert!(summary(app.world()).issue.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), b"unreadable old save");
        new_game(app.world_mut(), "Ysolde").unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("kept-1.json")).unwrap(),
            b"unreadable old save",
            "a refused save, unreadable too, is kept under the neutral name"
        );
        drop(app);
        cleanup(&path);
    }
}
