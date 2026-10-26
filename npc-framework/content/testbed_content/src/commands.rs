//! Testbed content commands.
//!
//! `TestbedDrive` (`{"type":"TestbedDrive","unit":12,"pos":[x,z]}`): the
//! unit, seated as the driver of a `ByCrew` vehicle, drives it to `pos`
//! (`npc_vehicles::set_drive_goal`; its tree's `HasDriveGoal` branch runs
//! `DriveVehicle`). A stand-in until RTS orders to vehicles are forwarded to
//! their driver by the command × vehicles glue (wave F4), which makes a plain
//! `IssueOrder { Move }` to the cart do the same.

use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::{CommandError, check_pos, entity};
use npc_framework::prelude::*;
use npc_framework::vehicles::drive::cond_mounted_as_driver;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TestbedDrive {
    pub unit: u32,
    pub pos: (i32, i32),
}

impl ContentCommand for TestbedDrive {
    const NAME: &'static str = "TestbedDrive";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let e = entity(world, self.unit)?;
        check_pos("pos", self.pos)?;
        if !cond_mounted_as_driver(world, e) {
            return Err(CommandError::Unavailable {
                reason: format!("unit {} is not driving a vehicle", self.unit),
            });
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        let Ok(e) = entity(world, self.unit) else {
            return;
        };
        if cond_mounted_as_driver(world, e) {
            set_drive_goal(world, e, SimPos::new(self.pos.0, self.pos.1));
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}
