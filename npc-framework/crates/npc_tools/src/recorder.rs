//! The replay recorder shared by every tool: `npcctl run --record` /
//! `replay record` (aligned checkpoints, stepped tick by tick), the BRP
//! `npc/record/start|stop` methods and `village --record` (live cadence,
//! checked once per frame), and `npc_kernel::replay::record`. One type, so
//! a replay has the same header, commands and checkpoint rules whoever
//! recorded it.
//!
//! ```ignore
//! let mut rec = Recorder::aligned(100, ticks);
//! while tick < ticks { step(); rec.after_tick(world); }
//! let replay = rec.finish(world)?;
//! ```

pub use npc_kernel::replay::{Cadence, Recorder};
