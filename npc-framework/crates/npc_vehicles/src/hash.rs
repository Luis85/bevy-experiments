//! State-hash encodings and their registration.

use bevy_app::App;
use npc_kernel::hash::{Fnv64, SimHash, SimHashRegistry, SimHashResource};
use npc_kernel::registry::KernelAppExt;
use npc_locomotion::register_locomotion_hash;

use crate::VehicleConfig;
use crate::drive::{DriveGoal, Driving};
use crate::vehicle::{MountedIn, Occupants, SeatPreference, SeatSlot, Vehicle, VehicleControl};

fn len(n: usize) -> u32 {
    u32::try_from(n).expect("too many seats to hash")
}

impl SimHash for Vehicle {
    const NAME: &'static str = "vehicle";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.seats.len()));
        for s in &self.seats {
            h.u8(s.role.code());
            h.u8(u8::from(s.required));
            s.offset.sim_hash(h);
        }
        h.i32(self.speed_mm_per_tick);
        h.u8(match self.control {
            VehicleControl::ByCrew => 0,
            VehicleControl::Direct => 1,
        });
    }
}

impl SimHash for Occupants {
    const NAME: &'static str = "occupants";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.seats.len()));
        for s in &self.seats {
            match *s {
                SeatSlot::Free => h.u8(0),
                SeatSlot::Claimed(id) => {
                    h.u8(1);
                    h.u32(id.0);
                }
                SeatSlot::Taken(id) => {
                    h.u8(2);
                    h.u32(id.0);
                }
            }
        }
    }
}

impl SimHash for MountedIn {
    const NAME: &'static str = "mounted_in";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.vehicle.0);
        h.u8(self.seat);
    }
}

impl SimHash for SeatPreference {
    const NAME: &'static str = "seat_preference";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(self.0.code());
    }
}

impl SimHash for DriveGoal {
    const NAME: &'static str = "drive_goal";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.0.sim_hash(h);
    }
}

impl SimHash for Driving {
    const NAME: &'static str = "driving";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.vehicle.0);
        self.goal.sim_hash(h);
    }
}

impl SimHashResource for VehicleConfig {
    const NAME: &'static str = "vehicle_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.mount_reach_mm);
        h.u32(self.mount_ticks);
        h.u32(self.dismount_ticks);
        h.u8(self.dismount_radius_cells);
        h.u8(self.goal_slot);
        h.i32(self.drive_tolerance_mm);
    }
}

/// Adds the vehicle state to the state hash (idempotent): the components
/// above, `vehicle_config`, and the locomotion state vehicles move by
/// (`move_intent`, `move_status`). Changes the content hash;
/// `VehiclesPlugin` calls it, so an app without the plugin hashes exactly
/// as before.
pub fn register_vehicle_hash(app: &mut App) {
    register_locomotion_hash(app);
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    if registry.components.iter().any(|c| c.name == Vehicle::NAME) {
        return;
    }
    app.register_sim_component::<Vehicle>()
        .register_sim_component::<Occupants>()
        .register_sim_component::<MountedIn>()
        .register_sim_component::<SeatPreference>()
        .register_sim_component::<DriveGoal>()
        .register_sim_component::<Driving>()
        .register_sim_resource::<VehicleConfig>();
}
