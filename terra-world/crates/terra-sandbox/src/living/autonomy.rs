//! Autonomous drivers produce pedal/steering intent for the same bounded
//! bicycle model as possession. Navigation never writes a target pose.
use super::contacts::MAX_AVOIDANCE_CLUSTER_BODIES;
use super::traffic::TrafficFrame;
use super::*;
use terra_core::{DriveInput, VehicleDynamicsConfig, VehicleGroundSample, VehicleKinematics};

const MAX_ADDITIONAL_CORRIDORS_PER_UPDATE: usize = 12;
const MAX_LOCAL_CORNER_DISTANCE_M: f64 = 64.;
const MAX_LOCAL_ROUTE_M: f64 = 160.;
const MAX_RECONNECTION_SWEEP_M: f64 = 64.;

/// Read-only terrain integration for autonomous road vehicles. The support
/// callback must fail on unavailable terrain; the envelope callback admits the
/// whole model-sized swept path, including rotation, before contacts run.
pub struct AutonomousVehicleEnvironment<'a> {
    pub sample: &'a dyn Fn(GeoPosition) -> Option<VehicleGroundSample>,
    pub resolve: &'a dyn Fn(&ActorState, &ActorState, [f64; 3]) -> Option<GeoPosition>,
    pub weather_grip: f64,
}

/// Compatibility envelope for custom kind/position resolvers. It samples the
/// actual body corners every 25 cm of translation plus rotational travel.
pub(super) fn resolve_compatibility_envelope(
    from: &ActorState,
    to: &ActorState,
    dimensions: [f64; 3],
    resolve: &impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
) -> Option<GeoPosition> {
    let angle = angle_delta(f64::from(to.heading_deg), f64::from(from.heading_deg));
    let intervals = ((from.position.surface_distance_m(to.position)
        + dimensions[0].hypot(dimensions[2]) * 0.5 * angle.abs())
        / 0.25)
        .ceil() as usize;
    if intervals > 512 {
        return None;
    }
    let count = intervals.max(1);
    let frame = LocalFrame::new(from.position);
    let delta = frame.to_local(to.position);
    let mut previous =
        terra_core::vehicle_support_points(from.position, f64::from(from.heading_deg), dimensions)
            .ok()?;
    let mut center = from.position;
    for n in 1..=count {
        let fraction = n as f64 / count as f64;
        let p = frame.to_geo(delta.map(|v| v * fraction)).ok()?;
        let points = terra_core::vehicle_support_points(
            p,
            f64::from(from.heading_deg) + angle.to_degrees() * fraction,
            dimensions,
        )
        .ok()?;
        for (&old, &new) in previous.iter().zip(&points) {
            let supported = resolve(ActorKind::Vehicle, old, new)?;
            supported.validate().ok()?;
        }
        center = resolve(ActorKind::Vehicle, center, p)?;
        center.validate().ok()?;
        previous = points;
    }
    Some(center)
}

pub(super) struct AutonomousDriver<'a> {
    pub config: VehicleDynamicsConfig,
    pub dimensions: [f64; 3],
    pub cap: f64,
    pub traffic: &'a TrafficFrame,
    pub environment: &'a AutonomousVehicleEnvironment<'a>,
    pub roads: &'a [RoadRoute],
    pub queue_limit: f64,
}

/// Returns accepted travelled metres, including each curved substep, for fuel
/// and delivery accounting. At most fifty 100 ms physical steps are admitted.
pub(super) fn advance_vehicle(
    unit: &mut LivingUnit,
    dynamics: &mut VehicleKinematics,
    driver: &AutonomousDriver<'_>,
    dt: f64,
    mut contacts: Option<&mut LivingContacts>,
) -> Result<f64, SandboxError> {
    if dt == 0. {
        return Ok(0.);
    }
    if driver.cap <= 0. || !matches!(unit.order, UnitOrder::Move { .. } | UnitOrder::Patrol) {
        dynamics.stop();
        unit.velocity_enu_m_s = [0.; 3];
        return Ok(0.);
    }
    let count = (dt / 0.1).ceil().max(1.) as usize;
    let step = dt / count as f64;
    let mut travelled = 0.;
    // Additional reconnection and local search work is shared by every physical substep and
    // retry in this update, including the public five-second maximum timestep.
    let mut detour_budget = MAX_ADDITIONAL_CORRIDORS_PER_UPDATE;
    let queue_frame = LocalFrame::new(unit.position);
    let queue_heading = f64::from(unit.heading_deg).to_radians();
    let queue_progress = |position| {
        let delta = queue_frame.to_local(position);
        delta[0] * queue_heading.sin() - delta[2] * queue_heading.cos()
    };
    for _ in 0..count {
        let Some((mut target, mut stop_at_target)) = advance_waypoints(unit, dynamics) else {
            dynamics.stop();
            unit.velocity_enu_m_s = [0.; 3];
            break;
        };
        // Plan around a stationary body before the nose reaches it. Planning
        // only proposes waypoints; every resulting step still has full source
        // and dynamic-body admission below.
        if let Some(contacts) = contacts.as_deref() {
            let delta = LocalFrame::new(unit.position).to_local(target);
            let distance = delta[0].hypot(delta[2]);
            let bearing = delta[0].atan2(-delta[2]).to_degrees();
            if distance > 8. && angle_delta(bearing, f64::from(unit.heading_deg)).abs() < 0.9 {
                let lookahead = distance.min((12. + dynamics.speed_m_s.abs()).min(24.));
                let probe = LocalFrame::new(unit.position).to_geo([
                    delta[0] / distance * lookahead,
                    0.,
                    delta[2] / distance * lookahead,
                ])?;
                if let Some(blocker) = contacts
                    .resolve(
                        unit.id,
                        unit.position,
                        unit.heading_deg,
                        probe,
                        bearing as f32,
                        [0.; 3],
                    )?
                    .contact
                {
                    if insert_detour(
                        unit,
                        target,
                        blocker.other_id,
                        contacts,
                        driver,
                        &mut detour_budget,
                    ) {
                        if let Some(next) = advance_waypoints(unit, dynamics) {
                            target = next.0;
                            stop_at_target = next.1;
                        }
                    }
                }
            }
        }
        let support = terra_core::sample_vehicle_ground(
            unit.position,
            f64::from(unit.heading_deg),
            driver.config.wheelbase_m,
            driver.dimensions[0] * 0.86,
            |p| {
                (driver.environment.sample)(p)
                    .map(|sample| crate::apply_authored_road_surface(driver.roads, p, sample))
            },
        )
        .ok();
        let Some(support) = support else {
            dynamics.stop();
            unit.velocity_enu_m_s = [0.; 3];
            break;
        };
        let road_cap = driver
            .traffic
            .network
            .speed_limit_at(unit.position)
            .unwrap_or(f64::INFINITY);
        let limit = (driver.traffic.limit(unit.id) - travelled).max(0.);
        let grip = support.grip_scale(driver.environment.weather_grip);
        let stopping_deceleration = (driver.config.brake_force_n / driver.config.mass_kg)
            .min(9.80665 * driver.config.grip * grip * 0.70)
            .max(0.15);
        let mut config = driver.config;
        config.max_speed_m_s = config.max_speed_m_s.min(driver.cap.min(road_cap).max(1.));
        config.reverse_speed_m_s = config
            .reverse_speed_m_s
            .min(driver.cap.min(road_cap).max(0.1));
        let mut cap = driver.cap.min(road_cap);
        if limit.is_finite() {
            cap = cap.min((2. * stopping_deceleration * (limit - 0.2).max(0.)).sqrt());
        }
        // Following distance limits progress toward the leader. Reversing
        // away remains admissible, so a stopped queue cannot forbid the
        // clearance needed to steer out of a blocked approach.
        let following_cap = (2.
            * stopping_deceleration
            * (driver.queue_limit - queue_progress(unit.position) - 0.2).max(0.))
        .sqrt();
        let input = route_input(
            unit,
            dynamics,
            &config,
            target,
            stop_at_target,
            cap,
            following_cap,
            stopping_deceleration,
        );
        let from = ActorState {
            id: unit.id,
            kind: unit.kind,
            position: unit.position,
            heading_deg: unit.heading_deg,
            velocity_enu_m_s: unit.velocity_enu_m_s,
        };
        let mut next = *dynamics;
        let candidate = terra_core::drive_actor(
            &from,
            &mut next,
            &config,
            input,
            step,
            grip,
            support.pitch_radians,
        )
        .map_err(|e| SandboxError::Invalid(e.to_string()))?;
        let (_, fraction) = driver
            .traffic
            .clip(unit.id, from.position, candidate.position);
        // Stop lines remain hard admission boundaries even if low grip makes
        // the controller's physical braking distance too long.
        let resolved = (fraction >= 1.
            && from.position.surface_distance_m(candidate.position) <= limit + 1e-6
            && queue_progress(candidate.position) <= driver.queue_limit + 1e-6)
            .then(|| (driver.environment.resolve)(&from, &candidate, driver.dimensions))
            .flatten()
            .filter(|p| p.validate().is_ok() && p.surface_distance_m(candidate.position) < 0.05);
        let Some(position) = resolved else {
            dynamics.stop();
            unit.velocity_enu_m_s = [0.; 3];
            break;
        };
        let mut admitted = candidate;
        admitted.position = position;
        let mut blocked = false;
        if let Some(contacts) = contacts.as_deref_mut() {
            let mut motion = contacts.resolve(
                unit.id,
                from.position,
                from.heading_deg,
                position,
                admitted.heading_deg,
                admitted.velocity_enu_m_s,
            )?;
            let mut clearance_steering = None;
            if let Some(blocker) = motion.contact {
                // Legacy/avatar overlaps and tight parked corners may need a
                // straight clearance step before turning. Retry through exactly
                // the same handling, envelope and contact contracts; never
                // assign a tangent velocity or rotate in place.
                for direction in [1., -1.] {
                    let mut trial_dynamics = *dynamics;
                    let trial_input = DriveInput {
                        throttle: 0.35 * direction,
                        steering: 0.,
                        ..Default::default()
                    };
                    let mut trial = terra_core::drive_actor(
                        &from,
                        &mut trial_dynamics,
                        &config,
                        trial_input,
                        step,
                        grip,
                        support.pitch_radians,
                    )
                    .map_err(|e| SandboxError::Invalid(e.to_string()))?;
                    clearance_steering = Some(trial_dynamics.steering_rad);
                    if !contacts.clears_blocking_corner(
                        unit.id,
                        blocker.other_id,
                        from.position,
                        trial.position,
                    ) || from.position.surface_distance_m(trial.position) > limit + 1e-6
                        || queue_progress(trial.position) > driver.queue_limit + 1e-6
                        || driver
                            .traffic
                            .clip(unit.id, from.position, trial.position)
                            .1
                            < 1.
                    {
                        continue;
                    }
                    let Some(position) =
                        (driver.environment.resolve)(&from, &trial, driver.dimensions).filter(
                            |p| p.validate().is_ok() && p.surface_distance_m(trial.position) < 0.05,
                        )
                    else {
                        continue;
                    };
                    trial.position = position;
                    let alternative = contacts.resolve(
                        unit.id,
                        from.position,
                        from.heading_deg,
                        trial.position,
                        trial.heading_deg,
                        trial.velocity_enu_m_s,
                    )?;
                    if alternative.contact.is_none() {
                        // Prefer a complete static detour when one is
                        // admissible. At a tight stationary corner, its first
                        // rotating sweep can be blocked even though this
                        // straight outward corridor is clear. Retain that
                        // clearance instead of undoing it on the next frame.
                        if contacts.moving_walker(blocker.other_id)
                            || contacts.stopped_routed_vehicle(blocker.other_id)
                            || !insert_detour(
                                unit,
                                target,
                                blocker.other_id,
                                contacts,
                                driver,
                                &mut detour_budget,
                            )
                        {
                            insert_clearance_waypoint(
                                unit, target, &from, &trial, contacts, driver,
                            );
                        }
                        motion = alternative;
                        admitted = trial;
                        next = trial_dynamics;
                        break;
                    }
                }
            }
            admitted.position = motion.position;
            admitted.heading_deg = motion.heading_deg;
            if let Some(contact) = motion.contact {
                blocked = true;
                next.stop();
                // Steering is a bounded actuator even when translation is
                // denied. Keep the valid centering intent so a tight corner
                // cannot retry the same locked steering angle forever.
                if let Some(steering) = clearance_steering {
                    next.steering_rad = steering;
                }
                admitted.velocity_enu_m_s = [0.; 3];
                // A bounded, fully admitted detour is a navigation suggestion,
                // never a pose correction. The next step still steers into it.
                insert_detour(
                    unit,
                    target,
                    contact.other_id,
                    contacts,
                    driver,
                    &mut detour_budget,
                );
            }
            contacts.update(
                unit.id,
                admitted.position,
                admitted.heading_deg,
                admitted.velocity_enu_m_s,
            )?;
        }
        travelled += from.position.surface_distance_m(admitted.position);
        unit.position = admitted.position;
        unit.heading_deg = admitted.heading_deg;
        unit.velocity_enu_m_s = admitted.velocity_enu_m_s;
        *dynamics = next;
        if blocked {
            break;
        }
    }
    Ok(travelled)
}

fn angle_delta(target_degrees: f64, heading_degrees: f64) -> f64 {
    ((target_degrees - heading_degrees + 180.).rem_euclid(360.) - 180.).to_radians()
}

fn advance_waypoints(
    unit: &mut LivingUnit,
    dynamics: &VehicleKinematics,
) -> Option<(GeoPosition, bool)> {
    for _ in 0..130 {
        let (target, final_stop) = match unit.order {
            UnitOrder::Move { destination } => (
                unit.path
                    .get(unit.path_index)
                    .copied()
                    .unwrap_or(destination),
                unit.path_index + 1 >= unit.path.len(),
            ),
            UnitOrder::Patrol => (
                *unit
                    .path
                    .first()
                    .or_else(|| unit.patrol.get(unit.path_index))?,
                false,
            ),
            _ => return None,
        };
        let distance = unit.position.surface_distance_m(target);
        let delta = LocalFrame::new(unit.position).to_local(target);
        let incoming = angle_delta(
            delta[0].atan2(-delta[2]).to_degrees(),
            f64::from(unit.heading_deg),
        );
        // A committed reverse leg must finish before changing direction. The
        // ordinary 3 m corner radius would consume a short rear clearance
        // while still backing, then immediately drive into the same blocker.
        let direction_change = !final_stop
            && reverse_toward(incoming, distance, dynamics.speed_m_s)
            && next_point(unit).is_some_and(|next| {
                let outgoing = LocalFrame::new(target).to_local(next);
                angle_delta(
                    outgoing[0].atan2(-outgoing[2]).to_degrees(),
                    f64::from(unit.heading_deg),
                )
                .abs()
                    < std::f64::consts::FRAC_PI_2
            });
        let must_stop = final_stop || direction_change;
        let threshold = if must_stop { 1.35 } else { 3. };
        if distance >= threshold || (must_stop && dynamics.speed_m_s.abs() > 0.45) {
            return Some((target, must_stop));
        }
        match unit.order {
            UnitOrder::Move { .. } => {
                if final_stop {
                    unit.order = UnitOrder::Idle;
                    unit.path.clear();
                    unit.path_index = 0;
                    return None;
                }
                unit.path_index += 1;
            }
            UnitOrder::Patrol => {
                if !unit.path.is_empty() {
                    unit.path.remove(0);
                    continue;
                }
                if unit.patrol.len() < 2 {
                    return None;
                }
                if unit.patrol.len() > 2
                    && unit.patrol[0].surface_distance_m(*unit.patrol.last().unwrap()) < 2.
                {
                    unit.patrol_reverse = false;
                    unit.path_index = if unit.path_index + 1 >= unit.patrol.len() {
                        1
                    } else {
                        unit.path_index + 1
                    };
                } else if unit.patrol_reverse {
                    if unit.path_index == 0 {
                        unit.patrol_reverse = false;
                        unit.path_index = 1;
                    } else {
                        unit.path_index -= 1;
                    }
                } else if unit.path_index + 1 >= unit.patrol.len() {
                    unit.patrol_reverse = true;
                    unit.path_index = unit.path_index.saturating_sub(1);
                } else {
                    unit.path_index += 1;
                }
            }
            _ => return None,
        }
    }
    None
}

fn next_point(unit: &LivingUnit) -> Option<GeoPosition> {
    match unit.order {
        UnitOrder::Move { .. } => unit.path.get(unit.path_index + 1).copied(),
        UnitOrder::Patrol if !unit.path.is_empty() => unit
            .path
            .get(1)
            .or_else(|| unit.patrol.get(unit.path_index))
            .copied(),
        UnitOrder::Patrol if unit.patrol_reverse => unit
            .path_index
            .checked_sub(1)
            .and_then(|n| unit.patrol.get(n))
            .copied(),
        UnitOrder::Patrol => unit
            .patrol
            .get(unit.path_index + 1)
            .or_else(|| unit.patrol.get(1))
            .copied(),
        _ => None,
    }
}

fn route_input(
    unit: &LivingUnit,
    state: &VehicleKinematics,
    config: &VehicleDynamicsConfig,
    target: GeoPosition,
    stop_at_target: bool,
    cap: f64,
    following_cap: f64,
    deceleration: f64,
) -> DriveInput {
    let frame = LocalFrame::new(unit.position);
    let delta = frame.to_local(target);
    let distance = delta[0].hypot(delta[2]);
    let bearing = delta[0].atan2(-delta[2]).to_degrees();
    let angle = angle_delta(bearing, f64::from(unit.heading_deg));
    // Low-speed reverse is useful for exact depots and legacy routes that begin
    // behind a parked body. Opposing throttle in step_vehicle brakes first.
    let reverse = reverse_toward(angle, distance, state.speed_m_s);
    let direction = if reverse { -1. } else { 1. };
    let travel_angle = if reverse {
        angle_delta(bearing, f64::from(unit.heading_deg) + 180.)
    } else {
        angle
    };
    let lookahead = distance.max(2.).min(6. + state.speed_m_s.abs() * 0.7);
    let desired_steering = if travel_angle.abs() > 1.1 {
        // Pure pursuit's sin(pi) is zero: a goal directly behind must request
        // an actual bounded turn rather than continuing straight away forever.
        travel_angle.signum() * 0.60 * direction
    } else {
        (2. * config.wheelbase_m * travel_angle.sin() / lookahead).atan() * direction
    };
    let assist = if config.steering_assist {
        1. / (1. + state.speed_m_s.abs() / 18.)
    } else {
        1.
    };
    let steering = (desired_steering / (0.60 * assist)).clamp(-1., 1.);
    let mut desired_speed = cap.min(if reverse {
        config.reverse_speed_m_s.min(2.5)
    } else {
        config.max_speed_m_s.min(following_cap)
    });
    desired_speed = desired_speed
        .min((2.3 / (desired_steering.tan().abs() / config.wheelbase_m).max(0.02)).sqrt());
    if travel_angle.abs() > 1.1 {
        desired_speed = desired_speed.min(2.);
    }
    if stop_at_target {
        desired_speed = desired_speed.min((2. * deceleration * (distance - 0.9).max(0.)).sqrt());
    } else if let Some(next) = next_point(unit) {
        let outgoing = LocalFrame::new(target).to_local(next);
        let turn = angle_delta(outgoing[0].atan2(-outgoing[2]).to_degrees(), bearing).abs();
        let corner_speed = if turn > 1.8 {
            1.5
        } else if turn > 0.75 {
            2.8
        } else {
            desired_speed
        };
        desired_speed = desired_speed.min(
            (corner_speed * corner_speed + 2. * deceleration * (distance - 3.).max(0.)).sqrt(),
        );
    }
    let current = state.speed_m_s * direction;
    if current > desired_speed + 0.1 || desired_speed < 0.08 {
        DriveInput {
            brake: ((current - desired_speed) * 0.55).clamp(0.15, 1.),
            steering,
            ..Default::default()
        }
    } else {
        DriveInput {
            throttle: direction * ((desired_speed - current) * 0.32 + 0.035).clamp(0., 0.6),
            steering,
            ..Default::default()
        }
    }
}

fn reverse_toward(angle: f64, distance: f64, speed: f64) -> bool {
    // Short rear-quarter targets should not require a forward loop into a
    // held stop line. Keep a distinct continuation threshold so small steering
    // changes do not alternate reverse and forward intent each frame.
    (angle.abs() > 1.9 && distance < 18.) || (speed < -0.25 && angle.abs() > 1.65)
}

enum DetourFailure {
    Unavailable,
    Body(u64),
}

/// Admit navigation geometry without moving the actor. The optional work
/// allowance covers new reconnection checks and narrow/cluster fallbacks. The
/// first two ordinary one-body legs retain their established work allowance.
/// Traffic clips and source/body sweeps remain authoritative for every leg
/// admitted here and for all later physical movement.
fn admit_detour(
    unit: &LivingUnit,
    points: &mut [GeoPosition],
    contacts: &LivingContacts,
    driver: &AutonomousDriver<'_>,
    budget: Option<&mut usize>,
) -> Result<ActorState, DetourFailure> {
    let from = ActorState {
        id: unit.id,
        kind: unit.kind,
        position: unit.position,
        heading_deg: unit.heading_deg,
        velocity_enu_m_s: unit.velocity_enu_m_s,
    };
    admit_detour_from(from, points, contacts, driver, budget)
}

fn admit_detour_from(
    mut from: ActorState,
    points: &mut [GeoPosition],
    contacts: &LivingContacts,
    driver: &AutonomousDriver<'_>,
    mut budget: Option<&mut usize>,
) -> Result<ActorState, DetourFailure> {
    for point in points {
        if let Some(left) = budget.as_deref_mut() {
            if *left == 0 {
                return Err(DetourFailure::Unavailable);
            }
            *left -= 1;
        }
        if driver.traffic.clip(from.id, from.position, *point).1 < 1. {
            return Err(DetourFailure::Unavailable);
        }
        let probe = admit_source_leg(&from, *point, driver)?;
        let motion = contacts
            .resolve(
                from.id,
                from.position,
                from.heading_deg,
                probe.position,
                probe.heading_deg,
                [0.; 3],
            )
            .map_err(|_| DetourFailure::Unavailable)?;
        if let Some(contact) = motion.contact {
            return Err(DetourFailure::Body(contact.other_id));
        }
        *point = probe.position;
        from = probe;
    }
    Ok(from)
}

fn admit_source_leg(
    from: &ActorState,
    point: GeoPosition,
    driver: &AutonomousDriver<'_>,
) -> Result<ActorState, DetourFailure> {
    resolve_source_leg(from, source_leg_proposal(from, point), driver)
}

fn source_leg_proposal(from: &ActorState, point: GeoPosition) -> ActorState {
    let delta = LocalFrame::new(from.position).to_local(point);
    let bearing = delta[0].atan2(-delta[2]).to_degrees();
    let heading = f64::from(from.heading_deg).to_radians();
    let speed = from.velocity_enu_m_s[0] * heading.sin() + from.velocity_enu_m_s[1] * heading.cos();
    let reverse = reverse_toward(
        angle_delta(bearing, f64::from(from.heading_deg)),
        delta[0].hypot(delta[2]),
        speed,
    );
    ActorState {
        position: point,
        heading_deg: (bearing + if reverse { 180. } else { 0. }).rem_euclid(360.) as f32,
        velocity_enu_m_s: [0.; 3],
        ..from.clone()
    }
}

fn resolve_source_leg(
    from: &ActorState,
    probe: ActorState,
    driver: &AutonomousDriver<'_>,
) -> Result<ActorState, DetourFailure> {
    let position = (driver.environment.resolve)(from, &probe, driver.dimensions)
        .filter(|p| p.validate().is_ok() && p.surface_distance_m(probe.position) < 0.05)
        .ok_or(DetourFailure::Unavailable)?;
    Ok(ActorState { position, ..probe })
}

/// Legacy one-body maneuvers remain incremental around transient occupancy.
/// Check the complete reconnecting source envelope now, while every later
/// physical step rechecks its then-current traffic reservation and contacts.
fn admit_reconnection_source(
    from: ActorState,
    point: GeoPosition,
    driver: &AutonomousDriver<'_>,
    budget: &mut usize,
) -> Result<ActorState, DetourFailure> {
    // Bound geometry before entering the source callback, even for an SDK
    // provider that accepts arbitrary segments. A call count alone does not
    // bound the amount of terrain a distant original waypoint would request.
    let distance = from
        .position
        .surface_distance_m(point)
        .hypot(point.altitude_m - from.position.altitude_m);
    if distance > MAX_LOCAL_ROUTE_M || *budget == 0 {
        return Err(DetourFailure::Unavailable);
    }
    let probe = source_leg_proposal(&from, point);
    let angle = angle_delta(f64::from(probe.heading_deg), f64::from(from.heading_deg));
    let radius = driver.dimensions[0].hypot(driver.dimensions[2]) * 0.5;
    // A complete local reconnection can exceed one source provider's per-call
    // sweep allowance. Split the same interpolated pose envelope, including
    // its original heading/reverse choice, instead of rejecting supported
    // careers or checking only a prefix. Each segment uses the shared budget.
    let segments = ((distance + radius * angle.abs()) / MAX_RECONNECTION_SWEEP_M)
        .ceil()
        .max(1.) as usize;
    if segments > *budget {
        return Err(DetourFailure::Unavailable);
    }
    let frame = LocalFrame::new(from.position);
    let delta = frame.to_local(point);
    let mut previous = from.clone();
    for n in 1..=segments {
        let fraction = n as f64 / segments as f64;
        let next = if n == segments {
            probe.clone()
        } else {
            let mut position = frame
                .to_geo(delta.map(|v| v * fraction))
                .map_err(|_| DetourFailure::Unavailable)?;
            position.altitude_m =
                from.position.altitude_m + (point.altitude_m - from.position.altitude_m) * fraction;
            ActorState {
                position,
                heading_deg: (f64::from(from.heading_deg) + angle.to_degrees() * fraction)
                    .rem_euclid(360.) as f32,
                ..probe.clone()
            }
        };
        // Recheck after a provider has adjusted support height. No callback
        // may inherit an unexpectedly large step from an earlier response.
        let sweep = previous
            .position
            .surface_distance_m(next.position)
            .hypot(next.position.altitude_m - previous.position.altitude_m)
            + radius
                * angle_delta(f64::from(next.heading_deg), f64::from(previous.heading_deg)).abs();
        if sweep > MAX_RECONNECTION_SWEEP_M + 0.1 {
            return Err(DetourFailure::Unavailable);
        }
        *budget -= 1;
        previous = resolve_source_leg(&previous, next, driver)?;
    }
    Ok(previous)
}

fn commit_detour<const N: usize>(unit: &mut LivingUnit, points: [GeoPosition; N]) {
    if matches!(unit.order, UnitOrder::Patrol) {
        unit.path.extend(points);
    } else {
        // Completed waypoints no longer own route capacity. A long-lived
        // blockage must remain recoverable when its supported flank returns.
        unit.path.drain(..unit.path_index);
        unit.path_index = 0;
        unit.path.splice(0..0, points);
    }
}

fn insert_detour(
    unit: &mut LivingUnit,
    target: GeoPosition,
    blocker: u64,
    contacts: &LivingContacts,
    driver: &AutonomousDriver<'_>,
    detour_budget: &mut usize,
) -> bool {
    let environment = driver.environment;
    match unit.order {
        UnitOrder::Move { .. }
            if unit.path.len().saturating_sub(unit.path_index) + 2 <= 129
                && unit.path_index < unit.path.len() => {}
        // A patrol saves only one pending maneuver, separately from its circuit.
        UnitOrder::Patrol if unit.path.is_empty() => {}
        _ => return false,
    }
    let heading = f64::from(unit.heading_deg).to_radians();
    let forward = LocalFrame::new(unit.position)
        .to_geo([heading.sin() * 10., 0., -heading.cos() * 10.])
        .ok();
    if !contacts.obstacle_ahead(blocker, unit.position, target)
        && !forward.is_some_and(|p| contacts.obstacle_ahead(blocker, unit.position, p))
    {
        return false;
    }
    let mut blocked_sides = Vec::with_capacity(2);
    for side in [1., -1.] {
        let Some(mut points) =
            contacts.avoidance_corners(unit.id, blocker, unit.position, target, side)
        else {
            continue;
        };
        if unit.position.surface_distance_m(points[0]) < 2.
            || target.surface_distance_m(points[0]) < 2.
            || points.iter().any(|p| (environment.sample)(*p).is_none())
            || unit
                .path
                .get(unit.path_index)
                .is_some_and(|p| p.surface_distance_m(points[0]) < 3.)
        {
            continue;
        }
        let admission = admit_detour(unit, &mut points, contacts, driver, None)
            .and_then(|from| admit_reconnection_source(from, target, driver, detour_budget));
        match admission {
            Ok(_) => {
                commit_detour(unit, points);
                return true;
            }
            Err(DetourFailure::Body(other)) if other != blocker => {
                blocked_sides.push((side, other));
            }
            _ => {}
        }
    }
    // Refine only an encountered static-body conflict. Unsupported source and
    // held intersection boundaries cannot create a larger route proposal.
    // Both original one-body sides have already had their normal opportunity.
    for (side, other) in blocked_sides {
        let mut obstacles = vec![blocker, other];
        while *detour_budget > 0 {
            let Some(corners) = contacts.avoidance_cluster_corners(
                unit.id,
                &obstacles,
                unit.position,
                target,
                side,
            ) else {
                break;
            };
            let mut points = [corners[0], corners[1], target];
            let cost = unit.position.surface_distance_m(points[0])
                + points[0].surface_distance_m(points[1])
                + points[1].surface_distance_m(points[2]);
            if corners.iter().any(|p| {
                unit.position.surface_distance_m(*p) > MAX_LOCAL_CORNER_DISTANCE_M
                    || (environment.sample)(*p).is_none()
            }) || cost > MAX_LOCAL_ROUTE_M
                || unit.position.surface_distance_m(points[0]) < 2.
                || target.surface_distance_m(points[0]) < 2.
                || unit
                    .path
                    .get(unit.path_index)
                    .is_some_and(|p| p.surface_distance_m(points[0]) < 3.)
            {
                break;
            }
            // The rejoining leg is part of acceptance. A wider approach must
            // not merely strand the car at another occupied or unsupported exit.
            match admit_detour(unit, &mut points, contacts, driver, Some(detour_budget)) {
                Ok(_) => {
                    commit_detour(unit, [points[0], points[1]]);
                    return true;
                }
                Err(DetourFailure::Body(next))
                    if obstacles.len() < MAX_AVOIDANCE_CLUSTER_BODIES
                        && !obstacles.contains(&next) =>
                {
                    obstacles.push(next);
                }
                _ => break,
            }
        }
    }
    // On a narrow supported road, the conservative circumscribed-radius
    // corners may not fit even when parallel bodies can physically pass. Try
    // one smaller corridor per side after the cluster fallback, sharing its
    // finite work allowance. The reconnection is admitted with both corners.
    for side in [1., -1.] {
        let Some(corners) =
            contacts.corridor_avoidance_corners(unit.id, blocker, unit.position, target, side)
        else {
            continue;
        };
        let mut points = [corners[0], corners[1], target];
        let cost = unit.position.surface_distance_m(points[0])
            + points[0].surface_distance_m(points[1])
            + points[1].surface_distance_m(points[2]);
        if corners.iter().any(|p| {
            unit.position.surface_distance_m(*p) > MAX_LOCAL_CORNER_DISTANCE_M
                || (environment.sample)(*p).is_none()
        }) || cost > MAX_LOCAL_ROUTE_M
            || unit.position.surface_distance_m(points[0]) < 2.
            || target.surface_distance_m(points[0]) < 2.
            || unit
                .path
                .get(unit.path_index)
                .is_some_and(|p| p.surface_distance_m(points[0]) < 3.)
        {
            continue;
        }
        if admit_detour(unit, &mut points, contacts, driver, Some(detour_budget)).is_ok() {
            commit_detour(unit, [points[0], points[1]]);
            return true;
        }
    }

    // If this car reached the obstacle before the flank became supported, its
    // short generic retreat may not leave enough approach room for the narrow
    // turn. Admit a complete reverse approach plus both corners and the join
    // before committing any of it; the reverse is ordinary saved route intent.
    if matches!(unit.order, UnitOrder::Move { .. })
        && unit.path.len().saturating_sub(unit.path_index) + 3 > 129
    {
        return false;
    }
    let frame = LocalFrame::new(unit.position);
    let goal = frame.to_local(target);
    let length = goal[0].hypot(goal[2]);
    if length < 0.01
        || angle_delta(
            goal[0].atan2(-goal[2]).to_degrees(),
            f64::from(unit.heading_deg),
        )
        .abs()
            > 0.15
    {
        return false;
    }
    let direction = [goal[0] / length, goal[2] / length];
    for side in [1., -1.] {
        let Some(corners) =
            contacts.corridor_avoidance_corners(unit.id, blocker, unit.position, target, side)
        else {
            continue;
        };
        let corner = frame.to_local(corners[0]);
        let back = corner[0] * direction[0] + corner[2] * direction[1] - 9.;
        if !(-18. ..=-2.).contains(&back) {
            continue;
        }
        let Ok(mut retreat) = frame.to_geo([direction[0] * back, 0., direction[1] * back]) else {
            continue;
        };
        retreat.altitude_m = unit.position.altitude_m;
        let mut points = [retreat, corners[0], corners[1], target];
        let cost = unit.position.surface_distance_m(points[0])
            + points
                .windows(2)
                .map(|pair| pair[0].surface_distance_m(pair[1]))
                .sum::<f64>();
        if points[..3].iter().any(|p| {
            unit.position.surface_distance_m(*p) > MAX_LOCAL_CORNER_DISTANCE_M
                || (environment.sample)(*p).is_none()
        }) || cost > MAX_LOCAL_ROUTE_M
        {
            continue;
        }
        if admit_detour(unit, &mut points, contacts, driver, Some(detour_budget)).is_ok() {
            commit_detour(unit, [points[0], points[1], points[2]]);
            return true;
        }
    }

    false
}

/// Commit an admitted outward clearance to a short navigation leg. Without
/// this intent, a forward route can undo a reverse retry on the very next
/// frame and oscillate forever against crossing pedestrians or a stopped queue.
/// The saved path
/// retains both the maneuver and the original destination across pause/resume.
fn insert_clearance_waypoint(
    unit: &mut LivingUnit,
    target: GeoPosition,
    from: &ActorState,
    trial: &ActorState,
    contacts: &LivingContacts,
    driver: &AutonomousDriver<'_>,
) -> bool {
    match unit.order {
        UnitOrder::Move { .. }
            if unit.path.len().saturating_sub(unit.path_index) < 129
                && unit.path_index < unit.path.len() => {}
        UnitOrder::Patrol if unit.path.is_empty() => {}
        _ => return false,
    }
    let frame = LocalFrame::new(from.position);
    let goal = frame.to_local(target);
    let delta = frame.to_local(trial.position);
    if delta[0] * goal[0] + delta[2] * goal[2] >= -1e-6 {
        return false;
    }
    let heading = f64::from(from.heading_deg).to_radians();
    let direction = (delta[0] * heading.sin() - delta[2] * heading.cos()).signum();
    let distance = (driver.dimensions[2] + 1.5).clamp(5., 8.);
    let Ok(position) = frame.to_geo([
        direction * heading.sin() * distance,
        0.,
        -direction * heading.cos() * distance,
    ]) else {
        return false;
    };
    if (driver.environment.sample)(position).is_none()
        || driver.traffic.clip(unit.id, from.position, position).1 < 1.
    {
        return false;
    }
    let probe = ActorState {
        position,
        velocity_enu_m_s: [0.; 3],
        ..from.clone()
    };
    let Some(position) = (driver.environment.resolve)(from, &probe, driver.dimensions)
        .filter(|p| p.validate().is_ok() && p.surface_distance_m(probe.position) < 0.05)
    else {
        return false;
    };
    if contacts
        .resolve(
            unit.id,
            from.position,
            from.heading_deg,
            position,
            from.heading_deg,
            [0.; 3],
        )
        .map_or(true, |motion| motion.contact.is_some())
    {
        return false;
    }
    if matches!(unit.order, UnitOrder::Patrol) {
        unit.path.push(position);
    } else {
        unit.path.drain(..unit.path_index);
        unit.path_index = 0;
        unit.path.insert(0, position);
    }
    true
}
