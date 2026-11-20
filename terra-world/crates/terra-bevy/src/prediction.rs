//! Client-owned display prediction, constrained by the resident source world.
//! The authoritative replica is deliberately borrowed immutably throughout.
use crate::{NetworkStatus, ResidentChunks, SandboxRuntime, WorldRuntime};
use bevy::prelude::*;
use terra_core::{ActorState, GeoPosition, LocalFrame, TravelMode};
use terra_net::prediction::{PredictedInput, PredictedVehicle};

pub(crate) fn predict_owner(
    mut status: ResMut<NetworkStatus>,
    resident: Res<ResidentChunks>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
) {
    status.predicted_actor = None;
    if !status.connected {
        return;
    }
    let Some(snapshot) = status.prediction.authoritative() else {
        return;
    };
    let Some(account) = sandbox.state.living.accounts.get(&sandbox.player_id) else {
        return;
    };
    if !account.possessed
        || account.active_unit != snapshot.actor.id
        || sandbox
            .state
            .living
            .units
            .get(&snapshot.actor.id)
            .is_none_or(|unit| unit.owner != Some(sandbox.player_id))
    {
        return;
    }
    let weather = runtime.effective_weather();
    let Ok(mut contacts) = terra_sandbox::LivingContacts::new(&sandbox.state) else {
        return;
    };
    let mut structures = sandbox.state.living.facility_footprints();
    structures.extend(
        sandbox
            .state
            .structures
            .values()
            .map(|structure| (structure.position, structure.radius_m)),
    );
    let input = status.desired_drive.map_or(
        PredictedInput::Move(status.desired_velocity_enu_m_s),
        PredictedInput::Drive,
    );
    let preview = status.prediction.preview_with_surface(
        status.prediction_seconds,
        input,
        status.prediction_partial_seconds,
        |actor, vehicle| {
            let support = terra_core::sample_vehicle_ground(
                actor.position,
                f64::from(actor.heading_deg),
                vehicle.config.wheelbase_m,
                vehicle.dimensions_m[0] * 0.86,
                |p| {
                    crate::environment::vehicle_ground_sample(&resident, p, &weather)
                        .map(|s| sandbox.state.living.apply_authored_road_surface(p, s))
                },
            )
            .ok()?;
            Some((
                support.grip_scale(terra_core::road_weather_grip(
                    weather.precipitation_mm_h,
                    weather.temperature_c,
                )),
                support.pitch_radians,
            ))
        },
        |from, candidate, vehicle| {
            let mut next =
                resolve_preview(&resident, &weather, &structures, from, candidate, vehicle)?;
            let admitted = contacts
                .resolve(
                    from.id,
                    from.position,
                    from.heading_deg,
                    next.position,
                    next.heading_deg,
                    next.velocity_enu_m_s,
                )
                .ok()?;
            next.position = admitted.position;
            next.heading_deg = admitted.heading_deg;
            if admitted.contact.is_some() {
                next.velocity_enu_m_s = [0.; 3];
            }
            // This only changes the transient local broad-phase copy.
            contacts
                .update(
                    from.id,
                    next.position,
                    next.heading_deg,
                    next.velocity_enu_m_s,
                )
                .ok()?;
            Some(next)
        },
    );
    status.predicted_actor = preview;
}

/// Conservative complete envelope checks prevent a predicted vehicle from
/// visually crossing unloaded terrain or a source building before correction.
fn resolve_preview(
    resident: &ResidentChunks,
    weather: &terra_core::WeatherSample,
    structures: &[(GeoPosition, f64)],
    from: &ActorState,
    candidate: &ActorState,
    vehicle: Option<&PredictedVehicle>,
) -> Option<ActorState> {
    let mut next = candidate.clone();
    let frame = LocalFrame::new(from.position);
    let delta = frame.to_local(candidate.position);
    let angle =
        (f64::from(candidate.heading_deg - from.heading_deg) + 540.).rem_euclid(360.) - 180.;
    let radius = vehicle.map_or(0., |vehicle| {
        vehicle.dimensions_m[0].hypot(vehicle.dimensions_m[2]) * 0.5
    });
    let swept_distance =
        from.position.surface_distance_m(candidate.position) + radius * angle.abs().to_radians();
    let required = (swept_distance / 0.25).ceil() as usize;
    // Refuse, rather than undersample, an oversized preview envelope.
    if required > 128 {
        return None;
    }
    let steps = required.max(1);
    let mut prior = if let Some(vehicle) = vehicle {
        terra_core::vehicle_support_points(
            from.position,
            f64::from(from.heading_deg),
            vehicle.dimensions_m,
        )
        .ok()?
        .to_vec()
    } else {
        vec![from.position]
    };
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let point = frame.to_geo(delta.map(|x| x * t)).ok()?;
        let envelope = if let Some(vehicle) = vehicle {
            terra_core::vehicle_support_points(
                point,
                f64::from(from.heading_deg) + angle * t,
                vehicle.dimensions_m,
            )
            .ok()?
            .to_vec()
        } else {
            vec![point]
        };
        for (previous, point) in prior.iter().zip(&envelope) {
            let surface = crate::environment::physical_surface(resident, *point, weather)?;
            match from.kind.travel_mode() {
                TravelMode::Ground => {
                    if surface.water
                        || surface.slope_degrees > if vehicle.is_some() { 32. } else { 44. }
                        || crate::environment::source_building_at_with_clearance(
                            resident,
                            *point,
                            if vehicle.is_some() { 0.2 } else { 0.32 },
                            vehicle.map_or(1.8, |vehicle| vehicle.dimensions_m[1]),
                        )
                        || !terra_sandbox::facility_path_clear(
                            terra_core::ActorKind::Human,
                            *previous,
                            *point,
                            structures,
                        )
                    {
                        return None;
                    }
                }
                TravelMode::Water if !surface.water => return None,
                _ => {}
            }
        }
        prior = envelope;
    }
    let surface = crate::environment::physical_surface(resident, next.position, weather)?;
    next.position.altitude_m = match from.kind.travel_mode() {
        TravelMode::Ground => surface.terrain_height_m + 0.025,
        TravelMode::Water => surface.terrain_height_m.max(0.) + 0.35,
        TravelMode::Air => next.position.altitude_m.max(surface.terrain_height_m + 3.),
    };
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};
    #[test]
    fn predicted_envelope_requires_loaded_terrain_and_cannot_cross_a_source_building() {
        let origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let weather =
            terra_core::synthetic_weather(&origin, &terra_core::WorldClock::default(), 42);
        let key = terra_core::TileKey::at(origin, 15).unwrap();
        let mut chunk = terra_core::WorldChunk {
            key,
            heights: terra_core::HeightField::constant(0.),
            features: vec![],
            sources: vec![],
            warnings: vec![],
        };
        let frame = LocalFrame::new(origin);
        let from = ActorState {
            id: 3,
            kind: terra_core::ActorKind::Vehicle,
            position: origin,
            velocity_enu_m_s: [0.; 3],
            heading_deg: 90.,
        };
        let mut candidate = from.clone();
        candidate.position = frame.to_geo([3., 0., 0.]).unwrap();
        candidate.velocity_enu_m_s = [4., 0., 0.];
        let vehicle = PredictedVehicle {
            config: Default::default(),
            dynamics: Default::default(),
            dimensions_m: [2., 2., 4.],
        };
        let mut resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::new(),
        };
        assert!(
            resolve_preview(&resident, &weather, &[], &from, &candidate, Some(&vehicle)).is_none()
        );
        resident.chunks.insert(key, Arc::new(chunk.clone()));
        assert!(
            resolve_preview(&resident, &weather, &[], &from, &candidate, Some(&vehicle)).is_some()
        );
        chunk.features.push(terra_core::WorldFeature {
            id: "solid-source-wall".into(),
            kind: terra_core::FeatureKind::Building,
            points: [[2., 0., -6.], [2.3, 0., -6.], [2.3, 0., 6.], [2., 0., 6.]]
                .map(|offset| frame.to_geo(offset).unwrap())
                .to_vec(),
            height_m: 4.,
            properties: BTreeMap::new(),
        });
        resident.chunks.insert(key, Arc::new(chunk));
        assert!(
            resolve_preview(&resident, &weather, &[], &from, &candidate, Some(&vehicle)).is_none()
        );
        assert_eq!(from.position, origin);
    }
}
