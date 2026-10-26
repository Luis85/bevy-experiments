//! Compact, fixed-tick CPU reference backend for Legion Battle Lab.
//!
//! No rendering, camera, window or Bevy dependency is present in this crate.
//! Unit indices remain stable for the lifetime of a battle. Dead units retain
//! their state and disappear from the spatial index on its next rebuild.

mod config;
mod math;
mod simulation;
mod spatial;
mod terrain;

pub use config::*;
pub use math::Point;
pub use simulation::{Battle, BattleStats, Outcome, UnitStore};
pub use terrain::{
    BRIDGE_HALF_WIDTH, RIVER_HALF_WIDTH, arena_half_extent, is_walkable, terrain_height,
};

/// Authoritative simulation frequency, independent of rendering.
pub const SIM_HZ: f64 = 20.0;
/// Duration of exactly one authoritative tick in seconds.
pub const SIM_DT: f32 = 1.0 / SIM_HZ as f32;
/// Capacity of this CPU backend with simulation LOD, not an FPS guarantee.
pub const MAX_UNITS: u32 = 1_000_000;
