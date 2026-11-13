//! Shared helpers of the coastal-world life tests (demography, fields,
//! weather, merchant boat). Every test edits the shipped world as JSON only.
#![allow(dead_code)]
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::TestbedIds;
use testbed_content::village::{
    self, CoinPurse, VillageState, Wallet, civic,
    frontier::{labor::ResidentProfile, settlements},
};

pub const SEED: u64 = 7;

/// The shipped scenario with its world document edited as JSON: `world` is
/// the whole world, so `world["day_night"]` and `world["living_world"]` are
/// both in reach.
pub fn world_for(seed: u64, edit: impl FnOnce(&mut Value)) -> App {
    let mut doc = testbed_content::scenario::scenario_doc("living_world").expect("scenario");
    edit(&mut doc.world);
    // The day length is not part of a scenario's world: `short_days` leaves a
    // note here and the built world gets it before its first tick.
    let day = doc
        .world
        .as_object_mut()
        .and_then(|world| world.remove(DAY_TICKS))
        .and_then(|ticks| ticks.as_u64());
    let mut app = testbed_content::scenario::load_scenario(&doc, seed)
        .map(|loaded| loaded.app)
        .unwrap_or_else(|error| panic!("{error}"));
    if let Some(ticks) = day {
        app.world_mut().resource_mut::<SimConfig>().day_length = ticks;
    }
    app
}

const DAY_TICKS: &str = "test_day_ticks";

pub fn world_with(edit: impl FnOnce(&mut Value)) -> App {
    world_for(SEED, edit)
}

/// A short day, so daily mechanics run within a test.
pub fn short_days(world: &mut Value, ticks: u64) {
    world[DAY_TICKS] = json!(ticks);
}

/// The `economy` block of the world document.
pub fn economy(world: &mut Value) -> &mut Value {
    &mut world["living_world"]["economy"]
}

/// Every day is `kind`: `clear`, `rain`, `storm` or `snow`.
pub fn only_weather(world: &mut Value, kind: &str) {
    for season in ["spring", "summer", "autumn", "winter"] {
        let mut odds = json!({"clear": 0, "rain": 0, "storm": 0, "snow": 0});
        odds[kind] = json!(1);
        economy(world)["weather"][season] = odds;
    }
    for shock in [
        "drought_permille",
        "blight_permille",
        "harsh_winter_permille",
    ] {
        economy(world)["weather"][shock] = json!(0);
    }
}

pub fn settlement(world: &World, key: &str) -> SimId {
    world
        .resource::<TestbedIds>()
        .id(&format!("settlement_{key}"))
}

pub fn entity(world: &World, id: SimId) -> Entity {
    world.resource::<SimIndex>().get(id).expect("entity")
}

pub fn id_of(world: &World, entity: Entity) -> SimId {
    *world.get::<SimId>(entity).expect("identity")
}

/// Residents of one settlement, dead or alive, ascending by identity.
pub fn residents(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world.get::<ResidentProfile>(**entity).is_some()
                && settlements::member(world, **entity) == Some(settlement)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

pub fn all_residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

/// Every coin of the closed system: wallets, dropped purses, the civic fund
/// and the merchant's purse.
pub fn money(world: &World) -> u64 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(world.get::<Wallet>(*entity).map_or(0, |w| w.coins))
                + u64::from(world.get::<CoinPurse>(*entity).map_or(0, |p| p.coins))
        })
        .sum::<u64>()
        + u64::from(world.resource::<VillageState>().fund)
        + u64::from(village::merchant::purse(world))
        // Coins a played character holds in escrow for a homestead upgrade.
        + village::live::escrow(world)
}

/// Every unit of a good on the island: packs, the ground, stores, building
/// sites and seed kept in fields.
pub fn units(world: &mut World, kind: &str) -> u32 {
    let items: u32 = world
        .query::<&Item>()
        .iter(world)
        .filter(|item| item.kind == kind)
        .map(|item| item.stack)
        .sum();
    items + civic::goods_in_custody(world, kind)
}

/// Moves existing coins from residents into the civic fund, as a levy would.
pub fn fund(world: &mut World, coins: u32) {
    let mut left = coins;
    for (_, actor) in all_residents(world) {
        if left == 0 {
            break;
        }
        let part = world
            .get::<Wallet>(actor)
            .map_or(0, |wallet| wallet.coins.saturating_sub(100))
            .min(left)
            .min(40);
        world.get_mut::<Wallet>(actor).expect("wallet").coins -= part;
        world.resource_mut::<VillageState>().fund += part;
        left -= part;
    }
    assert_eq!(left, 0, "residents could not fund {coins} coins");
}

pub fn teleport(world: &mut World, actor: Entity, to: SimPos) {
    // A test that moves somebody wants it awake: a late riser may still be
    // in bed in the first minutes of the day.
    if let Some(mut person) = world.get_mut::<village::Villager>(actor) {
        person.resting_until = 0;
    }
    world.entity_mut(actor).remove::<MoveIntent>().insert(to);
}

/// Applied journal effects of one type, oldest first.
pub fn effects(world: &World, kind: &str) -> Vec<Value> {
    world
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some(kind) => {
                Some(serde_json::to_value(effect).expect("effect JSON"))
            }
            _ => None,
        })
        .collect()
}

/// Runs until `done` holds, in steps of `step` ticks, for at most `limit`
/// ticks. Returns whether it held.
pub fn until(app: &mut App, limit: u64, step: u64, mut done: impl FnMut(&mut App) -> bool) -> bool {
    let end = app.world().resource::<SimClock>().tick + limit;
    loop {
        if done(app) {
            return true;
        }
        if app.world().resource::<SimClock>().tick >= end {
            return false;
        }
        run_ticks(app.world_mut(), step);
    }
}
