//! The kernel's replay functions with the village as the app factory:
//! a replay names `"village"` or `"village_n"` and is rebuilt with
//! [`try_new_sim_app`](crate::try_new_sim_app).

use bevy_app::App;
use npc_framework::kernel::replay as core;

pub use core::{
    Divergence, EntityDiff, REPLAY_VERSION, Replay, ScenarioInfo, VerifyReport, entity_diffs,
    record,
};

pub use crate::scenario::{new_sim_app, spawn_village_n, try_new_sim_app};

/// [`core::verify`] with the village factory.
pub fn verify(replay: &Replay) -> VerifyReport {
    core::verify(replay, try_new_sim_app)
}

/// [`core::replay_app`] with the village factory.
pub fn replay_app(replay: &Replay) -> Result<App, String> {
    core::replay_app(replay, try_new_sim_app)
}

/// [`core::diverge`] with the village factory.
pub fn diverge(a: &Replay, b: &Replay) -> Option<Divergence> {
    core::diverge(a, b, try_new_sim_app)
}

/// [`core::try_diverge`] with the village factory.
pub fn try_diverge(a: &Replay, b: &Replay) -> Result<Option<Divergence>, String> {
    core::try_diverge(a, b, try_new_sim_app)
}
