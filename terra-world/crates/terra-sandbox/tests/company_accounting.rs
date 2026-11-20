//! Persisted cash receipts, imported history and transactional failure evidence.
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

struct Fixture {
    world: SandboxState,
    catalog: Catalog,
    garage: u64,
    terminal: u64,
    shop: u64,
    vehicle: u64,
}

impl Fixture {
    fn new() -> Self {
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
        let terminal = world
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Terminal)
            .unwrap()
            .id;
        let shop = *world.living.shops.keys().next().unwrap();
        let vehicle = world
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == 1)
            .unwrap()
            .unit_id;
        let position = frame.to_geo([1_000., 0., -1_000.]).unwrap();
        world
            .transport
            .facilities
            .get_mut(&garage)
            .unwrap()
            .position = position;
        world
            .transport
            .facilities
            .get_mut(&garage)
            .unwrap()
            .passengers_waiting = 20;
        world
            .transport
            .facilities
            .get_mut(&terminal)
            .unwrap()
            .position = frame.to_geo([1_180., 0., -1_000.]).unwrap();
        world.living.shops.get_mut(&shop).unwrap().position = position;
        world.living.units.get_mut(&vehicle).unwrap().position =
            frame.to_geo([1_008., 0., -1_000.]).unwrap();
        world.living.roads.clear();
        for producer in world.living.producers.values_mut() {
            producer.enabled = false;
        }
        world.set_player_position(1, position).unwrap();
        let mut result = Self {
            world,
            catalog,
            garage,
            terminal,
            shop,
            vehicle,
        };
        result
            .company(CompanyCommand::Create {
                name: "Measured Transit".into(),
            })
            .unwrap();
        result
    }

    fn company(&mut self, action: CompanyCommand) -> Result<Outcome, SandboxError> {
        self.transport(TransportCommand::Company { action })
    }

    fn transport(&mut self, action: TransportCommand) -> Result<Outcome, SandboxError> {
        self.world
            .apply(1, Command::Transport { action }, &self.catalog)
    }

    fn report(&self) -> CompanyAccountingReport {
        let report = self.world.transport.companies[&1].accounting_report();
        assert!(report.reconciled);
        assert_eq!(report.expected_cash, report.cash as i64);
        assert_eq!(
            report.cash as i64,
            report.opening_cash as i64
                + report.operating.net()
                + report.capital.net()
                + report.financing.net()
        );
        report
    }

    fn fund_and_assign(&mut self, amount: u64) -> u64 {
        self.company(CompanyCommand::Deposit { amount }).unwrap();
        let driver = self
            .company(CompanyCommand::HireDriver {
                name: "Robin".into(),
            })
            .unwrap()
            .created_id
            .unwrap();
        let route = self
            .company(CompanyCommand::CreateRoute {
                name: "Finite passenger loop".into(),
                stops: vec![self.garage, self.terminal],
                service: FleetService::Passengers { count: 1 },
            })
            .unwrap()
            .created_id
            .unwrap();
        self.company(CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: self.vehicle,
            route_id: route,
        })
        .unwrap();
        driver
    }
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

#[test]
fn company_cash_categories_reconcile_setup_trade_service_and_financing_without_pricing_in_kind() {
    let mut f = Fixture::new();
    let conserved = money(&f.world);
    f.fund_and_assign(20_000);
    f.company(CompanyCommand::OpenDepot {
        facility_id: f.garage,
    })
    .unwrap();
    let buy = f
        .world
        .quote_company_depot_trade(1, f.garage, f.shop, "water", 2, true)
        .unwrap();
    f.company(CompanyCommand::TradeDepotStock {
        facility_id: f.garage,
        shop_id: f.shop,
        item: "water".into(),
        amount: 2,
        buy: true,
        quoted_total: buy.total_cost,
    })
    .unwrap();
    let sell = f
        .world
        .quote_company_depot_trade(1, f.garage, f.shop, "water", 1, false)
        .unwrap();
    f.company(CompanyCommand::TradeDepotStock {
        facility_id: f.garage,
        shop_id: f.shop,
        item: "water".into(),
        amount: 1,
        buy: false,
        quoted_total: sell.total_cost,
    })
    .unwrap();
    f.company(CompanyCommand::FundPublicServices { amount: 123 })
        .unwrap();
    f.company(CompanyCommand::Withdraw { amount: 200 }).unwrap();
    // The source is deliberately in kind. The journal must not fabricate an
    // acquisition price or value this contribution as sales/financing cash.
    f.world
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 3);
    f.company(CompanyCommand::TransferDepotStock {
        facility_id: f.garage,
        item: "wood".into(),
        amount: 3,
        deposit: true,
    })
    .unwrap();
    f.company(CompanyCommand::TransferDepotStock {
        facility_id: f.garage,
        item: "wood".into(),
        amount: 1,
        deposit: false,
    })
    .unwrap();
    let remote = LocalFrame::new(f.world.transport.facilities[&f.garage].position)
        .to_geo([40., 0., 0.])
        .unwrap();
    f.world.living.units.get_mut(&f.vehicle).unwrap().position = remote;
    f.world.set_player_position(1, remote).unwrap();
    f.world
        .transport
        .vehicles
        .get_mut(&f.vehicle)
        .unwrap()
        .condition = 0.9;
    let service = f
        .world
        .company_roadside_quote(1, f.vehicle, f.garage, 1, true)
        .unwrap();
    f.company(CompanyCommand::RoadsideService {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
        litres: 1,
        repair: true,
        quoted_total: service.total_cost,
    })
    .unwrap();
    let report = f.report();
    assert_eq!(
        report.coverage,
        CompanyAccountingCoverage::SinceRegistration
    );
    assert_eq!(report.opening_cash, 0);
    assert_eq!(
        report.totals[&CompanyCashCategory::OwnerPaidRegistration],
        2_000
    );
    assert_eq!(report.totals[&CompanyCashCategory::Recruitment], 1_000);
    assert_eq!(report.totals[&CompanyCashCategory::DepotAccess], 2_500);
    assert_eq!(
        report.totals[&CompanyCashCategory::InventoryPurchases],
        buy.total_cost
    );
    assert_eq!(
        report.totals[&CompanyCashCategory::GoodsSales],
        sell.total_cost
    );
    assert_eq!(report.totals[&CompanyCashCategory::ServiceCallout], 1_500);
    assert_eq!(report.totals[&CompanyCashCategory::Fuel], 150);
    assert_eq!(
        report.totals[&CompanyCashCategory::Repairs],
        service.repair_cost
    );
    assert_eq!(
        report.operating.net(),
        sell.total_cost as i64 - buy.total_cost as i64 - 1_000 - 123 - service.total_cost as i64
    );
    assert_eq!(report.capital.net(), -2_500);
    assert_eq!(report.financing.net(), 19_800);
    assert_eq!(report.known_owner_funding, 22_000);
    assert_eq!(
        report.cash_result_after_known_owner_costs,
        report.cash_result_before_financing - 2_000
    );
    assert_eq!(
        (
            report.unpriced_stock_in_units,
            report.unpriced_stock_out_units
        ),
        (3, 1)
    );
    assert!(report.personal_vehicle_used);
    assert_eq!(money(&f.world), conserved);
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
    restored.validate_with_catalog(&f.catalog).unwrap();
    assert_eq!(restored.transport.companies[&1].accounting_report(), report);
}

#[test]
fn recent_receipts_are_bounded_while_lifetime_totals_and_cash_remain_exact() {
    let mut f = Fixture::new();
    for _ in 0..100 {
        f.company(CompanyCommand::Deposit { amount: 17 }).unwrap();
        f.company(CompanyCommand::Withdraw { amount: 17 }).unwrap();
    }
    let report = f.report();
    assert_eq!(report.monetary_entry_count, 201);
    assert_eq!(report.recent_entries.len(), MAX_COMPANY_RECENT_ENTRIES);
    assert_eq!(report.recent_entries.first().unwrap().sequence, 138);
    assert_eq!(report.recent_entries.last().unwrap().sequence, 201);
    assert_eq!(
        report.financing,
        CompanyCashFlow {
            receipts: 1_700,
            payments: 1_700
        }
    );
    assert_eq!(report.owner_paid_capital, 2_000);
    assert_eq!(report.cash, 0);
    assert_eq!(report.cash_result_before_financing, 0);
    assert_eq!(report.cash_result_after_known_owner_costs, -2_000);
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
    restored.validate_with_catalog(&f.catalog).unwrap();
    assert_eq!(restored.transport.companies[&1].accounting_report(), report);
}

#[test]
fn legacy_save_starts_from_imported_cash_and_never_relabels_prior_income_as_new_profit() {
    let mut f = Fixture::new();
    f.fund_and_assign(10_000);
    let mut saved = serde_json::to_value(&f.world).unwrap();
    let company = saved["transport"]["companies"]["1"]
        .as_object_mut()
        .unwrap();
    company.remove("accounting");
    company.insert("revenue".into(), 12_345.into());
    company.insert("cash".into(), 8_823.into());
    company["drivers"][0]["wages_earned"] = 177.into();
    f.world = serde_json::from_value(saved).unwrap();
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let before = f.report();
    assert_eq!(
        before.coverage,
        CompanyAccountingCoverage::ImportedOpeningBalance
    );
    assert_eq!(before.recorded_since_elapsed_seconds, None);
    assert_eq!(before.opening_cash, 8_823);
    assert_eq!(before.opening_revenue, 12_345);
    assert_eq!(before.opening_payroll, 177);
    assert_eq!(before.operating.net(), 0);
    assert_eq!(before.known_owner_funding, 0);
    assert!(before.totals.is_empty());
    f.company(CompanyCommand::Deposit { amount: 500 }).unwrap();
    f.company(CompanyCommand::FundPublicServices { amount: 123 })
        .unwrap();
    let after = f.report();
    assert_eq!(
        after.coverage,
        CompanyAccountingCoverage::ImportedOpeningBalance
    );
    assert_eq!(after.opening_cash, 8_823);
    assert_eq!(after.opening_revenue, 12_345);
    assert_eq!(after.opening_payroll, 177);
    assert_eq!(after.operating.net(), -123);
    assert_eq!(after.financing.net(), 500);
    assert_eq!(after.known_owner_funding, 500);
    assert_eq!(after.cash_result_after_known_owner_costs, -123);
    assert_eq!(after.monetary_entry_count, 2);
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&f.world).unwrap()).unwrap();
    restored.validate_with_catalog(&f.catalog).unwrap();
    assert_eq!(restored.transport.companies[&1].accounting_report(), after);
}

#[test]
fn owner_paid_assigned_vehicle_costs_are_separate_and_unassigned_personal_spending_is_not_claimed()
{
    let mut f = Fixture::new();
    f.fund_and_assign(10_000);
    let conserved = money(&f.world);
    let company_cash = f.report().cash;
    f.transport(TransportCommand::Refuel {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
        litres: 1,
    })
    .unwrap();
    f.world
        .transport
        .vehicles
        .get_mut(&f.vehicle)
        .unwrap()
        .condition = 0.99;
    let repair = repair_price(&f.world.transport.vehicles[&f.vehicle]);
    f.transport(TransportCommand::Repair {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
    })
    .unwrap();
    f.transport(TransportCommand::Paint {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
        paint: 1,
    })
    .unwrap();
    let upgrade = upgrade_price(f.world.transport.vehicles[&f.vehicle].engine_level);
    f.transport(TransportCommand::Upgrade {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
        engine: true,
    })
    .unwrap();
    let report = f.report();
    assert_eq!(report.cash, company_cash);
    assert_eq!(report.owner_paid_operating, 150 + repair + 500);
    assert_eq!(report.owner_paid_capital, 2_000 + upgrade);
    assert_eq!(report.totals[&CompanyCashCategory::OwnerPaidFuel], 150);
    assert_eq!(report.totals[&CompanyCashCategory::OwnerPaidRepair], repair);
    assert_eq!(report.totals[&CompanyCashCategory::OwnerPaidPaint], 500);
    assert_eq!(
        report.totals[&CompanyCashCategory::OwnerPaidUpgrade],
        upgrade
    );
    assert_eq!(money(&f.world), conserved);
    let driver = f.world.transport.companies[&1].drivers[0].id;
    f.company(CompanyCommand::ReleaseVehicle { driver_id: driver })
        .unwrap();
    f.transport(TransportCommand::Refuel {
        vehicle_id: f.vehicle,
        facility_id: f.garage,
        litres: 1,
    })
    .unwrap();
    assert_eq!(
        f.report(),
        report,
        "unassigned personal fuel has no company allocation"
    );
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn assigned_rental_renewal_is_an_owner_paid_lease_without_treating_the_bond_as_an_expense() {
    let mut f = Fixture::new();
    // Clear the delivery bays; the lease still uses the normal checked garage
    // placement and finite fee/fuel/bond transaction.
    let far = LocalFrame::new(f.world.transport.facilities[&f.garage].position)
        .to_geo([80., 0., 0.])
        .unwrap();
    f.world.living.units.get_mut(&f.vehicle).unwrap().position = far;
    let rented = f
        .transport(TransportCommand::RentVehicle {
            model: VehicleModel::Pickup,
            facility_id: f.garage,
        })
        .unwrap()
        .created_id
        .unwrap();
    f.vehicle = rented;
    f.fund_and_assign(10_000);
    let before = f.report();
    let conserved = money(&f.world);
    let wallet = f.world.living.accounts[&1].money;
    let bond = f.world.transport.vehicles[&rented]
        .rental
        .as_ref()
        .unwrap()
        .bond_escrow;
    f.transport(TransportCommand::RenewRental { vehicle_id: rented })
        .unwrap();
    let report = f.report();
    let fee = rental_offer(VehicleModel::Pickup).fee;
    assert_eq!(report.cash, before.cash);
    assert_eq!(report.owner_paid_operating, fee);
    assert_eq!(report.totals[&CompanyCashCategory::OwnerPaidLease], fee);
    assert_eq!(f.world.living.accounts[&1].money, wallet - fee);
    assert_eq!(
        f.world.transport.vehicles[&rented]
            .rental
            .as_ref()
            .unwrap()
            .bond_escrow,
        bond
    );
    assert_eq!(money(&f.world), conserved);
    assert!(
        report.personal_vehicle_used,
        "initial prepaid lease/acquisition is explicitly unpriced"
    );
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

/// A valid compressed history at the event limit. It has no fabricated money:
/// old deposits and withdrawals cancel, and 64 exact final receipts reconcile.
fn fill_record_capacity(f: &mut Fixture) {
    let company = f.world.transport.companies.get_mut(&1).unwrap();
    let mut cash = company.cash;
    let mut recent_entries = Vec::new();
    for index in 0..MAX_COMPANY_RECENT_ENTRIES {
        let category = if index % 2 == 0 {
            cash += 1;
            CompanyCashCategory::OwnerDeposit
        } else {
            cash -= 1;
            CompanyCashCategory::OwnerWithdrawal
        };
        recent_entries.push(CompanyCashEntry {
            sequence: MAX_COMPANY_ACCOUNTING_TOTAL - MAX_COMPANY_RECENT_ENTRIES as u64
                + index as u64
                + 1,
            elapsed_seconds: f.world.transport.elapsed_seconds,
            category,
            amount: 1,
            cash_after: cash,
        });
    }
    company.accounting = Some(CompanyAccounting {
        coverage: CompanyAccountingCoverage::ImportedOpeningBalance,
        recorded_since_elapsed_seconds: f.world.transport.elapsed_seconds,
        opening_cash: company.cash,
        opening_revenue: company.revenue,
        opening_payroll: company.payroll_paid(),
        totals: [
            (
                CompanyCashCategory::OwnerDeposit,
                MAX_COMPANY_ACCOUNTING_TOTAL,
            ),
            (
                CompanyCashCategory::OwnerWithdrawal,
                MAX_COMPANY_ACCOUNTING_TOTAL,
            ),
        ]
        .into(),
        monetary_entry_count: MAX_COMPANY_ACCOUNTING_TOTAL,
        recent_entries,
        unpriced_stock_in_units: 0,
        unpriced_stock_out_units: 0,
        personal_vehicle_used: company.drivers.iter().any(|d| d.vehicle_id.is_some()),
    });
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn accounting_exhaustion_rejects_company_and_owner_payments_without_any_partial_mutation() {
    let mut f = Fixture::new();
    f.fund_and_assign(10_000);
    fill_record_capacity(&mut f);
    let before = serde_json::to_value(&f.world).unwrap();
    for action in [
        CompanyCommand::Deposit { amount: 1 },
        CompanyCommand::Withdraw { amount: 1 },
        CompanyCommand::FundPublicServices { amount: 1 },
        CompanyCommand::HireDriver {
            name: "Second".into(),
        },
        CompanyCommand::OpenDepot {
            facility_id: f.garage,
        },
    ] {
        assert!(matches!(f.company(action), Err(SandboxError::Capacity)));
        assert_eq!(serde_json::to_value(&f.world).unwrap(), before);
    }
    assert!(matches!(
        f.transport(TransportCommand::Refuel {
            vehicle_id: f.vehicle,
            facility_id: f.garage,
            litres: 1,
        }),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(serde_json::to_value(&f.world).unwrap(), before);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn exhausted_accounting_holds_payroll_but_allows_an_already_present_load_to_refund() {
    let mut f = Fixture::new();
    let driver = f.fund_and_assign(10_000);
    f.company(CompanyCommand::SetDriverRunning {
        driver_id: driver,
        running: true,
    })
    .unwrap();
    f.world.advance_transport(0.1).unwrap();
    assert!(f.world.transport.companies[&1].drivers[0].trip.is_some());
    let conserved = money(&f.world);
    fill_record_capacity(&mut f);
    let before = f.report();
    let payroll = f.world.transport.companies[&1].payroll_paid();
    let fraction = f.world.transport.companies[&1].drivers[0].wage_fraction;
    let far = LocalFrame::new(f.world.transport.facilities[&f.garage].position)
        .to_geo([40., 0., 0.])
        .unwrap();
    f.world.living.units.get_mut(&f.vehicle).unwrap().position = far;
    f.world.prepare_company_drivers(0.25).unwrap();
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].status,
        FleetStatus::AccountingFull
    );
    assert_eq!(f.world.transport.companies[&1].payroll_paid(), payroll);
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].wage_fraction,
        fraction
    );
    assert_eq!(f.report(), before);
    assert_eq!(money(&f.world), conserved);
    f.world.prepare_company_drivers(0.5).unwrap();
    assert_eq!(f.world.transport.companies[&1].payroll_paid(), payroll);
    assert_eq!(
        f.world.transport.companies[&1].drivers[0].wage_fraction,
        fraction
    );
    assert_eq!(f.report(), before);
    // Independent refund admission still works at the original pickup. This
    // moves escrow back to its issuer, not through the full company account.
    f.world.living.units.get_mut(&f.vehicle).unwrap().position =
        f.world.transport.facilities[&f.garage].position;
    f.company(CompanyCommand::ReturnLoad { driver_id: driver })
        .unwrap();
    f.world.advance_transport(0.1).unwrap();
    assert!(f.world.transport.companies[&1].drivers[0].trip.is_none());
    assert_eq!(f.report(), before);
    assert_eq!(money(&f.world), conserved);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn compound_service_rejects_atomically_when_only_its_first_receipt_would_fit() {
    let mut f = Fixture::new();
    f.fund_and_assign(10_000);
    let remote = LocalFrame::new(f.world.transport.facilities[&f.garage].position)
        .to_geo([40., 0., 0.])
        .unwrap();
    f.world.living.units.get_mut(&f.vehicle).unwrap().position = remote;
    f.world.set_player_position(1, remote).unwrap();
    f.world
        .transport
        .vehicles
        .get_mut(&f.vehicle)
        .unwrap()
        .condition = 0.9;
    fill_record_capacity(&mut f);
    let accounting = f
        .world
        .transport
        .companies
        .get_mut(&1)
        .unwrap()
        .accounting
        .as_mut()
        .unwrap();
    accounting.monetary_entry_count -= 1;
    for entry in &mut accounting.recent_entries {
        entry.sequence -= 1;
    }
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let quote = f
        .world
        .company_roadside_quote(1, f.vehicle, f.garage, 1, true)
        .unwrap();
    assert!(quote.callout_cost > 0 && quote.fuel_cost > 0 && quote.repair_cost > 0);
    let before = serde_json::to_value(&f.world).unwrap();
    assert!(matches!(
        f.company(CompanyCommand::RoadsideService {
            vehicle_id: f.vehicle,
            facility_id: f.garage,
            litres: 1,
            repair: true,
            quoted_total: quote.total_cost,
        }),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(serde_json::to_value(&f.world).unwrap(), before);
}

#[test]
fn delivery_accounting_exhaustion_retains_people_escrow_and_revenue_until_it_can_commit() {
    let mut f = Fixture::new();
    let driver = f.fund_and_assign(10_000);
    f.company(CompanyCommand::SetDriverRunning {
        driver_id: driver,
        running: true,
    })
    .unwrap();
    f.world.advance_transport(0.1).unwrap();
    let trip = f.world.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    // This fault-injection case begins from a valid pending-arrival snapshot.
    // The paired career tests separately establish ordinary physical movement.
    let distance = f.world.transport.facilities[&trip.pickup]
        .position
        .surface_distance_m(f.world.transport.facilities[&trip.destination].position);
    f.world.living.units.get_mut(&f.vehicle).unwrap().position =
        f.world.transport.facilities[&trip.destination].position;
    f.world
        .transport
        .vehicles
        .get_mut(&f.vehicle)
        .unwrap()
        .odometer_m = trip.pickup_odometer_m + distance;
    f.world.transport.elapsed_seconds = trip.pickup_elapsed_seconds + distance / 10.;
    fill_record_capacity(&mut f);
    let report = f.report();
    let conserved = money(&f.world);
    let waiting = f.world.transport.facilities[&trip.destination].passengers_waiting;
    for _ in 0..3 {
        f.world.advance_transport(0.1).unwrap();
        let current = &f.world.transport.companies[&1];
        assert_eq!(current.drivers[0].status, FleetStatus::AccountingFull);
        assert_eq!(current.drivers[0].trip.as_ref(), Some(&trip));
        assert_eq!(current.drivers[0].deliveries_completed, 0);
        assert_eq!(
            f.world.transport.facilities[&trip.destination].passengers_waiting,
            waiting
        );
        assert_eq!(f.report(), report);
        assert_eq!(money(&f.world), conserved);
    }
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn imported_revenue_limit_blocks_new_sales_without_blocking_paid_stock_acquisition() {
    let mut f = Fixture::new();
    f.company(CompanyCommand::Deposit { amount: 10_000 })
        .unwrap();
    f.company(CompanyCommand::OpenDepot {
        facility_id: f.garage,
    })
    .unwrap();
    let company = f.world.transport.companies.get_mut(&1).unwrap();
    company.revenue = MAX_COMPANY_ACCOUNTING_TOTAL;
    company.accounting = None;
    f.world.validate_with_catalog(&f.catalog).unwrap();
    let quote = f
        .world
        .quote_company_depot_trade(1, f.garage, f.shop, "water", 1, true)
        .unwrap();
    f.company(CompanyCommand::TradeDepotStock {
        facility_id: f.garage,
        shop_id: f.shop,
        item: "water".into(),
        amount: 1,
        buy: true,
        quoted_total: quote.total_cost,
    })
    .unwrap();
    let report = f.report();
    assert_eq!(
        report.coverage,
        CompanyAccountingCoverage::ImportedOpeningBalance
    );
    assert_eq!(report.opening_revenue, MAX_COMPANY_ACCOUNTING_TOTAL);
    assert_eq!(report.operating.net(), -(quote.total_cost as i64));
    let sale = f.world.living.shops[&f.shop].prices["water"].sell;
    let before = serde_json::to_value(&f.world).unwrap();
    assert!(matches!(
        f.company(CompanyCommand::TradeDepotStock {
            facility_id: f.garage,
            shop_id: f.shop,
            item: "water".into(),
            amount: 1,
            buy: false,
            quoted_total: sale,
        }),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(serde_json::to_value(&f.world).unwrap(), before);
    f.world.validate_with_catalog(&f.catalog).unwrap();
}

#[test]
fn malformed_accounting_is_rejected_without_treating_a_broken_new_ledger_as_legacy() {
    let mut f = Fixture::new();
    f.fund_and_assign(10_000);
    for case in 0..9 {
        let mut value = serde_json::to_value(&f.world).unwrap();
        let company = &mut value["transport"]["companies"]["1"];
        match case {
            0 => company["cash"] = 8_999.into(),
            1 => company["accounting"]["totals"]["owner_deposit"] = 10_001.into(),
            2 => company["accounting"]["recent_entries"][2]["cash_after"] = 8_999.into(),
            3 => company["accounting"]["recent_entries"][2]["sequence"] = 2.into(),
            4 => company["accounting"]["recent_entries"][2]["elapsed_seconds"] = (-1).into(),
            5 => company["accounting"]["monetary_entry_count"] = 4.into(),
            6 => company["accounting"]["opening_cash"] = 1.into(),
            7 => {
                company["accounting"]["totals"]["owner_deposit"] =
                    (MAX_COMPANY_ACCOUNTING_TOTAL + 1).into()
            }
            _ => company["drivers"][0]["wages_earned"] = u64::MAX.into(),
        }
        let world: SandboxState = serde_json::from_value(value).unwrap();
        assert!(
            world.validate_with_catalog(&f.catalog).is_err(),
            "case {case}"
        );
    }
}

/// Receipts, wallet, treasury and supplied service are one transaction.
#[test]
fn rejected_assigned_vehicle_services_leave_no_phantom_owner_expense() {
    for failure in ["wallet", "treasury", "journal"] {
        for service in ["fuel", "repair", "paint", "engine", "brakes", "lease"] {
            let mut f = Fixture::new();
            if service == "lease" {
                let far = LocalFrame::new(f.world.transport.facilities[&f.garage].position)
                    .to_geo([80., 0., 0.])
                    .unwrap();
                f.world.living.units.get_mut(&f.vehicle).unwrap().position = far;
                f.vehicle = f
                    .transport(TransportCommand::RentVehicle {
                        model: VehicleModel::Pickup,
                        facility_id: f.garage,
                    })
                    .unwrap()
                    .created_id
                    .unwrap();
            }
            f.fund_and_assign(10_000);
            f.world
                .transport
                .vehicles
                .get_mut(&f.vehicle)
                .unwrap()
                .condition = 0.9;
            match failure {
                "wallet" => f.world.living.accounts.get_mut(&1).unwrap().money = 0,
                "treasury" => f.world.transport.treasury = MAX_MONEY,
                "journal" => fill_record_capacity(&mut f),
                _ => unreachable!(),
            }
            f.world.validate_with_catalog(&f.catalog).unwrap();
            let before = serde_json::to_value(&f.world).unwrap();
            let action = match service {
                "fuel" => TransportCommand::Refuel {
                    vehicle_id: f.vehicle,
                    facility_id: f.garage,
                    litres: 1,
                },
                "repair" => TransportCommand::Repair {
                    vehicle_id: f.vehicle,
                    facility_id: f.garage,
                },
                "paint" => TransportCommand::Paint {
                    vehicle_id: f.vehicle,
                    facility_id: f.garage,
                    paint: (f.world.transport.vehicles[&f.vehicle].paint + 1) % 8,
                },
                "engine" | "brakes" => TransportCommand::Upgrade {
                    vehicle_id: f.vehicle,
                    facility_id: f.garage,
                    engine: service == "engine",
                },
                "lease" => TransportCommand::RenewRental {
                    vehicle_id: f.vehicle,
                },
                _ => unreachable!(),
            };
            let result = f.transport(action);
            if failure == "wallet" {
                assert!(
                    matches!(result, Err(SandboxError::Insufficient(_))),
                    "{failure}/{service}: {result:?}"
                );
            } else {
                assert!(
                    matches!(result, Err(SandboxError::Capacity)),
                    "{failure}/{service}: {result:?}"
                );
            }
            assert_eq!(
                serde_json::to_value(&f.world).unwrap(),
                before,
                "{failure}/{service}"
            );
            f.world.validate_with_catalog(&f.catalog).unwrap();
        }
    }
}
