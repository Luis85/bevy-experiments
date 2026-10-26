//! Stable entity ids: [`SimEntity`], the [`SimIdAllocator`] and the [`SimIndex`].
//!
//! Commands, saves, replays and state hashes refer to entities by [`SimId`], never by Bevy
//! `Entity`. Any iteration that changes state follows [`SimIndex`] order (ascending id).

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::{DeferredWorld, EntityWorldMut, World};
use fl_protocol::SimId;
use serde::{Deserialize, Serialize};

/// An entity's stable sim id. Immutable: component hooks keep [`SimIndex`] in sync on
/// insert, replace, remove and despawn.
///
/// Only fl_sim creates one: the spawn funnel and [`setup_spawn`](crate::setup_spawn) (with a
/// freshly allocated id) and loading (with the saved id). A feature reads the id with
/// [`SimEntity::id`] and spawns through a [`SpawnRequest`](crate::SpawnRequest).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[component(immutable, on_insert = index_sim_entity, on_discard = unindex_sim_entity)]
pub struct SimEntity(SimId);

impl SimEntity {
    /// The component for `id`; only the spawn code and loading make one.
    pub(crate) fn new(id: SimId) -> Self {
        SimEntity(id)
    }

    /// The entity's sim id.
    pub fn id(&self) -> SimId {
        self.0
    }
}

/// Runs on insert and replace, after `unindex_sim_entity` has dropped any old id.
fn index_sim_entity(mut world: DeferredWorld, ctx: HookContext) {
    let id = world
        .get::<SimEntity>(ctx.entity)
        .expect("on_insert runs with the component present")
        .0;
    sim_index(&mut world).insert(id, ctx.entity);
}

/// Runs on replace, remove and despawn, while the old value is still present.
fn unindex_sim_entity(mut world: DeferredWorld, ctx: HookContext) {
    let id = world
        .get::<SimEntity>(ctx.entity)
        .expect("on_discard runs with the component present")
        .0;
    sim_index(&mut world).remove(id);
}

fn sim_index<'w>(world: &'w mut DeferredWorld) -> bevy_ecs::change_detection::Mut<'w, SimIndex> {
    world
        .get_resource_mut::<SimIndex>()
        .expect("SimEntity needs the SimIndex resource (added by CoreFeature)")
}

/// Hands out sequential [`SimId`]s. The first id is 1; ids are never reused.
/// Hashed `"core.sim_id_allocator"`.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimIdAllocator {
    next: u32,
}

impl SimIdAllocator {
    /// How many ids have been handed out (the last id issued; 0 before the first).
    pub fn issued(&self) -> u32 {
        self.next
    }

    /// An allocator that has issued `issued` ids (loading a save).
    pub(crate) fn with_issued(issued: u32) -> Self {
        SimIdAllocator { next: issued }
    }

    /// The next unused id. Only the spawn code ([`setup_spawn`](crate::setup_spawn) and the
    /// spawn funnel) and loading allocate ids.
    ///
    /// # Panics
    /// After `u32::MAX` ids.
    pub(crate) fn alloc(&mut self) -> SimId {
        self.next = self.next.checked_add(1).expect("SimId space exhausted");
        SimId(self.next)
    }
}

/// Spawn an entity with sim id `id` and `bundle`: the one place sim entities are created.
/// [`setup_spawn`](crate::setup_spawn) (and with it the spawn funnel) passes a freshly
/// allocated id; loading a save passes the saved id.
///
/// # Panics
/// If another entity already has `id`.
pub(crate) fn spawn_with_id(
    world: &mut World,
    id: SimId,
    bundle: impl Bundle,
) -> EntityWorldMut<'_> {
    world.spawn((SimEntity::new(id), bundle))
}

/// Every [`SimEntity`], sorted by id.
#[derive(Resource, Default, Clone, Debug)]
pub struct SimIndex {
    entries: Vec<(SimId, Entity)>,
}

impl SimIndex {
    /// The entity with this id.
    pub fn get(&self, id: SimId) -> Option<Entity> {
        self.position(id).ok().map(|i| self.entries[i].1)
    }

    /// `(id, entity)` pairs in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = (SimId, Entity)> + '_ {
        self.entries.iter().copied()
    }

    /// Ids in ascending order.
    pub fn ids(&self) -> impl Iterator<Item = SimId> + '_ {
        self.entries.iter().map(|(id, _)| *id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn position(&self, id: SimId) -> Result<usize, usize> {
        self.entries.binary_search_by_key(&id, |(id, _)| *id)
    }

    /// # Panics
    /// If another entity already has `id`.
    fn insert(&mut self, id: SimId, entity: Entity) {
        match self.position(id) {
            Ok(i) => panic!(
                "{id} is already used by {:?}; cannot give it to {entity:?}",
                self.entries[i].1
            ),
            Err(i) => self.entries.insert(i, (id, entity)),
        }
    }

    fn remove(&mut self, id: SimId) {
        if let Ok(i) = self.position(id) {
            self.entries.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<SimIndex>();
        world
    }

    #[test]
    fn first_id_is_one() {
        let mut ids = SimIdAllocator::default();
        assert_eq!(ids.alloc(), SimId(1));
        assert_eq!(ids.alloc(), SimId(2));
    }

    #[test]
    fn index_is_sorted_by_id() {
        let mut world = world();
        let c = world.spawn(SimEntity(SimId(30))).id();
        let a = world.spawn(SimEntity(SimId(10))).id();
        let b = world.spawn(SimEntity(SimId(20))).id();
        let index = world.resource::<SimIndex>();
        assert_eq!(
            index.iter().collect::<Vec<_>>(),
            [(SimId(10), a), (SimId(20), b), (SimId(30), c)]
        );
    }

    #[test]
    fn replacing_and_removing_the_id_updates_the_index() {
        let mut world = world();
        let e = world.spawn(SimEntity(SimId(1))).id();
        world.entity_mut(e).insert(SimEntity(SimId(5)));
        assert_eq!(world.resource::<SimIndex>().get(SimId(1)), None);
        assert_eq!(world.resource::<SimIndex>().get(SimId(5)), Some(e));
        world.entity_mut(e).remove::<SimEntity>();
        assert!(world.resource::<SimIndex>().is_empty());
    }

    #[test]
    #[should_panic(expected = "already used")]
    fn duplicate_id_panics() {
        let mut world = world();
        world.spawn(SimEntity(SimId(1)));
        world.spawn(SimEntity(SimId(1)));
    }
}
