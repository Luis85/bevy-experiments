//! Generic, ECS-native behavior-tree runtime.
//!
//! A tree is a shared, serializable [`TreeDef`] stored in the [`AiRegistry`].
//! Each agent carries a [`Brain`] (which tree) and a [`BrainState`] (its own
//! per-node memory). Leaves start *actions*: ordinary components ticked by
//! ordinary systems that report through [`ActiveLeaf::status`].
//!
//! This crate knows nothing about time, ids or game rules. The host decides
//! which agents think and in which order via [`think`].

// Determinism guardrails (plan Task P1). The disallowed types/methods are
// listed in this crate's clippy.toml. An exemption is `#[allow(...)]` on the
// smallest item plus a `// determinism: <why>` comment, and only for code that
// never feeds sim state. Note: `iter_over_hash_type` only knows std's hash
// types (already banned); bevy/hashbrown/`EntityHash*` iteration is caught by
// the type bans, not by this lint.
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

mod builder;
mod registry;
mod runtime;
pub mod trace;
mod tree;

pub use builder::TreeBuilder;
pub use registry::{ActionFns, AiRegistry, CondFn, ScoreFn, StartFn, StopFn, StopReason};
pub use runtime::{
    ActiveLeaf, BbKey, BbValue, Blackboard, Brain, BrainState, BrainStats, ThinkRequest, think,
};
pub use trace::{AiTrace, AiTraceEvent, AiTraceKind, ThinkReason};
pub use tree::{
    Abort, ActionId, CondId, NameRef, NodeDef, NodeIdx, NodeKind, ParamValue, ScoreId, Status,
    TreeDef, TreeId,
};
