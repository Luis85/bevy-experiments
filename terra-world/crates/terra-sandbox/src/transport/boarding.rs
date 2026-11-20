//! Authority-owned, bounded dismount placement. Clients request ExitVehicle;
//! they never submit an exit coordinate or choose a terrain fallback.
use super::*;

const AVATAR_DIMENSIONS_M: [f64; 3] = [0.55, 1.8, 0.55];
const EXIT_SAMPLE_STEP_M: f64 = 0.25;

impl SandboxState {
    /// Exit beside a stopped owned vehicle using resident authoritative ground.
    ///
    /// `ground` must reject uncovered, wet, too-steep or source-obstructed points
    /// and return the supported height without moving the horizontal location.
    /// The planner checks all nine avatar-footprint points along a bounded
    /// corridor, plus authored structures/facilities and complete living-body
    /// occupancy. No state changes unless the complete transaction succeeds.
    ///
    /// Hosts hydrate their regional command dependencies before calling. Plain
    /// `apply(ExitVehicle)` uses the same body/footprint planner on the SDK's
    /// existing level-ground assumption; terrain-backed hosts use this method.
    pub fn exit_vehicle_with_ground<F>(
        &mut self,
        player: u64,
        catalog: &Catalog,
        ground: F,
    ) -> Result<Outcome, SandboxError>
    where
        F: Fn(GeoPosition) -> Option<GeoPosition>,
    {
        let position = self.vehicle_exit_position(player, &ground)?;
        self.apply_transport_transaction(
            player,
            TransportCommand::ExitVehicle,
            catalog,
            Some(position),
        )
    }

    pub(super) fn vehicle_exit_position<F>(
        &self,
        player: u64,
        ground: &F,
    ) -> Result<GeoPosition, SandboxError>
    where
        F: Fn(GeoPosition) -> Option<GeoPosition>,
    {
        if self.is_player_dormant(player) {
            return Err(invalid(
                "resume the owner's session before issuing commands",
            ));
        }
        if !self.players.contains_key(&player) {
            return Err(SandboxError::PlayerMissing);
        }
        if self.is_partial() {
            return Err(invalid(
                "hydrate the complete regional world before planning a vehicle exit",
            ));
        }
        if !self.transport.enabled {
            return Err(invalid(
                "transport services are not initialized for this driver",
            ));
        }
        let account = self
            .living
            .accounts
            .get(&player)
            .ok_or_else(|| invalid("driver account"))?;
        let vehicle = self.owned_vehicle(player, account.active_unit)?;
        let body = self
            .living
            .units
            .get(&account.active_unit)
            .ok_or_else(|| invalid("vehicle body"))?;
        let avatar = self
            .living
            .units
            .get(&account.avatar_unit)
            .ok_or_else(|| invalid("driver avatar"))?;
        if body.velocity_enu_m_s[0].hypot(body.velocity_enu_m_s[1]) > 0.5
            || vehicle
                .dynamics
                .speed_m_s
                .hypot(vehicle.dynamics.lateral_speed_m_s)
                > 0.5
        {
            return Err(invalid("stop the vehicle before exiting"));
        }
        let origin = body.position;
        let heading = f64::from(body.heading_deg).to_radians();
        let start = exit_support(origin, heading, ground)
            .filter(|p| (p.altitude_m - origin.altitude_m).abs() <= 1.)
            .ok_or_else(|| invalid("wait for supported ground before exiting the vehicle"))?;
        let frame = LocalFrame::new(origin);
        let (sin, cos) = heading.sin_cos();
        let [width, _, length] = vehicle.model.dimensions_m();
        let radius = AVATAR_DIMENSIONS_M[0].hypot(AVATAR_DIMENSIONS_M[2]) * 0.5;
        let side = width * 0.5 + radius + 0.25;
        let end = length * 0.5 + radius + 0.25;
        let seats = length * 0.25;
        // Near-side doors first, then the opposite side and bounded end/corner
        // alternatives. No broad teleport search or client-selected coordinate.
        let candidates = [
            (-side, seats),
            (side, seats),
            (-side, 0.),
            (side, 0.),
            (-side, -seats),
            (side, -seats),
            (0., -end),
            (0., end),
            (-side, -end),
            (side, -end),
            (-side, end),
            (side, end),
        ];
        let obstacles = self.living.facility_footprints();
        for (right, forward) in candidates {
            let delta = [right * cos + forward * sin, 0., right * sin - forward * cos];
            let distance = right.hypot(forward);
            if distance > INTERACTION_REACH_M {
                continue;
            }
            let mut proposed = frame.to_geo(delta)?;
            proposed.altitude_m = origin.altitude_m;
            let Some(destination) = exit_support(proposed, heading, ground) else {
                continue;
            };
            let mut placed = avatar.clone();
            placed.position = destination;
            placed.heading_deg = body.heading_deg;
            if self.spawn_obstruction(&placed)?.is_some() {
                continue;
            }
            let steps = (distance / EXIT_SAMPLE_STEP_M).ceil().max(1.) as usize;
            let mut previous = start;
            let mut admitted = true;
            for step in 0..=steps {
                let mut sample = frame.to_geo(delta.map(|v| v * step as f64 / steps as f64))?;
                sample.altitude_m = origin.altitude_m;
                let Some(supported) = exit_support(sample, heading, ground) else {
                    admitted = false;
                    break;
                };
                if (supported.altitude_m - previous.altitude_m).abs() > 0.4
                    || self.dismount_path_obstruction(&placed, previous, supported, body.id)?
                    || obstacles.iter().any(|(position, r)| {
                        (position.altitude_m - supported.altitude_m).abs() < 3.
                            && position.surface_distance_m(supported) < r + radius
                    })
                    || self.structures.values().any(|structure| {
                        let height = if structure.housing { 8. } else { 2.5 };
                        supported.altitude_m + AVATAR_DIMENSIONS_M[1]
                            >= structure.position.altitude_m
                            && supported.altitude_m <= structure.position.altitude_m + height
                            && structure.position.surface_distance_m(supported)
                                < structure.radius_m + radius
                    })
                {
                    admitted = false;
                    break;
                }
                previous = supported;
            }
            if admitted {
                return Ok(destination);
            }
        }
        Err(invalid(
            "no supported, unoccupied exit beside this vehicle; park in a clearer place or wait for terrain to load",
        ))
    }
}

fn exit_support<F>(position: GeoPosition, heading: f64, ground: &F) -> Option<GeoPosition>
where
    F: Fn(GeoPosition) -> Option<GeoPosition>,
{
    let center = checked_exit_sample(position, ground)?;
    let half = AVATAR_DIMENSIONS_M[0] * 0.5;
    let frame = LocalFrame::new(position);
    let (sin, cos) = heading.sin_cos();
    for (right, forward) in [
        (-half, -half),
        (half, -half),
        (-half, half),
        (half, half),
        (0., -half),
        (0., half),
        (-half, 0.),
        (half, 0.),
    ] {
        let mut point = frame
            .to_geo([right * cos + forward * sin, 0., right * sin - forward * cos])
            .ok()?;
        point.altitude_m = position.altitude_m;
        let sample = checked_exit_sample(point, ground)?;
        if (sample.altitude_m - center.altitude_m).abs() > 0.5 {
            return None;
        }
    }
    Some(center)
}

fn checked_exit_sample<F>(position: GeoPosition, ground: &F) -> Option<GeoPosition>
where
    F: Fn(GeoPosition) -> Option<GeoPosition>,
{
    let resolved = ground(position)?;
    (resolved.validate().is_ok()
        && position.surface_distance_m(resolved) <= 0.05
        && (-12_000.0..=99_997.0).contains(&resolved.altitude_m))
    .then_some(resolved)
}
