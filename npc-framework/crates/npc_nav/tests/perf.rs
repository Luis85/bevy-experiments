//! A* throughput on a 128 × 128 grid with 50 agents (plan Task C1 perf
//! numbers). Ignored by default; run in release:
//! `cargo test --release -p npc_nav --test perf -- --ignored --nocapture`.

use bevy_app::App;
use npc_kernel::spatial::{CellCoord, CellFlags, CellInfo, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimPos, run_ticks, spawn_sim};
use npc_locomotion::{MoveIntent, StraightLineMover, set_move_intent};
use npc_nav::{NavAgent, NavBudget, NavPlugin, NavStats};

fn crowd(budget: u32) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(128, 128)))
        .insert_resource(NavBudget(budget));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
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
        // Five long walls with a few gaps: searches must wind around them.
        for x in [-40i32, -20, 0, 20, 40] {
            for z in -64..64 {
                if (z + x).rem_euclid(31) != 0 {
                    grid.set_static(CellCoord::new(x, z), wall);
                }
            }
        }
    }
    for i in 0..50 {
        let from = SimPos::new(-60_000 + i * 200, -60_000 + (i * 2477) % 120_000);
        let to = SimPos::new(60_000 - i * 200, -60_000 + (i * 3511) % 120_000);
        let (_, e) = spawn_sim(app.world_mut(), (from, NavAgent));
        set_move_intent(app.world_mut(), e, MoveIntent::to(to, 400));
    }
    app
}

#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
// determinism: wall-clock timing of the test harness only, never sim state.
#[allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]
fn astar_throughput_128x128_50_agents() {
    // Everything in one tick: pure search throughput.
    let mut app = crowd(u32::MAX);
    let t = std::time::Instant::now();
    run_ticks(app.world_mut(), 1);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let stats = *app.world().resource::<NavStats>();
    println!(
        "one tick, unlimited budget: {} paths, {} failed, {} nodes in {ms:.2} ms = {:.0} nodes/ms",
        stats.paths_found,
        stats.paths_failed,
        stats.nodes_total,
        stats.nodes_total as f64 / ms
    );

    // Default budget: how many ticks, and the worst tick.
    let mut app = crowd(npc_nav::DEFAULT_NAV_BUDGET);
    let mut worst = 0.0f64;
    let mut ticks = 0;
    while app.world().resource::<NavStats>().paths_found
        + app.world().resource::<NavStats>().paths_failed
        < 50
    {
        let t = std::time::Instant::now();
        run_ticks(app.world_mut(), 1);
        worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
        ticks += 1;
        assert!(ticks < 1000);
    }
    let stats = *app.world().resource::<NavStats>();
    println!(
        "budget {}: all 50 served in {ticks} ticks, {} nodes, worst tick {worst:.3} ms",
        npc_nav::DEFAULT_NAV_BUDGET,
        stats.nodes_total
    );
}
