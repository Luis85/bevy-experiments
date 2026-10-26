//! `GameBuilder::prepare_headless` rejects bad content and orphan client features before any
//! window or sim thread exists.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use fl_client::ClientFeature;
use fl_game::{GameBuilder, check_client_features};
use fl_sim::SimFeature;

fn bad_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fl_cli/tests/fixtures/bad_mod")
}

/// A client feature presenting a sim feature that no match includes.
struct Ghost;

impl ClientFeature for Ghost {
    fn sim_feature(&self) -> &'static str {
        "ghost"
    }
    fn build(&self, _app: &mut App) {}
}

/// A client feature for the always-present core feature.
struct CoreView;

impl ClientFeature for CoreView {
    fn sim_feature(&self) -> &'static str {
        "core"
    }
    fn build(&self, _app: &mut App) {}
}

#[test]
fn builder_rejects_broken_content() {
    let Err(error) = GameBuilder::new().content_mod(bad_mod()).prepare_headless() else {
        panic!("a broken mod must not prepare");
    };
    assert!(
        error.contains("bogus_component.unit.ron"),
        "the error names the bad file: {error}"
    );
}

#[test]
fn orphan_client_feature_is_error() {
    let Err(error) = GameBuilder::new().client_feature(Ghost).prepare_headless() else {
        panic!("a client feature without its sim feature must not prepare");
    };
    assert_eq!(error, "client feature for 'ghost' has no SimFeature");
}

#[test]
fn client_feature_for_core_is_accepted() {
    let sim: Vec<Arc<dyn SimFeature>> = fl_catalog::features();
    let client: Vec<Arc<dyn ClientFeature>> = vec![Arc::new(CoreView)];
    assert_eq!(check_client_features(&sim, &client), Ok(()));
    let ghost: Vec<Arc<dyn ClientFeature>> = vec![Arc::new(CoreView), Arc::new(Ghost)];
    assert_eq!(
        check_client_features(&sim, &ghost),
        Err("client feature for 'ghost' has no SimFeature".to_owned())
    );
}

#[test]
fn unknown_scenario_is_error() {
    let Err(error) = GameBuilder::new().scenario("base.nope").prepare_headless() else {
        panic!("an unknown scenario must not prepare");
    };
    assert!(error.contains("base.nope"), "{error}");
}

#[test]
fn testbed_scenario_gets_terrain_view() {
    let app = GameBuilder::new()
        .scenario("base.testbed")
        .prepare_headless()
        .expect("the testbed scenario prepares");
    let map = app.world().resource::<fl_client::map_view::MapView>();
    let terrain = map.terrain.as_ref().expect("the testbed map has layers");
    assert_eq!(terrain.cells, UVec2::new(250, 250));
    assert_eq!(terrain.cell_m, 4.0);
}

#[test]
fn loading_a_scenario_replaces_the_map_view() {
    use fl_client::debug::{LoadRequest, load_match};
    use fl_client::map_view::MapView;

    // The stress map is the 2 km terrain map; the testbed scenario's map is 1 km.
    let mut app = GameBuilder::new()
        .stress(10)
        .prepare_headless()
        .expect("the stress match prepares");
    assert_eq!(
        app.world()
            .resource::<MapView>()
            .terrain
            .as_ref()
            .map(|t| t.cells),
        Some(UVec2::new(500, 500))
    );
    load_match(
        app.world_mut(),
        LoadRequest::Scenario("base.testbed".to_owned()),
    )
    .expect("the testbed scenario loads");
    let map = app.world().resource::<MapView>();
    assert_eq!(
        map.terrain.as_ref().map(|t| t.cells),
        Some(UVec2::new(250, 250))
    );
    assert_eq!(map.size_m, Vec2::new(1000.0, 1000.0));
}

#[test]
fn stress_map_view_has_scale_terrain() {
    use fl_client::map_view::MapView;

    let app = GameBuilder::new()
        .stress(500)
        .prepare_headless()
        .expect("the stress match prepares");
    let map = app.world().resource::<MapView>();
    let terrain = map
        .terrain
        .as_ref()
        .expect("the stress map is the 2 km terrain map");
    assert_eq!(terrain.cells, UVec2::new(500, 500));
    assert_eq!(map.size_m, Vec2::new(2000.0, 2000.0));
}

/// The save of the running match of `app`.
fn save_of(app: &mut App) -> Vec<u8> {
    use fl_client::LocalDriver;
    use fl_client::link::{PendingReplies, RunnerLink, send_query, wait_for_reply};
    use fl_view::{SimQuery, SimReply, TickDriver};

    let world = app.world_mut();
    let pending = world.resource::<LocalDriver>().pending();
    let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
        send_query(
            world.resource::<RunnerLink>(),
            &mut replies,
            SimQuery::Save { pending },
        )
    });
    match wait_for_reply(world, id, std::time::Duration::from_secs(10)) {
        Ok(SimReply::Save(bytes)) => bytes,
        other => panic!("a save: {other:?}"),
    }
}

#[test]
fn loading_a_save_shows_its_map() {
    use fl_client::debug::{LoadRequest, load_match};
    use fl_client::map_view::MapView;

    // Carried from Task 17: a save names no map, so the map comes from the loaded match.
    let mut app = GameBuilder::new()
        .stress(10)
        .prepare_headless()
        .expect("the stress match prepares");
    let save = save_of(&mut app);
    load_match(
        app.world_mut(),
        LoadRequest::Scenario("base.testbed".to_owned()),
    )
    .expect("the testbed scenario loads");
    assert_eq!(
        app.world().resource::<MapView>().size_m,
        Vec2::new(1000.0, 1000.0)
    );
    load_match(app.world_mut(), LoadRequest::Save(save)).expect("the save loads");
    let map = app.world().resource::<MapView>();
    assert_eq!(map.size_m, Vec2::new(2000.0, 2000.0), "the save's own map");
    assert_eq!(
        map.terrain.as_ref().map(|t| t.cells),
        Some(UVec2::new(500, 500))
    );
}
