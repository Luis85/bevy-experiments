use terra_core::{ActorKind, GeoPosition, LocalFrame, VehicleGroundSample, VehicleSurface};
use terra_sandbox::*;

fn fixture(points: &[[f64; 2]], heading: f32) -> (SandboxState, Catalog, u64, LocalFrame) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let path: Vec<_> = points
        .iter()
        .map(|p| frame.to_geo([p[0], 0., p[1]]).unwrap())
        .collect();
    let destination = *path.last().unwrap();
    let id = 1_000_000;
    let mut state = SandboxState::default();
    state.living = LivingWorld {
        enabled: true,
        origin: Some(origin),
        next_id: id + 1,
        ..Default::default()
    };
    state.living.units.insert(
        id,
        LivingUnit {
            id,
            name: "Autonomous test car".into(),
            kind: ActorKind::Vehicle,
            owner: None,
            role: UnitRole::Traffic,
            position: origin,
            heading_deg: heading,
            velocity_enu_m_s: [0.; 3],
            needs: Needs::default(),
            order: UnitOrder::Move { destination },
            home: origin,
            path,
            path_index: 0,
            patrol: vec![],
            patrol_reverse: false,
            pocket_money: 0,
            supplies: Inventory::new(),
        },
    );
    (state, Catalog::builtin(), id, frame)
}

#[test]
fn autonomous_turns_accelerate_and_brake_without_snapping_to_waypoints() {
    let (mut world, catalog, id, _) = fixture(&[[0., -30.], [30., -30.]], 0.);
    world.advance_living(0.1, 0., &catalog).unwrap();
    let first = &world.living.units[&id];
    assert!(first.velocity_enu_m_s[0].hypot(first.velocity_enu_m_s[1]) < 0.8);
    let target = match first.order {
        UnitOrder::Move { destination } => destination,
        _ => panic!(),
    };
    let mut curved = false;
    let mut previous = first.clone();
    for _ in 0..1200 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        let unit = &world.living.units[&id];
        let turn =
            (f64::from(unit.heading_deg - previous.heading_deg) + 540.).rem_euclid(360.) - 180.;
        assert!(turn.abs() <= 16.05, "heading snapped by {turn}");
        assert!(previous.position.surface_distance_m(unit.position) < 1.1);
        let dynamics = world.living.traffic.vehicle_dynamics[&id];
        dynamics.validate().unwrap();
        curved |= dynamics.steering_rad.abs() > 0.05 && dynamics.yaw_rate_rad_s.abs() > 0.01;
        previous = unit.clone();
        if matches!(unit.order, UnitOrder::Idle) {
            break;
        }
    }
    assert!(curved);
    assert!(matches!(previous.order, UnitOrder::Idle), "{:?}", previous);
    assert!(previous.position.surface_distance_m(target) < 1.5);
    assert_eq!(world.living.traffic.vehicle_dynamics[&id].speed_m_s, 0.);
}

#[test]
fn missing_wheel_support_stops_and_restored_support_resumes_the_same_order() {
    let (mut world, catalog, id, frame) = fixture(&[[0., -30.]], 0.);
    let origin = world.living.units[&id].position;
    let sample = |p| {
        (frame.to_local(p)[0] < 0.4).then_some(VehicleGroundSample {
            height_m: 100.,
            surface: VehicleSurface::Paved,
        })
    };
    world
        .advance_living_with_vehicle_environment(
            0.2,
            0.,
            &catalog,
            true,
            &|_, _, p| Some(p),
            AutonomousVehicleEnvironment {
                sample: &sample,
                resolve: &|_, to, _| Some(to.position),
                weather_grip: 1.,
            },
        )
        .unwrap();
    assert_eq!(world.living.units[&id].position, origin);
    assert_eq!(world.living.units[&id].velocity_enu_m_s, [0.; 3]);
    assert!(matches!(
        world.living.units[&id].order,
        UnitOrder::Move { .. }
    ));
    for _ in 0..30 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert!(world.living.units[&id].position.surface_distance_m(origin) > 3.);
}

#[test]
fn pause_and_save_restore_preserve_autonomous_steering_and_physical_progress() {
    let (mut world, catalog, id, _) = fixture(&[[0., -15.], [30., -15.]], 0.);
    for _ in 0..50 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    let mut restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    let body = world.living.units[&id].clone();
    let dynamics = world.living.traffic.vehicle_dynamics[&id];
    world.advance_living(0., 0., &catalog).unwrap();
    assert_eq!(world.living.units[&id], body);
    assert_eq!(world.living.traffic.vehicle_dynamics[&id], dynamics);
    for _ in 0..100 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        restored.advance_living(0.1, 0., &catalog).unwrap();
        assert_eq!(world.living.units[&id], restored.living.units[&id]);
        assert_eq!(
            world.living.traffic.vehicle_dynamics[&id],
            restored.living.traffic.vehicle_dynamics[&id]
        );
    }
}

#[test]
fn legacy_traffic_state_loads_with_bounded_default_handling() {
    let (world, catalog, _, _) = fixture(&[[0., -30.]], 0.);
    let mut json = serde_json::to_value(world).unwrap();
    json["living"]["traffic"]
        .as_object_mut()
        .unwrap()
        .remove("vehicle_dynamics");
    let mut restored: SandboxState = serde_json::from_value(json).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    restored.advance_living(0.1, 0., &catalog).unwrap();
    assert_eq!(restored.living.traffic.vehicle_dynamics.len(), 1);
}

#[test]
fn autonomous_motion_is_independent_of_accelerated_calendar_time() {
    let (mut first, catalog, id, _) = fixture(&[[0., -60.]], 0.);
    let mut accelerated = first.clone();
    for _ in 0..100 {
        first.advance_living(0.1, 0.1, &catalog).unwrap();
        accelerated.advance_living(0.1, 60., &catalog).unwrap();
    }
    assert_eq!(
        first.living.units[&id].position,
        accelerated.living.units[&id].position
    );
    assert_eq!(
        first.living.traffic.vehicle_dynamics,
        accelerated.living.traffic.vehicle_dynamics
    );
}

#[test]
fn blocked_vehicle_envelope_does_not_claim_travel_and_can_recover() {
    let (mut world, catalog, id, _) = fixture(&[[0., -30.]], 0.);
    let origin = world.living.units[&id].position;
    let sample = |_| {
        Some(VehicleGroundSample {
            height_m: 100.,
            surface: VehicleSurface::Paved,
        })
    };
    for _ in 0..30 {
        world
            .advance_living_with_vehicle_environment(
                0.1,
                0.,
                &catalog,
                true,
                &|_, _, p| Some(p),
                AutonomousVehicleEnvironment {
                    sample: &sample,
                    resolve: &|_, _, _| None,
                    weather_grip: 1.,
                },
            )
            .unwrap();
    }
    assert_eq!(world.living.units[&id].position, origin);
    assert_eq!(world.living.traffic.vehicle_dynamics[&id].speed_m_s, 0.);
    for _ in 0..30 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert!(world.living.units[&id].position.surface_distance_m(origin) > 3.);
}

#[test]
fn a_blocked_following_queue_allows_admitted_reverse_away_from_the_leader() {
    let (mut world, catalog, id, frame) = fixture(&[[0., 10.]], 0.);
    let origin = world.living.units[&id].position;
    let mut leader = world.living.units[&id].clone();
    leader.id += 1;
    leader.position = frame.to_geo([0., 0., -6.]).unwrap();
    let stopped_at = leader.position;
    let leader_id = leader.id;
    let destination = frame.to_geo([0., 0., -30.]).unwrap();
    leader.order = UnitOrder::Move { destination };
    leader.path = vec![destination];
    world.living.next_id += 1;
    world.living.units.insert(leader.id, leader);
    let sample = |_| {
        Some(VehicleGroundSample {
            height_m: 100.,
            surface: VehicleSurface::Paved,
        })
    };
    for _ in 0..30 {
        world
            .advance_living_with_vehicle_environment(
                0.1,
                0.,
                &catalog,
                true,
                &|_, _, p| Some(p),
                AutonomousVehicleEnvironment {
                    sample: &sample,
                    resolve: &|from, to, _| (from.id == id).then_some(to.position),
                    weather_grip: 1.,
                },
            )
            .unwrap();
    }
    let moved = world.living.units[&id].position;
    assert!(
        origin.surface_distance_m(moved) > 1.,
        "rear escape remained blocked"
    );
    assert!(moved.surface_distance_m(stopped_at) > origin.surface_distance_m(stopped_at) + 1.);
    assert_eq!(world.living.units[&leader_id].position, stopped_at);
}

#[test]
fn rear_quarter_goal_reverses_away_from_a_held_forward_boundary() {
    let (mut world, catalog, id, frame) = fixture(&[[7.1187566, 5.50666]], 239.645);
    let target = world.living.units[&id].path[0];
    let heading = 239.645_f64.to_radians();
    let forward_progress = |position| {
        let delta = frame.to_local(position);
        delta[0] * heading.sin() - delta[2] * heading.cos()
    };
    let sample = |_| {
        Some(VehicleGroundSample {
            height_m: 100.,
            surface: VehicleSurface::Paved,
        })
    };
    // The current approach cannot enter the held boundary. A physically
    // admitted rear maneuver remains available on this side of it.
    let resolve = |_: &terra_core::ActorState, to: &terra_core::ActorState, _| {
        (forward_progress(to.position) <= 1e-6).then_some(to.position)
    };
    for step in 0..600 {
        let previous = world.living.units[&id].position;
        world
            .advance_living_with_vehicle_environment(
                0.1,
                0.,
                &catalog,
                true,
                &|_, _, point| Some(point),
                AutonomousVehicleEnvironment {
                    sample: &sample,
                    resolve: &resolve,
                    weather_grip: 1.,
                },
            )
            .unwrap();
        let unit = &world.living.units[&id];
        assert!(forward_progress(unit.position) <= 1e-6);
        assert!(previous.surface_distance_m(unit.position) < 0.9);
        if step == 0 {
            assert!(world.living.traffic.vehicle_dynamics[&id].speed_m_s < -0.05);
        }
        if matches!(unit.order, UnitOrder::Idle) {
            break;
        }
    }
    let unit = &world.living.units[&id];
    assert!(matches!(unit.order, UnitOrder::Idle), "{unit:?}");
    assert!(unit.position.surface_distance_m(target) < 1.5);
    assert_eq!(world.living.traffic.vehicle_dynamics[&id].speed_m_s, 0.);
}

#[test]
fn patrol_keeps_clearance_from_a_stopped_queue_across_save_resume() {
    let (mut world, catalog, id, frame) = fixture(&[[121.608, -3.135]], 96.71054);
    let origin = world.living.units[&id].position;
    let target = world.living.units[&id].path[0];
    let patrol = vec![origin, target];
    let unit = world.living.units.get_mut(&id).unwrap();
    unit.order = UnitOrder::Patrol;
    unit.patrol = patrol.clone();
    unit.path.clear();
    unit.path_index = 1;

    // A crossing road user is waiting just beyond the turning car's nose.
    // The saved, aged reservation must remain occupied until the holder backs
    // out physically, after which the waiting peer can clear the approach.
    let mut queued = world.living.units[&id].clone();
    queued.id += 1;
    queued.position = frame.to_geo([2.939, 0., 2.242]).unwrap();
    queued.heading_deg = 3.488233;
    queued.patrol = vec![queued.position, frame.to_geo([3., 0., -30.]).unwrap()];
    let queued_id = queued.id;
    let queued_position = queued.position;
    world.living.next_id += 1;
    world.living.units.insert(queued.id, queued);
    let center = [2.939, -4.634];
    world.living.roads = vec![
        RoadRoute {
            id: "test:clearance-east-west".into(),
            points: vec![
                frame.to_geo([-40., 0., center[1]]).unwrap(),
                frame.to_geo([150., 0., center[1]]).unwrap(),
            ],
            width_m: 6.,
            rules: RoadRules::default(),
        },
        RoadRoute {
            id: "test:clearance-north-south".into(),
            points: vec![
                frame.to_geo([center[0], 0., -60.]).unwrap(),
                frame.to_geo([center[0], 0., 60.]).unwrap(),
            ],
            width_m: 6.,
            rules: RoadRules::default(),
        },
    ];
    let junction = RoadNetwork::new(&world.living.roads).junctions()[0].clone();
    world.living.traffic.elapsed_seconds = 300.;
    world.living.traffic.next_ticket = 2;
    world.living.traffic.junctions = vec![TrafficJunctionState {
        junction_id: junction.id,
        queue: vec![
            TrafficRequest {
                unit_id: id,
                ticket: 1,
            },
            TrafficRequest {
                unit_id: queued_id,
                ticket: 2,
            },
        ],
        holder: Some(id),
        granted_at: 83.5,
        progress_at: 142.,
        last_position: Some(origin),
    }];
    let advance = |state: &mut SandboxState| {
        state.advance_living(0.1, 0., &catalog).unwrap();
    };
    let mut restored: Option<SandboxState> = None;
    let mut backed_away = false;
    let mut completed_clearance = false;
    let mut released_junction = false;
    for _ in 0..300 {
        let previous = world.living.units[&id].clone();
        let contacts = LivingContacts::new(&world).unwrap();
        advance(&mut world);
        let unit = &world.living.units[&id];
        assert!(previous.position.surface_distance_m(unit.position) < 0.9);
        assert!(
            contacts
                .resolve(
                    id,
                    previous.position,
                    previous.heading_deg,
                    unit.position,
                    unit.heading_deg,
                    unit.velocity_enu_m_s,
                )
                .unwrap()
                .contact
                .is_none(),
            "clearance never waives the queued body's footprint"
        );
        assert_eq!(unit.patrol, patrol);
        assert_eq!(unit.path_index, 1);
        assert!(!unit.patrol_reverse);
        assert!(unit.path.len() <= 1, "only one pending clearance leg");
        backed_away |= frame.to_local(unit.position)[0] < -2.;
        released_junction |=
            unit.position.surface_distance_m(junction.position) >= junction.radius_m + 2. + 0.75;
        if !released_junction {
            assert!(
                world.living.units[&queued_id]
                    .position
                    .surface_distance_m(queued_position)
                    < 1e-6,
                "the waiting peer cannot enter before physical clearance"
            );
        }
        if let Some(saved) = &mut restored {
            advance(saved);
            assert_eq!(saved.living.units[&id], *unit);
            assert_eq!(saved.living.traffic, world.living.traffic);
            completed_clearance |= unit.path.is_empty();
        } else if !unit.path.is_empty() {
            let saved: SandboxState =
                serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
            saved.validate_with_catalog(&catalog).unwrap();
            restored = Some(saved);
        }
        if completed_clearance && frame.to_local(unit.position)[0] > 12. {
            break;
        }
    }
    assert!(restored.is_some(), "the outward maneuver must persist");
    assert!(backed_away && completed_clearance && released_junction);
    assert!(
        world.living.units[&queued_id]
            .position
            .surface_distance_m(queued_position)
            > 10.
    );
    assert!(
        frame.to_local(world.living.units[&id].position)[0] > 12.,
        "the patrol must resume forward progress after clearing the queue"
    );
}

#[test]
fn patrol_uses_supported_saved_detours_and_waits_when_both_sides_are_uncovered() {
    let (mut world, catalog, id, frame) = fixture(&[[0., -55.]], 0.);
    let target = world.living.units[&id].path[0];
    let patrol = vec![frame.origin, target];
    let unit = world.living.units.get_mut(&id).unwrap();
    unit.order = UnitOrder::Patrol;
    unit.patrol = patrol.clone();
    unit.path.clear();
    unit.path_index = 1;
    let mut parked = unit.clone();
    parked.id += 1;
    parked.order = UnitOrder::Idle;
    parked.position = frame.to_geo([0., 0., -16.]).unwrap();
    parked.home = parked.position;
    parked.patrol.clear();
    parked.path_index = 0;
    let parked_id = parked.id;
    let parked_position = parked.position;
    world.living.next_id += 1;
    world.living.units.insert(parked_id, parked);
    // A narrow supported road does not authorize a detour across unknown ground.
    for _ in 0..150 {
        world
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, p| {
                (frame.to_local(p)[0].abs() < 3.).then_some(p)
            })
            .unwrap();
        assert!(
            world.living.units[&id].path.len() <= 1,
            "supported straight clearance may persist, but an uncovered two-corner detour may not"
        );
    }
    assert!(frame.to_local(world.living.units[&id].position)[2] > -13.);
    assert_eq!(world.living.units[&id].path_index, 1);

    let mut saved: Option<SandboxState> = None;
    let mut arrived = false;
    for _ in 0..600 {
        let previous = world.living.units[&id].clone();
        let contacts = LivingContacts::new(&world).unwrap();
        world.advance_living(0.1, 0., &catalog).unwrap();
        let unit = &world.living.units[&id];
        assert!(previous.position.surface_distance_m(unit.position) <= 0.801);
        assert!(
            contacts
                .resolve(
                    id,
                    previous.position,
                    previous.heading_deg,
                    unit.position,
                    unit.heading_deg,
                    unit.velocity_enu_m_s
                )
                .unwrap()
                .contact
                .is_none()
        );
        assert_eq!(unit.patrol, patrol);
        assert!(
            unit.path.len() <= 2,
            "one bounded temporary avoidance maneuver"
        );
        assert_eq!(world.living.units[&parked_id].position, parked_position);
        if let Some(restored) = &mut saved {
            restored.advance_living(0.1, 0., &catalog).unwrap();
            assert_eq!(restored.living.units[&id], *unit);
            assert_eq!(restored.living.traffic, world.living.traffic);
        } else if unit.path.len() == 2 {
            saved = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if unit.position.surface_distance_m(target) < 3. {
            arrived = true;
            break;
        }
    }
    assert!(
        saved.is_some(),
        "the admitted two-corner maneuver persists across resume"
    );
    assert!(
        arrived,
        "the patrol physically passes the parked body within 60 seconds"
    );
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn crossing_walkers_and_courier_keep_their_avoidance_across_save_resume() {
    let (mut world, catalog, initial_id, frame) =
        fixture(&[[5.966, -4.236], [20., -20.]], 348.48563);
    let mut courier = world.living.units.remove(&initial_id).unwrap();
    courier.id += 2;
    courier.role = UnitRole::Hauler;
    let id = courier.id;
    let destination = *courier.path.last().unwrap();
    for (offset, position) in [[-0.946, -2.20], [-1.159, -2.151]].into_iter().enumerate() {
        let mut person = courier.clone();
        person.id = initial_id + offset as u64;
        person.kind = ActorKind::Human;
        person.role = UnitRole::Citizen;
        person.position = frame.to_geo([position[0], 0., position[1]]).unwrap();
        person.home = person.position;
        person.order = UnitOrder::Patrol;
        person.path.clear();
        person.path_index = 1;
        person.patrol = vec![person.position, frame.to_geo([0.84, 0., 4.484]).unwrap()];
        world.living.units.insert(person.id, person);
    }
    world.living.next_id = id + 1;
    world.living.units.insert(id, courier);
    let patrols: Vec<_> = world
        .living
        .units
        .values()
        .filter(|u| u.kind == ActorKind::Human)
        .map(|u| (u.id, u.patrol.clone(), u.position))
        .collect();
    let mut resumed_clearance = false;
    for _ in 0..1200 {
        let before = world.living.units[&id].position;
        world.advance_living(0.1, 0., &catalog).unwrap();
        let unit = &world.living.units[&id];
        assert!(before.surface_distance_m(unit.position) < 0.61);
        let arrived = matches!(unit.order, UnitOrder::Idle);
        let walking_detour = world
            .living
            .units
            .values()
            .any(|u| u.kind == ActorKind::Human && !u.path.is_empty());
        if !resumed_clearance && walking_detour {
            for (person, patrol, _) in &patrols {
                assert_eq!(world.living.units[person].patrol, *patrol);
                assert_eq!(world.living.units[person].path_index, 1);
                assert!(!world.living.units[person].patrol_reverse);
            }
            world = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
            world.validate_with_catalog(&catalog).unwrap();
            resumed_clearance = true;
        }
        if arrived {
            break;
        }
    }
    assert!(resumed_clearance, "clearance intent was never saved");
    for (person, patrol, start) in &patrols {
        assert_eq!(world.living.units[person].patrol, *patrol);
        assert!(
            world.living.units[person]
                .position
                .surface_distance_m(*start)
                > 2.
        );
    }
    let unit = &world.living.units[&id];
    assert!(matches!(unit.order, UnitOrder::Idle), "{unit:?}");
    assert!(unit.position.surface_distance_m(destination) < 1.5);
}

#[test]
fn patrol_detours_do_not_expand_the_persisted_patrol_cursor_bounds() {
    let (mut world, catalog, id, _) = fixture(&[[0., -30.]], 0.);
    let unit = world.living.units.get_mut(&id).unwrap();
    unit.kind = ActorKind::Human;
    unit.role = UnitRole::Citizen;
    unit.order = UnitOrder::Patrol;
    unit.patrol = vec![unit.position];
    unit.path = vec![unit.position, unit.position];
    unit.path_index = 2;
    assert!(world.validate_with_catalog(&catalog).is_err());
    world.living.units.get_mut(&id).unwrap().path_index = 0;
    world.validate_with_catalog(&catalog).unwrap();
    for remaining in [1, 0] {
        world.advance_living(0.1, 0., &catalog).unwrap();
        world.validate_with_catalog(&catalog).unwrap();
        assert_eq!(world.living.units[&id].path.len(), remaining);
        assert_eq!(world.living.units[&id].path_index, 0);
    }
}

#[test]
fn stationary_avatar_corner_retains_clearance_in_both_directions() {
    // Captured early native junction poses: the first needs forward room
    // before reversing, and the second needs rear room before a forward turn.
    for (start, heading, points) in [
        (
            [0.67728877, 2.35010145],
            132.51755,
            vec![
                [-4.8082438, -0.14681039],
                [-0.54432268, -2.74503939],
                [30., -2.74503939],
            ],
        ),
        (
            [-2.35910542, 0.02912159],
            79.12815,
            vec![[-0.54432268, -2.74503939], [30., -2.74503939]],
        ),
    ] {
        let (mut world, catalog, id, frame) = fixture(&points, heading);
        let destination = *world.living.units[&id].path.last().unwrap();
        let original_path_len = world.living.units[&id].path.len();
        let mut person = world.living.units[&id].clone();
        person.id += 1;
        person.name = "Stationary avatar corner".into();
        person.kind = ActorKind::Human;
        person.role = UnitRole::Citizen;
        person.order = UnitOrder::Idle;
        person.path.clear();
        let avatar_id = person.id;
        let avatar_position = person.position;
        world.living.next_id += 1;
        world.living.units.insert(person.id, person);
        let unit = world.living.units.get_mut(&id).unwrap();
        unit.role = UnitRole::Hauler;
        unit.position = frame.to_geo([start[0], 0., start[1]]).unwrap();
        let mut restored: Option<SandboxState> = None;
        for _ in 0..1200 {
            let previous = world.living.units[&id].clone();
            let contacts = LivingContacts::new(&world).unwrap();
            world.advance_living(0.05, 0., &catalog).unwrap();
            let unit = &world.living.units[&id];
            assert!(previous.position.surface_distance_m(unit.position) <= 0.41);
            assert!(
                contacts
                    .resolve(
                        id,
                        previous.position,
                        previous.heading_deg,
                        unit.position,
                        unit.heading_deg,
                        unit.velocity_enu_m_s,
                    )
                    .unwrap()
                    .contact
                    .is_none(),
                "a stationary avatar remains a full contact body"
            );
            assert_eq!(world.living.units[&avatar_id].position, avatar_position);
            if let Some(saved) = &mut restored {
                saved.advance_living(0.05, 0., &catalog).unwrap();
                assert_eq!(saved.living.units[&id], *unit);
                assert_eq!(saved.living.traffic, world.living.traffic);
            } else if unit.path.len() == original_path_len + 1 {
                let saved: SandboxState =
                    serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
                saved.validate_with_catalog(&catalog).unwrap();
                restored = Some(saved);
            }
            if matches!(unit.order, UnitOrder::Idle) {
                break;
            }
        }
        assert!(restored.is_some(), "the admitted clearance must persist");
        let unit = &world.living.units[&id];
        assert!(matches!(unit.order, UnitOrder::Idle), "{unit:?}");
        assert!(unit.position.surface_distance_m(destination) < 1.5);
        assert_eq!(world.living.traffic.vehicle_dynamics[&id].speed_m_s, 0.);
    }
}

#[test]
fn a_legacy_avatar_overlap_can_clear_before_turning_without_teleporting() {
    let (mut world, catalog, id, _) = fixture(&[[30., 0.]], 0.);
    let mut person = world.living.units[&id].clone();
    person.id += 1;
    person.name = "Waiting person".into();
    person.kind = ActorKind::Human;
    person.role = UnitRole::Citizen;
    person.order = UnitOrder::Idle;
    person.path.clear();
    let origin = person.position;
    world.living.next_id += 1;
    world.living.units.insert(person.id, person);
    let mut previous = world.living.units[&id].clone();
    for _ in 0..1200 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        let unit = &world.living.units[&id];
        assert!(previous.position.surface_distance_m(unit.position) <= 0.81);
        let turn =
            (f64::from(unit.heading_deg - previous.heading_deg) + 540.).rem_euclid(360.) - 180.;
        assert!(turn.abs() <= 16.05);
        previous = unit.clone();
        if matches!(unit.order, UnitOrder::Idle) {
            break;
        }
    }
    assert!(matches!(previous.order, UnitOrder::Idle), "{previous:?}");
    assert!(previous.position.surface_distance_m(origin) > 28.);
}

#[test]
fn cold_npc_handling_survives_active_ticks_and_complete_orphans_are_rejected() {
    let (mut world, catalog, id, _) = fixture(&[[0., -15.], [30., -15.]], 0.);
    for _ in 0..50 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    let body = world.living.units[&id].clone();
    let handling = world.living.traffic.vehicle_dynamics[&id];
    let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
    let mut active = partition
        .working_set(&Default::default(), &catalog)
        .unwrap();
    assert!(active.state.is_partial());
    assert!(!active.state.living.units.contains_key(&id));
    for _ in 0..20 {
        active.state.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert_eq!(active.state.living.traffic.vehicle_dynamics[&id], handling);
    let restored = partition
        .merge_active(&active.state, &active.active_regions, &catalog)
        .unwrap()
        .assemble_all(&catalog)
        .unwrap();
    assert_eq!(restored.living.units[&id], body);
    assert_eq!(restored.living.traffic.vehicle_dynamics[&id], handling);
    let mut forged = restored;
    forged.living.next_id += 1;
    forged
        .living
        .traffic
        .vehicle_dynamics
        .insert(id + 1, Default::default());
    assert!(forged.validate_with_catalog(&catalog).is_err());
}

#[test]
fn registered_autonomy_uses_real_cargo_mass_and_accepted_curve_distance() {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let mut empty = SandboxState::default();
    empty.add_player(1, origin).unwrap();
    empty.seed_living_demo(origin, &[], &catalog).unwrap();
    empty.ensure_transport_demo(&catalog).unwrap();
    let id = *empty.transport.vehicles.keys().next().unwrap();
    let start = frame.to_geo([800., 0., -800.]).unwrap();
    let end = LocalFrame::new(start).to_geo([0., 0., -200.]).unwrap();
    let unit = empty.living.units.get_mut(&id).unwrap();
    unit.position = start;
    unit.heading_deg = 0.;
    unit.order = UnitOrder::Move { destination: end };
    unit.path = vec![end];
    unit.path_index = 0;
    let vehicle = empty.transport.vehicles.get_mut(&id).unwrap();
    vehicle.parking_brake = false;
    vehicle.autopilot = true;
    let mut loaded = empty.clone();
    let capacity = loaded.transport.vehicles[&id].model.spec().cargo_capacity;
    loaded
        .transport
        .vehicles
        .get_mut(&id)
        .unwrap()
        .cargo
        .insert("wood".into(), capacity);
    for _ in 0..60 {
        empty.advance_living(0.1, 0., &catalog).unwrap();
        loaded.advance_living(0.1, 0., &catalog).unwrap();
    }
    let empty_vehicle = &empty.transport.vehicles[&id];
    let loaded_vehicle = &loaded.transport.vehicles[&id];
    assert!(empty_vehicle.odometer_m > 10.);
    assert!(loaded_vehicle.odometer_m + 0.05 < empty_vehicle.odometer_m);
    assert!(
        (empty_vehicle.odometer_m - start.surface_distance_m(empty.living.units[&id].position))
            .abs()
            < 0.01
    );
    assert!(empty_vehicle.dynamics.speed_m_s > 0.);
    assert!(
        !empty.living.traffic.vehicle_dynamics.contains_key(&id),
        "registered handling must have one authority"
    );
}
