//! Snapshot publishing in [`PublishSet::Snapshot`] (spec §5.3, ADR-0002): the per-tick
//! [`SimSnapshot`] the client renders from.
//!
//! This is the only place in the sim where change detection (`Changed<T>`, `Ref::is_changed`)
//! is allowed: it decides which records to rebuild, never what the sim does.

use std::sync::Arc;

use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, Res, ResMut};
use fl_protocol::{SimId, Tick};
use fl_view::{EntityRecord, SimSnapshot, flags};

use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::clock::SimClock;
use crate::counters::SimCounters;
use crate::debug_cmds::Invulnerable;
use crate::ids::SimIndex;
use crate::path::{PathFollow, PathState};
use crate::schedule::PublishSet;
use crate::units::{Health, Owner, Pose, UnitType};

/// The `hp_frac` of an entity without [`Health`].
const NO_HEALTH_FRAC: u16 = u16::MAX;

/// The last two snapshots: `current` is the one [`step_tick`](crate::step_tick) hands out,
/// `previous` the one before, whose buffer the next publish reuses once nobody else holds it.
/// Transient (kept: the next publish copies unchanged records from `current`)
/// `"core.snapshot_out"`.
#[derive(Resource)]
pub struct SnapshotOut {
    current: Arc<SimSnapshot>,
    previous: Arc<SimSnapshot>,
}

impl Default for SnapshotOut {
    fn default() -> Self {
        SnapshotOut {
            current: Arc::new(empty_snapshot(Tick(0), 0)),
            previous: Arc::new(empty_snapshot(Tick(0), 0)),
        }
    }
}

impl SnapshotOut {
    /// The snapshot of the last tick that ran (empty before the first one).
    pub fn current(&self) -> &Arc<SimSnapshot> {
        &self.current
    }
}

fn empty_snapshot(tick: Tick, capacity: usize) -> SimSnapshot {
    SimSnapshot {
        tick,
        entities: Vec::with_capacity(capacity),
        channels: Vec::new(),
    }
}

type UnitView<'a> = (
    Ref<'a, UnitType>,
    Ref<'a, Owner>,
    Ref<'a, Pose>,
    Option<Ref<'a, Health>>,
    Option<Ref<'a, PathFollow>>,
    Option<Ref<'a, Invulnerable>>,
);

/// Whether a unit whose path is in `state` shows as [`flags::MOVING`]: following a path or a
/// flow field.
fn moving(state: PathState) -> bool {
    matches!(state, PathState::Following | PathState::Flow { .. })
}

/// The record of one unit, from scratch.
fn build_record(sim_id: SimId, unit: &UnitView<'_>) -> EntityRecord {
    let (unit_type, owner, pose, health, follow, invulnerable) = unit;
    let mut bits = 0;
    if follow.as_ref().is_some_and(|f| moving(f.state)) {
        bits |= flags::MOVING;
    }
    if invulnerable.is_some() {
        bits |= flags::INVULNERABLE;
    }
    EntityRecord {
        sim_id,
        unit: unit_type.0,
        owner: owner.0,
        pos: pose.pos,
        heading: pose.heading,
        turrets: Vec::new(),
        hp_frac: health
            .as_ref()
            .map_or(NO_HEALTH_FRAC, |h| EntityRecord::hp_frac(h.hp, h.max)),
        flags: bits,
        visible_to: 0xFF,
    }
}

/// Whether last tick's record `prev` of `unit` is still exact: none of the components it is
/// built from changed, and none of the optional ones was removed (removal is not a change, so
/// it shows as a record that disagrees with the component being absent).
fn still_exact(prev: &EntityRecord, unit: &UnitView<'_>) -> bool {
    let (unit_type, owner, pose, health, follow, invulnerable) = unit;
    let changed = unit_type.is_changed()
        || owner.is_changed()
        || pose.is_changed()
        || health.as_ref().is_some_and(Ref::is_changed)
        || follow.as_ref().is_some_and(Ref::is_changed)
        || invulnerable.as_ref().is_some_and(Ref::is_changed);
    let removed = (health.is_none() && prev.hp_frac != NO_HEALTH_FRAC)
        || (follow.is_none() && prev.flags & flags::MOVING != 0)
        || (invulnerable.is_none() && prev.flags & flags::INVULNERABLE != 0);
    !changed && !removed
}

/// Write this tick's snapshot: one [`EntityRecord`] per entity with `SimEntity`, [`UnitType`],
/// [`Owner`] and [`Pose`], in [`SimIndex`] order. A record is rebuilt only when one of its
/// components changed (or an optional one was removed); the others are copied from last
/// tick's snapshot. The buffer of the snapshot before that is reused when no client holds it
/// any more. `visible_to` is every team until vision arrives (M3); `turrets` is empty.
///
/// Counts `"publish.records"` and `"publish.rebuilt"` (when not zero).
pub(crate) fn publish_snapshot(
    clock: Res<SimClock>,
    index: Res<SimIndex>,
    units: Query<UnitView<'_>>,
    mut out: ResMut<SnapshotOut>,
    mut counters: ResMut<SimCounters>,
) {
    let SnapshotOut { current, previous } = &mut *out;
    if Arc::get_mut(previous).is_none() {
        *previous = Arc::new(empty_snapshot(clock.tick, current.entities.len()));
    }
    let next = Arc::get_mut(previous).expect("just made unique");
    next.tick = clock.tick;
    next.entities.clear();
    next.channels.clear();

    let base = &current.entities;
    let mut cursor = 0;
    let mut rebuilt = 0;
    for (sim_id, entity) in index.iter() {
        let Ok(unit) = units.get(entity) else {
            continue;
        };
        // Both lists are sorted by sim id: advance through last tick's records in step.
        while base.get(cursor).is_some_and(|r| r.sim_id < sim_id) {
            cursor += 1;
        }
        let record = match base.get(cursor) {
            Some(prev) if prev.sim_id == sim_id && still_exact(prev, &unit) => prev.clone(),
            _ => {
                rebuilt += 1;
                build_record(sim_id, &unit)
            }
        };
        next.entities.push(record);
    }
    debug_assert!(
        next.entities
            .windows(2)
            .all(|pair| pair[0].sim_id < pair[1].sim_id),
        "snapshot records must be sorted by sim id"
    );

    // Like every counter, counted only when there was work.
    if !next.entities.is_empty() {
        counters.add("publish.records", next.entities.len() as u64);
    }
    if rebuilt > 0 {
        counters.add("publish.rebuilt", rebuilt);
    }
    std::mem::swap(current, previous);
}

/// [`SnapshotOut`] and the publish system.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app.init_resource::<SnapshotOut>();
    sim.register_resource::<SnapshotOut>(
        "core.snapshot_out",
        Class::TransientKept("read by step_tick after the tick; the next publish's base"),
    )
    .add_systems(PublishSet::Snapshot, publish_snapshot);
}
