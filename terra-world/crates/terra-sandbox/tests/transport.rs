use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use terra_core::{ActorKind, GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    state.add_player(1, origin).unwrap();
    state.add_player(2, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog)
}
fn action(
    state: &mut SandboxState,
    who: u64,
    action: TransportCommand,
    catalog: &Catalog,
) -> Result<Outcome, SandboxError> {
    state.apply(who, Command::Transport { action }, catalog)
}
fn vehicle(state: &SandboxState, owner: u64) -> u64 {
    state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == owner)
        .unwrap()
        .unit_id
}
fn garage(state: &SandboxState) -> u64 {
    state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id
}
fn freight(state: &SandboxState) -> u64 {
    state
        .transport
        .jobs
        .values()
        .find(|j| j.item.as_deref() == Some("wood"))
        .unwrap()
        .id
}
fn passenger(state: &SandboxState) -> u64 {
    state
        .transport
        .jobs
        .values()
        .find(|j| j.kind == TransportJobKind::Hitchhiker)
        .unwrap()
        .id
}
fn own_control(state: &mut SandboxState, catalog: &Catalog, owner: u64, id: u64) {
    let position = state.living.units[&id].position;
    state.set_player_position(owner, position).unwrap();
    action(
        state,
        owner,
        TransportCommand::EnterVehicle { vehicle_id: id },
        catalog,
    )
    .unwrap();
}
fn at_facility(state: &mut SandboxState, owner: u64, id: u64) {
    let position = state.transport.facilities[&id].position;
    state.set_player_position(owner, position).unwrap();
    let active = state.living.accounts[&owner].active_unit;
    state
        .living
        .units
        .get_mut(&active)
        .unwrap()
        .velocity_enu_m_s = [0.; 3];
}
fn money(state: &SandboxState) -> u64 {
    state.transport.treasury
        + state.living.accounts.values().map(|a| a.money).sum::<u64>()
        + state.living.shops.values().map(|s| s.money).sum::<u64>()
        + state
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
        + state
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| a.reward_escrow)
            .sum::<u64>()
}
fn goods(state: &SandboxState, item: &str) -> u64 {
    let quantity = |i: &Inventory| u64::from(i.get(item).copied().unwrap_or(0));
    state
        .living
        .producers
        .values()
        .map(|s| quantity(&s.stock))
        .sum::<u64>()
        + state
            .living
            .shops
            .values()
            .map(|s| quantity(&s.stock))
            .sum::<u64>()
        + state
            .transport
            .vehicles
            .values()
            .map(|v| quantity(&v.cargo))
            .sum::<u64>()
        + state
            .transport
            .jobs
            .values()
            .filter_map(|j| j.assignment.as_ref())
            .map(|a| quantity(&a.reserved_cargo))
            .sum::<u64>()
}
fn passengers(state: &SandboxState) -> u64 {
    state
        .transport
        .facilities
        .values()
        .map(|f| u64::from(f.passengers_waiting))
        .sum::<u64>()
        + state
            .transport
            .jobs
            .values()
            .filter(|j| j.assignment.is_some())
            .map(|j| u64::from(j.passengers))
            .sum::<u64>()
}
fn fuel(state: &SandboxState) -> f64 {
    state
        .transport
        .facilities
        .values()
        .map(|f| f.fuel_l)
        .sum::<f64>()
        + state
            .transport
            .vehicles
            .values()
            .map(|v| v.fuel_l)
            .sum::<f64>()
}

#[test]
fn manual_freight_and_refunds_respect_mixed_producer_storage_without_losing_escrow() {
    for loaded in [false, true] {
        let (mut state, catalog) = world();
        let vehicle_id = vehicle(&state, 1);
        let job_id = freight(&state);
        let job = state.transport.jobs[&job_id].clone();
        own_control(&mut state, &catalog, 1, vehicle_id);
        action(
            &mut state,
            1,
            TransportCommand::AcceptJob { job_id, vehicle_id },
            &catalog,
        )
        .unwrap();
        let target_facility = if loaded {
            at_facility(&mut state, 1, job.pickup_facility);
            action(
                &mut state,
                1,
                TransportCommand::PickupJob { job_id },
                &catalog,
            )
            .unwrap();
            job.destination_facility
        } else {
            job.pickup_facility
        };
        let site_id = state.transport.facilities[&target_facility]
            .linked_site
            .unwrap();
        let site = state.living.producers.get_mut(&site_id).unwrap();
        site.stock.clear();
        site.stock.insert("stone".into(), 1999);
        if loaded {
            at_facility(&mut state, 1, target_facility);
        }
        let before = state.clone();
        let command = if loaded {
            TransportCommand::CompleteJob { job_id }
        } else {
            TransportCommand::CancelJob { job_id }
        };
        assert!(action(&mut state, 1, command, &catalog).is_err());
        assert_eq!(
            state, before,
            "blocked unload/refund must leave all goods and funds untouched"
        );
        state
            .living
            .producers
            .get_mut(&site_id)
            .unwrap()
            .stock
            .insert("stone".into(), 1999 - job.amount);
        *state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .entry("stone".into())
            .or_default() += job.amount;
        let credits = money(&state);
        let wood = goods(&state, "wood");
        let command = if loaded {
            TransportCommand::CompleteJob { job_id }
        } else {
            TransportCommand::CancelJob { job_id }
        };
        action(&mut state, 1, command, &catalog).unwrap();
        assert!(state.transport.jobs[&job_id].assignment.is_none());
        assert_eq!(
            state.living.producers[&site_id].stock.values().sum::<u32>(),
            1999
        );
        assert_eq!(money(&state), credits);
        assert_eq!(goods(&state, "wood"), wood);
        state.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn character_creation_precedes_population_and_legacy_json_adds_no_currency() {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::default();
    let origin = GeoPosition::new(48., 11., 100.).unwrap();
    state.add_player(1, origin).unwrap();
    let profile = CharacterProfile {
        name: "Mara Rivera".into(),
        body_style: 2,
        skin: 5,
        hair: 3,
        outfit: 6,
    };
    state.set_character_profile(1, profile.clone()).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    assert_eq!(
        state.living.units[&state.living.accounts[&1].avatar_unit].name,
        profile.name
    );
    let mut old = serde_json::to_value(&state).unwrap();
    old.as_object_mut().unwrap().remove("transport");
    let legacy: SandboxState = serde_json::from_value(old).unwrap();
    legacy.validate_with_catalog(&catalog).unwrap();
    assert!(!legacy.transport.enabled);
    assert_eq!(legacy.transport.treasury, 0);
    assert_eq!(legacy.living, state.living);
    let before = state.clone();
    assert!(
        state
            .set_character_profile(
                1,
                CharacterProfile {
                    name: "\nName".into(),
                    ..profile
                }
            )
            .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn moving_vehicle_fatigues_its_avatar_in_physical_time_even_with_a_rest_order() {
    let (mut state, catalog) = world();
    let id = vehicle(&state, 1);
    own_control(&mut state, &catalog, 1, id);
    let avatar = state.living.accounts[&1].avatar_unit;
    let person = state.living.units.get_mut(&avatar).unwrap();
    person.needs = Needs {
        food: 1.,
        water: 1.,
        energy: 0.5,
    };
    person.order = UnitOrder::Rest;
    person.velocity_enu_m_s = [0.; 3];
    let parked_position = person.position;
    state.living.units.get_mut(&id).unwrap().velocity_enu_m_s = [5., 0., 0.];
    let mut accelerated_calendar = state.clone();

    state.advance_living(2., 0., &catalog).unwrap();
    accelerated_calendar
        .advance_living(2., 7_200., &catalog)
        .unwrap();
    let needs = state.living.units[&avatar].needs;
    let fast_needs = accelerated_calendar.living.units[&avatar].needs;
    assert!((needs.energy - 0.4993).abs() < 1e-6);
    assert_eq!(needs.energy, fast_needs.energy);
    assert!(fast_needs.food < needs.food && fast_needs.water < needs.water);
    assert_eq!(state.living.units[&avatar].position, parked_position);

    // Advancing only the calendar cannot charge driving fatigue again.
    state.advance_living(0., 60., &catalog).unwrap();
    assert_eq!(state.living.units[&avatar].needs.energy, needs.energy);

    // A stopped driver can recover, and releasing possession leaves ordinary
    // avatar rest behavior in place rather than consuming energy for an AI car.
    state.living.units.get_mut(&id).unwrap().velocity_enu_m_s = [0.; 3];
    state.advance_living(2., 0., &catalog).unwrap();
    let stopped_energy = state.living.units[&avatar].needs.energy;
    assert!(stopped_energy > needs.energy);
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::Release,
            },
            &catalog,
        )
        .unwrap();
    state.living.units.get_mut(&id).unwrap().velocity_enu_m_s = [5., 0., 0.];
    state.advance_living(2., 0., &catalog).unwrap();
    assert!(state.living.units[&avatar].needs.energy > stopped_energy);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn driver_needs_reduce_manual_vehicle_cap_without_changing_fleet_mechanics() {
    let (mut state, catalog) = world();
    let id = vehicle(&state, 1);
    let other = vehicle(&state, 2);
    let avatar = state.living.accounts[&1].avatar_unit;
    let mechanical = state.transport.vehicles[&id].speed_limit_m_s();
    let other_mechanical = state.transport.vehicles[&other].speed_limit_m_s();
    state.living.units.get_mut(&avatar).unwrap().needs.energy = 0.;

    // The owner's needs do not silently slow autonomous vehicles.
    assert_eq!(state.vehicle_speed_limit_m_s(id), Some(mechanical));
    own_control(&mut state, &catalog, 1, id);
    let depleted_cap = state.vehicle_speed_limit_m_s(id).unwrap();
    assert!((depleted_cap - mechanical * 0.35).abs() < 1e-6);
    assert_eq!(state.transport.vehicles[&id].speed_limit_m_s(), mechanical);
    assert_eq!(state.vehicle_speed_limit_m_s(other), Some(other_mechanical));

    let person = state.living.units.get_mut(&avatar).unwrap();
    person.needs.energy = 1.;
    person.needs.water = 0.1;
    let thirsty_cap = state.vehicle_speed_limit_m_s(id).unwrap();
    assert!(thirsty_cap > depleted_cap && thirsty_cap < mechanical * 0.5);
    state.living.units.get_mut(&avatar).unwrap().needs = Needs::default();
    assert_eq!(state.vehicle_speed_limit_m_s(id), Some(mechanical));

    state.living.units.get_mut(&avatar).unwrap().needs.energy = 0.;
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::Release,
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(state.vehicle_speed_limit_m_s(id), Some(mechanical));
    state.transport.vehicles.get_mut(&id).unwrap().fuel_l = 0.;
    assert_eq!(state.vehicle_speed_limit_m_s(id), Some(0.));
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn bootstrap_is_idempotent_and_registers_late_drivers_without_resetting_ledgers() {
    let (mut state, catalog) = world();
    state.transport.treasury -= 100;
    let v = vehicle(&state, 1);
    state.transport.vehicles.get_mut(&v).unwrap().fuel_l = 0.;
    let before = state.clone();
    state.ensure_transport_demo(&catalog).unwrap();
    assert_eq!(state, before);
    state.add_player(3, state.players[&1].position).unwrap();
    state.ensure_living_player(3).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    assert!(state.transport.drivers.contains_key(&3));
    assert!(state.transport.vehicles.values().any(|v| v.owner == 3));
    assert_eq!(state.transport.treasury, before.transport.treasury);
    assert_eq!(state.transport.vehicles[&v].fuel_l, 0.);
}

#[test]
fn purchase_refuel_upgrade_paint_and_sale_transfer_finite_money_and_fuel() {
    let (mut state, catalog) = world();
    let garage = garage(&state);
    at_facility(&mut state, 1, garage);
    let initial_money = money(&state);
    let initial_fuel = fuel(&state);
    let purchased = action(
        &mut state,
        1,
        TransportCommand::BuyVehicle {
            model: VehicleModel::Compact,
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap()
    .created_id
    .unwrap();
    assert_eq!(state.living.accounts[&1].money, 7_000);
    assert_eq!(money(&state), initial_money);
    assert!((fuel(&state) - initial_fuel).abs() < 1e-8);
    let spec_before = state.transport.vehicles[&purchased].dynamics_config();
    action(
        &mut state,
        1,
        TransportCommand::Upgrade {
            vehicle_id: purchased,
            facility_id: garage,
            engine: true,
        },
        &catalog,
    )
    .unwrap();
    action(
        &mut state,
        1,
        TransportCommand::Paint {
            vehicle_id: purchased,
            facility_id: garage,
            paint: 6,
        },
        &catalog,
    )
    .unwrap();
    action(
        &mut state,
        1,
        TransportCommand::Refuel {
            vehicle_id: purchased,
            facility_id: garage,
            litres: 5,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(money(&state), initial_money);
    assert!((fuel(&state) - initial_fuel).abs() < 1e-8);
    assert!(
        state.transport.vehicles[&purchased]
            .dynamics_config()
            .engine_power_kw
            > spec_before.engine_power_kw
    );
    assert_eq!(state.transport.vehicles[&purchased].paint, 6);
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::Refuel {
                vehicle_id: purchased,
                facility_id: garage,
                litres: 500
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(before, state);
    action(
        &mut state,
        1,
        TransportCommand::SellVehicle {
            vehicle_id: purchased,
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap();
    assert!(!state.living.units.contains_key(&purchased));
    assert_eq!(money(&state), initial_money);
    assert!((fuel(&state) - initial_fuel).abs() < 1e-8);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn ownership_reach_licensing_and_insufficient_funds_reject_atomically() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let garage = garage(&state);
    for (who, command) in [
        (2, TransportCommand::EnterVehicle { vehicle_id: v }),
        (
            1,
            TransportCommand::BuyVehicle {
                model: VehicleModel::Bus,
                facility_id: garage,
            },
        ),
        (
            1,
            TransportCommand::BuyLicense {
                license: DriverLicense::Passenger,
                facility_id: garage,
            },
        ),
    ] {
        let before = state.clone();
        assert!(action(&mut state, who, command, &catalog).is_err());
        assert_eq!(state, before);
    }
    state.transport.vehicles.get_mut(&v).unwrap().model = VehicleModel::Bus;
    let before = state.clone();
    assert!(
        state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::Possess { unit_id: v }
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    state.transport.vehicles.get_mut(&v).unwrap().model = VehicleModel::Pickup;
    let far = LocalFrame::new(state.players[&1].position)
        .to_geo([100., 50., 0.])
        .unwrap();
    state.set_player_position(1, far).unwrap();
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::EnterVehicle { vehicle_id: v },
            &catalog
        )
        .is_err()
    );
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::BuyVehicle {
                model: VehicleModel::Compact,
                facility_id: garage
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn freight_requires_pickup_and_arrival_and_pays_reserved_reward_once() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let id = freight(&state);
    let job = state.transport.jobs[&id].clone();
    own_control(&mut state, &catalog, 1, v);
    let initial = (money(&state), goods(&state, "wood"));
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!((money(&state), goods(&state, "wood")), initial);
    let accepted = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, accepted);
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!(state.transport.vehicles[&v].cargo["wood"], job.amount);
    assert_eq!((money(&state), goods(&state, "wood")), initial);
    let loaded = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert!(
        action(
            &mut state,
            2,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, loaded);
    at_facility(&mut state, 1, job.destination_facility);
    let before = state.living.accounts[&1].money;
    action(
        &mut state,
        1,
        TransportCommand::CompleteJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!(state.living.accounts[&1].money, before + job.reward);
    assert!(state.transport.vehicles[&v].cargo.is_empty());
    assert_eq!(state.transport.drivers[&1].jobs_completed, 1);
    assert_eq!((money(&state), goods(&state, "wood")), initial);
    let completed = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, completed);
}

#[test]
fn loaded_cancellation_preserves_cargo_until_physical_return_and_refunds_once() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let id = freight(&state);
    let job = state.transport.jobs[&id].clone();
    own_control(&mut state, &catalog, 1, v);
    let initial = (money(&state), goods(&state, "wood"));
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    action(
        &mut state,
        1,
        TransportCommand::CancelJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!((money(&state), goods(&state, "wood")), initial);
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    at_facility(&mut state, 1, job.destination_facility);
    action(
        &mut state,
        1,
        TransportCommand::CancelJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!(
        state.transport.jobs[&id].assignment.as_ref().unwrap().phase,
        TransportJobPhase::Returning
    );
    assert_eq!(state.transport.vehicles[&v].cargo["wood"], job.amount);
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::CancelJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!(state.transport.drivers[&1].jobs_completed, 0);
    assert_eq!((money(&state), goods(&state, "wood")), initial);
    assert!(state.transport.vehicles[&v].cargo.is_empty());
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CancelJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn passenger_trip_conserves_population_and_unlocks_paid_license() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let id = passenger(&state);
    let job = state.transport.jobs[&id].clone();
    let count = passengers(&state);
    let funds = money(&state);
    own_control(&mut state, &catalog, 1, v);
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(passengers(&state), count);
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    assert_eq!(
        state.transport.jobs[&id]
            .assignment
            .as_ref()
            .unwrap()
            .passengers_onboard,
        job.passengers
    );
    at_facility(&mut state, 1, job.destination_facility);
    action(
        &mut state,
        1,
        TransportCommand::CompleteJob { job_id: id },
        &catalog,
    )
    .unwrap();
    let garage = garage(&state);
    at_facility(&mut state, 1, garage);
    action(
        &mut state,
        1,
        TransportCommand::BuyLicense {
            license: DriverLicense::Passenger,
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap();
    assert!(
        state.transport.drivers[&1]
            .licenses
            .contains(&DriverLicense::Passenger)
    );
    assert_eq!(passengers(&state), count);
    assert_eq!(money(&state), funds);
}

#[test]
fn treasury_shortage_and_post_transfer_overflow_leave_entire_job_unchanged() {
    let (mut state, catalog) = world();
    let id = freight(&state);
    let v = vehicle(&state, 1);
    own_control(&mut state, &catalog, 1, v);
    state.transport.treasury = 0;
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::AcceptJob {
                job_id: id,
                vehicle_id: v
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
    state.transport.treasury = 100_000;
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    let job = state.transport.jobs[&id].clone();
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    at_facility(&mut state, 1, job.destination_facility);
    state.living.accounts.get_mut(&1).unwrap().money = MAX_MONEY;
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn expiry_uses_physical_time_and_requires_loaded_payload_return() {
    let (mut state, catalog) = world();
    let id = passenger(&state);
    let v = vehicle(&state, 1);
    own_control(&mut state, &catalog, 1, v);
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    let job = state.transport.jobs[&id].clone();
    at_facility(&mut state, 1, job.pickup_facility);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    state.advance_living(0.5, 3_600., &catalog).unwrap();
    assert_eq!(state.transport.elapsed_seconds, 0.5);
    assert_eq!(
        state.transport.jobs[&id].assignment.as_ref().unwrap().phase,
        TransportJobPhase::ToDestination
    );
    for _ in 0..121 {
        state.advance_transport(5.).unwrap();
    }
    assert_eq!(
        state.transport.jobs[&id].assignment.as_ref().unwrap().phase,
        TransportJobPhase::Returning
    );
    at_facility(&mut state, 1, job.destination_facility);
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::CompleteJob { job_id: id },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn trailer_capacity_coupling_and_loaded_unhitch_are_validated() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let garage = garage(&state);
    own_control(&mut state, &catalog, 1, v);
    at_facility(&mut state, 1, garage);
    let bulk = state
        .transport
        .jobs
        .values()
        .find(|j| j.kind == TransportJobKind::Freight && j.amount == 60)
        .unwrap()
        .id;
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::AcceptJob {
                job_id: bulk,
                vehicle_id: v
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
    let trailer = action(
        &mut state,
        1,
        TransportCommand::BuyTrailer {
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap()
    .created_id
    .unwrap();
    action(
        &mut state,
        1,
        TransportCommand::HitchTrailer {
            vehicle_id: v,
            trailer_id: trailer,
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(state.vehicle_cargo_capacity(v), 200);
    // Initial well stock is finite; provision this test's requested bulk load.
    let well = state.transport.facilities[&state.transport.jobs[&bulk].pickup_facility]
        .linked_site
        .unwrap();
    state
        .living
        .producers
        .get_mut(&well)
        .unwrap()
        .stock
        .insert("water".into(), 80);
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: bulk,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    let pickup = state.transport.jobs[&bulk].pickup_facility;
    at_facility(&mut state, 1, pickup);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: bulk },
        &catalog,
    )
    .unwrap();
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::UnhitchTrailer { vehicle_id: v },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
    state.record_vehicle_motion(1, v, 50., 2., 0.).unwrap();
    assert!(
        state.transport.trailers[&trailer]
            .position
            .surface_distance_m(state.living.units[&v].position)
            < 6.1
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn fuel_wear_and_rts_share_limits_and_reentry_releases_parking_brake() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    // This fixture seeds both crews at the same origin. Park this rover in
    // clear flat space before boarding so the later dismount does not cross
    // the other owner's overlapping rover.
    let mut parking = LocalFrame::new(state.living.units[&v].position)
        .to_geo([300., 0., 300.])
        .unwrap();
    parking.altitude_m = 100.;
    state.living.units.get_mut(&v).unwrap().position = parking;
    own_control(&mut state, &catalog, 1, v);
    let initial = state.transport.vehicles[&v].clone();
    state.record_vehicle_motion(1, v, 100., 5., 4.).unwrap();
    assert_eq!(state.transport.vehicles[&v].odometer_m, 100.);
    assert!(state.transport.vehicles[&v].fuel_l < initial.fuel_l);
    assert!(state.transport.vehicles[&v].condition < initial.condition);
    let before = state.clone();
    assert!(state.record_vehicle_motion(2, v, 10., 1., 0.).is_err());
    assert!(state.record_vehicle_motion(1, v, 100., 0.01, 0.).is_err());
    assert!(state.record_vehicle_motion(1, v, f64::NAN, 1., 0.).is_err());
    assert_eq!(state, before);
    action(&mut state, 1, TransportCommand::ExitVehicle, &catalog).unwrap();
    assert_eq!(state.transport.vehicles[&v].speed_limit_m_s(), 0.);
    action(
        &mut state,
        1,
        TransportCommand::EnterVehicle { vehicle_id: v },
        &catalog,
    )
    .unwrap();
    assert!(state.transport.vehicles[&v].speed_limit_m_s() > 0.);
    let position = state.living.units[&v].position;
    let destination = LocalFrame::new(position).to_geo([50., 0., 0.]).unwrap();
    action(
        &mut state,
        1,
        TransportCommand::SetAutopilot {
            vehicle_id: v,
            enabled: true,
            destination: Some(destination),
        },
        &catalog,
    )
    .unwrap();
    state.transport.vehicles.get_mut(&v).unwrap().fuel_l = 0.;
    state.advance_living(1., 0., &catalog).unwrap();
    assert!(state.living.units[&v].position.surface_distance_m(position) < 0.001);
    assert_eq!(state.transport.vehicles[&v].odometer_m, 100.);
    assert_eq!(state.transport.vehicles[&v].dynamics.engine_rpm, 0.);
    state.transport.vehicles.get_mut(&v).unwrap().fuel_l = 5.;
    state.advance_living(1., 0., &catalog).unwrap();
    assert!(state.transport.vehicles[&v].odometer_m > 100.);
    assert!(state.transport.vehicles[&v].fuel_l < 5.);
    assert!(state.transport.vehicles[&v].dynamics.speed_m_s > 0.);
    state
        .apply(
            1,
            Command::Living {
                action: LivingCommand::Possess { unit_id: v },
            },
            &catalog,
        )
        .unwrap();
    assert_eq!(state.transport.vehicles[&v].dynamics.speed_m_s, 0.);
    assert_eq!(state.transport.vehicles[&v].dynamics.lateral_speed_m_s, 0.);
    assert_eq!(state.transport.vehicles[&v].dynamics.yaw_rate_rad_s, 0.);
    assert!(!state.transport.vehicles[&v].autopilot);
}

#[test]
fn race_requires_ordered_proximity_mileage_and_positive_time_without_click_rewards() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let course = state.transport.courses.values().next().unwrap().clone();
    own_control(&mut state, &catalog, 1, v);
    state.set_player_position(1, course.checkpoints[0]).unwrap();
    let funds = money(&state);
    action(
        &mut state,
        1,
        TransportCommand::StartRace {
            course_id: course.id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    state.set_player_position(1, course.checkpoints[1]).unwrap();
    state.advance_transport(1.).unwrap();
    let before = state.clone();
    assert!(action(&mut state, 1, TransportCommand::RaceCheckpoint, &catalog).is_err());
    assert_eq!(state, before);
    state.set_player_position(1, course.checkpoints[0]).unwrap();
    for (index, gate) in course.checkpoints.iter().enumerate().skip(1) {
        let mut remaining = course.checkpoints[index - 1].surface_distance_m(*gate);
        while remaining > 0. {
            let segment = remaining.min(180.);
            state.record_vehicle_motion(1, v, segment, 5., 0.).unwrap();
            state.advance_transport(5.).unwrap();
            remaining -= segment;
        }
        state.set_player_position(1, *gate).unwrap();
        action(&mut state, 1, TransportCommand::RaceCheckpoint, &catalog).unwrap();
        let before = state.clone();
        assert!(action(&mut state, 1, TransportCommand::RaceCheckpoint, &catalog).is_err());
        assert_eq!(state, before);
    }
    assert!(!state.transport.races.contains_key(&1));
    assert_eq!(state.transport.personal_bests.len(), 1);
    assert!(state.transport.personal_bests[0].seconds > 0.);
    assert_eq!(money(&state), funds);
}

#[test]
fn transport_views_hide_other_private_ledgers_and_roundtrip_in_tagged_envelopes() {
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type")]
    enum Envelope {
        Snapshot { view: SandboxView },
    }
    let (mut state, catalog) = world();
    state
        .set_character_profile(1, CharacterProfile::default())
        .unwrap();
    state
        .set_character_profile(
            2,
            CharacterProfile {
                name: "Other driver".into(),
                outfit: 7,
                ..CharacterProfile::default()
            },
        )
        .unwrap();
    let v2 = vehicle(&state, 2);
    state.transport.vehicles.get_mut(&v2).unwrap().paint = 7;
    let garage = garage(&state);
    let trailer = action(
        &mut state,
        2,
        TransportCommand::BuyTrailer {
            facility_id: garage,
        },
        &catalog,
    )
    .unwrap()
    .created_id
    .unwrap();
    action(
        &mut state,
        2,
        TransportCommand::HitchTrailer {
            vehicle_id: v2,
            trailer_id: trailer,
        },
        &catalog,
    )
    .unwrap();
    let id = freight(&state);
    action(
        &mut state,
        2,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v2,
        },
        &catalog,
    )
    .unwrap();
    let view = state.snapshot_for(1, 500.).unwrap();
    view.transport.validate_replica(1, &view.living).unwrap();
    assert!(view.transport.vehicles.iter().all(|v| v.owner == 1));
    assert!(view.transport.jobs.iter().all(|j| j.id != id));
    assert_eq!(view.transport.driver.as_ref().unwrap().player_id, 1);
    assert_eq!(view.transport.visible_profiles[&2].name, "Other driver");
    let encoded = serde_json::to_vec(&Envelope::Snapshot { view }).unwrap();
    let Envelope::Snapshot { view } = serde_json::from_slice(&encoded).unwrap();
    view.transport.validate_replica(1, &view.living).unwrap();
    let client = TransportWorld::from_view(view.transport.clone(), 1);
    assert_eq!(client.treasury, 0);
    assert!(!client.drivers.contains_key(&2));
    assert_eq!(client.appearance(v2).unwrap().paint, 7);
    assert!(!client.vehicles.contains_key(&v2));
    assert_eq!(client.appearance(v2).unwrap().trailer, Some(trailer));
    assert_eq!(client.trailers[&trailer].attached_to, Some(v2));
    let mut corrupt = view.transport.clone();
    corrupt.vehicles.push(state.transport.vehicles[&v2].clone());
    assert!(corrupt.validate_replica(1, &view.living).is_err());
    let mut corrupt = view.transport.clone();
    corrupt.vehicles[0].cargo.insert("wood".into(), 1_000_000);
    assert!(corrupt.validate_replica(1, &view.living).is_err());
    let mut corrupt = view.transport.clone();
    corrupt
        .visible_profiles
        .insert(999, CharacterProfile::default());
    assert!(corrupt.validate_replica(1, &view.living).is_err());
    let mut corrupt = view.transport.clone();
    corrupt
        .visible_vehicles
        .iter_mut()
        .find(|v| v.unit_id == v2)
        .unwrap()
        .owner = 1;
    assert!(corrupt.validate_replica(1, &view.living).is_err());
    let mut corrupt = view.transport.clone();
    corrupt
        .trailers
        .iter_mut()
        .find(|t| t.id == trailer)
        .unwrap()
        .attached_to = None;
    assert!(corrupt.validate_replica(1, &view.living).is_err());
}

#[test]
fn active_cargo_driver_profile_and_handling_survive_save_and_regional_roundtrip() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let id = freight(&state);
    own_control(&mut state, &catalog, 1, v);
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    let pickup = state.transport.jobs[&id].pickup_facility;
    at_facility(&mut state, 1, pickup);
    action(
        &mut state,
        1,
        TransportCommand::PickupJob { job_id: id },
        &catalog,
    )
    .unwrap();
    state
        .transport
        .vehicles
        .get_mut(&v)
        .unwrap()
        .dynamics
        .steering_rad = 0.25;
    state
        .set_character_profile(
            1,
            CharacterProfile {
                name: "Persistent driver".into(),
                ..CharacterProfile::default()
            },
        )
        .unwrap();
    let decoded: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    decoded.validate_with_catalog(&catalog).unwrap();
    assert_eq!(decoded, state);
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    assert_eq!(partition.global.transport, state.transport);
    let active = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
    assert_eq!(active.state.transport, state.transport);
    assert_eq!(partition.assemble_all(&catalog).unwrap(), state);
    let mut corrupt = state.clone();
    corrupt
        .transport
        .vehicles
        .get_mut(&v)
        .unwrap()
        .cargo
        .clear();
    assert!(corrupt.validate_with_catalog(&catalog).is_err());
    let mut corrupt = state.clone();
    corrupt
        .transport
        .vehicles
        .get_mut(&v)
        .unwrap()
        .dynamics
        .steering_rad = f64::NAN;
    assert!(corrupt.validate().is_err());
}

#[test]
fn vehicle_purchase_respects_existing_global_body_capacity_including_cold_regions() {
    let (mut state, catalog) = world();
    let template = state
        .living
        .units
        .values()
        .find(|u| u.owner.is_none() && u.role == UnitRole::Citizen)
        .unwrap()
        .clone();
    let far = LocalFrame::new(state.players[&1].position)
        .to_geo([30_000., 0., 0.])
        .unwrap();
    while state.living.units.len() < MAX_LIVING_UNITS {
        let id = state.living.next_id;
        state.living.next_id += 1;
        let mut unit = template.clone();
        unit.id = id;
        unit.position = far;
        unit.owner = None;
        unit.kind = ActorKind::Human;
        unit.order = UnitOrder::Idle;
        unit.path.clear();
        unit.patrol.clear();
        unit.path_index = 0;
        state.living.units.insert(id, unit);
    }
    let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut active = partition
        .working_set(&BTreeSet::new(), &catalog)
        .unwrap()
        .state;
    assert!(active.living.units.len() < MAX_LIVING_UNITS);
    assert_eq!(active.living_unit_count(), MAX_LIVING_UNITS);
    let garage = garage(&active);
    at_facility(&mut active, 1, garage);
    let before = active.clone();
    assert!(matches!(
        action(
            &mut active,
            1,
            TransportCommand::BuyVehicle {
                model: VehicleModel::Compact,
                facility_id: garage
            },
            &catalog
        ),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(active, before);
}

#[test]
fn all_vehicle_definitions_have_valid_distinct_physical_capabilities() {
    let mut names = BTreeSet::new();
    for (n, model) in VEHICLE_MODELS.into_iter().enumerate() {
        let vehicle = VehicleState::new(n as u64 + 1, 1, model);
        vehicle.validate().unwrap();
        vehicle.dynamics_config().validate().unwrap();
        assert!(names.insert(model.spec().name));
        assert!(vehicle.speed_limit_m_s() <= 40.);
    }
    assert_eq!(names.len(), 9);
    assert_eq!(VehicleModel::Semi.spec().cargo_capacity, 0);
    assert!(VehicleModel::Bus.spec().passenger_seats > VehicleModel::Taxi.spec().passenger_seats);
}

#[test]
fn freight_cannot_consume_stock_reserved_for_existing_npc_couriers() {
    let (mut state, catalog) = world();
    state
        .living
        .producers
        .values_mut()
        .find(|s| s.kind == ProductionKind::Sawmill)
        .unwrap()
        .stock
        .remove("wood");
    state.advance_living(1., 0., &catalog).unwrap();
    let id = freight(&state);
    let job = state.transport.jobs[&id].clone();
    let source = state.transport.facilities[&job.pickup_facility]
        .linked_site
        .unwrap();
    let committed = state
        .living
        .deliveries
        .iter()
        .filter(|d| d.from_site == source && d.item == "wood" && d.phase == DeliveryPhase::ToPickup)
        .map(|d| d.amount)
        .sum::<u32>();
    assert!(committed > 0, "the authored economy must dispatch timber");
    state
        .living
        .producers
        .get_mut(&source)
        .unwrap()
        .stock
        .insert("wood".into(), committed);
    let v = vehicle(&state, 1);
    let before = state.clone();
    assert!(
        action(
            &mut state,
            1,
            TransportCommand::AcceptJob {
                job_id: id,
                vehicle_id: v
            },
            &catalog
        )
        .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn expired_uncollected_jobs_refund_without_loss_and_do_not_reset_issuer_capital() {
    let (mut state, catalog) = world();
    let v = vehicle(&state, 1);
    let id = freight(&state);
    let original = (money(&state), goods(&state, "wood"));
    let treasury = state.transport.treasury;
    action(
        &mut state,
        1,
        TransportCommand::AcceptJob {
            job_id: id,
            vehicle_id: v,
        },
        &catalog,
    )
    .unwrap();
    for _ in 0..121 {
        state.advance_transport(5.).unwrap();
    }
    assert!(state.transport.jobs[&id].assignment.is_none());
    assert_eq!(state.transport.treasury, treasury);
    assert_eq!((money(&state), goods(&state, "wood")), original);
    state.ensure_transport_demo(&catalog).unwrap();
    assert_eq!(state.transport.treasury, treasury);
}

#[test]
fn dormant_loaded_passenger_and_freight_jobs_keep_cargo_escrow_and_remaining_time() {
    for cargo_job in [false, true] {
        let (mut state, catalog) = world();
        let id = vehicle(&state, 1);
        own_control(&mut state, &catalog, 1, id);
        let job = if cargo_job {
            freight(&state)
        } else {
            passenger(&state)
        };
        action(
            &mut state,
            1,
            TransportCommand::AcceptJob {
                job_id: job,
                vehicle_id: id,
            },
            &catalog,
        )
        .unwrap();
        let pickup = state.transport.jobs[&job].pickup_facility;
        at_facility(&mut state, 1, pickup);
        action(
            &mut state,
            1,
            TransportCommand::PickupJob { job_id: job },
            &catalog,
        )
        .unwrap();
        assert_eq!(
            state.transport.jobs[&job]
                .assignment
                .as_ref()
                .unwrap()
                .phase,
            TransportJobPhase::ToDestination
        );
        let far = LocalFrame::new(state.players[&1].position)
            .to_geo([30_000., 0., 0.])
            .unwrap();
        for unit in state
            .living
            .units
            .values_mut()
            .filter(|u| u.owner == Some(1))
        {
            unit.position = far;
        }
        state.players.get_mut(&1).unwrap().position = far;
        state.set_player_dormant(1, true).unwrap();
        let bodies: Vec<_> = state
            .living
            .units
            .values()
            .filter(|u| u.owner == Some(1))
            .cloned()
            .collect();
        let saved_vehicle = state.transport.vehicles[&id].clone();
        let assignment = state.transport.jobs[&job]
            .assignment
            .as_ref()
            .unwrap()
            .clone();
        let remaining = assignment.deadline - state.transport.elapsed_seconds;
        let partition = RegionPartition::from_state(&state, 15, &catalog).unwrap();
        let mut active = partition.working_set(&BTreeSet::new(), &catalog).unwrap();
        assert!(!active.state.living.units.contains_key(&id));
        for _ in 0..400 {
            active.state.advance_living(5., 0., &catalog).unwrap();
        }
        let frozen = active.state.transport.jobs[&job]
            .assignment
            .as_ref()
            .unwrap();
        assert_eq!(frozen.phase, assignment.phase);
        assert_eq!(frozen.reward_escrow, assignment.reward_escrow);
        assert_eq!(frozen.passengers_onboard, assignment.passengers_onboard);
        assert_eq!(frozen.reserved_cargo, assignment.reserved_cargo);
        assert!(
            (frozen.deadline - active.state.transport.elapsed_seconds - remaining).abs() < 1e-6
        );
        assert_eq!(active.state.transport.vehicles[&id], saved_vehicle);
        active.state.validate_with_catalog(&catalog).unwrap();
        let mut resumed = partition
            .merge_active(&active.state, &active.active_regions, &catalog)
            .unwrap()
            .assemble_all(&catalog)
            .unwrap();
        resumed.set_player_dormant(1, false).unwrap();
        for body in bodies {
            assert_eq!(resumed.living.units[&body.id], body);
        }
        resumed.validate_with_catalog(&catalog).unwrap();
    }
}
