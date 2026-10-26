use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let mut world = SandboxState::default();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    world.add_player(1, origin).unwrap();
    world.add_player(2, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    (world, catalog)
}
fn money(world: &SandboxState) -> u64 {
    world.living.accounts.values().map(|a| a.money).sum::<u64>()
        + world.living.shops.values().map(|s| s.money).sum::<u64>()
        + world
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
}
fn action(
    world: &mut SandboxState,
    who: u64,
    action: LivingCommand,
    catalog: &Catalog,
) -> Result<Outcome, SandboxError> {
    world.apply(who, Command::Living { action }, catalog)
}

#[test]
fn trading_is_atomic_and_conserves_money_and_stock() {
    let (mut world, catalog) = world();
    let shop = *world.living.shops.keys().next().unwrap();
    let position = world.living.shops[&shop].position;
    world.set_player_position(1, position).unwrap();
    let original_money = money(&world);
    let bread = world.living.shops[&shop].stock["bread"];
    action(
        &mut world,
        1,
        LivingCommand::Buy {
            shop_id: shop,
            item: "bread".into(),
            amount: 3,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(world.players[&1].inventory["bread"], 3);
    assert_eq!(world.living.shops[&shop].stock["bread"], bread - 3);
    assert_eq!(money(&world), original_money);
    action(
        &mut world,
        1,
        LivingCommand::Sell {
            shop_id: shop,
            item: "bread".into(),
            amount: 2,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(money(&world), original_money);
    assert_eq!(world.players[&1].inventory["bread"], 1);
    let before = world.clone();
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::Buy {
                shop_id: shop,
                item: "bread".into(),
                amount: 100
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
    world
        .set_player_position(1, LocalFrame::new(position).to_geo([100., 0., 0.]).unwrap())
        .unwrap();
    let before = world.clone();
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::Sell {
                shop_id: shop,
                item: "bread".into(),
                amount: 1
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
}

#[test]
fn shop_cannot_buy_with_missing_cash_and_amount_overflow_rejects() {
    let (mut world, catalog) = world();
    let id = *world.living.shops.keys().next().unwrap();
    world.living.shops.get_mut(&id).unwrap().money = 0;
    let before = world.clone();
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::Sell {
                shop_id: id,
                item: "wood".into(),
                amount: 2
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::Buy {
                shop_id: id,
                item: "wood".into(),
                amount: u32::MAX
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
}

#[test]
fn possession_keeps_identity_inventory_and_original_body() {
    let (mut world, catalog) = world();
    let avatar = world.living.accounts[&1].avatar_unit;
    let original = world.living.units[&avatar].position;
    let rover = world
        .living
        .units
        .values()
        .find(|u| u.owner == Some(1) && u.kind == ActorKind::Vehicle)
        .unwrap()
        .id;
    let stock = world.players[&1].inventory.clone();
    action(
        &mut world,
        1,
        LivingCommand::Possess { unit_id: rover },
        &catalog,
    )
    .unwrap();
    assert_eq!(
        world.players[&1].position,
        world.living.units[&rover].position
    );
    let moved = LocalFrame::new(world.players[&1].position)
        .to_geo([8., 0., 0.])
        .unwrap();
    world
        .set_controlled_motion(1, moved, [3., 0., 0.], 90.)
        .unwrap();
    assert_eq!(world.living.units[&avatar].position, original);
    action(&mut world, 1, LivingCommand::Release, &catalog).unwrap();
    assert_eq!(world.players[&1].position, moved);
    assert_eq!(world.players[&1].inventory, stock);
    assert_eq!(world.living.units[&rover].position, moved);
    let other = world.living.accounts[&2].avatar_unit;
    let before = world.clone();
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::Possess { unit_id: other },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
}

#[test]
fn rts_orders_are_owned_atomic_and_collision_checked_before_progress() {
    let (mut world, catalog) = world();
    let own = world.living.accounts[&1].avatar_unit;
    let other = world.living.accounts[&2].avatar_unit;
    let destination = LocalFrame::new(world.players[&1].position)
        .to_geo([30., 0., 0.])
        .unwrap();
    let before = world.clone();
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::MoveUnits {
                unit_ids: vec![own, other],
                destination
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
    action(
        &mut world,
        1,
        LivingCommand::MoveUnits {
            unit_ids: vec![own],
            destination,
        },
        &catalog,
    )
    .unwrap();
    let start = world.living.units[&own].position;
    for _ in 0..10 {
        world
            .advance_living_with_environment(1., 1., &catalog, |_, _, _| None)
            .unwrap();
    }
    assert_eq!(world.living.units[&own].position, start);
    for _ in 0..12 {
        world.advance_living(1., 1., &catalog).unwrap();
    }
    assert!(
        world.living.units[&own]
            .position
            .surface_distance_m(destination)
            < 2.
    );
    assert_eq!(
        world.players[&1].position,
        world.living.units[&own].position
    );
    action(
        &mut world,
        1,
        LivingCommand::StopUnits {
            unit_ids: vec![own],
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(world.living.units[&own].order, UnitOrder::Idle);
}

#[test]
fn needs_are_per_body_and_consumption_rest_and_speed_are_connected() {
    let (mut world, catalog) = world();
    let avatar = world.living.accounts[&1].avatar_unit;
    world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("bread".into(), 2);
    world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("water".into(), 2);
    world.advance_living(1., 10_000., &catalog).unwrap();
    let before = world.living.units[&avatar].needs;
    assert!(before.food < 0.8 && before.water < 0.7);
    action(
        &mut world,
        1,
        LivingCommand::Consume {
            item: "bread".into(),
        },
        &catalog,
    )
    .unwrap();
    action(
        &mut world,
        1,
        LivingCommand::Consume {
            item: "water".into(),
        },
        &catalog,
    )
    .unwrap();
    assert!(world.living.units[&avatar].needs.food > before.food);
    assert_eq!(world.players[&1].inventory["bread"], 1);
    world.living.units.get_mut(&avatar).unwrap().needs.energy = 0.1;
    let slow = world.living.units[&avatar].speed_limit_m_s();
    action(
        &mut world,
        1,
        LivingCommand::Rest {
            unit_ids: vec![avatar],
        },
        &catalog,
    )
    .unwrap();
    for _ in 0..10 {
        world.advance_living(1., 1., &catalog).unwrap();
    }
    assert!(world.living.units[&avatar].speed_limit_m_s() > slow);
}

#[test]
fn finite_sources_production_and_deliveries_are_material_conserving() {
    let (mut world, catalog) = world();
    let well = world
        .living
        .producers
        .values()
        .find(|s| s.kind == ProductionKind::Well)
        .unwrap()
        .id;
    for site in world.living.producers.values_mut() {
        site.enabled = false;
    }
    let site = world.living.producers.get_mut(&well).unwrap();
    site.enabled = true;
    site.resource_remaining = Some(2);
    site.stock.clear();
    // No dispatch on zero real time: two cycles consume two source units exactly.
    world.advance_living(0., 1000., &catalog).unwrap();
    let site = &world.living.producers[&well];
    assert_eq!(site.stock["water"], 8);
    assert_eq!(site.resource_remaining, Some(0));
    assert_eq!(site.cycles_completed, 2);
    world.advance_living(0., 1000., &catalog).unwrap();
    assert_eq!(world.living.producers[&well].stock["water"], 8);
    // With production stopped, deliveries can only transfer existing goods.
    world.living.producers.get_mut(&well).unwrap().enabled = false;
    for unit in world
        .living
        .units
        .values_mut()
        .filter(|u| u.kind == ActorKind::Human && u.owner.is_none())
    {
        unit.needs = Needs::default();
        unit.pocket_money = 0;
    }
    let total = |w: &SandboxState| {
        w.living
            .producers
            .values()
            .map(|s| s.stock.values().map(|x| u64::from(*x)).sum::<u64>())
            .sum::<u64>()
            + w.living
                .shops
                .values()
                .map(|s| s.stock.values().map(|x| u64::from(*x)).sum::<u64>())
                .sum::<u64>()
            + w.living
                .units
                .values()
                .map(|u| u.supplies.values().map(|x| u64::from(*x)).sum::<u64>())
                .sum::<u64>()
    };
    // Create real retail demand; fully stocked stores need no delivery.
    world
        .living
        .shops
        .values_mut()
        .next()
        .unwrap()
        .stock
        .remove("wood");
    let before = total(&world);
    for _ in 0..600 {
        world.advance_living(0.5, 0., &catalog).unwrap();
    }
    assert_eq!(total(&world), before);
    assert!(world.living.metrics.deliveries_completed > 0);
}

#[test]
fn only_owner_may_operate_sites_and_save_rejects_cargo_and_account_tampering() {
    let (mut world, catalog) = world();
    let id = *world.living.producers.keys().next().unwrap();
    let pos = world.living.producers[&id].position;
    world.set_player_position(2, pos).unwrap();
    let before = world.clone();
    assert!(
        action(
            &mut world,
            2,
            LivingCommand::SetProduction {
                site_id: id,
                enabled: false
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
    world.set_player_position(1, pos).unwrap();
    action(
        &mut world,
        1,
        LivingCommand::SetProduction {
            site_id: id,
            enabled: false,
        },
        &catalog,
    )
    .unwrap();
    world.validate_with_catalog(&catalog).unwrap();
    let mut invalid = world.clone();
    invalid.living.accounts.get_mut(&1).unwrap().active_unit =
        invalid.living.accounts[&2].avatar_unit;
    assert!(invalid.validate().is_err());
    let bytes = serde_json::to_vec(&world).unwrap();
    let restored: SandboxState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(world, restored);
    restored.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn v1_save_missing_living_field_migrates_without_minting_anything() {
    let mut old = SandboxState::default();
    old.add_player(1, GeoPosition::new(0., 0., 0.).unwrap())
        .unwrap();
    let mut json = serde_json::to_value(&old).unwrap();
    json.as_object_mut().unwrap().remove("living");
    let restored: SandboxState = serde_json::from_value(json).unwrap();
    assert!(!restored.living.enabled);
    assert!(restored.living.accounts.is_empty());
    restored.validate().unwrap();
}

#[test]
fn snapshot_is_bounded_hides_other_wallets_and_serializes_in_tagged_envelope() {
    let (world, _) = world();
    let view = world.snapshot_for(1, 5000.).unwrap();
    assert!(view.living.units.len() <= 64);
    assert_eq!(view.living.account.as_ref().unwrap().player_id, 1);
    assert!(
        view.living
            .units
            .iter()
            .all(|u| u.pocket_money == 0 && u.supplies.is_empty() && u.path.is_empty())
    );
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(tag = "type")]
    enum Packet {
        State { view: SandboxView },
    }
    let bytes = serde_json::to_vec(&Packet::State { view }).unwrap();
    let _: Packet = serde_json::from_slice(&bytes).unwrap();
    assert!(bytes.len() < 64 * 1024);
}

#[test]
fn rts_active_selection_preserves_orders_and_changes_interaction_body() {
    let (mut world, catalog) = world();
    let worker = world
        .living
        .units
        .values()
        .find(|u| u.owner == Some(1) && u.role == UnitRole::Worker && u.kind == ActorKind::Human)
        .unwrap()
        .id;
    let target = LocalFrame::new(world.living.units[&worker].position)
        .to_geo([10., 0., 0.])
        .unwrap();
    action(
        &mut world,
        1,
        LivingCommand::MoveUnits {
            unit_ids: vec![worker],
            destination: target,
        },
        &catalog,
    )
    .unwrap();
    let order = world.living.units[&worker].order.clone();
    action(
        &mut world,
        1,
        LivingCommand::SelectActive { unit_id: worker },
        &catalog,
    )
    .unwrap();
    assert!(!world.living.accounts[&1].possessed);
    assert_eq!(world.living.accounts[&1].active_unit, worker);
    assert_eq!(world.living.units[&worker].order, order);
    assert_eq!(
        world.players[&1].position,
        world.living.units[&worker].position
    );
}

#[test]
fn untrusted_living_views_reject_duplicates_nonfinite_fields_and_unowned_control() {
    let (world, _) = world();
    let view = world.snapshot_for(1, 5000.).unwrap().living;
    view.validate_for(1).unwrap();
    let mut bad = view.clone();
    bad.units.push(bad.units[0].clone());
    assert!(bad.validate_for(1).is_err());
    let mut bad = view.clone();
    bad.units[0].needs.food = f32::NAN;
    assert!(bad.validate_for(1).is_err());
    let mut bad = view.clone();
    bad.account.as_mut().unwrap().active_unit =
        bad.units.iter().find(|u| u.owner != Some(1)).unwrap().id;
    assert!(bad.validate_for(1).is_err());
    let mut bad = view.clone();
    bad.units[0].pocket_money = 100;
    assert!(bad.validate_for(1).is_err());
    assert!(view.validate_for(2).is_err());
}

#[test]
fn facility_geometry_and_forecourt_routing_match_swept_collision() {
    let (world, _) = world();
    let shop = world.living.shops.values().next().unwrap();
    let service = shop.position;
    let north = LocalFrame::new(service).to_geo([0., 0., -20.]).unwrap();
    let footprints = world.living.facility_footprints();
    assert!(!facility_path_clear(
        ActorKind::Human,
        north,
        service,
        &footprints
    ));
    let route = world.living.route_between(north, service);
    assert!(route.len() > 1);
    let mut from = north;
    for point in route {
        assert!(
            facility_path_clear(ActorKind::Vehicle, from, point, &footprints),
            "route crosses a facility"
        );
        from = point;
    }
    assert!(from.surface_distance_m(service) < 0.1);
}

#[test]
fn construction_cannot_trap_a_body_and_extreme_rts_altitudes_reject_atomically() {
    let (mut world, catalog) = world();
    let avatar = world.living.accounts[&1].avatar_unit;
    let position = world.players[&1].position;
    world
        .apply(
            1,
            Command::ClaimPlot {
                center: position,
                radius_m: 40.,
            },
            &catalog,
        )
        .unwrap();
    world
        .apply(
            1,
            Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
            &catalog,
        )
        .unwrap();
    let before = world.clone();
    assert!(
        world
            .apply(
                1,
                Command::Build {
                    blueprint: "fence".into(),
                    position
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(before, world);
    let extreme = GeoPosition {
        altitude_m: 1e300,
        ..position
    };
    assert!(
        action(
            &mut world,
            1,
            LivingCommand::MoveUnits {
                unit_ids: vec![avatar],
                destination: extreme
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, world);
    let helicopter = world
        .living
        .units
        .values()
        .find(|unit| unit.owner == Some(1) && unit.kind == ActorKind::Helicopter)
        .unwrap();
    assert!(helicopter.position.altitude_m > position.altitude_m + 10.);
}

#[test]
fn traffic_starts_are_separated_and_have_valid_forward_waypoints() {
    let (world, _) = world();
    let traffic: Vec<_> = world
        .living
        .units
        .values()
        .filter(|u| u.role == UnitRole::Traffic)
        .collect();
    assert_eq!(traffic.len(), 8);
    for (i, unit) in traffic.iter().enumerate() {
        assert!(unit.path_index < unit.patrol.len());
        assert!(
            unit.position
                .surface_distance_m(unit.patrol[unit.path_index])
                > 1.
        );
        for other in traffic.iter().skip(i + 1) {
            assert!(unit.position.surface_distance_m(other.position) >= 8.);
        }
    }
}

#[test]
fn repeated_source_roads_keep_unique_stable_segment_ids_in_saved_and_wire_state() {
    use std::collections::{BTreeMap, BTreeSet};
    use terra_core::{FeatureKind, WorldFeature};

    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let first = WorldFeature {
        id: format!("source:{}", "r".repeat(121)),
        kind: FeatureKind::Road,
        points: vec![origin, frame.to_geo([80., 0., 0.]).unwrap()],
        height_m: 3.048,
        properties: BTreeMap::from([("width".into(), "10 ft".into())]),
    };
    let mut continuation = first.clone();
    continuation.points = vec![
        frame.to_geo([80., 0., 0.]).unwrap(),
        frame.to_geo([160., 0., 0.]).unwrap(),
    ];
    let seed = |features: &[WorldFeature]| {
        let mut state = SandboxState::default();
        state.add_player(1, origin).unwrap();
        state.seed_living_demo(origin, features, &catalog).unwrap();
        state
    };
    let mut state = seed(&[first.clone(), continuation.clone(), first.clone()]);
    assert_eq!(
        state.living.roads.len(),
        2,
        "identical inputs must not duplicate a road"
    );
    assert!(state.living.roads.iter().all(|r| r.id.len() <= 128));
    assert!(
        state.living.roads.iter().all(|r| r.width_m == 3.048),
        "preserve the provider's normalized width when the source tag has a unit"
    );
    let ids = |world: &SandboxState| -> BTreeSet<String> {
        world.living.roads.iter().map(|r| r.id.clone()).collect()
    };
    assert_eq!(
        ids(&state).len(),
        2,
        "distinct segments retain distinct IDs"
    );
    let reordered = seed(&[continuation, first]);
    assert_eq!(ids(&state), ids(&reordered));
    state.validate_with_catalog(&catalog).unwrap();
    state
        .snapshot_for(1, 5000.)
        .unwrap()
        .living
        .validate_for(1)
        .unwrap();

    state.living.roads.push(state.living.roads[0].clone());
    assert!(
        state.validate().is_err(),
        "a duplicate road must fail before it can be saved"
    );
}

#[test]
fn regional_view_marks_all_supported_entity_and_geometry_reductions() {
    let (base, catalog) = world();
    assert!(!base.snapshot_for(1, 5000.).unwrap().truncated);

    let mut more_sites = base.clone();
    let site = more_sites.living.producers.values().next().unwrap().clone();
    while more_sites.living.producers.len() < 17 {
        let mut extra = site.clone();
        extra.id = more_sites.living.next_id;
        more_sites.living.next_id += 1;
        more_sites.living.producers.insert(extra.id, extra);
    }
    let mut more_roads = base.clone();
    while more_roads.living.roads.len() < 9 {
        let mut extra = more_roads.living.roads[0].clone();
        extra.id = format!("authored:extra:{}", more_roads.living.roads.len());
        more_roads.living.roads.push(extra);
    }
    let mut detailed_road = base.clone();
    let frame = LocalFrame::new(detailed_road.players[&1].position);
    detailed_road.living.roads[0].points = (0..40)
        .map(|i| frame.to_geo([f64::from(i) * 3., 0., 0.]).unwrap())
        .collect();

    for state in [more_sites, more_roads, detailed_road] {
        state.validate_with_catalog(&catalog).unwrap();
        let view = state.snapshot_for(1, 5000.).unwrap();
        assert!(
            view.living.truncated,
            "every living-data reduction must be explicit"
        );
        assert!(
            view.truncated,
            "top-level completeness must include the living layer"
        );
        view.living.validate_for(1).unwrap();
    }
}

#[test]
fn a_vehicle_turning_at_a_waypoint_does_not_yield_to_its_previous_following_queue() {
    let (mut state, catalog) = world();
    let origin = state.living.origin.unwrap();
    let frame = LocalFrame::new(origin);
    let couriers: Vec<_> = state
        .living
        .units
        .values()
        .filter(|u| u.role == UnitRole::Hauler)
        .take(2)
        .map(|u| u.id)
        .collect();
    state
        .living
        .units
        .retain(|id, unit| unit.kind != ActorKind::Vehicle || couriers.contains(id));
    let leader = state.living.units.get_mut(&couriers[0]).unwrap();
    leader.position = origin;
    leader.heading_deg = 315.;
    let destination = frame.to_geo([20., 0., 10.]).unwrap();
    leader.order = UnitOrder::Move { destination };
    leader.path = vec![destination];
    leader.path_index = 0;
    let follower = state.living.units.get_mut(&couriers[1]).unwrap();
    follower.position = frame.to_geo([5., 0., 2.5]).unwrap();
    follower.heading_deg = 315.;
    follower.order = UnitOrder::Move {
        destination: origin,
    };
    follower.path = vec![origin];
    follower.path_index = 0;
    state.advance_living(0.1, 0., &catalog).unwrap();
    assert!(
        state.living.units[&couriers[0]]
            .position
            .surface_distance_m(origin)
            > 0.5,
        "the leader must turn away instead of deadlocking against its following queue"
    );
}

#[test]
fn populated_settlement_delivers_with_facility_collision_and_traffic_enabled() {
    let (mut state, catalog) = world();
    let footprints = state.living.facility_footprints();
    for _ in 0..2400 {
        state
            .advance_living_with_environment(0.05, 3., &catalog, |kind, old, mut next| {
                if facility_path_clear(kind, old, next, &footprints) {
                    next.altitude_m = 100.025;
                    Some(next)
                } else {
                    None
                }
            })
            .unwrap();
    }
    assert!(state.living.metrics.production_cycles > 0);
    assert!(
        state.living.metrics.deliveries_completed > 0,
        "traffic and physical footprints must not deadlock all couriers"
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn parked_owned_rover_is_not_a_same_direction_traffic_queue() {
    let (mut state, catalog) = world();
    let origin = state.living.origin.unwrap();
    let frame = LocalFrame::new(origin);
    let courier = state
        .living
        .units
        .values()
        .find(|u| u.role == UnitRole::Hauler)
        .unwrap()
        .id;
    let rover = state
        .living
        .units
        .values()
        .find(|u| u.owner == Some(1) && u.kind == ActorKind::Vehicle)
        .unwrap()
        .id;
    state
        .living
        .units
        .retain(|id, unit| unit.kind != ActorKind::Vehicle || [courier, rover].contains(id));
    let destination = frame.to_geo([-476., 0., -224.]).unwrap();
    let unit = state.living.units.get_mut(&courier).unwrap();
    unit.position = frame.to_geo([0., 0., 8.6]).unwrap();
    let start = unit.position;
    unit.heading_deg = 296.;
    unit.order = UnitOrder::Move { destination };
    unit.path = vec![destination];
    unit.path_index = 0;
    let parked = state.living.units.get_mut(&rover).unwrap();
    parked.position = frame.to_geo([-6., 0., 5.]).unwrap();
    parked.heading_deg = 0.;
    parked.order = UnitOrder::Idle;
    parked.velocity_enu_m_s = [0.; 3];
    state.advance_living(0.1, 0., &catalog).unwrap();
    assert!(
        state.living.units[&courier]
            .position
            .surface_distance_m(start)
            > 0.5,
        "a parked, differently oriented body must not freeze a same-lane traffic queue"
    );
}

#[test]
fn rejected_demo_seed_leaves_the_previous_world_unchanged() {
    for existing_player in [false, true] {
        let mut state = SandboxState::new();
        if existing_player {
            state
                .add_player(1, GeoPosition::new(48.137, 11.575, 100_000.).unwrap())
                .unwrap();
        }
        let before = state.clone();
        let altitude = if existing_player { 0. } else { 100_000. };
        assert!(
            state
                .seed_living_demo(
                    GeoPosition::new(48.137, 11.575, altitude).unwrap(),
                    &[],
                    &Catalog::builtin()
                )
                .is_err()
        );
        assert_eq!(state, before);
    }
}
