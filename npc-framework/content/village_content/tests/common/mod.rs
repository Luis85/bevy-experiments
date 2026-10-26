//! Helpers shared by the debug-tooling integration tests.
#![allow(dead_code, unused_imports)]

pub use bevy_app::App;
pub use bevy_ecs::prelude::*;
pub use npc_framework::ai::{AiRegistry, BrainState};
pub use npc_framework::kernel::commands::{SimCommand, submit};
pub use npc_framework::kernel::orders::{CurrentOrder, Order, OrderKind};
pub use npc_framework::prelude::{
    NeedId, NeedRegistry, Needs, Reservations, SimClock, SimId, SimIndex, SimPos, run_ticks,
};
pub use village_content::needs::{ENERGY, HUNGER, VillagerNeeds, need_id};
pub use village_content::replay::new_sim_app;
pub use village_content::scenario::VillageIds;
pub use village_content::villager::VillagerTree;
pub use village_content::{SetFood, SpawnVillager, Stockpile};

pub fn village(seed: u64) -> App {
    new_sim_app(seed, "village", 1)
}

pub fn ids(app: &App) -> VillageIds {
    *app.world().resource::<VillageIds>()
}

/// The `SimId` the next spawn gets (a `SpawnVillager` command, a runtime
/// spawn). Read it instead of hard-coding `SimId(6)`: scenario entities are
/// numbered first, so a new scenario entity moves every runtime id up
/// (`.claude/rules/content.md`, "SimId stability").
pub fn next_sim_id(app: &App) -> SimId {
    let next = app
        .world()
        .get_resource::<npc_framework::kernel::ids::SimIdAllocator>()
        .map_or(1, |a| a.next.max(1));
    SimId(next)
}

pub fn ent(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("sim entity exists")
}

pub fn tick(app: &App) -> u64 {
    app.world().resource::<SimClock>().tick
}

/// Jumps the clock (only for tests that do not record replays).
pub fn set_tick(app: &mut App, tick: u64) {
    app.world_mut().insert_resource(SimClock { tick });
}

/// Running tree path of `id`, as the overlay shows it.
pub fn path(app: &App, id: SimId) -> String {
    let state = app
        .world()
        .get::<BrainState>(ent(app, id))
        .expect("agent has BrainState");
    let tree = app.world().resource::<VillagerTree>().0;
    app.world()
        .resource::<AiRegistry>()
        .tree(tree)
        .path_string(&state.running_path)
}

pub fn pos(app: &App, id: SimId) -> SimPos {
    *app.world().get::<SimPos>(ent(app, id)).unwrap()
}

pub fn food(app: &App) -> u32 {
    let ids = ids(app);
    app.world()
        .get::<Stockpile>(ent(app, ids.stockpile))
        .unwrap()
        .food
}

/// `id`'s hunger and energy.
pub fn needs(app: &App, id: SimId) -> VillagerNeeds {
    let n = app.world().get::<Needs>(ent(app, id)).unwrap();
    VillagerNeeds::of(app.world(), n).expect("villager needs")
}

pub fn order(app: &App, id: SimId) -> Option<OrderKind> {
    app.world().get::<CurrentOrder>(ent(app, id)).unwrap().0
}

/// Submits `cmd` (panics when rejected).
pub fn send(app: &mut App, at: Option<u64>, cmd: SimCommand) -> (u64, u32) {
    submit(app.world_mut(), at, cmd.clone())
        .unwrap_or_else(|e| panic!("command {cmd:?} rejected: {e}"))
}

pub fn set_food(app: &mut App, food: u32) {
    let stockpile = ids(app).stockpile.0;
    send(app, None, SimCommand::content(&SetFood { stockpile, food }));
}

pub fn set_hunger(app: &mut App, value: i32) {
    let unit = ids(app).villager.0;
    send(
        app,
        None,
        SimCommand::SetNeed {
            unit,
            need: HUNGER.into(),
            value,
        },
    );
}

/// Runs one tick at a time until `pred` holds (checked after each tick).
/// Returns the number of ticks run, or `None` when `max_ticks` ran out.
pub fn run_until(app: &mut App, max_ticks: u64, mut pred: impl FnMut(&App) -> bool) -> Option<u64> {
    for i in 1..=max_ticks {
        run_ticks(app.world_mut(), 1);
        if pred(app) {
            return Some(i);
        }
    }
    None
}
