//! fl_test_utils: headless test helpers for the sim (spec §12.3, ADR-0005, ADR-0006).
//!
//! - [`ScenarioBuilder`] sets up a sim in a few calls (a scenario, the scale scenario, or a
//!   map, players and units of its own; wandering and scheduled commands) and builds a
//!   [`Scenario`]: a [`SimRunner`](fl_sim::SimRunner) stepped by a
//!   [`TestDriver`](fl_sim::TestDriver), recording every state hash.
//! - Determinism: [`assert_deterministic`] (twin runs) and [`assert_shuffle_invariant`]
//!   (runs with the order of unordered systems shuffled, [`shuffle_seeds`]).
//! - Save fitness: [`assert_save_round_trip`] (save, load, save gives the same bytes) and
//!   [`assert_branch`] (a loaded sim runs on with the original's hashes).
//! - Terrain fixtures (ADR-0014): [`ascii_terrain_content`] draws a test map as rows of
//!   characters ([`ScenarioBuilder::ascii_map`] sets one up).
//! - Pose traces: [`pose_trace`] hashes every unit's pose after each tick, to prove a change
//!   leaves unit movement as it was ([`write_pose_trace`], [`read_pose_trace`]).
//! - Perf gate (ADR-0010): [`run_counters`] collects a [`CounterSim`]'s per-tick work counters
//!   (and allocations): the scale mix on terrain, group moves, flow-cache thrash, a choke-point
//!   crowd; [`check_budgets`] holds them to `docs/perf/budgets.ron` ([`load_budgets`]),
//!   [`record_budgets`] writes new budgets for the lead to bless.

mod builder;
mod determinism;
mod fitness;
mod perf;
mod terrain;
mod trace;

pub use builder::{Scenario, ScenarioBuilder, base_content};
pub use determinism::{assert_deterministic, assert_shuffle_invariant, shuffle_seeds};
pub use fitness::{assert_branch, assert_save_round_trip};
pub use perf::{
    BudgetFile, CounterRun, CounterSim, TickCharges, budget_for, budgets_path, check_budgets,
    load_budgets, record_budgets, run_counter_scenario, run_counters, scale_path_charges,
    scale_queue_lengths,
};
pub use terrain::{
    UnitCell, ascii_terrain_content, ascii_terrain_content_with, unit_cells, write_ascii_mod,
};
pub use trace::{pose_hash, pose_trace, read_pose_trace, write_pose_trace};
