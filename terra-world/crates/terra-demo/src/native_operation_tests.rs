//! Real UI activation/queue/dispatch regressions for reviewed operations.
use super::actor_context_tests::{activate, app, disabled, label};
use super::*;
use bevy::ecs::system::RunSystemOnce;
use terra_bevy::rules::Command;
use terra_bevy::{SandboxIntent, SandboxQueue, rules::*};

fn dispatch(app: &mut App) {
    app.world_mut()
        .run_system_once(crate::interaction::actions)
        .unwrap();
}

fn apply_pending(app: &mut App) -> Command {
    let intent = app
        .world_mut()
        .resource_mut::<SandboxQueue>()
        .0
        .pop_front()
        .expect("reviewed command");
    let SandboxIntent::Command(command) = intent else {
        panic!("expected domain command")
    };
    let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
    let catalog = sandbox.catalog.clone();
    let player = sandbox.player_id;
    sandbox
        .state
        .apply(player, command.clone(), &catalog)
        .unwrap();
    command
}

fn industry_app() -> (App, u64, u64, u64) {
    let (mut app, _, other, _) = app();
    let (first, second) = {
        let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
        let ids: Vec<_> = sandbox
            .state
            .living
            .producers
            .keys()
            .copied()
            .take(2)
            .collect();
        let position = sandbox.state.living.producers[&ids[0]].position;
        for id in &ids {
            sandbox.state.living.producers.get_mut(id).unwrap().owner = Some(1);
        }
        sandbox.state.set_player_position(1, position).unwrap();
        (ids[0], ids[1])
    };
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.panel = Panel::Industry;
        ui.selected_site = Some(first);
    }
    app.update();
    (app, first, second, other)
}

#[test]
fn industry_activation_rejects_changed_explicit_target_state_actor_and_world() {
    for changed in 0..12 {
        let (mut app, first, second, other) = industry_app();
        assert!(!disabled(&mut app, |action| matches!(
            action,
            Action::ToggleProduction
        )));
        if changed == 10 {
            activate(&mut app, |action| matches!(action, Action::NextSite));
        }
        activate(&mut app, |action| {
            matches!(action, Action::ToggleProduction)
        });
        assert!(
            app.world()
                .resource::<UiQueue>()
                .0
                .iter()
                .any(|action| matches!(action, Action::IndustryReviewed { travel: false, .. }))
        );
        match changed {
            0 => app.world_mut().resource_mut::<UiState>().selected_site = Some(second),
            1 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .living
                    .producers
                    .remove(&first);
            }
            2 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .living
                    .producers
                    .get_mut(&first)
                    .unwrap()
                    .owner = Some(2)
            }
            3 => {
                let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
                let site = s.state.living.producers.get_mut(&first).unwrap();
                site.enabled = !site.enabled;
            }
            4 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .restore_revision += 1
            }
            5 => app.world_mut().resource_mut::<SandboxRuntime>().player_id = 2,
            6 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .living
                    .accounts
                    .get_mut(&1)
                    .unwrap()
                    .active_unit = other
            }
            7 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .set_player_dormant(1, true)
                    .unwrap();
            }
            8 => {
                let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
                let position = terra_core::LocalFrame::new(s.state.players[&1].position)
                    .to_geo([100., 0., 0.])
                    .unwrap();
                s.state.set_player_position(1, position).unwrap();
            }
            9 => {
                app.world_mut().resource_mut::<UiState>().selected_site = Some(second);
                app.update();
            }
            10 => {}
            _ => app
                .world_mut()
                .resource_mut::<crate::control::ControlQueue>()
                .0
                .push_back(crate::control::ControlIntent::Release),
        }
        let before = app.world().resource::<SandboxRuntime>().state.clone();
        dispatch(&mut app);
        assert!(
            app.world().resource::<SandboxQueue>().0.is_empty(),
            "case {changed}"
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            before,
            "case {changed}"
        );
    }
}

#[test]
fn industry_retains_desired_operation_through_progress_and_deliberate_recovery() {
    let (mut app, first, second, _) = industry_app();
    let enabled = app
        .world()
        .resource::<SandboxRuntime>()
        .state
        .living
        .producers[&first]
        .enabled;
    let operation = label(&mut app, |label| {
        matches!(label, Label::ProductionOperation)
    });
    assert!(operation.contains(if enabled { "Pause" } else { "Resume" }));
    activate(&mut app, |action| {
        matches!(action, Action::ToggleProduction)
    });
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        s.state
            .living
            .producers
            .get_mut(&first)
            .unwrap()
            .progress_seconds += 0.5;
        let position = terra_core::LocalFrame::new(s.state.players[&1].position)
            .to_geo([1., 0., 0.])
            .unwrap();
        s.state.set_player_position(1, position).unwrap();
    }
    dispatch(&mut app);
    let command = apply_pending(&mut app);
    assert!(
        matches!(command,Command::Living {action:LivingCommand::SetProduction {site_id,enabled:desired}} if site_id==first && desired!=enabled)
    );
    assert_eq!(
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .living
            .producers[&first]
            .enabled,
        !enabled
    );
    app.world_mut()
        .resource_mut::<SandboxRuntime>()
        .state
        .living
        .producers
        .remove(&first);
    app.update();
    assert!(
        selected_site(
            app.world().resource::<SandboxRuntime>(),
            app.world().resource::<UiState>()
        )
        .is_none()
    );
    assert!(label(&mut app, |label| matches!(label, Label::Industry)).contains("unavailable"));
    assert!(disabled(&mut app, |action| matches!(
        action,
        Action::ToggleProduction
    )));
    activate(&mut app, |action| matches!(action, Action::NextSite));
    dispatch(&mut app);
    assert_eq!(
        app.world().resource::<UiState>().selected_site,
        Some(second)
    );
}

fn company_command(app: &mut App, command: CompanyCommand) -> Outcome {
    let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
    let catalog = s.catalog.clone();
    s.state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::Company { action: command },
            },
            &catalog,
        )
        .unwrap()
}

fn fleet_app(assigned: bool) -> (App, u64, u64, u64, u64) {
    let (mut app, _, _, vehicle) = app();
    let garage = {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let facility = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .clone();
        s.state.set_player_position(1, facility.position).unwrap();
        facility.id
    };
    company_command(
        &mut app,
        CompanyCommand::Create {
            name: "Reviewed Transit".into(),
        },
    );
    company_command(&mut app, CompanyCommand::Deposit { amount: 15_000 });
    let driver = company_command(&mut app, CompanyCommand::HireDriver { name: "Ari".into() })
        .created_id
        .unwrap();
    let relief = company_command(&mut app, CompanyCommand::HireDriver { name: "Bea".into() })
        .created_id
        .unwrap();
    let stops = app
        .world()
        .resource::<SandboxRuntime>()
        .state
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Terminal)
        .take(2)
        .map(|f| f.id)
        .collect();
    let route = company_command(
        &mut app,
        CompanyCommand::CreateRoute {
            name: "Town service".into(),
            stops,
            service: FleetService::Passengers { count: 1 },
        },
    )
    .created_id
    .unwrap();
    if assigned {
        company_command(
            &mut app,
            CompanyCommand::Assign {
                driver_id: driver,
                vehicle_id: vehicle,
                route_id: route,
            },
        );
    }
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.panel = Panel::Fleet;
        ui.fleet.tab = crate::fleet::FleetTab::Drivers;
        ui.fleet.driver = Some(driver);
        ui.fleet.vehicle = Some(vehicle);
        ui.fleet.route = Some(route);
        ui.fleet.relief = Some(relief);
        ui.fleet.depot = Some(garage);
    }
    app.update();
    (app, driver, relief, vehicle, garage)
}

fn fleet_action(app: &mut App, action: crate::fleet::FleetAction) {
    let world = app.world_mut();
    let entity = world
        .query::<(Entity, &ActionButton)>()
        .iter(world)
        .find(|(_, button)| matches!(button.0,Action::Fleet(value) if value==action))
        .unwrap()
        .0;
    world.trigger(Activate { entity });
    world.flush();
}

#[test]
fn fleet_activation_rejects_changed_driver_assignment_actor_and_queued_selection() {
    use crate::fleet::FleetAction as A;
    for changed in 0..10 {
        let (mut app, driver, relief, _, _) = fleet_app(true);
        if changed == 0 {
            fleet_action(&mut app, A::NextDriver);
        }
        fleet_action(&mut app, A::Run);
        assert!(
            app.world()
                .resource::<UiQueue>()
                .0
                .iter()
                .any(|action| matches!(action, Action::FleetReviewed { action: A::Run, .. }))
        );
        match changed {
            0 => {}
            1 => app.world_mut().resource_mut::<UiState>().fleet.driver = Some(relief),
            2 => app
                .world_mut()
                .resource_mut::<SandboxRuntime>()
                .state
                .transport
                .companies
                .get_mut(&1)
                .unwrap()
                .drivers
                .retain(|d| d.id != driver),
            3 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .restore_revision += 1
            }
            4 => app.world_mut().resource_mut::<SandboxRuntime>().player_id = 2,
            5 => {
                let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
                let other = s
                    .state
                    .living
                    .units
                    .values()
                    .find(|unit| {
                        unit.owner == Some(1) && unit.id != s.state.living.accounts[&1].active_unit
                    })
                    .unwrap()
                    .id;
                s.state.living.accounts.get_mut(&1).unwrap().active_unit = other;
            }
            6 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .transport
                    .companies
                    .get_mut(&1)
                    .unwrap()
                    .drivers
                    .iter_mut()
                    .find(|d| d.id == driver)
                    .unwrap()
                    .running = true
            }
            7 => {
                app.world_mut().resource_mut::<UiState>().fleet.driver = Some(relief);
                app.update();
            }
            8 => app
                .world_mut()
                .resource_mut::<crate::control::ControlQueue>()
                .0
                .push_back(crate::control::ControlIntent::Release),
            _ => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .set_player_dormant(1, true)
                    .unwrap();
            }
        }
        let before = app.world().resource::<SandboxRuntime>().state.clone();
        dispatch(&mut app);
        assert!(
            app.world().resource::<SandboxQueue>().0.is_empty(),
            "case {changed}"
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            before,
            "case {changed}"
        );
    }
}

#[test]
fn fleet_normal_assign_run_pause_hold_and_release_keep_exact_driver_and_values() {
    use crate::fleet::FleetAction as A;
    let (mut app, driver, _, vehicle, _) = fleet_app(false);
    for action in [A::Assign, A::Run, A::Pause] {
        app.update();
        fleet_action(&mut app, action);
        dispatch(&mut app);
        apply_pending(&mut app);
    }
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.fleet.tab = crate::fleet::FleetTab::Recovery;
    }
    app.update();
    fleet_action(&mut app, A::Hold);
    dispatch(&mut app);
    apply_pending(&mut app);
    let s = &app.world().resource::<SandboxRuntime>().state;
    let employee = s.transport.companies[&1]
        .drivers
        .iter()
        .find(|d| d.id == driver)
        .unwrap();
    assert_eq!(employee.vehicle_id, Some(vehicle));
    assert_eq!(employee.status, FleetStatus::Held);
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.fleet.tab = crate::fleet::FleetTab::Drivers;
    }
    app.update();
    fleet_action(&mut app, A::Release);
    dispatch(&mut app);
    apply_pending(&mut app);
    assert!(
        app.world()
            .resource::<SandboxRuntime>()
            .state
            .transport
            .companies[&1]
            .drivers
            .iter()
            .find(|d| d.id == driver)
            .unwrap()
            .vehicle_id
            .is_none()
    );
}

#[test]
fn rental_countdown_preserves_reviewed_operations_but_expired_run_is_rejected() {
    use crate::fleet::FleetAction as A;
    let (mut app, driver, _, _, garage) = fleet_app(false);
    let rental = {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = s.catalog.clone();
        s.state
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
            .unwrap()
    };
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        // This checks a countdown tick, not a legitimate pickup transition.
        // Garage delivery can place a rental within reach of Town interchange.
        let position = terra_core::LocalFrame::new(s.state.players[&1].position)
            .to_geo([500., 0., 500.])
            .unwrap();
        assert!(
            s.state.transport.companies[&1].routes[0]
                .stops
                .iter()
                .all(|stop| {
                    position.surface_distance_m(s.state.transport.facilities[stop].position) > 20.
                })
        );
        s.state.living.units.get_mut(&rental).unwrap().position = position;
    }
    app.world_mut().resource_mut::<UiState>().fleet.vehicle = Some(rental);
    for action in [A::Assign, A::Run, A::Pause] {
        app.update();
        fleet_action(&mut app, action);
        let before_seconds = app
            .world()
            .resource::<SandboxRuntime>()
            .state
            .transport
            .vehicles[&rental]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds;
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .advance_transport(0.1)
            .unwrap();
        assert!(
            app.world()
                .resource::<SandboxRuntime>()
                .state
                .transport
                .vehicles[&rental]
                .rental
                .as_ref()
                .unwrap()
                .remaining_seconds
                < before_seconds
        );
        dispatch(&mut app);
        assert!(
            !app.world().resource::<SandboxQueue>().0.is_empty(),
            "rental operation {} rejected: {}",
            match action {
                A::Assign => "Assign",
                A::Run => "Run",
                _ => "Pause",
            },
            app.world().resource::<SandboxRuntime>().last_message
        );
        let command = apply_pending(&mut app);
        match action {
            A::Assign => assert!(matches!(command, Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::Assign {driver_id,vehicle_id,..}
                }
            } if driver_id == driver && vehicle_id == rental)),
            _ => assert!(matches!(command, Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::SetDriverRunning {driver_id,running}
                }
            } if driver_id == driver && running == (action == A::Run))),
        }
    }
    app.update();
    fleet_action(&mut app, A::Run);
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        while s.state.transport.vehicles[&rental]
            .rental
            .as_ref()
            .unwrap()
            .remaining_seconds
            > 0.
        {
            s.state.advance_transport(5.).unwrap();
        }
    }
    let before = app.world().resource::<SandboxRuntime>().state.clone();
    dispatch(&mut app);
    assert!(app.world().resource::<SandboxQueue>().0.is_empty());
    assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
    app.update();
    assert!(disabled(&mut app, |action| matches!(
        action,
        Action::Fleet(A::Run)
    )));
}

fn loaded_handover_app() -> (App, u64, u64, u64, u64) {
    let (mut app, driver, relief, original, garage) = fleet_app(true);
    let replacement = {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = s.catalog.clone();
        // Reassign existing scenario capital to buy a real replacement, keeping
        // the fixture's money finite and using the normal vehicle command.
        s.state.transport.treasury -= 30_000;
        s.state.living.accounts.get_mut(&1).unwrap().money += 30_000;
        s.state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::BuyVehicle {
                        model: VehicleModel::Pickup,
                        facility_id: garage,
                    },
                },
                &catalog,
            )
            .unwrap()
            .created_id
            .unwrap()
    };
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let first_stop = s.state.transport.companies[&1].routes[0].stops[0];
        let position = s.state.transport.facilities[&first_stop].position;
        s.state.living.units.get_mut(&original).unwrap().position = position;
    }
    company_command(
        &mut app,
        CompanyCommand::SetDriverRunning {
            driver_id: driver,
            running: true,
        },
    );
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = s.catalog.clone();
        s.state.advance_living(0.1, 0., &catalog).unwrap();
        assert!(s.state.transport.companies[&1].drivers[0].trip.is_some());
    }
    company_command(&mut app, CompanyCommand::HoldVehicle { driver_id: driver });
    {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let position = s.state.living.units[&original].position;
        s.state.living.units.get_mut(&replacement).unwrap().position =
            terra_core::LocalFrame::new(position)
                .to_geo([9., 0., 0.])
                .unwrap();
        s.state.set_player_position(1, position).unwrap();
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.fleet.tab = crate::fleet::FleetTab::Recovery;
        ui.fleet.vehicle = Some(replacement);
    }
    app.update();
    (app, driver, relief, original, replacement)
}

#[test]
fn loaded_fleet_handover_rechecks_review_and_preserves_the_exact_trip_and_bodies() {
    use crate::fleet::FleetAction as A;
    for changed in 0..5 {
        let (mut app, driver, relief, original, replacement) = loaded_handover_app();
        assert!(!disabled(&mut app, |action| matches!(
            action,
            Action::Fleet(A::Handover)
        )));
        fleet_action(&mut app, A::Handover);
        match changed {
            0 => app.world_mut().resource_mut::<UiState>().fleet.relief = Some(u64::MAX),
            1 => app.world_mut().resource_mut::<UiState>().fleet.vehicle = Some(original),
            2 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .restore_revision += 1
            }
            3 => {
                company_command(&mut app, CompanyCommand::ReturnLoad { driver_id: driver });
            }
            _ => {}
        }
        let before = app.world().resource::<SandboxRuntime>().state.clone();
        dispatch(&mut app);
        if changed < 4 {
            assert!(
                app.world().resource::<SandboxQueue>().0.is_empty(),
                "case {changed}"
            );
            assert_eq!(
                app.world().resource::<SandboxRuntime>().state,
                before,
                "case {changed}"
            );
            continue;
        }
        assert!(matches!(apply_pending(&mut app), Command::Transport {
            action: TransportCommand::Company {
                action: CompanyCommand::HandoverLoad {driver_id, relief_driver_id, vehicle_id}
            }
        } if driver_id == driver && relief_driver_id == relief && vehicle_id == replacement));
        let s = app.world().resource::<SandboxRuntime>();
        let old = &s.state.transport.companies[&1].drivers[0];
        let new = &s.state.transport.companies[&1].drivers[1];
        assert!(old.trip.is_none());
        assert_eq!(old.vehicle_id, None);
        assert_eq!(new.vehicle_id, Some(replacement));
        let mut expected_trip = before.transport.companies[&1].drivers[0]
            .trip
            .clone()
            .unwrap();
        expected_trip.transferred_distance_m += (before.transport.vehicles[&original].odometer_m
            - expected_trip.pickup_odometer_m)
            .max(0.);
        expected_trip.pickup_odometer_m = before.transport.vehicles[&replacement].odometer_m;
        assert_eq!(new.trip.as_ref(), Some(&expected_trip));
        assert_eq!(new.status, FleetStatus::Held);
        assert_eq!(
            s.state.transport.companies[&1].cash,
            before.transport.companies[&1].cash
        );
        assert_eq!(
            s.state.living.accounts[&1].money,
            before.living.accounts[&1].money
        );
        assert_eq!(s.state.transport.treasury, before.transport.treasury);
        for body in [original, replacement] {
            assert_eq!(
                s.state.living.units[&body].position,
                before.living.units[&body].position
            );
        }
        assert!(s.state.transport.vehicles[&original].cargo.is_empty());
        assert_eq!(
            s.state.transport.vehicles[&replacement].cargo,
            before.transport.vehicles[&original].cargo
        );
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }
}

#[test]
fn fleet_cash_and_depot_transfers_bind_amount_item_direction_and_context() {
    use crate::fleet::FleetAction as A;
    for changed in 0..4 {
        let (mut app, _, _, _, garage) = fleet_app(false);
        company_command(
            &mut app,
            CompanyCommand::OpenDepot {
                facility_id: garage,
            },
        );
        {
            let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
            s.state
                .players
                .get_mut(&1)
                .unwrap()
                .inventory
                .insert("bread".into(), 10);
        }
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.fleet.tab = crate::fleet::FleetTab::Depot;
            ui.fleet.item = 0;
            ui.fleet.quantity = 4;
        }
        app.update();
        fleet_action(&mut app, A::DepositGoods);
        match changed {
            0 => app.world_mut().resource_mut::<UiState>().fleet.quantity = 8,
            1 => app.world_mut().resource_mut::<UiState>().fleet.item = 1,
            2 => app.world_mut().resource_mut::<UiState>().fleet.depot = Some(u64::MAX),
            _ => {}
        }
        let before = app.world().resource::<SandboxRuntime>().state.clone();
        dispatch(&mut app);
        if changed < 3 {
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());
            assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
        } else {
            let command = apply_pending(&mut app);
            assert!(
                matches!(command,Command::Transport {action:TransportCommand::Company {action:CompanyCommand::TransferDepotStock {facility_id,amount:4,deposit:true,..}}} if facility_id==garage)
            );
            assert_eq!(
                app.world().resource::<SandboxRuntime>().state.players[&1].inventory["bread"],
                6
            );
            app.update();
            fleet_action(&mut app, A::WithdrawGoods);
            dispatch(&mut app);
            apply_pending(&mut app);
            assert_eq!(
                app.world().resource::<SandboxRuntime>().state.players[&1].inventory["bread"],
                10
            );
        }
    }
    let (mut app, _, _, _, _) = fleet_app(false);
    app.world_mut().resource_mut::<UiState>().fleet.tab = crate::fleet::FleetTab::Company;
    app.update();
    fleet_action(&mut app, A::Deposit);
    app.world_mut().resource_mut::<UiState>().fleet.transfer = 1_000;
    let before = app.world().resource::<SandboxRuntime>().state.clone();
    dispatch(&mut app);
    assert!(app.world().resource::<SandboxQueue>().0.is_empty());
    assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
    app.update();
    fleet_action(&mut app, A::Deposit);
    dispatch(&mut app);
    assert!(matches!(
        apply_pending(&mut app),
        Command::Transport {
            action: TransportCommand::Company {
                action: CompanyCommand::Deposit { amount: 1_000 }
            }
        }
    ));
}

fn building_app() -> (App, u64, u64) {
    let (mut app, _, other, _) = app();
    let structure = {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = s.catalog.clone();
        let origin = s.state.players[&1].position;
        // Place within reach, away from the controlled avatar and its nearby
        // owned bodies; the real construction command must authorize it.
        let position = terra_core::LocalFrame::new(origin)
            .to_geo([12., 0., 0.])
            .unwrap();
        s.state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("planks".into(), 20);
        s.state
            .apply(
                1,
                Command::ClaimPlot {
                    center: origin,
                    radius_m: 40.,
                },
                &catalog,
            )
            .unwrap();
        s.state
            .apply(
                1,
                Command::Build {
                    blueprint: "fence".into(),
                    position,
                },
                &catalog,
            )
            .unwrap()
            .created_id
            .unwrap()
    };
    app.world_mut().resource_mut::<UiState>().panel = Panel::Build;
    app.update();
    (app, structure, other)
}

fn review_damage(app: &mut App) {
    activate(app, |action| matches!(action, Action::Damage));
    dispatch(app);
    assert!(crate::building_review::has_review(
        app.world().resource::<UiState>()
    ));
    app.update();
}

#[test]
fn building_damage_is_named_cancelable_and_one_use_with_exact_health() {
    let (mut app, id, _) = building_app();
    assert!(
        label(&mut app, |label| matches!(label, Label::DamageTarget))
            .contains(&format!("fence #{id}"))
    );
    let before = app.world().resource::<SandboxRuntime>().state.clone();
    review_damage(&mut app);
    assert!(app.world().resource::<SandboxQueue>().0.is_empty());
    assert!(label(&mut app, |label| matches!(label, Label::DamageReview)).contains("Health:"));
    activate(&mut app, |action| matches!(action, Action::CancelDamage));
    dispatch(&mut app);
    assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
    review_damage(&mut app);
    activate(&mut app, |action| matches!(action, Action::ConfirmDamage));
    let queued = *app.world().resource::<UiQueue>().0.back().unwrap();
    dispatch(&mut app);
    assert!(
        matches!(apply_pending(&mut app),Command::Damage {structure_id,amount:25} if structure_id==id)
    );
    assert_eq!(
        app.world().resource::<SandboxRuntime>().state.structures[&id].hitpoints,
        before.structures[&id].hitpoints - 25
    );
    app.world_mut()
        .resource_mut::<UiQueue>()
        .0
        .push_back(queued);
    dispatch(&mut app);
    assert!(app.world().resource::<SandboxQueue>().0.is_empty());
}

#[test]
fn building_confirmation_rejects_changed_structure_health_authority_actor_and_restore() {
    for changed in 0..8 {
        let (mut app, id, other) = building_app();
        review_damage(&mut app);
        activate(&mut app, |action| matches!(action, Action::ConfirmDamage));
        match changed {
            0 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .structures
                    .remove(&id);
            }
            1 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .structures
                    .get_mut(&id)
                    .unwrap()
                    .hitpoints -= 1
            }
            2 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .structures
                    .get_mut(&id)
                    .unwrap()
                    .owner = 2
            }
            3 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .restore_revision += 1
            }
            4 => app.world_mut().resource_mut::<SandboxRuntime>().player_id = 2,
            5 => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .living
                    .accounts
                    .get_mut(&1)
                    .unwrap()
                    .active_unit = other
            }
            6 => {
                let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
                let position = terra_core::LocalFrame::new(s.state.players[&1].position)
                    .to_geo([100., 0., 0.])
                    .unwrap();
                s.state.set_player_position(1, position).unwrap();
            }
            _ => {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .structures
                    .get_mut(&id)
                    .unwrap()
                    .blueprint = "house".into()
            }
        }
        let before = app.world().resource::<SandboxRuntime>().state.clone();
        dispatch(&mut app);
        assert!(
            app.world().resource::<SandboxQueue>().0.is_empty(),
            "case {changed}"
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            before,
            "case {changed}"
        );
    }
}

#[test]
fn missing_faction_remains_selected_until_deliberate_browsing() {
    let (mut app, _, _, _) = app();
    let faction = {
        let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
        let catalog = s.catalog.clone();
        s.state
            .apply(
                1,
                Command::CreateFaction {
                    name: "Current community".into(),
                },
                &catalog,
            )
            .unwrap()
            .created_id
            .unwrap()
    };
    {
        let mut ui = app.world_mut().resource_mut::<UiState>();
        ui.panel = Panel::Factions;
        ui.selected_faction = Some(u64::MAX);
    }
    app.update();
    activate(&mut app, |action| matches!(action, Action::InviteFaction));
    dispatch(&mut app);
    assert_eq!(
        app.world().resource::<UiState>().selected_faction,
        Some(u64::MAX)
    );
    assert!(!app.world().resource::<UiState>().editing_gps);
    assert!(!disabled(&mut app, |action| matches!(
        action,
        Action::NextFaction
    )));
    activate(&mut app, |action| matches!(action, Action::NextFaction));
    dispatch(&mut app);
    assert_eq!(
        app.world().resource::<UiState>().selected_faction,
        Some(faction)
    );
}
