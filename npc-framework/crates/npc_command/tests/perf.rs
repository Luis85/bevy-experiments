//! A 200-unit group order (plan wave F1 perf numbers). Ignored by default;
//! run in release:
//! `cargo test --release -p npc_command --test perf -- --ignored --nocapture`.

use bevy_app::App;
use npc_command::{CommandPlugin, Commandable, IssueOrder, OrderQueue, Owner, PlayerId, UnitOrder};
use npc_kernel::spatial::{CellCoord, CellFlags, CellInfo, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimId, SimPos, run_ticks, spawn_sim, submit};
use npc_locomotion::StraightLineMover;
use npc_nav::{NavAgent, NavPlugin};

fn crowd(nav: bool) -> (App, Vec<SimId>) {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(128, 128)));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
        CommandPlugin,
    ));
    app.finish();
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        let wall = CellInfo {
            flags: CellFlags {
                blocking: true,
                ..Default::default()
            },
            extra_cost: 0,
        };
        for x in -20..=20 {
            grid.set_static(CellCoord::new(x, 0), wall);
        }
    }
    let ids = (0..200)
        .map(|i: i32| {
            let at = SimPos::new(-15_000 + (i % 20) * 1500, 10_000 + (i / 20) * 1500);
            let (id, e) = spawn_sim(
                app.world_mut(),
                (at, Commandable::new(200), Owner(PlayerId(1))),
            );
            if nav {
                app.world_mut().entity_mut(e).insert(NavAgent);
            }
            id
        })
        .collect();
    (app, ids)
}

#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
// determinism: wall-clock timing of the test harness only, never sim state.
#[allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]
fn group_order_200_units() {
    for nav in [false, true] {
        let (mut app, ids) = crowd(nav);
        let cmd = IssueOrder::new(PlayerId(1), ids, UnitOrder::Move { pos: (0, -20_000) });
        let t = std::time::Instant::now();
        submit(app.world_mut(), None, cmd.to_command()).unwrap();
        let submit_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        run_ticks(app.world_mut(), 1); // applies: formation, 200 queues, first engine pass
        let apply_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let ticks = 400;
        run_ticks(app.world_mut(), ticks);
        let per_tick_ms = t.elapsed().as_secs_f64() * 1000.0 / ticks as f64;
        let mut q = app.world_mut().query::<&OrderQueue>();
        let busy = q.iter(app.world()).filter(|q| q.has_order()).count();
        println!(
            "200 units, nav={nav}: submit {submit_ms:.3} ms, apply tick {apply_ms:.3} ms, \
             {per_tick_ms:.3} ms/tick over {ticks} ticks, {busy} still moving"
        );
    }
}
