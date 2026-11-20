use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

fn road(id: &str, points: Vec<GeoPosition>) -> RoadRoute {
    RoadRoute {
        id: id.into(),
        points,
        width_m: 6.,
        rules: RoadRules::default(),
    }
}
fn crossing_world() -> (SandboxState, Catalog, LocalFrame) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let p = |x, z| frame.to_geo([x, 0., z]).unwrap();
    let mut world = SandboxState::default();
    world.living = LivingWorld {
        enabled: true,
        origin: Some(origin),
        roads: vec![
            road("test:east-west", vec![p(-80., 0.), p(80., 0.)]),
            road("test:north-south", vec![p(0., -80.), p(0., 80.)]),
        ],
        ..Default::default()
    };
    (world, Catalog::builtin(), frame)
}
fn vehicle(world: &mut SandboxState, frame: LocalFrame, start: [f64; 2], end: [f64; 2]) -> u64 {
    let id = world.living.next_id;
    world.living.next_id += 1;
    let position = frame.to_geo([start[0], 0., start[1]]).unwrap();
    let destination = frame.to_geo([end[0], 0., end[1]]).unwrap();
    let delta = LocalFrame::new(position).to_local(destination);
    let path = world.living.vehicle_route_between(position, destination);
    world.living.units.insert(
        id,
        LivingUnit {
            id,
            name: format!("Car {id}"),
            kind: ActorKind::Vehicle,
            owner: None,
            role: UnitRole::Traffic,
            position,
            heading_deg: delta[0].atan2(-delta[2]).to_degrees() as f32,
            velocity_enu_m_s: [0.; 3],
            needs: Needs::default(),
            order: UnitOrder::Move { destination },
            home: position,
            path,
            path_index: 0,
            patrol: vec![],
            patrol_reverse: false,
            pocket_money: 0,
            supplies: Inventory::new(),
        },
    );
    id
}

#[test]
fn supported_source_rules_are_explicit_and_legacy_roads_remain_compatible() {
    let rules = RoadRules::from_properties(
        &[
            ("oneway".into(), "-1".into()),
            ("maxspeed".into(), "30 mph".into()),
            ("layer".into(), "1".into()),
            ("bridge".into(), "yes".into()),
        ]
        .into(),
    );
    assert_eq!(rules.direction, RoadDirection::Reverse);
    assert!((rules.speed_limit_m_s.unwrap() - 13.4112).abs() < 0.0001);
    assert_eq!(rules.layer, 1);
    assert_eq!(rules.structure, RoadStructure::Bridge);
    let footway = RoadRules::from_properties(&[("highway".into(), "footway".into())].into());
    assert!(!footway.motor_access);
    let restricted = RoadRules::from_properties(
        &[
            ("access".into(), "private".into()),
            ("motorcar".into(), "yes".into()),
        ]
        .into(),
    );
    assert!(
        restricted.motor_access,
        "specific mode overrides generic access"
    );
    let conditional =
        RoadRules::from_properties(&[("maxspeed".into(), "50 @ (wet)".into())].into());
    assert_eq!(conditional.speed_limit_m_s, None);
    let (world, _, _) = crossing_world();
    let mut value = serde_json::to_value(&world.living.roads[0]).unwrap();
    value.as_object_mut().unwrap().remove("rules");
    let old: RoadRoute = serde_json::from_value(value).unwrap();
    assert_eq!(old.rules, RoadRules::default());
}

#[test]
fn directed_graph_connects_clipped_crossings_and_rejects_wrong_way_shortcuts() {
    let (mut world, _, frame) = crossing_world();
    let network = RoadNetwork::new(&world.living.roads);
    assert_eq!(network.junctions().len(), 1);
    assert_eq!(network.junctions()[0].approaches, 4);
    assert_eq!(network.directed_lane_count(), 8);
    let from = frame.to_geo([-60., 0., 0.]).unwrap();
    let to = frame.to_geo([0., 0., -60.]).unwrap();
    let plan = world.living.vehicle_route_plan(from, to);
    assert_eq!(plan.kind, RoadPlanKind::Connected);
    assert_eq!(plan.points.last(), Some(&to));
    assert!(
        plan.points
            .iter()
            .any(|p| p.surface_distance_m(frame.origin) < 4.)
    );
    world.living.roads.truncate(1);
    world.living.roads[0].rules.direction = RoadDirection::Forward;
    let east = frame.to_geo([60., 0., 0.]).unwrap();
    assert_eq!(
        world.living.vehicle_route_plan(from, east).kind,
        RoadPlanKind::Connected
    );
    assert_eq!(
        world.living.vehicle_route_plan(east, from).kind,
        RoadPlanKind::BlockedDirection
    );
    assert_eq!(world.living.vehicle_route_between(east, from), vec![east]);
    world.living.roads[0].rules.motor_access = false;
    assert_eq!(
        world.living.vehicle_route_plan(from, east).kind,
        RoadPlanKind::BlockedAccess
    );
    assert!(world.living.pedestrian_route_between(from, east).len() > 1);
}

#[test]
fn grade_separation_and_date_line_geometry_do_not_create_false_intersections() {
    let (mut world, _, _) = crossing_world();
    world.living.roads[1].rules.layer = 1;
    world.living.roads[1].rules.structure = RoadStructure::Bridge;
    assert!(RoadNetwork::new(&world.living.roads).junctions().is_empty());
    world.living.roads[1].rules = RoadRules::default();
    for p in &mut world.living.roads[1].points {
        p.altitude_m += 6.;
    }
    assert!(RoadNetwork::new(&world.living.roads).junctions().is_empty());
    for origin in [
        GeoPosition::new(0., 179.9999, 10.).unwrap(),
        GeoPosition::new(89.99, 179.9, 10.).unwrap(),
    ] {
        let frame = LocalFrame::new(origin);
        let p = |x, z| frame.to_geo([x, 0., z]).unwrap();
        let roads = [
            road("test:ew", vec![p(-60., 0.), p(60., 0.)]),
            road("test:ns", vec![p(0., -60.), p(0., 60.)]),
        ];
        let network = RoadNetwork::new(&roads);
        assert_eq!(network.junctions().len(), 1);
        assert_eq!(
            network.plan(p(-40., 0.), p(0., -40.), true).kind,
            RoadPlanKind::Connected
        );
    }
}

#[test]
fn four_way_arrivals_clear_fifo_without_overlapping_and_survive_save_resume() {
    let (mut world, catalog, frame) = crossing_world();
    let ids = [
        vehicle(&mut world, frame, [-28., 1.75], [28., 1.75]),
        vehicle(&mut world, frame, [1.75, 28.], [1.75, -28.]),
        vehicle(&mut world, frame, [28., -1.75], [-28., -1.75]),
        vehicle(&mut world, frame, [-1.75, -28.], [-1.75, 28.]),
    ];
    let destinations: Vec<_> = ids
        .iter()
        .map(|id| match world.living.units[id].order {
            UnitOrder::Move { destination } => destination,
            _ => unreachable!(),
        })
        .collect();
    let mut grants = vec![];
    let mut restored = None;
    for step in 0..800 {
        let positions: Vec<_> = ids
            .iter()
            .map(|id| world.living.units[id].position)
            .collect();
        world.advance_living(0.1, 0., &catalog).unwrap();
        if let Some(holder) = world
            .living
            .traffic
            .junctions
            .first()
            .and_then(|j| j.holder)
        {
            if grants.last() != Some(&holder) {
                grants.push(holder);
            }
        }
        for (i, &id) in ids.iter().enumerate() {
            assert!(positions[i].surface_distance_m(world.living.units[&id].position) <= 0.801);
        }
        let inside = ids
            .iter()
            .filter(|id| {
                world.living.units[id]
                    .position
                    .surface_distance_m(frame.origin)
                    < 5.
            })
            .count();
        assert!(
            inside <= 1,
            "two crossing vehicles entered the conflict zone"
        );
        if step == 4 {
            world.validate_with_catalog(&catalog).unwrap();
            let copy: SandboxState =
                serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
            copy.validate_with_catalog(&catalog).unwrap();
            assert_eq!(
                world, copy,
                "compiled cache must not affect semantic equality"
            );
            restored = Some(copy);
        } else if let Some(copy) = &mut restored {
            copy.advance_living(0.1, 0., &catalog).unwrap();
            assert_eq!(
                &world, copy,
                "queue order diverged after resume at step {step}"
            );
        }
    }
    for (id, goal) in ids.iter().zip(destinations) {
        assert!(
            world.living.units[id].position.surface_distance_m(goal) < 1.6,
            "vehicle {id} did not clear; grants={grants:?}, traffic={:?}, bodies={:?}",
            world.living.traffic,
            world
                .living
                .units
                .values()
                .map(|u| (
                    u.id,
                    frame.to_local(u.position),
                    u.heading_deg,
                    u.path_index,
                    u.path.get(u.path_index).map(|p| frame.to_local(*p))
                ))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(grants, ids);
    assert!(world.living.metrics.intersection_waits > 0);
    assert_eq!(world.living.traffic.active_permits(), 0);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn blocked_exit_does_not_hold_up_an_unrelated_clear_turn() {
    let (mut world, catalog, frame) = crossing_world();
    let north = vehicle(&mut world, frame, [1.75, 24.], [1.75, -35.]);
    let east = vehicle(&mut world, frame, [-24., 1.75], [35., 1.75]);
    let blocker = vehicle(&mut world, frame, [1.75, -10.], [1.75, -30.]);
    world.living.units.get_mut(&blocker).unwrap().order = UnitOrder::Idle;
    let mut first = None;
    for _ in 0..100 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        if first.is_none() {
            first = world
                .living
                .traffic
                .junctions
                .first()
                .and_then(|j| j.holder);
        }
    }
    assert_eq!(first, Some(east));
    assert!(world.living.metrics.blocked_exits > 0);
    assert!(frame.to_local(world.living.units[&north].position)[2] > 6.);
    assert!(frame.to_local(world.living.units[&east].position)[0] > 20.);
    world.living.units.remove(&blocker);
    for _ in 0..200 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert!(frame.to_local(world.living.units[&north].position)[2] < -25.);
}

#[test]
fn stalled_grant_requeues_after_physical_timeout_and_hold_prunes_reservations() {
    let (mut world, catalog, frame) = crossing_world();
    let first = vehicle(&mut world, frame, [-25., 1.75], [35., 1.75]);
    let second = vehicle(&mut world, frame, [1.75, 25.], [1.75, -35.]);
    for _ in 0..130 {
        world
            .advance_living_with_environment(0.1, 0., &catalog, |_, from, to| {
                if frame.to_local(from)[0] < -20. {
                    None
                } else {
                    Some(to)
                }
            })
            .unwrap();
    }
    assert!(world.living.metrics.intersection_requeues > 0);
    assert!(frame.to_local(world.living.units[&second].position)[2] < -6.);
    assert!(frame.to_local(world.living.units[&first].position)[0] < -20.);
    let stopped = world.living.units.get_mut(&first).unwrap();
    stopped.order = UnitOrder::Idle;
    stopped.path.clear();
    stopped.path_index = 0;
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(
        !world
            .living
            .traffic
            .junctions
            .iter()
            .any(|j| j.queue.iter().any(|r| r.unit_id == first))
    );
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn numeric_speed_limit_is_obeyed_without_accelerated_calendar_motion() {
    let (mut world, catalog, frame) = crossing_world();
    world.living.roads.truncate(1);
    world.living.roads[0].rules.speed_limit_m_s = Some(2.);
    let id = vehicle(&mut world, frame, [-40., 1.75], [40., 1.75]);
    let mut distance = 0.;
    for _ in 0..40 {
        let previous = world.living.units[&id].position;
        world.advance_living(0.1, 100., &catalog).unwrap();
        let step = previous.surface_distance_m(world.living.units[&id].position);
        assert!(step <= 0.201);
        distance += step;
    }
    assert!(distance > 6. && distance < 8.1);
    assert!((world.living.traffic.elapsed_seconds - 4.).abs() < 1e-9);
}

#[test]
fn occupied_junction_never_expires_and_large_steps_stay_behind_stop_lines() {
    let (mut world, catalog, frame) = crossing_world();
    let east = vehicle(&mut world, frame, [-40., 1.75], [40., 1.75]);
    let north = vehicle(&mut world, frame, [1.75, 40.], [1.75, -40.]);
    let blocker = vehicle(&mut world, frame, [0., 0.], [30., 0.]);
    let parked = world.living.units.get_mut(&blocker).unwrap();
    parked.order = UnitOrder::Idle;
    parked.path.clear();
    for _ in 0..20 {
        world.advance_living(5., 0., &catalog).unwrap();
        assert!(
            world.living.units[&east]
                .position
                .surface_distance_m(frame.origin)
                > 6.7
        );
        assert!(
            world.living.units[&north]
                .position
                .surface_distance_m(frame.origin)
                > 6.7
        );
    }
    assert_eq!(world.living.traffic.active_permits(), 0);
    assert_eq!(world.living.metrics.intersection_grants, 0);
    world.living.units.remove(&blocker);
    for _ in 0..60 {
        world.advance_living(0.5, 0., &catalog).unwrap();
        if frame.to_local(world.living.units[&east].position)[0] > 20.
            && frame.to_local(world.living.units[&north].position)[2] < -20.
        {
            break;
        }
    }
    assert!(frame.to_local(world.living.units[&east].position)[0] > 20.);
    assert!(frame.to_local(world.living.units[&north].position)[2] < -20.);
}

#[test]
fn manual_approach_and_occupancy_block_automatic_admission_without_trusting_a_permit() {
    let (mut world, catalog, frame) = crossing_world();
    let east = vehicle(&mut world, frame, [-30., 1.75], [30., 1.75]);
    let manual = vehicle(&mut world, frame, [1.75, -14.], [1.75, 30.]);
    let driver_position = world.living.units[&manual].position;
    world.add_player(1, driver_position).unwrap();
    let avatar = world.living.next_id;
    world.living.next_id += 1;
    let mut person = world.living.units[&manual].clone();
    person.id = avatar;
    person.kind = ActorKind::Human;
    person.owner = Some(1);
    person.role = UnitRole::Player;
    person.position = frame.to_geo([80., 0., 80.]).unwrap();
    person.home = person.position;
    person.order = UnitOrder::Idle;
    person.path.clear();
    world.living.units.insert(avatar, person);
    let unit = world.living.units.get_mut(&manual).unwrap();
    unit.owner = Some(1);
    unit.role = UnitRole::Player;
    unit.order = UnitOrder::Idle;
    unit.path.clear();
    unit.velocity_enu_m_s = [0., -8., 0.];
    world.living.accounts.insert(
        1,
        LivingAccount {
            player_id: 1,
            money: 0,
            avatar_unit: avatar,
            active_unit: manual,
            possessed: true,
        },
    );
    for _ in 0..50 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert_eq!(world.living.metrics.intersection_grants, 0);
    assert!(frame.to_local(world.living.units[&east].position)[0] < -6.);
    // The trusted host has now moved the controlled car well clear; its old
    // projected trajectory no longer owns a phantom crossing reservation.
    world
        .set_controlled_motion(1, frame.to_geo([1.75, 0., 50.]).unwrap(), [0.; 3], 180.)
        .unwrap();
    for _ in 0..80 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert!(frame.to_local(world.living.units[&east].position)[0] > 20.);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn cached_road_changes_stale_regional_ids_and_ticket_extremes_are_safe() {
    let (mut world, catalog, frame) = crossing_world();
    let first = vehicle(&mut world, frame, [-25., 1.75], [35., 1.75]);
    let _second = vehicle(&mut world, frame, [1.75, 25.], [1.75, -35.]);
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert_eq!(world.living.traffic.queued_vehicles(), 2);
    let mut invalid = world.clone();
    invalid.living.traffic.junctions[0].queue[1].ticket =
        invalid.living.traffic.junctions[0].queue[0].ticket;
    assert!(invalid.validate_with_catalog(&catalog).is_err());
    world.living.units.remove(&first);
    // Explicit despawn removes the body's physical state; the stale scheduling
    // hint below is still allowed and is pruned before admission.
    world.living.traffic.vehicle_dynamics.remove(&first);
    // A body may become cold/absent without making the global scheduling hint
    // authoritative. Active admission prunes it before the next motion step.
    world.validate_with_catalog(&catalog).unwrap();
    world.living.traffic.next_ticket = u64::MAX;
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(
        !world
            .living
            .traffic
            .junctions
            .iter()
            .any(|j| j.queue.iter().any(|r| r.unit_id == first))
    );
    assert!(world.living.traffic.next_ticket < 1000);
    world.living.roads[1].rules.layer = 1;
    world.living.roads[1].rules.structure = RoadStructure::Bridge;
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert_eq!(
        world.living.traffic.queued_vehicles(),
        0,
        "changed source rules must invalidate compiled junctions"
    );
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn reverse_one_way_population_uses_reversed_segment_lengths_without_bunching() {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let feature = terra_core::WorldFeature {
        id: "osm:way:reverse-uneven".into(),
        kind: terra_core::FeatureKind::Road,
        points: vec![
            origin,
            frame.to_geo([0., 0., -20.]).unwrap(),
            frame.to_geo([1000., 0., -20.]).unwrap(),
        ],
        height_m: 6.,
        properties: [
            ("oneway".into(), "-1".into()),
            ("highway".into(), "residential".into()),
        ]
        .into(),
    };
    let catalog = Catalog::builtin();
    let mut world = SandboxState::default();
    world
        .seed_living_demo(origin, &[feature], &catalog)
        .unwrap();
    let traffic: Vec<_> = world
        .living
        .units
        .values()
        .filter(|u| u.role == UnitRole::Traffic)
        .collect();
    assert_eq!(
        traffic.len(),
        8,
        "reverse way must distribute starts across the long segment, not collapse them at a short endpoint"
    );
    for (i, unit) in traffic.iter().enumerate() {
        assert!(
            matches!(unit.order, UnitOrder::Move { .. }),
            "open one-way traffic must stop instead of reverse"
        );
        assert!((unit.heading_deg - 270.).abs() < 0.1);
        for other in traffic.iter().skip(i + 1) {
            assert!(unit.position.surface_distance_m(other.position) > 8.);
        }
    }
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn facility_corner_budget_never_degrades_a_directed_route_into_a_shortcut() {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let mut world = LivingWorld {
        origin: Some(origin),
        ..Default::default()
    };
    for index in 0..2 {
        let mut route = road(
            &format!("test:long-{index}"),
            (0..64)
                .map(|i| {
                    frame
                        .to_geo([(index * 63 + i) as f64 * 2., 0., 0.])
                        .unwrap()
                })
                .collect(),
        );
        route.rules.direction = RoadDirection::Forward;
        world.roads.push(route);
    }
    let goal = frame.to_geo([252., 0., 0.]).unwrap();
    assert_eq!(
        world.vehicle_route_plan(origin, goal).kind,
        RoadPlanKind::Connected
    );
    world.shops.insert(
        1,
        Shop {
            id: 1,
            name: "Corner budget obstacle".into(),
            position: frame.to_geo([80., 0., 7.75]).unwrap(),
            stock: Inventory::new(),
            prices: Default::default(),
            money: 0,
            source_feature: None,
            authored: true,
        },
    );
    let plan = world.vehicle_route_plan(origin, goal);
    assert_eq!(plan.kind, RoadPlanKind::Unavailable);
    assert!(plan.points.is_empty());
    assert_eq!(world.vehicle_route_between(origin, goal), vec![origin]);
}
