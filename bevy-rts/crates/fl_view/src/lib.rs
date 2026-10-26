//! fl_view: the client's only read model of the simulation (spec §5.3, ADR-0002).
//!
//! - [`snapshot`]: [`SimSnapshot`], per-entity [`EntityRecord`]s sorted by `SimId`, and
//!   string-keyed [`ViewChannel`]s that features publish.
//! - [`event`]: [`SimEvent`]s raised during a tick.
//! - [`journal`]: [`JournalEntry`], the observe-only debug record (spec §15 B).
//! - [`runner`]: the runner protocol ([`TickInput`], [`TickOutput`], [`SimQuery`], [`SimReply`],
//!   [`RunnerControl`]) and the channel pair between a client and a threaded runner.
//! - [`driver`]: [`CommandStamper`] and the [`TickDriver`] trait that decides when to tick.
//! - [`ai`]: the AI seam (ADR-0016): a [`PlayerView`] and the [`AiController`] that reads it.
//!
//! Values stay in `Fx`, `Angle` and integers; the client converts to `f32`. No Bevy here.
#![deny(clippy::float_arithmetic)]

pub mod ai;
pub mod driver;
pub mod event;
pub mod journal;
pub mod runner;
pub mod snapshot;

pub use ai::{AiController, NullAi, PlayerView, ai_controller};
pub use driver::{CommandDraft, CommandStamper, TickDriver};
pub use event::SimEvent;
pub use journal::JournalEntry;
pub use runner::{
    FromRunner, OpaqueContent, RunnerControl, RunnerEnd, RunnerFailure, RunnerPort, SimQuery,
    SimReply, TickInput, TickOutput, ToRunner, runner_channel,
};
pub use snapshot::{EntityRecord, SimSnapshot, ViewChannel, flags};

/// Version of the snapshot, event and runner-protocol schema in this crate.
pub const VIEW_FORMAT_VERSION: u32 = 1;
