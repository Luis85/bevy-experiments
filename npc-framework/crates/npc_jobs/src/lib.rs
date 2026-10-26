//! Smart objects and reservations.
//!
//! Places *advertise* what they offer ([`Provides`]) and where an agent
//! stands to use them ([`Approach`]); agents *claim* them in [`Reservations`]
//! so two agents never use the same bed, field or food unit. Claims are part
//! of the state hash, released synchronously when a branch ends, and dropped
//! automatically when the claimant or the target leaves the sim. Every claim
//! attempt and every claim dropped is journaled (`Reserve` / `Release`).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use std::collections::BTreeMap;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use npc_kernel::hash::Fnv64;
use npc_kernel::journal::{self, JournalKind, ReleaseReason, record_deferred};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::snapshot::{ResourceSnapshotFns, SnapshotAppExt};
use npc_kernel::{SimHashResource, SimId, SimPos};
use npc_needs::NeedId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Smart-object advertisement: using this place restores `need` by
/// `amount_per_tick` while the matching action runs.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Provides {
    pub need: NeedId,
    pub amount_per_tick: i32,
}

/// Where an agent stands to use this place.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Approach(pub SimPos);

/// Where an agent stands to use `place`: its `Approach`, else its `SimPos`.
pub fn approach_of(world: &World, place: Entity) -> Option<SimPos> {
    world
        .get::<Approach>(place)
        .map(|a| a.0)
        .or_else(|| world.get::<SimPos>(place).copied())
}

/// Claims on targets. Invariant: no target maps to an empty claimant list
/// (released targets are removed), so equal claim sets compare and hash equal.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Reservations {
    /// target → claimants in claim order.
    pub claims: BTreeMap<SimId, Vec<SimId>>,
}

impl Reservations {
    /// Claims `target` for `by` if fewer than `capacity` claimants hold it.
    /// Re-claiming by an existing claimant returns true without duplicating.
    pub fn try_reserve(&mut self, target: SimId, by: SimId, capacity: u32) -> bool {
        if self.holds(target, by) {
            return true;
        }
        if self.count(target) >= capacity {
            return false;
        }
        self.claims.entry(target).or_default().push(by);
        true
    }

    pub fn release(&mut self, target: SimId, by: SimId) {
        if let Some(claimants) = self.claims.get_mut(&target) {
            claimants.retain(|&c| c != by);
            if claimants.is_empty() {
                self.claims.remove(&target);
            }
        }
    }

    /// Releases every claim held by `by`.
    pub fn release_all(&mut self, by: SimId) {
        self.claims.retain(|_, claimants| {
            claimants.retain(|&c| c != by);
            !claimants.is_empty()
        });
    }

    pub fn count(&self, target: SimId) -> u32 {
        self.claims
            .get(&target)
            .map_or(0, |v| u32::try_from(v.len()).unwrap_or(u32::MAX))
    }

    pub fn holds(&self, target: SimId, by: SimId) -> bool {
        self.claims.get(&target).is_some_and(|v| v.contains(&by))
    }

    /// Slots of a `capacity`-slot target left for `me`: `capacity` minus
    /// the claims of **others** (`me`'s own claim does not count against
    /// it), saturating. `> 0`: `me` may claim (or keeps) a slot.
    pub fn free_slots(&self, target: SimId, capacity: u32, me: SimId) -> u32 {
        let own = u32::from(self.holds(target, me));
        capacity.saturating_sub(self.count(target).saturating_sub(own))
    }

    /// `by`'s slot on `target`: its 0-based position in claim order (a slot
    /// index moves down when an earlier claimant releases).
    pub fn slot_of(&self, target: SimId, by: SimId) -> Option<u32> {
        let i = self.claims.get(&target)?.iter().position(|&c| c == by)?;
        u32::try_from(i).ok()
    }

    /// Targets `by` holds, ascending.
    pub fn held_by(&self, by: SimId) -> Vec<SimId> {
        self.claims
            .iter()
            .filter(|(_, claimants)| claimants.contains(&by))
            .map(|(&target, _)| target)
            .collect()
    }
}

fn len_u32(n: usize) -> u32 {
    u32::try_from(n).expect("too many reservations")
}

/// Snapshot form of [`Reservations`]: `(target, claimants in claim order)`,
/// ascending by target (JSON maps need string keys).
fn save_reservations(world: &World) -> Option<Value> {
    let r = world.get_resource::<Reservations>()?;
    let claims: Vec<(SimId, Vec<SimId>)> = r.claims.iter().map(|(&t, c)| (t, c.clone())).collect();
    Some(serde_json::to_value(claims).expect("reservations serialize to JSON"))
}

fn load_reservations(world: &mut World, value: Value) -> Result<(), String> {
    let claims: Vec<(SimId, Vec<SimId>)> =
        serde_json::from_value(value).map_err(|e| e.to_string())?;
    // The live resource never holds an empty claim list (every `Reservations`
    // method removes a target with its last claimant, see the type's
    // invariant), so a saved empty list can only come from a hand-edited
    // file; dropping it keeps the hash equal to the live shape.
    world.insert_resource(Reservations {
        claims: claims.into_iter().filter(|(_, c)| !c.is_empty()).collect(),
    });
    Ok(())
}

impl SimHashResource for Reservations {
    const NAME: &'static str = "reservations";
    fn snapshot_fns() -> Option<ResourceSnapshotFns> {
        Some(ResourceSnapshotFns {
            save: save_reservations,
            load: load_reservations,
        })
    }
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len_u32(self.claims.len()));
        for (target, claimants) in &self.claims {
            h.u32(target.0);
            h.u32(len_u32(claimants.len()));
            for c in claimants {
                h.u32(c.0);
            }
        }
    }
    fn sim_hash_entity(&self, id: SimId, h: &mut Fnv64) {
        let held = self.held_by(id);
        h.u32(len_u32(held.len()));
        for target in held {
            h.u32(target.0);
        }
    }
    fn entity_json(&self, id: SimId) -> Option<(&'static str, Value)> {
        serde_json::to_value(self.held_by(id))
            .ok()
            .map(|v| ("reservations_held", v))
    }
}

/// A capacity-1 target is free for `me` when nobody else holds it
/// (`free_slots(reservations, target, 1, me) > 0`).
pub fn free_for(reservations: &Reservations, target: SimId, me: SimId) -> bool {
    reservations.count(target) == 0 || reservations.holds(target, me)
}

/// Slots of a `capacity`-slot target (a well with 3 buckets, a stockpile
/// with `food` units, a workbench with 2 seats) left for `me`:
/// [`Reservations::free_slots`]. The multi-slot [`free_for`].
pub fn free_slots(reservations: &Reservations, target: SimId, capacity: u32, me: SimId) -> u32 {
    reservations.free_slots(target, capacity, me)
}

/// Claims one of `target`'s `capacity` slots for `me` ([`reserve`]:
/// journaled `Reserve { target, ok }`); returns `me`'s slot index (its
/// position in claim order, [`Reservations::slot_of`]) or `None` when all
/// slots are taken by others. Re-claiming keeps the slot.
pub fn try_reserve_slot(world: &mut World, me: SimId, target: SimId, capacity: u32) -> Option<u32> {
    if !reserve(world, me, target, capacity) {
        return None;
    }
    world.resource::<Reservations>().slot_of(target, me)
}

/// Drops every claim of `agent`, journaling one `Release { reason }` per
/// target held.
pub fn release_all(world: &mut World, agent: Entity, reason: ReleaseReason) {
    let Some(me) = world.get::<SimId>(agent).copied() else {
        return;
    };
    let held: Vec<SimId> = if journal::is_recording(world) {
        world
            .get_resource::<Reservations>()
            .map(|r| r.held_by(me))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if let Some(mut reservations) = world.get_resource_mut::<Reservations>() {
        reservations.release_all(me);
    }
    for target in held {
        journal::record(world, Some(me), || JournalKind::Release { target, reason });
    }
}

/// Drops `agent`'s claim on `target` (journaled `Release { Finished }` if it
/// held one).
pub fn release(world: &mut World, agent: Entity, target: SimId) {
    let Some(me) = world.get::<SimId>(agent).copied() else {
        return;
    };
    let Some(mut reservations) = world.get_resource_mut::<Reservations>() else {
        return;
    };
    let held = reservations.holds(target, me);
    reservations.release(target, me);
    if held {
        journal::record(world, Some(me), || JournalKind::Release {
            target,
            reason: ReleaseReason::Finished,
        });
    }
}

/// Tries to claim `target` for `me` with `capacity`, journaling
/// `Reserve { target, ok }`. Returns whether the claim is held.
pub fn reserve(world: &mut World, me: SimId, target: SimId, capacity: u32) -> bool {
    let ok = world
        .resource_mut::<Reservations>()
        .try_reserve(target, me, capacity);
    journal::record(world, Some(me), || JournalKind::Reserve { target, ok });
    ok
}

/// `SimId` leave hook: no claim of a leaving entity may outlive it (its
/// running leaf's stop is never called when it is despawned mid-branch), nor
/// may a claim *on* it (a despawned bed, field or stockpile).
fn release_on_leave(world: &mut DeferredWorld, id: SimId, _entity: Entity) {
    let Some(mut reservations) = world.get_resource_mut::<Reservations>() else {
        return;
    };
    let held = reservations.held_by(id);
    reservations.release_all(id);
    let claimants = reservations.claims.remove(&id).unwrap_or_default();
    for target in held {
        record_deferred(world, Some(id), || JournalKind::Release {
            target,
            reason: ReleaseReason::Despawn,
        });
    }
    for claimant in claimants {
        record_deferred(world, Some(claimant), || JournalKind::Release {
            target: id,
            reason: ReleaseReason::Despawn,
        });
    }
}

/// Installs [`Reservations`] (hashed as `reservations`) and releases claims
/// of entities leaving the sim.
pub struct NpcJobsPlugin;

impl Plugin for NpcJobsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Reservations>()
            .register_sim_resource::<Reservations>()
            .register_snapshot_component_named::<Provides>("provides")
            .register_snapshot_component_named::<Approach>("approach")
            .on_sim_id_leave("npc_jobs.reservations", release_on_leave);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_keeps_reservations_hash_equal() {
        let mut live = Reservations::default();
        for (t, by) in [(10, 3), (10, 1), (20, 2), (30, 1)] {
            assert!(live.try_reserve(SimId(t), SimId(by), 2));
        }
        // Emptied targets are removed, never left as empty entries.
        live.release(SimId(20), SimId(2));
        live.release_all(SimId(3));
        assert!(live.claims.values().all(|c| !c.is_empty()));
        let hash = |r: &Reservations| {
            let mut h = Fnv64::default();
            r.sim_hash(&mut h);
            h.0
        };
        let mut world = World::new();
        world.insert_resource(live.clone());
        let saved = save_reservations(&world).unwrap();
        let mut fresh = World::new();
        load_reservations(&mut fresh, saved).unwrap();
        let loaded = fresh.resource::<Reservations>();
        assert_eq!(loaded, &live);
        assert_eq!(hash(loaded), hash(&live));
    }

    #[test]
    fn multi_slot_claims() {
        let (well, a, b, c) = (SimId(10), SimId(1), SimId(2), SimId(3));
        let mut r = Reservations::default();
        assert_eq!(free_slots(&r, well, 2, a), 2);
        assert!(r.try_reserve(well, a, 2));
        assert_eq!(free_slots(&r, well, 2, a), 2, "own claim does not count");
        assert_eq!(free_slots(&r, well, 2, b), 1);
        assert!(r.try_reserve(well, b, 2));
        assert_eq!(free_slots(&r, well, 2, c), 0);
        assert!(!r.try_reserve(well, c, 2));
        assert_eq!((r.slot_of(well, a), r.slot_of(well, b)), (Some(0), Some(1)));
        assert_eq!(r.slot_of(well, c), None);
        r.release(well, a);
        assert_eq!(r.slot_of(well, b), Some(0), "slots move down");
        // Capacity 1 agrees with `free_for`.
        for me in [a, b, c] {
            assert_eq!(free_slots(&r, well, 1, me) > 0, free_for(&r, well, me));
        }
        assert_eq!(free_slots(&r, well, 0, b), 0);
    }

    #[test]
    fn try_reserve_slot_journals_and_returns_the_slot() {
        let mut world = World::new();
        world.init_resource::<Reservations>();
        let t = SimId(9);
        assert_eq!(try_reserve_slot(&mut world, SimId(1), t, 2), Some(0));
        assert_eq!(try_reserve_slot(&mut world, SimId(2), t, 2), Some(1));
        assert_eq!(
            try_reserve_slot(&mut world, SimId(1), t, 2),
            Some(0),
            "kept"
        );
        assert_eq!(try_reserve_slot(&mut world, SimId(3), t, 2), None);
    }
}
