//! The played unit's tree. `npc_agent_io` injects its `ExternalControl`
//! guard above everything at freeze time; then
//!
//! ```text
//! Root (selector)
//! ├─ guard HasDriveGoal (LowerPriority) → DriveVehicle        npc_vehicles
//! ├─ guard has_unit_order (LowerPriority) → ExecuteOrder      npc_command
//! ├─ guard Mounted (LowerPriority) → IslandIdle 20            (ride along)
//! └─ IslandIdle 40
//! ```
//!
//! Residents run their own data-authored trees (`village::resident`).

use bevy_app::App;
use npc_framework::prelude::*;
use npc_framework::vehicles::{COND_HAS_DRIVE_GOAL, COND_MOUNTED, DRIVE_VEHICLE};

use crate::actions::ISLAND_IDLE;

/// The played unit's tree: drive, orders, riding, else idle.
pub const TREE_IDLE: &str = "island_idle";

fn unit_tree(b: &mut TreeBuilder) {
    b.selector("Root", |b| {
        b.guard("Drive", COND_HAS_DRIVE_GOAL, Abort::LowerPriority, |b| {
            b.leaf("DriveVehicle", DRIVE_VEHICLE, ());
        });
        b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
            b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
        });
        b.guard("Mounted", COND_MOUNTED, Abort::LowerPriority, |b| {
            b.leaf("Ride", ISLAND_IDLE, 20);
        });
        b.leaf("Idle", ISLAND_IDLE, 40);
    });
}

pub fn register_trees(app: &mut App) {
    app.register_tree(TREE_IDLE, unit_tree);
}
