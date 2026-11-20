use std::cell::Cell;
use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

const DRIVER: u64 = 1_000_000;

fn parked_cluster() -> (SandboxState, Catalog, LocalFrame, GeoPosition) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let destination = frame.to_geo([0., 0., -60.]).unwrap();
    let mut world = SandboxState::default();
    world.living = LivingWorld {
        enabled: true,
        origin: Some(origin),
        next_id: DRIVER + 4,
        ..Default::default()
    };
    let driver = LivingUnit {
        id: DRIVER,
        name: "Cluster approach".into(),
        kind: ActorKind::Vehicle,
        owner: None,
        role: UnitRole::Traffic,
        position: origin,
        heading_deg: 0.,
        velocity_enu_m_s: [0.; 3],
        needs: Needs::default(),
        order: UnitOrder::Move { destination },
        home: origin,
        path: vec![destination],
        path_index: 0,
        patrol: vec![],
        patrol_reverse: false,
        pocket_money: 100,
        supplies: Inventory::from([("wood".into(), 12)]),
    };
    world.living.units.insert(DRIVER, driver.clone());
    for (index, east) in [0., 7., -7.].into_iter().enumerate() {
        let mut parked = driver.clone();
        parked.id = DRIVER + 1 + index as u64;
        parked.name = format!("Parked obstruction {index}");
        parked.position = frame.to_geo([east, 0., -16.]).unwrap();
        parked.home = parked.position;
        parked.order = UnitOrder::Idle;
        parked.path.clear();
        world.living.units.insert(parked.id, parked);
    }
    (world, Catalog::builtin(), frame, destination)
}

fn assert_physical_step(before: &SandboxState, world: &SandboxState) {
    let old = &before.living.units[&DRIVER];
    let unit = &world.living.units[&DRIVER];
    assert!(old.position.surface_distance_m(unit.position) <= 0.801);
    let turn = (f64::from(unit.heading_deg - old.heading_deg) + 540.).rem_euclid(360.) - 180.;
    assert!(turn.abs() <= 16.05, "heading snapped by {turn}");
    assert!(unit.path.len() <= 129);
    assert!(
        LivingContacts::new(before)
            .unwrap()
            .resolve(
                DRIVER,
                old.position,
                old.heading_deg,
                unit.position,
                unit.heading_deg,
                unit.velocity_enu_m_s,
            )
            .unwrap()
            .contact
            .is_none()
    );
    for (id, prior) in &before.living.units {
        let current = &world.living.units[id];
        assert_eq!(current.pocket_money, prior.pocket_money);
        assert_eq!(current.supplies, prior.supplies);
        if *id != DRIVER && matches!(prior.order, UnitOrder::Idle) {
            assert_eq!(current.position, prior.position);
            assert_eq!(current.heading_deg, prior.heading_deg);
        }
    }
}

#[test]
fn autonomous_vehicle_passes_a_cluster_without_moving_parked_bodies_or_losing_its_load() {
    let (mut world, catalog, frame, destination) = parked_cluster();
    world.validate_with_catalog(&catalog).unwrap();
    let mut resumed: Option<SandboxState> = None;
    let mut travelled = 0.;
    let mut farthest_side: f64 = 0.;
    let mut max_step_m: f64 = 0.;
    let mut max_heading_delta_deg: f64 = 0.;
    let mut max_path_waypoints = 0;
    let mut max_source_queries = 0;
    let mut total_source_queries = 0;
    let mut resumed_steps = 0;
    let source_queries = Cell::new(0_usize);
    let support = |_, _, position| {
        source_queries.set(source_queries.get() + 1);
        let local = frame.to_local(position);
        (local[0].abs() <= 40. && (-80. ..=40.).contains(&local[2])).then_some(position)
    };
    for step in 0..1200 {
        let before = world.clone();
        source_queries.set(0);
        world
            .advance_living_with_environment(0.1, 0., &catalog, support)
            .unwrap();
        max_source_queries = max_source_queries.max(source_queries.get());
        total_source_queries += source_queries.get();
        assert!(
            source_queries.get() <= 16_384,
            "finite fixture source-work ceiling"
        );
        assert_physical_step(&before, &world);
        let unit = &world.living.units[&DRIVER];
        let step_m = before.living.units[&DRIVER]
            .position
            .surface_distance_m(unit.position);
        travelled += step_m;
        max_step_m = max_step_m.max(step_m);
        let turn = (f64::from(unit.heading_deg - before.living.units[&DRIVER].heading_deg) + 540.)
            .rem_euclid(360.)
            - 180.;
        max_heading_delta_deg = max_heading_delta_deg.max(turn.abs());
        max_path_waypoints = max_path_waypoints.max(unit.path.len());
        farthest_side = farthest_side.max(frame.to_local(unit.position)[0].abs());
        if let Some(saved) = &mut resumed {
            saved
                .advance_living_with_environment(0.1, 0., &catalog, support)
                .unwrap();
            assert_eq!(*saved, world, "exact resumed continuation at step {step}");
            resumed_steps += 1;
        } else if unit.path.len() > 1 {
            resumed = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
            resumed
                .as_ref()
                .unwrap()
                .validate_with_catalog(&catalog)
                .unwrap();
        }
        if matches!(unit.order, UnitOrder::Idle) {
            assert!(unit.position.surface_distance_m(destination) < 1.5);
            assert!(resumed.is_some(), "the admitted maneuver was saved");
            assert!(
                farthest_side > 10.,
                "the car physically passed outside the cluster"
            );
            assert!(travelled >= 60. && travelled < 160.);
            world.validate_with_catalog(&catalog).unwrap();
            println!(
                "NAVIGATION_CLUSTER_REPORT {}",
                serde_json::json!({
                    "case": "three_parked_bodies",
                    "obstacle_count": 3,
                    "physical_step_seconds": 0.1,
                    "physical_steps": step + 1,
                    "elapsed_physical_seconds": (step + 1) as f64 * 0.1,
                    "elapsed_limit_seconds": 120,
                    "distance_travelled_m": travelled,
                    "remaining_goal_distance_m": unit.position.surface_distance_m(destination),
                    "farthest_lateral_distance_m": farthest_side,
                    "max_step_distance_m": max_step_m,
                    "max_heading_delta_deg": max_heading_delta_deg,
                    "max_path_waypoints": max_path_waypoints,
                    "source_queries": total_source_queries,
                    "max_source_queries_per_step": max_source_queries,
                    "fixture_source_query_ceiling_per_step": 16_384,
                    "exact_resumed_steps": resumed_steps,
                    "scope": "Authored flat SDK fixture; no world coverage, hardware performance, company profitability or general traffic-flow guarantee.",
                    "verification": {
                        "physical_arrival": true,
                        "bounded_steps_and_steering": true,
                        "supported_complete_envelopes": true,
                        "body_sweeps_clear": true,
                        "parked_bodies_unchanged": true,
                        "goods_and_money_conserved": true,
                        "source_work_bounded": true,
                        "path_bound_retained": true,
                        "exact_save_resume": resumed_steps > 0,
                        "complete_state_valid": true
                    }
                })
            );
            return;
        }
    }
    panic!(
        "cluster approach did not finish: local={:?}, route={:?}, dynamics={:?}",
        frame.to_local(world.living.units[&DRIVER].position),
        world.living.units[&DRIVER].path,
        world.living.traffic.vehicle_dynamics[&DRIVER],
    );
}

#[test]
fn uncovered_cluster_sides_hold_the_original_order_until_support_returns() {
    let (mut world, catalog, frame, destination) = parked_cluster();
    for _ in 0..150 {
        let before = world.clone();
        world
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, position| {
                (frame.to_local(position)[0].abs() < 3.).then_some(position)
            })
            .unwrap();
        assert_physical_step(&before, &world);
        let unit = &world.living.units[&DRIVER];
        assert!(frame.to_local(unit.position)[0].abs() < 1.9);
        assert!(frame.to_local(unit.position)[2] > -13.);
        assert_eq!(unit.order, UnitOrder::Move { destination });
        assert_eq!(unit.path.last(), Some(&destination));
    }
    let mut restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    for _ in 0..1200 {
        let before = world.clone();
        world.advance_living(0.1, 0., &catalog).unwrap();
        restored.advance_living(0.1, 0., &catalog).unwrap();
        assert_physical_step(&before, &world);
        assert_eq!(world, restored);
        let unit = &world.living.units[&DRIVER];
        if matches!(unit.order, UnitOrder::Idle) {
            assert!(unit.position.surface_distance_m(destination) < 1.5);
            return;
        }
    }
    panic!("restored support must permit the original cluster approach to finish");
}

#[test]
fn moving_obstruction_keeps_its_physical_priority_and_clears_without_a_static_detour() {
    let (mut world, catalog, frame, destination) = parked_cluster();
    world
        .living
        .units
        .retain(|id, _| *id == DRIVER || *id == DRIVER + 1);
    let crossing = world.living.units.get_mut(&(DRIVER + 1)).unwrap();
    let crossing_goal = frame.to_geo([24., 0., -16.]).unwrap();
    crossing.order = UnitOrder::Move {
        destination: crossing_goal,
    };
    crossing.path = vec![crossing_goal];
    crossing.heading_deg = 90.;
    crossing.velocity_enu_m_s = [1.8, 0., 0.];
    let mut saved: Option<SandboxState> = None;
    let mut crossing_was_moving = false;
    for step in 0..900 {
        let before = world.clone();
        world.advance_living(0.1, 0., &catalog).unwrap();
        assert_physical_step(&before, &world);
        let car = &world.living.units[&DRIVER];
        let crossing = &world.living.units[&(DRIVER + 1)];
        crossing_was_moving |= crossing.position.surface_distance_m(crossing.home) > 3.;
        assert!(world.spawn_obstruction(car).unwrap().is_none());
        assert!(
            car.path.len() <= 1,
            "a moving body must not seed a static detour"
        );
        if let Some(saved) = &mut saved {
            saved.advance_living(0.1, 0., &catalog).unwrap();
            assert_eq!(*saved, world);
        } else if step == 15 {
            saved = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(car.order, UnitOrder::Idle) {
            assert!(crossing_was_moving && saved.is_some());
            assert!(car.position.surface_distance_m(destination) < 1.5);
            world.validate_with_catalog(&catalog).unwrap();
            return;
        }
    }
    panic!("the moving body cleared but the original driver did not continue");
}

#[test]
fn cluster_uses_the_supported_opposite_side_when_the_first_side_is_missing() {
    let (mut world, catalog, frame, destination) = parked_cluster();
    let mut farthest_west: f64 = 0.;
    for _ in 0..900 {
        let before = world.clone();
        world
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, position| {
                let east = frame.to_local(position)[0];
                (-40. ..=3.).contains(&east).then_some(position)
            })
            .unwrap();
        assert_physical_step(&before, &world);
        let unit = &world.living.units[&DRIVER];
        let east = frame.to_local(unit.position)[0];
        farthest_west = farthest_west.min(east);
        assert!(east < 1.9);
        if matches!(unit.order, UnitOrder::Idle) {
            assert!(unit.position.surface_distance_m(destination) < 1.5);
            assert!(farthest_west < -10.);
            return;
        }
    }
    panic!("the supported opposite cluster flank must remain usable");
}

#[test]
fn patrol_cluster_maneuver_keeps_its_circuit_and_exact_saved_continuation() {
    let (mut world, catalog, frame, destination) = parked_cluster();
    let circuit = vec![frame.origin, destination];
    let unit = world.living.units.get_mut(&DRIVER).unwrap();
    unit.order = UnitOrder::Patrol;
    unit.path.clear();
    unit.patrol = circuit.clone();
    unit.path_index = 1;
    let mut restored: Option<SandboxState> = None;
    for _ in 0..900 {
        let before = world.clone();
        world.advance_living(0.1, 0., &catalog).unwrap();
        assert_physical_step(&before, &world);
        let unit = &world.living.units[&DRIVER];
        assert_eq!(unit.order, UnitOrder::Patrol);
        assert_eq!(unit.patrol, circuit);
        assert!(unit.path.len() <= 2);
        if !unit.path.is_empty() {
            assert_eq!(unit.path_index, 1);
            assert!(!unit.patrol_reverse);
        }
        if let Some(saved) = &mut restored {
            saved.advance_living(0.1, 0., &catalog).unwrap();
            assert_eq!(*saved, world);
        } else if unit.path.len() == 2 {
            restored = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
            restored
                .as_ref()
                .unwrap()
                .validate_with_catalog(&catalog)
                .unwrap();
        }
        if unit.position.surface_distance_m(destination) < 3. {
            assert!(restored.is_some());
            return;
        }
    }
    panic!("the patrol must physically pass the same parked cluster");
}
