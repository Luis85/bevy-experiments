//! A compact, read-only summary of a testbed world (the `content_summary` of
//! `npcctl run --scenario testbed`): every actor's place, progression and
//! load, the doors, the items on the ground and the vehicles' crews.
//! Everything is ordered by `SimId` or by name; it only reads the world.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde_json::{Value, json};

use crate::world::{TestbedIds, TestbedUnit};

fn entity(world: &World, id: SimId) -> Option<Entity> {
    world.get_resource::<SimIndex>()?.get(id)
}

/// `{kind: count}` of the items `holder` carries (inventory and hands).
fn load(world: &World, holder: Entity) -> BTreeMap<String, u32> {
    let mut ids: Vec<SimId> = world
        .get::<Inventory>(holder)
        .map(|i| i.items.clone())
        .unwrap_or_default();
    if let Some(eq) = world.get::<Equipment>(holder) {
        ids.extend(eq.items());
    }
    ids.sort_unstable();
    ids.dedup();
    let mut out = BTreeMap::new();
    for id in ids {
        if let Some(item) = entity(world, id).and_then(|e| world.get::<Item>(e)) {
            *out.entry(item.kind.clone()).or_insert(0) += item.stack;
        }
    }
    out
}

/// The summary (see the module docs).
pub fn summary_json(world: &World) -> Value {
    let Some(ids) = world.get_resource::<TestbedIds>() else {
        return Value::Null;
    };
    let names: BTreeMap<SimId, &str> = ids.names.iter().map(|(n, &id)| (id, n.as_str())).collect();
    let mut units = Vec::new();
    for &id in &ids.units {
        let Some(e) = entity(world, id) else {
            continue;
        };
        let pos = world.get::<SimPos>(e).copied().unwrap_or_default();
        let exp = world.get::<Experience>(e).copied();
        let sheet = world.get::<CharacterSheet>(e);
        units.push(json!({
            "sim_id": id,
            "name": names.get(&id),
            "template": world.get::<TestbedUnit>(e).map(|u| u.template.clone()),
            "owner": world.get::<Owner>(e).map(|o| o.0.0),
            "pos": [pos.x, pos.z],
            "level": exp.map(|x| x.level),
            "xp": exp.map(|x| x.xp),
            "unspent": sheet.map(|s| s.unspent),
            "skills": sheet.map(|s| s.skills.clone()),
            "carries": load(world, e),
            "riding": world.get::<MountedIn>(e).map(|m| m.vehicle),
            "orders": world.get::<OrderQueue>(e).map_or(0, OrderQueue::len),
        }));
    }
    let mut doors = BTreeMap::new();
    let mut vehicles = BTreeMap::new();
    for (name, &id) in &ids.names {
        let Some(e) = entity(world, id) else {
            continue;
        };
        if let Some(d) = world.get::<Door>(e) {
            let state = match (d.open, d.is_locked()) {
                (true, _) => "open",
                (false, true) => "locked",
                (false, false) => "closed",
            };
            doors.insert(name.clone(), state);
        }
        if world.get::<Vehicle>(e).is_some() {
            let pos = world.get::<SimPos>(e).copied().unwrap_or_default();
            vehicles.insert(name.clone(), json!({"sim_id": id, "pos": [pos.x, pos.z]}));
        }
    }
    let mut ground: BTreeMap<String, u32> = BTreeMap::new();
    for &e in world.resource::<SimIndex>().map.values() {
        if world.get::<CarriedBy>(e).is_some() || world.get::<SimPos>(e).is_none() {
            continue;
        }
        if let Some(item) = world.get::<Item>(e) {
            *ground.entry(item.kind.clone()).or_insert(0) += item.stack;
        }
    }
    json!({
        "units": units,
        "doors": doors,
        "vehicles": vehicles,
        "ground_items": ground,
    })
}
