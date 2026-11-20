use terra_core::{
    DriveInput, GeoPosition, LocalFrame, VehicleGroundSample, VehicleKinematics, VehicleSurface,
    step_vehicle,
};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let id = *state.transport.vehicles.keys().next().unwrap();
    state
        .set_player_position(1, state.living.units[&id].position)
        .unwrap();
    command(
        &mut state,
        &catalog,
        TransportCommand::EnterVehicle { vehicle_id: id },
    );
    (state, catalog, id)
}
fn command(state: &mut SandboxState, catalog: &Catalog, action: TransportCommand) -> Outcome {
    state
        .apply(1, Command::Transport { action }, catalog)
        .unwrap()
}
fn at_facility(state: &mut SandboxState, facility: u64) {
    state
        .set_player_position(1, state.transport.facilities[&facility].position)
        .unwrap();
    let active = state.living.accounts[&1].active_unit;
    state
        .living
        .units
        .get_mut(&active)
        .unwrap()
        .velocity_enu_m_s = [0.; 3];
}

#[test]
fn reserved_passengers_have_no_mass_until_boarded_and_saved_load_resumes_exactly() {
    let (mut state, catalog, vehicle) = fixture();
    let base = state.transport.vehicle_dynamics_config(vehicle).unwrap();
    let job = state
        .transport
        .jobs
        .values()
        .find(|j| j.kind == TransportJobKind::Hitchhiker)
        .unwrap()
        .clone();
    command(
        &mut state,
        &catalog,
        TransportCommand::AcceptJob {
            job_id: job.id,
            vehicle_id: vehicle,
        },
    );
    assert_eq!(
        state.transport.vehicle_dynamics_config(vehicle).unwrap(),
        base
    );
    assert_eq!(
        state.transport.vehicle_load(vehicle).unwrap().passengers_kg,
        0.
    );
    at_facility(&mut state, job.pickup_facility);
    command(
        &mut state,
        &catalog,
        TransportCommand::PickupJob { job_id: job.id },
    );
    let loaded = state.transport.vehicle_dynamics_config(vehicle).unwrap();
    assert_eq!(
        loaded.mass_kg - base.mass_kg,
        f64::from(job.passengers) * PASSENGER_MASS_KG
    );
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    assert_eq!(
        restored.transport.vehicle_dynamics_config(vehicle),
        state.transport.vehicle_dynamics_config(vehicle)
    );
    let mut original_motion = VehicleKinematics::default();
    let mut restored_motion = original_motion;
    for _ in 0..40 {
        let input = DriveInput {
            throttle: 0.8,
            steering: 0.2,
            ..Default::default()
        };
        assert_eq!(
            step_vehicle(&loaded, &mut original_motion, input, 0.05, 0.65, 0.08),
            step_vehicle(
                &restored.transport.vehicle_dynamics_config(vehicle).unwrap(),
                &mut restored_motion,
                input,
                0.05,
                0.65,
                0.08
            )
        );
    }
    assert_eq!(original_motion, restored_motion);
    at_facility(&mut state, job.destination_facility);
    command(
        &mut state,
        &catalog,
        TransportCommand::CompleteJob { job_id: job.id },
    );
    assert_eq!(
        state.transport.vehicle_load(vehicle).unwrap().passengers_kg,
        0.
    );
    assert_eq!(
        state
            .transport
            .vehicle_dynamics_config(vehicle)
            .unwrap()
            .mass_kg,
        base.mass_kg
    );
}

#[test]
fn trailer_and_actual_cargo_are_counted_once_and_change_handling_and_fuel() {
    let (mut state, catalog, vehicle) = fixture();
    let base = state.transport.vehicle_dynamics_config(vehicle).unwrap();
    let mut empty_state = state.clone();
    let garage = state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    at_facility(&mut state, garage);
    let trailer = command(
        &mut state,
        &catalog,
        TransportCommand::BuyTrailer {
            facility_id: garage,
        },
    )
    .created_id
    .unwrap();
    command(
        &mut state,
        &catalog,
        TransportCommand::HitchTrailer {
            vehicle_id: vehicle,
            trailer_id: trailer,
        },
    );
    let tare = 600. + f64::from(state.transport.trailers[&trailer].capacity) * 4.;
    assert_eq!(
        state.transport.vehicle_load(vehicle).unwrap().trailer_kg,
        tare
    );
    assert_eq!(
        state
            .transport
            .vehicle_dynamics_config(vehicle)
            .unwrap()
            .mass_kg,
        base.mass_kg + tare
    );
    let job = state
        .transport
        .jobs
        .values()
        .find(|j| j.item.as_deref() == Some("wood"))
        .unwrap()
        .clone();
    command(
        &mut state,
        &catalog,
        TransportCommand::AcceptJob {
            job_id: job.id,
            vehicle_id: vehicle,
        },
    );
    assert_eq!(state.transport.vehicle_load(vehicle).unwrap().cargo_kg, 0.);
    at_facility(&mut state, job.pickup_facility);
    command(
        &mut state,
        &catalog,
        TransportCommand::PickupJob { job_id: job.id },
    );
    let load = state.transport.vehicle_load(vehicle).unwrap();
    let loaded = state.transport.vehicle_dynamics_config(vehicle).unwrap();
    assert_eq!(load.cargo_kg, f64::from(job.amount) * CARGO_UNIT_MASS_KG);
    assert_eq!(loaded.mass_kg, base.mass_kg + tare + load.cargo_kg);
    let mut light = VehicleKinematics::default();
    let mut heavy = light;
    for _ in 0..100 {
        let input = DriveInput {
            throttle: 1.,
            ..Default::default()
        };
        step_vehicle(&base, &mut light, input, 0.05, 1., 0.).unwrap();
        step_vehicle(&loaded, &mut heavy, input, 0.05, 1., 0.).unwrap();
    }
    assert!(light.speed_m_s > heavy.speed_m_s + 1.);
    light = VehicleKinematics {
        speed_m_s: 20.,
        ..Default::default()
    };
    heavy = light;
    for _ in 0..20 {
        step_vehicle(&base, &mut light, DriveInput::stopped(), 0.05, 1., 0.).unwrap();
        step_vehicle(&loaded, &mut heavy, DriveInput::stopped(), 0.05, 1., 0.).unwrap();
    }
    assert!(heavy.speed_m_s > light.speed_m_s + 0.5);
    let empty_fuel = empty_state.transport.vehicles[&vehicle].fuel_l;
    let loaded_fuel = state.transport.vehicles[&vehicle].fuel_l;
    empty_state
        .record_vehicle_motion(1, vehicle, 100., 5., 0.)
        .unwrap();
    state
        .record_vehicle_motion(1, vehicle, 100., 5., 0.)
        .unwrap();
    assert!(
        loaded_fuel - state.transport.vehicles[&vehicle].fuel_l
            > empty_fuel - empty_state.transport.vehicles[&vehicle].fuel_l
    );
    state.validate_with_catalog(&catalog).unwrap();
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(restored.transport.vehicle_load(vehicle), Some(load));
    assert_eq!(
        restored.transport.vehicle_dynamics_config(vehicle),
        state.transport.vehicle_dynamics_config(vehicle)
    );
    state
        .transport
        .trailers
        .get_mut(&trailer)
        .unwrap()
        .attached_to = None;
    assert!(
        state.transport.vehicle_dynamics_config(vehicle).is_none(),
        "broken towing references must fail closed"
    );
}

#[test]
fn authored_road_material_respects_ribbon_without_rewriting_height_or_source_roads() {
    let origin = GeoPosition::new(48., 179.999999, 20.).unwrap();
    let frame = LocalFrame::new(origin);
    let mut living = LivingWorld::default();
    living.roads.push(RoadRoute {
        id: "authored:test".into(),
        points: vec![
            frame.to_geo([-20., 0., 0.]).unwrap(),
            frame.to_geo([20., 0., 0.]).unwrap(),
        ],
        width_m: 6.,
        rules: Default::default(),
    });
    let ground = VehicleGroundSample {
        height_m: 123.5,
        surface: VehicleSurface::Grass,
    };
    let road = living.apply_authored_road_surface(origin, ground);
    assert_eq!(road.height_m, ground.height_m);
    assert_eq!(road.surface, VehicleSurface::Paved);
    assert_eq!(
        living.apply_authored_road_surface(frame.to_geo([0., 0., 4.]).unwrap(), ground),
        ground
    );
    living.roads[0].id = "osm:source-track".into();
    assert_eq!(living.apply_authored_road_surface(origin, ground), ground);
}
