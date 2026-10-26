//! The headless client wired to the threaded sim runner: base content in, visuals out, debug
//! commands round-tripping through the runner.

use std::time::Duration;

use bevy::prelude::*;
use fl_client::debug::run_console_line;
use fl_client::link::RunnerStatus;
use fl_client::placeholder::PlayerColors;
use fl_client::view::UnitView;
use fl_game::GameBuilder;
use fl_protocol::PlayerId;

const MAX_UPDATES: usize = 200;

fn unit_views(app: &mut App) -> usize {
    let mut views = app.world_mut().query::<&UnitView>();
    views.iter(app.world()).count()
}

/// Updates `app` (10 ms apart) until `n` unit views exist; panics after [`MAX_UPDATES`].
fn update_until_views(app: &mut App, n: usize) {
    for _ in 0..MAX_UPDATES {
        app.update();
        if unit_views(app) == n {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let failed = app.world().resource::<RunnerStatus>().failed.clone();
    panic!(
        "{n} unit views expected within {MAX_UPDATES} updates, found {} (runner: {failed:?})",
        unit_views(app),
    );
}

#[test]
fn testbed_visuals_appear_headless() {
    let mut app = GameBuilder::new()
        .prepare_headless()
        .expect("the testbed prepares");
    update_until_views(&mut app, 10);
    let colors = app.world().resource::<PlayerColors>();
    assert_eq!(
        colors.0.len(),
        2,
        "both testbed players have a faction colour"
    );
    assert_ne!(colors.color(PlayerId(0)), colors.color(PlayerId(1)));
}

#[test]
fn debug_spawn_round_trips_through_runner() {
    let mut app = GameBuilder::new()
        .prepare_headless()
        .expect("the testbed prepares");
    update_until_views(&mut app, 10);
    // (500, 400) is grass on the testbed; (500, 500) is the river, where a tank cannot spawn.
    let out = run_console_line(app.world_mut(), "spawn base.de.pz3 0 500 400");
    assert!(!out.starts_with("error"), "{out}");
    update_until_views(&mut app, 11);
    assert!(app.world().resource::<RunnerStatus>().failed.is_none());
}

#[test]
fn stress_mode_spawns_the_requested_units() {
    let mut app = GameBuilder::new()
        .stress(40)
        .prepare_headless()
        .expect("the stress scenario prepares");
    update_until_views(&mut app, 40);
}
