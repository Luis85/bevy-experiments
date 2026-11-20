//! Decision previews disclose current arithmetic without granting a load,
//! exposing issuer cash, or pretending a partial view proves availability.
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64, u64, u64, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let frame = LocalFrame::new(origin);
    let mut world = SandboxState::new();
    world.add_player(1, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    world.ensure_transport_demo(&catalog).unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    let depot = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Depot)
        .unwrap()
        .id;
    let shop = *world.living.shops.keys().next().unwrap();
    let terminal = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Terminal && f.linked_site == Some(shop))
        .unwrap()
        .id;
    for (facility, x) in [(garage, 0.), (terminal, 150.), (depot, 300.)] {
        world
            .transport
            .facilities
            .get_mut(&facility)
            .unwrap()
            .position = frame.to_geo([x, 0., -500.]).unwrap();
    }
    world
        .set_player_position(1, world.transport.facilities[&garage].position)
        .unwrap();
    for action in [
        CompanyCommand::Create {
            name: "Route arithmetic".into(),
        },
        CompanyCommand::Deposit { amount: 10_000 },
        CompanyCommand::OpenDepot {
            facility_id: garage,
        },
    ] {
        world
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::Company { action },
                },
                &catalog,
            )
            .unwrap();
    }
    (world, catalog, garage, terminal, depot, shop)
}

#[test]
fn warehouse_preview_exposes_retail_shortfall_without_reclassifying_contributed_stock() {
    let (world, _, source, buyer, _, shop) = fixture();
    let before = world.clone();
    let preview = world
        .company_route_preview(
            1,
            &[source, buyer],
            &FleetService::DepotFreight {
                item: "water".into(),
                amount: 8,
            },
        )
        .unwrap();
    let retail = preview.retail_comparison.unwrap();
    assert_eq!(preview.visible_pickup_units, Some(0));
    assert_eq!(preview.buyer_unit_price, Some(30));
    assert_eq!(preview.full_load_receipt, Some(240));
    assert_eq!(retail.shop_id, shop);
    assert_eq!(retail.unit_price, 80);
    assert_eq!(retail.full_load_cost, 640);
    assert_eq!(retail.merchandise_margin, Some(-400));
    assert!(retail.available_units >= 8);
    assert_eq!(
        world, before,
        "a comparison is not a purchase, load or cash valuation"
    );
}

#[test]
fn passenger_receipt_is_first_leg_arithmetic_not_private_funding_or_profit() {
    let (mut world, _, source, destination, third, _) = fixture();
    let service = FleetService::Passengers { count: 3 };
    let preview = world
        .company_route_preview(1, &[source, destination, third], &service)
        .unwrap();
    assert_eq!(preview.destination, destination);
    assert_eq!(preview.requested_load, 3);
    assert_eq!(preview.visible_pickup_units, Some(0));
    let fare = fleet_fare(
        &service,
        world.transport.facilities[&source].position,
        world.transport.facilities[&destination].position,
    );
    assert_eq!(preview.full_load_receipt, Some(fare));
    assert_eq!(
        preview.receipt_payroll_seconds(),
        Some(fare as f64 / COMPANY_WAGE_PER_SECOND)
    );
    world.transport.treasury = 0;
    assert_eq!(
        world
            .company_route_preview(1, &[source, destination, third], &service)
            .unwrap(),
        preview,
        "issuer cash is not public route availability; dispatch still reserves it authoritatively"
    );
}

#[test]
fn missing_buyer_and_depot_are_unknown_while_known_empty_inventory_stays_zero() {
    let (mut world, _, source, buyer, _, shop) = fixture();
    let service = FleetService::DepotFreight {
        item: "water".into(),
        amount: 8,
    };
    world.living.shops.remove(&shop);
    let preview = world
        .company_route_preview(1, &[source, buyer], &service)
        .unwrap();
    assert_eq!(preview.visible_pickup_units, Some(0));
    assert_eq!(preview.full_load_receipt, None);
    assert_eq!(preview.visible_destination_units, None);
    assert_eq!(preview.destination_target_units, None);
    assert_eq!(preview.retail_comparison, None);
    let other_owner = world
        .company_route_preview(2, &[source, buyer], &service)
        .unwrap();
    assert_eq!(other_owner.visible_pickup_units, None);
}

#[test]
fn transfer_has_no_receipt_and_does_not_confuse_one_item_with_free_bay_capacity() {
    let (world, _, source, _, destination, _) = fixture();
    let preview = world
        .company_route_preview(
            1,
            &[source, destination],
            &FleetService::DepotTransfer {
                item: "water".into(),
                amount: 8,
            },
        )
        .unwrap();
    assert_eq!(preview.full_load_receipt, Some(0));
    assert_eq!(preview.receipt_payroll_seconds(), Some(0.));
    assert_eq!(preview.destination_target_units, None);
    assert!(!preview.public_fare);
    assert_eq!(preview.retail_comparison, None);
}

#[test]
fn retail_comparison_updates_and_discloses_small_stock_without_reserving_prices() {
    let (mut world, _, source, buyer, _, shop) = fixture();
    let service = FleetService::DepotFreight {
        item: "water".into(),
        amount: 8,
    };
    world
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("water".into(), 1);
    world
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .prices
        .get_mut("water")
        .unwrap()
        .buy = 90;
    let before = world.clone();
    let preview = world
        .company_route_preview(1, &[source, buyer], &service)
        .unwrap();
    let retail = preview.retail_comparison.unwrap();
    assert_eq!(retail.available_units, 1);
    assert_eq!(retail.full_load_cost, 720);
    assert_eq!(retail.merchandise_margin, Some(-480));
    assert_eq!(world, before);
    for stops in [
        vec![],
        vec![source],
        vec![source, source],
        vec![source, u64::MAX],
    ] {
        assert!(world.company_route_preview(1, &stops, &service).is_err());
        assert_eq!(world, before);
    }
}
