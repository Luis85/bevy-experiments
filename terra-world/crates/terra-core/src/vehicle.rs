//! Bounded, fixed-step reference road-vehicle dynamics shared by local and
//! network authorities. Controls are intent; they never contain coordinates.
//! This surface bicycle model supplies acceleration, braking, steering, grip
//! loss and a gearbox. A game can replace it with its own rigid-body/tire solver.

use crate::{ActorKind, ActorState};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VehicleError {
    #[error("vehicle input, configuration, or state is outside its finite bounds")]
    Invalid,
    #[error("vehicle step must be finite and between zero and 0.25 seconds")]
    TimeStep,
}

/// Negative throttle requests reverse. Opposite throttle first brakes to rest;
/// it cannot reverse a moving vehicle in a single step. Positive steering turns
/// right. All values are normalized and validated before any state mutation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DriveInput {
    pub throttle: f64,
    pub brake: f64,
    pub steering: f64,
    pub handbrake: bool,
}

impl DriveInput {
    pub fn validate(&self) -> Result<(), VehicleError> {
        if bounded(self.throttle, -1., 1.)
            && bounded(self.brake, 0., 1.)
            && bounded(self.steering, -1., 1.)
        {
            Ok(())
        } else {
            Err(VehicleError::Invalid)
        }
    }

    pub fn stopped() -> Self {
        Self {
            brake: 1.,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleDynamicsConfig {
    pub mass_kg: f64,
    pub wheelbase_m: f64,
    pub engine_power_kw: f64,
    pub brake_force_n: f64,
    pub max_speed_m_s: f64,
    pub reverse_speed_m_s: f64,
    pub grip: f64,
    pub steering_assist: bool,
    pub traction_control: bool,
    pub abs: bool,
}

impl Default for VehicleDynamicsConfig {
    fn default() -> Self {
        Self {
            mass_kg: 1400.,
            wheelbase_m: 2.65,
            engine_power_kw: 95.,
            brake_force_n: 13_000.,
            max_speed_m_s: 40.,
            reverse_speed_m_s: 8.,
            grip: 1.,
            steering_assist: true,
            traction_control: true,
            abs: true,
        }
    }
}

impl VehicleDynamicsConfig {
    pub fn validate(&self) -> Result<(), VehicleError> {
        if bounded(self.mass_kg, 200., 100_000.)
            && bounded(self.wheelbase_m, 1., 20.)
            && bounded(self.engine_power_kw, 1., 2000.)
            && bounded(self.brake_force_n, 100., 1_500_000.)
            && bounded(self.max_speed_m_s, 1., 100.)
            && bounded(self.reverse_speed_m_s, 0.1, 20.)
            && bounded(self.grip, 0.1, 3.)
        {
            Ok(())
        } else {
            Err(VehicleError::Invalid)
        }
    }
}

/// Runtime handling state is serializable so a restored vehicle does not lose
/// its signed speed, steering or gearbox. World pose remains on the actor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VehicleKinematics {
    pub speed_m_s: f64,
    pub lateral_speed_m_s: f64,
    pub steering_rad: f64,
    pub yaw_rate_rad_s: f64,
    pub engine_rpm: f64,
    pub gear: i8,
}

impl Default for VehicleKinematics {
    fn default() -> Self {
        Self {
            speed_m_s: 0.,
            lateral_speed_m_s: 0.,
            steering_rad: 0.,
            yaw_rate_rad_s: 0.,
            engine_rpm: 800.,
            gear: 0,
        }
    }
}

impl VehicleKinematics {
    pub fn validate(&self) -> Result<(), VehicleError> {
        if bounded(self.speed_m_s, -20., 100.)
            && bounded(self.lateral_speed_m_s, -30., 30.)
            && bounded(self.steering_rad, -0.75, 0.75)
            && bounded(self.yaw_rate_rad_s, -4., 4.)
            && bounded(self.engine_rpm, 0., 12_000.)
            && (-1..=8).contains(&self.gear)
        {
            Ok(())
        } else {
            Err(VehicleError::Invalid)
        }
    }

    /// Clear physical momentum after a rejected move or a parked vehicle.
    pub fn stop(&mut self) {
        self.speed_m_s = 0.;
        self.lateral_speed_m_s = 0.;
        self.yaw_rate_rad_s = 0.;
        self.gear = 0;
        self.engine_rpm = 800.;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleStep {
    pub heading_delta_deg: f64,
    pub forward_distance_m: f64,
    pub lateral_distance_m: f64,
    pub acceleration_m_s2: f64,
    pub slipping: bool,
}

/// Integrate a candidate geodetic pose without admitting it into the world.
/// Both authorities use this helper; callers resolve its swept path against
/// their resident terrain before committing the pose and handling state.
pub fn drive_actor(
    actor: &ActorState,
    state: &mut VehicleKinematics,
    config: &VehicleDynamicsConfig,
    input: DriveInput,
    dt: f64,
    grip_scale: f64,
    slope_radians: f64,
) -> Result<ActorState, VehicleError> {
    actor.validate().map_err(|_| VehicleError::Invalid)?;
    if actor.kind != ActorKind::Vehicle {
        return Err(VehicleError::Invalid);
    }
    let mut next = *state;
    let step = step_vehicle(config, &mut next, input, dt, grip_scale, slope_radians)?;
    let mut candidate = actor.clone();
    if dt == 0. {
        return Ok(candidate);
    }
    let mid = (actor.heading_deg as f64 + step.heading_delta_deg * 0.5).to_radians();
    candidate.velocity_enu_m_s = [
        (step.forward_distance_m * mid.sin() + step.lateral_distance_m * mid.cos()) / dt,
        (step.forward_distance_m * mid.cos() - step.lateral_distance_m * mid.sin()) / dt,
        0.,
    ];
    candidate.advance(dt).map_err(|_| VehicleError::Invalid)?;
    candidate.heading_deg =
        (actor.heading_deg as f64 + step.heading_delta_deg).rem_euclid(360.) as f32;
    let heading = (candidate.heading_deg as f64).to_radians();
    let speed = next.speed_m_s.hypot(next.lateral_speed_m_s);
    if speed > config.max_speed_m_s {
        let scale = config.max_speed_m_s / speed;
        next.speed_m_s *= scale;
        next.lateral_speed_m_s *= scale;
    }
    candidate.velocity_enu_m_s = [
        next.speed_m_s * heading.sin() + next.lateral_speed_m_s * heading.cos(),
        next.speed_m_s * heading.cos() - next.lateral_speed_m_s * heading.sin(),
        0.,
    ];
    *state = next;
    Ok(candidate)
}

/// Conservative reference weather grip; it deliberately does not claim a
/// physically measured tire or road model. Surface providers may override it.
pub fn road_weather_grip(precipitation_mm_h: f32, temperature_c: f32) -> f64 {
    let wet = if precipitation_mm_h.is_finite() {
        precipitation_mm_h.max(0.) as f64
    } else {
        0.
    };
    if temperature_c < 1. && wet > 0.1 {
        0.32
    } else {
        (1. - wet * 0.025).clamp(0.58, 1.)
    }
}

/// Nine bounded support samples for a model-sized reference surface envelope.
/// This is a conservative admission aid, not an articulated rigid-body hull.
pub fn vehicle_support_points(
    center: crate::GeoPosition,
    heading_deg: f64,
    dimensions: [f64; 3],
) -> Result<[crate::GeoPosition; 9], VehicleError> {
    if center.validate().is_err()
        || !heading_deg.is_finite()
        || !bounded(dimensions[0], 0.5, 8.)
        || !bounded(dimensions[1], 0.5, 8.)
        || !bounded(dimensions[2], 1., 30.)
    {
        return Err(VehicleError::Invalid);
    }
    let width = dimensions[0] * 0.5;
    let length = dimensions[2] * 0.5;
    let angle = heading_deg.to_radians();
    let frame = crate::LocalFrame::new(center);
    let mut points = [center; 9];
    // Keep the supplied center exact; only offset support points need an ECEF
    // round trip. This also preserves a stable anchor at the date line/poles.
    for (point, (right, forward)) in points.iter_mut().skip(1).zip([
        (-width, -length),
        (width, -length),
        (-width, length),
        (width, length),
        (0., -length),
        (0., length),
        (-width, 0.),
        (width, 0.),
    ]) {
        *point = frame
            .to_geo([
                forward * angle.sin() + right * angle.cos(),
                0.,
                -forward * angle.cos() + right * angle.sin(),
            ])
            .map_err(|_| VehicleError::Invalid)?;
    }
    Ok(points)
}

/// Conservative clearance beneath an explicitly elevated source building part.
/// `min_height_m` is above terrain at the building footprint, never an absolute
/// coordinate from source geometry. Unknown/non-finite dimensions fail closed.
/// A 15 cm reference margin avoids fitting a roof exactly against an underside.
pub fn has_underpass_clearance(
    ground_altitude_m: f64,
    body_base_altitude_m: f64,
    body_height_m: f64,
    min_height_m: f64,
) -> bool {
    [
        ground_altitude_m,
        body_base_altitude_m,
        body_height_m,
        min_height_m,
    ]
    .iter()
    .all(|v| v.is_finite())
        && (0.1..=32.).contains(&body_height_m)
        && (0.0..=2000.).contains(&min_height_m)
        && body_base_altitude_m.max(ground_altitude_m) + body_height_m + 0.15
            < ground_altitude_m + min_height_m
}

fn bounded(value: f64, minimum: f64, maximum: f64) -> bool {
    value.is_finite() && (minimum..=maximum).contains(&value)
}

fn approach(value: f64, target: f64, change: f64) -> f64 {
    value + (target - value).clamp(-change, change)
}

/// Integrate physical seconds, never calendar time. `grip_scale` describes the
/// sampled surface/weather and `slope_radians` is positive uphill along the
/// vehicle's forward axis. Terrain/collision admission remains the host's job.
/// Invalid inputs leave the complete state unchanged.
pub fn step_vehicle(
    config: &VehicleDynamicsConfig,
    state: &mut VehicleKinematics,
    input: DriveInput,
    dt: f64,
    grip_scale: f64,
    slope_radians: f64,
) -> Result<VehicleStep, VehicleError> {
    config.validate()?;
    state.validate()?;
    input.validate()?;
    if !bounded(dt, 0., 0.25) {
        return Err(VehicleError::TimeStep);
    }
    if !bounded(grip_scale, 0.05, 2.) || !bounded(slope_radians, -1.2, 1.2) {
        return Err(VehicleError::Invalid);
    }
    if dt == 0. {
        return Ok(VehicleStep {
            heading_delta_deg: 0.,
            forward_distance_m: 0.,
            lateral_distance_m: 0.,
            acceleration_m_s2: 0.,
            slipping: false,
        });
    }
    let mut next = *state;
    let initial_speed = next.speed_m_s;
    let friction = (config.grip * grip_scale).clamp(0.05, 3.);
    // The longitudinal slope reduces the contact-normal load. Cross-slope,
    // semantic material and rough support are supplied through grip_scale.
    let traction_force = config.mass_kg * 9.80665 * friction * slope_radians.cos();
    let direction = if input.throttle.abs() > 0.001 {
        input.throttle.signum()
    } else {
        0.
    };
    let opposing =
        initial_speed.abs() > 0.35 && direction != 0. && direction != initial_speed.signum();
    let drive_request = if opposing || input.handbrake {
        0.
    } else {
        input.throttle
    };
    let engine_force =
        (config.engine_power_kw * 1000. / initial_speed.abs().max(4.)).min(config.mass_kg * 7.5);
    let traction_limit = traction_force * if config.traction_control { 0.88 } else { 1. };
    let driving_force = (drive_request * engine_force).clamp(-traction_limit, traction_limit);
    let braking = input
        .brake
        .max(if opposing { input.throttle.abs() } else { 0. });
    let brake_limit = traction_force * if config.abs { 0.96 } else { 0.72 };
    let braking_force = (braking * config.brake_force_n
        + if input.handbrake {
            config.brake_force_n * 0.65
        } else {
            0.
        })
    .min(brake_limit);
    let resistance = 0.40 * initial_speed * initial_speed + config.mass_kg * 9.80665 * 0.012;
    let slope_force = config.mass_kg * 9.80665 * slope_radians.sin();
    let mut acceleration = (driving_force - slope_force) / config.mass_kg;
    // Apply drag/brakes toward zero without oscillating into reverse at rest.
    let driven = initial_speed + acceleration * dt;
    let after_drag = approach(driven, 0., resistance / config.mass_kg * dt);
    let reduced = approach(after_drag, 0., braking_force / config.mass_kg * dt);
    next.speed_m_s = reduced.clamp(-config.reverse_speed_m_s, config.max_speed_m_s);
    if opposing && next.speed_m_s.signum() != initial_speed.signum() {
        next.speed_m_s = 0.;
    }
    if input.handbrake && next.speed_m_s.abs() < 0.1 {
        next.speed_m_s = 0.;
    }
    acceleration = (next.speed_m_s - initial_speed) / dt;

    let assist = if config.steering_assist {
        1. / (1. + next.speed_m_s.abs() / 18.)
    } else {
        1.
    };
    let target_steering = input.steering * 0.60 * assist;
    next.steering_rad = approach(next.steering_rad, target_steering, 1.8 * dt);
    let requested_yaw = next.speed_m_s / config.wheelbase_m * next.steering_rad.tan();
    // Engine/brake force and cornering share one tire budget. Use only the
    // brake force actually needed this step, so holding at rest is stable and
    // cannot consume a fictitious full-speed impulse or reverse the vehicle.
    let applied_brake = (after_drag - reduced) / dt * config.mass_kg;
    let longitudinal_force = (driving_force - applied_brake).abs().min(traction_force);
    let lateral_acceleration = (traction_force * traction_force
        - longitudinal_force * longitudinal_force)
        .max(0.)
        .sqrt()
        / config.mass_kg;
    let yaw_limit = (lateral_acceleration / next.speed_m_s.abs().max(1.5)).min(2.8);
    next.yaw_rate_rad_s = requested_yaw.clamp(-yaw_limit, yaw_limit);
    let slipping =
        requested_yaw.abs() > yaw_limit + 1e-9 || (input.handbrake && next.speed_m_s.abs() > 3.);
    let slip_target = if slipping {
        -input.steering * next.speed_m_s.abs() * if input.handbrake { 0.28 } else { 0.10 }
    } else {
        0.
    };
    next.lateral_speed_m_s = approach(next.lateral_speed_m_s, slip_target, friction * 9.80665 * dt);
    next.lateral_speed_m_s = next.lateral_speed_m_s.clamp(-15., 15.);
    next.gear = if next.speed_m_s < -0.15 {
        -1
    } else if next.speed_m_s.abs() < 0.15 {
        0
    } else {
        (1 + (next.speed_m_s / 7.).floor() as i8).clamp(1, 6)
    };
    let gear_ratio = match next.gear {
        -1 => 3.2,
        0 => 0.,
        1 => 3.4,
        2 => 2.2,
        3 => 1.55,
        4 => 1.16,
        5 => 0.92,
        _ => 0.72,
    };
    next.engine_rpm = (800.
        + next.speed_m_s.abs() / (std::f64::consts::TAU * 0.34) * 60. * gear_ratio * 3.5)
        .clamp(800., 7200.);
    next.validate()?;
    let result = VehicleStep {
        heading_delta_deg: next.yaw_rate_rad_s.to_degrees() * dt,
        forward_distance_m: (initial_speed + next.speed_m_s) * 0.5 * dt,
        lateral_distance_m: next.lateral_speed_m_s * dt,
        acceleration_m_s2: acceleration,
        slipping,
    };
    if ![
        result.heading_delta_deg,
        result.forward_distance_m,
        result.lateral_distance_m,
        result.acceleration_m_s2,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(VehicleError::Invalid);
    }
    *state = next;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_accelerates_with_inertia_and_brakes_without_reversing() {
        let cfg = VehicleDynamicsConfig::default();
        let mut state = VehicleKinematics::default();
        let throttle = DriveInput {
            throttle: 1.,
            ..Default::default()
        };
        step_vehicle(&cfg, &mut state, throttle, 0.05, 1., 0.).unwrap();
        assert!((0.1..0.5).contains(&state.speed_m_s));
        for _ in 0..300 {
            step_vehicle(&cfg, &mut state, throttle, 0.05, 1., 0.).unwrap();
        }
        assert!(state.speed_m_s > 20. && state.speed_m_s <= cfg.max_speed_m_s);
        for _ in 0..300 {
            step_vehicle(&cfg, &mut state, DriveInput::stopped(), 0.05, 1., 0.).unwrap();
        }
        assert_eq!(state.speed_m_s, 0.);
    }

    #[test]
    fn reverse_throttle_brakes_first_and_reverse_steering_changes_sign() {
        let cfg = VehicleDynamicsConfig::default();
        let mut state = VehicleKinematics {
            speed_m_s: 12.,
            ..Default::default()
        };
        let reverse = DriveInput {
            throttle: -1.,
            steering: 1.,
            ..Default::default()
        };
        let first = step_vehicle(&cfg, &mut state, reverse, 0.05, 1., 0.).unwrap();
        assert!(state.speed_m_s > 0. && first.heading_delta_deg > 0.);
        for _ in 0..240 {
            step_vehicle(&cfg, &mut state, reverse, 0.05, 1., 0.).unwrap();
        }
        let last = step_vehicle(&cfg, &mut state, reverse, 0.05, 1., 0.).unwrap();
        assert!(state.speed_m_s < -1. && state.speed_m_s >= -cfg.reverse_speed_m_s);
        assert!(last.heading_delta_deg < 0.);
    }

    #[test]
    fn wet_grip_and_mass_affect_stopping_and_acceleration() {
        let cfg = VehicleDynamicsConfig::default();
        let initial = VehicleKinematics {
            speed_m_s: 20.,
            ..Default::default()
        };
        let mut dry = initial;
        let mut wet = initial;
        for _ in 0..20 {
            step_vehicle(&cfg, &mut dry, DriveInput::stopped(), 0.05, 1., 0.).unwrap();
            step_vehicle(&cfg, &mut wet, DriveInput::stopped(), 0.05, 0.3, 0.).unwrap();
        }
        assert!(wet.speed_m_s > dry.speed_m_s + 3.);
        let heavy = VehicleDynamicsConfig {
            mass_kg: 7000.,
            ..cfg
        };
        let mut light_state = VehicleKinematics::default();
        let mut heavy_state = light_state;
        for _ in 0..100 {
            step_vehicle(
                &cfg,
                &mut light_state,
                DriveInput {
                    throttle: 1.,
                    ..Default::default()
                },
                0.05,
                1.,
                0.,
            )
            .unwrap();
            step_vehicle(
                &heavy,
                &mut heavy_state,
                DriveInput {
                    throttle: 1.,
                    ..Default::default()
                },
                0.05,
                1.,
                0.,
            )
            .unwrap();
        }
        assert!(light_state.speed_m_s > heavy_state.speed_m_s + 3.);
    }

    #[test]
    fn service_brake_holds_both_slopes_without_creep_and_release_rolls_downhill() {
        let config = VehicleDynamicsConfig::default();
        for slope in [-0.2_f64, 0.2] {
            for input in [
                DriveInput::stopped(),
                DriveInput {
                    handbrake: true,
                    ..Default::default()
                },
            ] {
                let mut state = VehicleKinematics::default();
                for _ in 0..400 {
                    let step = step_vehicle(&config, &mut state, input, 0.05, 1., slope).unwrap();
                    assert_eq!(
                        state.speed_m_s, 0.,
                        "a held brake must not creep on a supported slope"
                    );
                    assert_eq!(step.forward_distance_m, 0.);
                }
            }
            let mut free = VehicleKinematics::default();
            for _ in 0..20 {
                step_vehicle(&config, &mut free, DriveInput::default(), 0.05, 1., slope).unwrap();
            }
            assert!(
                free.speed_m_s * slope < -0.2,
                "released brakes must permit gravity to roll downhill"
            );
        }
        let mut weak = VehicleKinematics::default();
        for _ in 0..20 {
            step_vehicle(&config, &mut weak, DriveInput::stopped(), 0.05, 0.1, 0.4).unwrap();
        }
        assert!(
            weak.speed_m_s < -1.,
            "service braking cannot invent grip on an icy steep slope"
        );
    }

    #[test]
    fn cornering_and_braking_share_the_available_contact_force() {
        let config = VehicleDynamicsConfig {
            steering_assist: false,
            ..Default::default()
        };
        let initial = VehicleKinematics {
            speed_m_s: 25.,
            steering_rad: 0.6,
            ..Default::default()
        };
        let input = DriveInput {
            steering: 1.,
            ..Default::default()
        };
        let mut coasting = initial;
        let mut braking = initial;
        let coast = step_vehicle(&config, &mut coasting, input, 0.05, 1., 0.).unwrap();
        let brake = step_vehicle(
            &config,
            &mut braking,
            DriveInput { brake: 1., ..input },
            0.05,
            1.,
            0.,
        )
        .unwrap();
        assert!(coast.slipping && brake.slipping);
        assert!(braking.yaw_rate_rad_s.abs() < coasting.yaw_rate_rad_s.abs() * 0.5);
        let normal = config.mass_kg * 9.80665 * config.grip;
        let braking_force = config.brake_force_n.min(normal * 0.96);
        let corner_force = config.mass_kg * braking.speed_m_s.abs() * braking.yaw_rate_rad_s.abs();
        assert!(braking_force.hypot(corner_force) <= normal + 1e-6);
        assert!(braking.speed_m_s < coasting.speed_m_s);
    }

    #[test]
    fn invalid_or_zero_steps_never_mutate_state() {
        let cfg = VehicleDynamicsConfig::default();
        let mut state = VehicleKinematics::default();
        let before = state;
        for dt in [f64::NAN, -1., 0.26] {
            assert!(step_vehicle(&cfg, &mut state, DriveInput::default(), dt, 1., 0.).is_err());
            assert_eq!(state, before);
        }
        assert!(
            step_vehicle(
                &cfg,
                &mut state,
                DriveInput {
                    steering: f64::NAN,
                    ..Default::default()
                },
                0.05,
                1.,
                0.
            )
            .is_err()
        );
        step_vehicle(&cfg, &mut state, DriveInput::default(), 0., 1., 0.).unwrap();
        assert_eq!(state, before);
    }

    #[test]
    fn subnormal_step_cannot_publish_infinite_acceleration_or_partial_state() {
        let cfg = VehicleDynamicsConfig {
            max_speed_m_s: 1.,
            ..Default::default()
        };
        let mut state = VehicleKinematics {
            speed_m_s: 100.,
            ..Default::default()
        };
        let before = state;
        assert_eq!(
            step_vehicle(&cfg, &mut state, DriveInput::default(), 1e-320, 1., 0.),
            Err(VehicleError::Invalid)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn vehicle_support_envelope_retains_model_length_across_heading_and_date_line() {
        let p = crate::GeoPosition::new(48., 179.99999, 12.).unwrap();
        let points = vehicle_support_points(p, 90., [2.65, 3.25, 10.8]).unwrap();
        assert!(points.iter().all(|p| p.validate().is_ok()));
        assert!((points[5].surface_distance_m(points[6]) - 10.8).abs() < 0.05);
        assert!((points[7].surface_distance_m(points[8]) - 2.65).abs() < 0.05);
        assert_eq!(points[0], p);
    }

    #[test]
    fn fixed_input_is_deterministic_and_serialized_state_resumes_exactly() {
        let cfg = VehicleDynamicsConfig::default();
        let input = DriveInput {
            throttle: 0.8,
            steering: 0.2,
            ..Default::default()
        };
        let mut a = VehicleKinematics::default();
        for _ in 0..100 {
            step_vehicle(&cfg, &mut a, input, 0.05, 0.8, 0.04).unwrap();
        }
        let mut b: VehicleKinematics =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        for _ in 0..100 {
            assert_eq!(
                step_vehicle(&cfg, &mut a, input, 0.05, 0.8, 0.04).unwrap(),
                step_vehicle(&cfg, &mut b, input, 0.05, 0.8, 0.04).unwrap()
            );
        }
        assert_eq!(a, b);
    }
    #[test]
    fn underpass_clearance_uses_body_height_real_terrain_and_a_safety_margin() {
        assert!(has_underpass_clearance(350., 350.025, 1.55, 2.4));
        assert!(!has_underpass_clearance(350., 350.025, 3.25, 2.4));
        assert!(!has_underpass_clearance(350., 352., 1.55, 3.0));
        assert!(!has_underpass_clearance(350., 350., 2.25, 2.4));
        assert!(!has_underpass_clearance(350., 350., f64::NAN, 4.));
        assert!(!has_underpass_clearance(350., 350., 1.8, f64::INFINITY));
        assert!(!has_underpass_clearance(350., 0., 3.25, 2.4));
    }
}
