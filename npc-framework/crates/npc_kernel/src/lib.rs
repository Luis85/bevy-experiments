//! Deterministic NPC sim kernel. Knows no game content.
//!
//! Rules (villager spec §6.2, hardening spec §2):
//! - sim systems never read `Time`; they read [`SimClock`];
//! - sim state is integers only (positions in millimetres, needs in milli-units);
//! - anything order-sensitive iterates sorted by [`SimId`];
//! - randomness comes from [`rng::sim_rand`] (counter-based), never a stateful RNG;
//! - no presentation components are ever inserted on sim entities.
//!
//! What lives here: ids ([`ids`]), the clock ([`clock`]), counter RNG
//! ([`rng`]), fixed-point positions ([`math`]), the [`NpcSimTick`] schedule
//! and its [`NpcSet`] phases ([`schedule`]), the command bus ([`commands`],
//! with content commands registered by name), standing orders ([`orders`]),
//! the journal ([`journal`]), replays ([`replay`]), the state-hash registry
//! ([`hash`]), inspection ([`inspect`]), the stable-name registry and freeze
//! ([`registry`]), the host-safe driver ([`driver`]), the integer
//! spatial grid ([`spatial`]) and world snapshots ([`snapshot`], ADR 0013).
//!
//! Content (unit types, needs, actions, trees, commands) registers through
//! [`registry::KernelAppExt`] (or the facade's `NpcAppExt`); everything is
//! frozen, sorted by name, in [`NpcKernelPlugin`]'s `Plugin::finish`.

// Determinism guardrails (plan Task P1). The disallowed types/methods are
// listed in this crate's clippy.toml. An exemption is `#[allow(...)]` on the
// smallest item plus a `// determinism: <why>` comment, and only for code that
// never feeds sim state. Note: `iter_over_hash_type` only knows std's hash
// types (already banned); bevy/hashbrown/`EntityHash*` iteration is caught by
// the type bans, not by this lint.
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod clock;
pub mod commands;
pub mod driver;
pub mod hash;
pub mod ids;
pub mod inspect;
pub mod journal;
pub mod json;
pub mod math;
pub mod orders;
pub mod registry;
pub mod replay;
pub mod rng;
pub mod schedule;
pub mod snapshot;
pub mod spatial;

pub use clock::{SimClock, SimConfig};
pub use commands::{CommandError, ContentCommand, SimCommand, submit};
pub use driver::{DeterminismMode, HostPosition, NpcDriverPlugin, SimControl, SimRate, SimTime};
pub use hash::{Fnv64, SimHash, SimHashResource};
pub use ids::{SimId, SimIdAllocator, SimIndex, spawn_sim};
pub use journal::{Journal, JournalPlugin, NeedLevel};
pub use json::JsonObject;
pub use math::{PrevSimPos, SimPos};
pub use orders::{CurrentOrder, Order, OrderKind};
pub use registry::{KernelAppExt, content_hash, ensure_frozen};
pub use rng::WorldSeed;
pub use schedule::{NpcKernelPlugin, NpcSet, NpcSimTick, run_ticks};
pub use snapshot::{
    EntitySnapshot, LoadError, LoadMode, LoadReport, SNAPSHOT_VERSION, SnapshotAppExt,
    SnapshotRegistry, WorldSnapshot,
};
pub use spatial::{SpatialGrid, SpatialPlugin};
