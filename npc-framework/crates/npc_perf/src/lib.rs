//! Built-in profiler for the NPC framework.
//!
//! A [`PerfLayer`] (a `tracing_subscriber` layer) times every Bevy system
//! span (`info_span!("system", name = <type path>)`, emitted with the
//! `bevy_ecs/trace` feature) and every custom mechanic span
//! (`info_span!("mechanic", name = "think.agent")`). A [`PerfCollector`]
//! tags systems with mechanics ([`MechanicTags`]), keeps rolling
//! p50/p95/max per system and per mechanic, checks [`PerfBudgets`] (once a
//! mechanic has [`DEFAULT_MIN_SAMPLES`] samples; [`PerfBudgets::scaled_for_profile`]
//! relaxes them ×[`DEBUG_BUDGET_SCALE`] in dev builds) and
//! produces serde [`PerfReport`]s.
//!
//! Profiling data never feeds the simulation: it lives only here.
//!
//! # Wiring
//! - **Headless:** enable `bevy_ecs/trace` (and `bevy_ecs/debug` for real
//!   system names), call [`install_global`] **before** systems are added
//!   (spans created with no subscriber stay disabled), and call
//!   [`PerfCollector::end_frame`] after each tick.
//! - **In the game:** return `PerfCollector::global().boxed_layer()` from
//!   `LogPlugin::custom_layer`, insert `PerfHandle(PerfCollector::global().clone())`
//!   and add [`end_frame_system`] (e.g. in `Last`).

mod budget;
mod collector;
mod layer;
mod report;
mod stats;
mod tags;

use bevy_ecs::prelude::{Res, Resource};
use tracing_subscriber::layer::SubscriberExt;

pub use budget::{
    BudgetBreach, DEBUG_BUDGET_SCALE, NPC_BUDGET_TABLE, PerfBudgetTable, PerfBudgets,
};
pub use collector::{
    BREACH_INTERVAL, BreachLog, DEFAULT_MIN_SAMPLES, DEFAULT_WINDOW, PerfCollector,
};
pub use layer::{MECHANIC_SPAN, PerfLayer, SYSTEM_SPAN};
pub use report::{PerfReport, clip};
pub use stats::{MechanicStats, SystemStats};
pub use tags::{MechanicTags, UNTAGGED};

/// The collector as a Bevy resource.
#[derive(Resource, Clone, Debug)]
pub struct PerfHandle(pub PerfCollector);

/// Installs `registry().with(collector.layer())` as the global default
/// subscriber. Returns `false` (and changes nothing) if a global default is
/// already set. Used headless; the game goes through `LogPlugin` instead.
pub fn install_global(collector: &PerfCollector) -> bool {
    let subscriber = tracing_subscriber::registry().with(collector.layer());
    tracing::subscriber::set_global_default(subscriber).is_ok()
}

/// Bevy system calling [`PerfCollector::end_frame`] on the [`PerfHandle`]
/// resource, if present. Add it once per frame (e.g. in `Last`).
pub fn end_frame_system(handle: Option<Res<PerfHandle>>) {
    if let Some(handle) = handle {
        handle.0.end_frame();
    }
}
