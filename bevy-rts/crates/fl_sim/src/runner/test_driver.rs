//! [`TestDriver`]: explicit steps for tests and tools (spec §5.3).

use std::collections::BTreeMap;

use fl_protocol::{PlayerCommand, PlayerId, Tick};
use fl_view::{CommandDraft, CommandStamper, TickDriver, TickInput};

/// Hands out a tick on every [`poll`](TickDriver::poll), with the commands stamped for it.
/// Input delay 0 unless set: a command issued before tick `T` runs in tick `T`.
#[derive(Debug, Clone, Default)]
pub struct TestDriver {
    next: Tick,
    delay: u32,
    stamper: CommandStamper,
}

impl TestDriver {
    /// Input delay 0, starting at tick 0.
    pub fn new() -> Self {
        TestDriver::default()
    }

    /// Input delay `delay`, starting at tick 0.
    pub fn with_delay(delay: u32) -> Self {
        TestDriver {
            delay,
            ..TestDriver::default()
        }
    }

    /// The same driver, delivering `next` first (a sim that already ran, or was loaded).
    #[must_use]
    pub fn starting_at(self, next: Tick) -> Self {
        TestDriver { next, ..self }
    }

    /// After a load: hold exactly `pending`, and continue each player's seq after its
    /// `last_seq` (the sim's [`last_seqs`](crate::last_seqs)) or its highest pending seq.
    pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_seq: &BTreeMap<PlayerId, u32>) {
        self.stamper.restore(pending, last_seq);
    }
}

impl TickDriver for TestDriver {
    fn next_tick(&self) -> Tick {
        self.next
    }

    fn input_delay(&self) -> u32 {
        self.delay
    }

    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand {
        let exec_tick = Tick(self.next.0 + self.delay);
        self.stamper.stamp(exec_tick, draft)
    }

    /// Always ready.
    fn poll(&mut self) -> Option<TickInput> {
        let tick = self.next;
        self.next = tick.next();
        Some(TickInput {
            tick,
            commands: self.stamper.take_due(tick),
        })
    }

    fn pending(&self) -> Vec<PlayerCommand> {
        self.stamper.held()
    }
}
