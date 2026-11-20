//! Real Bevy schedules exercise the source / mutable-world persistence barrier.
use bevy::{app::AppExit, prelude::*, time::TimeUpdateStrategy};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use terra_bevy::{
    RegionalPersistence, ResidentChunks, SandboxIntent, SandboxQueue, SandboxRuntime,
    TerraWorldPlugin, WorldConfig, WorldRuntime,
};
use terra_core::{FeatureKind, GeoPosition, HeightField, TileKey, WorldChunk, WorldFeature};
use terra_sandbox::Command;
use terra_stream::{ChunkPayload, ChunkProvider, StreamError};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct FlatWithTree;
impl ChunkProvider for FlatWithTree {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        let mut position = key.center();
        position.altitude_m = 100.;
        Ok(ChunkPayload {
            cache_hit: false,
            chunk: WorldChunk {
                key,
                heights: HeightField::constant(100.),
                features: vec![WorldFeature {
                    id: format!("tree:{key}"),
                    kind: FeatureKind::Tree,
                    points: vec![position],
                    height_m: 10.,
                    properties: BTreeMap::new(),
                }],
                sources: vec![],
                warnings: vec![],
            },
        })
    }
}

fn directory(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "terra-bevy-regions-{label}-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn config(directory: &Path) -> WorldConfig {
    let mut focus = TileKey::at(GeoPosition::new(48.137, 11.575, 100.).unwrap(), 15)
        .unwrap()
        .center();
    focus.altitude_m = 100.;
    WorldConfig {
        focus,
        chunk_radius: 0,
        max_resident_chunks: 1,
        max_queued: 1,
        max_in_flight: 1,
        terrain_resolution: 9,
        integrate_per_frame: 1,
        time_scale: 0.,
        live_weather: false,
        base_data_revision: "authored-regional-adapter-fixture-v1".into(),
        cache_dir: directory.join("cache"),
        save_path: directory.join("legacy.json"),
        region_store: Some(directory.join("world.sqlite")),
        region_checkpoint_seconds: 3600.,
        ..Default::default()
    }
}

fn app(config: WorldConfig) -> App {
    app_with_provider(config, Arc::new(FlatWithTree))
}

fn app_with_provider(config: WorldConfig, provider: Arc<dyn ChunkProvider>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins(TerraWorldPlugin::with_provider(config, provider));
    // This fixture isolates resource and structure paging from authored wildlife.
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .ecology
        .config
        .enabled = false;
    app
}

fn until(app: &mut App, complete: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.update();
        assert!(
            !app.world().resource::<RegionalPersistence>().blocked()
                || app.world().resource::<RegionalPersistence>().pending(),
            "{}",
            app.world().resource::<SandboxRuntime>().last_message
        );
        if complete(app) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "regional fixture timed out: {}",
            app.world().resource::<SandboxRuntime>().last_message
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn loaded(app: &mut App, focus: GeoPosition) {
    let key = TileKey::at(focus, 15).unwrap();
    until(app, |app| {
        let resident = &app.world().resource::<ResidentChunks>().chunks;
        resident.len() == 1
            && resident.contains_key(&key)
            && !app
                .world()
                .resource::<RegionalPersistence>()
                .authority_paused()
    });
}

fn intent(app: &mut App, intent: SandboxIntent) {
    app.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(intent);
    until(app, |app| {
        app.world().resource::<SandboxQueue>().0.is_empty()
            && !app
                .world()
                .resource::<RegionalPersistence>()
                .authority_paused()
    });
    let sandbox = app.world().resource::<SandboxRuntime>();
    assert!(
        !app.world().resource::<RegionalPersistence>().blocked(),
        "{}",
        sandbox.last_message
    );
    assert!(
        !sandbox.last_message.starts_with("Action rejected:"),
        "{}",
        sandbox.last_message
    );
}

#[test]
fn eviction_return_and_restart_preserve_depleted_resources_and_inventory() {
    let directory = directory("resource-resume");
    let config = config(&directory);
    let origin = config.focus;
    let id = format!("tree:{}", TileKey::at(origin, 15).unwrap());
    let mut game = app(config.clone());
    loaded(&mut game, origin);
    intent(
        &mut game,
        SandboxIntent::Command(Command::Gather {
            node_id: id.clone(),
            amount: 20,
        }),
    );
    assert_eq!(
        game.world()
            .resource::<SandboxRuntime>()
            .state
            .resource_nodes[&id]
            .remaining,
        0
    );
    let inventory = game.world().resource::<SandboxRuntime>().state.players[&1]
        .inventory
        .clone();
    let far = GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap();
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .set_player_position(1, far)
        .unwrap();
    game.world_mut().resource_mut::<WorldRuntime>().focus = far;
    loaded(&mut game, far);
    assert!(
        !game
            .world()
            .resource::<SandboxRuntime>()
            .state
            .resource_nodes
            .contains_key(&id)
    );
    assert!(
        game.world()
            .resource::<RegionalPersistence>()
            .status
            .as_ref()
            .unwrap()
            .dormant_records
            > 0
    );
    {
        let world = game.world();
        let persistence = world.resource::<RegionalPersistence>();
        let before_generation = persistence.status.as_ref().unwrap().generation;
        let exported = persistence
            .export_world(
                world.resource::<SandboxRuntime>(),
                world.resource::<WorldRuntime>(),
                world.resource::<terra_bevy::PoliticalWorld>(),
            )
            .unwrap();
        assert!(!exported.state.is_partial());
        assert_eq!(exported.state.resource_nodes[&id].remaining, 0);
        assert_eq!(
            persistence.status.as_ref().unwrap().generation,
            before_generation
        );
        let mut truncated = exported;
        truncated.state = world.resource::<SandboxRuntime>().state.clone();
        assert!(
            truncated
                .validate_for(
                    &world.resource::<WorldRuntime>().config,
                    &world.resource::<SandboxRuntime>().catalog,
                )
                .is_err(),
            "legacy JSON must reject an incomplete active overlay"
        );
        let unsafe_export = directory.join("incomplete.json");
        assert!(truncated.write(&unsafe_export).is_err());
        assert!(!unsafe_export.exists());
    }
    intent(&mut game, SandboxIntent::Save);
    // A dropped process need not deliver an exit event: the prior commit is
    // already enough to recover both the active and dormant overlay.
    drop(game);
    let mut resumed = app(config);
    loaded(&mut resumed, far);
    assert_eq!(resumed.world().resource::<WorldRuntime>().focus, far);
    resumed.world_mut().resource_mut::<WorldRuntime>().focus = origin;
    loaded(&mut resumed, origin);
    assert!(resumed.world().resource::<RegionalPersistence>().restored);
    let state = &resumed.world().resource::<SandboxRuntime>().state;
    assert_eq!(state.players[&1].inventory, inventory);
    assert_eq!(
        state.resource_nodes[&id].remaining, 0,
        "source registration must not refill a cold node"
    );
    drop(resumed);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn commands_resolve_a_dormant_workbench_when_the_player_returns() {
    let directory = directory("dormant-workbench");
    let config = config(&directory);
    let origin = config.focus;
    let mut game = app(config);
    loaded(&mut game, origin);
    for command in [
        Command::ClaimPlot {
            center: origin,
            radius_m: 40.,
        },
        Command::Craft {
            recipe: "planks".into(),
            count: 4,
        },
        Command::Build {
            blueprint: "workbench".into(),
            position: origin,
        },
    ] {
        intent(&mut game, SandboxIntent::Command(command));
    }
    let workbench = *game
        .world()
        .resource::<SandboxRuntime>()
        .state
        .structures
        .keys()
        .next()
        .unwrap();
    let far = GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap();
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .set_player_position(1, far)
        .unwrap();
    game.world_mut().resource_mut::<WorldRuntime>().focus = far;
    loaded(&mut game, far);
    assert!(
        !game
            .world()
            .resource::<SandboxRuntime>()
            .state
            .structures
            .contains_key(&workbench)
    );
    // Save can retire pages halfway through a mixed intent batch. The following
    // trusted relocation and craft must reactivate the workbench before querying.
    game.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    game.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::MoveTo(origin));
    intent(
        &mut game,
        SandboxIntent::Command(Command::Craft {
            recipe: "tool".into(),
            count: 1,
        }),
    );
    let state = &game.world().resource::<SandboxRuntime>().state;
    assert_eq!(state.players[&1].inventory.get("tool"), Some(&1));
    assert!(
        state.structures.contains_key(&workbench),
        "the returning player's interaction region stays pinned"
    );
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn explicit_restore_rewinds_inventory_and_clock_without_overwriting_legacy_json() {
    let directory = directory("explicit-restore");
    let config = config(&directory);
    let origin = config.focus;
    let mut game = app(config.clone());
    loaded(&mut game, origin);
    let clock = game.world().resource::<WorldRuntime>().clock;
    let inventory = game.world().resource::<SandboxRuntime>().state.players[&1]
        .inventory
        .clone();
    intent(&mut game, SandboxIntent::Save);
    // Arrange a change after the committed barrier; load must use the store,
    // even when a different legacy JSON path is configured.
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 1);
    game.world_mut()
        .resource_mut::<WorldRuntime>()
        .clock
        .unix_seconds += 12_345.;
    intent(&mut game, SandboxIntent::Load);
    assert_eq!(
        game.world().resource::<SandboxRuntime>().state.players[&1].inventory,
        inventory
    );
    assert_eq!(game.world().resource::<WorldRuntime>().clock, clock);
    assert!(!config.save_path.exists());
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn stale_background_result_cannot_replace_a_newer_game_owned_revision() {
    let directory = directory("stale-completion");
    let config = config(&directory);
    let mut game = app(config.clone());
    loaded(&mut game, config.focus);
    let saved_wood = game.world().resource::<SandboxRuntime>().state.players[&1].inventory["wood"];
    game.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    game.update();
    assert!(game.world().resource::<RegionalPersistence>().pending());
    assert!(
        game.world()
            .resource::<RegionalPersistence>()
            .authority_paused()
    );
    // A custom game that violates the pause contract still must not silently
    // replace a newer revision with a background result.
    {
        let mut sandbox = game.world_mut().resource_mut::<SandboxRuntime>();
        sandbox
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("wood".into(), saved_wood + 7);
        sandbox.revision += 1;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !game.world().resource::<RegionalPersistence>().blocked() {
        game.update();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        game.world().resource::<SandboxRuntime>().state.players[&1].inventory["wood"],
        saved_wood + 7
    );
    assert!(
        game.world()
            .resource::<RegionalPersistence>()
            .last_error
            .as_deref()
            .unwrap()
            .contains("another world revision")
    );
    intent(&mut game, SandboxIntent::Load);
    assert_eq!(
        game.world().resource::<SandboxRuntime>().state.players[&1].inventory["wood"],
        saved_wood
    );
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn source_mismatch_exits_without_mutating_the_committed_world() {
    let directory = directory("identity-mismatch");
    let config = config(&directory);
    let mut game = app(config.clone());
    loaded(&mut game, config.focus);
    intent(&mut game, SandboxIntent::Save);
    let generation = game
        .world()
        .resource::<RegionalPersistence>()
        .status
        .as_ref()
        .unwrap()
        .generation;
    drop(game);
    let mut other = config.clone();
    other.seed += 1;
    let mut wrong = app(other);
    wrong.update();
    assert!(wrong.should_exit().unwrap().is_error());
    assert!(wrong.world().resource::<RegionalPersistence>().blocked());
    drop(wrong);
    let store = terra_persistence::RegionStore::open(
        config.region_store.as_ref().unwrap(),
        serde_json::json!({
            "adapter": "terra-bevy-local-regions-v1",
            "world": terra_bevy::sandbox::WorldIdentity::from(&config),
        }),
        &terra_sandbox::Catalog::builtin(),
        config.chunk_level,
    )
    .unwrap();
    assert_eq!(
        store.status().generation,
        generation,
        "the mismatched attempt must not commit"
    );
    drop(store);
    let mut resumed = app(config.clone());
    resumed.update();
    assert!(!resumed.world().resource::<RegionalPersistence>().blocked());
    assert!(resumed.world().resource::<RegionalPersistence>().restored);
    drop(resumed);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn graceful_exit_commits_last_changes_and_failed_commit_returns_error() {
    let directory = directory("exit-commit");
    let config = config(&directory);
    let mut game = app(config.clone());
    loaded(&mut game, config.focus);
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 73);
    game.world_mut().write_message(AppExit::Success);
    game.update();
    assert_eq!(game.should_exit(), Some(AppExit::Success));
    assert_eq!(
        game.world()
            .resource::<RegionalPersistence>()
            .last_checkpoint_reason,
        "Graceful exit"
    );
    drop(game);
    let mut resumed = app(config.clone());
    loaded(&mut resumed, config.focus);
    assert_eq!(
        resumed.world().resource::<SandboxRuntime>().state.players[&1].inventory["wood"],
        73
    );
    // An invalid game-owned mutation cannot be committed, and exit cannot
    // report success while losing the requested durable write.
    resumed
        .world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("not_in_catalog".into(), 1);
    resumed.world_mut().write_message(AppExit::Success);
    resumed.update();
    assert!(resumed.should_exit().unwrap().is_error());
    assert!(resumed.world().resource::<RegionalPersistence>().blocked());
    drop(resumed);
    let mut recovered = app(config.clone());
    recovered.update();
    assert_eq!(
        recovered.world().resource::<SandboxRuntime>().state.players[&1].inventory["wood"],
        73
    );
    assert!(
        !recovered.world().resource::<SandboxRuntime>().state.players[&1]
            .inventory
            .contains_key("not_in_catalog")
    );
    drop(recovered);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn checkpoint_recovery_replays_skipped_source_and_clears_the_error_without_resuming_time() {
    let directory = directory("recover-source");
    let config = config(&directory);
    let origin = config.focus;
    let origin_id = format!("tree:{}", TileKey::at(origin, 15).unwrap());
    let mut game = app(config.clone());
    loaded(&mut game, origin);
    intent(
        &mut game,
        SandboxIntent::Command(Command::Gather {
            node_id: origin_id.clone(),
            amount: 20,
        }),
    );
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("invalid_game_item".into(), 1);
    game.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Save);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !game.world().resource::<RegionalPersistence>().blocked() {
        game.update();
        assert!(
            Instant::now() < deadline,
            "failed checkpoint completion did not arrive"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(game.world().resource::<WorldRuntime>().clock.paused);
    assert!(game.world().resource::<WorldRuntime>().last_error.is_some());
    let recovery_hint = game.world().resource::<RegionalPersistence>().status_line();
    assert!(recovery_hint.contains("F5 retry checkpoint"));
    assert!(recovery_hint.contains("F9 restore last checkpoint"));
    assert!(!recovery_hint.contains("inspect F12"));
    let far = GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap();
    let far_key = TileKey::at(far, 15).unwrap();
    let far_id = format!("tree:{far_key}");
    game.world_mut().resource_mut::<WorldRuntime>().focus = far;
    game.world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .push_back(SandboxIntent::Command(Command::Craft {
            recipe: "planks".into(),
            count: 1,
        }));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !game
        .world()
        .resource::<ResidentChunks>()
        .chunks
        .contains_key(&far_key)
    {
        game.update();
        assert!(
            Instant::now() < deadline,
            "blocked geometry did not finish loading"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !game
            .world()
            .resource::<SandboxRuntime>()
            .state
            .resource_nodes
            .contains_key(&far_id)
    );
    assert_eq!(
        game.world().resource::<SandboxRuntime>().state.players[&1]
            .inventory
            .get("planks"),
        None
    );
    game.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .remove("invalid_game_item");
    intent(&mut game, SandboxIntent::Save);
    assert!(game.world().resource::<WorldRuntime>().last_error.is_none());
    assert!(game.world().resource::<WorldRuntime>().clock.paused);
    assert!(
        game.world()
            .resource::<SandboxRuntime>()
            .last_message
            .contains("remains paused")
    );
    game.update();
    let state = &game.world().resource::<SandboxRuntime>().state;
    assert_eq!(
        state.resource_nodes[&far_id].remaining, 20,
        "recovered loaded chunks need their gameplay source records"
    );
    assert_eq!(
        state.resource_nodes[&origin_id].remaining, 0,
        "replaying source must preserve depletion"
    );
    assert!(!game.world().resource::<RegionalPersistence>().blocked());
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn paused_autosave_uses_real_time_even_when_bevy_virtual_time_clamps_a_slow_frame() {
    let directory = directory("wall-clock");
    let mut config = config(&directory);
    config.region_checkpoint_seconds = 1.;
    let origin = config.focus;
    let mut game = app(config);
    loaded(&mut game, origin);
    intent(&mut game, SandboxIntent::Save);
    let generation = game
        .world()
        .resource::<RegionalPersistence>()
        .status
        .as_ref()
        .unwrap()
        .generation;
    game.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
    game.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        1500,
    )));
    game.update();
    assert!(game.world().resource::<Time>().delta_secs_f64() < 1.);
    game.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    until(&mut game, |app| {
        app.world()
            .resource::<RegionalPersistence>()
            .status
            .as_ref()
            .unwrap()
            .generation
            > generation
    });
    let persistence = game.world().resource::<RegionalPersistence>();
    assert!(persistence.status.as_ref().unwrap().generation > generation);
    assert_eq!(persistence.last_checkpoint_reason, "Automatic checkpoint");
    assert!(game.world().resource::<WorldRuntime>().clock.paused);
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_visible_source_tree_keeps_its_boundary_tombstone_active_without_nearby_bodies() {
    struct BoundaryTree;
    impl ChunkProvider for BoundaryTree {
        fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
            let mut payload = FlatWithTree.load(key)?;
            payload.chunk.features[0].points[0].longitude = key.bounds().east;
            Ok(payload)
        }
    }
    let directory = directory("boundary-tombstone");
    let config = config(&directory);
    let origin = config.focus;
    let source_key = TileKey::at(origin, 15).unwrap();
    let id = format!("tree:{source_key}");
    let mut game = app_with_provider(config, Arc::new(BoundaryTree));
    loaded(&mut game, origin);
    let feature_position = game
        .world()
        .resource::<SandboxRuntime>()
        .state
        .world_features[&id]
        .position;
    assert_ne!(TileKey::at(feature_position, 15).unwrap(), source_key);
    {
        let mut sandbox = game.world_mut().resource_mut::<SandboxRuntime>();
        // Arrange a validated authored removal and move the sole retained body
        // far away, so proximity pins cannot accidentally make this test pass.
        sandbox.state.feature_edits.insert(id.clone());
        sandbox.state.resource_nodes.remove(&id);
        sandbox
            .state
            .set_player_position(
                1,
                GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap(),
            )
            .unwrap();
    }
    intent(&mut game, SandboxIntent::Save);
    assert_eq!(game.world().resource::<ResidentChunks>().chunks.len(), 1);
    let state = &game.world().resource::<SandboxRuntime>().state;
    assert!(state.world_features.contains_key(&id));
    assert!(
        state.feature_edits.contains(&id),
        "the visible source's tombstone must survive retirement in its adjacent geographic page"
    );
    assert!(!state.resource_nodes.contains_key(&id));
    drop(game);
    let _ = std::fs::remove_dir_all(directory);
}
