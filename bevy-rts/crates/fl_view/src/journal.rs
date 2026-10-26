//! The debug journal record (spec §15 B). Observe-only: sim logic never reads it, and it is
//! never hashed.

use fl_protocol::{PlayerId, SimId, Tick};
use serde::{Deserialize, Serialize};

/// One journal record.
///
/// `step_tick` sorts each tick's entries by `(phase, kind, sim_id, emitted)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub tick: Tick,
    /// Index of the sim phase that emitted the record.
    pub phase: u8,
    /// Registered record kind, such as `"movement.arrived"`.
    pub kind: String,
    pub sim_id: Option<SimId>,
    pub player: Option<PlayerId>,
    /// The record's rank, in emission order, within its `(tick, phase, kind, sim_id)` group.
    pub emitted: u32,
    /// Kind-specific data.
    pub data: serde_json::Value,
}
