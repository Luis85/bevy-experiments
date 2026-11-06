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

#[cfg(test)]
pub use field::FxView;
pub use field::{BattleViewModel, ContactView, CorpseView, EnemyView, HealthView};
pub use interpolation::{ActorInterpolation, BattleViewPlugin, SceneBlend};
pub use placement::placement_preview;
pub use readouts::{FieldHint, OrdnanceSlotView, ReticleReadout};
pub use screen::{BattleScreen, arena_geometry};

/// Per-thread counts of the battle view builds, for the once-per-egui-pass
/// tests of `ui/battle/frame.rs`. Test builds only: production pays nothing.
#[cfg(test)]
pub(crate) mod builds {
    use std::cell::Cell;

    /// How many times each battle view was built on this thread.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct ViewBuilds {
        /// `BattleScreen::build`.
        pub screen: u32,
        /// `BattleViewModel::build_with`.
        pub field: u32,
        /// `ui::hud::view_model`.
        pub hud: u32,
    }

    thread_local! {
        static VIEW_BUILDS: Cell<ViewBuilds> = const {
            Cell::new(ViewBuilds { screen: 0, field: 0, hud: 0 })
        };
    }

    /// Counts one build of the view `which` selects.
    pub(crate) fn record(which: impl FnOnce(&mut ViewBuilds) -> &mut u32) {
        VIEW_BUILDS.with(|cell| {
            let mut builds = cell.get();
            *which(&mut builds) += 1;
            cell.set(builds);
        });
    }

    /// This thread's counts since the last take, which resets them.
    pub(crate) fn take() -> ViewBuilds {
        VIEW_BUILDS.with(Cell::take)
    }

    thread_local! {
        static ID_LOOKUPS: Cell<u32> = const { Cell::new(0) };
    }

    /// Counts one search by id for an actor's pose or previous position: the
    /// fallback for a store that does not line up with the attempt's order.
    pub(crate) fn record_id_lookup() {
        ID_LOOKUPS.with(|cell| cell.set(cell.get() + 1));
    }

    /// This thread's id searches since the last take, which resets them.
    pub(crate) fn take_id_lookups() -> u32 {
        ID_LOOKUPS.with(Cell::take)
    }
}
