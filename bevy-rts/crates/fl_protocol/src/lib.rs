//! fl_protocol: ids, ticks and the command protocol that is the only input to the sim.
//!
//! - [`ids`]: tick, player, sim and interned id newtypes.
//! - [`command`]: the lockstep envelope [`PlayerCommand`] and the [`CommandPayload`] trait (ADR-0007).
//! - [`payloads`]: the core and `debug.*` payloads (ADR-0008). Content ids inside payloads are names.
//! - [`header`]: [`MatchHeader`] and its replay, handshake and save checks (ADR-0007).
//! - [`tables`]: [`StringTables`], the frozen name lists behind every dense id (ADR-0004).
#![deny(clippy::float_arithmetic)]

pub mod command;
pub mod header;
pub mod ids;
pub mod payloads;
pub mod tables;

pub use command::{CommandKind, CommandPayload, PlayerCommand};
pub use header::{Controller, FORMAT_VERSION, HeaderMismatch, MatchHeader};
pub use ids::{CommandTypeId, FactionId, PlayerId, SIM_TICK_HZ, SimId, TeamId, Tick, UnitTypeId};
pub use payloads::{
    DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSetTerrainCmd, DebugSpawnCmd,
    DebugTeleportCmd, DebugWanderCmd, FollowCmd, MoveCmd, RotateCmd, StopCmd,
};
pub use tables::StringTables;
