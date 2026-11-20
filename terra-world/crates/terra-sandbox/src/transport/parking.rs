//! Bounded delivery bays for purchased and rented vehicles. Placement is
//! prepared before any identifiers, funds, bond or station fuel can change.
use super::*;

const PARKING_CLEARANCE_M: f64 = 1.1;
const PARKING_SAMPLE_STEP_M: f64 = 0.5;

impl SandboxState {
    /// Deliver a purchased/rented vehicle onto clear resident garage ground.
    /// The callback supplies model-height-aware vehicle support at each point;
    /// it must reject unavailable, wet, too-steep or source-obstructed ground.
    /// Only BuyVehicle and RentVehicle are accepted. All ordinary licensing,
    /// funds, fuel, capacity and atomic transaction rules remain in force.
    pub fn deliver_vehicle_with_ground<F>(
        &mut self,
        player: u64,
        action: TransportCommand,
        catalog: &Catalog,
        ground: F,
    ) -> Result<Outcome, SandboxError>
    where
        F: Fn(GeoPosition, f64) -> Option<GeoPosition>,
    {
        let (model, facility_id) = match &action {
            TransportCommand::BuyVehicle { model, facility_id }
            | TransportCommand::RentVehicle { model, facility_id } => (*model, *facility_id),
            _ => return Err(invalid("garage delivery requires a purchase or rental")),
        };
        let position = self.garage_vehicle_position(player, model, facility_id, &ground)?;
        self.apply_transport_transaction(player, action, catalog, Some(position))
    }

    pub(super) fn garage_vehicle_position<F>(
        &self,
        player: u64,
        model: VehicleModel,
        facility_id: u64,
        ground: &F,
    ) -> Result<GeoPosition, SandboxError>
    where
        F: Fn(GeoPosition, f64) -> Option<GeoPosition>,
    {
        if self.is_player_dormant(player) {
            return Err(invalid(
                "resume the owner's session before issuing commands",
            ));
        }
        if !self.players.contains_key(&player) {
            return Err(SandboxError::PlayerMissing);
        }
        if !self.transport.enabled || !self.living.accounts.contains_key(&player) {
            return Err(invalid(
                "transport services are not initialized for this driver",
            ));
        }
        self.require_garage(player, facility_id)?;
        self.require_license(player, model.spec().required_license)?;
        // Global capacity is known even when bodies are cold. Preserve that
        // admission result before asking a host to hydrate for geometry.
        if self.living_unit_count() >= MAX_LIVING_UNITS
            || self.transport.vehicles.len() >= MAX_TRANSPORT_VEHICLES
            || self
                .transport
                .vehicles
                .values()
                .filter(|v| v.owner == player)
                .count()
                >= MAX_PLAYER_VEHICLES
        {
            return Err(SandboxError::Capacity);
        }
        if self.is_partial() {
            return Err(invalid(
                "hydrate the complete regional world before garage delivery",
            ));
        }
        let origin = self.transport.facilities[&facility_id].position;
        let frame = LocalFrame::new(origin);
        let [width, height, length] = model.dimensions_m();
        // A clear door-side walking strip is part of a usable new parking bay.
        // It also avoids stacking newly supplied models onto the starter rover.
        let dimensions = [
            width + 2. * PARKING_CLEARANCE_M,
            height,
            length + 2. * PARKING_CLEARANCE_M,
        ];
        let footprints = self.living.facility_footprints();
        let sample = |point: GeoPosition| {
            let supported = ground(point, height)?;
            (supported.validate().is_ok()
                && point.surface_distance_m(supported) <= 0.05
                && (-12_000.0..=99_997.0).contains(&supported.altitude_m))
            .then_some(supported)
        };
        for radius in [0., 4., 8., 12., 16.] {
            for direction in 0..if radius == 0. { 1 } else { 8 } {
                let angle = direction as f64 * std::f64::consts::TAU / 8.;
                let mut point = frame.to_geo([radius * angle.cos(), 0., radius * angle.sin()])?;
                point.altitude_m = origin.altitude_m;
                let Some(position) = sample(point) else {
                    continue;
                };
                if position
                    .surface_distance_m(origin)
                    .hypot(position.altitude_m - origin.altitude_m)
                    > INTERACTION_REACH_M
                {
                    continue;
                }
                let body = terra_core::ContactBody {
                    id: self.living.next_id,
                    position,
                    heading_deg: 0.,
                    dimensions_m: dimensions,
                };
                if self.spawn_shape_obstruction(body)?.is_some()
                    || footprints.iter().any(|(p, r)| {
                        terra_core::body_overlaps_column(body, *p, *r, 8.).unwrap_or(true)
                    })
                    || self.structures.values().any(|structure| {
                        terra_core::body_overlaps_column(
                            body,
                            structure.position,
                            structure.radius_m,
                            if structure.housing { 8. } else { 2.5 },
                        )
                        .unwrap_or(true)
                    })
                {
                    continue;
                }
                let local = LocalFrame::new(position);
                let columns = (dimensions[0] / PARKING_SAMPLE_STEP_M).ceil() as usize;
                let rows = (dimensions[2] / PARKING_SAMPLE_STEP_M).ceil() as usize;
                let mut supported = true;
                'footprint: for column in 0..=columns {
                    for row in 0..=rows {
                        let mut p = local.to_geo([
                            dimensions[0] * (column as f64 / columns as f64 - 0.5),
                            0.,
                            dimensions[2] * (row as f64 / rows as f64 - 0.5),
                        ])?;
                        p.altitude_m = position.altitude_m;
                        if sample(p).is_none_or(|ground| {
                            (ground.altitude_m - position.altitude_m).abs() > 1.
                        }) {
                            supported = false;
                            break 'footprint;
                        }
                    }
                }
                if supported {
                    return Ok(position);
                }
            }
        }
        Err(invalid(
            "no supported, unoccupied garage parking bay; clear nearby space or wait for terrain to load",
        ))
    }
}
