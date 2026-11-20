use super::*;

/// Authored reference loading units. A game with item-specific density can
/// replace this policy at its authority boundary; no scientific mass is implied.
pub const CARGO_UNIT_MASS_KG: f64 = 10.;
pub const PASSENGER_MASS_KG: f64 = 75.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleLoad {
    pub cargo_kg: f64,
    pub passengers_kg: f64,
    pub trailer_kg: f64,
}
impl VehicleLoad {
    pub fn total_kg(self) -> f64 {
        self.cargo_kg + self.passengers_kg + self.trailer_kg
    }
}

impl TransportWorld {
    /// Read actual carried cargo and onboard riders, excluding reservations
    /// still waiting at pickup. A trailer adds its empty mass only: all cargo
    /// already lives in the towing vehicle's single authoritative inventory.
    pub fn vehicle_load(&self, vehicle_id: u64) -> Option<VehicleLoad> {
        let vehicle = self.vehicles.get(&vehicle_id)?;
        let cargo_kg = vehicle
            .cargo
            .values()
            .map(|n| f64::from(*n) * CARGO_UNIT_MASS_KG)
            .sum();
        let private_riders = self
            .jobs
            .values()
            .filter_map(|job| job.assignment.as_ref())
            .filter(|a| a.vehicle_id == vehicle_id)
            .map(|a| f64::from(a.passengers_onboard))
            .sum::<f64>();
        let fleet_riders = self
            .companies
            .values()
            .flat_map(|c| &c.drivers)
            .filter(|d| d.vehicle_id == Some(vehicle_id))
            .filter_map(|d| d.trip.as_ref())
            .map(|trip| match trip.service {
                FleetService::Passengers { count } => f64::from(count),
                FleetService::Freight { .. }
                | FleetService::DepotFreight { .. }
                | FleetService::DepotTransfer { .. } => 0.,
            })
            .sum::<f64>();
        let trailer_kg = if let Some(id) = vehicle.trailer {
            let trailer = self.trailers.get(&id)?;
            if trailer.attached_to != Some(vehicle_id) || trailer.owner != vehicle.owner {
                return None;
            }
            // One reference cargo-trailer family, sized by its declared capacity.
            600. + f64::from(trailer.capacity) * 4.
        } else {
            0.
        };
        Some(VehicleLoad {
            cargo_kg,
            passengers_kg: (private_riders + fleet_riders) * PASSENGER_MASS_KG,
            trailer_kg,
        })
    }

    /// The effective configuration used by local authority, host and its
    /// private prediction snapshot. Base VehicleState config already adds cargo.
    pub fn vehicle_dynamics_config(
        &self,
        vehicle_id: u64,
    ) -> Option<terra_core::VehicleDynamicsConfig> {
        let vehicle = self.vehicles.get(&vehicle_id)?;
        let load = self.vehicle_load(vehicle_id)?;
        let mut config = vehicle.dynamics_config();
        config.mass_kg += load.passengers_kg + load.trailer_kg;
        config.validate().ok()?;
        Some(config)
    }
}

impl LivingWorld {
    /// The rendered authored settlement roads are gameplay source geometry too.
    /// Override material only after a real ground sample has established
    /// coverage; this function never invents height or traversable water.
    pub fn apply_authored_road_surface(
        &self,
        position: GeoPosition,
        sample: terra_core::VehicleGroundSample,
    ) -> terra_core::VehicleGroundSample {
        apply_authored_road_surface(&self.roads, position, sample)
    }
}

/// Shared native/host/manual/autonomous policy, applied only after support exists.
pub fn apply_authored_road_surface(
    roads: &[RoadRoute],
    position: GeoPosition,
    mut sample: terra_core::VehicleGroundSample,
) -> terra_core::VehicleGroundSample {
    let frame = LocalFrame::new(position);
    if roads
        .iter()
        .filter(|road| road.id.starts_with("authored:"))
        .any(|road| {
            road.points.windows(2).any(|pair| {
                let a = frame.to_local(pair[0]);
                let b = frame.to_local(pair[1]);
                let dx = b[0] - a[0];
                let dz = b[2] - a[2];
                let length = dx * dx + dz * dz;
                let t = if length > 1e-12 {
                    (-(a[0] * dx + a[2] * dz) / length).clamp(0., 1.)
                } else {
                    0.
                };
                (a[0] + t * dx).hypot(a[2] + t * dz) <= f64::from(road.width_m) * 0.5
            })
        })
    {
        sample.surface = terra_core::VehicleSurface::Paved;
    }
    sample
}
