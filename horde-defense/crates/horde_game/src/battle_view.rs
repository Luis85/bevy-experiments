//! Presentation seam between the simulation and the egui battlefield painter.
//! `BattleViewModel` is a pure per-frame snapshot of one attempt plus its
//! cosmetic caches; it is built headlessly and never written back to the core.
//! Actor positions blend between the last two fixed ticks by the fixed clock's
//! overstep, so motion stays smooth at any display rate.
//!
//! `BattleScreen` is the same seam for the battle screen's controls and
//! readouts around the painter: built from `&Game`, it carries every attempt
//! value the screen lays out, so egui code never reads the attempt itself.
mod field;
mod interpolation;
mod placement;
mod readouts;
mod screen;
#[cfg(test)]
mod tests;

pub use field::{BattleViewModel, EnemyView, HealthView};
pub use interpolation::{ActorInterpolation, BattleViewPlugin, SceneBlend};
pub use placement::placement_preview;
pub use readouts::{FieldHint, OrdnanceSlotView, ReticleReadout};
pub use screen::{BattleScreen, arena_geometry};
