//! Bounded authored geometry cases; these are not general road-flow benchmarks.
use std::cell::Cell;
use terra_core::{ActorKind, GeoPosition, LocalFrame, VehicleGroundSample, VehicleSurface};
use terra_sandbox::*;

const DRIVER: u64 = 1_000_000;

#[derive(Default)]
struct RunMetrics {
    updates: usize,
    queries: usize,
    max_queries: usize,
    travelled_m: f64,
    max_step_m: f64,
    max_heading_deg: f64,
    max_path_points: usize,
    resumed_steps: usize,
}

impl RunMetrics {
    fn advance(
        &mut self,
        world: &mut SandboxState,
        catalog: &Catalog,
        support: impl Fn(ActorKind, GeoPosition, GeoPosition) -> Option<GeoPosition>,
    ) {
        let before = world.living.units[&DRIVER].clone();
        let queries = Cell::new(0);
        world
            .advance_living_with_environment(0.1, 0., catalog, |kind, from, to| {
                queries.set(queries.get() + 1);
                support(kind, from, to)
            })
            .unwrap();
        let next = &world.living.units[&DRIVER];
        let distance = before.position.surface_distance_m(next.position);
        let turn =
            (f64::from(next.heading_deg - before.heading_deg) + 540.).rem_euclid(360.) - 180.;
        self.updates += 1;
        self.queries += queries.get();
        self.max_queries = self.max_queries.max(queries.get());
        self.travelled_m += distance;
        self.max_step_m = self.max_step_m.max(distance);
        self.max_heading_deg = self.max_heading_deg.max(turn.abs());
        self.max_path_points = self.max_path_points.max(next.path.len());
        assert!(queries.get() <= 32_768, "bounded three-body fixture work");
    }

    fn emit(&self, mut result: serde_json::Value) {
        result["scope"] = serde_json::json!(
            "Bounded authored flat SDK geometry, 100 ms physical steps and frozen calendar. No general road-flow, world-coverage, hardware-capacity or profitability claim."
        );
        result["metrics"] = serde_json::json!({
            "physical_steps": self.updates,
            "elapsed_physical_seconds": self.updates as f64 * 0.1,
            "driver_travelled_m": self.travelled_m,
            "driver_max_step_m": self.max_step_m,
            "driver_max_heading_delta_deg": self.max_heading_deg,
            "max_path_waypoints": self.max_path_points,
            "source_queries": self.queries,
            "max_source_queries_per_update": self.max_queries,
            "exact_resumed_steps": self.resumed_steps
        });
        result["limits"] = serde_json::json!({
            "physical_step_seconds": 0.1, "driver_max_step_m": 0.801,
            "driver_max_heading_delta_deg": 16.05, "max_path_waypoints": 129,
            "fixture_source_queries_per_update": 32_768,
            "extra_corridor_admissions_per_vehicle_update": 12
        });
        result["verification"] = serde_json::json!({
            "physical_arrival": true, "bounded_motion": true,
            "supported_complete_envelopes": true, "body_sweeps_clear": true,
            "goods_and_money_conserved": true, "source_work_bounded": true,
            "complete_state_valid": true, "exact_save_resume": self.resumed_steps > 0
        });
        println!("NAVIGATION_GEOMETRY_REPORT {result}");
    }
}

fn corridor() -> (SandboxState, Catalog, LocalFrame, GeoPosition) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let destination = frame.to_geo([0., 0., -60.]).unwrap();
    let mut world = SandboxState::default();
    world.living = LivingWorld {
        enabled: true,
        origin: Some(origin),
        next_id: DRIVER + 2,
        ..Default::default()
    };
    let driver = LivingUnit {
        id: DRIVER,
        name: "Narrow corridor driver".into(),
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
    let mut parked = driver.clone();
    parked.id += 1;
    parked.name = "Parked corridor obstruction".into();
    parked.position = frame.to_geo([0., 0., -16.]).unwrap();
    parked.home = parked.position;
    parked.order = UnitOrder::Idle;
    parked.path.clear();
    world.living.units.insert(DRIVER, driver);
    world.living.units.insert(parked.id, parked);
    (world, Catalog::builtin(), frame, destination)
}

fn assert_step(before: &SandboxState, after: &SandboxState, frame: LocalFrame, half_width: f64) {
    let old = &before.living.units[&DRIVER];
    let next = &after.living.units[&DRIVER];
    assert!(old.position.surface_distance_m(next.position) <= 0.801);
    let turn = (f64::from(next.heading_deg - old.heading_deg) + 540.).rem_euclid(360.) - 180.;
    assert!(turn.abs() <= 16.05);
    assert!(next.path.len() <= 129);
    let mut contacts = LivingContacts::new(before).unwrap();
    for (&id, old) in &before.living.units {
        let next = &after.living.units[&id];
        assert!(old.position.surface_distance_m(next.position) <= 0.801);
        let turn = (f64::from(next.heading_deg - old.heading_deg) + 540.).rem_euclid(360.) - 180.;
        assert!(turn.abs() <= 16.05);
        assert!(
            contacts
                .resolve(
                    id,
                    old.position,
                    old.heading_deg,
                    next.position,
                    next.heading_deg,
                    next.velocity_enu_m_s
                )
                .unwrap()
                .contact
                .is_none()
        );
        contacts
            .update(id, next.position, next.heading_deg, next.velocity_enu_m_s)
            .unwrap();
    }
    for p in terra_core::vehicle_support_points(
        next.position,
        f64::from(next.heading_deg),
        [2.2, 1.8, 3.9],
    )
    .unwrap()
    {
        assert!(frame.to_local(p)[0].abs() <= half_width + 1e-6);
    }
    for (&id, previous) in &before.living.units {
        let current = &after.living.units[&id];
        assert_eq!(previous.pocket_money, current.pocket_money);
        assert_eq!(previous.supplies, current.supplies);
        if matches!(previous.order, UnitOrder::Idle) {
            assert_eq!(previous.position, current.position);
            assert_eq!(previous.heading_deg, current.heading_deg);
        }
    }
}

#[test]
fn a_supported_narrow_corridor_passes_a_parked_body_without_leaving_its_envelope() {
    let (mut world, catalog, frame, destination) = corridor();
    let mut metrics = RunMetrics::default();
    let half_width = 4.75;
    let queries = Cell::new(0_usize);
    let support = |_, _, p| {
        queries.set(queries.get() + 1);
        let local = frame.to_local(p);
        (local[0].abs() <= half_width && (-80. ..=20.).contains(&local[2])).then_some(p)
    };
    let mut resumed: Option<SandboxState> = None;
    let mut total_queries = 0;
    let mut max_queries = 0;
    let mut travelled = 0.;
    let mut max_lateral: f64 = 0.;
    for step in 0..1200 {
        let before = world.clone();
        queries.set(0);
        metrics.advance(&mut world, &catalog, support);
        total_queries += queries.get();
        max_queries = max_queries.max(queries.get());
        assert!(queries.get() <= 16_384);
        assert_step(&before, &world, frame, half_width);
        let unit = &world.living.units[&DRIVER];
        travelled += before.living.units[&DRIVER]
            .position
            .surface_distance_m(unit.position);
        max_lateral = max_lateral.max(frame.to_local(unit.position)[0].abs());
        if let Some(saved) = &mut resumed {
            saved
                .advance_living_with_environment(0.1, 0., &catalog, support)
                .unwrap();
            assert_eq!(*saved, world);
            metrics.resumed_steps += 1;
        } else if unit.path.len() > 1 {
            resumed = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(unit.order, UnitOrder::Idle) {
            assert!(unit.position.surface_distance_m(destination) < 1.5);
            assert!(resumed.is_some());
            world.validate_with_catalog(&catalog).unwrap();
            metrics.emit(serde_json::json!({
                "case": "parked_body_narrow_corridor", "support_width_m": 2. * half_width,
                "physical_seconds": (step + 1) as f64 * 0.1,
                "travelled_m": travelled, "max_lateral_m": max_lateral,
                "source_queries": total_queries, "max_source_queries_per_step": max_queries,
                "arrived": true, "goods_money_preserved": true, "exact_resume": true,
                "supported_envelopes": true, "swept_contacts_clear": true
            }));
            return;
        }
    }
    println!(
        "NAVIGATION_GEOMETRY_REPORT {}",
        serde_json::json!({
            "case": "parked_body_narrow_corridor", "support_width_m": 2. * half_width,
            "physical_seconds": 120., "travelled_m": travelled,
            "remaining_m": world.living.units[&DRIVER].position.surface_distance_m(destination),
            "max_lateral_m": max_lateral, "source_queries": total_queries,
            "max_source_queries_per_step": max_queries, "arrived": false
        })
    );
    panic!(
        "narrow route did not progress: {:?}",
        world.living.units[&DRIVER]
    );
}

#[test]
fn a_long_unavailable_flank_keeps_value_and_replans_after_support_returns() {
    let (mut world, catalog, frame, destination) = corridor();
    let mut metrics = RunMetrics::default();
    // This five-metre strip has no supported passing envelope. Repeated
    // attempts must not spend the route capacity on already-consumed retreats.
    // Restoring the same narrow passing strip now also requires a fully
    // admitted reverse approach; there is no wider turning area to borrow.
    for _ in 0..6000 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 2.5 && (-80. ..=20.).contains(&local[2])).then_some(p)
        });
        assert_step(&before, &world, frame, 2.5);
        assert!(
            matches!(world.living.units[&DRIVER].order, UnitOrder::Move { destination: p } if p == destination)
        );
    }
    let mut saved: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    saved.validate_with_catalog(&catalog).unwrap();
    for step in 0..1200 {
        let before = world.clone();
        let support = |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 4.75 && (-80. ..=20.).contains(&local[2])).then_some(p)
        };
        metrics.advance(&mut world, &catalog, support);
        saved
            .advance_living_with_environment(0.1, 0., &catalog, support)
            .unwrap();
        assert_eq!(saved, world);
        metrics.resumed_steps += 1;
        assert_step(&before, &world, frame, 4.75);
        if matches!(world.living.units[&DRIVER].order, UnitOrder::Idle) {
            assert!(
                world.living.units[&DRIVER]
                    .position
                    .surface_distance_m(destination)
                    < 1.5
            );
            world.validate_with_catalog(&catalog).unwrap();
            metrics.emit(serde_json::json!({
                "case": "flank_unavailable_then_restored", "unsupported_seconds": 600,
                "unavailable_width_m": 5., "restored_width_m": 9.5,
                "seconds_after_restoration": (step + 1) as f64 * 0.1,
                "arrived": true, "goods_money_preserved": true, "exact_resume": true,
                "supported_envelopes": true, "swept_contacts_clear": true
            }));
            return;
        }
    }
    panic!(
        "the restored supported flank did not recover the same order: {:?}",
        world.living.units[&DRIVER]
    );
}

#[test]
fn a_missing_reconnection_rejects_partial_detours_until_the_complete_route_returns() {
    let (mut world, catalog, frame, destination) = corridor();
    let mut metrics = RunMetrics::default();
    // Both wide corners fit, but the third leg crosses an unavailable strip.
    for _ in 0..20 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 40.
                && (-80. ..=20.).contains(&local[2])
                && !(-38. ..=-32.).contains(&local[2]))
            .then_some(p)
        });
        assert_step(&before, &world, frame, 40.);
        assert!(
            world.living.units[&DRIVER]
                .path
                .iter()
                .all(|p| frame.to_local(*p)[0].abs() < 0.0001),
            "the missing final corridor must not admit a two-leg detour"
        );
    }
    let mut saved: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    for step in 0..1200 {
        let before = world.clone();
        let support = |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 40. && (-80. ..=20.).contains(&local[2])).then_some(p)
        };
        metrics.advance(&mut world, &catalog, support);
        saved
            .advance_living_with_environment(0.1, 0., &catalog, support)
            .unwrap();
        assert_eq!(saved, world);
        metrics.resumed_steps += 1;
        assert_step(&before, &world, frame, 40.);
        if matches!(world.living.units[&DRIVER].order, UnitOrder::Idle) {
            assert!(
                world.living.units[&DRIVER]
                    .position
                    .surface_distance_m(destination)
                    < 1.5
            );
            world.validate_with_catalog(&catalog).unwrap();
            metrics.emit(serde_json::json!({
                "case": "unavailable_reconnection", "unavailable_strip_m": [-38, -32],
                "unsupported_seconds": 2., "seconds_after_restoration": (step + 1) as f64 * 0.1,
                "partial_proposals_rejected": true, "arrived": true,
                "goods_money_preserved": true, "exact_resume": true,
                "supported_envelopes": true, "swept_contacts_clear": true
            }));
            return;
        }
    }
    panic!("restored reconnection did not admit the retained route");
}

#[test]
fn two_moving_crossings_clear_physically_before_the_original_route_continues() {
    let (mut world, catalog, frame, destination) = corridor();
    let mut metrics = RunMetrics::default();
    world.living.units.remove(&(DRIVER + 1));
    let mut crossing_ids = vec![];
    for (index, (east, north, direction)) in
        [(-12., -16., 1.), (12., -23., -1.)].into_iter().enumerate()
    {
        let mut crossing = world.living.units[&DRIVER].clone();
        crossing.id += 1 + index as u64;
        crossing.position = frame.to_geo([east, 0., north]).unwrap();
        crossing.home = crossing.position;
        crossing.heading_deg = if direction > 0. { 90. } else { 270. };
        crossing.velocity_enu_m_s = [direction, 0., 0.];
        let destination = frame.to_geo([-east, 0., north]).unwrap();
        crossing.order = UnitOrder::Move { destination };
        crossing.path = vec![destination];
        crossing_ids.push(crossing.id);
        world.living.units.insert(crossing.id, crossing);
    }
    world.living.next_id = DRIVER + 3;
    let mut saved: Option<SandboxState> = None;
    for step in 0..1200 {
        let before = world.clone();
        let support = |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 20. && (-80. ..=20.).contains(&local[2])).then_some(p)
        };
        metrics.advance(&mut world, &catalog, support);
        assert_step(&before, &world, frame, 20.);
        for id in &crossing_ids {
            assert!(
                before.living.units[id]
                    .position
                    .surface_distance_m(world.living.units[id].position)
                    <= 0.801
            );
        }
        assert!(
            world
                .spawn_obstruction(&world.living.units[&DRIVER])
                .unwrap()
                .is_none()
        );
        if let Some(copy) = &mut saved {
            copy.advance_living_with_environment(0.1, 0., &catalog, support)
                .unwrap();
            assert_eq!(*copy, world);
            metrics.resumed_steps += 1;
        } else if step == 10 {
            saved = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(world.living.units[&DRIVER].order, UnitOrder::Idle) {
            assert!(
                world.living.units[&DRIVER]
                    .position
                    .surface_distance_m(destination)
                    < 1.5
            );
            assert!(crossing_ids.iter().all(|id| {
                world.living.units[id]
                    .position
                    .surface_distance_m(world.living.units[id].home)
                    > 20.
            }));
            assert!(saved.is_some());
            world.validate_with_catalog(&catalog).unwrap();
            metrics.emit(serde_json::json!({
                "case": "two_moving_crossings", "physical_seconds": (step + 1) as f64 * 0.1,
                "crossings_physically_cleared": 2, "arrived": true,
                "goods_money_preserved": true, "exact_resume": true,
                "supported_envelopes": true, "swept_contacts_clear": true
            }));
            return;
        }
    }
    panic!("moving crossings cleared but the original route did not continue");
}

#[test]
fn a_blocked_narrow_junction_preserves_the_queue_and_admits_an_unrelated_exit() {
    let (mut world, catalog, frame, _) = corridor();
    let mut metrics = RunMetrics::default();
    let point = |east, north| frame.to_geo([east, 0., north]).unwrap();
    world.living.roads = vec![
        RoadRoute {
            id: "geometry:north-south".into(),
            points: vec![point(0., 20.), point(0., -80.)],
            width_m: 9.5,
            rules: RoadRules::default(),
        },
        RoadRoute {
            id: "geometry:east-west".into(),
            points: vec![point(-40., -25.), point(40., -25.)],
            width_m: 9.5,
            rules: RoadRules::default(),
        },
    ];
    let destination = point(1.75, -60.);
    let path = world
        .living
        .vehicle_route_between(point(1.75, 0.), destination);
    let driver = world.living.units.get_mut(&DRIVER).unwrap();
    driver.position = point(1.75, 0.);
    driver.home = driver.position;
    driver.order = UnitOrder::Move { destination };
    driver.path = path;
    let blocker = world.living.units.get_mut(&(DRIVER + 1)).unwrap();
    blocker.position = point(1.75, -35.);
    blocker.home = blocker.position;
    let mut crossing = world.living.units[&DRIVER].clone();
    crossing.id = DRIVER + 2;
    crossing.position = point(-30., -23.25);
    crossing.home = crossing.position;
    crossing.heading_deg = 90.;
    let crossing_goal = point(30., -23.25);
    crossing.order = UnitOrder::Move {
        destination: crossing_goal,
    };
    crossing.path = world
        .living
        .vehicle_route_between(crossing.position, crossing_goal);
    world.living.units.insert(crossing.id, crossing);
    world.living.next_id = DRIVER + 3;
    let support = |_, _, p| {
        let local = frame.to_local(p);
        (local[0].abs() <= 40.
            && (-80. ..=20.).contains(&local[2])
            && (local[0].abs() <= 4.75 || (local[2] + 25.).abs() <= 4.75))
            .then_some(p)
    };
    let mut first_grant = None;
    for _ in 0..120 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, support);
        assert_step(&before, &world, frame, 40.);
        if first_grant.is_none() {
            first_grant = world.living.traffic.junctions.iter().find_map(|j| j.holder);
        }
        assert!(frame.to_local(world.living.units[&DRIVER].position)[2] > -18.);
    }
    assert_eq!(first_grant, Some(DRIVER + 2));
    assert!(
        world.living.units[&(DRIVER + 2)]
            .position
            .surface_distance_m(crossing_goal)
            < 1.5
    );
    assert!(world.living.metrics.blocked_exits > 0);
    let mut saved: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    // An explicit new order moves the obstruction out through its normal
    // handling. It is neither deleted nor relocated to recover the queue.
    let blocker_goal = point(1.75, -75.);
    for state in [&mut world, &mut saved] {
        let path = state
            .living
            .vehicle_route_between(state.living.units[&(DRIVER + 1)].position, blocker_goal);
        let blocker = state.living.units.get_mut(&(DRIVER + 1)).unwrap();
        blocker.order = UnitOrder::Move {
            destination: blocker_goal,
        };
        blocker.path = path;
        blocker.path_index = 0;
    }
    for step in 0..1200 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, support);
        saved
            .advance_living_with_environment(0.1, 0., &catalog, support)
            .unwrap();
        assert_eq!(saved, world);
        metrics.resumed_steps += 1;
        assert_step(&before, &world, frame, 40.);
        if matches!(world.living.units[&DRIVER].order, UnitOrder::Idle) {
            assert!(
                world.living.units[&DRIVER]
                    .position
                    .surface_distance_m(destination)
                    < 1.5
            );
            assert_eq!(world.living.units.len(), 3);
            world.validate_with_catalog(&catalog).unwrap();
            metrics.emit(serde_json::json!({
                    "case": "blocked_narrow_junction", "road_width_m": 9.5,
                    "blocked_seconds": 12., "seconds_after_physical_clear_order": (step + 1) as f64 * 0.1,
                    "unrelated_exit_completed_while_blocked": true,
                    "queue_crossed_only_after_exit_available": true, "arrived": true,
                    "goods_money_preserved": true, "exact_resume": true,
                    "supported_envelopes": true, "swept_contacts_clear": true
                }));
            return;
        }
    }
    panic!("the junction did not resume after its exit physically cleared");
}

#[test]
fn a_patrol_keeps_its_circuit_through_a_saved_narrow_reverse_approach() {
    let (mut world, catalog, frame, destination) = corridor();
    let mut metrics = RunMetrics::default();
    let circuit = vec![frame.origin, destination];
    let driver = world.living.units.get_mut(&DRIVER).unwrap();
    driver.order = UnitOrder::Patrol;
    driver.path.clear();
    driver.patrol = circuit.clone();
    driver.path_index = 1;
    for _ in 0..6000 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 2.5 && (-80. ..=20.).contains(&local[2])).then_some(p)
        });
        assert_step(&before, &world, frame, 2.5);
        assert_eq!(world.living.units[&DRIVER].patrol, circuit);
    }
    let mut saved: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    let mut reverse_approach_seen = false;
    for _ in 0..1200 {
        let before = world.clone();
        let support = |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 4.75 && (-80. ..=20.).contains(&local[2])).then_some(p)
        };
        metrics.advance(&mut world, &catalog, support);
        saved
            .advance_living_with_environment(0.1, 0., &catalog, support)
            .unwrap();
        assert_eq!(world, saved);
        assert_step(&before, &world, frame, 4.75);
        let unit = &world.living.units[&DRIVER];
        assert_eq!(unit.order, UnitOrder::Patrol);
        assert_eq!(unit.patrol, circuit);
        assert!(unit.path.len() <= 3);
        reverse_approach_seen |= unit.path.len() == 3;
        if unit.position.surface_distance_m(destination) < 3. {
            assert!(reverse_approach_seen);
            world.validate_with_catalog(&catalog).unwrap();
            return;
        }
    }
    panic!("the saved patrol did not recover through the supported narrow flank");
}

#[test]
fn unconsumed_waypoints_still_own_the_full_route_capacity() {
    let (mut world, catalog, frame, destination) = corridor();
    let path = vec![destination; 129];
    world.living.units.get_mut(&DRIVER).unwrap().path = path.clone();
    let mut metrics = RunMetrics::default();
    for _ in 0..30 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, |_, _, p| {
            let local = frame.to_local(p);
            (local[0].abs() <= 40. && (-80. ..=20.).contains(&local[2])).then_some(p)
        });
        assert_step(&before, &world, frame, 40.);
        assert_eq!(world.living.units[&DRIVER].path, path);
        assert_eq!(world.living.units[&DRIVER].path_index, 0);
    }
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn a_far_original_waypoint_never_enters_an_unbounded_reconnection_callback() {
    let (mut world, catalog, frame, _) = corridor();
    let destination = frame.to_geo([0., 0., -2_000.]).unwrap();
    let unit = world.living.units.get_mut(&DRIVER).unwrap();
    unit.order = UnitOrder::Move { destination };
    unit.path = vec![destination];
    let max_distance = Cell::new(0_f64);
    let calls = Cell::new(0_usize);
    let sample = |_| {
        Some(VehicleGroundSample {
            height_m: 100.,
            surface: VehicleSurface::Paved,
        })
    };
    let resolve = |from: &terra_core::ActorState, to: &terra_core::ActorState, _| {
        let distance = from
            .position
            .surface_distance_m(to.position)
            .hypot(to.position.altitude_m - from.position.altitude_m);
        max_distance.set(max_distance.get().max(distance));
        calls.set(calls.get() + 1);
        Some(to.position)
    };
    for _ in 0..20 {
        calls.set(0);
        world
            .advance_living_with_vehicle_environment(
                5.,
                0.,
                &catalog,
                true,
                &|_, _, p| Some(p),
                AutonomousVehicleEnvironment {
                    sample: &sample,
                    resolve: &resolve,
                    weather_grip: 1.,
                },
            )
            .unwrap();
        assert!(max_distance.get() <= 160.);
        assert!(
            calls.get() <= 512,
            "bounded fixture work at the maximum public timestep"
        );
        assert!(
            matches!(world.living.units[&DRIVER].order, UnitOrder::Move { destination: p } if p == destination)
        );
        assert_eq!(world.living.units[&DRIVER].pocket_money, 100);
        assert_eq!(world.living.units[&DRIVER].supplies["wood"], 12);
    }
    assert!(
        max_distance.get() > 5.,
        "the test exercised actual detour callbacks"
    );
    world.validate_with_catalog(&catalog).unwrap();
}

fn long_legacy_reconnection() -> (SandboxState, Catalog, LocalFrame, GeoPosition) {
    let (mut world, catalog, frame, _) = corridor();
    let destination = frame.to_geo([0., 0., -155.]).unwrap();
    let unit = world.living.units.get_mut(&DRIVER).unwrap();
    unit.order = UnitOrder::Move { destination };
    unit.path = vec![destination];
    let blocker = world.living.units.get_mut(&(DRIVER + 1)).unwrap();
    blocker.kind = ActorKind::Human;
    blocker.role = UnitRole::Worker;
    blocker.position = frame.to_geo([0., 0., -12.]).unwrap();
    blocker.home = blocker.position;
    let corners = LivingContacts::new(&world)
        .unwrap()
        .avoidance_corners(
            DRIVER,
            DRIVER + 1,
            world.living.units[&DRIVER].position,
            destination,
            1.,
        )
        .unwrap();
    let reconnect_m = corners[1].surface_distance_m(destination);
    assert!(reconnect_m > 128. && reconnect_m <= 160.);
    (world, catalog, frame, destination)
}

#[test]
fn legacy_reconnection_admits_a_complete_bounded_leg_above_one_callback_budget() {
    let (mut world, catalog, frame, destination) = long_legacy_reconnection();
    let support = |_, _, p| {
        let local = frame.to_local(p);
        (local[0].abs() <= 12. && (-175. ..=20.).contains(&local[2])).then_some(p)
    };
    let mut resumed: Option<SandboxState> = None;
    let mut metrics = RunMetrics::default();
    for _ in 0..600 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, support);
        assert_step(&before, &world, frame, 12.);
        let unit = &world.living.units[&DRIVER];
        if let Some(saved) = &mut resumed {
            saved
                .advance_living_with_environment(0.1, 0., &catalog, support)
                .unwrap();
            assert_eq!(*saved, world);
            metrics.resumed_steps += 1;
        } else if unit.path.len() == 3 {
            assert_eq!(unit.path[2], destination);
            assert!(frame.to_local(unit.path[0])[0].abs() > 4.);
            resumed = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(unit.order, UnitOrder::Idle) {
            assert!(unit.position.surface_distance_m(destination) < 1.5);
            assert!(metrics.resumed_steps > 0);
            world.validate_with_catalog(&catalog).unwrap();
            println!(
                "NAVIGATION_RECONNECTION_REPORT {}",
                serde_json::json!({
                    "case": "complete_155m_route", "elapsed_physical_seconds": metrics.updates as f64 * 0.1,
                    "driver_travelled_m": metrics.travelled_m, "source_queries": metrics.queries,
                    "max_source_queries_per_update": metrics.max_queries,
                    "exact_resumed_steps": metrics.resumed_steps,
                    "supported_complete_envelopes": true, "body_sweeps_clear": true,
                    "goods_and_money_conserved": true, "original_destination_reached": true
                })
            );
            return;
        }
    }
    panic!(
        "bounded complete reconnect did not arrive: {:?}",
        world.living.units[&DRIVER]
    );
}

#[test]
fn a_gap_in_the_last_reconnection_segment_prevents_partial_route_commit() {
    let (mut world, catalog, frame, destination) = long_legacy_reconnection();
    let rejected = Cell::new(0_usize);
    let support = |_, _, p| {
        let local = frame.to_local(p);
        if (-130. ..-129.).contains(&local[2]) {
            rejected.set(rejected.get() + 1);
            None
        } else {
            (local[0].abs() <= 12. && (-175. ..=20.).contains(&local[2])).then_some(p)
        }
    };
    let mut metrics = RunMetrics::default();
    for _ in 0..20 {
        let before = world.clone();
        metrics.advance(&mut world, &catalog, support);
        assert_step(&before, &world, frame, 12.);
        let unit = &world.living.units[&DRIVER];
        assert_eq!(
            unit.path,
            vec![destination],
            "unavailable final segment must not commit admitted prefixes"
        );
        assert_eq!(unit.path_index, 0);
        assert!(matches!(unit.order, UnitOrder::Move { destination: p } if p == destination));
    }
    assert!(
        rejected.get() > 0,
        "full source admission must inspect the final segment"
    );
    world.validate_with_catalog(&catalog).unwrap();
}
