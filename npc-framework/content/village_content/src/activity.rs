//! What an agent is visibly doing. Sim state (deterministic), written by action
//! start/stop functions; read by presentation to pick animations.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::activity"]
pub enum Activity {
    #[default]
    Idle,
    Walk,
    Work,
    Eat,
    Sleep,
    Drink,
}

impl npc_framework::kernel::SimHash for Activity {
    const NAME: &'static str = "activity";
    fn sim_hash(&self, h: &mut npc_framework::kernel::Fnv64) {
        h.u8(match self {
            Activity::Idle => 0,
            Activity::Walk => 1,
            Activity::Work => 2,
            Activity::Eat => 3,
            Activity::Sleep => 4,
            Activity::Drink => 5,
        });
    }
}
