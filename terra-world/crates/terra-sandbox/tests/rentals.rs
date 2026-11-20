use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut world = SandboxState::new();
    world.add_player(1, origin).unwrap();
    world.add_player(2, origin).unwrap();
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    world.ensure_transport_demo(&catalog).unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .clone();
    world.set_player_position(1, garage.position).unwrap();
    (world, catalog, garage.id)
}
fn apply(
    world: &mut SandboxState,
    catalog: &Catalog,
    action: TransportCommand,
) -> Result<Outcome, SandboxError> {
    world.apply(1, Command::Transport { action }, catalog)
}
fn money(world: &SandboxState) -> u64 {
    world.transport.treasury
        + world.living.accounts.values().map(|a| a.money).sum::<u64>()
        + world.living.shops.values().map(|s| s.money).sum::<u64>()
        + world
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
        + world
            .transport
            .vehicles
            .values()
            .filter_map(|v| v.rental.as_ref())
            .map(|r| r.bond_escrow)
            .sum::<u64>()
        + world
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| a.reward_escrow)
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .map(|c| {
                c.cash
                    + c.departed_wages
                    + c.drivers
                        .iter()
                        .map(|d| d.wages_earned + d.trip.as_ref().map_or(0, |t| t.reward_escrow))
                        .sum::<u64>()
            })
            .sum::<u64>()
}
fn fuel(world: &SandboxState) -> f64 {
    world
        .transport
        .facilities
        .values()
        .map(|f| f.fuel_l)
        .sum::<f64>()
        + world
            .transport
            .vehicles
            .values()
            .map(|v| v.fuel_l)
            .sum::<f64>()
}
fn rent(world: &mut SandboxState, catalog: &Catalog, garage: u64) -> u64 {
    apply(
        world,
        catalog,
        TransportCommand::RentVehicle {
            model: VehicleModel::Pickup,
            facility_id: garage,
        },
    )
    .unwrap()
    .created_id
    .unwrap()
}
fn return_rental(
    world: &mut SandboxState,
    catalog: &Catalog,
    garage: u64,
    id: u64,
) -> Result<Outcome, SandboxError> {
    let quote = world.rental_return_quote(1, id)?;
    apply(
        world,
        catalog,
        TransportCommand::ReturnRental {
            vehicle_id: id,
            facility_id: garage,
            quoted_refund: quote.total_refund,
        },
    )
}
#[test]
fn rental_purchase_renewal_and_damaged_return_conserve_bond_money_and_fuel() {
    let (mut world, catalog, garage) = fixture();
    let credits = money(&world);
    let litres = fuel(&world);
    let wallet = world.living.accounts[&1].money;
    let offer = rental_offer(VehicleModel::Pickup);
    let id = rent(&mut world, &catalog, garage);
    assert_eq!(world.living.accounts[&1].money, wallet - offer.total);
    assert_eq!(world.transport.vehicles[&id].fuel_l, 10.);
    assert_eq!(
        world.transport.vehicles[&id]
            .rental
            .as_ref()
            .unwrap()
            .bond_escrow,
        offer.bond
    );
    assert_eq!(money(&world), credits);
    assert!((fuel(&world) - litres).abs() < 1e-6);
    apply(
        &mut world,
        &catalog,
        TransportCommand::RenewRental { vehicle_id: id },
    )
    .unwrap();
    assert_eq!(
        world.transport.vehicles[&id]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds,
        600.
    );
    assert_eq!(money(&world), credits);
    world.transport.vehicles.get_mut(&id).unwrap().condition = 0.8;
    let quote = world.rental_return_quote(1, id).unwrap();
    assert!(quote.damage_cost > 0);
    assert_eq!(quote.fuel_refund, 1500);
    let before = world.living.accounts[&1].money;
    return_rental(&mut world, &catalog, garage, id).unwrap();
    assert_eq!(world.living.accounts[&1].money, before + quote.total_refund);
    assert!(!world.living.units.contains_key(&id));
    assert!(!world.transport.vehicles.contains_key(&id));
    assert_eq!(money(&world), credits);
    assert!((fuel(&world) - litres).abs() < 1e-6);
    let snapshot = serde_json::to_value(&world).unwrap();
    assert!(
        apply(
            &mut world,
            &catalog,
            TransportCommand::ReturnRental {
                vehicle_id: id,
                facility_id: garage,
                quoted_refund: quote.total_refund
            }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), snapshot);
    world.validate_with_catalog(&catalog).unwrap();
}
#[test]
fn rental_expiry_stops_motion_without_repossessing_and_remote_renewal_keeps_identity() {
    let (mut world, catalog, garage) = fixture();
    let id = rent(&mut world, &catalog, garage);
    let origin = world.living.units[&id].position;
    let far = LocalFrame::new(origin).to_geo([500., 0., 500.]).unwrap();
    world.living.units.get_mut(&id).unwrap().position = far;
    world.transport.vehicles.get_mut(&id).unwrap().parking_brake = false;
    for _ in 0..60 {
        world.advance_transport(5.).unwrap();
    }
    assert_eq!(world.vehicle_speed_limit_m_s(id), Some(0.));
    assert_eq!(world.living.units[&id].position, far);
    let bond = world.transport.vehicles[&id]
        .rental
        .as_ref()
        .unwrap()
        .bond_escrow;
    let credits = money(&world);
    apply(
        &mut world,
        &catalog,
        TransportCommand::RenewRental { vehicle_id: id },
    )
    .unwrap();
    assert!(world.vehicle_speed_limit_m_s(id).unwrap() > 0.);
    assert_eq!(world.living.units[&id].position, far);
    assert_eq!(
        world.transport.vehicles[&id]
            .rental
            .as_ref()
            .unwrap()
            .bond_escrow,
        bond
    );
    assert_eq!(money(&world), credits);
    world.validate_with_catalog(&catalog).unwrap();
}
#[test]
fn rental_clocks_freeze_during_dormancy_and_survive_cold_region_restoration() {
    let (mut world, catalog, garage) = fixture();
    let id = rent(&mut world, &catalog, garage);
    world.advance_transport(5.).unwrap();
    world.set_player_dormant(1, true).unwrap();
    let rental = world.transport.vehicles[&id].rental.clone();
    for _ in 0..100 {
        world.advance_transport(5.).unwrap();
    }
    assert_eq!(world.transport.vehicles[&id].rental, rental);
    let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
    let mut restored = partition.assemble_all(&catalog).unwrap();
    assert_eq!(restored.transport.vehicles[&id].rental, rental);
    restored.set_player_dormant(1, false).unwrap();
    restored.advance_transport(5.).unwrap();
    assert_eq!(
        restored.transport.vehicles[&id]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds,
        290.
    );
    restored.validate_with_catalog(&catalog).unwrap();
}
#[test]
fn rental_admission_and_refund_denials_are_atomic_and_owner_private() {
    let (mut world, catalog, garage) = fixture();
    let id = rent(&mut world, &catalog, garage);
    for action in [
        TransportCommand::SellVehicle {
            vehicle_id: id,
            facility_id: garage,
        },
        TransportCommand::Upgrade {
            vehicle_id: id,
            facility_id: garage,
            engine: true,
        },
        TransportCommand::ReturnRental {
            vehicle_id: id,
            facility_id: garage,
            quoted_refund: 0,
        },
        TransportCommand::RentVehicle {
            model: VehicleModel::Bus,
            facility_id: garage,
        },
    ] {
        let before = serde_json::to_value(&world).unwrap();
        assert!(apply(&mut world, &catalog, action).is_err());
        assert_eq!(serde_json::to_value(&world).unwrap(), before);
    }
    let own = world.snapshot_for(1, 2000.).unwrap();
    own.transport.validate_replica(1, &own.living).unwrap();
    assert!(
        own.transport
            .vehicles
            .iter()
            .find(|v| v.unit_id == id)
            .unwrap()
            .rental
            .is_some()
    );
    let other = world.snapshot_for(2, 2000.).unwrap();
    assert!(other.transport.vehicles.iter().all(|v| v.unit_id != id));
    let public =
        serde_json::to_value(VehicleAppearance::from(&world.transport.vehicles[&id])).unwrap();
    assert!(public.get("rental").is_none());
    assert!(public.get("bond_escrow").is_none());
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        world
            .apply(
                2,
                Command::Transport {
                    action: TransportCommand::RenewRental { vehicle_id: id }
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
    // Deposit is exact authored bond, references and finite times are validated.
    for corruption in 0..4 {
        let mut damaged = world.clone();
        let r = damaged
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .rental
            .as_mut()
            .unwrap();
        match corruption {
            0 => r.bond_escrow += 1,
            1 => r.facility_id = u64::MAX,
            2 => r.remaining_seconds = 3601.,
            _ => r.remaining_seconds = f64::NAN,
        }
        assert!(damaged.validate_with_catalog(&catalog).is_err());
    }
}
#[test]
fn rental_refund_full_wallet_or_empty_issuer_preserves_vehicle_and_bond() {
    let (mut world, catalog, garage) = fixture();
    let id = rent(&mut world, &catalog, garage);
    for full_wallet in [false, true] {
        let mut copy = world.clone();
        if full_wallet {
            copy.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
        } else {
            copy.transport.treasury = 0;
        }
        let before = serde_json::to_value(&copy).unwrap();
        assert!(return_rental(&mut copy, &catalog, garage, id).is_err());
        assert_eq!(serde_json::to_value(&copy).unwrap(), before);
    }
    let before = serde_json::to_value(&world).unwrap();
    let position = world.living.units[&id].position;
    world.living.units.get_mut(&id).unwrap().position =
        LocalFrame::new(position).to_geo([30., 0., 0.]).unwrap();
    let outside = serde_json::to_value(&world).unwrap();
    assert!(return_rental(&mut world, &catalog, garage, id).is_err());
    assert_eq!(serde_json::to_value(&world).unwrap(), outside);
    world.living.units.get_mut(&id).unwrap().position = position;
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
}
#[test]
fn expired_rental_keeps_loaded_company_trip_until_renewal_and_physical_return() {
    let (mut world, catalog, garage) = fixture();
    let id = rent(&mut world, &catalog, garage);
    let act = |w: &mut SandboxState, c| apply(w, &catalog, TransportCommand::Company { action: c });
    act(
        &mut world,
        CompanyCommand::Create {
            name: "Rental fleet".into(),
        },
    )
    .unwrap();
    act(&mut world, CompanyCommand::Deposit { amount: 5000 }).unwrap();
    let driver = act(
        &mut world,
        CompanyCommand::HireDriver { name: "Ari".into() },
    )
    .unwrap()
    .created_id
    .unwrap();
    let stops: Vec<_> = world
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Terminal)
        .map(|f| f.id)
        .collect();
    let route = act(
        &mut world,
        CompanyCommand::CreateRoute {
            name: "Local service".into(),
            stops: stops.clone(),
            service: FleetService::Passengers { count: 1 },
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    act(
        &mut world,
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: id,
            route_id: route,
        },
    )
    .unwrap();
    act(
        &mut world,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    )
    .unwrap();
    world.living.units.get_mut(&id).unwrap().position =
        world.transport.facilities[&stops[0]].position;
    world.advance_transport(1.).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    world
        .transport
        .vehicles
        .get_mut(&id)
        .unwrap()
        .rental
        .as_mut()
        .unwrap()
        .remaining_seconds = 0.1;
    world.advance_transport(1.).unwrap();
    world.prepare_company_drivers(1.).unwrap();
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::RentalExpired
    );
    let held = world.transport.companies[&1].clone();
    let position = world.living.units[&id].position;
    for _ in 0..5 {
        world.advance_living(1., 0., &catalog).unwrap();
    }
    assert_eq!(world.transport.companies[&1], held);
    assert_eq!(world.living.units[&id].position, position);
    let before = money(&world);
    let trip = held.drivers[0].trip.clone();
    apply(
        &mut world,
        &catalog,
        TransportCommand::RenewRental { vehicle_id: id },
    )
    .unwrap();
    assert_eq!(world.transport.companies[&1].drivers[0].trip, trip);
    assert_eq!(money(&world), before);
    let accounting = world.transport.companies[&1].accounting_report();
    assert!(accounting.reconciled);
    assert_eq!(accounting.cash, held.cash);
    assert_eq!(
        accounting.totals[&CompanyCashCategory::OwnerPaidLease],
        rental_offer(VehicleModel::Pickup).fee
    );
    assert_eq!(
        accounting.owner_paid_operating,
        rental_offer(VehicleModel::Pickup).fee
    );
    assert!(accounting.personal_vehicle_used);
    act(&mut world, CompanyCommand::ReturnLoad { driver_id: driver }).unwrap();
    world.advance_transport(1.).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_none());
    act(
        &mut world,
        CompanyCommand::ReleaseVehicle { driver_id: driver },
    )
    .unwrap();
    world.living.units.get_mut(&id).unwrap().position =
        world.transport.facilities[&garage].position;
    return_rental(&mut world, &catalog, garage, id).unwrap();
    assert_eq!(money(&world), before);
    world.validate_with_catalog(&catalog).unwrap();
}
#[test]
fn exhausted_fuel_or_rental_capacity_rejects_purchase_without_escrow_or_new_body() {
    let (world, catalog, garage) = fixture();
    for scenario in 0..3 {
        let mut copy = world.clone();
        match scenario {
            0 => copy.transport.facilities.get_mut(&garage).unwrap().fuel_l = 9.,
            1 => copy.living.accounts.get_mut(&1).unwrap().money = 0,
            _ => copy.transport.treasury = MAX_MONEY,
        }
        let before = serde_json::to_value(&copy).unwrap();
        assert!(
            apply(
                &mut copy,
                &catalog,
                TransportCommand::RentVehicle {
                    model: VehicleModel::Pickup,
                    facility_id: garage
                }
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&copy).unwrap(), before);
    }
}
