//! Native application orchestration. Rendering publishes intents; these services
//! apply commands, advance fixed simulation ticks and persist. Sound cues and
//! runtime notices leave as Bevy messages. No Bevy types cross the horde_core
//! boundary.
mod apply;
mod command;
mod persistence;
mod plugin;
mod runtime;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use apply::{RuntimeCtx, flush, step};
#[cfg(test)]
pub(crate) use apply::{flush_for_test, poll_one_save_for_test};
pub use command::{AppCommand, AppOutcome, AttackAims, ControlFrame, RuntimeNotice, SaveIntent};
#[cfg(feature = "review")]
pub use persistence::PersistenceMetrics;
pub use plugin::{RuntimeSet, SimRuntimePlugin};
pub(crate) use runtime::push_cue;
pub use runtime::{GameSession, Runtime};
