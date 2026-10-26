//! The runner protocol between a client (or tool) and the `SimRunner` (spec §5.3, ADR-0002).

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use fl_protocol::{MatchHeader, PlayerCommand, SimId, Tick};
use serde::{Deserialize, Serialize};

use crate::event::SimEvent;
use crate::journal::JournalEntry;
use crate::snapshot::SimSnapshot;

/// The input of one tick: the commands whose `exec_tick` is `tick`, in `(player, seq)` order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickInput {
    pub tick: Tick,
    pub commands: Vec<PlayerCommand>,
}

/// The output of one tick. Lives in-process only; it is never serialized.
#[derive(Debug, Clone, PartialEq)]
pub struct TickOutput {
    pub tick: Tick,
    /// The state hash, when one was due this tick.
    pub hash: Option<u64>,
    pub snapshot: Arc<SimSnapshot>,
    /// Events raised during the tick, in order.
    pub events: Vec<SimEvent>,
    /// The tick's journal entries (debug builds).
    pub journal: Vec<JournalEntry>,
}

/// A read-only question the runner answers between ticks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SimQuery {
    /// Inspect one entity.
    Entity(SimId),
    WorldSummary,
    /// Journal entries from `tick` on, at most `limit`.
    JournalSince {
        tick: Tick,
        limit: u32,
    },
    Perf,
    /// Save the match; `pending` are the commands stamped but not yet delivered.
    Save {
        pending: Vec<PlayerCommand>,
    },
    Header,
    /// The named debug query `name` (ADR-0015), with its arguments; `core.debug_queries`
    /// lists every registered name.
    Debug {
        name: String,
        args: serde_json::Value,
    },
}

/// The answer to a [`SimQuery`] or a [`RunnerControl`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SimReply {
    /// `None` when the entity doesn't exist.
    Entity(Option<serde_json::Value>),
    WorldSummary(serde_json::Value),
    Journal(Vec<JournalEntry>),
    Perf(serde_json::Value),
    /// The encoded save.
    Save(Vec<u8>),
    Header(MatchHeader),
    /// The answer of a [`SimQuery::Debug`]: JSON, with `Fx` as its raw `i64`.
    Debug(serde_json::Value),
    /// The query or control failed: an unknown debug query, a debug query's own error, a save
    /// that cannot be taken now, a refused reload.
    Error(String),
    /// A [`RunnerControl::ReloadContent`] applied: the match runs on from `tick` under the
    /// content with `rules_hash`.
    Reloaded {
        tick: Tick,
        rules_hash: u64,
    },
}

/// A message to the runner. Non-exhaustive (ADR-0012 refinement, R80), like [`SimQuery`] and
/// [`SimReply`]: a match outside this crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToRunner {
    Tick(TickInput),
    /// A query; the reply carries the same `id`.
    Query {
        id: u64,
        query: SimQuery,
    },
    /// A change to the running match that no command makes (testbed only); answered with a
    /// [`FromRunner::Reply`] carrying the same `id`.
    Control {
        id: u64,
        control: RunnerControl,
    },
    Shutdown,
}

/// What a [`ToRunner::Control`] asks the runner to do, between ticks.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RunnerControl {
    /// Apply new content with the same ID sets in place (a value-only hot reload, ADR-0012):
    /// answered with [`SimReply::Reloaded`], or [`SimReply::Error`] when the runner refuses it
    /// (no cheats, other ID sets, not content).
    ReloadContent(OpaqueContent),
}

/// Content handed to the runner without naming its type: `fl_view` does not depend on
/// `fl_content`, so the runner downcasts it (to `fl_content::ContentDb`). Two handles are
/// equal when they share one allocation ([`Arc::ptr_eq`]), never by value.
#[derive(Clone)]
pub struct OpaqueContent(pub Arc<dyn Any + Send + Sync>);

impl fmt::Debug for OpaqueContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpaqueContent")
    }
}

impl PartialEq for OpaqueContent {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// A message from the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum FromRunner {
    Tick(TickOutput),
    Reply {
        id: u64,
        reply: SimReply,
    },
    /// The runner stopped; the match is over (spec §5.3, failures).
    Failed(RunnerFailure),
}

/// Why the runner stopped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerFailure {
    /// The tick that failed.
    pub tick: Tick,
    pub message: String,
}

/// The client's end of the runner channels.
#[derive(Debug, Clone)]
pub struct RunnerPort {
    pub tx: crossbeam_channel::Sender<ToRunner>,
    pub rx: crossbeam_channel::Receiver<FromRunner>,
}

/// The runner's end of the runner channels.
#[derive(Debug, Clone)]
pub struct RunnerEnd {
    pub rx: crossbeam_channel::Receiver<ToRunner>,
    pub tx: crossbeam_channel::Sender<FromRunner>,
}

/// A connected pair of unbounded channels between a client and a runner.
#[must_use]
pub fn runner_channel() -> (RunnerPort, RunnerEnd) {
    let (to_runner_tx, to_runner_rx) = crossbeam_channel::unbounded();
    let (from_runner_tx, from_runner_rx) = crossbeam_channel::unbounded();
    (
        RunnerPort {
            tx: to_runner_tx,
            rx: from_runner_rx,
        },
        RunnerEnd {
            rx: to_runner_rx,
            tx: from_runner_tx,
        },
    )
}
