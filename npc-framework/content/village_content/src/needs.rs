//! The villager's needs: hunger, energy and thirst (tuned for a 2400-tick
//! day: ~1-2 meals, one night's sleep and a few drinks at the well per day).

use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

/// Need name (journal, `SetNeed`): hunger satisfaction.
pub const HUNGER: &str = "Hunger";
/// Need name (journal, `SetNeed`): energy.
pub const ENERGY: &str = "Energy";
/// Need name (journal, `SetNeed`): thirst satisfaction (100% = not thirsty).
pub const THIRST: &str = "Thirst";

/// Hunger: decays 40/tick always.
pub const HUNGER_DEF: NeedDef = NeedDef {
    name: HUNGER,
    key: "hunger",
    order: 0,
    decay_per_tick: 40,
    thresholds: NeedThresholds {
        low_enter: 50_000,
        low_exit: 85_000,
        urgent_enter: 20_000,
        urgent_exit: 60_000,
    },
};

/// Energy: decays 40/tick while awake (sleeping pauses it).
pub const ENERGY_DEF: NeedDef = NeedDef {
    name: ENERGY,
    key: "energy",
    order: 1,
    decay_per_tick: 40,
    thresholds: NeedThresholds {
        low_enter: 40_000,
        low_exit: 90_000,
        urgent_enter: 15_000,
        urgent_exit: 60_000,
    },
};

/// Thirst: decays 45/tick while awake (sleeping pauses it, like energy), so
/// a villager is Low after ~1100 awake ticks and drinks at the well; one
/// drink (`DRINK_TICKS` at the well's `Provides`) fills it.
pub const THIRST_DEF: NeedDef = NeedDef {
    name: THIRST,
    key: "thirst",
    order: 2,
    decay_per_tick: 45,
    thresholds: NeedThresholds {
        low_enter: 50_000,
        low_exit: 85_000,
        urgent_enter: 20_000,
        urgent_exit: 60_000,
    },
};

/// Id of need `name`.
///
/// # Panics
/// When the need is not registered (`VillagePlugin` missing) or content is
/// not frozen yet.
pub fn need_id(world: &World, name: &str) -> NeedId {
    world
        .resource::<NeedRegistry>()
        .id(name)
        .unwrap_or_else(|| panic!("need `{name}` is not registered (VillagePlugin missing?)"))
}

/// A villager's needs by name: the JSON shape of the village inspector
/// (`{"hunger":{..},"energy":{..},"thirst":{..}}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VillagerNeeds {
    pub hunger: Need,
    pub energy: Need,
    pub thirst: Need,
}

impl VillagerNeeds {
    /// The villager view of `needs` (`None` when hunger, energy or thirst is
    /// missing).
    pub fn of(world: &World, needs: &Needs) -> Option<Self> {
        let registry = world.get_resource::<NeedRegistry>()?;
        let get = |name| {
            registry
                .id(name)
                .and_then(|id| needs.values.get(id.0 as usize).copied())
        };
        Some(Self {
            hunger: get(HUNGER)?,
            energy: get(ENERGY)?,
            thirst: get(THIRST)?,
        })
    }
}
