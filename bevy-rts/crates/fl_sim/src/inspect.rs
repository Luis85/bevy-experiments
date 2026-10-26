//! JSON views of sim state for agents and tools (spec §15 C): `fl_cli inspect`, BRP.
//! Observe-only; nothing here feeds back into the sim.

use bevy_ecs::world::World;
use fl_protocol::SimId;
use serde_json::{Map, Value, json};

use crate::build::MatchHeaderRes;
use crate::classify::StateRegistry;
use crate::clock::SimClock;
use crate::commands::last_seqs;
use crate::hash::compute_state_hash;
use crate::ids::SimIndex;
use crate::map::MapInfo;
use crate::players::Players;
use crate::rng::MatchSeed;

/// `{"sim_id": n, "components": {name: value}}` with every `Hashed` component the entity has,
/// keyed by its registry name (`"core.pose"`), in name order; `None` if no entity has `id`.
pub fn inspect_entity(world: &World, id: SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    let mut components = Map::new();
    if let Some(state) = world.get_resource::<StateRegistry>() {
        for (_, name, entry) in state.components.iter() {
            if let Some(value) = entry.codec.as_ref().and_then(|c| c.dump(world, entity)) {
                components.insert(name.to_owned(), value);
            }
        }
    }
    Some(json!({ "sim_id": id.0, "components": components }))
}

/// A summary of the world: `{"tick", "seed", "header": {"features", "mods", "rules_hash"},
/// "state_hash", "entities": [sim ids], "players", "map", "last_seq"}`.
/// `map` is `null` before a map is set up. `last_seq` maps each player id (as a string) to
/// the highest command seq the sim took from it ([`last_seqs`]), which a client needs to
/// restore its command stamper.
///
/// # Panics
/// As [`compute_state_hash`] does, on a world that was not built by
/// [`build_sim_app`](crate::build_sim_app).
pub fn inspect_world(world: &World) -> Value {
    let entities: Vec<u32> = world.resource::<SimIndex>().ids().map(|id| id.0).collect();
    let players = world
        .get_resource::<Players>()
        .map_or(Value::Array(Vec::new()), |p| to_json(&p.list));
    let map = world.get_resource::<MapInfo>().map_or(Value::Null, to_json);
    let header = &world.resource::<MatchHeaderRes>().0;
    json!({
        "tick": world.resource::<SimClock>().tick.0,
        "seed": world.resource::<MatchSeed>().0,
        "header": {
            "features": to_json(&header.features),
            "mods": to_json(&header.mods),
            "rules_hash": header.rules_hash,
        },
        "state_hash": compute_state_hash(world),
        "entities": entities,
        "players": players,
        "map": map,
        "last_seq": to_json(&last_seqs(world)),
    })
}

fn to_json(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value)
        .unwrap_or_else(|err| Value::String(format!("<cannot serialize to JSON: {err}>")))
}
