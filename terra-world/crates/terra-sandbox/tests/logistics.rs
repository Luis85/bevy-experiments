use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64, u64, u64) {
    let c = Catalog::builtin();
    let o = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let f = LocalFrame::new(o);
    let mut w = SandboxState::default();
    w.add_player(1, o).unwrap();
    w.seed_living_demo(o, &[], &c).unwrap();
    let source = w
        .living
        .producers
        .values()
        .find(|p| p.kind == ProductionKind::Well)
        .unwrap()
        .id;
    let target = w
        .living
        .producers
        .values()
        .find(|p| p.kind == ProductionKind::Bakery)
        .unwrap()
        .id;
    w.living
        .producers
        .retain(|id, _| *id == source || *id == target);
    let s = w.living.producers.get_mut(&source).unwrap();
    s.stock = Inventory::from([("water".into(), 4)]);
    s.enabled = false;
    s.position = f.to_geo([-20., 0., 0.]).unwrap();
    let t = w.living.producers.get_mut(&target).unwrap();
    t.stock.clear();
    t.inputs = Inventory::from([("water".into(), 1)]);
    t.position = f.to_geo([100., 0., 0.]).unwrap();
    for s in w.living.shops.values_mut() {
        s.prices.clear();
    }
    w.living.units.retain(|_, u| u.role != UnitRole::Traffic);
    let couriers: Vec<_> = w
        .living
        .units
        .values()
        .filter(|u| u.role == UnitRole::Hauler)
        .map(|u| u.id)
        .collect();
    for (i, id) in couriers.iter().enumerate() {
        let u = w.living.units.get_mut(id).unwrap();
        u.position = f.to_geo([i as f64 * 4., 0., 40.]).unwrap();
        u.order = UnitOrder::Idle;
        u.path.clear();
        u.path_index = 0;
    }
    (w, c, source, target, couriers[0])
}
fn water(w: &SandboxState) -> u64 {
    w.living
        .producers
        .values()
        .map(|p| u64::from(p.stock.get("water").copied().unwrap_or(0)))
        .sum::<u64>()
        + w.living
            .units
            .values()
            .map(|u| u64::from(u.supplies.get("water").copied().unwrap_or(0)))
            .sum::<u64>()
}
#[test]
fn dispatch_reserves_source_stock_across_all_couriers() {
    let (mut w, c, s, _, _) = fixture();
    w.advance_living(1., 0., &c).unwrap();
    assert_eq!(w.living.deliveries.len(), 1);
    assert_eq!(w.living.deliveries[0].amount, 4);
    assert_eq!(w.living.producers[&s].stock["water"], 4);
    w.advance_living(1., 0., &c).unwrap();
    assert!(
        w.living
            .deliveries
            .iter()
            .filter(|d| d.phase == DeliveryPhase::ToPickup)
            .map(|d| d.amount)
            .sum::<u32>()
            <= w.living.producers[&s].stock["water"]
    );
    w.validate_with_catalog(&c).unwrap();
}

#[test]
fn dispatch_books_both_service_bays_without_blocking_independent_pairs() {
    let (mut world, catalog, source, target, _) = fixture();
    let frame = LocalFrame::new(world.living.origin.unwrap());
    // Two explicit, independent input shortages. The general demo starts with
    // stocked recipes, so it does not promise two ready orders at real time 1s.
    let mut second_source = world.living.producers[&source].clone();
    second_source.id = world.living.next_id;
    world.living.next_id += 1;
    second_source.kind = ProductionKind::Woodlot;
    second_source.position = frame.to_geo([60., 0., 40.]).unwrap();
    second_source.stock = Inventory::from([("wood".into(), 4)]);
    second_source.outputs = Inventory::from([("wood".into(), 4)]);
    let mut second_target = world.living.producers[&target].clone();
    second_target.id = world.living.next_id;
    world.living.next_id += 1;
    second_target.kind = ProductionKind::Sawmill;
    second_target.position = frame.to_geo([100., 0., 40.]).unwrap();
    second_target.inputs = Inventory::from([("wood".into(), 1)]);
    second_target.outputs = Inventory::from([("planks".into(), 1)]);
    world
        .living
        .producers
        .insert(second_source.id, second_source);
    world
        .living
        .producers
        .insert(second_target.id, second_target);
    world.validate_with_catalog(&catalog).unwrap();
    world.advance_living(1., 0., &catalog).unwrap();
    assert!(
        world.living.deliveries.len() >= 2,
        "independent facility pairs should work concurrently"
    );
    let mut bays = std::collections::BTreeSet::new();
    for delivery in &world.living.deliveries {
        assert!(bays.insert(delivery.from_site), "pickup bay double-booked");
        assert!(bays.insert(delivery.to_site), "dropoff bay double-booked");
    }
    let saved = serde_json::to_vec(&world).unwrap();
    let mut restored: SandboxState = serde_json::from_slice(&saved).unwrap();
    restored.advance_living(1., 0., &catalog).unwrap();
    let mut bays = std::collections::BTreeSet::new();
    for delivery in &restored.living.deliveries {
        assert!(bays.insert(delivery.from_site));
        assert!(bays.insert(delivery.to_site));
    }
}
#[test]
fn stranded_pickup_releases_reservation_without_removing_goods() {
    let (mut w, c, s, t, _) = fixture();
    let initial = water(&w);
    w.advance_living(1., 0., &c).unwrap();
    w.living.producers.get_mut(&t).unwrap().enabled = false;
    for _ in 0..75 {
        w.advance_living_with_environment(1., 0., &c, |_, _, _| None)
            .unwrap();
        assert_eq!(water(&w), initial);
    }
    assert!(w.living.deliveries.is_empty());
    assert_eq!(w.living.producers[&s].stock["water"], 4);
    assert_eq!(w.living.metrics.delivery_replans, 3);
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn blocked_loaded_courier_physically_returns_cargo_without_loss() {
    let (mut w, c, s, t, courier) = fixture();
    w.living
        .units
        .retain(|id, u| u.role != UnitRole::Hauler || *id == courier);
    w.living.units.get_mut(&courier).unwrap().position = w.living.producers[&s].position;
    let initial = water(&w);
    w.advance_living(1., 0., &c).unwrap();
    w.advance_living(0., 0., &c).unwrap();
    assert_eq!(w.living.deliveries[0].phase, DeliveryPhase::ToDropoff);
    w.living.producers.get_mut(&t).unwrap().enabled = false;
    // Wait for actual departure, including any bounded reverse/direction-change
    // stop, before the host reports a persistent blockage. The return test
    // needs a body outside the pickup radius, not a fixed acceleration time.
    for _ in 0..100 {
        w.advance_living(0.1, 0., &c).unwrap();
        if w.living.units[&courier]
            .position
            .surface_distance_m(w.living.producers[&s].position)
            > 4.
        {
            break;
        }
    }
    assert!(
        w.living.units[&courier]
            .position
            .surface_distance_m(w.living.producers[&s].position)
            > 3.,
        "the loaded courier must physically depart before blockage: {:?}",
        w.living.units[&courier]
    );
    assert_eq!(w.living.deliveries[0].phase, DeliveryPhase::ToDropoff);
    assert_eq!(w.living.units[&courier].supplies["water"], 4);
    for _ in 0..70 {
        w.advance_living_with_environment(1., 0., &c, |_, _, _| None)
            .unwrap();
        assert_eq!(water(&w), initial);
    }
    assert_eq!(w.living.deliveries[0].phase, DeliveryPhase::Returning);
    assert_eq!(w.living.units[&courier].supplies["water"], 4);
    for _ in 0..180 {
        w.advance_living(1., 0., &c).unwrap();
        if w.living.deliveries.is_empty() {
            break;
        }
    }
    assert!(w.living.deliveries.is_empty());
    assert!(w.living.units[&courier].supplies.is_empty());
    assert_eq!(w.living.producers[&s].stock["water"], 4);
    assert_eq!(w.living.metrics.deliveries_returned, 1);
    assert_eq!(water(&w), initial);
    w.validate_with_catalog(&c).unwrap();
}
#[test]
fn road_graph_uses_connections_and_repeated_queries_are_deterministic() {
    let (mut w, _, _, _, _) = fixture();
    let o = w.living.origin.unwrap();
    let f = LocalFrame::new(o);
    w.living.producers.clear();
    w.living.shops.clear();
    w.living.roads = vec![RoadRoute {
        id: "authored:elbow".into(),
        rules: Default::default(),
        width_m: 6.,
        points: vec![
            o,
            f.to_geo([100., 0., 0.]).unwrap(),
            f.to_geo([100., 0., 100.]).unwrap(),
        ],
    }];
    let goal = f.to_geo([100., 0., 100.]).unwrap();
    let path = w.living.route_between(o, goal);
    assert!(path.len() >= 3);
    assert_eq!(path.last(), Some(&goal));
    assert_eq!(path, w.living.route_between(o, goal));
}
#[test]
fn stalled_delivery_metadata_is_validated_and_missing_legacy_fields_default() {
    let (mut w, c, _, _, _) = fixture();
    w.advance_living(1., 0., &c).unwrap();
    let mut wire = serde_json::to_value(&w).unwrap();
    let d = &mut wire["living"]["deliveries"][0];
    d.as_object_mut().unwrap().remove("stalled_seconds");
    d.as_object_mut().unwrap().remove("route_retries");
    d.as_object_mut().unwrap().remove("last_position");
    let mut old: SandboxState = serde_json::from_value(wire).unwrap();
    old.validate_with_catalog(&c).unwrap();
    assert_eq!(old.living.deliveries[0].route_retries, 0);
    old.living.deliveries[0].stalled_seconds = f64::NAN;
    assert!(old.validate().is_err());
    let mut view = w.living.snapshot(1, w.players[&1].position, 1000.);
    view.deliveries[0].route_retries = 255;
    assert!(view.validate_for(1).is_err());
}
