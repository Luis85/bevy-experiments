//! Core and `debug.*` command payloads (ADR-0007, ADR-0008).
//!
//! Every payload denies unknown fields. Content ids are names, never dense ids. Every
//! `debug.*` payload is [`DEBUG_ONLY`](CommandPayload::DEBUG_ONLY).

use fl_math::{Angle, Fx, FxVec2};
use serde::{Deserialize, Serialize};

use crate::command::CommandPayload;
use crate::ids::{PlayerId, SimId};

/// `"core.move"`: drive to `target`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveCmd {
    pub target: FxVec2,
}

impl CommandPayload for MoveCmd {
    const NAME: &'static str = "core.move";
}

/// `"core.stop"`: drop current orders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopCmd;

impl CommandPayload for StopCmd {
    const NAME: &'static str = "core.stop";
}

/// `"core.rotate"`: each unit turns its hull to face `toward`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotateCmd {
    pub toward: FxVec2,
}

impl CommandPayload for RotateCmd {
    const NAME: &'static str = "core.rotate";
}

/// `"core.follow"`: follow `leader`, keeping the unit's current offset from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowCmd {
    pub leader: SimId,
}

impl CommandPayload for FollowCmd {
    const NAME: &'static str = "core.follow";
}

/// `"debug.spawn"`: spawn the unit type named `unit` (e.g. `"base.de.pz3"`) for `owner`.
/// Needs no units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugSpawnCmd {
    pub unit: String,
    pub owner: PlayerId,
    pub pos: FxVec2,
    pub heading: Angle,
}

impl CommandPayload for DebugSpawnCmd {
    const NAME: &'static str = "debug.spawn";
    const NEEDS_UNITS: bool = false;
    const DEBUG_ONLY: bool = true;
}

/// `"debug.wander"`: wander within `radius` metres of the current position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugWanderCmd {
    pub radius: Fx,
}

impl CommandPayload for DebugWanderCmd {
    const NAME: &'static str = "debug.wander";
    const DEBUG_ONLY: bool = true;
}

/// `"debug.set_owner"`: hand the units to `owner`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugSetOwnerCmd {
    pub owner: PlayerId,
}

impl CommandPayload for DebugSetOwnerCmd {
    const NAME: &'static str = "debug.set_owner";
    const DEBUG_ONLY: bool = true;
}

/// `"debug.invulnerable"`: turn invulnerability on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugInvulnerableCmd {
    pub on: bool,
}

impl CommandPayload for DebugInvulnerableCmd {
    const NAME: &'static str = "debug.invulnerable";
    const DEBUG_ONLY: bool = true;
}

/// `"debug.set_hp"`: set hit points to `hp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugSetHpCmd {
    pub hp: u32,
}

impl CommandPayload for DebugSetHpCmd {
    const NAME: &'static str = "debug.set_hp";
    const DEBUG_ONLY: bool = true;
}

/// `"debug.teleport"`: move the units to `pos` instantly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugTeleportCmd {
    pub pos: FxVec2,
}

impl CommandPayload for DebugTeleportCmd {
    const NAME: &'static str = "debug.teleport";
    const DEBUG_ONLY: bool = true;
}

/// `"debug.set_terrain"`: the cells `min_cell..=max_cell` (`(x, y)`, both corners inclusive)
/// become the terrain named `terrain` (a content name such as `"base.water"`, never a dense
/// id). Needs no units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebugSetTerrainCmd {
    pub min_cell: (u32, u32),
    pub max_cell: (u32, u32),
    pub terrain: String,
}

impl CommandPayload for DebugSetTerrainCmd {
    const NAME: &'static str = "debug.set_terrain";
    const NEEDS_UNITS: bool = false;
    const DEBUG_ONLY: bool = true;
}
