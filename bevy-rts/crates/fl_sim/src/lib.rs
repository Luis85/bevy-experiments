//! fl_sim: the deterministic simulation.
//!
//! The sim is its own Bevy [`App`](bevy_app::App) and `World`, built from `bevy_app` and
//! `bevy_ecs` only by [`build_sim_app`]. It is stepped one tick at a time with
//! [`step_tick`], never with `app.update()` or `run_schedule`.
//!
//! Game mechanics arrive as [`SimFeature`]s, ordered by their dependencies
//! ([`sort_features`]); [`CoreFeature`] is always first. Each registers through a
//! [`SimBuilder`]: def components, rules sections, the [`Class`] of every component and
//! resource, command handlers, RNG streams and systems. Every name lives in a
//! [`StableRegistry`], frozen when the sim is built and exported to the [`MatchHeaderRes`]'s
//! string tables.
//!
//! [`SimTick`] runs eight [`Phase`]s, each split into hook sub-sets ([`InputSet`],
//! [`ThinkSet`], ...) and ended by an explicit sync point, on a single-threaded executor with
//! ambiguity detection at error level. Entities carry a stable [`SimEntity`] id, indexed in
//! [`SimIndex`]; every in-tick spawn and despawn is a [`SpawnRequest`] or [`DespawnRequest`]
//! carried out by the funnels in [`LifecycleSet`], and setup spawns with [`setup_spawn`], the
//! same code. Randomness is keyed ([`rolls`] over the [`MatchSeed`]). Every
//! [`HashInterval`] ticks [`step_tick`] hashes the tick, every `Hashed` resource and every
//! `Hashed` component in `SimId` order ([`compute_state_hash`]).
//! [`PlayerCommand`](fl_protocol::PlayerCommand)s, delivered in each tick's
//! [`TickInput`](fl_view::TickInput), are the only input to the sim; they apply in
//! `(player, seq)` order, and late, early and duplicate commands are rejected. The `Publish`
//! phase writes each tick's snapshot ([`SnapshotOut`]).
//!
//! A [`SimRunner`] owns the sim world and steps it inline or on its own thread, fed by a
//! [`TickDriver`](fl_view::TickDriver): [`TestDriver`], [`ReplayDriver`] or
//! [`LockstepDriver`].
//!
//! Terrain is `Hashed` state ([`TerrainGrid`], ADR-0014): every cell's terrain kind and height,
//! which `debug.set_terrain` changes. The [`nav`] module builds the `Derived` navigation data
//! from it (per-class blocking, clearance and path cost, [`nav::NavGrid`]), and movement scales
//! each step by the cell's speed factor and never enters a cell its profile cannot pass.
//! Overlapping units push apart: separation steering works out each tick's pushes into the
//! Transient [`SteerPushes`], and movement adds them to the step, sliding along walls.
//!
//! Tools ask named, read-only [`debug_query`]s that features register (`SimQuery::Debug`,
//! ADR-0015); `"core.debug_queries"` lists them.
//!
//! [`save_world`] writes the `Hashed` state between ticks as a versioned [`SaveFile`];
//! [`load_world`] builds a fresh sim from one, migrating old versions and remapping dense ids
//! through the string tables, and runs on with the same hashes.
//!
//! Hot reload (ADR-0012), testbed only: [`apply_content_values`] puts content with the same ID
//! sets into the running match between ticks (also as [`SimRunner::control`]), keeping ids,
//! orders and state ratios and rebuilding the `Derived` state; content whose ID sets changed
//! rebuilds the match from a save with [`load_world_with`] and [`LoadOptions`], dropping the
//! units whose type is gone.
//!
//! Observability (spec §15): systems record registered [`JournalRecord`]s into the
//! [`Journal`], which [`step_tick`] drains, sorted, into each tick's output; every tick runs in
//! a `sim_tick` span, and [`RunConfig::phase_markers`] adds the `fl_sim::phase` marker events
//! the profiler times phases with. A [`ReplayRecorder`] records a match as a [`Replay`], and
//! [`verify_replay`] plays one back, compares its hashes and runs its [`SemanticCheck`]s;
//! [`rebless_replay`] re-runs one on this build for `fl_cli rebless` (ADR-0021).
#![deny(clippy::float_arithmetic)]

mod build;
mod builder;
mod classify;
mod clock;
mod counters;
mod debug_query;
mod events;
mod feature;
mod hash;
mod ids;
mod inspect;
mod map;
pub mod nav;
mod observe;
mod players;
mod publish;
mod registry;
mod reload;
mod replay;
mod rng;
mod runner;
mod save;
mod schedule;
mod spatial;
mod step;

mod commands;
mod debug_cmds;
mod movement;
mod orders;
mod path;
mod scale;
mod scenario;
mod spawn;
mod steer;
mod terrain;
mod units;

pub use build::{
    BuildError, Content, MatchHeaderRes, MatchSetup, RunConfig, SIM_VERSION, build_id,
    build_sim_app, load_content, sim_registry,
};
pub use builder::SimBuilder;
pub use classify::{
    Class, Codec, FillFn, TraceFn, check_classification, codec_formats, trace_type,
};
pub use clock::SimClock;
pub use commands::{
    CommandCtx, CommandHandler, CommandInbox, CommandLog, CommandRegistry, LastSeq,
    MAX_COMMAND_UNITS, MAX_WAYPOINTS, RejectReason, last_seqs,
};
pub use counters::SimCounters;
pub use debug_cmds::Invulnerable;
pub use debug_query::{DebugQueries, DebugQueryFn, debug_query};
pub use events::SimEvents;
pub use feature::{CoreFeature, FeatureError, SimFeature, sort_features};
/// Defined in `fl_content`, whose `TerrainDef::blocks` names it (ADR-0014); its
/// [`SimHash`] impl stays here.
pub use fl_content::MoveClass;
pub use hash::{SimHash, StateHashRules, StateHasher, compute_state_hash};
pub use ids::{SimEntity, SimIdAllocator, SimIndex};
pub use inspect::{inspect_entity, inspect_world};
pub use map::MapInfo;
pub use movement::{MovementRules, SteeringRules};
pub use observe::{Journal, JournalRecord, PHASE_TARGET};
pub use orders::{formation_targets, space_slots};
pub use path::{FlowKey, PathFollow, PathQueue, PathRequest, PathState, RequestKind, enqueue_path};
pub use players::{PlayerInfo, Players};
pub use publish::SnapshotOut;
pub use registry::StableRegistry;
pub use reload::{ReloadError, ReloadReport, apply_content_values};
pub use replay::{
    Divergence, REPLAY_FORMAT_VERSION, ReblessEntry, Replay, ReplayError, ReplayEvent,
    ReplayRecorder, ReplayStart, SemanticCheck, SemanticFailure, VerifyOutcome, first_divergence,
    rebless_replay, replay_app, replay_inputs, verify_replay,
};
pub use rng::{MatchSeed, Stream, rolls};
pub use runner::{
    JOURNAL_RING, LockstepDriver, LoopbackSession, NetSession, ReplayDriver, SimRunner, TestDriver,
    loopback,
};
pub use save::{
    LoadCtx, LoadError, LoadOptions, Loaded, SAVE_FORMAT_VERSION, SaveCtx, SaveError, SaveFile,
    SavedValue, SimSave, load_world, load_world_with, save_world,
};
pub use scale::{Follower, Projectile, SCALE_MAP, ScaleMix, ScaleScenario};
pub use scenario::{SetupError, setup_map, setup_scenario, setup_scenario_def};
pub use schedule::{
    ActSet, EconomySet, InputSet, LifecycleSet, PerceiveSet, Phase, PublishSet, ResolveSet,
    SimHook, SimTick, ThinkSet, configure_sim_schedule, system_order,
};
pub use spatial::{SpatialCell, SpatialGrid, SpatialRules, rebuild_spatial};
pub use spawn::{DespawnRequest, SpawnError, SpawnInit, SpawnRequest, setup_despawn, setup_spawn};
pub use steer::SteerPushes;
pub use step::{HashInterval, step_tick};
pub use terrain::{CellRect, TerrainGrid};
pub use units::{
    Footprint, Health, HullDef, Mobility, MobilityDef, MoveOrder, Order, Owner, Pose, Selectable,
    SelectableDef, UnitType, Wander,
};
