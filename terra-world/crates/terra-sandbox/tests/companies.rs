use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn fixture() -> (SandboxState, Catalog, u64, Vec<u64>) {
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
        .position;
    world.set_player_position(1, garage).unwrap();
    let vehicle = world
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    let frame = LocalFrame::new(origin);
    let mut stops = Vec::new();
    for (index, x) in [500., 650., 800.].into_iter().enumerate() {
        let id = world.transport.next_id;
        world.transport.next_id += 1;
        world.transport.facilities.insert(
            id,
            TransportFacility {
                id,
                name: format!("Test stop {index}"),
                kind: TransportFacilityKind::Terminal,
                position: frame.to_geo([x, 0., -500.]).unwrap(),
                linked_site: None,
                passengers_waiting: 30,
                fuel_l: 0.,
            },
        );
        stops.push(id);
    }
    world.living.units.get_mut(&vehicle).unwrap().position =
        world.transport.facilities[&stops[0]].position;
    world.living.roads.clear();
    world.validate_with_catalog(&catalog).unwrap();
    (world, catalog, vehicle, stops)
}

fn company(
    world: &mut SandboxState,
    catalog: &Catalog,
    command: CompanyCommand,
) -> Result<Outcome, SandboxError> {
    world.apply(
        1,
        Command::Transport {
            action: TransportCommand::Company { action: command },
        },
        catalog,
    )
}

fn setup(
    world: &mut SandboxState,
    catalog: &Catalog,
    vehicle: u64,
    stops: Vec<u64>,
    service: FleetService,
) -> u64 {
    company(
        world,
        catalog,
        CompanyCommand::Create {
            name: "Valley Transit".into(),
        },
    )
    .unwrap();
    company(world, catalog, CompanyCommand::Deposit { amount: 10_000 }).unwrap();
    let driver = company(
        world,
        catalog,
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    let route = company(
        world,
        catalog,
        CompanyCommand::CreateRoute {
            name: "Valley loop".into(),
            stops,
            service,
        },
    )
    .unwrap()
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
    )
    .unwrap();
    company(
        world,
        catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    )
    .unwrap();
    driver
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
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| a.reward_escrow)
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .map(|company| {
                company.cash
                    + company.departed_wages
                    + company
                        .drivers
                        .iter()
                        .map(|driver| driver.wages_earned)
                        .sum::<u64>()
                    + company
                        .drivers
                        .iter()
                        .filter_map(|driver| driver.trip.as_ref())
                        .map(|trip| trip.reward_escrow)
                        .sum::<u64>()
            })
            .sum::<u64>()
}

fn passengers(world: &SandboxState) -> u64 {
    world
        .transport
        .facilities
        .values()
        .map(|f| u64::from(f.passengers_waiting))
        .sum::<u64>()
        + world
            .transport
            .jobs
            .values()
            .filter(|job| job.assignment.is_some())
            .map(|job| u64::from(job.passengers))
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .flat_map(|company| &company.drivers)
            .filter_map(|driver| driver.trip.as_ref())
            .map(|trip| match trip.service {
                FleetService::Passengers { count } => u64::from(count),
                _ => 0,
            })
            .sum::<u64>()
}

fn roadside_command(quote: CompanyRoadsideQuote) -> CompanyCommand {
    CompanyCommand::RoadsideService {
        vehicle_id: quote.vehicle_id,
        facility_id: quote.facility_id,
        litres: quote.litres,
        repair: quote.repair,
        quoted_total: quote.total_cost,
    }
}

#[test]
fn roadside_service_recovers_loaded_fuel_and_repair_failures_without_losing_commitments() {
    for freight in [false, true] {
        for repair in [false, true] {
            let (mut world, catalog, vehicle, stops) = fixture();
            let service = if freight {
                let source = world
                    .living
                    .producers
                    .values()
                    .find(|s| s.kind == ProductionKind::Woodlot)
                    .unwrap()
                    .id;
                let destination = world
                    .living
                    .producers
                    .values()
                    .find(|s| s.kind == ProductionKind::Sawmill)
                    .unwrap()
                    .id;
                world
                    .transport
                    .facilities
                    .get_mut(&stops[0])
                    .unwrap()
                    .linked_site = Some(source);
                world
                    .transport
                    .facilities
                    .get_mut(&stops[1])
                    .unwrap()
                    .linked_site = Some(destination);
                world
                    .living
                    .producers
                    .get_mut(&source)
                    .unwrap()
                    .stock
                    .insert("wood".into(), 100);
                world
                    .living
                    .producers
                    .get_mut(&destination)
                    .unwrap()
                    .stock
                    .remove("wood");
                FleetService::Freight {
                    item: "wood".into(),
                    amount: 8,
                }
            } else {
                FleetService::Passengers { count: 2 }
            };
            world.living.deliveries.clear();
            for producer in world.living.producers.values_mut() {
                producer.enabled = false;
            }
            let driver = setup(&mut world, &catalog, vehicle, stops[..2].to_vec(), service);
            company(
                &mut world,
                &catalog,
                CompanyCommand::Deposit { amount: 5_000 },
            )
            .unwrap();
            for _ in 0..8 {
                world.advance_living(0.1, 0., &catalog).unwrap();
            }
            assert!(world.transport.companies[&1].drivers[0].trip.is_some());
            if repair {
                company(
                    &mut world,
                    &catalog,
                    CompanyCommand::SetDriverRunning {
                        driver_id: driver,
                        running: false,
                    },
                )
                .unwrap();
                world
                    .transport
                    .vehicles
                    .get_mut(&vehicle)
                    .unwrap()
                    .condition = 0.04;
            } else {
                world.transport.vehicles.get_mut(&vehicle).unwrap().fuel_l = 0.;
            }
            world.advance_living(0.1, 0., &catalog).unwrap();
            assert_eq!(
                world.transport.companies[&1].drivers[0].status,
                if repair {
                    FleetStatus::NeedsRepair
                } else {
                    FleetStatus::NeedsFuel
                }
            );
            let position = world.living.units[&vehicle].position;
            let garage = world
                .transport
                .facilities
                .values()
                .find(|f| f.kind == TransportFacilityKind::Garage)
                .unwrap()
                .id;
            assert!(
                position.surface_distance_m(world.transport.facilities[&garage].position) > 100.
            );
            world
                .set_player_position(1, LocalFrame::new(position).to_geo([0., 0., 10.]).unwrap())
                .unwrap();
            let quote = world
                .company_roadside_quote(1, vehicle, garage, if repair { 0 } else { 10 }, repair)
                .unwrap();
            assert_eq!(quote.callout_cost, 1_500);
            assert_eq!(
                quote.total_cost,
                quote.callout_cost + quote.fuel_cost + quote.repair_cost
            );
            let before_company = world.transport.companies[&1].clone();
            let before_vehicle = world.transport.vehicles[&vehicle].clone();
            let before_body = world.living.units[&vehicle].clone();
            let before_fuel = world.transport.facilities[&garage].fuel_l + before_vehicle.fuel_l;
            let before_money = money(&world);
            let before_goods = goods(&world, "wood");
            let before_people = passengers(&world);
            let treasury = world.transport.treasury;
            let command = Command::Transport {
                action: TransportCommand::Company {
                    action: roadside_command(quote),
                },
            };
            let command: Command =
                serde_json::from_slice(&serde_json::to_vec(&command).unwrap()).unwrap();
            world.apply(1, command, &catalog).unwrap();
            assert_eq!(
                world.living.units[&vehicle], before_body,
                "roadside support must not move or reorder the vehicle"
            );
            assert_eq!(
                world.transport.companies[&1].drivers,
                before_company.drivers
            );
            assert_eq!(world.transport.companies[&1].routes, before_company.routes);
            assert_eq!(
                world.transport.companies[&1].cash,
                before_company.cash - quote.total_cost
            );
            assert_eq!(world.transport.treasury, treasury + quote.total_cost);
            let mut expected_vehicle = before_vehicle;
            expected_vehicle.fuel_l += f64::from(quote.litres);
            if repair {
                expected_vehicle.condition = 1.;
            }
            assert_eq!(world.transport.vehicles[&vehicle], expected_vehicle);
            assert!(
                (world.transport.facilities[&garage].fuel_l + expected_vehicle.fuel_l
                    - before_fuel)
                    .abs()
                    < 1e-8
            );
            assert_eq!(money(&world), before_money);
            assert_eq!(goods(&world, "wood"), before_goods);
            assert_eq!(passengers(&world), before_people);
            world = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
            world.validate_with_catalog(&catalog).unwrap();
            for _ in 0..400 {
                world.advance_living(0.1, 0., &catalog).unwrap();
                assert_eq!(money(&world), before_money);
                assert_eq!(goods(&world, "wood"), before_goods);
                assert_eq!(passengers(&world), before_people);
                if world.transport.companies[&1].drivers[0].deliveries_completed == 1 {
                    break;
                }
            }
            assert_eq!(
                world.transport.companies[&1].drivers[0].deliveries_completed,
                1
            );
            assert!(world.transport.companies[&1].drivers[0].trip.is_none());
            if repair {
                assert_eq!(
                    world.transport.companies[&1].drivers[0].status,
                    FleetStatus::Paused
                );
            }
            world.validate_with_catalog(&catalog).unwrap();
        }
    }
}

#[test]
fn roadside_quote_ownership_reach_funds_motion_and_source_denials_are_atomic() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    world.transport.vehicles.get_mut(&vehicle).unwrap().fuel_l = 0.;
    world
        .transport
        .vehicles
        .get_mut(&vehicle)
        .unwrap()
        .condition = 0.5;
    world.advance_living(0.1, 0., &catalog).unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    world
        .set_player_position(2, world.transport.facilities[&garage].position)
        .unwrap();
    world
        .apply(
            2,
            Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::Create {
                        name: "Other company".into(),
                    },
                },
            },
            &catalog,
        )
        .unwrap();
    let position = world.living.units[&vehicle].position;
    world
        .set_player_position(1, LocalFrame::new(position).to_geo([0., 0., 10.]).unwrap())
        .unwrap();
    let quote = world
        .company_roadside_quote(1, vehicle, garage, 10, true)
        .unwrap();
    assert!(world.transport.companies[&1].cash >= quote.total_cost);
    for scenario in 0..10 {
        let mut candidate = world.clone();
        let mut offer = quote;
        let mut owner = 1;
        match scenario {
            0 => candidate
                .set_player_position(1, LocalFrame::new(position).to_geo([0., 30., 0.]).unwrap())
                .unwrap(),
            1 => {
                candidate
                    .living
                    .units
                    .get_mut(&vehicle)
                    .unwrap()
                    .velocity_enu_m_s = [1., 0., 0.]
            }
            2 => {
                let amount = candidate.transport.companies[&1].cash;
                company(
                    &mut candidate,
                    &catalog,
                    CompanyCommand::Withdraw { amount },
                )
                .unwrap();
            }
            3 => owner = 2,
            4 => offer.total_cost -= 1,
            5 => {
                candidate
                    .transport
                    .vehicles
                    .get_mut(&vehicle)
                    .unwrap()
                    .condition = 0.4
            }
            6 => {
                candidate
                    .transport
                    .facilities
                    .get_mut(&garage)
                    .unwrap()
                    .fuel_l = 0.
            }
            7 => offer.litres = 500,
            8 => candidate.set_player_dormant(1, true).unwrap(),
            9 => candidate.transport.treasury = MAX_MONEY,
            _ => unreachable!(),
        }
        let before = candidate.clone();
        assert!(
            candidate
                .apply(
                    owner,
                    Command::Transport {
                        action: TransportCommand::Company {
                            action: roadside_command(offer)
                        }
                    },
                    &catalog
                )
                .is_err(),
            "scenario {scenario}"
        );
        assert_eq!(
            candidate, before,
            "scenario {scenario} changed a rejected transaction"
        );
    }
    assert!(
        world
            .company_roadside_quote(1, vehicle, stops[0], 10, true)
            .is_err()
    );
    assert!(
        world
            .company_roadside_quote(1, vehicle, garage, 0, false)
            .is_err()
    );
    company(&mut world, &catalog, roadside_command(quote)).unwrap();
    let before_replay = world.clone();
    assert!(
        company(&mut world, &catalog, roadside_command(quote)).is_err(),
        "a stale repair quote cannot charge twice"
    );
    assert_eq!(world, before_replay);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn company_runs_actual_physical_passenger_route_and_conserves_money_and_people() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let initial_money = money(&world);
    let initial_people = passengers(&world);
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 2 },
    );
    let original = world.living.units[&vehicle].position;
    for _ in 0..400 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        assert_eq!(money(&world), initial_money);
        assert_eq!(passengers(&world), initial_people);
        if world.transport.companies[&1].drivers[0].deliveries_completed > 0 {
            break;
        }
    }
    let company_state = &world.transport.companies[&1];
    assert_eq!(company_state.drivers[0].deliveries_completed, 1);
    assert!(company_state.revenue > 0);
    let accounting = company_state.accounting_report();
    assert!(accounting.reconciled);
    assert_eq!(
        accounting.totals[&CompanyCashCategory::PublicFares],
        company_state.revenue
    );
    assert_eq!(
        accounting.totals[&CompanyCashCategory::Payroll],
        company_state.payroll_paid()
    );
    assert_eq!(
        accounting.operating.net(),
        company_state.revenue as i64
            - company_state.payroll_paid() as i64
            - COMPANY_HIRE_PRICE as i64
    );
    assert!(company_state.payroll_paid() > 0);
    assert!(original.surface_distance_m(world.living.units[&vehicle].position) > 100.);
    assert!(world.transport.vehicles[&vehicle].odometer_m > 100.);
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: false,
        },
    )
    .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::ReleaseVehicle { driver_id: driver },
    )
    .unwrap();
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn loaded_pause_finishes_delivery_rejects_takeover_and_survives_json_resume() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 2 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: false,
        },
    )
    .unwrap();
    let snapshot = world.clone();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::ReleaseVehicle { driver_id: driver }
        )
        .is_err()
    );
    for action in [
        Command::Living {
            action: LivingCommand::Possess { unit_id: vehicle },
        },
        Command::Living {
            action: LivingCommand::SelectActive { unit_id: vehicle },
        },
        Command::Living {
            action: LivingCommand::StopUnits {
                unit_ids: vec![vehicle],
            },
        },
        Command::Living {
            action: LivingCommand::MoveUnits {
                unit_ids: vec![vehicle],
                destination: world.transport.facilities[&stops[1]].position,
            },
        },
        Command::Transport {
            action: TransportCommand::EnterVehicle {
                vehicle_id: vehicle,
            },
        },
        Command::Transport {
            action: TransportCommand::SetAutopilot {
                vehicle_id: vehicle,
                enabled: false,
                destination: None,
            },
        },
    ] {
        assert!(world.apply(1, action, &catalog).is_err());
        assert_eq!(world, snapshot);
    }
    let encoded = serde_json::to_vec(&world).unwrap();
    let mut restored: SandboxState = serde_json::from_slice(&encoded).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    for _ in 0..400 {
        restored.advance_living(0.1, 0., &catalog).unwrap();
        if restored.transport.companies[&1].drivers[0].trip.is_none() {
            break;
        }
    }
    assert_eq!(
        restored.transport.companies[&1].drivers[0].deliveries_completed,
        1
    );
    assert_eq!(
        restored.transport.companies[&1].drivers[0].status,
        FleetStatus::Paused
    );
    let settled = restored.transport.companies[&1].clone();
    restored.advance_living(1., 0., &catalog).unwrap();
    assert_eq!(restored.transport.companies[&1], settled);
    company(
        &mut restored,
        &catalog,
        CompanyCommand::ReleaseVehicle { driver_id: driver },
    )
    .unwrap();
}

#[test]
fn zero_payroll_prevents_motion_and_deposit_resumes_without_minting_wages() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    let cash = world.transport.companies[&1].cash;
    company(
        &mut world,
        &catalog,
        CompanyCommand::Withdraw { amount: cash },
    )
    .unwrap();
    let original = world.living.units[&vehicle].position;
    let before = money(&world);
    world.advance_living(2., 0., &catalog).unwrap();
    assert_eq!(world.living.units[&vehicle].position, original);
    assert_eq!(world.transport.companies[&1].drivers[0].wages_earned, 0);
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::WaitingCompanyFunds
    );
    company(&mut world, &catalog, CompanyCommand::Deposit { amount: 10 }).unwrap();
    world.advance_living(2., 0., &catalog).unwrap();
    assert!(original.surface_distance_m(world.living.units[&vehicle].position) > 1.);
    assert_eq!(world.transport.companies[&1].drivers[0].wages_earned, 4);
    assert_eq!(money(&world), before);
}

#[test]
fn fractional_payroll_is_independent_of_frame_chunking_and_survives_reassignment() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    let mut split = world.clone();
    world.prepare_company_drivers(1.).unwrap();
    for _ in 0..10 {
        split.prepare_company_drivers(0.1).unwrap();
    }
    assert_eq!(
        world.transport.companies[&1].payroll_paid(),
        split.transport.companies[&1].payroll_paid()
    );
    assert!(
        (world.transport.companies[&1].drivers[0].wage_fraction
            - split.transport.companies[&1].drivers[0].wage_fraction)
            .abs()
            < 1e-9
    );
    let driver_id = split.transport.companies[&1].drivers[0].id;
    let route_id = split.transport.companies[&1].routes[0].id;
    company(
        &mut split,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id,
            running: false,
        },
    )
    .unwrap();
    for _ in 0..400 {
        split.advance_living(0.13, 0., &catalog).unwrap();
        if split.transport.companies[&1].drivers[0].trip.is_none() {
            break;
        }
    }
    let fraction = split.transport.companies[&1].drivers[0].wage_fraction;
    let wages = split.transport.companies[&1].drivers[0].wages_earned;
    company(
        &mut split,
        &catalog,
        CompanyCommand::ReleaseVehicle { driver_id },
    )
    .unwrap();
    company(
        &mut split,
        &catalog,
        CompanyCommand::Assign {
            driver_id,
            vehicle_id: vehicle,
            route_id,
        },
    )
    .unwrap();
    assert_eq!(
        split.transport.companies[&1].drivers[0].wage_fraction,
        fraction
    );
    assert_eq!(split.transport.companies[&1].drivers[0].wages_earned, wages);
}

#[test]
fn company_private_replica_rejects_another_owner_and_legacy_saves_add_no_company() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    let view = world.snapshot_for(1, 2000.).unwrap();
    view.transport.validate_replica(1, &view.living).unwrap();
    assert!(view.transport.company.is_some());
    let other = world.snapshot_for(2, 2000.).unwrap();
    assert!(other.transport.company.is_none());
    let mut corrupt = other.transport.clone();
    corrupt.company = view.transport.company;
    assert!(corrupt.validate_replica(2, &other.living).is_err());
    let mut legacy = serde_json::to_value(&world).unwrap();
    legacy["transport"]
        .as_object_mut()
        .unwrap()
        .remove("companies");
    // Unassigning is not needed in a genuinely old save: the absent company
    // leaves an ordinary empty stopped vehicle. This fixture has no active load.
    let restored: SandboxState = serde_json::from_value(legacy).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    assert!(restored.transport.companies.is_empty());
}

#[test]
fn dormant_owner_freezes_employee_payroll_and_loads_without_catchup() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    world.set_player_dormant(1, true).unwrap();
    let original = world.living.units[&vehicle].position;
    let company_cash = world.transport.companies[&1].cash;
    let physical_age = world.transport.elapsed_seconds
        - world.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .pickup_elapsed_seconds;
    for _ in 0..20 {
        world.advance_living(5., 100., &catalog).unwrap();
    }
    assert_eq!(world.living.units[&vehicle].position, original);
    assert_eq!(world.transport.companies[&1].cash, company_cash);
    let age = world.transport.elapsed_seconds
        - world.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .pickup_elapsed_seconds;
    assert!((age - physical_age).abs() < 1e-8);
    world.set_player_dormant(1, false).unwrap();
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(original.surface_distance_m(world.living.units[&vehicle].position) < 5.);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn invalid_company_mutations_are_atomic_and_route_names_and_capacity_are_bounded() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    for command in [
        CompanyCommand::Withdraw { amount: MAX_MONEY },
        CompanyCommand::Deposit { amount: 0 },
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
        CompanyCommand::CreateRoute {
            name: "bad".into(),
            stops: vec![stops[0], stops[0]],
            service: FleetService::Passengers { count: 1 },
        },
        CompanyCommand::CreateRoute {
            name: "bad".into(),
            stops: stops[..2].to_vec(),
            service: FleetService::Passengers { count: 0 },
        },
        CompanyCommand::DeleteRoute {
            route_id: world.transport.companies[&1].routes[0].id,
        },
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: vehicle,
            route_id: world.transport.companies[&1].routes[0].id,
        },
    ] {
        let before = world.clone();
        assert!(company(&mut world, &catalog, command).is_err());
        assert_eq!(world, before);
    }
}

fn goods(world: &SandboxState, item: &str) -> u64 {
    let count = |inventory: &Inventory| u64::from(inventory.get(item).copied().unwrap_or(0));
    world
        .living
        .producers
        .values()
        .map(|s| count(&s.stock))
        .sum::<u64>()
        + world
            .living
            .shops
            .values()
            .map(|s| count(&s.stock))
            .sum::<u64>()
        + world
            .transport
            .vehicles
            .values()
            .map(|v| count(&v.cargo))
            .sum::<u64>()
        + world
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| count(&a.reserved_cargo))
            .sum::<u64>()
        + world
            .living
            .deliveries
            .iter()
            .filter(|d| d.item == item && d.phase != DeliveryPhase::ToPickup)
            .map(|d| u64::from(d.amount))
            .sum::<u64>()
}

#[test]
fn freight_follows_ordered_waypoint_delivers_real_stock_and_stops_at_finite_demand() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let source = world
        .living
        .producers
        .values()
        .find(|site| site.kind == ProductionKind::Woodlot)
        .unwrap()
        .id;
    let destination = world
        .living
        .producers
        .values()
        .find(|site| site.kind == ProductionKind::Sawmill)
        .unwrap()
        .id;
    world
        .transport
        .facilities
        .get_mut(&stops[0])
        .unwrap()
        .linked_site = Some(source);
    world
        .transport
        .facilities
        .get_mut(&stops[2])
        .unwrap()
        .linked_site = Some(destination);
    world
        .living
        .producers
        .get_mut(&source)
        .unwrap()
        .stock
        .insert("wood".into(), 200);
    world
        .living
        .producers
        .get_mut(&destination)
        .unwrap()
        .stock
        .remove("wood");
    // Disable unrelated couriers/production so the assertion isolates the
    // actual fleet stock transfer. Physical vehicle motion still runs normally.
    world.living.deliveries.clear();
    for site in world.living.producers.values_mut() {
        site.enabled = false;
    }
    let target = world.fleet_destination_target(stops[2], "wood");
    assert!(target > 0);
    let money_before = money(&world);
    let goods_before = goods(&world, "wood");
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops.clone(),
        FleetService::Freight {
            item: "wood".into(),
            amount: 40,
        },
    );
    let mut saw_middle_stop = false;
    for _ in 0..3000 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        saw_middle_stop |= world.transport.companies[&1].drivers[0].next_stop == 2;
        assert_eq!(money(&world), money_before);
        assert_eq!(goods(&world, "wood"), goods_before);
        if world.transport.companies[&1].drivers[0].status == FleetStatus::WaitingDemand {
            break;
        }
    }
    assert!(saw_middle_stop);
    assert_eq!(
        world.living.producers[&destination]
            .stock
            .get("wood")
            .copied()
            .unwrap_or(0),
        target
    );
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::WaitingDemand
    );
    assert!(world.transport.vehicles[&vehicle].cargo.is_empty());
    let cash = world.transport.companies[&1].cash;
    for _ in 0..20 {
        world.advance_living(0.1, 0., &catalog).unwrap();
    }
    assert_eq!(
        world.transport.companies[&1].cash, cash,
        "waiting demand charges no wages"
    );
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn blocked_route_stops_payroll_with_load_intact_until_explicit_resume() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    for _ in 0..40 {
        world
            .advance_living_with_environment(1., 0., &catalog, |_, _, _| None)
            .unwrap();
    }
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::RouteBlocked
    );
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    let cash = world.transport.companies[&1].cash;
    world.advance_living(1., 0., &catalog).unwrap();
    assert_eq!(world.transport.companies[&1].cash, cash);
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    )
    .unwrap();
    world.advance_living(1., 0., &catalog).unwrap();
    assert!(world.transport.companies[&1].cash < cash);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn cold_regional_company_restores_passenger_and_freight_escrow_without_offline_payroll() {
    for freight in [false, true] {
        let (mut world, catalog, vehicle, stops) = fixture();
        let frame = LocalFrame::new(world.living.origin.unwrap());
        for (index, stop) in stops.iter().enumerate() {
            world.transport.facilities.get_mut(stop).unwrap().position = frame
                .to_geo([20_000. + index as f64 * 150., 0., -500.])
                .unwrap();
        }
        world.living.units.get_mut(&vehicle).unwrap().position =
            world.transport.facilities[&stops[0]].position;
        let service = if freight {
            let source = world
                .living
                .producers
                .values()
                .find(|site| site.kind == ProductionKind::Woodlot)
                .unwrap()
                .id;
            let destination = world
                .living
                .producers
                .values()
                .find(|site| site.kind == ProductionKind::Sawmill)
                .unwrap()
                .id;
            world
                .transport
                .facilities
                .get_mut(&stops[0])
                .unwrap()
                .linked_site = Some(source);
            world
                .transport
                .facilities
                .get_mut(&stops[1])
                .unwrap()
                .linked_site = Some(destination);
            world
                .living
                .producers
                .get_mut(&source)
                .unwrap()
                .stock
                .insert("wood".into(), 100);
            world
                .living
                .producers
                .get_mut(&destination)
                .unwrap()
                .stock
                .remove("wood");
            FleetService::Freight {
                item: "wood".into(),
                amount: 8,
            }
        } else {
            FleetService::Passengers { count: 2 }
        };
        setup(&mut world, &catalog, vehicle, stops[..2].to_vec(), service);
        world.advance_living(0.1, 0., &catalog).unwrap();
        let original = world.transport.companies[&1].clone();
        let original_vehicle = world.transport.vehicles[&vehicle].clone();
        assert!(original.drivers[0].trip.is_some());
        world.set_player_dormant(1, true).unwrap();
        let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
        let mut working = partition
            .working_set(&Default::default(), &catalog)
            .unwrap();
        assert!(
            !working.state.living.units.contains_key(&vehicle),
            "offline fleet body must be genuinely cold"
        );
        for _ in 0..400 {
            working.state.advance_living(5., 0., &catalog).unwrap();
        }
        let merged = partition
            .merge_active(&working.state, &working.active_regions, &catalog)
            .unwrap();
        let mut restored = merged.assemble_all(&catalog).unwrap();
        let current = &restored.transport.companies[&1];
        assert_eq!(current.cash, original.cash);
        assert_eq!(current.payroll_paid(), original.payroll_paid());
        assert_eq!(current.revenue, original.revenue);
        let trip = current.drivers[0].trip.as_ref().unwrap();
        assert_eq!(
            trip.reward_escrow,
            original.drivers[0].trip.as_ref().unwrap().reward_escrow
        );
        assert_eq!(
            trip.service,
            original.drivers[0].trip.as_ref().unwrap().service
        );
        assert_eq!(
            restored.transport.vehicles[&vehicle].cargo,
            original_vehicle.cargo
        );
        assert!((restored.transport.elapsed_seconds - trip.pickup_elapsed_seconds).abs() < 1e-6);
        restored.set_player_dormant(1, false).unwrap();
        restored.advance_living(0.1, 0., &catalog).unwrap();
        assert!(
            restored.transport.vehicles[&vehicle].odometer_m < original_vehicle.odometer_m + 2.
        );
        assert_eq!(
            restored.transport.companies[&1].drivers[0].deliveries_completed,
            0
        );
        restored.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn driver_paused_before_disconnect_stays_parked_after_hydration() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: false,
        },
    )
    .unwrap();
    let original = world.transport.companies[&1].clone();
    let position = world.living.units[&vehicle].position;
    world.set_player_dormant(1, true).unwrap();
    let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
    let mut restored = partition.assemble_all(&catalog).unwrap();
    restored.advance_living(5., 0., &catalog).unwrap();
    restored.set_player_dormant(1, false).unwrap();
    restored.advance_living(5., 0., &catalog).unwrap();
    assert_eq!(restored.transport.companies[&1], original);
    assert_eq!(restored.living.units[&vehicle].position, position);
    assert_eq!(
        restored.transport.companies[&1].drivers[0].status,
        FleetStatus::Paused
    );
}

#[test]
fn issuer_starvation_preserves_source_and_queues_without_hidden_payroll() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 2 },
    );
    // Trusted scenario accounting moves issuer capital into the owner's wallet;
    // no player command may set rewards or mint a replenishment.
    let issuer = world.transport.treasury;
    world.transport.treasury = 0;
    world.living.accounts.get_mut(&1).unwrap().money += issuer;
    let before = money(&world);
    let people = passengers(&world);
    let cash = world.transport.companies[&1].cash;
    for _ in 0..5 {
        world.advance_living(1., 0., &catalog).unwrap();
    }
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::WaitingIssuerFunds
    );
    assert!(world.transport.companies[&1].drivers[0].trip.is_none());
    assert_eq!(world.transport.companies[&1].cash, cash);
    assert_eq!(money(&world), before);
    assert_eq!(passengers(&world), people);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn full_mixed_item_producer_preserves_fleet_cargo_and_fare_until_capacity_is_freed() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let source = world
        .living
        .producers
        .values()
        .find(|site| site.kind == ProductionKind::Woodlot)
        .unwrap()
        .id;
    let destination = world
        .living
        .producers
        .values()
        .find(|site| site.kind == ProductionKind::Sawmill)
        .unwrap()
        .id;
    world
        .transport
        .facilities
        .get_mut(&stops[0])
        .unwrap()
        .linked_site = Some(source);
    world
        .transport
        .facilities
        .get_mut(&stops[1])
        .unwrap()
        .linked_site = Some(destination);
    world
        .living
        .producers
        .get_mut(&source)
        .unwrap()
        .stock
        .insert("wood".into(), 100);
    world
        .living
        .producers
        .get_mut(&destination)
        .unwrap()
        .stock
        .clear();
    for site in world.living.producers.values_mut() {
        site.enabled = false;
    }
    world.living.deliveries.clear();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Freight {
            item: "wood".into(),
            amount: 8,
        },
    );
    world.advance_living(0.1, 0., &catalog).unwrap();
    let escrow = world.transport.companies[&1].drivers[0]
        .trip
        .as_ref()
        .unwrap()
        .reward_escrow;
    let load = world.transport.vehicles[&vehicle].cargo.clone();
    world
        .living
        .producers
        .get_mut(&destination)
        .unwrap()
        .stock
        .insert("stone".into(), 1999);
    let credits = money(&world);
    let wood = goods(&world, "wood");
    for _ in 0..400 {
        world.advance_living(0.1, 0., &catalog).unwrap();
        if world.transport.companies[&1].drivers[0].status == FleetStatus::DestinationFull {
            break;
        }
    }
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::DestinationFull
    );
    assert_eq!(
        world.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .reward_escrow,
        escrow
    );
    assert_eq!(world.transport.vehicles[&vehicle].cargo, load);
    assert_eq!(world.transport.companies[&1].revenue, 0);
    assert_eq!(
        world.living.producers[&destination]
            .stock
            .values()
            .sum::<u32>(),
        1999
    );
    assert_eq!(money(&world), credits);
    assert_eq!(goods(&world, "wood"), wood);
    // Trusted unloading into another inventory creates room; the waiting
    // physical delivery retries without taking or paying a second load.
    world
        .living
        .producers
        .get_mut(&destination)
        .unwrap()
        .stock
        .insert("stone".into(), 1990);
    world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("stone".into(), 9);
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(world.transport.vehicles[&vehicle].cargo.is_empty());
    assert_eq!(world.transport.companies[&1].revenue, escrow);
    assert_eq!(
        world.transport.companies[&1].drivers[0].deliveries_completed,
        1
    );
    assert_eq!(
        world.living.producers[&destination]
            .stock
            .values()
            .sum::<u32>(),
        1998
    );
    assert_eq!(money(&world), credits);
    assert_eq!(goods(&world, "wood"), wood);
    world.validate_with_catalog(&catalog).unwrap();
}

fn recovery_fixture(freight: bool) -> (SandboxState, Catalog, u64, u64, Vec<u64>) {
    let (mut world, catalog, vehicle, stops) = fixture();
    let service = if freight {
        let source = world
            .living
            .producers
            .values()
            .find(|s| s.kind == ProductionKind::Woodlot)
            .unwrap()
            .id;
        let destination = world
            .living
            .producers
            .values()
            .find(|s| s.kind == ProductionKind::Sawmill)
            .unwrap()
            .id;
        world
            .transport
            .facilities
            .get_mut(&stops[0])
            .unwrap()
            .linked_site = Some(source);
        world
            .transport
            .facilities
            .get_mut(&stops[1])
            .unwrap()
            .linked_site = Some(destination);
        world
            .living
            .producers
            .get_mut(&destination)
            .unwrap()
            .stock
            .remove("wood");
        FleetService::Freight {
            item: "wood".into(),
            amount: 8,
        }
    } else {
        FleetService::Passengers { count: 2 }
    };
    let driver = setup(&mut world, &catalog, vehicle, stops[..2].to_vec(), service);
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    (world, catalog, vehicle, driver, stops)
}

#[test]
fn held_loaded_company_returns_physically_and_refunds_exactly_once_after_restore() {
    for freight in [false, true] {
        let (mut world, catalog, vehicle, driver, stops) = recovery_fixture(freight);
        // Wait for physical departure: bounded acceleration and the initial
        // turn no longer grant immediate waypoint cruising speed.
        for _ in 0..300 {
            world.advance_living(0.1, 0., &catalog).unwrap();
            if world.living.units[&vehicle]
                .position
                .surface_distance_m(world.transport.facilities[&stops[0]].position)
                > 35.
            {
                break;
            }
        }
        assert!(
            world.living.units[&vehicle]
                .position
                .surface_distance_m(world.transport.facilities[&stops[0]].position)
                > 30.
        );
        company(
            &mut world,
            &catalog,
            CompanyCommand::HoldVehicle { driver_id: driver },
        )
        .unwrap();
        let position = world.living.units[&vehicle].position;
        let before = world.transport.companies[&1].clone();
        for _ in 0..20 {
            world.advance_living(1., 0., &catalog).unwrap();
        }
        assert_eq!(world.living.units[&vehicle].position, position);
        assert_eq!(world.transport.companies[&1], before);
        let credits = money(&world);
        let people = passengers(&world);
        let wood = goods(&world, "wood");
        company(
            &mut world,
            &catalog,
            CompanyCommand::ReturnLoad { driver_id: driver },
        )
        .unwrap();
        let trip = world.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .clone();
        assert!(trip.returning);
        let original = serde_json::to_value(&world).unwrap();
        assert!(
            company(
                &mut world,
                &catalog,
                CompanyCommand::ReturnLoad { driver_id: driver }
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&world).unwrap(), original);
        let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
        world = partition.assemble_all(&catalog).unwrap();
        for _ in 0..2000 {
            world.advance_living(0.1, 0., &catalog).unwrap();
            if world.transport.companies[&1].drivers[0].trip.is_none() {
                break;
            }
        }
        let d = &world.transport.companies[&1].drivers[0];
        assert!(d.trip.is_none(), "return did not finish: {:?}", d.status);
        assert_eq!(d.status, FleetStatus::Paused);
        assert_eq!(d.deliveries_completed, 0);
        assert_eq!(world.transport.companies[&1].revenue, 0);
        assert!(
            world.living.units[&vehicle]
                .position
                .surface_distance_m(world.transport.facilities[&stops[0]].position)
                <= 20.
        );
        assert_eq!(money(&world), credits);
        assert_eq!(passengers(&world), people);
        assert_eq!(goods(&world, "wood"), wood);
        assert!(world.transport.vehicles[&vehicle].cargo.is_empty());
        world.validate_with_catalog(&catalog).unwrap();
        company(
            &mut world,
            &catalog,
            CompanyCommand::ReleaseVehicle { driver_id: driver },
        )
        .unwrap();
    }
}

#[test]
fn full_source_retains_cancelled_company_cargo_and_escrow_until_space_exists() {
    let (mut world, catalog, vehicle, driver, stops) = recovery_fixture(true);
    let source = world.transport.facilities[&stops[0]].linked_site.unwrap();
    world.living.producers.get_mut(&source).unwrap().stock =
        Inventory::from([("stone".into(), 2000)]);
    company(
        &mut world,
        &catalog,
        CompanyCommand::ReturnLoad { driver_id: driver },
    )
    .unwrap();
    let load = world.transport.vehicles[&vehicle].cargo.clone();
    let escrow = world.transport.companies[&1].drivers[0]
        .trip
        .as_ref()
        .unwrap()
        .reward_escrow;
    let treasury = world.transport.treasury;
    world.advance_transport(1.).unwrap();
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::DestinationFull
    );
    assert_eq!(world.transport.vehicles[&vehicle].cargo, load);
    assert_eq!(world.transport.treasury, treasury);
    world
        .living
        .producers
        .get_mut(&source)
        .unwrap()
        .stock
        .insert("stone".into(), 1992);
    world.advance_transport(1.).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_none());
    assert_eq!(world.transport.treasury, treasury + escrow);
    world.advance_transport(1.).unwrap();
    assert_eq!(world.transport.treasury, treasury + escrow);
    assert_eq!(world.living.producers[&source].stock["wood"], 8);
    world.validate_with_catalog(&catalog).unwrap();
}

fn replacement_crew(world: &mut SandboxState, catalog: &Catalog) -> (u64, u64) {
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .clone();
    world.set_player_position(1, garage.position).unwrap();
    // Move existing scenario capital, rather than manufacture purchase money.
    world.transport.treasury -= 30_000;
    world.living.accounts.get_mut(&1).unwrap().money += 30_000;
    let vehicle = world
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::BuyVehicle {
                    model: VehicleModel::Pickup,
                    facility_id: garage.id,
                },
            },
            catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    let driver = company(
        world,
        catalog,
        CompanyCommand::HireDriver {
            name: "Relief".into(),
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    (driver, vehicle)
}

#[test]
fn nearby_handover_preserves_physical_proof_wallets_and_load_without_moving_bodies() {
    for freight in [false, true] {
        let (mut world, catalog, vehicle, driver, _) = recovery_fixture(freight);
        for _ in 0..40 {
            world.advance_living(0.1, 0., &catalog).unwrap();
        }
        company(
            &mut world,
            &catalog,
            CompanyCommand::HoldVehicle { driver_id: driver },
        )
        .unwrap();
        let (relief, replacement) = replacement_crew(&mut world, &catalog);
        let position = world.living.units[&vehicle].position;
        world.living.units.get_mut(&replacement).unwrap().position =
            LocalFrame::new(position).to_geo([0., 0., 9.]).unwrap();
        world.set_player_position(1, position).unwrap();
        let replacement_position = world.living.units[&replacement].position;
        let original = world.transport.companies[&1].drivers[0].clone();
        let trip = original.trip.as_ref().unwrap();
        let proof = world.transport.vehicles[&vehicle].odometer_m - trip.pickup_odometer_m;
        let credits = money(&world);
        let people = passengers(&world);
        let wood = goods(&world, "wood");
        company(
            &mut world,
            &catalog,
            CompanyCommand::HandoverLoad {
                driver_id: driver,
                relief_driver_id: relief,
                vehicle_id: replacement,
            },
        )
        .unwrap();
        assert_eq!(world.living.units[&vehicle].position, position);
        assert_eq!(
            world.living.units[&replacement].position,
            replacement_position
        );
        assert!(!world.is_company_vehicle(vehicle));
        assert!(world.is_company_vehicle(replacement));
        assert_eq!(
            world.transport.companies[&1].drivers[0].wages_earned,
            original.wages_earned
        );
        let new = &world.transport.companies[&1].drivers[1];
        assert_eq!(new.status, FleetStatus::Held);
        assert_eq!(new.trip.as_ref().unwrap().reward_escrow, trip.reward_escrow);
        assert!((new.trip.as_ref().unwrap().transferred_distance_m - proof).abs() < 1e-6);
        assert!(world.transport.vehicles[&vehicle].cargo.is_empty());
        assert_eq!(money(&world), credits);
        assert_eq!(passengers(&world), people);
        assert_eq!(goods(&world, "wood"), wood);
        world = serde_json::from_value(serde_json::to_value(&world).unwrap()).unwrap();
        world.validate_with_catalog(&catalog).unwrap();
        company(
            &mut world,
            &catalog,
            CompanyCommand::SetDriverRunning {
                driver_id: relief,
                running: true,
            },
        )
        .unwrap();
        company(
            &mut world,
            &catalog,
            CompanyCommand::SetDriverRunning {
                driver_id: relief,
                running: false,
            },
        )
        .unwrap();
        for _ in 0..2000 {
            world.advance_living(0.1, 0., &catalog).unwrap();
            if world.transport.companies[&1].drivers[1].trip.is_none() {
                break;
            }
        }
        assert_eq!(
            world.transport.companies[&1].drivers[1].deliveries_completed,
            1
        );
        assert_eq!(world.transport.companies[&1].revenue, trip.reward_escrow);
        assert_eq!(money(&world), credits);
        assert_eq!(passengers(&world), people);
        assert_eq!(goods(&world, "wood"), wood);
        world.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn handover_rejects_distance_ownership_capacity_motion_and_duplicate_mutations_atomically() {
    let (mut world, catalog, vehicle, driver, _) = recovery_fixture(false);
    company(
        &mut world,
        &catalog,
        CompanyCommand::HoldVehicle { driver_id: driver },
    )
    .unwrap();
    let (relief, replacement) = replacement_crew(&mut world, &catalog);
    let position = world.living.units[&vehicle].position;
    world.living.units.get_mut(&replacement).unwrap().position =
        LocalFrame::new(position).to_geo([0., 0., 10.]).unwrap();
    world.set_player_position(1, position).unwrap();
    for scenario in 0..7 {
        let mut candidate = world.clone();
        let mut next = relief;
        let mut body = replacement;
        match scenario {
            0 => {
                candidate
                    .living
                    .units
                    .get_mut(&replacement)
                    .unwrap()
                    .position = LocalFrame::new(position).to_geo([0., 0., 21.]).unwrap()
            }
            1 => {
                candidate
                    .living
                    .units
                    .get_mut(&replacement)
                    .unwrap()
                    .velocity_enu_m_s = [1., 0., 0.]
            }
            2 => {
                body = candidate
                    .transport
                    .vehicles
                    .values()
                    .find(|v| v.owner == 2)
                    .unwrap()
                    .unit_id
            }
            3 => {
                candidate
                    .transport
                    .vehicles
                    .get_mut(&replacement)
                    .unwrap()
                    .model = VehicleModel::BoxTruck
            }
            4 => next = driver,
            5 => {
                candidate.living.accounts.get_mut(&1).unwrap().active_unit = replacement;
            }
            _ => candidate
                .set_player_position(1, LocalFrame::new(position).to_geo([100., 0., 0.]).unwrap())
                .unwrap(),
        }
        let before = serde_json::to_value(&candidate).unwrap();
        assert!(
            company(
                &mut candidate,
                &catalog,
                CompanyCommand::HandoverLoad {
                    driver_id: driver,
                    relief_driver_id: next,
                    vehicle_id: body
                }
            )
            .is_err(),
            "scenario {scenario}"
        );
        assert_eq!(serde_json::to_value(&candidate).unwrap(), before);
    }
    company(
        &mut world,
        &catalog,
        CompanyCommand::HandoverLoad {
            driver_id: driver,
            relief_driver_id: relief,
            vehicle_id: replacement,
        },
    )
    .unwrap();
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::HandoverLoad {
                driver_id: driver,
                relief_driver_id: relief,
                vehicle_id: replacement
            }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
}

#[test]
fn dismissing_an_employee_preserves_earned_wallet_and_settles_fractional_final_pay() {
    let (mut world, catalog, vehicle, stops) = fixture();
    let driver = setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    world.advance_living(0.13, 0., &catalog).unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::HoldVehicle { driver_id: driver },
    )
    .unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::DismissDriver { driver_id: driver }
        )
        .is_err()
    );
    company(
        &mut world,
        &catalog,
        CompanyCommand::ReturnLoad { driver_id: driver },
    )
    .unwrap();
    world.advance_transport(1.).unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::ReleaseVehicle { driver_id: driver },
    )
    .unwrap();
    world.transport.companies.get_mut(&1).unwrap().cash -= 123;
    let d = &mut world.transport.companies.get_mut(&1).unwrap().drivers[0];
    d.wages_earned += 123;
    d.wage_fraction = 0.75;
    // This fixture predates the journal: preserve its authored earned pay as an
    // imported opening balance instead of fabricating old payroll receipts.
    world.transport.companies.get_mut(&1).unwrap().accounting = None;
    let money_before = money(&world);
    let payroll = world.transport.companies[&1].payroll_paid();
    company(
        &mut world,
        &catalog,
        CompanyCommand::DismissDriver { driver_id: driver },
    )
    .unwrap();
    assert!(world.transport.companies[&1].drivers.is_empty());
    assert_eq!(money(&world), money_before);
    assert_eq!(world.transport.companies[&1].payroll_paid(), payroll + 1);
    assert!(world.transport.companies[&1].departed_wages >= 124);
    let accounting = world.transport.companies[&1].accounting_report();
    assert!(accounting.reconciled);
    assert_eq!(
        accounting.coverage,
        CompanyAccountingCoverage::ImportedOpeningBalance
    );
    assert_eq!(accounting.opening_payroll, payroll);
    assert_eq!(accounting.totals[&CompanyCashCategory::Payroll], 1);
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::DismissDriver { driver_id: driver }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn private_depot_conserves_items_enforces_capacity_and_survives_restore_without_public_exposure() {
    let (mut world, catalog, _, _) = fixture();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Create {
            name: "Depot company".into(),
        },
    )
    .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Deposit { amount: 10_000 },
    )
    .unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    let cash = money(&world);
    company(
        &mut world,
        &catalog,
        CompanyCommand::OpenDepot {
            facility_id: garage,
        },
    )
    .unwrap();
    assert_eq!(money(&world), cash);
    let carried = world.players[&1].inventory.clone();
    let item = carried.keys().next().unwrap().clone();
    let amount = carried[&item].min(4);
    company(
        &mut world,
        &catalog,
        CompanyCommand::TransferDepotStock {
            facility_id: garage,
            item: item.clone(),
            amount,
            deposit: true,
        },
    )
    .unwrap();
    assert_eq!(world.transport.companies[&1].depots[0].stock[&item], amount);
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::CloseDepot {
                facility_id: garage
            }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
    let view = world.snapshot_for(1, 2000.).unwrap();
    view.transport.validate_replica(1, &view.living).unwrap();
    assert_eq!(
        view.transport.company.as_ref().unwrap().depots[0].stock[&item],
        amount
    );
    assert!(
        world
            .snapshot_for(2, 2000.)
            .unwrap()
            .transport
            .company
            .is_none()
    );
    let partition = RegionPartition::from_state(&world, 15, &catalog).unwrap();
    world = partition.assemble_all(&catalog).unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::TransferDepotStock {
            facility_id: garage,
            item: item.clone(),
            amount,
            deposit: false,
        },
    )
    .unwrap();
    assert_eq!(world.players[&1].inventory, carried);
    // Aggregate, mixed item capacity applies before publication.
    world.transport.companies.get_mut(&1).unwrap().depots[0].stock =
        Inventory::from([("stone".into(), 1999)]);
    world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 2);
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::TransferDepotStock {
                facility_id: garage,
                item: "wood".into(),
                amount: 2,
                deposit: true
            }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
    company(
        &mut world,
        &catalog,
        CompanyCommand::TransferDepotStock {
            facility_id: garage,
            item: "wood".into(),
            amount: 1,
            deposit: true,
        },
    )
    .unwrap();
    assert_eq!(
        world.transport.companies[&1].depots[0]
            .stock
            .values()
            .sum::<u32>(),
        2000
    );
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn depot_households_create_finite_recurrent_demand_and_return_earned_money_to_services() {
    let (mut world, catalog, _, _) = fixture();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Create {
            name: "Local economy".into(),
        },
    )
    .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Deposit { amount: 10_000 },
    )
    .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
    )
    .unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .clone();
    company(
        &mut world,
        &catalog,
        CompanyCommand::OpenDepot {
            facility_id: garage.id,
        },
    )
    .unwrap();
    let shop = world
        .living
        .shops
        .values()
        .min_by(|a, b| {
            a.position
                .surface_distance_m(garage.position)
                .total_cmp(&b.position.surface_distance_m(garage.position))
        })
        .unwrap()
        .id;
    assert!(
        world.living.shops[&shop]
            .position
            .surface_distance_m(garage.position)
            <= 200.
    );
    world
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("water".into(), 3);
    world
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .insert("bread".into(), 3);
    world.transport.companies.get_mut(&1).unwrap().cash -= 2_000;
    world.transport.companies.get_mut(&1).unwrap().drivers[0].wages_earned = 2_000;
    // Authored historical wages have no recorded event history. Household
    // spending must preserve them without claiming a complete new ledger.
    world.transport.companies.get_mut(&1).unwrap().accounting = None;
    let before_money = money(&world);
    let wages = world.transport.companies[&1].payroll_paid();
    let treasury = world.transport.treasury;
    let water = goods(&world, "water");
    let bread = goods(&world, "bread");
    for _ in 0..24 {
        world.advance_transport(5.).unwrap();
    }
    assert_eq!(world.transport.companies[&1].household_consumed["water"], 1);
    assert_eq!(world.transport.companies[&1].household_consumed["bread"], 1);
    assert_eq!(world.transport.treasury, treasury + 100);
    assert_eq!(money(&world), before_money);
    assert_eq!(world.transport.companies[&1].payroll_paid(), wages);
    assert_eq!(goods(&world, "water") + 1, water);
    assert_eq!(goods(&world, "bread") + 1, bread);
    world.set_player_dormant(1, true).unwrap();
    let saved = world.transport.companies[&1].clone();
    for _ in 0..24 {
        world.advance_transport(5.).unwrap();
    }
    assert_eq!(world.transport.companies[&1], saved);
    world.set_player_dormant(1, false).unwrap();
    // Empty retail shelves never refill themselves or extract an unpaid pass.
    world
        .living
        .shops
        .get_mut(&shop)
        .unwrap()
        .stock
        .remove("water");
    let company_before = world.transport.companies[&1].clone();
    for _ in 0..12 {
        world.advance_transport(5.).unwrap();
    }
    assert_eq!(world.transport.companies[&1], company_before);
    assert_eq!(money(&world), before_money);
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn owned_depot_waives_only_onsite_callout_and_never_fuel_or_repair_value() {
    let (mut world, catalog, vehicle, driver, _) = recovery_fixture(false);
    company(
        &mut world,
        &catalog,
        CompanyCommand::HoldVehicle { driver_id: driver },
    )
    .unwrap();
    let garage = world
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .clone();
    world.set_player_position(1, garage.position).unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::OpenDepot {
            facility_id: garage.id,
        },
    )
    .unwrap();
    let remote = world
        .company_roadside_quote(1, vehicle, garage.id, 1, false)
        .unwrap();
    assert_eq!(remote.callout_cost, COMPANY_ROADSIDE_CALLOUT_PRICE);
    world.living.units.get_mut(&vehicle).unwrap().position = garage.position;
    let onsite = world
        .company_roadside_quote(1, vehicle, garage.id, 1, false)
        .unwrap();
    assert_eq!(onsite.callout_cost, 0);
    assert_eq!(onsite.total_cost, COMPANY_ROADSIDE_FUEL_PRICE_PER_LITRE);
    let before = money(&world);
    let fuel =
        world.transport.facilities[&garage.id].fuel_l + world.transport.vehicles[&vehicle].fuel_l;
    company(&mut world, &catalog, roadside_command(onsite)).unwrap();
    assert_eq!(money(&world), before);
    assert!(
        (fuel
            - world.transport.facilities[&garage.id].fuel_l
            - world.transport.vehicles[&vehicle].fuel_l)
            .abs()
            < 1e-6
    );
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    world.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn public_service_contribution_restarts_funded_work_without_creating_currency() {
    let (mut world, catalog, vehicle, stops) = fixture();
    setup(
        &mut world,
        &catalog,
        vehicle,
        stops[..2].to_vec(),
        FleetService::Passengers { count: 1 },
    );
    let amount = world.transport.treasury;
    world.transport.treasury = 0;
    world.living.accounts.get_mut(&1).unwrap().money += amount;
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert_eq!(
        world.transport.companies[&1].drivers[0].status,
        FleetStatus::WaitingIssuerFunds
    );
    let credits = money(&world);
    let cash = world.transport.companies[&1].cash;
    company(
        &mut world,
        &catalog,
        CompanyCommand::FundPublicServices { amount: 1000 },
    )
    .unwrap();
    assert_eq!(world.transport.companies[&1].cash, cash - 1000);
    assert_eq!(world.transport.treasury, 1000);
    assert_eq!(money(&world), credits);
    world.advance_living(0.1, 0., &catalog).unwrap();
    assert!(world.transport.companies[&1].drivers[0].trip.is_some());
    assert_eq!(money(&world), credits);
    let before = serde_json::to_value(&world).unwrap();
    assert!(
        company(
            &mut world,
            &catalog,
            CompanyCommand::FundPublicServices { amount: MAX_MONEY }
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&world).unwrap(), before);
}
