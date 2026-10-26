//! Inspect section: a unit's owner, current order (with progress) and
//! queue, its squad; a squad's members. Read-only.

use bevy_ecs::prelude::*;
use npc_kernel::{SimId, SimIndex};
use serde_json::{Value, json};

use crate::types::{Commandable, ExecutingOrder, MemberOf, OrderQueue, Owner, Squad};

/// The command section of `entity` (`None` when it has nothing of it), e.g.
/// `{"owner":1,"current":{"order":{"type":"Move","pos":[0,0]},"started":4,
/// "progress":"Moving"},"queued":[..],"squad":12,"executing":true}`.
pub fn command_json(world: &World, entity: Entity) -> Option<Value> {
    let mut map = serde_json::Map::new();
    if let Some(o) = world.get::<Owner>(entity) {
        map.insert("owner".into(), json!(o.0));
    }
    if let Some(c) = world.get::<Commandable>(entity) {
        map.insert("speed_mm_per_tick".into(), json!(c.speed_mm_per_tick));
    }
    if let Some(q) = world.get::<OrderQueue>(entity) {
        map.insert(
            "current".into(),
            q.current.as_ref().map_or(Value::Null, |a| {
                json!({
                    "order": a.order.order,
                    "group": a.order.group,
                    "player": a.order.player,
                    "started": a.started,
                    "progress": a.progress,
                })
            }),
        );
        map.insert(
            "queued".into(),
            Value::Array(q.queued.iter().map(|o| json!(o.order)).collect()),
        );
        map.insert(
            "executing".into(),
            json!(world.get::<ExecutingOrder>(entity).is_some()),
        );
    }
    if let Some(m) = world.get::<MemberOf>(entity) {
        map.insert("squad".into(), json!(m.0));
    }
    if let Some(s) = world.get::<Squad>(entity) {
        map.insert("members".into(), json!(s.members));
        map.insert("formation".into(), json!(s.formation));
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// [`command_json`] by `SimId`.
pub fn command_json_of(world: &World, id: SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    command_json(world, entity)
}
