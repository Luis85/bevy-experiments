//! Per-tick sim event buffers (fired, impact, destroyed, …) that the client drains.

use bevy_ecs::resource::Resource;

/// The events of type `E` raised during the last tick. The core buffer is
/// `SimEvents<fl_view::SimEvent>`, Transient `"core.sim_events"`: reset to empty first thing
/// in [`InputSet::Housekeeping`](crate::InputSet::Housekeeping).
#[derive(Resource)]
pub struct SimEvents<E: Send + Sync + 'static> {
    events: Vec<E>,
}

impl<E: Send + Sync + 'static> Default for SimEvents<E> {
    fn default() -> Self {
        SimEvents { events: Vec::new() }
    }
}

impl<E: Send + Sync + 'static> SimEvents<E> {
    pub fn push(&mut self, event: E) {
        self.events.push(event);
    }

    /// Take every event, in the order they were pushed.
    pub fn drain(&mut self) -> std::vec::Drain<'_, E> {
        self.events.drain(..)
    }

    /// Events in the order they were pushed.
    pub fn iter(&self) -> impl Iterator<Item = &E> + '_ {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Drop every event, keeping the buffer: the Transient reset of `"core.sim_events"`.
    pub fn clear(&mut self) {
        self.events.clear();
    }
}
