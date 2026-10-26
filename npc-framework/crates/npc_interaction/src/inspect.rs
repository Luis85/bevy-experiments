//! Inspect section: what a unit carries, wears and is doing, and what a
//! door or item is. Read-only.

use bevy_ecs::prelude::*;
use npc_kernel::{SimId, SimIndex};
use serde_json::{Value, json};

use crate::door::Door;
use crate::items::{
    CarriedBy, Equipment, Inventory, Item, carried_weight, entity_of, item_weight, kind_def,
};
use crate::run::Interacting;
use crate::targets::{Interactable, ToolTarget};

/// One item as JSON: `{"id":5,"kind":"axe","stack":1,"quality":0,
/// "weight_g":1500,"tags":["tool:axe"],"slot":"main_hand"}`.
pub fn item_json(world: &World, id: SimId) -> Value {
    let Some(e) = entity_of(world, id) else {
        return json!({ "id": id, "gone": true });
    };
    let Some(item) = world.get::<Item>(e) else {
        return json!({ "id": id });
    };
    let tags = kind_def(world, &item.kind).map_or_else(Vec::new, |d| d.tags.clone());
    json!({
        "id": id,
        "kind": item.kind,
        "stack": item.stack,
        "quality": item.quality,
        "weight_g": item_weight(world, item),
        "tags": tags,
        "slot": world.get::<CarriedBy>(e).and_then(|c| c.slot.clone()),
    })
}

/// The interaction section of `entity` (`None` when it has nothing of it).
pub fn interaction_json(world: &World, entity: Entity) -> Option<Value> {
    let mut map = serde_json::Map::new();
    if let Some(inv) = world.get::<Inventory>(entity) {
        map.insert(
            "inventory".into(),
            json!({
                "slots": inv.slots,
                "used": inv.items.len(),
                "max_weight_g": inv.max_weight_g,
                "weight_g": carried_weight(world, entity),
                "items": inv.items.iter().map(|&id| item_json(world, id)).collect::<Vec<_>>(),
            }),
        );
    }
    if let Some(eq) = world.get::<Equipment>(entity) {
        map.insert("equipment".into(), json!(eq.slots));
    }
    if let Some(i) = world.get::<Interacting>(entity) {
        map.insert("interacting".into(), json!(i));
    }
    if let Some(d) = world.get::<Door>(entity) {
        map.insert("door".into(), json!(d));
    }
    if let Some(&id) = world.get::<SimId>(entity)
        && world.get::<Item>(entity).is_some()
    {
        map.insert("item".into(), item_json(world, id));
        if let Some(c) = world.get::<CarriedBy>(entity) {
            map.insert("carried_by".into(), json!(c));
        }
    }
    if let Some(t) = world.get::<ToolTarget>(entity) {
        map.insert("tool_target".into(), json!(t));
    }
    if let Some(i) = world.get::<Interactable>(entity) {
        map.insert("verbs".into(), json!(i.verbs));
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// [`interaction_json`] by `SimId`.
pub fn interaction_json_of(world: &World, id: SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    interaction_json(world, entity)
}
