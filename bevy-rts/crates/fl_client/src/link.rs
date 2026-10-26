//! The client's link to the sim runner: the port, received snapshots and events, runner status
//! and query replies (spec §5.3, ADR-0002).
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use crossbeam_channel::RecvTimeoutError;
use fl_protocol::{
    CommandKind, CommandPayload, CommandTypeId, MatchHeader, StringTables, Tick, tables,
};
use fl_view::{
    FromRunner, JournalEntry, RunnerControl, RunnerFailure, RunnerPort, SimEvent, SimQuery,
    SimReply, SimSnapshot, TickInput, ToRunner,
};

/// The channel pair to the runner and the header of the match it runs.
#[derive(Resource)]
pub struct RunnerLink {
    pub port: RunnerPort,
    pub header: MatchHeader,
}

/// The last two snapshots received; the view interpolates from `prev` to `curr`.
#[derive(Resource, Default)]
pub struct SnapshotBuffer {
    pub prev: Option<Arc<SimSnapshot>>,
    pub curr: Option<Arc<SimSnapshot>>,
}

impl SnapshotBuffer {
    /// Makes `snapshot` current and the current one previous.
    pub fn push(&mut self, snapshot: Arc<SimSnapshot>) {
        self.prev = self.curr.replace(snapshot);
    }
}

/// Real time since [`SnapshotBuffer::curr`] arrived: the receive timeline the view interpolates
/// on (ruling R42), since a tick's snapshot arrives a frame or more after its input was sent.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SnapshotAge(pub Duration);

/// One sim event, written once per event of every received tick, in order.
#[derive(Message, Clone, Debug)]
pub struct SimEventMsg(pub SimEvent);

/// One journal entry, written once per entry of every received tick, in order.
#[derive(Message, Clone, Debug)]
pub struct SimJournalMsg(pub JournalEntry);

/// How many journal entries [`JournalBuffer`] keeps by default.
pub const JOURNAL_CAPACITY: usize = 10_000;

/// The most recent journal entries received, oldest first (spec §15 B). Observe-only.
///
/// When full, each new entry drops the oldest one and counts it in [`JournalBuffer::dropped`].
#[derive(Resource, Debug)]
pub struct JournalBuffer {
    entries: VecDeque<JournalEntry>,
    capacity: usize,
    dropped: u64,
}

impl Default for JournalBuffer {
    fn default() -> Self {
        JournalBuffer::with_capacity(JOURNAL_CAPACITY)
    }
}

impl JournalBuffer {
    /// An empty buffer keeping at most `capacity` entries (at least one).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        JournalBuffer {
            entries: VecDeque::new(),
            capacity: capacity.max(1),
            dropped: 0,
        }
    }

    /// Appends `entry`, dropping the oldest entry when full.
    pub fn push(&mut self, entry: JournalEntry) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
            self.dropped += 1;
        }
        self.entries.push_back(entry);
    }

    /// The kept entries whose tick is `tick` or later, oldest first.
    pub fn since(&self, tick: Tick) -> impl Iterator<Item = &JournalEntry> {
        let start = self.entries.partition_point(|entry| entry.tick < tick);
        self.entries.range(start..)
    }

    /// Every kept entry, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &JournalEntry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many entries were dropped to stay within capacity.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

/// What the client knows about the runner.
#[derive(Resource, Default)]
pub struct RunnerStatus {
    /// Set when the runner reported a failure or vanished; the match is over.
    pub failed: Option<RunnerFailure>,
    /// The last tick received.
    pub last_tick: Option<Tick>,
    /// The last state hash received.
    pub last_hash: Option<u64>,
}

/// Runner messages read outside `drive_runner` (by a remote debug method waiting for a query
/// reply), oldest first; `drive_runner` handles them before reading the channel again, so no
/// tick output is lost or reordered.
#[derive(Resource, Default, Debug)]
pub struct RunnerBacklog(pub VecDeque<FromRunner>);

/// What the running match was driven with, for a replay of it: when present, every
/// [`TickInput`] sent to the runner (by `drive_runner` and `fl.step`) is
/// [recorded](InputLog::record), and `drive_runner` appends every state hash it receives. A
/// loaded scenario or save starts a new log; a hot-reload rebuild continues the match, and
/// its log.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct InputLog {
    /// How many ticks were sent.
    pub ticks: u32,
    /// The inputs sent that carry commands, in tick order.
    pub inputs: Vec<TickInput>,
    pub hashes: Vec<(Tick, u64)>,
}

impl InputLog {
    /// Counts `input`, sent to the runner, and keeps it if it carries commands.
    pub fn record(&mut self, input: &TickInput) {
        self.ticks = self.ticks.saturating_add(1);
        if !input.commands.is_empty() {
            self.inputs.push(input.clone());
        }
    }
}

/// The longest [`wait_for_reply`] waits by default: a hot reload rebuilds the navigation data
/// or saves the match before the runner answers.
pub const REPLY_WAIT: Duration = Duration::from_secs(10);

/// Replies to [`send_query`] calls, by query id, until taken.
#[derive(Resource, Default)]
pub struct PendingReplies {
    replies: BTreeMap<u64, SimReply>,
    /// Queries whose replies nobody waits for any more; dropped on arrival.
    abandoned: BTreeSet<u64>,
    next_id: u64,
}

impl PendingReplies {
    /// Removes and returns the reply to query `id`, once it has arrived.
    pub fn take(&mut self, id: u64) -> Option<SimReply> {
        self.replies.remove(&id)
    }

    /// Stores the reply to query `id`, unless the query was abandoned.
    pub fn insert(&mut self, id: u64, reply: SimReply) {
        if !self.abandoned.remove(&id) {
            self.replies.insert(id, reply);
        }
    }

    /// Stops waiting for query `id`: its reply is dropped, now or when it arrives.
    pub fn abandon(&mut self, id: u64) {
        if self.replies.remove(&id).is_none() {
            self.abandoned.insert(id);
        }
    }

    /// Drops every reply and abandoned id, for a new runner: the old one's replies never
    /// arrive. Ids stay unique across runners, so a query of the new runner never shares an
    /// id with one somebody still holds from the old (final review I4).
    pub fn forget_runner(&mut self) {
        self.replies.clear();
        self.abandoned.clear();
    }

    fn allocate(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Encodes `payload` as a [`CommandKind`] whose type id is the index of `P::NAME` in the
/// `"commands"` string table.
///
/// # Errors
///
/// If the table doesn't list `P::NAME`, the id doesn't fit a [`CommandTypeId`], or the payload
/// fails to encode.
pub fn encode_command<P: CommandPayload>(
    tables: &StringTables,
    payload: &P,
) -> Result<CommandKind, String> {
    let payload = postcard::to_allocvec(payload)
        .map_err(|err| format!("command '{}' failed to encode: {err}", P::NAME))?;
    encode_command_bytes(tables, P::NAME, payload)
}

/// A [`CommandKind`] of the command `name` with the postcard `payload`; its type id is the
/// index of `name` in the `"commands"` string table.
///
/// # Errors
///
/// If the table doesn't list `name` or the id doesn't fit a [`CommandTypeId`].
pub fn encode_command_bytes(
    tables: &StringTables,
    name: &str,
    payload: Vec<u8>,
) -> Result<CommandKind, String> {
    let id = tables.id_of(tables::COMMANDS, name).ok_or_else(|| {
        format!(
            "command '{name}' is not in the match's \"{}\" table",
            tables::COMMANDS
        )
    })?;
    let type_id = u16::try_from(id)
        .map(CommandTypeId)
        .map_err(|_| format!("command '{name}' has id {id}, beyond u16"))?;
    Ok(CommandKind { type_id, payload })
}

/// Sends `q` to the runner and returns its id; the reply lands in `replies` under that id.
///
/// If the runner is gone the reply never arrives, and [`RunnerStatus::failed`] says why.
pub fn send_query(link: &RunnerLink, replies: &mut PendingReplies, q: SimQuery) -> u64 {
    let id = replies.allocate();
    if link.port.tx.send(ToRunner::Query { id, query: q }).is_err() {
        tracing::warn!(id, "query not sent: the runner is gone");
    }
    id
}

/// Sends `control` to the runner and returns its id; the reply lands in `replies` under that
/// id, as a query's does.
pub fn send_control(
    link: &RunnerLink,
    replies: &mut PendingReplies,
    control: RunnerControl,
) -> u64 {
    let id = replies.allocate();
    if link
        .port
        .tx
        .send(ToRunner::Control { id, control })
        .is_err()
    {
        tracing::warn!(id, "control not sent: the runner is gone");
    }
    id
}

/// The wall-clock end of a bounded wait for the runner ([`wait_for_reply`], `fl.step`).
#[expect(
    clippy::disallowed_types,
    reason = "a blocking wait for the runner is bounded in wall-clock time (spec §15)"
)]
pub(crate) struct Deadline(std::time::Instant);

#[expect(
    clippy::disallowed_types,
    reason = "a blocking wait for the runner is bounded in wall-clock time (spec §15)"
)]
impl Deadline {
    /// `timeout` from now.
    pub(crate) fn after(timeout: Duration) -> Deadline {
        Deadline(std::time::Instant::now() + timeout)
    }

    pub(crate) fn remaining(&self) -> Duration {
        self.0.saturating_duration_since(std::time::Instant::now())
    }
}

/// Why a bounded wait for the runner ended without what it waited for.
pub(crate) enum WaitError {
    /// The deadline passed.
    Timeout,
    /// No match runs, or the runner failed or is gone.
    Failed(String),
}

/// Reads one message from the runner, waiting at most until `deadline`: the one place that
/// reads the runner outside `drive_runner`. A reply goes to [`PendingReplies`]; a tick output
/// (its tick is returned) or a failure goes to the [`RunnerBacklog`] for `drive_runner`, so
/// nothing is lost or reordered.
pub(crate) fn read_runner(
    world: &mut World,
    deadline: &Deadline,
) -> Result<Option<Tick>, WaitError> {
    world.init_resource::<PendingReplies>();
    world.init_resource::<RunnerBacklog>();
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return Err(WaitError::Timeout);
    }
    let link = world
        .get_resource::<RunnerLink>()
        .ok_or_else(|| WaitError::Failed("no match is running".to_owned()))?;
    match link.port.rx.recv_timeout(remaining) {
        Ok(FromRunner::Reply { id, reply }) => {
            world.resource_mut::<PendingReplies>().insert(id, reply);
            Ok(None)
        }
        Ok(FromRunner::Tick(output)) => {
            let tick = output.tick;
            world
                .resource_mut::<RunnerBacklog>()
                .0
                .push_back(FromRunner::Tick(output));
            Ok(Some(tick))
        }
        Ok(FromRunner::Failed(failure)) => {
            let text = failure_text(&failure);
            world
                .resource_mut::<RunnerBacklog>()
                .0
                .push_back(FromRunner::Failed(failure));
            Err(WaitError::Failed(text))
        }
        Err(RecvTimeoutError::Timeout) => Err(WaitError::Timeout),
        Err(RecvTimeoutError::Disconnected) => {
            Err(WaitError::Failed("the sim runner disconnected".to_owned()))
        }
    }
}

/// Blocks until the reply to query or control `id` arrives, for at most `timeout`, and
/// returns it ([`SimReply::Error`] included). Messages read meanwhile are kept as
/// [`read_runner`] keeps them.
///
/// # Errors
///
/// No match is running, the runner failed or disconnected, or `timeout` passed. Then `id` is
/// abandoned: its late reply is dropped when it arrives.
pub fn wait_for_reply(world: &mut World, id: u64, timeout: Duration) -> Result<SimReply, String> {
    world.init_resource::<PendingReplies>();
    let deadline = Deadline::after(timeout);
    let result = loop {
        if let Some(reply) = world.resource_mut::<PendingReplies>().take(id) {
            return Ok(reply);
        }
        if let Some(failure) = world
            .get_resource::<RunnerStatus>()
            .and_then(|status| status.failed.as_ref())
        {
            break Err(failure_text(failure));
        }
        match read_runner(world, &deadline) {
            Ok(_) => {}
            Err(WaitError::Timeout) => {
                break Err(format!(
                    "the sim runner did not answer within {} ms",
                    timeout.as_millis()
                ));
            }
            Err(WaitError::Failed(message)) => break Err(message),
        }
    };
    world.resource_mut::<PendingReplies>().abandon(id);
    result
}

/// How a runner failure reads in an error.
pub(crate) fn failure_text(failure: &RunnerFailure) -> String {
    format!(
        "the sim runner failed at tick {}: {}",
        failure.tick.0, failure.message
    )
}
