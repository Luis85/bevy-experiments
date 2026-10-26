//! `npc_agent_io`: external agent control, in real time (spec
//! `docs/superpowers/specs/2026-10-23-agent-control-design.md`).
//!
//! An agent (Claude Code through `npcctl`, an MCP client, a script) attaches
//! to a unit as a controller, observes it from its own point of view, and
//! acts through the affordances it is offered, while the world keeps
//! running at its normal tick rate.
//!
//! - [`AgentIoPlugin`]: the controller components, the recorded commands
//!   (`AttachController`, `DetachController`, `AgentAct`, `AgentCancel`,
//!   `AgentClear`), the injected `ExternalControl` branch of every tree and
//!   its `ExternalAction` leaf ([`control`]), the built-in `self` section and
//!   `wait` affordance, the event buffer ([`events`]) and stats ([`stats`]).
//!   Opt-in: without it nothing of this exists and content behaves exactly
//!   as before.
//! - Registries ([`registry`], [`AgentIoAppExt`]): capability crates and
//!   content add observation sections, perception, affordances and action
//!   handlers by name.
//! - Methods ([`api`]): `agent/attach|detach|observe|act|act_and_wait|cancel|clear|
//!   events|stats|list_units`, the notes methods `agent/notes/list|add|
//!   update` (the unit's controller) and `notes/list|add|update` (the
//!   player), and `world/state`, transport-agnostic.
//! - Hosts: [`session`] runs a scenario headless in real time behind a
//!   JSON-RPC-over-HTTP server ([`http`], std only, BRP wire format) with
//!   `world/load_scenario|reset` and `session/stop` (`npcctl serve`); with
//!   feature `remote`, [`remote`] registers the same methods on Bevy's BRP
//!   (windowed games, through `npc_tools`' remote plugin).
//! - Clients: [`http::RpcClient`] (the `npcctl agent|world` CLI), [`mcp`]
//!   (a stdio MCP server, `npcctl mcp`) and [`greedy`] (a scripted
//!   reference controller, `examples/greedy_controller`).
//! - Notes ([`notes`]): a host-side [`NotesStore`] per agent folder (diary,
//!   memory, proposals as one Obsidian-compatible markdown file per entry;
//!   never sim state, never hashed or snapshotted), served by the notes
//!   methods through the host's `NotesDirs` (unit to folder), `NotesClock`
//!   and `NotesStores` (with each folder's [`NotesOwner`]).
//! - Scenarios ([`scenario`]): versioned JSON files, loaded by a loader the
//!   content crate registers; sessions record replays that verify bit for
//!   bit without the agent.
//!
//! Determinism: observations only read the world; every action is a
//! `SimCommand` stamped with the tick it applies at, so the journal and the
//! replay of an agent session are reproducible without the agent. Wall
//! clocks are read only for latency stats, request deadlines and session
//! pacing ([`stats::wall_now`]).

// Determinism guardrails, as in the sim crates (this crate's clippy.toml is
// the kernel's). The wall-clock exemptions are tooling-only and marked.
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod api;
pub mod builtin;
pub mod control;
pub mod events;
pub mod greedy;
pub mod http;
pub mod mcp;
pub mod notes;
pub mod observe;
pub mod registry;
#[cfg(feature = "remote")]
pub mod remote;
pub mod scenario;
pub mod session;
pub mod stats;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::hash::{SimHash, SimHashRegistry};
use npc_kernel::registry::{KernelAppExt, add_tree_transform};
use npc_kernel::snapshot::SnapshotAppExt;

pub use api::{AgentIoConfig, RpcError, RpcResult, call};
pub use control::{
    AgentAct, AgentCancel, AgentClear, AgentIoSet, AttachController, ControlMode, DetachController,
    ExternalController, ExternalQueue, IdlePolicy, QueuedAction, Step,
};
pub use events::{AgentEvent, AgentEvents};
pub use notes::{
    Book, NewNote, Note, NoteChange, NotesError, NotesOwner, NotesStore, ProposalStatus, Stamp,
    Unreadable, Writer,
};
pub use registry::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, AgentRegistry, Rejection,
    Section, VerbGateFn, verb_gate,
};
pub use scenario::{ScenarioDoc, ScenarioLoaders};

/// Installs external agent control (see the crate docs). Add it next to
/// `NpcPlugins` and the content plugins, before the app is finished (it
/// registers an action, a condition, a tree transform, components and
/// commands, which must happen before the freeze).
///
/// Content hash: it adds names (and a branch to every tree), so a replay
/// recorded with it only verifies with it, and vice versa.
pub struct AgentIoPlugin {
    /// Turn the journal on (keeping its verbosity): the event stream and
    /// observations' recent events come from it. Default `true`.
    pub enable_journal: bool,
}

impl Default for AgentIoPlugin {
    fn default() -> Self {
        Self {
            enable_journal: true,
        }
    }
}

impl Plugin for AgentIoPlugin {
    fn build(&self, app: &mut App) {
        app.register_action(
            control::EXTERNAL_ACTION,
            control::external_action_start,
            control::external_action_stop,
        )
        .register_cond(
            control::EXTERNAL_CONTROL_COND,
            control::cond_external_control,
        )
        .register_command::<AttachController>()
        .register_command::<DetachController>()
        .register_command::<AgentAct>()
        .register_command::<AgentCancel>()
        .register_command::<AgentClear>();
        add_tree_transform(
            app.world_mut(),
            "npc_agent_io.external_control",
            control::inject_external_control,
        );
        {
            let mut hashed = app.world_mut().get_resource_or_init::<SimHashRegistry>();
            hashed.add_component::<ExternalController>();
            hashed.add_component::<ExternalQueue>();
        }
        app.register_snapshot_component_named::<ExternalController>(ExternalController::NAME)
            .register_snapshot_component_named::<ExternalQueue>(ExternalQueue::NAME)
            .register_snapshot_resource_with(
                "agent_io_state",
                api::save_agent_io_state,
                api::load_agent_io_state,
            );
        app.register_observation("self", observe::self_section)
            .register_affordances("npc_agent_io.wait", builtin::wait_affordances)
            .register_action_handler("wait", builtin::wait_handler);
        app.init_resource::<AgentEvents>()
            .init_resource::<stats::AgentStats>()
            .init_resource::<AgentIoConfig>()
            .init_resource::<api::AgentIoState>();
        control::configure_sets(app);
        app.add_systems(
            npc_kernel::NpcSimTick,
            (
                control::external_dispatch_system.in_set(AgentIoSet::Dispatch),
                control::external_advance_system.in_set(AgentIoSet::Advance),
            ),
        );
        if self.enable_journal {
            let world = app.world_mut();
            let verbose = world
                .get_resource::<npc_kernel::Journal>()
                .is_some_and(|j| j.verbose);
            npc_kernel::journal::set_journal(world, true, verbose);
        }
    }
}
