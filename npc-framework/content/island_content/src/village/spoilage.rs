//! Food spoils (review fix S2.4): a good with `shelf_life_days` loses a
//! share of every stack once a day, in packs and on store shelves.
//!
//! The loss is proportional with a seeded remainder, no per-item age: with
//! `D` = shelf life in day-percent (`shelf_life_days * 100` in a pack,
//! `* tuning.storage_shelf_percent` on a shelf), a holder of `n` units loses
//! `n * 100 / D` and one more with probability `(n * 100 % D) / D`. Holders in
//! order: living actors by `SimId`, then depots by `SimId`; goods by key.
//! Reserved units (a buyer's hold on a shelf) never spoil.
use super::civic::{CivicState, Depot};
use super::economy as econ;
use bevy_ecs::prelude::*;
use npc_framework::kernel::rng::sim_range;
use npc_framework::prelude::*;
use serde_json::json;

/// `sim_rand` purpose of the spoilage remainder (id = holder, tick field =
/// `day * 256 + good index`).
const SPOILAGE: u32 = 0x5350_4f31;

/// Units of `count` lost today with mean life `life_percent` day-percent.
fn lost(seed: u64, holder: SimId, day: u64, index: u64, count: u32, life_percent: u64) -> u32 {
    if count == 0 || life_percent == 0 {
        return 0;
    }
    let scaled = u64::from(count) * 100;
    let whole = scaled / life_percent;
    let rest = scaled % life_percent;
    let extra = u64::from(
        rest > 0
            && (sim_range(
                seed,
                holder,
                day.saturating_mul(256).saturating_add(index),
                SPOILAGE,
                0,
                life_percent.min(i32::MAX as u64) as i32,
            ) as u64)
                < rest,
    );
    (whole + extra).min(u64::from(count)) as u32
}

/// The daily pass, at the first tick of each day.
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let day_length = world.resource::<SimConfig>().day_length.max(1);
    if now == 0 || !now.is_multiple_of(day_length) {
        return;
    }
    let day = now / day_length;
    let Some(registry) = super::frontier::economy_spec::registry(world) else {
        return;
    };
    let store_percent = u64::from(registry.tuning().storage_shelf_percent);
    // Perishable goods with their index in the catalog sorted by key.
    let perishable: Vec<(u64, &'static str, u64)> = registry
        .trade_kinds()
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            let life = registry.spec.good(kind)?.shelf_life_days?;
            Some((index as u64, *kind, u64::from(life)))
        })
        .collect();
    if perishable.is_empty() {
        return;
    }
    let seed = world.resource::<WorldSeed>().0;
    // Packs of the living, by identity.
    let mut people: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &super::Villager)>()
        .iter(world)
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    for (id, actor) in people {
        if !super::living(world, actor) {
            continue;
        }
        for (index, kind, life) in &perishable {
            let count = econ::count(world, actor, kind);
            let gone = lost(seed, id, day, *index, count, life * 100);
            if gone > 0 && econ::consume(world, actor, kind, gone).is_ok() {
                super::ledger::spoiled(world, kind, gone);
                record_effect(
                    world,
                    Some(id),
                    "VillageSpoiled",
                    &json!({"holder":id,"good":kind,"units":gone}),
                );
            }
        }
    }
    // Store shelves, by identity: free units only, owners in identity order.
    let depots: Vec<SimId> = world
        .get_resource::<CivicState>()
        .map(|civic| {
            let mut ids: Vec<SimId> = civic.depots.values().flatten().copied().collect();
            ids.sort_unstable();
            ids
        })
        .unwrap_or_default();
    for depot_id in depots {
        let Some(entity) = world.resource::<SimIndex>().get(depot_id) else {
            continue;
        };
        for (index, kind, life) in &perishable {
            let Some(free) = world
                .get::<Depot>(entity)
                .map(|depot| depot.available(kind, None, now))
            else {
                continue;
            };
            let gone = lost(seed, depot_id, day, *index, free, life * store_percent);
            if gone == 0 {
                continue;
            }
            if let Some(mut depot) = world.get_mut::<Depot>(entity) {
                let mut left = gone;
                if let Some(lots) = depot.lots.get_mut(*kind) {
                    for units in lots.values_mut() {
                        let take = left.min(*units);
                        *units -= take;
                        left -= take;
                        if left == 0 {
                            break;
                        }
                    }
                    lots.retain(|_, units| *units > 0);
                }
                if depot.lots.get(*kind).is_some_and(|lots| lots.is_empty()) {
                    depot.lots.remove(*kind);
                }
            }
            super::ledger::spoiled(world, kind, gone);
            record_effect(
                world,
                None,
                "VillageSpoiled",
                &json!({"holder":depot_id,"good":kind,"units":gone}),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::lost;
    use npc_framework::prelude::SimId;

    #[test]
    fn the_loss_is_the_share_of_a_day_with_a_seeded_remainder() {
        // 1000 loaves, 20 days: 50 a day exactly.
        assert_eq!(lost(7, SimId(3), 4, 1, 1000, 2000), 50);
        // Twice as long on a shelf.
        assert_eq!(lost(7, SimId(3), 4, 1, 1000, 4000), 25);
        // Never more than the stack, nothing from nothing.
        assert_eq!(lost(7, SimId(3), 4, 1, 3, 1), 3);
        assert_eq!(lost(7, SimId(3), 4, 1, 0, 2000), 0);
        // One loaf of a 20-day food goes on about one day in twenty.
        let days = (0..2000_u64)
            .filter(|day| lost(7, SimId(3), *day, 1, 1, 2000) == 1)
            .count();
        assert!((60..=140).contains(&days), "{days}");
    }
}
