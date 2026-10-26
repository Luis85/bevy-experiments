//! Task P2: schedule-shuffle determinism.
//!
//! `ScheduleBuildSettings::shuffle_seed` (bevy_ecs feature `debug`, enabled
//! for this crate's tests in Cargo.toml) shuffles the `NpcSimTick` system order
//! within its ordering constraints. On the `SingleThreadedExecutor` (the
//! multi-threaded one can produce orders a shuffle never would), 8 shuffle
//! seeds must give the same state hashes as the unshuffled default build.
//! A difference means two systems are ordered only by chance: add the
//! missing `.before/.after/.chain` (ambiguity detection only catches
//! *conflicting* access, not semantic order dependencies).

use bevy_app::App;
use bevy_ecs::prelude::{Component, With};
use bevy_ecs::schedule::{ScheduleBuildSettings, Schedules, SingleThreadedExecutor};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::{NpcSimTick, Order, SimCommand, SimId, run_ticks, submit};
use village_content::needs::HUNGER;
use village_content::replay::new_sim_app;
use village_content::{SetFood, SpawnVillager, Stockpile, Villager};

const TICKS: u64 = 5_000;
const EVERY: u64 = 500;
const SHUFFLE_SEEDS: [u64; 8] = [1, 2, 3, 5, 8, 13, 21, 0xdead_beef];

/// The `SimId`s of every `T`, sorted (looked up, not hard-coded: a new
/// scenario entity renumbers the plots).
fn ids_of<T: Component>(app: &mut App) -> Vec<u32> {
    let world = app.world_mut();
    let mut q = world.query_filtered::<&SimId, With<T>>();
    let mut ids: Vec<u32> = q.iter(world).map(|id| id.0).collect();
    ids.sort_unstable();
    ids
}

/// A 4-plot village (shared stockpiles and contention) with a few commands,
/// so orders, spawns and despawns run too.
fn app(shuffle: Option<u64>) -> App {
    let mut app = new_sim_app(7, "village_n", 4);
    if let Some(seed) = shuffle {
        app.edit_schedule(NpcSimTick, |schedule| {
            let settings = schedule.get_build_settings();
            schedule.set_build_settings(ScheduleBuildSettings {
                shuffle_seed: Some(seed),
                ..settings
            });
            schedule.set_executor(SingleThreadedExecutor::new());
        });
    }
    let (villagers, stockpiles) = (ids_of::<Villager>(&mut app), ids_of::<Stockpile>(&mut app));
    let world = app.world_mut();
    let cmds = [
        (
            50,
            SimCommand::Order {
                unit: villagers[0],
                order: Order::GoTo { pos: (9000, -7000) },
            },
        ),
        (
            300,
            SimCommand::content(&SpawnVillager {
                pos: (1000, 1000),
                home: None,
            }),
        ),
        (
            700,
            SimCommand::SetNeed {
                unit: villagers[1],
                need: HUNGER.into(),
                value: 5_000,
            },
        ),
        (
            1200,
            SimCommand::Despawn {
                unit: stockpiles[2],
            },
        ),
        (
            2600,
            SimCommand::content(&SetFood {
                stockpile: stockpiles[0],
                food: 0,
            }),
        ),
    ];
    for (tick, cmd) in cmds {
        submit(world, Some(tick), cmd).expect("valid command");
    }
    app
}

/// Hashes every `EVERY` ticks up to `TICKS`, and the system order used.
fn run(shuffle: Option<u64>) -> (Vec<u64>, Vec<String>) {
    let mut app = app(shuffle);
    let world = app.world_mut();
    let mut hashes = Vec::new();
    for _ in 0..TICKS / EVERY {
        run_ticks(world, EVERY);
        hashes.push(state_hash(world));
    }
    let order = world
        .resource::<Schedules>()
        .get(NpcSimTick)
        .expect("NpcSimTick schedule")
        .systems()
        .expect("NpcSimTick initialised")
        .map(|(_, system)| system.name().to_string())
        .collect();
    (hashes, order)
}

#[test]
fn shuffled_schedules_give_same_hash() {
    let (expected, base_order) = run(None);
    let mut reordered = 0;
    for seed in SHUFFLE_SEEDS {
        let (hashes, order) = run(Some(seed));
        assert_eq!(
            base_order.len(),
            order.len(),
            "shuffle {seed}: same systems"
        );
        if order != base_order {
            reordered += 1;
        }
        assert_eq!(
            hashes, expected,
            "shuffle seed {seed}: state hashes differ from the unshuffled run\n\
             unshuffled order: {base_order:#?}\nshuffled order: {order:#?}"
        );
    }
    // Today every NpcSimTick system is chained, so a shuffle has no freedom; this
    // count shows when unordered systems appear (and the test starts biting).
    eprintln!(
        "shuffle: {reordered}/{} seeds changed the order of {} systems",
        SHUFFLE_SEEDS.len(),
        base_order.len()
    );
}
