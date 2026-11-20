//! Bounded owner-only presentation prediction. These poses never enter domain
//! saves, cargo proofs, authority, contacts of other actors, or command payloads.
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use terra_core::{ActorKind, ActorState, DriveInput, VehicleDynamicsConfig, VehicleKinematics};

pub const PREDICTION_HORIZON_SECONDS: f64 = 0.25;
pub const MAX_PREDICTED_INPUTS: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PredictedVehicle {
    pub config: VehicleDynamicsConfig,
    pub dynamics: VehicleKinematics,
    pub dimensions_m: [f64; 3],
}

/// Sent only to the principal controlling this body. `sequence` names the
/// latest held input present at this completed authoritative fixed step, not an
/// acknowledgement that a freshly received input has already been simulated.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MotionSnapshot {
    pub player_id: u64,
    pub tick: u64,
    pub sequence: Option<u64>,
    pub actor: ActorState,
    pub movement_allowed: bool,
    pub maximum_speed_m_s: f64,
    pub vehicle: Option<PredictedVehicle>,
}
impl MotionSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.player_id == 0
            || self.actor.id == 0
            || self.actor.validate().is_err()
            || !self.maximum_speed_m_s.is_finite()
            || !(0.0..=500.0).contains(&self.maximum_speed_m_s)
            || self.vehicle.as_ref().is_some_and(|vehicle| {
                self.actor.kind != ActorKind::Vehicle
                    || vehicle.config.validate().is_err()
                    || vehicle.dynamics.validate().is_err()
                    || vehicle
                        .dimensions_m
                        .iter()
                        .any(|v| !v.is_finite() || !(0.1..=32.).contains(v))
            })
        {
            Err("Invalid authoritative motion snapshot.".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PredictedInput {
    Move([f64; 3]),
    Drive(DriveInput),
}
impl PredictedInput {
    fn valid(self) -> bool {
        match self {
            Self::Move(v) => v.iter().all(|v| v.is_finite() && v.abs() <= 500.),
            Self::Drive(v) => v.validate().is_ok(),
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct InputFrame {
    sequence: u64,
    input: PredictedInput,
    seconds: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PredictionStats {
    pub reconciliations: u64,
    pub body_resets: u64,
    pub horizon_resets: u64,
    pub rejected_snapshots: u64,
    pub maximum_authority_gap_m: f64,
    pub last_authority_gap_m: f64,
    pub pending_inputs: usize,
}

#[derive(Default)]
pub struct ClientPrediction {
    authoritative: Option<MotionSnapshot>,
    inputs: VecDeque<InputFrame>,
    last_sent: u64,
    last_received_at: f64,
    suspended_until_snapshot: bool,
    pub stats: PredictionStats,
}
impl ClientPrediction {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn authoritative(&self) -> Option<&MotionSnapshot> {
        self.authoritative.as_ref()
    }
    pub fn pending_len(&self) -> usize {
        self.inputs.len()
    }
    pub fn suspend(&mut self) {
        self.inputs.clear();
        self.suspended_until_snapshot = true;
        self.stats.pending_inputs = 0;
    }
    pub fn record(
        &mut self,
        sequence: u64,
        input: PredictedInput,
        seconds: f64,
    ) -> Result<(), String> {
        if sequence <= self.last_sent
            || !input.valid()
            || !seconds.is_finite()
            || !(0.0..=0.1).contains(&seconds)
        {
            return Err("Invalid or repeated predicted input.".into());
        }
        self.last_sent = sequence;
        if self.authoritative.is_none() || self.suspended_until_snapshot {
            return Ok(());
        }
        let total = self.inputs.iter().map(|input| input.seconds).sum::<f64>() + seconds;
        if self.inputs.len() >= MAX_PREDICTED_INPUTS || total > PREDICTION_HORIZON_SECONDS + 1e-9 {
            self.suspend();
            self.stats.horizon_resets = self.stats.horizon_resets.saturating_add(1);
        } else {
            self.inputs.push_back(InputFrame {
                sequence,
                input,
                seconds,
            });
            self.stats.pending_inputs = self.inputs.len();
        }
        Ok(())
    }
    pub fn reconcile(
        &mut self,
        snapshot: MotionSnapshot,
        now: f64,
        displayed: Option<&ActorState>,
    ) -> Result<bool, String> {
        if snapshot.validate().is_err()
            || !now.is_finite()
            || now < 0.
            || snapshot
                .sequence
                .is_some_and(|sequence| sequence > self.last_sent)
            || self
                .authoritative
                .as_ref()
                .is_some_and(|old| snapshot.player_id != old.player_id)
        {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return Err("Invalid authoritative motion acknowledgement.".into());
        }
        if self
            .authoritative
            .as_ref()
            .is_some_and(|old| snapshot.tick <= old.tick)
        {
            return Ok(false);
        }
        if self.authoritative.as_ref().is_some_and(|old| {
            old.sequence
                .is_some_and(|previous| snapshot.sequence.is_none_or(|next| next < previous))
        }) {
            self.stats.rejected_snapshots = self.stats.rejected_snapshots.saturating_add(1);
            return Err("Authoritative input acknowledgement moved backwards.".into());
        }
        let changed_body = self.authoritative.as_ref().is_none_or(|old| {
            old.actor.id != snapshot.actor.id || old.actor.kind != snapshot.actor.kind
        });
        if changed_body || !snapshot.movement_allowed {
            self.inputs.clear();
            self.stats.body_resets = self.stats.body_resets.saturating_add(1);
        } else {
            self.inputs.retain(|input| {
                snapshot
                    .sequence
                    .is_none_or(|sequence| input.sequence > sequence)
            });
        }
        if let Some(displayed) = displayed.filter(|actor| actor.id == snapshot.actor.id) {
            let correction = displayed
                .position
                .surface_distance_m(snapshot.actor.position)
                .hypot(displayed.position.altitude_m - snapshot.actor.position.altitude_m);
            if correction.is_finite() {
                self.stats.last_authority_gap_m = correction;
                self.stats.maximum_authority_gap_m =
                    self.stats.maximum_authority_gap_m.max(correction);
            }
        }
        self.stats.reconciliations = self.stats.reconciliations.saturating_add(1);
        self.stats.pending_inputs = self.inputs.len();
        self.authoritative = Some(snapshot);
        self.last_received_at = now;
        self.suspended_until_snapshot = false;
        Ok(true)
    }
    /// Replays only outstanding input, with at most one partial display frame.
    /// The resolver must admit terrain and contacts; None stops locally. Its
    /// output is validated before use. This is a preview, never authority.
    pub fn preview(
        &self,
        now: f64,
        current: PredictedInput,
        partial_seconds: f64,
        grip: f64,
        slope: f64,
        resolve: impl FnMut(&ActorState, &ActorState, Option<&PredictedVehicle>) -> Option<ActorState>,
    ) -> Option<ActorState> {
        self.preview_with_surface(
            now,
            current,
            partial_seconds,
            |_, _| Some((grip, slope)),
            resolve,
        )
    }

    /// Sample grip and longitudinal slope at each replayed vehicle pose. A
    /// terrain boundary stops the preview instead of reusing the original
    /// snapshot's surface throughout a turn or source-material transition.
    pub fn preview_with_surface(
        &self,
        now: f64,
        current: PredictedInput,
        partial_seconds: f64,
        mut surface: impl FnMut(&ActorState, &PredictedVehicle) -> Option<(f64, f64)>,
        mut resolve: impl FnMut(
            &ActorState,
            &ActorState,
            Option<&PredictedVehicle>,
        ) -> Option<ActorState>,
    ) -> Option<ActorState> {
        let snapshot = self.authoritative.as_ref()?;
        let mut actor = snapshot.actor.clone();
        if !now.is_finite()
            || now < self.last_received_at
            || now - self.last_received_at > PREDICTION_HORIZON_SECONDS
            || !snapshot.movement_allowed
            || self.suspended_until_snapshot
            || !current.valid()
        {
            return Some(actor);
        }
        let mut vehicle = snapshot.vehicle.clone();
        let mut total = 0.;
        let partial = if partial_seconds.is_finite() {
            partial_seconds.clamp(0., 0.05)
        } else {
            0.
        };
        for (input, dt) in self
            .inputs
            .iter()
            .map(|f| (f.input, f.seconds))
            .chain(std::iter::once((current, partial)))
        {
            if total + dt > PREDICTION_HORIZON_SECONDS + 1e-9 {
                break;
            }
            total += dt;
            if dt <= 0. {
                continue;
            }
            let mut candidate = actor.clone();
            match (input, vehicle.as_mut()) {
                (PredictedInput::Drive(input), Some(vehicle))
                    if snapshot.maximum_speed_m_s > 0. =>
                {
                    let Some((grip, slope)) = surface(&actor, vehicle) else {
                        actor.velocity_enu_m_s = [0.; 3];
                        vehicle.dynamics.stop();
                        break;
                    };
                    let mut config = vehicle.config;
                    config.max_speed_m_s =
                        config.max_speed_m_s.min(snapshot.maximum_speed_m_s.max(1.));
                    candidate = terra_core::drive_actor(
                        &actor,
                        &mut vehicle.dynamics,
                        &config,
                        input,
                        dt,
                        grip,
                        slope,
                    )
                    .ok()?;
                }
                (PredictedInput::Move(velocity), None) => {
                    candidate.velocity_enu_m_s = velocity;
                    let speed = candidate.speed_m_s();
                    if speed > snapshot.maximum_speed_m_s {
                        candidate.velocity_enu_m_s =
                            velocity.map(|v| v * snapshot.maximum_speed_m_s / speed);
                    }
                    if !matches!(
                        candidate.kind,
                        ActorKind::Airplane | ActorKind::Helicopter | ActorKind::Balloon
                    ) {
                        candidate.velocity_enu_m_s[2] = 0.;
                    }
                    candidate.advance(dt).ok()?;
                    if candidate.speed_m_s() > 0.01 {
                        candidate.heading_deg =
                            velocity[0].atan2(velocity[1]).to_degrees().rem_euclid(360.) as f32;
                    }
                }
                _ => {
                    candidate.velocity_enu_m_s = [0.; 3];
                    if let Some(vehicle) = &mut vehicle {
                        vehicle.dynamics.stop();
                    }
                }
            }
            let admitted = resolve(&actor, &candidate, vehicle.as_ref());
            match admitted.filter(|next| {
                next.id == actor.id
                    && next.kind == actor.kind
                    && next.validate().is_ok()
                    && next.speed_m_s() <= snapshot.maximum_speed_m_s + 0.001
                    && snapshot
                        .actor
                        .position
                        .surface_distance_m(next.position)
                        .hypot(snapshot.actor.position.altitude_m - next.position.altitude_m)
                        <= snapshot.maximum_speed_m_s.max(1.) * total + 3.
                    && actor
                        .position
                        .surface_distance_m(next.position)
                        .hypot(actor.position.altitude_m - next.position.altitude_m)
                        <= snapshot.maximum_speed_m_s.max(1.) * dt + 3.
            }) {
                Some(next) => {
                    actor = next;
                    if actor.speed_m_s() < 0.0001 {
                        if let Some(vehicle) = &mut vehicle {
                            vehicle.dynamics.stop();
                        }
                    }
                }
                None => {
                    actor.velocity_enu_m_s = [0.; 3];
                    if let Some(vehicle) = &mut vehicle {
                        vehicle.dynamics.stop();
                    }
                    break;
                }
            }
        }
        Some(actor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(tick: u64, sequence: Option<u64>) -> MotionSnapshot {
        MotionSnapshot {
            player_id: 1,
            tick,
            sequence,
            actor: ActorState {
                id: 100,
                kind: ActorKind::Human,
                position: terra_core::GeoPosition::new(48., 11., 0.).unwrap(),
                velocity_enu_m_s: [0.; 3],
                heading_deg: 0.,
            },
            movement_allowed: true,
            maximum_speed_m_s: 4.,
            vehicle: None,
        }
    }
    #[test]
    fn reconciles_only_unacknowledged_input_and_ignores_old_snapshots() {
        let mut p = ClientPrediction::default();
        p.reconcile(snapshot(1, None), 0., None).unwrap();
        p.record(1, PredictedInput::Move([4., 0., 0.]), 0.05)
            .unwrap();
        p.record(2, PredictedInput::Move([4., 0., 0.]), 0.05)
            .unwrap();
        let preview = p
            .preview(
                0.1,
                PredictedInput::Move([4., 0., 0.]),
                0.,
                1.,
                0.,
                |_, next, _| Some(next.clone()),
            )
            .unwrap();
        assert!(
            (preview
                .position
                .surface_distance_m(snapshot(1, None).actor.position)
                - 0.4)
                .abs()
                < 0.001
        );
        let mut ack = snapshot(2, Some(1));
        ack.actor.position = terra_core::LocalFrame::new(ack.actor.position)
            .to_geo([0.2, 0., 0.])
            .unwrap();
        p.reconcile(ack.clone(), 0.1, Some(&preview)).unwrap();
        assert_eq!(p.pending_len(), 1);
        assert!(!p.reconcile(snapshot(1, None), 0.2, None).unwrap());
        let same = p
            .preview(
                0.1,
                PredictedInput::Move([0.; 3]),
                0.,
                1.,
                0.,
                |_, next, _| Some(next.clone()),
            )
            .unwrap();
        assert!(same.position.surface_distance_m(preview.position) < 0.001);
        assert!(p.reconcile(snapshot(3, Some(99)), 0.2, None).is_err());
        assert_eq!(p.authoritative().unwrap().tick, 2);
    }
    #[test]
    fn absent_terrain_horizon_body_change_and_pause_fail_closed() {
        let mut p = ClientPrediction::default();
        let origin = snapshot(1, None);
        p.reconcile(origin.clone(), 0., None).unwrap();
        p.record(1, PredictedInput::Move([4., 0., 0.]), 0.05)
            .unwrap();
        let blocked = p
            .preview(
                0.1,
                PredictedInput::Move([4., 0., 0.]),
                0.05,
                1.,
                0.,
                |_, _, _| None,
            )
            .unwrap();
        assert_eq!(blocked.position, origin.actor.position);
        assert_eq!(
            p.preview(
                1.,
                PredictedInput::Move([4., 0., 0.]),
                0.05,
                1.,
                0.,
                |_, a, _| Some(a.clone())
            )
            .unwrap()
            .position,
            origin.actor.position
        );
        for seq in 2..=20 {
            p.record(seq, PredictedInput::Move([4., 0., 0.]), 0.05)
                .unwrap();
        }
        assert_eq!(p.pending_len(), 0);
        assert_eq!(p.stats.horizon_resets, 1);
        let mut next = snapshot(2, Some(20));
        next.actor.id = 101;
        next.movement_allowed = false;
        p.reconcile(next.clone(), 1., None).unwrap();
        assert_eq!(
            p.preview(
                1.,
                PredictedInput::Move([4., 0., 0.]),
                0.05,
                1.,
                0.,
                |_, a, _| Some(a.clone())
            )
            .unwrap()
            .position,
            next.actor.position
        );
        assert!(p.record(20, PredictedInput::Move([0.; 3]), 0.05).is_err());
        assert!(
            p.record(21, PredictedInput::Move([f64::NAN; 3]), 0.05)
                .is_err()
        );
    }
    #[test]
    fn vehicle_preview_uses_shared_dynamics_without_mutating_authority_or_inventory() {
        let mut p = ClientPrediction::default();
        let mut origin = snapshot(1, None);
        origin.actor.kind = ActorKind::Vehicle;
        origin.maximum_speed_m_s = 30.;
        origin.vehicle = Some(PredictedVehicle {
            config: VehicleDynamicsConfig::default(),
            dynamics: VehicleKinematics::default(),
            dimensions_m: [2., 2., 4.],
        });
        p.reconcile(origin.clone(), 0., None).unwrap();
        let input = DriveInput {
            throttle: 1.,
            steering: 0.4,
            ..Default::default()
        };
        p.record(1, PredictedInput::Drive(input), 0.05).unwrap();
        let actual = p
            .preview(
                0.05,
                PredictedInput::Drive(input),
                0.,
                0.8,
                0.02,
                |_, next, _| Some(next.clone()),
            )
            .unwrap();
        let mut kinematics = VehicleKinematics::default();
        let mut config = VehicleDynamicsConfig::default();
        config.max_speed_m_s = 30.;
        let expected = terra_core::drive_actor(
            &origin.actor,
            &mut kinematics,
            &config,
            input,
            0.05,
            0.8,
            0.02,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(p.authoritative(), Some(&origin));
        assert_ne!(actual.position, origin.actor.position);
    }

    #[test]
    fn vehicle_replay_resamples_surface_each_step_and_stops_at_unknown_wheels() {
        let mut prediction = ClientPrediction::default();
        let mut origin = snapshot(1, None);
        origin.actor.kind = ActorKind::Vehicle;
        origin.maximum_speed_m_s = 30.;
        origin.vehicle = Some(PredictedVehicle {
            config: VehicleDynamicsConfig::default(),
            dynamics: VehicleKinematics::default(),
            dimensions_m: [2., 2., 4.],
        });
        prediction.reconcile(origin.clone(), 0., None).unwrap();
        let input = DriveInput {
            throttle: 1.,
            steering: 0.2,
            ..Default::default()
        };
        for sequence in 1..=3 {
            prediction
                .record(sequence, PredictedInput::Drive(input), 0.05)
                .unwrap();
        }
        let mut samples = Vec::new();
        let mut admitted = Vec::new();
        let preview = prediction
            .preview_with_surface(
                0.15,
                PredictedInput::Drive(input),
                0.,
                |actor, _| {
                    samples.push(actor.position);
                    match samples.len() {
                        1 => Some((1., 0.)),
                        2 => Some((0.25, 0.10)),
                        _ => None,
                    }
                },
                |_, next, _| {
                    admitted.push(next.clone());
                    Some(next.clone())
                },
            )
            .unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(admitted.len(), 2);
        assert_eq!(samples[0], origin.actor.position);
        assert_eq!(samples[1], admitted[0].position);
        assert_eq!(samples[2], admitted[1].position);
        assert_eq!(preview.position, admitted[1].position);
        assert_eq!(preview.velocity_enu_m_s, [0.; 3]);
        assert_eq!(prediction.authoritative(), Some(&origin));
        let mut config = origin.vehicle.as_ref().unwrap().config;
        config.max_speed_m_s = 30.;
        let mut state = origin.vehicle.as_ref().unwrap().dynamics;
        let first =
            terra_core::drive_actor(&origin.actor, &mut state, &config, input, 0.05, 1., 0.)
                .unwrap();
        let second =
            terra_core::drive_actor(&first, &mut state, &config, input, 0.05, 0.25, 0.10).unwrap();
        assert_eq!(admitted, [first, second]);
    }
    #[test]
    fn malformed_resolver_cannot_change_identity_speed_or_vertical_location() {
        for fault in 0..4 {
            let mut p = ClientPrediction::default();
            let origin = snapshot(1, None);
            p.reconcile(origin.clone(), 0., None).unwrap();
            p.record(1, PredictedInput::Move([4., 0., 0.]), 0.05)
                .unwrap();
            let result = p
                .preview(
                    0.05,
                    PredictedInput::Move([0.; 3]),
                    0.,
                    1.,
                    0.,
                    |_, next, _| {
                        let mut bad = next.clone();
                        match fault {
                            0 => bad.id += 1,
                            1 => bad.position.altitude_m += 1000.,
                            2 => bad.velocity_enu_m_s = [400., 0., 0.],
                            _ => bad.heading_deg = f32::NAN,
                        }
                        Some(bad)
                    },
                )
                .unwrap();
            assert_eq!(result.position, origin.actor.position);
            assert_eq!(result.velocity_enu_m_s, [0.; 3]);
        }
        let mut p = ClientPrediction::default();
        p.record(1, PredictedInput::Move([0.; 3]), 0.05).unwrap();
        p.reconcile(snapshot(1, Some(1)), 0., None).unwrap();
        assert!(p.reconcile(snapshot(2, None), 0.1, None).is_err());
        assert_eq!(p.authoritative().unwrap().tick, 1);
    }
    #[test]
    fn grounding_allowance_is_shared_by_entire_replay_not_repeated_per_frame() {
        let mut p = ClientPrediction::default();
        let origin = snapshot(1, None);
        p.reconcile(origin.clone(), 0., None).unwrap();
        for sequence in 1..=32 {
            p.record(sequence, PredictedInput::Move([0.; 3]), 0.000001)
                .unwrap();
        }
        let result = p
            .preview(
                0.01,
                PredictedInput::Move([0.; 3]),
                0.,
                1.,
                0.,
                |previous, next, _| {
                    let mut next = next.clone();
                    next.position = terra_core::LocalFrame::new(previous.position)
                        .to_geo([2.9, 0., 0.])
                        .unwrap();
                    Some(next)
                },
            )
            .unwrap();
        assert!(result.position.surface_distance_m(origin.actor.position) < 3.001);
        assert_eq!(result.velocity_enu_m_s, [0.; 3]);
    }
}
