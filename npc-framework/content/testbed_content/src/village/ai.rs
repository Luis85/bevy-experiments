//! Autonomous production and survival, in sorted Effects order.
use super::{catalog, economy as econ, model::*};
use bevy_ecs::prelude::*;
use npc_framework::crafting::{CraftSource, Crafting, RecipeRegistry, begin_craft, cancel_craft};
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{Interactor, Item};
use npc_framework::prelude::*;
use serde_json::json;

pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let (day, season, night) = calendar(world);
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .map(|(e, id, v)| (*id, e, v.clone()))
        .collect();
    people.sort_by_key(|(id, _, _)| *id);
    for (id, e, v) in &people {
        vitals(world, *id, *e, v, now);
    }
    let mut patches: Vec<_> = world
        .query::<(Entity, &SimId, &ResourcePatch)>()
        .iter(world)
        .map(|(e, id, p)| (*id, e, p.clone()))
        .collect();
    patches.sort_by_key(|(id, _, _)| *id);
    for (_, e, mut p) in patches {
        if now >= p.replenish_at {
            let capacity = if p.kind == catalog::GRAIN {
                match season {
                    Season::Winter => p.capacity / 4,
                    Season::Summer => p.capacity,
                    _ => p.capacity * 3 / 4,
                }
            } else {
                p.capacity
            };
            p.remaining = capacity;
            p.replenish_at = now + world.resource::<SimConfig>().day_length.max(1);
            world.entity_mut(e).insert(p);
        }
    }
    finish_gathers(world, now);
    dividends(world, day);
    for (_, e, old) in people {
        if !living(world, e) || old.role == Role::Visitor {
            continue;
        }
        let v = world.get::<Villager>(e).expect("villager").clone();
        if now < v.next_decision
            || now < v.resting_until
            || world.get::<Gathering>(e).is_some()
            || world.get::<Crafting>(e).is_some()
        {
            continue;
        }
        world
            .get_mut::<Villager>(e)
            .expect("villager")
            .next_decision = now + 20;
        if satisfy(
            world,
            e,
            "Thirst",
            catalog::WATER,
            Role::WaterKeeper,
            55_000,
            90_000,
        ) {
            continue;
        }
        if satisfy(world, e, "Hunger", "bread", Role::Baker, 50_000, 80_000) {
            continue;
        }
        if econ::need(world, e, "Energy") < 30_000
            || (night && econ::need(world, e, "Energy") < 85_000)
        {
            if travel(world, e, v.home) {
                start_rest(world, e, 300, 400);
            }
            continue;
        }
        recover_found(world, e);
        if night {
            travel(world, e, v.home);
            continue;
        }
        match v.role {
            Role::Farmer => gather_for(world, e, catalog::GRAIN),
            Role::WaterKeeper => gather_for(world, e, catalog::WATER),
            Role::Woodcutter => gather_for(world, e, "log"),
            Role::Miner => gather_for(world, e, "ore"),
            Role::Miller | Role::Baker | Role::Carpenter | Role::Smith => manufacture(world, e, &v),
            Role::Innkeeper => {
                for (kind, target) in [("log", 4), (catalog::BOWL, 2), ("hammer", 1)] {
                    if econ::count(world, e, kind) < target {
                        buy_input(world, e, kind, target);
                        break;
                    }
                }
            }
            Role::Visitor => {}
        }
    }
}
fn vitals(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64) {
    let mut v = *world.get::<Vitality>(e).expect("vitality");
    if v.died_at.is_some() {
        return;
    }
    let hunger = econ::need(world, e, "Hunger");
    let thirst = econ::need(world, e, "Thirst");
    let energy = econ::need(world, e, "Energy");
    let rules = world
        .resource::<crate::world_runtime::ActiveWorld>()
        .0
        .clone();
    v.starved_ticks = if hunger <= 100_000 - rules.needs.hunger.out_milli {
        v.starved_ticks.saturating_add(1)
    } else {
        0
    };
    v.dehydrated_ticks = if thirst <= 100_000 - rules.needs.thirst.out_milli {
        v.dehydrated_ticks.saturating_add(1)
    } else {
        0
    };
    if v.starved_ticks > 600 {
        v.health = v.health.saturating_sub(40);
    }
    if v.dehydrated_ticks > 600 {
        v.health = v.health.saturating_sub(80);
    }
    if energy <= 100_000 - rules.needs.energy.out_milli && now >= person.resting_until {
        v.exhaustion_ticks = v.exhaustion_ticks.saturating_add(1);
        stop_work(world, e, "exhausted");
        start_rest(world, e, 1200, 40);
        record_effect(
            world,
            Some(id),
            "VillageExhausted",
            &json!({"recovery":"slow ground rest; hunger and thirst continue"}),
        );
    }
    if person.resting_until > now {
        world.entity_mut(e).remove::<MoveIntent>();
        econ::restore(world, e, "Energy", person.rest_rate);
        if hunger > 50_000 && thirst > 50_000 {
            v.health = (v.health + 10).min(100_000);
        }
    }
    let pct = [
        (energy, rules.needs.energy),
        (hunger, rules.needs.hunger),
        (thirst, rules.needs.thirst),
    ]
    .into_iter()
    .filter(|(value, spec)| 100_000 - *value >= spec.low_milli)
    .map(|(_, spec)| i32::from(spec.low_speed_pct))
    .min()
    .unwrap_or(100);
    let speed = if person.resting_until > now {
        0
    } else {
        person.base_speed * pct / 100
    };
    if let Some(base) = world.get::<VillageSight>(e) {
        let mut sensors = base.0.clone();
        if world.resource::<SimConfig>().is_night(now) {
            for s in &mut sensors.sensors {
                if let npc_framework::perception::Sensor::Vision(vision) = s {
                    vision.range_mm = (i64::from(vision.range_mm)
                        * i64::from(rules.day_night.night_vision_pct)
                        / 100) as i32;
                }
            }
        }
        world.entity_mut(e).insert(sensors);
    }
    if let Some(mut c) = world.get_mut::<Commandable>(e) {
        c.speed_mm_per_tick = speed;
    }
    if let Some(mut u) = world.get_mut::<crate::TestbedUnit>(e) {
        u.speed_mm_per_tick = speed;
    }
    if let Some(mut intent) = world.get_mut::<MoveIntent>(e) {
        intent.speed_mm_per_tick = speed;
    }
    if v.health <= 0 {
        v.health = 0;
        v.died_at = Some(now);
        world.entity_mut(e).insert(v);
        die(world, id, e, person, now);
    } else {
        world.entity_mut(e).insert(v);
    }
}
pub(crate) fn stop_work(world: &mut World, e: Entity, reason: &str) {
    npc_framework::agent_io::control::cancel_all(world, e);
    npc_framework::command::cancel_orders(world, e, reason);
    cancel_craft(world, e, reason);
    world.entity_mut(e).remove::<(Gathering, MoveIntent)>();
}
pub(crate) fn start_rest(world: &mut World, e: Entity, ticks: u64, rate: i32) {
    let now = world.resource::<SimClock>().tick;
    if let Some(mut v) = world.get_mut::<Villager>(e) {
        v.resting_until = now + ticks;
        v.rest_rate = rate;
    }
    world.entity_mut(e).remove::<MoveIntent>();
}
fn die(world: &mut World, id: SimId, e: Entity, person: &Villager, now: u64) {
    stop_work(world, e, "dead");
    let pos = world.get::<SimPos>(e).copied().unwrap_or_default();
    for item in items::carried_items(world, e) {
        if let Some(item) = entity_of(world, item) {
            items::place_in_world(world, item, pos);
        }
    }
    let coins = world.get_mut::<Wallet>(e).map_or(0, |mut w| {
        let c = w.coins;
        w.coins = 0;
        c
    });
    if coins > 0 {
        spawn_sim(
            world,
            (
                pos,
                Item::new(catalog::COINS),
                CoinPurse { coins },
                Perceivable::new(crate::world::kinds::ITEM),
            ),
        );
    }
    let diary = world.get::<VillageDiary>(e).cloned();
    let dropped = diary.map(|book| {
        spawn_sim(
            world,
            (
                pos,
                Item::new(catalog::DIARY),
                DroppedDiary {
                    author: id,
                    author_name: person.name.clone(),
                    life: person.life,
                    died_at: now,
                    pages: book.pages,
                    sealed: false,
                },
                Perceivable::new(crate::world::kinds::ITEM),
            ),
        )
        .0
    });
    world
        .entity_mut(e)
        .remove::<(BrainState, ActiveLeaf, CurrentOrder, MoveIntent, Interactor)>();
    if world.get::<Brain>(e).is_some() {
        let tree = world
            .resource::<AiRegistry>()
            .tree_id(super::REMAINS_TREE)
            .expect("registered remains tree");
        world.entity_mut(e).insert(Brain { tree });
    }
    world.entity_mut(e).remove::<(
        npc_framework::perception::RevealsFog,
        npc_framework::perception::ReachSense,
    )>();
    world
        .entity_mut(e)
        .insert(npc_framework::perception::Sensors::new());
    if let Some(mut c) = world.get_mut::<Commandable>(e) {
        c.speed_mm_per_tick = 0;
    }
    record_effect(
        world,
        Some(id),
        "VillageDeath",
        &json!({"permanent":true,"diary":dropped,"pos":[pos.x,pos.z],"coins_dropped":coins}),
    );
}
pub(crate) fn travel(world: &mut World, e: Entity, pos: SimPos) -> bool {
    let here = world.get::<SimPos>(e).copied().unwrap_or_default();
    if here.dist2(pos) <= 640_000 {
        world.entity_mut(e).remove::<MoveIntent>();
        return true;
    }
    let speed = world
        .get::<Commandable>(e)
        .map_or(70, |c| c.speed_mm_per_tick);
    let mut intent = MoveIntent::to(pos, speed);
    intent.tolerance_mm = 600;
    if world.get::<MoveIntent>(e) != Some(&intent) {
        world.entity_mut(e).insert(intent);
    }
    false
}
fn resident(world: &mut World, role: Role, kind: &str) -> Option<Entity> {
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .filter(|(e, _, v)| v.role == role && living(world, *e) && econ::count(world, *e, kind) > 0)
        .map(|(e, id, _)| (*id, e))
        .collect();
    people.sort_by_key(|(id, _)| *id);
    people.first().map(|(_, e)| *e)
}
fn satisfy(
    world: &mut World,
    e: Entity,
    need: &str,
    good: &str,
    seller: Role,
    threshold: i32,
    amount: i32,
) -> bool {
    if econ::need(world, e, need) >= threshold {
        return false;
    }
    if econ::consume(world, e, good, 1).is_ok() {
        econ::restore(world, e, need, amount);
        return true;
    }
    if let Some(s) = resident(world, seller, good) {
        if world.get::<Wallet>(e).map_or(0, |w| w.coins) < catalog::price(good).unwrap_or(u32::MAX)
        {
            return false;
        }
        let pos = world.get::<SimPos>(s).copied().unwrap_or_default();
        if travel(world, e, pos) {
            return econ::trade(world, e, s, good, 1).is_ok();
        }
        return true;
    }
    // Supply can be empty. Producers must be able to restart that chain while hungry/thirsty.
    false
}
fn gather_for(world: &mut World, e: Entity, kind: &str) {
    if econ::count(world, e, kind) >= catalog::stock_limit(kind) {
        return;
    }
    let mut patches: Vec<_> = world
        .query::<(Entity, &SimId, &ResourcePatch, &SimPos)>()
        .iter(world)
        .filter(|(_, _, p, _)| p.kind == kind && p.remaining > 0)
        .map(|(e, id, _, p)| (*id, e, *p))
        .collect();
    patches.sort_by_key(|(id, _, _)| *id);
    if let Some((id, p, pos)) = patches.first().copied()
        && travel(world, e, pos)
    {
        let _ = begin_gather(world, e, p, id);
    }
}
pub(crate) fn begin_gather(
    world: &mut World,
    e: Entity,
    patch: Entity,
    id: SimId,
) -> Result<(), String> {
    let p = world
        .get::<ResourcePatch>(patch)
        .ok_or("not a resource patch")?;
    if !living(world, e) || !econ::near(world, e, patch) {
        return Err("move within 1.5 m of the resource patch".into());
    }
    if p.remaining == 0 {
        return Err("depleted until the next day".into());
    }
    if world.get::<Gathering>(e).is_some() || world.get::<Crafting>(e).is_some() {
        return Err("already working".into());
    }
    econ::fit(world, e, &[Item::new(&p.kind)])?;
    let ends = world.resource::<SimClock>().tick + u64::from(p.gather_ticks);
    world
        .entity_mut(e)
        .remove::<MoveIntent>()
        .insert(Gathering { patch: id, ends });
    Ok(())
}
fn finish_gathers(world: &mut World, now: u64) {
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Gathering)>()
        .iter(world)
        .filter(|(_, _, g)| g.ends <= now)
        .map(|(e, id, g)| (*id, e, *g))
        .collect();
    people.sort_by_key(|(id, _, _)| *id);
    for (id, e, g) in people {
        world.entity_mut(e).remove::<Gathering>();
        if !living(world, e) {
            continue;
        }
        let Some(patch) = entity_of(world, g.patch) else {
            continue;
        };
        let Some(p) = world.get::<ResourcePatch>(patch).cloned() else {
            continue;
        };
        if p.remaining == 0
            || !econ::near(world, e, patch)
            || econ::fit(world, e, &[Item::new(&p.kind)]).is_err()
        {
            continue;
        }
        world
            .get_mut::<ResourcePatch>(patch)
            .expect("patch")
            .remaining -= 1;
        econ::grant(world, e, Item::new(&p.kind));
        econ::produce(world, &p.kind, 1);
        record_effect(
            world,
            Some(id),
            "VillageGathered",
            &json!({"kind":p.kind,"patch":g.patch,"quantity":1}),
        );
    }
}
fn buy_input(world: &mut World, e: Entity, kind: &str, wanted: u32) -> bool {
    let mut suppliers: Vec<_> = world
        .query::<(Entity, &SimId, &Villager, &SimPos)>()
        .iter(world)
        .filter(|(s, _, v, _)| {
            *s != e
                && v.role != Role::Visitor
                && catalog::outputs(v.role).contains(&kind)
                && living(world, *s)
                && econ::count(world, *s, kind) > 0
        })
        .map(|(s, id, _, p)| (*id, s, *p))
        .collect();
    suppliers.sort_by_key(|(id, _, _)| *id);
    let Some((_, seller, pos)) = suppliers.first().copied() else {
        return false;
    };
    if travel(world, e, pos) {
        let available = econ::count(world, seller, kind);
        let coins = world.get::<Wallet>(e).map_or(0, |w| w.coins);
        let n = wanted
            .saturating_sub(econ::count(world, e, kind))
            .min(available)
            .min(8)
            .min(coins / catalog::price(kind).unwrap_or(1));
        if n > 0 {
            let _ = econ::trade(world, e, seller, kind, n);
        }
    }
    true
}
fn manufacture(world: &mut World, e: Entity, v: &Villager) {
    let secondary = match v.role {
        Role::Carpenter => {
            econ::count(world, e, "plank") >= 8 && econ::count(world, e, catalog::BOWL) < 6
        }
        Role::Smith => econ::count(world, e, "ingot") >= 4 && econ::count(world, e, "hammer") < 3,
        _ => false,
    };
    let Some(recipe) = catalog::recipe(v.role, secondary) else {
        return;
    };
    let def = world
        .resource::<RecipeRegistry>()
        .recipes
        .get(recipe)
        .expect("village recipe")
        .clone();
    if def
        .outputs
        .iter()
        .all(|o| econ::count(world, e, &o.kind) >= catalog::stock_limit(&o.kind))
    {
        return;
    }
    for input in &def.inputs {
        if econ::count(world, e, &input.key) < input.count {
            buy_input(world, e, &input.key, input.count.max(4));
            return;
        }
    }
    let Some(station) = entity_of(world, v.workplace) else {
        return;
    };
    let pos = world.get::<SimPos>(station).copied().unwrap_or_default();
    if travel(world, e, pos) {
        let _ = begin_craft(
            world,
            e,
            recipe,
            Some(v.workplace),
            CraftSource::Command,
            None,
        );
    }
}
fn dividends(world: &mut World, day: u64) {
    if day <= world.resource::<VillageState>().last_dividend_day {
        return;
    }
    let mut people: Vec<_> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .filter(|(e, _, v)| v.role != Role::Visitor && living(world, *e))
        .map(|(e, id, _)| (*id, e))
        .collect();
    people.sort_by_key(|(id, _)| *id);
    let mut fund = world.resource::<VillageState>().fund;
    let mut contributed = 0;
    let mut paid = 0;
    for (_, e) in &people {
        let mut w = world.get_mut::<Wallet>(*e).expect("wallet");
        let excess = w.coins.saturating_sub(NPC_SEED_COINS);
        w.coins -= excess;
        fund += excess;
        contributed += excess;
    }
    for (_, e) in &people {
        let mut w = world.get_mut::<Wallet>(*e).expect("wallet");
        let grant = 100_u32.saturating_sub(w.coins).min(fund);
        w.coins += grant;
        fund -= grant;
        paid += grant;
    }
    for (_, e) in &people {
        if world
            .get::<Villager>(*e)
            .is_some_and(|v| v.role == Role::Innkeeper)
        {
            let _ = econ::consume(world, *e, "log", 1);
            if day.is_multiple_of(4) {
                let _ = econ::consume(world, *e, catalog::BOWL, 1);
            }
            if day.is_multiple_of(7) {
                let _ = econ::consume(world, *e, "hammer", 1);
            }
        }
    }
    let mut state = world.resource_mut::<VillageState>();
    state.fund = fund;
    state.last_dividend_day = day;
    record_effect(
        world,
        None,
        "VillageCommunityFund",
        &json!({"day":day,"contributed":contributed,"paid":paid,"balance":fund}),
    );
}
fn recover_found(world: &mut World, e: Entity) {
    let mut lost: Vec<_> = world
        .query::<(Entity, &SimId, &Item)>()
        .iter(world)
        .filter(|(item, _, _)| {
            world
                .get::<npc_framework::interaction::CarriedBy>(*item)
                .is_none()
                && (world.get::<DroppedDiary>(*item).is_some()
                    || world.get::<CoinPurse>(*item).is_some())
                && econ::near(world, e, *item)
        })
        .map(|(e, id, _)| (*id, e))
        .collect();
    lost.sort_by_key(|(id, _)| *id);
    if let Some((_, item)) = lost.first().copied() {
        if let Some(purse) = world.get::<CoinPurse>(item).copied() {
            if let Some(mut wallet) = world.get_mut::<Wallet>(e)
                && let Some(coins) = wallet.coins.checked_add(purse.coins)
            {
                wallet.coins = coins;
                world.despawn(item);
            }
        } else if items::can_store(world, e, item).is_ok() {
            items::put_in_inventory(world, e, item);
        }
    }
}
