//! Explicit RTS movement resumes parked personal vehicles through the ordinary
//! bounded vehicle simulation; group rejection must never partly release a car.
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64, u64) {
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let catalog = Catalog::builtin();
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
        .id;
    let starter = world
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    // An authored flat, empty test yard isolates brake semantics from the
    // settlement's initial traffic. No test repositions a body after ordering.
    let yard = LocalFrame::new(origin).to_geo([500., 0., -500.]).unwrap();
    world
        .transport
        .facilities
        .get_mut(&garage)
        .unwrap()
        .position = yard;
    world.set_player_position(1, yard).unwrap();
    world.living.units.get_mut(&starter).unwrap().position =
        LocalFrame::new(yard).to_geo([-20., 0., 0.]).unwrap();
    world.living.units.get_mut(&starter).unwrap().heading_deg = 0.;
    world.living.roads.clear();
    world.validate_with_catalog(&catalog).unwrap();
    (world, catalog, garage, starter)
}

fn transport(world: &mut SandboxState, catalog: &Catalog, action: TransportCommand) -> Outcome {
    world
        .apply(1, Command::Transport { action }, catalog)
        .unwrap()
}

fn company(world: &mut SandboxState, catalog: &Catalog, action: CompanyCommand) -> Outcome {
    transport(world, catalog, TransportCommand::Company { action })
}

fn rental(world: &mut SandboxState, catalog: &Catalog, garage: u64) -> u64 {
    transport(
        world,
        catalog,
        TransportCommand::RentVehicle {
            model: VehicleModel::Compact,
            facility_id: garage,
        },
    )
    .created_id
    .unwrap()
}

fn assign_company(world: &mut SandboxState, catalog: &Catalog, vehicle: u64) -> u64 {
    company(
        world,
        catalog,
        CompanyCommand::Create {
            name: "Yard Transit".into(),
        },
    );
    company(world, catalog, CompanyCommand::Deposit { amount: 5_000 });
    let driver = company(
        world,
        catalog,
        CompanyCommand::HireDriver {
            name: "Rowan".into(),
        },
    )
    .created_id
    .unwrap();
    let stops = world
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Terminal)
        .map(|f| f.id)
        .collect();
    let route = company(
        world,
        catalog,
        CompanyCommand::CreateRoute {
            name: "Local loop".into(),
            stops,
            service: FleetService::Passengers { count: 1 },
        },
    )
    .created_id
    .unwrap();
    company(
        world,
        catalog,
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: vehicle,
            route_id: route,
        },
    );
    driver
}

fn destination(world: &SandboxState, vehicle: u64) -> GeoPosition {
    LocalFrame::new(world.living.units[&vehicle].position)
        .to_geo([0., 0., -80.])
        .unwrap()
}

fn move_order(
    world: &mut SandboxState,
    catalog: &Catalog,
    ids: Vec<u64>,
    destination: GeoPosition,
) -> Result<Outcome, SandboxError> {
    world.apply(
        1,
        Command::Living {
            action: LivingCommand::MoveUnits {
                unit_ids: ids,
                destination,
            },
        },
        catalog,
    )
}

fn flat_step(world: &mut SandboxState, catalog: &Catalog) {
    world
        .advance_living_with_environment(0.1, 0., catalog, |_, _, mut candidate| {
            candidate.altitude_m = 100.;
            Some(candidate)
        })
        .unwrap();
}

#[test]
fn accepted_rts_move_resumes_exited_rented_and_company_released_vehicles_physically() {
    for parked_case in ["exited", "rented", "company released"] {
        let (mut world, catalog, garage, starter) = fixture();
        let vehicle = match parked_case {
            "exited" => {
                world
                    .set_player_position(1, world.living.units[&starter].position)
                    .unwrap();
                transport(
                    &mut world,
                    &catalog,
                    TransportCommand::EnterVehicle {
                        vehicle_id: starter,
                    },
                );
                transport(&mut world, &catalog, TransportCommand::ExitVehicle);
                starter
            }
            "rented" => rental(&mut world, &catalog, garage),
            _ => {
                let id = rental(&mut world, &catalog, garage);
                let driver = assign_company(&mut world, &catalog, id);
                company(
                    &mut world,
                    &catalog,
                    CompanyCommand::ReleaseVehicle { driver_id: driver },
                );
                assert!(!world.is_company_vehicle(id));
                id
            }
        };
        assert!(
            world.transport.vehicles[&vehicle].parking_brake,
            "{parked_case}"
        );
        let start = world.living.units[&vehicle].position;
        let target = destination(&world, vehicle);
        let mut expected_vehicle = world.transport.vehicles[&vehicle].clone();
        expected_vehicle.parking_brake = false;
        let money = world.living.accounts[&1].money;
        let company_before = world.transport.companies.clone();
        move_order(&mut world, &catalog, vec![vehicle], target).unwrap();
        assert_eq!(
            world.transport.vehicles[&vehicle], expected_vehicle,
            "the accepted order changes only the parking flag, not fuel, costs, cargo, dynamics or autopilot"
        );
        assert_eq!(
            world.living.units[&vehicle].position, start,
            "ordering never teleports"
        );
        assert_eq!(world.living.accounts[&1].money, money);
        assert_eq!(world.transport.companies, company_before);
        assert!(!world.living.units[&vehicle].path.is_empty());

        // Missing ground still stops the same released vehicle through the
        // real envelope resolver; a new move is not a terrain bypass.
        world
            .advance_living_with_environment(0.1, 0., &catalog, |_, _, _| None)
            .unwrap();
        assert_eq!(world.living.units[&vehicle].position, start);
        let before_odometer = world.transport.vehicles[&vehicle].odometer_m;
        for _ in 0..80 {
            let previous = world.living.units[&vehicle].position;
            flat_step(&mut world, &catalog);
            let moved = previous.surface_distance_m(world.living.units[&vehicle].position);
            assert!(
                moved <= 1.05,
                "bounded 0.1 s movement: {moved} m for {parked_case}"
            );
        }
        assert!(
            start.surface_distance_m(world.living.units[&vehicle].position) > 5.,
            "{parked_case} should move after explicit RTS intent"
        );
        assert!(world.transport.vehicles[&vehicle].odometer_m > before_odometer);
        assert!(world.transport.vehicles[&vehicle].fuel_l < expected_vehicle.fuel_l);
        world.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn rejected_mixed_groups_preserve_all_brakes_orders_and_authority() {
    let (mut world, catalog, garage, starter) = fixture();
    let rented = rental(&mut world, &catalog, garage);
    world
        .transport
        .vehicles
        .get_mut(&starter)
        .unwrap()
        .parking_brake = true;
    let foreign = world.living.accounts[&2].avatar_unit;
    let target = destination(&world, rented);
    let before = world.clone();
    for ids in [
        vec![rented, foreign],
        vec![foreign, rented],
        vec![rented, u64::MAX],
        vec![rented, rented],
    ] {
        assert!(move_order(&mut world, &catalog, ids, target).is_err());
        assert_eq!(
            world, before,
            "an invalid member cannot partly release a valid car"
        );
    }
    let too_far = LocalFrame::new(target).to_geo([3000., 0., 0.]).unwrap();
    assert!(move_order(&mut world, &catalog, vec![starter, rented], too_far).is_err());
    assert_eq!(world, before);

    assign_company(&mut world, &catalog, rented);
    let before = world.clone();
    for ids in [vec![starter, rented], vec![rented, starter]] {
        assert!(move_order(&mut world, &catalog, ids, target).is_err());
        assert_eq!(
            world, before,
            "company assignment rejects the entire group before releasing any brake"
        );
    }
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn an_rts_brake_release_does_not_bypass_fuel_condition_or_rental_expiry() {
    for held_by in ["fuel", "condition", "rental"] {
        let (mut world, catalog, garage, _) = fixture();
        let id = rental(&mut world, &catalog, garage);
        let vehicle = world.transport.vehicles.get_mut(&id).unwrap();
        match held_by {
            "fuel" => vehicle.fuel_l = 0.,
            "condition" => vehicle.condition = 0.05,
            _ => vehicle.rental.as_mut().unwrap().remaining_seconds = 0.,
        }
        let start = world.living.units[&id].position;
        let target = destination(&world, id);
        let odometer = world.transport.vehicles[&id].odometer_m;
        move_order(&mut world, &catalog, vec![id], target).unwrap();
        assert!(!world.transport.vehicles[&id].parking_brake);
        assert_eq!(world.transport.vehicles[&id].speed_limit_m_s(), 0.);
        for _ in 0..5 {
            flat_step(&mut world, &catalog);
        }
        assert!(
            start.surface_distance_m(world.living.units[&id].position) < 0.001,
            "{held_by} remains authoritative"
        );
        assert_eq!(world.transport.vehicles[&id].odometer_m, odometer);
        world.validate_with_catalog(&catalog).unwrap();
    }
}
