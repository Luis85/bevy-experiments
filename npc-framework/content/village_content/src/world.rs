//! Village places (smart objects) and the villager marker.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::Fnv64;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// What a place is, for presentation and debugging.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::world"]
pub enum PlaceKind {
    House,
    Bed,
    Stockpile,
    Field,
    Well,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
#[require(SimPos)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::world"]
pub struct Place {
    pub kind: PlaceKind,
    /// Facing of the model in degrees around +Y, presentation hint only.
    pub yaw_deg: i32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Home;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Bed {
    /// Home this bed belongs to.
    pub home: SimId,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::world"]
pub struct Stockpile {
    pub food: u32,
    pub capacity: u32,
}

impl SimHash for Stockpile {
    const NAME: &'static str = "stockpile";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.food);
        h.u32(self.capacity);
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkSite {
    /// Ticks of work per shift.
    pub work_ticks: u32,
    /// Food added to the stockpile per completed shift.
    pub yield_food: u32,
    /// Stockpile that receives the yield.
    pub stockpile: SimId,
}

/// A well: drinking water for any villager. It advertises `Provides { need:
/// Thirst }` and an `Approach`; up to `slots` villagers drink at once (claimed
/// through `Reservations`, capacity `slots`). The water never runs out.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Well {
    /// Villagers that may drink at the same time.
    pub slots: u32,
}

/// A villager agent. Requires a [`CurrentOrder`] (no order by default), so
/// villagers take orders.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
#[require(CurrentOrder)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::world"]
pub struct Villager {
    pub home: SimId,
    /// Walking speed in mm per tick (default 60 = 1.2 m/s).
    pub speed: i32,
}

/// Kind namer for the journal (`Spawn`) and inspection: `"villager"`,
/// `"house"`, `"bed"`, `"stockpile"`, `"field"`, `"well"` (others: not ours).
pub fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    if world.get::<Villager>(entity).is_some() {
        return Some("villager");
    }
    world.get::<Place>(entity).map(|p| match p.kind {
        PlaceKind::House => "house",
        PlaceKind::Bed => "bed",
        PlaceKind::Stockpile => "stockpile",
        PlaceKind::Field => "field",
        PlaceKind::Well => "well",
    })
}
