//! Inspect section: a vehicle's control mode, seats and occupants, whether
//! it can move; an actor's seat and drive goal. Read-only.

use bevy_ecs::prelude::*;
use npc_kernel::{SimId, SimIndex};
use serde_json::{Value, json};

use crate::drive::{DriveGoal, Driving, drive_goal};
use crate::seats::{can_move, driver_of, entity_of, seat_role};
use crate::vehicle::{MountedIn, Occupants, SeatSlot, Vehicle};

fn slot_json(slot: SeatSlot) -> Value {
    match slot {
        SeatSlot::Free => json!("free"),
        SeatSlot::Claimed(id) => json!({ "claimed_by": id }),
        SeatSlot::Taken(id) => json!({ "occupant": id }),
    }
}

/// The vehicle section of `entity` (`None` when it has nothing of it):
/// `{"vehicle":{"control":"ByCrew","speed_mm_per_tick":80,"seats":[{"seat":0,
/// "role":"Driver","required":true,"offset":{..},"state":{"occupant":5}}],
/// "occupants":[5],"driver":5,"can_move":true,"why_not":null},
/// "mounted_in":{"vehicle":9,"seat":0,"role":"Driver"},"drive_goal":{..}}`.
pub fn vehicle_json(world: &World, entity: Entity) -> Option<Value> {
    let mut map = serde_json::Map::new();
    if let Some(v) = world.get::<Vehicle>(entity) {
        let occ = world.get::<Occupants>(entity).cloned().unwrap_or_default();
        let seats: Vec<Value> = v
            .seats
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let seat = u8::try_from(i).unwrap_or(u8::MAX);
                json!({
                    "seat": seat,
                    "role": s.role.name(),
                    "required": s.required,
                    "offset": s.offset,
                    "state": slot_json(occ.slot(seat)),
                })
            })
            .collect();
        let movable = can_move(world, entity);
        map.insert(
            "vehicle".into(),
            json!({
                "control": v.control,
                "speed_mm_per_tick": v.speed_mm_per_tick,
                "seats": seats,
                "occupants": occ.units(),
                "driver": driver_of(world, entity).map(|(id, _)| id),
                "can_move": movable.is_ok(),
                "why_not": movable.err(),
            }),
        );
    }
    if let Some(m) = world.get::<MountedIn>(entity) {
        let role = entity_of(world, m.vehicle)
            .and_then(|v| seat_role(world, v, m.seat))
            .map(|r| r.name());
        map.insert(
            "mounted_in".into(),
            json!({ "vehicle": m.vehicle, "seat": m.seat, "role": role }),
        );
    }
    if world.get::<DriveGoal>(entity).is_some() || world.get::<Driving>(entity).is_some() {
        map.insert(
            "drive_goal".into(),
            json!(drive_goal(world, entity).map(|(g, _)| g)),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// [`vehicle_json`] by `SimId`.
pub fn vehicle_json_of(world: &World, id: SimId) -> Option<Value> {
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    vehicle_json(world, entity)
}
