//! The sim clock.

use bevy_ecs::resource::Resource;
use fl_protocol::Tick;
use serde::{Deserialize, Serialize};

/// The tick being simulated. [`step_tick`](crate::step_tick) advances it after the tick ran,
/// so between ticks it names the next tick to run. Hashed `"core.clock"`.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimClock {
    pub tick: Tick,
}
