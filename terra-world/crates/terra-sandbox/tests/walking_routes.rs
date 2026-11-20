use std::cell::Cell;
use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

const WALKER: u64 = 1_000_000;

fn fixture() -> (SandboxState, Catalog, LocalFrame, GeoPosition) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let destination = frame.to_geo([50., 0., 0.]).unwrap();
    let catalog = Catalog::builtin();
    let mut world = SandboxState::default();
    world.add_player(1, origin).unwrap();
    world.living = LivingWorld {
        enabled: true,
        origin: Some(origin),
        next_id: WALKER + 1,
        roads: vec![RoadRoute {
            id: "test:perimeter".into(),
            points: [
                [-100., -80.],
                [100., -80.],
                [100., 100.],
                [-100., 100.],
                [-100., -80.],
            ]
            .into_iter()
            .map(|p| frame.to_geo([p[0], 0., p[1]]).unwrap())
            .collect(),
            width_m: 6.,
            rules: RoadRules::default(),
        }],
        ..Default::default()
    };
    world.living.units.insert(
        WALKER,
        LivingUnit {
            id: WALKER,
            name: "Route reviewer".into(),
            kind: ActorKind::Human,
            owner: Some(1),
            role: UnitRole::Player,
            position: origin,
            heading_deg: 0.,
            velocity_enu_m_s: [0.; 3],
            needs: Needs::default(),
            order: UnitOrder::Idle,
            home: origin,
            path: vec![],
            path_index: 0,
            patrol: vec![],
            patrol_reverse: false,
            pocket_money: 0,
            supplies: Inventory::new(),
        },
    );
    world.living.accounts.insert(
        1,
        LivingAccount {
            player_id: 1,
            money: 20_000,
            avatar_unit: WALKER,
            active_unit: WALKER,
            possessed: true,
        },
    );
    world.validate_with_catalog(&catalog).unwrap();
    (world, catalog, frame, destination)
}

fn flat(_: ActorKind, _: GeoPosition, mut to: GeoPosition) -> Option<GeoPosition> {
    to.altitude_m = 100.;
    Some(to)
}

fn length(from: GeoPosition, path: &[GeoPosition]) -> f64 {
    let mut previous = from;
    path.iter()
        .map(|point| {
            let distance = previous.surface_distance_m(*point);
            previous = *point;
            distance
        })
        .sum()
}

fn assert_destination(unit: &LivingUnit, destination: GeoPosition) {
    let UnitOrder::Move {
        destination: actual,
    } = unit.order
    else {
        panic!("missing move order")
    };
    // The ordinary group command maps its first target through the local frame.
    assert!(
        actual
            .surface_distance_m(destination)
            .hypot(actual.altitude_m - destination.altitude_m)
            < 1e-6
    );
}

fn parked_body(world: &mut SandboxState, id: u64, position: GeoPosition, heading_deg: f32) {
    let mut parked = world.living.units[&WALKER].clone();
    parked.id = id;
    parked.owner = None;
    parked.role = UnitRole::Traffic;
    parked.kind = ActorKind::Vehicle;
    parked.heading_deg = heading_deg;
    parked.position = position;
    parked.home = position;
    parked.order = UnitOrder::Idle;
    parked.path.clear();
    parked.path_index = 0;
    parked.velocity_enu_m_s = [0.; 3];
    world.living.next_id = world.living.next_id.max(id + 1);
    world.living.units.insert(id, parked);
}

#[test]
fn supported_owned_walk_removes_perimeter_excursion_without_moving_the_actor() {
    let (mut world, catalog, frame, destination) = fixture();
    let mut road_only = world.clone();
    road_only
        .apply(
            1,
            Command::Living {
                action: LivingCommand::MoveUnits {
                    unit_ids: vec![WALKER],
                    destination,
                },
            },
            &catalog,
        )
        .unwrap();
    assert!(length(frame.origin, &road_only.living.units[&WALKER].path) > 200.);
    let samples = Cell::new(0);
    let inventory = world.players[&1].inventory.clone();
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, |kind, from, to| {
            samples.set(samples.get() + 1);
            flat(kind, from, to)
        })
        .unwrap();
    let unit = &world.living.units[&WALKER];
    assert_eq!(unit.position, frame.origin, "planning is not movement");
    assert_eq!(unit.order, road_only.living.units[&WALKER].order);
    assert_eq!(unit.path.len(), 1);
    assert!(length(frame.origin, &unit.path) < 50.1);
    assert!((1..=16_384).contains(&samples.get()));
    assert_eq!(world.living.accounts[&1].active_unit, WALKER);
    assert!(!world.living.accounts[&1].possessed);

    let mut travelled = 0.;
    let mut restored = None;
    for step in 0..240 {
        let previous = world.living.units[&WALKER].position;
        world
            .advance_living_with_environment(0.1, 0., &catalog, flat)
            .unwrap();
        let unit = &world.living.units[&WALKER];
        let moved = previous.surface_distance_m(unit.position);
        assert!(moved <= 0.451, "ordinary speed/time budget still applies");
        travelled += moved;
        if let Some(state) = &mut restored {
            let state: &mut SandboxState = state;
            state
                .advance_living_with_environment(0.1, 0., &catalog, flat)
                .unwrap();
            assert_eq!(state.living.units[&WALKER], *unit);
        } else if step == 40 {
            restored = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(unit.order, UnitOrder::Idle) {
            break;
        }
    }
    assert!(matches!(world.living.units[&WALKER].order, UnitOrder::Idle));
    assert!(
        world.living.units[&WALKER]
            .position
            .surface_distance_m(destination)
            < 1.5
    );
    assert!((48.5..50.1).contains(&travelled));
    assert_eq!(world.players[&1].inventory, inventory);
    assert_eq!(world.living.accounts[&1].money, 20_000);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn unsupported_strip_or_only_missing_corner_support_keeps_a_real_detour() {
    for corner_only in [false, true] {
        let (mut world, catalog, frame, destination) = fixture();
        let samples = Cell::new(0);
        world
            .move_units_with_environment(
                1,
                vec![WALKER],
                destination,
                &catalog,
                |kind, from, to| {
                    samples.set(samples.get() + 1);
                    let p = frame.to_local(to);
                    let blocked = if corner_only {
                        (15. ..35.).contains(&p[0]) && (0.1..0.4).contains(&p[2])
                    } else {
                        (24.8..25.2).contains(&p[0]) && p[2].abs() < 50.
                    };
                    (!blocked).then(|| flat(kind, from, to)).flatten()
                },
            )
            .unwrap();
        let unit = &world.living.units[&WALKER];
        assert!(
            unit.path.len() > 1,
            "uncovered support cannot authorize the direct shortcut"
        );
        assert!(length(frame.origin, &unit.path) > 100.);
        assert_eq!(unit.position, frame.origin);
        assert_destination(unit, destination);
        assert!(samples.get() <= 16_384);
    }
}

#[test]
fn an_unavailable_or_sliding_resolver_cannot_replace_the_road_route() {
    let (world, catalog, frame, destination) = fixture();
    let mut original = world.clone();
    original
        .apply(
            1,
            Command::Living {
                action: LivingCommand::MoveUnits {
                    unit_ids: vec![WALKER],
                    destination,
                },
            },
            &catalog,
        )
        .unwrap();
    for sliding in [false, true] {
        let mut candidate = world.clone();
        candidate
            .move_units_with_environment(1, vec![WALKER], destination, &catalog, |_, _, to| {
                if sliding {
                    let mut p = LocalFrame::new(to).to_geo([0.01, 0., 0.]).unwrap();
                    p.altitude_m = 100.;
                    Some(p)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            candidate.living.units[&WALKER].path,
            original.living.units[&WALKER].path
        );
        assert_eq!(candidate.living.units[&WALKER].position, frame.origin);
    }
}

#[test]
fn a_parked_body_gets_a_short_supported_detour_with_physical_save_resume() {
    let (mut world, catalog, frame, destination) = fixture();
    let parked_id = WALKER + 1;
    let parked_position = frame.to_geo([25., 0., 0.]).unwrap();
    parked_body(&mut world, parked_id, parked_position, 90.);
    let original = world
        .living
        .pedestrian_route_between(frame.origin, destination);
    assert!(length(frame.origin, &original) > 200.);
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, flat)
        .unwrap();
    let planned = &world.living.units[&WALKER];
    assert_eq!(planned.path.len(), 3);
    assert!((50. ..60.).contains(&length(frame.origin, &planned.path)));
    assert_eq!(planned.position, frame.origin);
    assert_destination(planned, destination);
    let mut restored = None;
    let mut travelled = 0.;
    for step in 0..300 {
        let previous = world.living.units[&WALKER].clone();
        let contacts = LivingContacts::new(&world).unwrap();
        world
            .advance_living_with_environment(0.1, 0., &catalog, flat)
            .unwrap();
        let current = &world.living.units[&WALKER];
        let moved = previous.position.surface_distance_m(current.position);
        assert!(moved <= 0.451);
        travelled += moved;
        assert!(
            contacts
                .resolve(
                    WALKER,
                    previous.position,
                    previous.heading_deg,
                    current.position,
                    current.heading_deg,
                    current.velocity_enu_m_s
                )
                .unwrap()
                .contact
                .is_none()
        );
        assert_eq!(world.living.units[&parked_id].position, parked_position);
        if let Some(state) = &mut restored {
            let state: &mut SandboxState = state;
            state
                .advance_living_with_environment(0.1, 0., &catalog, flat)
                .unwrap();
            assert_eq!(state.living.units[&WALKER], *current);
        } else if step == 5 {
            assert_eq!(current.path.len(), 3, "save before the detour is consumed");
            restored = Some(serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap());
        }
        if matches!(current.order, UnitOrder::Idle) {
            break;
        }
    }
    assert!(matches!(world.living.units[&WALKER].order, UnitOrder::Idle));
    assert!(
        world.living.units[&WALKER]
            .position
            .surface_distance_m(destination)
            < 1.5
    );
    assert!(travelled < 60.);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn native_market_pickup_poses_admit_a_local_bypass_on_supported_ground() {
    // Exact horizontal poses from the 0.12 native 261.8 m market-route review.
    // Ground is deliberately explicit/flat here; native terrain qualification
    // remains a separate source-backed check, not an implication of this test.
    let (mut world, catalog, _, _) = fixture();
    let start = GeoPosition::new(48.13776627021241, 11.574897031097356, 100.).unwrap();
    let pickup = GeoPosition::new(48.13775480396596, 11.574919386929219, 100.).unwrap();
    let destination = GeoPosition::new(48.13718885568857, 11.575134355469912, 100.).unwrap();
    // Retain the reviewed map's road topology too. The ordinary rectangular
    // fixture has a nearer northern edge and cannot reproduce this excursion.
    world.living.roads = [
        (
            "review:settlement-loop",
            6.,
            vec![
                (48.13780937047792, 11.573387711836105),
                (48.137809381786134, 11.575),
                (48.13780937047792, 11.57661228816389),
                (48.13699998869211, 11.576612262822913),
                (48.13619060679176, 11.576612237482731),
                (48.13619061809933, 11.574999999999998),
                (48.13619060679176, 11.573387762517264),
                (48.136999988692104, 11.573387737177082),
                (48.13780937047792, 11.573387711836105),
            ],
        ),
        (
            "review:market-lane",
            5.,
            vec![
                (48.136999988692104, 11.573387737177082),
                (48.137, 11.575),
                (48.13699998869211, 11.576612262822913),
            ],
        ),
    ]
    .into_iter()
    .map(|(id, width_m, points)| RoadRoute {
        id: id.into(),
        width_m,
        points: points
            .into_iter()
            .map(|(latitude, longitude)| GeoPosition::new(latitude, longitude, 100.).unwrap())
            .collect(),
        rules: RoadRules::default(),
    })
    .collect();
    world.living.units.get_mut(&WALKER).unwrap().position = start;
    world.players.get_mut(&1).unwrap().position = start;
    parked_body(&mut world, WALKER + 1, pickup, 0.);
    world.living.units.get_mut(&(WALKER + 1)).unwrap().owner = Some(1);
    world.transport.enabled = true;
    world.transport.drivers.insert(1, DriverProgress::new(1));
    let mut vehicle = VehicleState::new(WALKER + 1, 1, VehicleModel::Pickup);
    vehicle.parking_brake = true;
    world.transport.vehicles.insert(WALKER + 1, vehicle);
    let contact = LivingContacts::new(&world)
        .unwrap()
        .resolve(WALKER, start, 0., destination, 0., [0.; 3])
        .unwrap()
        .contact
        .unwrap();
    assert_eq!(contact.other_id, WALKER + 1);
    let original = world.living.pedestrian_route_between(start, destination);
    assert!(length(start, &original) > 250.);
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, flat)
        .unwrap();
    let unit = &world.living.units[&WALKER];
    assert_eq!(unit.position, start);
    assert_eq!(unit.path.len(), 3);
    assert!(length(start, &unit.path) < 85.);
    assert_destination(unit, destination);
    let contacts = LivingContacts::new(&world).unwrap();
    let mut previous = start;
    for point in &unit.path {
        assert!(
            contacts
                .resolve(WALKER, previous, 0., *point, 0., [0.; 3])
                .unwrap()
                .contact
                .is_none()
        );
        previous = *point;
    }
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn local_detours_try_the_other_side_but_never_plan_around_a_moving_body() {
    let (world, catalog, frame, destination) = fixture();
    let position = frame.to_geo([25., 0., 0.]).unwrap();
    for moving in [false, true] {
        let mut candidate = world.clone();
        parked_body(&mut candidate, WALKER + 1, position, 90.);
        if moving {
            candidate
                .living
                .units
                .get_mut(&(WALKER + 1))
                .unwrap()
                .velocity_enu_m_s[0] = 1.;
        } else {
            let positive_corner = LivingContacts::new(&candidate)
                .unwrap()
                .avoidance_corners(WALKER, WALKER + 1, frame.origin, destination, 1.)
                .unwrap()[0];
            parked_body(&mut candidate, WALKER + 2, positive_corner, 0.);
        }
        candidate
            .move_units_with_environment(1, vec![WALKER], destination, &catalog, flat)
            .unwrap();
        let path = &candidate.living.units[&WALKER].path;
        if moving {
            assert!(length(frame.origin, path) > 100.);
            let original = candidate
                .living
                .pedestrian_route_between(frame.origin, destination);
            assert!(path.iter().all(|point| {
                original
                    .iter()
                    .any(|old| point.surface_distance_m(*old) < 1e-6)
            }));
        } else {
            assert_eq!(path.len(), 3);
            assert!(length(frame.origin, path) < 60.);
            assert!(frame.to_local(path[0])[2] < -2.);
            assert!(frame.to_local(path[1])[2] < -2.);
        }
        assert_eq!(candidate.living.units[&WALKER].position, frame.origin);
        candidate.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn body_detours_require_support_at_every_corner_and_keep_the_fallback_when_unavailable() {
    let (mut world, catalog, frame, destination) = fixture();
    parked_body(
        &mut world,
        WALKER + 1,
        frame.to_geo([25., 0., 0.]).unwrap(),
        90.,
    );
    let original = world
        .living
        .pedestrian_route_between(frame.origin, destination);
    let samples = Cell::new(0);
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, |kind, from, to| {
            samples.set(samples.get() + 1);
            (frame.to_local(to)[2].abs() < 1.)
                .then(|| flat(kind, from, to))
                .flatten()
        })
        .unwrap();
    let unit = &world.living.units[&WALKER];
    assert_eq!(unit.path.len(), original.len());
    assert!(unit.path.iter().zip(&original).all(|(point, old)| {
        point.surface_distance_m(*old) < 1e-6 && (point.altitude_m - old.altitude_m).abs() < 1e-6
    }));
    assert_eq!(unit.position, frame.origin);
    assert_destination(unit, destination);
    assert!(samples.get() <= 16_384);
}

#[test]
fn body_corner_proposals_cannot_skip_missing_footprint_support() {
    let (mut world, catalog, frame, destination) = fixture();
    parked_body(
        &mut world,
        WALKER + 1,
        frame.to_geo([25., 0., 0.]).unwrap(),
        90.,
    );
    let contacts = LivingContacts::new(&world).unwrap();
    let holes: Vec<_> = [1., -1.]
        .into_iter()
        .map(|side| {
            let corner = contacts
                .avoidance_corners(WALKER, WALKER + 1, frame.origin, destination, side)
                .unwrap()[0];
            LocalFrame::new(corner).to_geo([0.275, 0., 0.275]).unwrap()
        })
        .collect();
    let calls = Cell::new(0);
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, |kind, from, to| {
            calls.set(calls.get() + 1);
            (!holes.iter().any(|hole| hole.surface_distance_m(to) < 0.03))
                .then(|| flat(kind, from, to))
                .flatten()
        })
        .unwrap();
    let unit = &world.living.units[&WALKER];
    assert!(length(frame.origin, &unit.path) > 100.);
    assert_eq!(unit.position, frame.origin);
    assert_destination(unit, destination);
    assert!(calls.get() <= 16_384);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn shortcut_planning_keeps_group_ownership_and_distance_errors_atomic() {
    let (mut world, catalog, frame, destination) = fixture();
    let before = world.clone();
    assert!(
        world
            .move_units_with_environment(2, vec![WALKER], destination, &catalog, flat)
            .is_err()
    );
    assert_eq!(world, before);
    assert!(
        world
            .move_units_with_environment(1, vec![WALKER, WALKER + 10], destination, &catalog, flat)
            .is_err()
    );
    assert_eq!(world, before);
    let too_far = frame.to_geo([2100., 0., 0.]).unwrap();
    assert!(
        world
            .move_units_with_environment(1, vec![WALKER], too_far, &catalog, flat)
            .is_err()
    );
    assert_eq!(world, before);
}

#[test]
fn a_full_group_shares_one_support_query_budget_without_losing_orders() {
    for with_obstacle in [false, true] {
        let (mut world, catalog, frame, destination) = fixture();
        let template = world.living.units[&WALKER].clone();
        let mut ids = vec![WALKER];
        for offset in 1..MAX_GROUP_ORDER {
            let mut unit = template.clone();
            unit.id += offset as u64;
            unit.position = frame.to_geo([-(offset as f64) * 0.5, 0., 0.]).unwrap();
            ids.push(unit.id);
            world.living.units.insert(unit.id, unit);
        }
        world.living.next_id = WALKER + MAX_GROUP_ORDER as u64;
        if with_obstacle {
            parked_body(
                &mut world,
                WALKER + MAX_GROUP_ORDER as u64,
                frame.to_geo([25., 0., 0.]).unwrap(),
                90.,
            );
        }
        let starts: Vec<_> = ids
            .iter()
            .map(|id| world.living.units[id].position)
            .collect();
        let calls = Cell::new(0);
        world
            .move_units_with_environment(1, ids.clone(), destination, &catalog, |kind, from, to| {
                calls.set(calls.get() + 1);
                flat(kind, from, to)
            })
            .unwrap();
        assert!(calls.get() > 12_000);
        assert!(
            calls.get() <= 16_384,
            "the budget is per command, not per selected body"
        );
        let mut shortened = 0;
        let mut retained = 0;
        for (id, start) in ids.into_iter().zip(starts) {
            let unit = &world.living.units[&id];
            assert_eq!(unit.position, start);
            assert!(matches!(unit.order, UnitOrder::Move { .. }));
            assert!((1..=129).contains(&unit.path.len()));
            if length(start, &unit.path) < 100. {
                shortened += 1;
            } else {
                retained += 1;
            }
        }
        assert!(shortened > 0);
        assert!(
            retained > 0,
            "later actors keep their route at the shared limit"
        );
        world.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn pedestrian_facility_routes_use_walking_clearance_while_vehicle_clearance_stays_intact() {
    let (mut world, _, frame, _) = fixture();
    world.living.roads.clear();
    world.living.shops.insert(
        WALKER + 1,
        Shop {
            id: WALKER + 1,
            name: "Clearance test shop".into(),
            position: frame.origin,
            stock: Inventory::new(),
            prices: Default::default(),
            money: 0,
            source_feature: None,
            authored: true,
        },
    );
    world.living.next_id += 1;
    let from = frame.to_geo([-10., 0., -2.7]).unwrap();
    let to = frame.to_geo([10., 0., -2.7]).unwrap();
    let footprints = world.living.facility_footprints();
    assert!(facility_path_clear(ActorKind::Human, from, to, &footprints));
    assert!(!facility_path_clear(
        ActorKind::Vehicle,
        from,
        to,
        &footprints
    ));
    assert_eq!(world.living.pedestrian_route_between(from, to), vec![to]);
    assert!(world.living.vehicle_route_between(from, to).len() > 1);
}

#[test]
fn invalid_host_height_cannot_publish_waypoints_outside_the_save_contract() {
    let (mut world, catalog, _, mut destination) = fixture();
    // Test the actual save boundary: a small ground adjustment could otherwise
    // pass the local one-metre support check while exceeding the schema limit.
    let height = 99_999.9;
    world
        .living
        .units
        .get_mut(&WALKER)
        .unwrap()
        .position
        .altitude_m = height;
    world.players.get_mut(&1).unwrap().position.altitude_m = height;
    for road in &mut world.living.roads {
        for point in &mut road.points {
            point.altitude_m = height;
        }
    }
    destination.altitude_m = height;
    world.validate_with_catalog(&catalog).unwrap();
    world
        .move_units_with_environment(1, vec![WALKER], destination, &catalog, |_, _, mut to| {
            to.altitude_m = 100_000.1;
            Some(to)
        })
        .unwrap();
    assert!(world.living.units[&WALKER].path.len() > 1);
    assert!(
        world.living.units[&WALKER]
            .path
            .iter()
            .all(|p| p.altitude_m <= 100_000.)
    );
    world.validate_with_catalog(&catalog).unwrap();
}
