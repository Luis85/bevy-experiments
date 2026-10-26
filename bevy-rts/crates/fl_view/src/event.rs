//! Events the sim raises during a tick (spec §5.3).

use fl_protocol::{PlayerId, SimId, UnitTypeId};
use serde::{Deserialize, Serialize};

/// Something that happened during a tick, in the order it was raised.
///
/// The client drains every tick's events, so none are lost when several ticks run in one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SimEvent {
    /// An entity was spawned; `unit` is `None` for entities that aren't units.
    Spawned {
        sim_id: SimId,
        unit: Option<UnitTypeId>,
        owner: PlayerId,
    },
    Despawned {
        sim_id: SimId,
    },
    /// A command was rejected (late, early, duplicate seq, invalid, ...).
    CommandRejected {
        player: PlayerId,
        seq: u32,
        reason: String,
    },
    /// A feature event, encoded by the feature under its channel name.
    Custom {
        channel: String,
        data: Vec<u8>,
    },
}
