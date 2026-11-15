//! Reused queries for the hot lookups of leaf starts and conditions (S4).
//!
//! `World::try_query` builds a new `QueryState` (matching every archetype)
//! on each call; in the supplier lookup that was a fifth of its time. The
//! states here are built once per world and only brought up to date.
//!
//! A pure memo: neither hashed nor saved, and every answer is the one a fresh
//! query gives, sorted by identity.
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryState;
use npc_framework::prelude::*;
use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The reused query states, one per component type asked for.
#[derive(Resource, Default)]
pub struct LookupCache(Mutex<BTreeMap<TypeId, Box<dyn Any + Send + Sync>>>);

type Ids<T> = QueryState<(Entity, &'static SimId), With<T>>;

/// Test switch: while this resource exists the S4 lookups take their
/// original paths (a fresh query here, the map-based route search, the
/// copying stock count), so a test can compare both (`island_lookup_neutrality`).
/// Not state: neither hashed nor saved.
#[derive(Resource, Default)]
pub struct BruteForce;

/// Whether [`BruteForce`] is on.
pub fn brute_force(world: &World) -> bool {
    world.contains_resource::<BruteForce>()
}

/// Every entity with a `T` and a [`SimId`], ascending by identity: the
/// answer of `try_query::<(Entity, &SimId, &T)>()` sorted by identity.
pub fn ids_with<T: Component>(world: &World) -> Vec<(SimId, Entity)> {
    let mut out = with_state::<T, _>(world, |state, world| {
        state
            .query(world)
            .iter()
            .map(|(e, id)| (*id, e))
            .collect::<Vec<_>>()
    })
    .unwrap_or_else(|| {
        world
            .try_query_filtered::<(Entity, &SimId), With<T>>()
            .map(|mut query| query.iter(world).map(|(e, id)| (*id, e)).collect())
            .unwrap_or_default()
    });
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

fn with_state<T: Component, R>(
    world: &World,
    read: impl FnOnce(&mut Ids<T>, &World) -> R,
) -> Option<R> {
    if brute_force(world) {
        return None;
    }
    let cache = world.get_resource::<LookupCache>()?;
    let mut states = cache.0.lock().ok()?;
    let key = TypeId::of::<T>();
    let slot = match states.entry(key) {
        std::collections::btree_map::Entry::Occupied(slot) => slot.into_mut(),
        std::collections::btree_map::Entry::Vacant(slot) => {
            let state: Ids<T> = QueryState::try_new(world)?;
            slot.insert(Box::new(state))
        }
    };
    let state = slot.downcast_mut::<Ids<T>>()?;
    Some(read(state, world))
}
