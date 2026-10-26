//! The iteration-1 village layout (shared by tests and the app), its
//! `village_n` grid variant for benchmarks, and the headless app constructor.
//!
//! Positions in mm on a 32 m × 32 m tile centred on the origin:
//! - House     (-6000, -5000), yaw 0,   approach (-6000, -2600)
//! - Bed       (-2500, -5500), yaw 90,  approach (-2500, -4000), Provides Energy
//! - Stockpile ( 5000, -4000), yaw 0,   approach ( 5000, -2800), food 4 / capacity 10, Provides Hunger
//! - Field     ( 3000,  5000), yaw 0,   approach ( 3000,  2200), work_ticks 300, yield_food 3
//! - Villager  (    0,     0), home = House, speed 60
//! - Well      (-4500,  4500), yaw 0,   approach (-4500,  3300), 2 slots, Provides Thirst
//!
//! The well is spawned after the villager (in `village_n`, after every
//! plot), so the older places and villagers keep their SimIds (house 1, bed
//! 2, stockpile 3, field 4, villager 5, well 6 in `village`).
//!
//! `village_nav` (opt-in pathfinding, plan Task C1) is `village_n` plus, per
//! plot, a wall (`GridBlocker`) between the villager and the stockpile and
//! field, and `NavAgent` on every villager: the villagers path-find around it
//! with `npc_nav`. The other scenarios do not add the nav plugin, so their
//! content hash, state hashes and journals are unchanged.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::kernel::spatial::GridBounds;
use npc_framework::nav::{NavAgent, NavPlugin};
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

use crate::VillagePlugin;
use crate::activity::Activity;
use crate::needs::{ENERGY, HUNGER, THIRST, need_id};
use crate::villager::VillagerTree;
use crate::world::{Bed, Home, Place, PlaceKind, Stockpile, Villager, Well, WorkSite};

/// SimIds of everything the scenario spawned.
#[derive(Resource, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VillageIds {
    pub house: SimId,
    pub bed: SimId,
    pub stockpile: SimId,
    pub field: SimId,
    pub villager: SimId,
    pub well: SimId,
}

/// Energy restored per tick asleep in the bed.
pub const BED_ENERGY_PER_TICK: i32 = 200;
/// Hunger restored per tick eating at the stockpile.
pub const STOCKPILE_HUNGER_PER_TICK: i32 = 1000;
pub const STOCKPILE_START_FOOD: u32 = 4;
pub const STOCKPILE_CAPACITY: u32 = 10;
pub const FIELD_WORK_TICKS: u32 = 300;
pub const FIELD_YIELD_FOOD: u32 = 3;
/// Villager walking speed in mm per tick (1.2 m/s).
pub const VILLAGER_SPEED: i32 = 60;
/// Thirst restored per tick drinking at the well (one drink of
/// `DRINK_TICKS` fills it from 0).
pub const WELL_THIRST_PER_TICK: i32 = 2500;
/// Villagers that can drink at one well at the same time.
pub const WELL_SLOTS: u32 = 2;

/// Bundle of a fresh villager of `home` standing at `pos`, running `tree`
/// (as spawned by the scenarios and by the `SpawnVillager` command), with
/// full needs.
pub fn villager_bundle(world: &World, home: SimId, pos: SimPos, tree: TreeId) -> impl Bundle {
    (
        Villager {
            home,
            speed: VILLAGER_SPEED,
        },
        pos,
        PrevSimPos(pos),
        world.resource::<NeedRegistry>().full(),
        Activity::Idle,
        Brain { tree },
    )
}

/// The villager tree's id (freezing content first if needed).
fn village_tree(world: &mut World) -> TreeId {
    crate::finish_content(world);
    world
        .get_resource::<VillagerTree>()
        .expect("spawning a village requires VillagePlugin")
        .0
}

/// Spawns the layout above (in the order house, bed, stockpile, field,
/// villager, well) and inserts `VillageIds`. Requires `NpcPlugins` +
/// `VillagePlugin`.
pub fn spawn_village(world: &mut World) -> VillageIds {
    let tree = village_tree(world);
    let origin = SimPos::new(0, 0);
    let mut ids = spawn_plot(world, tree, origin);
    ids.well = spawn_well(world, origin);
    world.insert_resource(ids);
    ids
}

/// Grid spacing of `spawn_village_n` plots in mm. The iteration-1 layout
/// spans x -6000..5000 and z -5500..5000, so 16 m plots never overlap.
pub const PLOT_SIZE: i32 = 16_000;

/// `n` copies of the iteration-1 layout (one house, bed, stockpile, field,
/// villager and well each, the field feeding its own stockpile) on a square
/// grid of `PLOT_SIZE` plots centred on the origin, filled row by row.
/// Spawned plot by plot in the same order as [`spawn_village`], then one well
/// per plot (so the plots keep the SimIds they had before wells existed), so
/// `n == 1` is exactly the iteration-1 village. Inserts the first plot's
/// `VillageIds` (when `n > 0`) so single-village tools keep working; returns
/// all plots.
pub fn spawn_village_n(world: &mut World, n: u32) -> Vec<VillageIds> {
    let tree = village_tree(world);
    // ceil(sqrt(n)) in integers (was an f64 sqrt; identical for every u32).
    let root = n.isqrt();
    let side = if root * root < n { root + 1 } else { root }.max(1) as i32;
    let half = (side - 1) * PLOT_SIZE / 2;
    let offset = |k: i32| SimPos::new((k % side) * PLOT_SIZE - half, (k / side) * PLOT_SIZE - half);
    let mut plots: Vec<VillageIds> = (0..n as i32)
        .map(|k| spawn_plot(world, tree, offset(k)))
        .collect();
    for (k, plot) in (0..n as i32).zip(plots.iter_mut()) {
        plot.well = spawn_well(world, offset(k));
    }
    if let Some(&first) = plots.first() {
        world.insert_resource(first);
    }
    plots
}

/// One iteration-1 layout shifted by `offset`, without its well (`well` is
/// left at `SimId(0)`; see [`spawn_well`]).
fn spawn_plot(world: &mut World, tree: TreeId, offset: SimPos) -> VillageIds {
    let at = |x: i32, z: i32| SimPos::new(x + offset.x, z + offset.z);
    let place = |kind, yaw_deg| Place { kind, yaw_deg };
    let (hunger, energy) = (need_id(world, HUNGER), need_id(world, ENERGY));

    let (house, _) = spawn_sim(
        world,
        (
            place(PlaceKind::House, 0),
            at(-6000, -5000),
            Approach(at(-6000, -2600)),
            Home,
        ),
    );
    let (bed, _) = spawn_sim(
        world,
        (
            place(PlaceKind::Bed, 90),
            at(-2500, -5500),
            Approach(at(-2500, -4000)),
            Bed { home: house },
            Provides {
                need: energy,
                amount_per_tick: BED_ENERGY_PER_TICK,
            },
        ),
    );
    let (stockpile, _) = spawn_sim(
        world,
        (
            place(PlaceKind::Stockpile, 0),
            at(5000, -4000),
            Approach(at(5000, -2800)),
            Stockpile {
                food: STOCKPILE_START_FOOD,
                capacity: STOCKPILE_CAPACITY,
            },
            Provides {
                need: hunger,
                amount_per_tick: STOCKPILE_HUNGER_PER_TICK,
            },
        ),
    );
    let (field, _) = spawn_sim(
        world,
        (
            place(PlaceKind::Field, 0),
            at(3000, 5000),
            Approach(at(3000, 2200)),
            WorkSite {
                work_ticks: FIELD_WORK_TICKS,
                yield_food: FIELD_YIELD_FOOD,
                stockpile,
            },
        ),
    );
    let bundle = villager_bundle(world, house, at(0, 0), tree);
    let (villager, _) = spawn_sim(world, bundle);

    VillageIds {
        house,
        bed,
        stockpile,
        field,
        villager,
        well: SimId(0),
    }
}

/// The well of the plot at `offset`.
fn spawn_well(world: &mut World, offset: SimPos) -> SimId {
    let thirst = need_id(world, THIRST);
    let (well, _) = spawn_sim(
        world,
        (
            Place {
                kind: PlaceKind::Well,
                yaw_deg: 0,
            },
            SimPos::new(-4500 + offset.x, 4500 + offset.z),
            Approach(SimPos::new(-4500 + offset.x, 3300 + offset.z)),
            Well { slots: WELL_SLOTS },
            Provides {
                need: thirst,
                amount_per_tick: WELL_THIRST_PER_TICK,
            },
        ),
    );
    well
}

/// Centre of each plot's `village_nav` wall, relative to the plot: cells
/// x = 2, z = -4..=3 (with 1 m cells), across the straight lines from the
/// villager's start to the stockpile and field approaches.
pub const NAV_WALL_CENTER: SimPos = SimPos::new(2500, 0);
/// Half length (z) of each plot's `village_nav` wall.
pub const NAV_WALL_HALF_MM: i32 = 3500;

/// [`spawn_village_n`] with a wall per plot and `NavAgent` villagers (the
/// `village_nav` scenario; the app needs `npc_nav`'s `NavPlugin`). The plots
/// get the same SimIds as in `village_n`; the walls come after them.
pub fn spawn_village_nav(world: &mut World, n: u32) -> Vec<VillageIds> {
    let plots = spawn_village_n(world, n);
    for plot in &plots {
        let index = world.resource::<SimIndex>();
        let (house, villager) = (index.get(plot.house), index.get(plot.villager));
        let home = house
            .and_then(|e| world.get::<SimPos>(e).copied())
            .expect("house spawned");
        // The house sits at (-6000, -5000) in its plot.
        let origin = SimPos::new(home.x + 6000, home.z + 5000);
        if let Some(v) = villager {
            world.entity_mut(v).insert(NavAgent);
        }
        spawn_sim(
            world,
            (
                SimPos::new(origin.x + NAV_WALL_CENTER.x, origin.z + NAV_WALL_CENTER.z),
                GridBlocker(Footprint {
                    half_x_mm: 0,
                    half_z_mm: NAV_WALL_HALF_MM,
                }),
            ),
        );
    }
    plots
}

/// Grid bounds (1 m cells) that hold `n` plots of [`spawn_village_n`] with a
/// 16 m margin on each side.
pub fn nav_bounds(n: u32) -> GridBounds {
    let root = n.isqrt();
    let side = if root * root < n { root + 1 } else { root }.max(1);
    let cells = side * (PLOT_SIZE as u32 / 1000) + 32;
    GridBounds::centered(cells, cells)
}

/// Names accepted by [`new_sim_app`].
pub const SCENARIOS: [&str; 3] = ["village", "village_n", "village_nav"];

/// The shared headless constructor (CLI, replays, benchmarks, tests): a bare
/// `App` with `NpcPlugins` (no driver) + `VillagePlugin`, content frozen
/// (the app itself is not finished, so plugins may still be added), the
/// scenario spawned, plus [`ScenarioInfo`]. It is the village's
/// replay `AppFactory` ([`try_new_sim_app`]). `"village"` is the iteration-1 layout
/// (`villagers` is ignored); `"village_n"` is [`spawn_village_n`] with
/// `villagers` plots; `"village_nav"` is [`spawn_village_nav`] with
/// `villagers` plots (at least 1) and `npc_nav`'s `NavPlugin`. Drive it with
/// `run_ticks(app.world_mut(), n)`.
///
/// # Panics
/// On an unknown scenario; see [`try_new_sim_app`].
pub fn new_sim_app(seed: u64, scenario: &str, villagers: u32) -> App {
    try_new_sim_app(seed, scenario, villagers).unwrap_or_else(|e| panic!("{e}"))
}

/// [`new_sim_app`] returning an error message for an unknown scenario.
pub fn try_new_sim_app(seed: u64, scenario: &str, villagers: u32) -> Result<App, String> {
    try_new_sim_app_with(seed, scenario, villagers, |_| {})
}

/// [`try_new_sim_app`] with `plugins` adding more plugins before content is
/// frozen (e.g. the agent-control plugins of [`crate::agent`]). With a no-op
/// `plugins` it is exactly [`try_new_sim_app`].
pub fn try_new_sim_app_with(
    seed: u64,
    scenario: &str,
    villagers: u32,
    plugins: impl FnOnce(&mut App),
) -> Result<App, String> {
    let villagers = match scenario {
        "village" => 1,
        "village_n" => villagers,
        "village_nav" => villagers.max(1),
        other => {
            return Err(format!(
                "unknown scenario `{other}` (expected one of {SCENARIOS:?})"
            ));
        }
    };
    let mut app = App::new();
    app.add_plugins((NpcPlugins::new().seed(seed).without_driver(), VillagePlugin));
    if scenario == "village_nav" {
        app.insert_resource(SpatialGrid::new(1000, nav_bounds(villagers)))
            .add_plugins(NavPlugin);
    }
    plugins(&mut app);
    // Freeze content now (spawning needs the tree and need ids) without
    // finishing the app, so callers can still add plugins (MinimalPlugins,
    // the journal, the remote interface); the plugins' own `finish` later
    // finds everything frozen.
    crate::finish_content(app.world_mut());
    let world = app.world_mut();
    match scenario {
        "village" => {
            spawn_village(world);
        }
        "village_nav" => {
            spawn_village_nav(world, villagers);
        }
        _ => {
            spawn_village_n(world, villagers);
        }
    }
    app.insert_resource(ScenarioInfo {
        name: scenario.to_owned(),
        villagers,
    });
    Ok(app)
}
