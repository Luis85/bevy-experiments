use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, u64, Vec<u64>) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::new();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let garage = state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .position;
    state.set_player_position(1, garage).unwrap();
    let vehicle = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    let stops: Vec<_> = state
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Terminal)
        .take(2)
        .map(|f| f.id)
        .collect();
    assert_eq!(stops.len(), 2);
    (state, catalog, vehicle, stops)
}

fn apply(state: &mut SandboxState, catalog: &Catalog, command: CompanyCommand) -> Outcome {
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::Company { action: command },
            },
            catalog,
        )
        .unwrap()
}

fn guide(state: &SandboxState, catalog: &Catalog) -> ActivityGuidance {
    let before = state.clone();
    let result = state.activity_guidance(1, catalog, GuideTrack::Company, None);
    assert_eq!(
        *state, before,
        "deriving a company briefing must not alter authority"
    );
    result
}

fn focus(guide: &ActivityGuidance) -> &GuideCompanyFocus {
    let Some(GuideAction::OpenCompany(focus)) = &guide.action else {
        panic!("{guide:?}")
    };
    focus
}

fn company(state: &mut SandboxState, catalog: &Catalog, vehicle: u64, stops: Vec<u64>) -> u64 {
    apply(
        state,
        catalog,
        CompanyCommand::Create {
            name: "Valley Transit".into(),
        },
    );
    apply(state, catalog, CompanyCommand::Deposit { amount: 10_000 });
    let driver = apply(
        state,
        catalog,
        CompanyCommand::HireDriver { name: "Ari".into() },
    )
    .created_id
    .unwrap();
    let route = apply(
        state,
        catalog,
        CompanyCommand::CreateRoute {
            name: "Town service".into(),
            stops,
            service: FleetService::Passengers { count: 1 },
        },
    )
    .created_id
    .unwrap();
    apply(
        state,
        catalog,
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: vehicle,
            route_id: route,
        },
    );
    driver
}

#[test]
fn company_setup_guidance_follows_real_paid_commands_without_a_second_progress_ledger() {
    let (mut state, catalog, vehicle, stops) = world();
    let wallet = state.living.accounts[&1].money;
    assert_eq!(guide(&state, &catalog).step, 1);
    apply(
        &mut state,
        &catalog,
        CompanyCommand::Create {
            name: "Valley Transit".into(),
        },
    );
    assert_eq!(
        state.living.accounts[&1].money,
        wallet - COMPANY_REGISTRATION_PRICE
    );
    assert_eq!(guide(&state, &catalog).step, 2);
    apply(
        &mut state,
        &catalog,
        CompanyCommand::Deposit { amount: 5_000 },
    );
    assert_eq!(guide(&state, &catalog).step, 3);
    let driver = apply(
        &mut state,
        &catalog,
        CompanyCommand::HireDriver { name: "Ari".into() },
    )
    .created_id
    .unwrap();
    let plan = guide(&state, &catalog);
    assert_eq!(focus(&plan).section, GuideCompanySection::Routes);
    let route = apply(
        &mut state,
        &catalog,
        CompanyCommand::CreateRoute {
            name: "Town service".into(),
            stops,
            service: FleetService::Passengers { count: 1 },
        },
    )
    .created_id
    .unwrap();
    let assign = guide(&state, &catalog);
    assert_eq!(focus(&assign).driver_id, Some(driver));
    assert_eq!(focus(&assign).section, GuideCompanySection::Drivers);
    apply(
        &mut state,
        &catalog,
        CompanyCommand::Assign {
            driver_id: driver,
            vehicle_id: vehicle,
            route_id: route,
        },
    );
    let ready = guide(&state, &catalog);
    assert_eq!(ready.step, 5);
    assert_eq!(focus(&ready).status, Some(FleetStatus::Paused));
    assert_eq!(focus(&ready).vehicle_id, Some(vehicle));
    assert_eq!(state.transport.companies[&1].revenue, 0);
    assert_eq!(state.transport.companies[&1].cash, 4_000);
    assert_eq!(
        state.living.accounts[&1].money,
        wallet - COMPANY_REGISTRATION_PRICE - 5_000
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn held_loaded_service_resumes_guidance_and_preserves_its_physical_return_commitment() {
    let (mut state, catalog, vehicle, stops) = world();
    let pickup = stops[0];
    let driver = company(&mut state, &catalog, vehicle, stops);
    state.living.units.get_mut(&vehicle).unwrap().position =
        state.transport.facilities[&pickup].position;
    apply(
        &mut state,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    state.advance_living(0.1, 0., &catalog).unwrap();
    assert!(state.transport.companies[&1].drivers[0].trip.is_some());
    apply(
        &mut state,
        &catalog,
        CompanyCommand::HoldVehicle { driver_id: driver },
    );
    let held = guide(&state, &catalog);
    assert_eq!(focus(&held).section, GuideCompanySection::Recovery);
    assert_eq!(focus(&held).status, Some(FleetStatus::Held));
    let before = state.clone();
    state.advance_living(1., 0., &catalog).unwrap();
    assert_eq!(
        state.transport.companies[&1].cash,
        before.transport.companies[&1].cash
    );
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let resumed = guide(&restored, &catalog);
    assert_eq!(held.description(), resumed.description());
    assert!(
        held.action
            .as_ref()
            .unwrap()
            .same_intent(resumed.action.as_ref().unwrap())
    );
    let trip = state.transport.companies[&1].drivers[0]
        .trip
        .clone()
        .unwrap();
    apply(
        &mut state,
        &catalog,
        CompanyCommand::ReturnLoad { driver_id: driver },
    );
    let returning = guide(&state, &catalog);
    assert!(
        !held
            .action
            .unwrap()
            .same_intent(returning.action.as_ref().unwrap())
    );
    assert_eq!(focus(&returning).facility_id, Some(trip.pickup));
    assert!(returning.detail.contains("Return pickup"));
    assert!(returning.detail.contains("not a paid delivery"));
    assert_eq!(
        state.transport.companies[&1].drivers[0]
            .trip
            .as_ref()
            .unwrap()
            .reward_escrow,
        trip.reward_escrow
    );
    assert_eq!(state.transport.companies[&1].revenue, 0);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn source_failure_briefing_names_recovery_and_cannot_restart_the_vehicle() {
    let (mut state, catalog, vehicle, stops) = world();
    let pickup = stops[0];
    let driver = company(&mut state, &catalog, vehicle, stops);
    state.living.units.get_mut(&vehicle).unwrap().position =
        state.transport.facilities[&pickup].position;
    apply(
        &mut state,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    state.advance_living(0.1, 0., &catalog).unwrap();
    for _ in 0..40 {
        state
            .advance_living_with_environment(1., 0., &catalog, |_, _, _| None)
            .unwrap();
    }
    let blocked = guide(&state, &catalog);
    assert_eq!(focus(&blocked).status, Some(FleetStatus::RouteBlocked));
    assert_eq!(focus(&blocked).section, GuideCompanySection::Recovery);
    assert!(blocked.detail.contains("Payroll is paused"));
    assert!(blocked.detail.contains("neither bypasses terrain"));
    let cash = state.transport.companies[&1].cash;
    state.advance_living(1., 0., &catalog).unwrap();
    assert_eq!(state.transport.companies[&1].cash, cash);
    assert_eq!(
        state.transport.companies[&1].drivers[0].status,
        FleetStatus::RouteBlocked
    );
    apply(
        &mut state,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    state.advance_living(1., 0., &catalog).unwrap();
    assert!(state.transport.companies[&1].cash < cash);
}

#[test]
fn suggested_journey_keeps_needs_and_accepted_work_ahead_of_owned_company_and_explicit_goals() {
    let (mut state, catalog, vehicle, stops) = world();
    company(&mut state, &catalog, vehicle, stops);
    state.advance_settlements(0., &catalog).unwrap();
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Suggested, None)
            .track,
        GuideTrack::Company
    );
    let contract = *state.settlements.contracts.keys().next().unwrap();
    state
        .apply(
            1,
            Command::Settlement {
                action: SettlementCommand::AcceptContract {
                    contract_id: contract,
                },
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Suggested, None)
            .track,
        GuideTrack::Supply
    );
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs.water = 0.1;
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Suggested, None)
            .track,
        GuideTrack::Suggested
    );
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Company, None)
            .track,
        GuideTrack::Company
    );
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Home, None)
            .track,
        GuideTrack::Home
    );
}

#[test]
fn company_focus_rejects_retargeted_service_but_not_routine_payroll_or_movement() {
    let (mut state, catalog, vehicle, stops) = world();
    company(&mut state, &catalog, vehicle, stops);
    let shown = guide(&state, &catalog);
    let action = shown.action.as_ref().unwrap();
    let mut current = focus(&shown).clone();
    for mutation in 0..7 {
        let mut changed = current.clone();
        match mutation {
            0 => changed.owner += 1,
            1 => changed.driver_id = Some(99),
            2 => changed.vehicle_id = Some(99),
            3 => changed.route_id = Some(99),
            4 => changed.next_stop = Some(1),
            5 => changed.status = Some(FleetStatus::Held),
            _ => changed.running = true,
        }
        assert!(!action.same_intent(&GuideAction::OpenCompany(changed)));
    }
    state.living.units.get_mut(&vehicle).unwrap().position =
        LocalFrame::new(state.players[&1].position)
            .to_geo([30., 0., 30.])
            .unwrap();
    state.transport.companies.get_mut(&1).unwrap().drivers[0].wages_earned += 1;
    let updated = guide(&state, &catalog);
    assert!(action.same_intent(updated.action.as_ref().unwrap()));
    current.section = GuideCompanySection::Finances;
    assert!(!action.same_intent(&GuideAction::OpenCompany(current)));
}

#[test]
fn briefing_prioritizes_owned_recovery_and_does_not_reveal_another_company() {
    let (mut state, catalog, vehicle, stops) = world();
    company(&mut state, &catalog, vehicle, stops);
    apply(
        &mut state,
        &catalog,
        CompanyCommand::HireDriver {
            name: "Spare".into(),
        },
    );
    let owner = state.transport.companies.get_mut(&1).unwrap();
    owner.drivers[0].status = FleetStatus::NeedsFuel;
    let expected = owner.drivers[0].id;
    owner.drivers.swap(0, 1);
    let briefing = guide(&state, &catalog);
    assert_eq!(focus(&briefing).driver_id, Some(expected));
    assert!(briefing.detail.contains("Refuel the fleet vehicle"));
    assert!(briefing.detail.contains("20 m"));
    let origin = state.players[&1].position;
    state.add_player(2, origin).unwrap();
    // A second seed call intentionally does nothing once the world exists.
    // A newly admitted principal must get its ordinary living account first.
    let loading = state.activity_guidance(2, &catalog, GuideTrack::Company, None);
    assert!(loading.action.is_none());
    assert!(!loading.description().contains("Valley Transit"));
    state.ensure_living_player(2).unwrap();
    let stranger = state.activity_guidance(2, &catalog, GuideTrack::Company, None);
    assert_eq!(stranger.step, 1);
    assert!(!stranger.description().contains("Valley Transit"));
    assert!(!stranger.description().contains("Ari"));
    assert_eq!(focus(&stranger).owner, 2);
    assert_eq!(focus(&stranger).driver_id, None);
}

#[test]
fn money_limits_and_empty_capacity_failures_do_not_promise_a_deposit_or_load_return_fix() {
    let (mut state, catalog, vehicle, stops) = world();
    let pickup = stops[0];
    let driver = company(&mut state, &catalog, vehicle, stops);
    state.transport.companies.get_mut(&1).unwrap().drivers[0].status = FleetStatus::DestinationFull;
    let empty = guide(&state, &catalog);
    assert_eq!(focus(&empty).section, GuideCompanySection::Routes);
    assert!(empty.detail.contains("no loaded trip to return"));
    let company = state.transport.companies.get_mut(&1).unwrap();
    company.drivers[0].status = FleetStatus::WaitingCompanyFunds;
    company.drivers[0].wages_earned = MAX_MONEY;
    let capped = guide(&state, &catalog);
    assert_eq!(focus(&capped).section, GuideCompanySection::Finances);
    assert!(capped.detail.contains("Additional company deposits cannot"));

    state.transport.companies.get_mut(&1).unwrap().drivers[0].wages_earned = 0;
    state.living.units.get_mut(&vehicle).unwrap().position =
        state.transport.facilities[&pickup].position;
    apply(
        &mut state,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    state.advance_living(0.1, 0., &catalog).unwrap();
    assert!(state.transport.companies[&1].drivers[0].trip.is_some());
    let company = state.transport.companies.get_mut(&1).unwrap();
    company.drivers[0].status = FleetStatus::DestinationFull;
    company.cash = MAX_MONEY;
    let payout = guide(&state, &catalog);
    assert_eq!(focus(&payout).section, GuideCompanySection::Company);
    assert!(payout.detail.contains("a deposit makes this limit worse"));
    state.transport.companies.get_mut(&1).unwrap().drivers[0]
        .trip
        .as_mut()
        .unwrap()
        .returning = true;
    let returning = guide(&state, &catalog);
    assert_eq!(focus(&returning).section, GuideCompanySection::Recovery);
    assert!(returning.detail.contains("refund-account capacity"));
    assert!(!returning.title.contains("delivery payment"));
}

#[test]
fn expired_rental_guidance_preserves_exact_vehicle_and_remote_renewal_rules() {
    let (mut state, catalog, _, stops) = world();
    let garage = state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    let vehicle = state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::RentVehicle {
                    model: VehicleModel::Compact,
                    facility_id: garage,
                },
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    let driver = company(&mut state, &catalog, vehicle, stops);
    state
        .transport
        .vehicles
        .get_mut(&vehicle)
        .unwrap()
        .rental
        .as_mut()
        .unwrap()
        .remaining_seconds = 0.;
    apply(
        &mut state,
        &catalog,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    state.prepare_company_drivers(0.1).unwrap();
    let expired = guide(&state, &catalog);
    assert_eq!(focus(&expired).section, GuideCompanySection::Garage);
    assert_eq!(focus(&expired).vehicle_id, Some(vehicle));
    assert_eq!(focus(&expired).facility_id, Some(garage));
    assert!(expired.detail.contains("renewal works from anywhere"));
    state
        .set_player_position(
            1,
            LocalFrame::new(state.players[&1].position)
                .to_geo([1000., 0., 1000.])
                .unwrap(),
        )
        .unwrap();
    let cash = state.transport.companies[&1].cash;
    let wallet = state.living.accounts[&1].money;
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::RenewRental {
                    vehicle_id: vehicle,
                },
            },
            &catalog,
        )
        .unwrap();
    assert!(state.living.accounts[&1].money < wallet);
    assert_eq!(state.transport.companies[&1].cash, cash);
    assert!(
        state.transport.vehicles[&vehicle]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds
            > 0.
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn legacy_company_guidance_does_not_invent_missing_accounting_or_resume_state() {
    let (mut state, catalog, vehicle, stops) = world();
    company(&mut state, &catalog, vehicle, stops);
    let expected = guide(&state, &catalog);
    let mut legacy = serde_json::to_value(&state).unwrap();
    legacy["transport"]["companies"]["1"]
        .as_object_mut()
        .unwrap()
        .remove("accounting");
    let restored: SandboxState = serde_json::from_value(legacy).unwrap();
    restored.validate_with_catalog(&catalog).unwrap();
    let observed = guide(&restored, &catalog);
    assert_eq!(expected.description(), observed.description());
    assert!(
        expected
            .action
            .unwrap()
            .same_intent(observed.action.as_ref().unwrap())
    );
    assert!(restored.transport.companies[&1].accounting.is_none());
    assert!(!restored.transport.companies[&1].drivers[0].running);
}
