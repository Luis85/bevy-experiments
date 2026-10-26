//! The vehicle and crew components.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

/// What a seat is for.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
pub enum SeatRole {
    /// Steers the vehicle (`ByCrew`: the only way it moves).
    Driver,
    #[default]
    Passenger,
    Gunner,
}

impl SeatRole {
    /// The role's name (`"Driver"`, `"Passenger"`, `"Gunner"`).
    pub fn name(self) -> &'static str {
        match self {
            SeatRole::Driver => "Driver",
            SeatRole::Passenger => "Passenger",
            SeatRole::Gunner => "Gunner",
        }
    }

    /// The role named `name` (case-insensitive).
    pub fn parse(name: &str) -> Option<Self> {
        [SeatRole::Driver, SeatRole::Passenger, SeatRole::Gunner]
            .into_iter()
            .find(|r| r.name().eq_ignore_ascii_case(name))
    }

    pub(crate) fn code(self) -> u8 {
        match self {
            SeatRole::Driver => 0,
            SeatRole::Passenger => 1,
            SeatRole::Gunner => 2,
        }
    }
}

/// One seat: its role, whether the vehicle can move with it empty
/// (`required`), and where its occupant sits relative to the vehicle's
/// `SimPos` (mm, world axes; vehicles have no heading in the sim).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Seat {
    pub role: SeatRole,
    pub required: bool,
    pub offset: SimPos,
}

impl Seat {
    /// A seat of `role`, not required, no offset.
    pub const fn new(role: SeatRole) -> Self {
        Self {
            role,
            required: false,
            offset: SimPos::new(0, 0),
        }
    }

    /// A driver seat (required: a vehicle never moves without its driver).
    pub const fn driver() -> Self {
        Self::new(SeatRole::Driver).required()
    }

    pub const fn passenger() -> Self {
        Self::new(SeatRole::Passenger)
    }

    pub const fn gunner() -> Self {
        Self::new(SeatRole::Gunner)
    }

    /// The vehicle cannot move while this seat is empty.
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// The seat is optional (e.g. a cosmetic driver seat of a `Direct`
    /// vehicle).
    pub const fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// The occupant sits at the vehicle's position plus `(x, z)` mm.
    pub const fn offset(mut self, x: i32, z: i32) -> Self {
        self.offset = SimPos::new(x, z);
        self
    }
}

/// Who moves the vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum VehicleControl {
    /// Only its driver moves it: orders go to the driver, whose
    /// `DriveVehicle` action writes the vehicle's `MoveIntent`. Without a
    /// driver aboard it cannot move.
    #[default]
    ByCrew,
    /// The vehicle is a unit of its own (its own `MoveIntent`, `NavAgent`,
    /// orders, brain); the crew is capacity or cosmetic.
    Direct,
}

/// A vehicle: a sim entity with a `SimPos` and seats. Moves through the
/// locomotion seam like any agent (`MoveIntent`, optionally `NavAgent`).
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[require(Occupants)]
#[reflect(Component)]
pub struct Vehicle {
    pub seats: Vec<Seat>,
    /// The speed its driver (or [`command_vehicle_to`](crate::command_vehicle_to))
    /// drives it at.
    pub speed_mm_per_tick: i32,
    pub control: VehicleControl,
}

impl Vehicle {
    /// A `ByCrew` vehicle without seats (add them with [`Vehicle::seat`]).
    pub fn by_crew(speed_mm_per_tick: i32) -> Self {
        Self {
            seats: Vec::new(),
            speed_mm_per_tick,
            control: VehicleControl::ByCrew,
        }
    }

    /// A `Direct` vehicle without seats.
    pub fn direct(speed_mm_per_tick: i32) -> Self {
        Self {
            seats: Vec::new(),
            speed_mm_per_tick,
            control: VehicleControl::Direct,
        }
    }

    pub fn seat(mut self, seat: Seat) -> Self {
        assert!(self.seats.len() < usize::from(u8::MAX), "too many seats");
        self.seats.push(seat);
        self
    }

    /// `n` seats like `seat`.
    pub fn seats(mut self, n: usize, seat: Seat) -> Self {
        for _ in 0..n {
            self = self.seat(seat);
        }
        self
    }
}

/// The state of one seat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum SeatSlot {
    #[default]
    Free,
    /// Someone is mounting into it (held for the mount's duration).
    Claimed(SimId),
    /// Someone sits in it.
    Taken(SimId),
}

/// The crew view of a vehicle, one slot per seat (index = seat index).
/// Kept in step with [`MountedIn`] by this crate only; read it, never
/// write it.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Occupants {
    pub seats: Vec<SeatSlot>,
}

impl Occupants {
    /// The slot of `seat` (`Free` beyond the stored length).
    pub fn slot(&self, seat: u8) -> SeatSlot {
        self.seats
            .get(usize::from(seat))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn set(&mut self, seat: u8, slot: SeatSlot) {
        let i = usize::from(seat);
        if self.seats.len() <= i {
            self.seats.resize(i + 1, SeatSlot::Free);
        }
        self.seats[i] = slot;
    }

    /// Who sits aboard, ascending `SimId`.
    pub fn units(&self) -> Vec<SimId> {
        let mut v: Vec<SimId> = self
            .seats
            .iter()
            .filter_map(|s| match s {
                SeatSlot::Taken(id) => Some(*id),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v
    }

    /// `(seat, unit)` of everyone aboard, in seat order.
    pub fn seated(&self) -> Vec<(u8, SimId)> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                SeatSlot::Taken(id) => Some((u8::try_from(i).unwrap_or(u8::MAX), *id)),
                _ => None,
            })
            .collect()
    }

    /// The seat `unit` sits in.
    pub fn seat_of(&self, unit: SimId) -> Option<u8> {
        self.seated()
            .into_iter()
            .find_map(|(s, id)| (id == unit).then_some(s))
    }

    /// The seat `unit` holds a claim on.
    pub fn claim_of(&self, unit: SimId) -> Option<u8> {
        self.seats.iter().enumerate().find_map(|(i, s)| {
            (*s == SeatSlot::Claimed(unit)).then(|| u8::try_from(i).unwrap_or(u8::MAX))
        })
    }

    pub fn is_empty(&self) -> bool {
        !self.seats.iter().any(|s| matches!(s, SeatSlot::Taken(_)))
    }
}

/// The relationship "this actor sits in `vehicle`, seat `seat`" (by
/// `SimId`, so it hashes and orders deterministically; the vehicle's
/// [`Occupants`] is the other side). While it is present the actor's
/// `SimPos` follows the vehicle and its own locomotion is suspended.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct MountedIn {
    pub vehicle: SimId,
    pub seat: u8,
}

/// Optional on an actor: the seat role it prefers when it mounts without
/// naming one (the `Interact "mount"` leaf or command): the first free seat
/// of that role, else the first free seat.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct SeatPreference(pub SeatRole);
