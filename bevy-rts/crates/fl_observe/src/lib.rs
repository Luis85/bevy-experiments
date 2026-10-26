//! fl_observe: headless observability (spec §15).
//!
//! - [`config`](DebugConfig): `debug.ron`, read by [`load_debug_config`]; `RUST_LOG`
//!   overrides its log filter.
//! - Logging: a [`RunDir`] per run ([`new_run_dir`]), `log.jsonl` written by
//!   [`json_file_layer`] (every record carries the tick of its `sim_tick` span), and
//!   [`init_headless`], the subscriber of headless processes.
//! - The [`Profiler`] and its [`ProfilerLayer`]: per-system, per-phase, per-feature and
//!   per-tick timings with rolling p50/p95/max, and [`Budgets`] that warn.
//! - [`JournalSink`], which writes `journal.jsonl`.
//!
//! Everything here is observe-only: the sim never reads it, and it is never hashed. This is
//! the one headless crate that reads a wall clock, behind scoped `#[expect]`s.

mod config;
mod logging;
mod profiler;
mod sink;

pub use config::{Budgets, DebugConfig, load_debug_config};
pub use logging::{
    JsonFileLayer, RunDir, global_run_dir, init_headless, is_profiled, json_file_layer,
    new_run_dir, observe_layers, set_global_run_dir,
};
pub use profiler::{
    BUDGET_TARGET, PHASE_TARGET, PerfSnapshot, Profiler, ProfilerLayer, Stat, SystemStat, WINDOW,
    feature_of, global_profiler,
};
pub use sink::JournalSink;
