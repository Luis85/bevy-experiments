//! Content commands: `Mount`, `Dismount` and `Unload`.
//!
//! - `{"type":"Mount","unit":5,"vehicle":9,"role":"Driver"}` (`role`
//!   optional: without it the unit's `SeatPreference`, else the first free
//!   seat):
//!   claims the seat, then runs the `mount` verb (`Source::Command`).
//! - `{"type":"Dismount","unit":5}`: the `dismount` verb on the unit's
//!   vehicle.
//! - `{"type":"Unload","vehicle":9}`: every occupant gets off now, each
//!   onto its own free cell ([`unload`](crate::unload)).

use bevy_ecs::prelude::*;
use npc_interaction::builtin::{DISMOUNT, MOUNT};
use npc_interaction::run::busy_with;
use npc_interaction::{Interactor, Source, begin, can_interact};
use npc_kernel::commands::{CommandError, entity, entity_with};
use npc_kernel::{ContentCommand, SimId};
use serde::{Deserialize, Serialize};

use crate::seats::{choose_seat, default_seat, mounted_in, unload};
use crate::vehicle::{Occupants, SeatRole, Vehicle};

fn unavailable(reason: impl Into<String>) -> CommandError {
    CommandError::Unavailable {
        reason: reason.into(),
    }
}

/// `unit` mounts `vehicle` (in a seat of `role`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Mount {
    pub unit: u32,
    pub vehicle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<SeatRole>,
}

impl Mount {
    pub fn new(unit: SimId, vehicle: SimId) -> Self {
        Self {
            unit: unit.0,
            vehicle: vehicle.0,
            role: None,
        }
    }

    pub fn as_role(mut self, role: SeatRole) -> Self {
        self.role = Some(role);
        self
    }

    /// The seat this command takes: the first free seat of `role`
    /// (strictly), or without a role [`default_seat`].
    fn seat(&self, world: &World, actor: Entity, vehicle: Entity) -> Result<u8, String> {
        match self.role {
            Some(role) => choose_seat(world, vehicle, SimId(self.unit), Some(role)),
            None => default_seat(world, vehicle, actor),
        }
    }
}

impl ContentCommand for Mount {
    const NAME: &'static str = "Mount";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let actor = entity_with::<Interactor>(world, self.unit, "interactor")?;
        let vehicle = entity_with::<Vehicle>(world, self.vehicle, "vehicle")?;
        if let Some(b) = busy_with(world, actor) {
            return Err(unavailable(format!("unit {} is busy with {b}", self.unit)));
        }
        self.seat(world, actor, vehicle).map_err(unavailable)?;
        can_interact(world, actor, MOUNT, SimId(self.vehicle), None)
            .map(|_| ())
            .map_err(unavailable)
    }

    fn apply(self, world: &mut World) {
        if let Ok(actor) = entity(world, self.unit) {
            let _ = crate::mount(
                world,
                actor,
                SimId(self.vehicle),
                self.role,
                Source::Command,
            );
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// `unit` gets off its vehicle.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dismount {
    pub unit: u32,
}

impl Dismount {
    pub fn new(unit: SimId) -> Self {
        Self { unit: unit.0 }
    }
}

impl ContentCommand for Dismount {
    const NAME: &'static str = "Dismount";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let actor = entity_with::<Interactor>(world, self.unit, "interactor")?;
        let m = mounted_in(world, actor)
            .ok_or_else(|| unavailable(format!("unit {} is not mounted", self.unit)))?;
        if let Some(b) = busy_with(world, actor) {
            return Err(unavailable(format!("unit {} is busy with {b}", self.unit)));
        }
        can_interact(world, actor, DISMOUNT, m.vehicle, None)
            .map(|_| ())
            .map_err(unavailable)
    }

    fn apply(self, world: &mut World) {
        let Ok(actor) = entity(world, self.unit) else {
            return;
        };
        if let Some(m) = mounted_in(world, actor) {
            let _ = begin(world, actor, DISMOUNT, m.vehicle, None, Source::Command);
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Every occupant of `vehicle` gets off.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Unload {
    pub vehicle: u32,
}

impl Unload {
    pub fn new(vehicle: SimId) -> Self {
        Self { vehicle: vehicle.0 }
    }
}

impl ContentCommand for Unload {
    const NAME: &'static str = "Unload";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let vehicle = entity_with::<Vehicle>(world, self.vehicle, "vehicle")?;
        if world
            .get::<Occupants>(vehicle)
            .is_none_or(Occupants::is_empty)
        {
            return Err(unavailable(format!("vehicle {} is empty", self.vehicle)));
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        if let Ok(vehicle) = entity(world, self.vehicle) {
            let _ = unload(world, vehicle);
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.vehicle))
    }
}
