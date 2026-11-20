use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

fn isolated_driver(origin: GeoPosition, heading: f32) -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let home = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, home).unwrap();
    state.add_player(2, home).unwrap();
    state.seed_living_demo(home, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let id = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    state.transport.vehicles.get_mut(&id).unwrap().model = VehicleModel::Pickup;
    let body = state.living.units.get_mut(&id).unwrap();
    body.position = origin;
    body.heading_deg = heading;
    body.velocity_enu_m_s = [0.; 3];
    state.set_player_position(1, origin).unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle { vehicle_id: id },
            },
            &catalog,
        )
        .unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog, id)
}

fn origin() -> GeoPosition {
    GeoPosition::new(48.147, 11.585, 100.).unwrap()
}

fn level(mut point: GeoPosition) -> Option<GeoPosition> {
    point.altitude_m = 100.;
    Some(point)
}

#[test]
fn a_single_player_reference_crew_has_a_usable_grounded_dismount() {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    state.add_player(1, origin()).unwrap();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let vehicle = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    let position = state.living.units[&vehicle].position;
    state.set_player_position(1, position).unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle {
                    vehicle_id: vehicle,
                },
            },
            &catalog,
        )
        .unwrap();
    state.exit_vehicle_with_ground(1, &catalog, level).unwrap();
    let avatar = &state.living.units[&state.living.accounts[&1].avatar_unit];
    assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
    assert!(avatar.position.surface_distance_m(position) > 1.5);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn grounded_exit_checks_the_whole_avatar_and_resumes_deterministically() {
    let (mut state, catalog, vehicle) = isolated_driver(origin(), 0.);
    let frame = LocalFrame::new(origin());
    // The first left-door center is dry, but its outer avatar corners cross
    // this coverage edge. A center-only test would select the wrong side.
    let ground = |point| (frame.to_local(point)[0] >= -1.8).then(|| level(point).unwrap());
    assert!(ground(frame.to_geo([-1.66, 0., -1.275]).unwrap()).is_some());
    let before = state.clone();
    let mut restored: SandboxState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    state.exit_vehicle_with_ground(1, &catalog, ground).unwrap();
    restored
        .exit_vehicle_with_ground(1, &catalog, ground)
        .unwrap();
    assert_eq!(state, restored);
    let account = &state.living.accounts[&1];
    let avatar = &state.living.units[&account.avatar_unit];
    assert_eq!(account.active_unit, account.avatar_unit);
    assert!(account.possessed);
    assert!(frame.to_local(avatar.position)[0] > 1.5);
    assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
    assert_eq!(avatar.velocity_enu_m_s, [0.; 3]);
    assert_eq!(state.players[&1].position, avatar.position);
    assert!(state.transport.vehicles[&vehicle].parking_brake);
    assert!(!state.transport.vehicles[&vehicle].autopilot);
    assert_eq!(state.transport.vehicles[&vehicle].dynamics.speed_m_s, 0.);
    assert_eq!(state.transport.vehicles[&vehicle].dynamics.engine_rpm, 0.);
    assert_eq!(
        state.living.units[&vehicle].position,
        before.living.units[&vehicle].position
    );
    assert_eq!(
        state.living.accounts[&1].money,
        before.living.accounts[&1].money
    );
    assert_eq!(state.players[&1].inventory, before.players[&1].inventory);
    assert_eq!(
        state.transport.vehicles[&vehicle].cargo,
        before.transport.vehicles[&vehicle].cargo
    );
    assert_eq!(
        state.transport.vehicles[&vehicle].fuel_l,
        before.transport.vehicles[&vehicle].fuel_l
    );
    assert_eq!(
        state.transport.vehicles[&vehicle].odometer_m,
        before.transport.vehicles[&vehicle].odometer_m
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn unsupported_shifted_or_discontinuous_exit_ground_rejects_without_mutation() {
    let (state, catalog, _) = isolated_driver(origin(), 0.);
    let frame = LocalFrame::new(origin());
    let cases: Vec<Box<dyn Fn(GeoPosition) -> Option<GeoPosition>>> = vec![
        Box::new(|_| None),
        Box::new(|mut p| {
            p.altitude_m = f64::NAN;
            Some(p)
        }),
        Box::new(move |p| LocalFrame::new(p).to_geo([10., 0., 0.]).ok()),
        Box::new(move |p| {
            // A dry center strip does not support a whole avatar.
            (frame.to_local(p)[0].abs() < 0.1).then(|| level(p).unwrap())
        }),
        Box::new(move |mut p| {
            // Clear endpoints cannot authorize crossing a narrow unsupported
            // ring between the seat and every dismount position.
            let r = origin().surface_distance_m(p);
            if (0.65..0.95).contains(&r) {
                return None;
            }
            p.altitude_m = 100.;
            Some(p)
        }),
        Box::new(move |mut p| {
            p.altitude_m = if origin().surface_distance_m(p) > 0.8 {
                96.
            } else {
                100.
            };
            Some(p)
        }),
    ];
    for (index, ground) in cases.into_iter().enumerate() {
        let mut candidate = state.clone();
        assert!(
            candidate
                .exit_vehicle_with_ground(1, &catalog, ground)
                .is_err(),
            "case {index}"
        );
        assert_eq!(
            candidate, state,
            "case {index} changed possession, position or parking state"
        );
    }
}

#[test]
fn rotated_avatar_corners_cannot_cross_an_uncovered_edge() {
    let (mut state, catalog, _) = isolated_driver(origin(), 45.);
    let frame = LocalFrame::new(origin());
    // The diagonal door center and its axis-aligned 0.55 m square are dry.
    // The actual 45-degree avatar corner extends beyond this coverage edge.
    let ground = |point| (frame.to_local(point)[0] >= -0.6).then(|| level(point).unwrap());
    let radius = 0.55_f64.hypot(0.55) * 0.5;
    let first_door_east = (-(2.05 * 0.5 + radius + 0.25) + 5.1 * 0.25) / 2_f64.sqrt();
    assert!(first_door_east - 0.275 > -0.6);
    assert!(first_door_east - radius < -0.6);
    state.exit_vehicle_with_ground(1, &catalog, ground).unwrap();
    let avatar = &state.living.units[&state.living.accounts[&1].avatar_unit];
    assert_eq!(avatar.heading_deg, 45.);
    assert!(frame.to_local(avatar.position)[0] > 1.5);
    assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
}

#[test]
fn dismount_avoids_resident_dormant_people_and_other_vehicle_corridors() {
    let (mut state, catalog, vehicle) = isolated_driver(origin(), 0.);
    let frame = LocalFrame::new(origin());
    let blocker = state.living.accounts[&2].avatar_unit;
    let mut left_door = frame.to_geo([-1.66, 0., -1.275]).unwrap();
    left_door.altitude_m = 100.;
    state.set_player_position(2, left_door).unwrap();
    state.set_player_dormant(2, true).unwrap();
    let blocked_body = state.living.units[&blocker].clone();
    state.exit_vehicle_with_ground(1, &catalog, level).unwrap();
    let avatar = &state.living.units[&state.living.accounts[&1].avatar_unit];
    assert!(frame.to_local(avatar.position)[0] > 1.5);
    assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
    assert_eq!(state.living.units[&blocker], blocked_body);
    state.validate_with_catalog(&catalog).unwrap();

    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle {
                    vehicle_id: vehicle,
                },
            },
            &catalog,
        )
        .unwrap();
    let other = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 2)
        .unwrap()
        .unit_id;
    state.living.units.get_mut(&other).unwrap().position = origin();
    let before = state.clone();
    assert!(state.exit_vehicle_with_ground(1, &catalog, level).is_err());
    assert_eq!(
        state, before,
        "a clear endpoint cannot teleport through the other parked vehicle"
    );
}

#[test]
fn exiting_cannot_cross_a_structure_or_ignore_a_moving_vehicle() {
    let (mut state, catalog, vehicle) = isolated_driver(origin(), 0.);
    let structure_id = state.next_id;
    state.next_id += 1;
    state.structures.insert(
        structure_id,
        Structure {
            id: structure_id,
            owner: 1,
            faction_owner: None,
            blueprint: "fence".into(),
            position: origin(),
            hitpoints: 100,
            max_hitpoints: 100,
            radius_m: 0.8,
            housing: false,
            workbench: false,
        },
    );
    let before = state.clone();
    assert!(state.exit_vehicle_with_ground(1, &catalog, level).is_err());
    assert_eq!(state, before);
    state.structures.remove(&structure_id);
    state
        .transport
        .vehicles
        .get_mut(&vehicle)
        .unwrap()
        .dynamics
        .speed_m_s = 0.6;
    let before = state.clone();
    assert!(state.exit_vehicle_with_ground(1, &catalog, level).is_err());
    assert_eq!(state, before);
}

#[test]
fn legacy_exit_command_places_clear_of_rotated_vehicles_at_geodetic_seams() {
    for point in [
        GeoPosition::new(0., 179.99999, 100.).unwrap(),
        GeoPosition::new(89.99, -179.9, 100.).unwrap(),
    ] {
        let (mut state, catalog, vehicle) = isolated_driver(point, 37.);
        state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::ExitVehicle,
                },
                &catalog,
            )
            .unwrap();
        let account = &state.living.accounts[&1];
        let avatar = &state.living.units[&account.avatar_unit];
        assert_eq!(avatar.kind, ActorKind::Human);
        assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
        assert!(avatar.position.surface_distance_m(point) < 5.);
        assert!(
            avatar
                .position
                .surface_distance_m(state.living.units[&vehicle].position)
                > 1.5
        );
        state.validate_with_catalog(&catalog).unwrap();
    }
}
