use terra_core::GeoPosition;
use terra_sandbox::*;

fn garage_world() -> (SandboxState, Catalog, u64) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.ensure_transport_demo(&catalog).unwrap();
    let garage = state
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    state
        .set_player_position(1, state.transport.facilities[&garage].position)
        .unwrap();
    (state, catalog, garage)
}

fn park_and_dismount(rental: bool) {
    let (mut state, catalog, facility_id) = garage_world();
    let action = if rental {
        TransportCommand::RentVehicle {
            model: VehicleModel::Compact,
            facility_id,
        }
    } else {
        TransportCommand::BuyVehicle {
            model: VehicleModel::Compact,
            facility_id,
        }
    };
    let id = state
        .apply(1, Command::Transport { action }, &catalog)
        .unwrap()
        .created_id
        .unwrap();
    let body = &state.living.units[&id];
    assert_eq!(
        state.spawn_obstruction(body).unwrap(),
        None,
        "garage delivery must not stack onto the starter rover or its driver"
    );
    assert!(
        body.position
            .surface_distance_m(state.transport.facilities[&facility_id].position)
            <= INTERACTION_REACH_M
    );
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle { vehicle_id: id },
            },
            &catalog,
        )
        .unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::ExitVehicle,
            },
            &catalog,
        )
        .unwrap();
    let avatar = &state.living.units[&state.living.accounts[&1].avatar_unit];
    assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn purchased_vehicle_has_clear_garage_parking_and_a_usable_exit() {
    park_and_dismount(false);
}

#[test]
fn rented_vehicle_has_clear_garage_parking_and_a_usable_exit() {
    park_and_dismount(true);
}

#[test]
fn unsupported_or_occupied_parking_leaves_purchase_and_rental_ledgers_unchanged() {
    for rental in [false, true] {
        let (state, catalog, facility_id) = garage_world();
        let action = || {
            if rental {
                TransportCommand::RentVehicle {
                    model: VehicleModel::Compact,
                    facility_id,
                }
            } else {
                TransportCommand::BuyVehicle {
                    model: VehicleModel::Compact,
                    facility_id,
                }
            }
        };
        let mut uncovered = state.clone();
        assert!(
            uncovered
                .deliver_vehicle_with_ground(1, action(), &catalog, |_, _| None)
                .is_err()
        );
        assert_eq!(uncovered, state);

        let mut narrow = state.clone();
        let garage = state.transport.facilities[&facility_id].position;
        assert!(
            narrow
                .deliver_vehicle_with_ground(1, action(), &catalog, |p, _| {
                    // A collection of tiny supported bay centers never supplies a
                    // complete vehicle and door-side footprint.
                    let d = garage.surface_distance_m(p);
                    ((d / 4. - (d / 4.).round()).abs() < 0.001).then_some(p)
                })
                .is_err()
        );
        assert_eq!(narrow, state);

        let mut occupied = state.clone();
        let id = occupied.next_id;
        occupied.next_id += 1;
        occupied.structures.insert(
            id,
            Structure {
                id,
                owner: 1,
                faction_owner: None,
                blueprint: "fence".into(),
                position: garage,
                hitpoints: 100,
                max_hitpoints: 100,
                radius_m: 40.,
                housing: false,
                workbench: false,
            },
        );
        let before = occupied.clone();
        assert!(
            occupied
                .deliver_vehicle_with_ground(1, action(), &catalog, |p, _| Some(p))
                .is_err()
        );
        assert_eq!(occupied, before);
    }
}

#[test]
fn a_long_bus_and_repeated_rentals_get_distinct_usable_garage_bays() {
    let (mut state, catalog, facility_id) = garage_world();
    state.living.accounts.get_mut(&1).unwrap().money = 200_000;
    state
        .transport
        .drivers
        .get_mut(&1)
        .unwrap()
        .licenses
        .insert(DriverLicense::Passenger);
    let garage = state.transport.facilities[&facility_id].position;
    let before_fuel = state.transport.facilities[&facility_id].fuel_l;
    let before_money = state.living.accounts[&1].money + state.transport.treasury;
    for model in [
        VehicleModel::Bus,
        VehicleModel::Compact,
        VehicleModel::Compact,
    ] {
        state.set_player_position(1, garage).unwrap();
        let vehicle_id = state
            .deliver_vehicle_with_ground(
                1,
                TransportCommand::RentVehicle { model, facility_id },
                &catalog,
                |mut p, _| {
                    p.altitude_m = 100.;
                    Some(p)
                },
            )
            .unwrap()
            .created_id
            .unwrap();
        assert_eq!(
            state
                .spawn_obstruction(&state.living.units[&vehicle_id])
                .unwrap(),
            None
        );
        state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::EnterVehicle { vehicle_id },
                },
                &catalog,
            )
            .unwrap();
        state
            .exit_vehicle_with_ground(1, &catalog, |mut p| {
                p.altitude_m = 100.;
                Some(p)
            })
            .unwrap();
        let avatar = &state.living.units[&state.living.accounts[&1].avatar_unit];
        assert_eq!(state.spawn_obstruction(avatar).unwrap(), None);
    }
    let bond: u64 = state
        .transport
        .vehicles
        .values()
        .filter_map(|v| v.rental.as_ref())
        .map(|r| r.bond_escrow)
        .sum();
    assert_eq!(
        state.living.accounts[&1].money + state.transport.treasury + bond,
        before_money
    );
    assert_eq!(
        state.transport.facilities[&facility_id].fuel_l,
        before_fuel - 30.
    );
    assert_eq!(
        state
            .transport
            .vehicles
            .values()
            .filter(|v| v.rental.is_some())
            .map(|v| v.fuel_l)
            .sum::<f64>(),
        30.
    );
    state.validate_with_catalog(&catalog).unwrap();
}
