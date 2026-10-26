//! [`step_tick`]: the one way to advance the sim (spec §5.3, §5.6; ADR-0005).
//!
//! Every mode (the threaded runner, inline runs, tests, `fl_cli`) steps the sim through it,
//! so housekeeping that a Bevy `App` would do in `update()` (tracker clearing, the clock) and
//! the state hash happen the same way everywhere.

use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use fl_view::{SimEvent, TickInput, TickOutput};

use crate::classify::{check_tick, clear_message_buffers};
use crate::clock::SimClock;
use crate::commands::CommandInbox;
use crate::counters::SimCounters;
use crate::events::SimEvents;
use crate::hash::compute_state_hash;
use crate::observe::Journal;
use crate::publish::SnapshotOut;
use crate::schedule::SimTick;
use crate::spawn::assert_requests_drained;

/// Hash the state every `n` ticks, starting at tick 0; 0 turns hashing off. External
/// `"core.hash_interval"`: `RunConfig::hash_interval`, else the `StateHash.interval_ticks`
/// rule.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashInterval(pub u32);

/// Run one tick with `input`'s commands:
/// 1. put the commands, sorted by `(player, seq)`, into the [`CommandInbox`];
///    `apply_commands` rejects the late, early and duplicate ones; the sort counts
///    `"sort.items"` (one per command, added once the tick has run);
/// 2. run [`SimTick`]; in debug builds, check the classification again if the tick
///    registered new types, and that every entity with a classified component has a
///    [`SimEntity`](crate::SimEntity);
/// 3. compute the state hash if the [`HashInterval`] divides the tick;
/// 4. build the [`TickOutput`]: the snapshot the `Publish` phase wrote ([`SnapshotOut`]), the
///    tick's events, copied from [`SimEvents`] (the buffer stays readable until the next
///    tick's `Housekeeping` clears it), and the tick's [`Journal`] records, drained and sorted
///    by `(phase, kind, sim_id, emission order)`;
/// 5. clear every message buffer (a message is read in the tick it was written, R72), and
///    `world.clear_trackers()`, so removal buffers and change ticks behave as in an `App`;
/// 6. advance the [`SimClock`].
///
/// The tick runs inside a `sim_tick` tracing span with field `tick`.
///
/// # Panics
/// `"driver out of sync"` unless `input.tick` is the clock's tick; any panic of a sim system;
/// in debug builds, a failed check of step 2.
pub fn step_tick(world: &mut World, input: TickInput) -> TickOutput {
    let tick = world.resource::<SimClock>().tick;
    assert!(
        input.tick == tick,
        "driver out of sync: input tick {}, sim tick {}",
        input.tick.0,
        tick.0
    );
    let _span = tracing::info_span!("sim_tick", tick = tick.0).entered();

    let mut commands = input.commands;
    commands.sort_by_key(|cmd| (cmd.player, cmd.seq));
    let sorted = commands.len() as u64;
    world.resource_mut::<CommandInbox>().0 = commands;

    world.run_schedule(SimTick);
    if cfg!(debug_assertions) {
        check_tick(world);
    }
    // Counted after the tick, whose `Housekeeping` clears the counters.
    if sorted > 0 {
        world
            .resource_mut::<SimCounters>()
            .add("sort.items", sorted);
    }

    let interval = world.resource::<HashInterval>().0;
    let hash = (interval > 0 && tick.0.is_multiple_of(interval)).then(|| {
        let hash = compute_state_hash(world);
        tracing::debug!(hash = format_args!("{hash:016x}"), "state hash");
        hash
    });

    let snapshot = Arc::clone(world.resource::<SnapshotOut>().current());
    let events = world
        .resource::<SimEvents<SimEvent>>()
        .iter()
        .cloned()
        .collect();
    let journal = world.resource_mut::<Journal>().take_sorted();
    let output = TickOutput {
        tick,
        hash,
        snapshot,
        events,
        journal,
    };

    if cfg!(debug_assertions) {
        assert_requests_drained(world);
    }
    clear_message_buffers(world);
    world.clear_trackers();
    world.resource_mut::<SimClock>().tick = tick.next();
    output
}
