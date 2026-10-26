//! Behaviour-tree conditions over [`PerceptionMemory`].
//!
//! The plugin registers three by name, testing the agent's
//! [`Sensors::interest`] mask:
//! - `sees_kind`: a currently sensed (vision, proximity or reach) percept matches;
//! - `remembers_kind`: a remembered percept matches;
//! - `heard_noise`: a noise was heard within the last
//!   `PerceptionConfig::noise_window_ticks` ticks.
//!
//! For a fixed kind, register the generic versions under your own name:
//! `app.register_cond("sees_food", sees::<FOOD>)`, `remembers::<FOOD>`,
//! `heard::<CHOP>` (a mask of [`ALL_KINDS`](crate::ALL_KINDS) also matches
//! noises of kind 0).

use bevy_ecs::prelude::*;
use npc_kernel::SimClock;

use crate::PerceptionConfig;
use crate::memory::PerceptionMemory;
use crate::sensors::{ALL_KINDS, Sensors};

fn memory(world: &World, agent: Entity) -> Option<&PerceptionMemory> {
    world.get::<PerceptionMemory>(agent)
}

fn interest(world: &World, agent: Entity) -> u32 {
    world
        .get::<Sensors>(agent)
        .map_or(ALL_KINDS, |s| s.interest)
}

/// `sees_kind`: a sensed percept matches the agent's interest mask.
pub fn sees_kind(world: &World, agent: Entity) -> bool {
    let mask = interest(world, agent);
    memory(world, agent).is_some_and(|m| m.senses_kind(mask))
}

/// `remembers_kind`: a remembered percept matches the agent's interest mask.
pub fn remembers_kind(world: &World, agent: Entity) -> bool {
    let mask = interest(world, agent);
    memory(world, agent).is_some_and(|m| m.remembers_kind(mask))
}

/// `heard_noise`: any noise within the configured window.
pub fn heard_noise(world: &World, agent: Entity) -> bool {
    heard::<ALL_KINDS>(world, agent)
}

/// A sensed percept has a kind in `MASK`.
pub fn sees<const MASK: u32>(world: &World, agent: Entity) -> bool {
    memory(world, agent).is_some_and(|m| m.senses_kind(MASK))
}

/// A remembered percept has a kind in `MASK`.
pub fn remembers<const MASK: u32>(world: &World, agent: Entity) -> bool {
    memory(world, agent).is_some_and(|m| m.remembers_kind(MASK))
}

/// A noise with a kind in `MASK` was heard within the configured window.
pub fn heard<const MASK: u32>(world: &World, agent: Entity) -> bool {
    let now = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let window = world.get_resource::<PerceptionConfig>().map_or_else(
        || PerceptionConfig::default().noise_window_ticks,
        |c| c.noise_window_ticks,
    );
    memory(world, agent)
        .and_then(|m| m.last_noise)
        .is_some_and(|n| {
            now.saturating_sub(n.tick) <= window && (MASK == ALL_KINDS || n.kind & MASK != 0)
        })
}
