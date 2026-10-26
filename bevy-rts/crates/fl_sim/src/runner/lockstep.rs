//! [`LockstepDriver`]: advances only when every player's commands for the tick are present
//! (spec §5.3, ADR-0007). M0 ships the in-process [`loopback`] [`NetSession`], with no
//! sockets, to prove the path.

use std::collections::BTreeMap;

use crossbeam_channel::{Receiver, Sender};
use fl_protocol::{PlayerCommand, PlayerId, Tick};
use fl_view::{CommandDraft, CommandStamper, TickDriver, TickInput};

/// Carries each player's command batches to every peer.
pub trait NetSession: Send {
    /// Send `player`'s batch for exec tick `tick` (possibly empty) to every peer.
    fn send(&mut self, tick: Tick, player: PlayerId, commands: Vec<PlayerCommand>);
    /// Every batch received since the last call, in arrival order.
    fn recv(&mut self) -> Vec<(Tick, PlayerId, Vec<PlayerCommand>)>;
}

type Batch = (Tick, PlayerId, Vec<PlayerCommand>);

/// One player's end of an in-process [`loopback`] network.
#[derive(Debug)]
pub struct LoopbackSession {
    peers: Vec<Sender<Batch>>,
    inbox: Receiver<Batch>,
}

/// One session per player in `players`; every send reaches every session, the sender's
/// included. Sends to a dropped session are ignored.
pub fn loopback(players: &[PlayerId]) -> Vec<LoopbackSession> {
    let (senders, receivers): (Vec<Sender<Batch>>, Vec<Receiver<Batch>>) = players
        .iter()
        .map(|_| crossbeam_channel::unbounded())
        .unzip();
    receivers
        .into_iter()
        .map(|inbox| LoopbackSession {
            peers: senders.clone(),
            inbox,
        })
        .collect()
}

impl NetSession for LoopbackSession {
    fn send(&mut self, tick: Tick, player: PlayerId, commands: Vec<PlayerCommand>) {
        for peer in &self.peers {
            let _ = peer.send((tick, player, commands.clone()));
        }
    }

    fn recv(&mut self) -> Vec<Batch> {
        self.inbox.try_iter().collect()
    }
}

/// Lockstep over a [`NetSession`]: the local player's commands go out as one batch per exec
/// tick, and tick `T` runs once every player's batch for `T` is in.
///
/// [`poll`](TickDriver::poll) for tick `T`:
/// 1. sends the local batch for exec tick `T + delay` (every command issued since the last
///    poll, possibly none), unless an earlier poll for `T` already sent it;
/// 2. drains the session;
/// 3. returns the input for `T` once every player's batch for `T` is present. Ticks below
///    `delay` need no batches: nobody can have issued a command for them.
///
/// [`issue`](TickDriver::issue) stamps `exec_tick = next_tick + delay`, or one tick later
/// while the driver waits for a peer: that batch has already been sent.
pub struct LockstepDriver<S: NetSession> {
    local: PlayerId,
    players: Vec<PlayerId>,
    session: S,
    delay: u32,
    next: Tick,
    /// The exec tick of the next local batch to send.
    next_batch: Tick,
    /// Local commands not yet sent.
    stamper: CommandStamper,
    /// Received batches (the local one included) not yet delivered, by exec tick and player.
    batches: BTreeMap<Tick, BTreeMap<PlayerId, Vec<PlayerCommand>>>,
}

impl<S: NetSession> LockstepDriver<S> {
    /// `local` issues commands; `players` (sorted and deduplicated here) all must send a
    /// batch for every tick from `delay` on.
    pub fn new(local: PlayerId, mut players: Vec<PlayerId>, session: S, delay: u32) -> Self {
        players.sort_unstable();
        players.dedup();
        debug_assert!(
            players.contains(&local),
            "the local player is one of the players"
        );
        LockstepDriver {
            local,
            players,
            session,
            delay,
            next: Tick(0),
            next_batch: Tick(delay),
            stamper: CommandStamper::default(),
            batches: BTreeMap::new(),
        }
    }

    fn receive(&mut self) {
        for (tick, player, commands) in self.session.recv() {
            if tick < self.next || !self.players.contains(&player) {
                tracing::warn!(
                    tick = tick.0,
                    player = player.0,
                    "lockstep batch ignored: tick already ran or unknown player"
                );
                continue;
            }
            self.batches
                .entry(tick)
                .or_default()
                .insert(player, commands);
        }
    }

    fn ready(&self, tick: Tick) -> bool {
        tick.0 < self.delay
            || self.batches.get(&tick).is_some_and(|by_player| {
                self.players
                    .iter()
                    .all(|player| by_player.contains_key(player))
            })
    }
}

impl<S: NetSession> TickDriver for LockstepDriver<S> {
    fn next_tick(&self) -> Tick {
        self.next
    }

    fn input_delay(&self) -> u32 {
        self.delay
    }

    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand {
        debug_assert_eq!(
            draft.player, self.local,
            "a lockstep peer issues only its own player's commands"
        );
        self.stamper.stamp(self.next_batch, draft)
    }

    fn poll(&mut self) -> Option<TickInput> {
        let tick = self.next;
        let exec = Tick(tick.0 + self.delay);
        if self.next_batch == exec {
            let batch = self.stamper.take_due(exec);
            self.batches
                .entry(exec)
                .or_default()
                .insert(self.local, batch.clone());
            self.session.send(exec, self.local, batch);
            self.next_batch = exec.next();
        }
        self.receive();
        if !self.ready(tick) {
            return None;
        }
        let mut commands: Vec<PlayerCommand> = self
            .batches
            .remove(&tick)
            .into_iter()
            .flat_map(BTreeMap::into_values)
            .flatten()
            .collect();
        commands.sort_by_key(|cmd| (cmd.player, cmd.seq));
        self.next = tick.next();
        Some(TickInput { tick, commands })
    }

    /// The local commands not yet delivered: unsent, and sent but not yet run.
    fn pending(&self) -> Vec<PlayerCommand> {
        let mut commands = self.stamper.held();
        commands.extend(
            self.batches
                .values()
                .filter_map(|by_player| by_player.get(&self.local))
                .flatten()
                .cloned(),
        );
        commands.sort_by_key(|cmd| (cmd.exec_tick, cmd.player, cmd.seq));
        commands
    }
}
