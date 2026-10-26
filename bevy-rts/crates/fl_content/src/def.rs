//! The def-component trait: an authoring-side component that becomes runtime components at spawn.

use std::any::Any;
use std::fmt::Debug;

use bevy_ecs::world::EntityWorldMut;
use fl_math::{Angle, FxVec2};
use fl_protocol::{PlayerId, UnitTypeId};

use crate::db::ContentDb;
use crate::link::LinkCtx;

/// A named, plugin-registered piece of an object definition (e.g. `"Hull"`, `"Mobility"`).
///
/// Defs are authoring-friendly (real-world units, string refs). `instantiate` converts one
/// into runtime components once, at spawn; `reload` updates them after a value-only hot
/// reload.
pub trait DefComponent: erased_serde::Serialize + Debug + Send + Sync + 'static {
    /// Link-pass validation: check cross-references and value ranges, reporting through `ctx`.
    fn validate(&self, _ctx: &mut LinkCtx<'_>) {}
    /// Insert this def's runtime components on a freshly spawned entity.
    fn instantiate(&self, ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>);
    /// Update an entity spawned from an earlier version of this def to this one, in place
    /// (a value-only hot reload, ADR-0012): `ctx.content` is the new content. Keeps the
    /// entity's state ratios (a hull keeps its HP ratio) and the state this def does not own
    /// (orders, paths). By default it [`instantiate`](Self::instantiate)s again, replacing the
    /// def's runtime components.
    fn reload(&self, ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        self.instantiate(ctx, entity);
    }
    /// Upcast for typed access (`ResolvedObject::get`).
    fn as_any(&self) -> &dyn Any;
}

erased_serde::serialize_trait_object!(DefComponent);

/// Everything a def component needs to know when its unit is spawned (or reloaded).
pub struct SpawnCtx<'a> {
    pub content: &'a ContentDb,
    pub unit: UnitTypeId,
    pub player: PlayerId,
    pub pos: FxVec2,
    pub heading: Angle,
}
