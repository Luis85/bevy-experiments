//! Wave F1: `npc_command` through the facade (`NpcPlugins::with_command`).

#![cfg(feature = "command")]

use bevy_app::App;
use npc_framework::kernel::registry::ContentHash;
use npc_framework::prelude::*;

fn app(command: bool) -> App {
    let mut app = App::new();
    let plugins = NpcPlugins::new().seed(3).without_driver();
    let plugins = if command {
        plugins.with_command()
    } else {
        plugins
    };
    app.add_plugins(plugins);
    app.finish();
    app.cleanup();
    app
}

fn manifest(app: &App) -> Vec<(String, String)> {
    app.world().resource::<ContentHash>().manifest.clone()
}

#[test]
fn command_is_opt_in() {
    let off = manifest(&app(false));
    let names = [
        "IssueOrder",
        "CreateSquad",
        "order_queue",
        "ExecuteOrder",
        "has_unit_order",
    ];
    for name in names {
        assert!(
            !off.iter().any(|(_, n)| n == name),
            "`{name}` registered without with_command()"
        );
    }
    let on = manifest(&app(true));
    for name in names {
        assert!(on.iter().any(|(_, n)| n == name), "`{name}` missing");
    }
}

#[test]
fn with_command_moves_a_unit() {
    let mut app = app(true);
    let w = app.world_mut();
    let (id, e) = spawn_sim(
        w,
        (SimPos::new(0, 0), Commandable::new(250), Owner(PlayerId(1))),
    );
    submit_order(
        w,
        &IssueOrder::new(PlayerId(1), [id], UnitOrder::Move { pos: (2000, 0) }),
    )
    .unwrap();
    run_ticks(w, 12);
    assert_eq!(*w.get::<SimPos>(e).unwrap(), SimPos::new(2000, 0));
    assert!(w.get::<OrderQueue>(e).unwrap().is_empty());
}
