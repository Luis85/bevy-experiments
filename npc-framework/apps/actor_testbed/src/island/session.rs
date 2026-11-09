//! In-process title/play lifecycle. Construction and restore are staged in an
//! isolated headless world before the live renderer's world is replaced.
use super::*;
use npc_framework::agent_io::{api::AgentIoState, events::AgentEvents};
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::prelude::*;
use std::{collections::BTreeSet, path::Path};
use testbed_content::village::{VillageState, Villager, Vitality, colony::AwaitingArrival};

#[derive(Resource)]
pub(crate) struct SessionState {
    pub(crate) available_save: bool,
    pub(crate) issue: Option<String>,
    // Reserved aliases from immutable construction, before a saved world's
    // historical visitor/estate aliases are restored.
    reserved_names: BTreeSet<String>,
}

pub(crate) struct Summary {
    pub(crate) can_continue: bool,
    pub(crate) can_start: bool,
    pub(crate) active: bool,
    pub(crate) player: String,
    pub(crate) day: u64,
    pub(crate) season: String,
    pub(crate) issue: Option<String>,
    pub(crate) profile: String,
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
    let (day, season, _) = testbed_content::village::calendar(world);
    Summary {
        can_continue: session.available_save || local.active,
        can_start: local._lock.is_some(),
        active: local.active,
        player,
        day: day + 1,
        season: format!("{season:?}"),
        issue: session.issue.clone(),
        profile: world
            .resource::<testbed_content::world_runtime::ActiveWorld>()
            .0
            .name
            .clone(),
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
        Err(error) => (None, Some(error)),
    };
    let mut resumed = None;
    if lock.is_some() && path.exists() {
        let prepared = snapshot::read(path).and_then(|saved| {
            let (doc, seed) = saved_construction(&saved, seed_override)?;
            let mut staged = testbed_content::scenario::load_with(&doc, seed, |_| {})?;
            attach_controllers(&doc, &mut staged)?;
            restore_validated(staged.app.world_mut(), &saved)?;
            Ok((doc, seed, saved))
        });
        match prepared {
            Ok(value) => resumed = Some(value),
            Err(error) => {
                issue = Some(format!(
                    "Continue is unavailable: {error} A new game keeps a backup of this save."
                ))
            }
        }
    }
    let (doc, seed) = if let Some((doc, seed, _)) = &resumed {
        (doc.clone(), *seed)
    } else {
        let doc = match fresh_construction(rules) {
            Ok(doc) => doc,
            Err(error) => {
                issue = Some(format!(
                    "The edited world rules could not load: {error} New game will use the shipped island rules; your files are preserved."
                ));
                fresh_construction(None)?
            }
        };
        (doc, seed_override.unwrap_or(7))
    };
    let mut loaded = testbed_content::scenario::load_with(&doc, seed, host)?;
    attach_controllers(&doc, &mut loaded)?;
    let mut app = loaded.app;
    let played = app.world().resource::<VillageState>().played;
    let reserved_names = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .names
        .iter()
        .filter(|(_, id)| **id != played)
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    if let Some((_, _, saved)) = &resumed {
        restore_validated(app.world_mut(), saved)?;
    }
    install(&mut app, path, doc, seed, lock, false);
    app.insert_resource(SessionState {
        available_save: resumed.is_some(),
        issue,
        reserved_names,
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

fn named_construction(world: &World, name: &str) -> Result<ScenarioDoc, String> {
    if let Some(error) = name_problem(world, name) {
        return Err(error);
    }
    let mut doc = world.resource::<LocalIsland>().doc.clone();
    let actor = doc
        .actors
        .first_mut()
        .ok_or("The island has no visiting character template")?;
    actor.extra.insert("name".into(), name.trim().into());
    let context = doc
        .world
        .get_mut("explorer_context")
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
pub(crate) fn new_game(world: &mut World, name: &str) -> Result<(), String> {
    if world.resource::<LocalIsland>()._lock.is_none() {
        return Err("This save is open in another window.".into());
    }
    let doc = named_construction(world, name)?;
    let seed = world.resource::<LocalIsland>().seed;
    let mut staged = testbed_content::scenario::load_with(&doc, seed, |_| {})?;
    attach_controllers(&doc, &mut staged)?;
    validate_loaded(staged.app.world())?;
    let prepared = snapshot::save(staged.app.world_mut())?;
    snapshot::check(world, &prepared)
        .map_err(|error| format!("These rules need a new host: {error}"))?;
    let scenario = staged.app.world().resource::<ScenarioInfo>().clone();
    let before = snapshot::save(world)?;
    let old_scenario = world.resource::<ScenarioInfo>().clone();
    let old_seed = world.resource::<WorldSeed>().0;
    let old_doc = world.resource::<LocalIsland>().doc.clone();
    let was_active = world.resource::<LocalIsland>().active;
    let path = world.resource::<LocalIsland>().path.clone();
    if path.exists() {
        backup(&path)?;
    }
    if let Err(error) = replace_world(world, &prepared, seed, scenario) {
        let restored = replace_world(world, &before, old_seed, old_scenario);
        return Err(format!(
            "The new island could not start: {error}. Previous world restore: {restored:?}"
        ));
    }
    {
        let mut local = world.resource_mut::<LocalIsland>();
        local.doc = doc;
        local.active = true;
    }
    if let Err(error) = save(world) {
        {
            let mut local = world.resource_mut::<LocalIsland>();
            local.doc = old_doc;
            local.active = was_active;
        }
        let restored = replace_world(world, &before, old_seed, old_scenario);
        return Err(format!(
            "The new island could not be saved: {error}. Previous world restore: {restored:?}"
        ));
    }
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
        new_game(app.world_mut(), "Robin").unwrap();
        assert_eq!(summary(app.world()).player, "Robin");
        assert_eq!(app.world().resource::<SimClock>().tick, 0);
        let saved = snapshot::read(&path).unwrap();
        assert_eq!(
            saved.header["scenario"]["world"]["explorer_context"]["actor"],
            "Robin"
        );
        drop(app);
        let resumed = open(&path, None, None, |_| {}).unwrap();
        assert!(summary(resumed.world()).can_continue);
        assert_eq!(summary(resumed.world()).player, "Robin");
        drop(resumed);
        cleanup(&path);
    }

    #[test]
    fn new_name_validation_uses_fresh_aliases_instead_of_saved_history() {
        let path = path("historical-names");
        let mut app = open(&path, None, None, |_| {}).unwrap();
        new_game(app.world_mut(), "Robin").unwrap();
        let played = app.world().resource::<VillageState>().played;
        // Host lookup fixture: saved aliases can describe historical actors.
        // Fresh-game validation must not inherit those aliases, or forget a
        // reserved construction name if a later lookup was renamed.
        app.world_mut()
            .resource_mut::<testbed_content::TestbedIds>()
            .names
            .insert("Former visitor".into(), SimId(played.0 + 1));
        app.world_mut()
            .resource_mut::<testbed_content::TestbedIds>()
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
        new_game(app.world_mut(), "Robin").unwrap();
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
            new_game(app.world_mut(), "Wren")
                .unwrap_err()
                .contains("could not be saved")
        );
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(summary(app.world()).player, "Robin");
        assert!(summary(app.world()).active);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        app.world_mut().resource_mut::<LocalIsland>().path = path.clone();
        drop(app);
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
        new_game(app.world_mut(), "Wren").unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("previous-1.json")).unwrap(),
            b"unreadable old save"
        );
        drop(app);
        cleanup(&path);
    }
}
