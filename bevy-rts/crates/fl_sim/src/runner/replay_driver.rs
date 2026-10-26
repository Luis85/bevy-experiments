//! [`ReplayDriver`]: a recorded input stream, as fast as the caller steps (spec §5.3).

use std::collections::BTreeMap;

use fl_protocol::{PlayerCommand, Tick};
use fl_view::{CommandDraft, TickDriver, TickInput};

/// Delivers recorded [`TickInput`]s from `start` on. A tick without a recorded input runs with
/// no commands, so a recording may keep only the ticks that had commands.
#[derive(Debug, Clone)]
pub struct ReplayDriver {
    next: Tick,
    until: Option<Tick>,
    /// Recorded commands not yet delivered, by tick.
    inputs: BTreeMap<Tick, Vec<PlayerCommand>>,
}

impl ReplayDriver {
    /// Replay `inputs` (in any order; inputs for one tick are merged), starting at tick
    /// `start`: inputs before it are dropped. With `until`, ticks from `until` on are never
    /// delivered: [`poll`](TickDriver::poll) returns `None` once `until` is next.
    pub fn new(inputs: Vec<TickInput>, start: Tick, until: Option<Tick>) -> Self {
        let mut by_tick: BTreeMap<Tick, Vec<PlayerCommand>> = BTreeMap::new();
        for input in inputs.into_iter().filter(|input| input.tick >= start) {
            by_tick
                .entry(input.tick)
                .or_default()
                .extend(input.commands);
        }
        for commands in by_tick.values_mut() {
            commands.sort_by_key(|cmd| (cmd.player, cmd.seq));
        }
        ReplayDriver {
            next: start,
            until,
            inputs: by_tick,
        }
    }
}

impl TickDriver for ReplayDriver {
    fn next_tick(&self) -> Tick {
        self.next
    }

    fn input_delay(&self) -> u32 {
        0
    }

    /// # Panics
    /// Always: a replay plays back recorded commands only.
    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand {
        panic!(
            "a ReplayDriver takes no new commands (player {} at tick {})",
            draft.player.0, self.next.0
        );
    }

    fn poll(&mut self) -> Option<TickInput> {
        if self.until.is_some_and(|until| self.next >= until) {
            return None;
        }
        let tick = self.next;
        self.next = tick.next();
        Some(TickInput {
            tick,
            commands: self.inputs.remove(&tick).unwrap_or_default(),
        })
    }

    fn pending(&self) -> Vec<PlayerCommand> {
        self.inputs.values().flatten().cloned().collect()
    }
}
