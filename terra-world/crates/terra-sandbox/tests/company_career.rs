//! A finite, priced novice passenger service. This is a transport-career
//! measurement, not an indefinite-economy or human playtest claim.
use terra_core::GeoPosition;
use terra_sandbox::*;

const STEP: f64 = 0.25;
const SERVICE_SECONDS: u32 = 3_600;

fn transport(world: &mut SandboxState, catalog: &Catalog, action: TransportCommand) -> Outcome {
    world
        .apply(1, Command::Transport { action }, catalog)
        .unwrap()
}

fn company(world: &mut SandboxState, catalog: &Catalog, action: CompanyCommand) -> Outcome {
    transport(world, catalog, TransportCommand::Company { action })
}

fn cash_and_escrow(world: &SandboxState) -> u64 {
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
                        .map(|d| {
                            d.wages_earned
                                + d.trip.as_ref().map_or(0, |t| {
                                    t.reward_escrow
                                        + t.goods_payment.as_ref().map_or(0, |p| p.total)
                                })
                        })
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
            .filter(|j| j.assignment.is_some())
            .map(|j| u64::from(j.passengers))
            .sum::<u64>()
        + world
            .transport
            .companies
            .values()
            .flat_map(|c| &c.drivers)
            .filter_map(|d| d.trip.as_ref())
            .map(|t| match t.service {
                FleetService::Passengers { count } => u64::from(count),
                _ => 0,
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

struct Measurement {
    money: u64,
    people: u64,
    initial_fuel: f64,
    consumed_fuel: f64,
    renewals: u32,
    road_holds: Vec<f64>,
    resume_at: Option<f64>,
}

impl Measurement {
    fn check(&self, world: &SandboxState) {
        assert_eq!(
            cash_and_escrow(world),
            self.money,
            "cash, earned wages and all escrow are conserved"
        );
        assert_eq!(
            passengers(world),
            self.people,
            "services move existing queues"
        );
        assert!(
            (fuel(world) + self.consumed_fuel - self.initial_fuel).abs() < 1e-6,
            "fuel transfers and physical consumption reconcile"
        );
        if let Some(c) = world.transport.companies.get(&1) {
            assert!(c.accounting_report().reconciled);
        }
    }

    fn step(&mut self, world: &mut SandboxState, catalog: &Catalog, vehicle: u64) {
        if let Some(driver) = world
            .transport
            .companies
            .get(&1)
            .and_then(|c| c.drivers.iter().find(|d| d.vehicle_id == Some(vehicle)))
            .filter(|d| d.status == FleetStatus::RouteBlocked)
        {
            let resume_at = *self.resume_at.get_or_insert_with(|| {
                self.road_holds.push(world.transport.elapsed_seconds);
                assert!(
                    self.road_holds.len() <= 12,
                    "bounded explicit road recovery exhausted at {} seconds: {:?}",
                    world.transport.elapsed_seconds,
                    world.living.units[&vehicle]
                );
                world.transport.elapsed_seconds + 10.
            });
            if world.transport.elapsed_seconds >= resume_at {
                let (driver_id, running) = (driver.id, driver.running);
                company(
                    world,
                    catalog,
                    CompanyCommand::SetDriverRunning { driver_id, running },
                );
                self.resume_at = None;
            }
        }
        if world.transport.vehicles[&vehicle]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds
            < 30.
        {
            transport(
                world,
                catalog,
                TransportCommand::RenewRental {
                    vehicle_id: vehicle,
                },
            );
            self.renewals += 1;
        }
        let before = fuel(world);
        // Hold the accelerated ecology clock at zero to isolate transport
        // cash flow. Physical movement, traffic, wear, lease time and payroll
        // all run normally. This does not measure the avatar's living costs.
        world.advance_living(STEP, 0., catalog).unwrap();
        let used = before - fuel(world);
        assert!(used >= -1e-8, "physical travel cannot create fuel");
        self.consumed_fuel += used;
        self.check(world);
    }
}

#[test]
fn finite_novice_passenger_career_prices_rental_costs_and_resumes_for_one_physical_hour() {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
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
    let mut stops: Vec<_> = world
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Terminal)
        .map(|f| f.id)
        .collect();
    stops.sort_unstable();
    assert_eq!(stops.len(), 2);
    let initial_owner_cash = world.living.accounts[&1].money;
    let initial_issuer_cash = world.transport.treasury;
    let personal_vehicles: Vec<_> = world
        .transport
        .vehicles
        .values()
        .map(|v| (v.unit_id, v.odometer_m, v.fuel_l, v.condition))
        .collect();
    assert_eq!(initial_owner_cash, 25_000, "use the authored novice wallet");
    let mut measurement = Measurement {
        money: cash_and_escrow(&world),
        people: passengers(&world),
        initial_fuel: fuel(&world),
        consumed_fuel: 0.,
        renewals: 0,
        road_holds: Vec::new(),
        resume_at: None,
    };
    // Initial positioning represents walking to the garage before this
    // transport measurement; no service vehicle, route or economy is edited.
    world
        .set_player_position(1, world.transport.facilities[&garage].position)
        .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Create {
            name: "First Hour Transit".into(),
        },
    );
    company(
        &mut world,
        &catalog,
        CompanyCommand::Deposit { amount: 10_000 },
    );
    let driver = company(
        &mut world,
        &catalog,
        CompanyCommand::HireDriver {
            name: "Robin".into(),
        },
    )
    .created_id
    .unwrap();
    let offer = rental_offer(VehicleModel::Compact);
    let vehicle = transport(
        &mut world,
        &catalog,
        TransportCommand::RentVehicle {
            model: VehicleModel::Compact,
            facility_id: garage,
        },
    )
    .created_id
    .unwrap();
    let route = company(
        &mut world,
        &catalog,
        CompanyCommand::CreateRoute {
            name: "Town and hillside".into(),
            stops: stops.clone(),
            service: FleetService::Passengers { count: 3 },
        },
    )
    .created_id
    .unwrap();
    company(
        &mut world,
        &catalog,
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: vehicle,
            route_id: route,
        },
    );
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    measurement.check(&world);
    let mut checkpoints = Vec::new();
    for tick in 1..=SERVICE_SECONDS * 4 {
        measurement.step(&mut world, &catalog, vehicle);
        let employee = &world.transport.companies[&1].drivers[0];
        assert!(
            !matches!(
                employee.status,
                FleetStatus::WaitingCompanyFunds
                    | FleetStatus::NeedsRepair
                    | FleetStatus::NeedsFuel
                    | FleetStatus::RentalExpired
            ),
            "novice service stopped at {} seconds: {:?}",
            world.transport.elapsed_seconds,
            employee
        );
        if tick % 2_400 == 0 {
            world.validate_with_catalog(&catalog).unwrap();
            let report = world.transport.companies[&1].accounting_report();
            checkpoints.push(serde_json::json!({
                "physical_seconds": world.transport.elapsed_seconds,
                "deliveries": employee.deliveries_completed,
                "company_cash_minor_credits": report.cash,
                "operating_net_minor_credits": report.operating.net(),
                "owner_wallet_minor_credits": world.living.accounts[&1].money,
                "issuer_cash_minor_credits": world.transport.treasury,
            }));
            let saved = serde_json::to_vec(&world).unwrap();
            let restored: SandboxState = serde_json::from_slice(&saved).unwrap();
            restored.validate_with_catalog(&catalog).unwrap();
            assert_eq!(
                restored, world,
                "resume retains exact paid and in-flight commitments"
            );
            world = restored;
            measurement.check(&world);
        }
    }
    company(
        &mut world,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: false,
        },
    );
    for _ in 0..2_400 {
        if world.transport.companies[&1].drivers[0].trip.is_none() {
            break;
        }
        measurement.step(&mut world, &catalog, vehicle);
    }
    assert!(
        world.transport.companies[&1].drivers[0].trip.is_none(),
        "finish the last paid load"
    );
    company(
        &mut world,
        &catalog,
        CompanyCommand::ReleaseVehicle { driver_id: driver },
    );
    let garage_position = world.transport.facilities[&garage].position;
    // The garage marker is the starter pickup's occupied position. Return to
    // this rental's actual original delivery bay beside it, not that body.
    let destination = world.living.units[&vehicle].home;
    // Release parks the vehicle. The ordinary vehicle Autopilot command
    // releases its brake and orders a licensed, physical return to the garage.
    transport(
        &mut world,
        &catalog,
        TransportCommand::SetAutopilot {
            vehicle_id: vehicle,
            enabled: true,
            destination: Some(destination),
        },
    );
    for _ in 0..2_400 {
        let unit = &world.living.units[&vehicle];
        if unit
            .position
            .surface_distance_m(garage_position)
            .hypot(unit.position.altitude_m - garage_position.altitude_m)
            < INTERACTION_REACH_M - 0.5
            && unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1]) < 0.5
        {
            break;
        }
        measurement.step(&mut world, &catalog, vehicle);
    }
    let distance_m = world.transport.vehicles[&vehicle].odometer_m;
    let consumed_fuel_l = 10. - world.transport.vehicles[&vehicle].fuel_l;
    let refund = world.rental_return_quote(1, vehicle).unwrap();
    transport(
        &mut world,
        &catalog,
        TransportCommand::ReturnRental {
            vehicle_id: vehicle,
            facility_id: garage,
            quoted_refund: refund.total_refund,
        },
    );
    measurement.check(&world);
    for (id, odometer, fuel, condition) in personal_vehicles {
        let vehicle = &world.transport.vehicles[&id];
        assert_eq!(
            (vehicle.odometer_m, vehicle.fuel_l, vehicle.condition),
            (odometer, fuel, condition),
            "the unpriced starter vehicle is not used by this career"
        );
    }
    let c = &world.transport.companies[&1];
    let report = c.accounting_report();
    let final_owner_cash = world.living.accounts[&1].money;
    let net_cash = final_owner_cash as i64 + c.cash as i64 - initial_owner_cash as i64;
    let lease_fees = offer.fee * (1 + u64::from(measurement.renewals));
    let used_fuel_cash = offer.fuel_cost - refund.fuel_refund;
    let exact_known_costs = COMPANY_REGISTRATION_PRICE
        + COMPANY_HIRE_PRICE
        + c.payroll_paid()
        + lease_fees
        + used_fuel_cash
        + refund.damage_cost;
    assert_eq!(
        net_cash,
        c.revenue as i64 - exact_known_costs as i64,
        "owner financing and refundable bond are not operating income or costs"
    );
    assert!(
        net_cash > 0,
        "this finite intended career must pay its known cash costs"
    );
    assert!(c.drivers[0].deliveries_completed >= 12);
    assert_eq!(report.totals[&CompanyCashCategory::PublicFares], c.revenue);
    assert_eq!(
        report.totals[&CompanyCashCategory::Payroll],
        c.payroll_paid()
    );
    assert_eq!(report.totals[&CompanyCashCategory::OwnerDeposit], 10_000);
    let restored = RegionPartition::from_state(&world, 15, &catalog)
        .unwrap()
        .assemble_all(&catalog)
        .unwrap();
    assert_eq!(restored.transport.companies[&1].accounting_report(), report);
    measurement.check(&restored);
    println!(
        "NOVICE_PASSENGER_CAREER_REPORT {}",
        serde_json::json!({
            "scenario": "seeded_settlement_compact_rental_passenger_company",
            "service_physical_seconds": SERVICE_SECONDS,
            "total_physical_seconds_including_completion_and_return": world.transport.elapsed_seconds,
            "ecology_clock_seconds": 0,
            "initial_owner_minor_credits": initial_owner_cash,
            "initial_issuer_minor_credits": initial_issuer_cash,
            "final_issuer_minor_credits": world.transport.treasury,
            "final_owner_minor_credits": final_owner_cash,
            "final_company_minor_credits": c.cash,
            "owner_deposits_minor_credits": 10_000,
            "additional_owner_deposits_minor_credits": 0,
            "deliveries": c.drivers[0].deliveries_completed,
            "gross_public_fares_minor_credits": c.revenue,
            "registration_minor_credits": COMPANY_REGISTRATION_PRICE,
            "recruitment_minor_credits": COMPANY_HIRE_PRICE,
            "payroll_minor_credits": c.payroll_paid(),
            "rental_lease_minor_credits": lease_fees,
            "rental_renewals": measurement.renewals,
        "road_holds_waited_10_seconds_then_resumed": measurement.road_holds,
            "fuel_cash_net_of_return_refund_minor_credits": used_fuel_cash,
            "rental_damage_minor_credits": refund.damage_cost,
            "bond_posted_minor_credits": offer.bond,
            "bond_returned_minor_credits": refund.bond_refund,
            "fuel_consumed_litres": consumed_fuel_l,
            "vehicle_distance_metres": distance_m,
        "unpriced_starter_vehicle_unused": true,
            "net_after_all_known_cash_costs_minor_credits": net_cash,
            "cash_and_escrow_conserved_minor_credits": measurement.money,
            "passengers_conserved": measurement.people,
            "save_resume_checkpoints": checkpoints,
            "regional_accounting_resume_exact": true,
            "unpriced": ["owner planning and management time", "owner living costs outside the frozen ecology clock"],
            "limits": ["finite one-hour public-service scenario", "repeated journeys circulate existing passenger queues", "not indefinite treasury sustainability", "not a human playtest or every terrain layout"],
            "persisted_accounting": report,
        })
    );
}
