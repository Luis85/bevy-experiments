//! Command stamping and the [`TickDriver`] trait (spec §5.3, §5.5, ADR-0002, ADR-0007).

use std::collections::BTreeMap;

use fl_protocol::{CommandKind, PlayerCommand, PlayerId, SimId, Tick};

use crate::runner::TickInput;

/// A command before the driver stamps it with `exec_tick` and `seq`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDraft {
    pub player: PlayerId,
    pub units: Vec<SimId>,
    pub kind: CommandKind,
    pub queue: bool,
}

/// Assigns per-player sequence numbers and holds stamped commands until their tick is due.
#[derive(Debug, Clone, Default)]
pub struct CommandStamper {
    /// The seq the next stamp for each player gets.
    next_seq: BTreeMap<PlayerId, u32>,
    /// Stamped commands not yet taken, by `exec_tick`.
    held: BTreeMap<Tick, Vec<PlayerCommand>>,
}

impl CommandStamper {
    /// Stamps `draft` for `exec_tick` with the player's next seq (starting at 0) and holds it.
    pub fn stamp(&mut self, exec_tick: Tick, draft: CommandDraft) -> PlayerCommand {
        let next = self.next_seq.entry(draft.player).or_insert(0);
        let seq = *next;
        *next = seq + 1;
        let command = PlayerCommand {
            exec_tick,
            player: draft.player,
            seq,
            units: draft.units,
            kind: draft.kind,
            queue: draft.queue,
        };
        self.held
            .entry(exec_tick)
            .or_default()
            .push(command.clone());
        command
    }

    /// Removes and returns the held commands with `exec_tick <= tick`, sorted by `(player, seq)`.
    pub fn take_due(&mut self, tick: Tick) -> Vec<PlayerCommand> {
        let later = match tick.0.checked_add(1) {
            Some(after) => self.held.split_off(&Tick(after)),
            None => BTreeMap::new(),
        };
        let due = std::mem::replace(&mut self.held, later);
        let mut commands: Vec<PlayerCommand> = due.into_values().flatten().collect();
        commands.sort_by_key(|command| (command.player, command.seq));
        commands
    }

    /// Every held command, by `exec_tick`, then `(player, seq)`.
    #[must_use]
    pub fn held(&self) -> Vec<PlayerCommand> {
        let mut commands: Vec<PlayerCommand> = self.held.values().flatten().cloned().collect();
        commands.sort_by_key(|command| (command.exec_tick, command.player, command.seq));
        commands
    }

    /// Resets the stamper after a load: holds exactly `pending`, and continues each player's seq
    /// after the larger of its last applied seq and its highest pending seq.
    pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_applied: &BTreeMap<PlayerId, u32>) {
        self.held.clear();
        self.next_seq = last_applied
            .iter()
            .map(|(&player, &seq)| (player, seq + 1))
            .collect();
        for command in pending {
            let next = self.next_seq.entry(command.player).or_insert(0);
            *next = (*next).max(command.seq + 1);
            self.held
                .entry(command.exec_tick)
                .or_default()
                .push(command);
        }
    }
}

/// Decides when the sim ticks and which commands go into each tick.
///
/// Implementations: `LocalDriver` (client, input delay 1), `LockstepDriver`, `ReplayDriver` and
/// `TestDriver` (input delay 0). Each command is delivered in the [`TickInput`] of exactly its
/// `exec_tick`.
pub trait TickDriver {
    /// The tick the next [`TickDriver::poll`] delivers input for.
    fn next_tick(&self) -> Tick;
    /// How many ticks after [`TickDriver::next_tick`] an issued command executes.
    fn input_delay(&self) -> u32;
    /// Stamps `draft` with `exec_tick = next_tick + input_delay` and the player's next seq.
    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand;
    /// The next tick's input if it is ready; advances [`TickDriver::next_tick`] when it is.
    fn poll(&mut self) -> Option<TickInput>;
    /// Commands stamped but not yet delivered.
    fn pending(&self) -> Vec<PlayerCommand>;
}
