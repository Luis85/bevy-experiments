//! Optional think observer.
//!
//! When an [`AiTrace`] resource exists and is `enabled`, [`think`](crate::think)
//! appends one [`AiTraceEvent`] per decision it makes, in the order it makes
//! them. Without the resource (or with `enabled == false`) nothing is
//! recorded and nothing is allocated. Recording never changes what `think`
//! does: conditions are evaluated exactly as often with or without it.
//!
//! The host drains `events` (e.g. `npc_kernel`'s journal maps them to
//! `SimId`s and node names every tick). Nobody else clears the buffer, so a
//! host that enables the trace without draining it grows it without bound.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::registry::StopReason;
use crate::tree::{ActionId, CondId, NodeIdx, Status};

/// Think-event buffer. Insert it (with `enabled: true`) to observe `think`.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct AiTrace {
    /// Record events at all.
    pub enabled: bool,
    /// Also record every condition evaluation ([`AiTraceKind::CondEval`]).
    pub verbose: bool,
    /// Events in decision order; drained by the host.
    pub events: Vec<AiTraceEvent>,
}

/// One decision of `think` for `agent`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiTraceEvent {
    pub agent: Entity,
    pub kind: AiTraceKind,
}

/// What `think` decided. Node indices refer to the agent's `TreeDef::nodes`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AiTraceKind {
    /// The agent is evaluated this call (first event of each evaluation).
    ThinkStart { reason: ThinkReason },
    /// A leaf's `start` ran and returned `status`.
    LeafStart {
        node: NodeIdx,
        action: ActionId,
        status: Status,
    },
    /// A leaf's `stop` ran: `Finished` when its result was consumed,
    /// `Aborted` when its branch was interrupted. Emitted just before the
    /// action's `stop` function runs.
    LeafStop {
        node: NodeIdx,
        action: ActionId,
        reason: StopReason,
    },
    /// Reactive guard `guard` passed and aborted the running branch;
    /// `aborted` is the running path (root to leaf) that was interrupted.
    /// Emitted before the aborted leaf's `LeafStop`.
    Abort {
        guard: NodeIdx,
        aborted: Vec<NodeIdx>,
    },
    /// A condition was evaluated for guard `node` (only when `verbose`).
    CondEval {
        node: NodeIdx,
        cond: CondId,
        result: bool,
    },
    /// The root finished this call with `status` (Success or Failure); the
    /// running path is now empty. Not emitted while the tree is Running.
    RootDone { status: Status },
}

/// Why an agent was evaluated (see [`think`](crate::think)'s skip rule).
/// When several apply, the first in declaration order wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThinkReason {
    /// It had a `ThinkRequest`.
    Request,
    /// Its active leaf had finished (status not `Running`).
    LeafDone,
    /// It had no active leaf.
    NoLeaf,
}
