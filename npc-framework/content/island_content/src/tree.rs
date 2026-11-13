//! The testbed unit trees, one per [`Behaviour`](crate::world::Behaviour).
//! They share the same top: `npc_agent_io` injects its `ExternalControl`
//! guard above everything at freeze time; then
//!
//! ```text
//! Root (selector)
//! ├─ guard HasDriveGoal (LowerPriority) → DriveVehicle        npc_vehicles
//! ├─ guard has_unit_order (LowerPriority) → ExecuteOrder      npc_command
//! ├─ guard Mounted (LowerPriority) → TbIdle 20                (ride along)
//! ├─ [worker] guard tb_has_tool_work (LowerPriority)
//! │    → sequence TbFindToolWork, TbWalk 1000, Interact "use_tool_on", TbIdle 10
//! ├─ [wander, worker] sequence TbPickWander, TbWalk 300, TbIdle 60
//! └─ TbIdle 40
//! ```
//!
//! Every `LowerPriority` guard implies its child can start: `DriveVehicle`
//! starts whenever the unit drives and has a goal, `ExecuteOrder` whenever
//! an order is held, `TbIdle` always, `TbFindToolWork` exactly when
//! `tb_has_tool_work` holds.

use bevy_app::App;
use npc_framework::prelude::*;
use npc_framework::vehicles::{COND_HAS_DRIVE_GOAL, COND_MOUNTED, DRIVE_VEHICLE};

use crate::actions::{COND_HAS_TOOL_WORK, TB_FIND_TOOL_WORK, TB_IDLE, TB_PICK_WANDER, TB_WALK};

pub const TREE_IDLE: &str = "testbed_idle";
pub const TREE_WANDER: &str = "testbed_wander";
pub const TREE_WORKER: &str = "testbed_worker";

fn unit_tree(b: &mut TreeBuilder, wander: bool, worker: bool) {
    b.selector("Root", |b| {
        b.guard("Drive", COND_HAS_DRIVE_GOAL, Abort::LowerPriority, |b| {
            b.leaf("DriveVehicle", DRIVE_VEHICLE, ());
        });
        b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
            b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
        });
        b.guard("Mounted", COND_MOUNTED, Abort::LowerPriority, |b| {
            b.leaf("Ride", TB_IDLE, 20);
        });
        if worker {
            b.guard("ToolWork", COND_HAS_TOOL_WORK, Abort::LowerPriority, |b| {
                b.sequence("Work", |b| {
                    b.leaf("FindToolWork", TB_FIND_TOOL_WORK, ());
                    b.leaf("WalkToWork", TB_WALK, 1000);
                    b.leaf("UseTool", "Interact", "use_tool_on");
                    b.leaf("Rest", TB_IDLE, 10);
                });
            });
        }
        if wander {
            b.sequence("Wander", |b| {
                b.leaf("PickSpot", TB_PICK_WANDER, ());
                b.leaf("WalkToSpot", TB_WALK, 300);
                b.leaf("Linger", TB_IDLE, 60);
            });
        }
        b.leaf("Idle", TB_IDLE, 40);
    });
}

pub fn register_trees(app: &mut App) {
    app.register_tree(TREE_IDLE, |b| unit_tree(b, false, false))
        .register_tree(TREE_WANDER, |b| unit_tree(b, true, false))
        .register_tree(TREE_WORKER, |b| unit_tree(b, true, true));
}
