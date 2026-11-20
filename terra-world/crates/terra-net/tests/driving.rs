//! Actual loopback TCP regressions for protocol-5 driving, transport privacy,
//! and durable vehicle/job ownership. No client submits a world coordinate.
use std::time::{Duration, Instant};
use terra_core::DriveInput;
use terra_net::{ClientMessage, NetClient, Server, ServerConfig, ServerMessage};
use terra_sandbox::{
    CharacterProfile, Command, LivingCommand, SandboxView, TransportCommand, TransportFacilityKind,
    TransportJobKind, TransportWorld,
};

fn server() -> Server {
    let mut host = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        calendar_time_scale: 0.,
        ..Default::default()
    })
    .unwrap();
    host.enable_living_demo(&[]).unwrap();
    host
}

fn pump(
    host: &mut Server,
    clients: &mut [&mut NetClient],
    count: usize,
) -> Vec<Vec<ServerMessage>> {
    let mut messages: Vec<_> = clients.iter().map(|_| Vec::new()).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    for step in 0.. {
        let report = host.update(0.025);
        assert_eq!(
            report.domain_errors, 0,
            "authority domain update failed: {report:?}"
        );
        assert!(
            report.persistence_error.is_none(),
            "checkpoint failed: {report:?}"
        );
        for (index, client) in clients.iter_mut().enumerate() {
            messages[index].extend(client.poll().unwrap());
        }
        if step + 1 >= count && !host.persistence_pending() {
            return messages;
        }
        assert!(
            Instant::now() < deadline,
            "asynchronous checkpoint did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    unreachable!()
}

fn identity(messages: &[ServerMessage]) -> (u64, String) {
    messages
        .iter()
        .find_map(|message| match message {
            ServerMessage::Welcome {
                actor_id,
                resume_token,
                ..
            } => Some((*actor_id, resume_token.clone())),
            _ => None,
        })
        .expect("successful TCP handshake")
}

fn last_view(messages: &[ServerMessage]) -> &SandboxView {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::SandboxSnapshot { view, .. } => Some(view),
            _ => None,
        })
        .next_back()
        .expect("replicated sandbox snapshot")
}

fn has_error(messages: &[ServerMessage], expected: &str) -> bool {
    messages
        .iter()
        .any(|message| matches!(message, ServerMessage::Error { code, .. } if code == expected))
}

fn send_command(client: &mut NetClient, sequence: u64, command: Command) {
    client
        .send(ClientMessage::Sandbox { sequence, command })
        .unwrap();
}

fn transport(client: &mut NetClient, sequence: u64, action: TransportCommand) {
    send_command(client, sequence, Command::Transport { action });
}

fn own_vehicle(host: &Server, player: u64) -> u64 {
    host.sandbox()
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == player)
        .unwrap()
        .unit_id
}

fn controlled() -> (Server, NetClient, u64, u64, String) {
    let mut host = server();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let messages = pump(&mut host, &mut [&mut client], 8);
    let (player, token) = identity(&messages[0]);
    let vehicle = own_vehicle(&host, player);
    transport(
        &mut client,
        1,
        TransportCommand::EnterVehicle {
            vehicle_id: vehicle,
        },
    );
    let messages = pump(&mut host, &mut [&mut client], 6);
    assert!(
        messages[0]
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxResult { sequence: 1, .. }))
    );
    assert!(host.sandbox().living.accounts[&player].possessed);
    assert_eq!(host.sandbox().living.accounts[&player].active_unit, vehicle);
    (host, client, player, vehicle, token)
}

#[test]
fn tcp_pedal_intent_accelerates_steers_coasts_and_consumes_authoritative_fuel() {
    let (mut host, mut client, player, vehicle, _) = controlled();
    let start = host.sandbox().living.units[&vehicle].position;
    let fuel = host.sandbox().transport.vehicles[&vehicle].fuel_l;
    client
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 20);
    let speed = host.sandbox().transport.vehicles[&vehicle]
        .dynamics
        .speed_m_s;
    assert!(
        speed > 0.5 && speed < 12.,
        "pedals should build velocity over time: {speed}"
    );
    let first = host.sandbox().living.units[&vehicle].position;
    assert!(start.surface_distance_m(first) > 0.10);
    let heading = host.sandbox().living.units[&vehicle].heading_deg;
    client
        .send(ClientMessage::Drive {
            sequence: 2,
            input: DriveInput {
                throttle: 0.7,
                steering: 0.75,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 16);
    let turn = ((host.sandbox().living.units[&vehicle].heading_deg - heading + 540.)
        .rem_euclid(360.)
        - 180.)
        .abs();
    assert!(
        turn > 1.,
        "steering must change authoritative heading: {turn}"
    );
    let before_coast = host.sandbox().living.units[&vehicle].position;
    client
        .send(ClientMessage::Drive {
            sequence: 3,
            input: DriveInput::default(),
        })
        .unwrap();
    let messages = pump(&mut host, &mut [&mut client], 8);
    let state = &host.sandbox().transport.vehicles[&vehicle];
    assert!(
        state.dynamics.speed_m_s > 0.1,
        "releasing the pedal should retain inertia"
    );
    assert!(before_coast.surface_distance_m(host.sandbox().living.units[&vehicle].position) > 0.05);
    assert!(state.fuel_l < fuel);
    assert!(
        state.odometer_m
            >= start.surface_distance_m(host.sandbox().living.units[&vehicle].position) - 0.02
    );
    assert_eq!(
        host.actors()[&player].position,
        host.sandbox().living.units[&vehicle].position
    );
    let view = last_view(&messages[0]);
    view.transport
        .validate_replica(player, &view.living)
        .unwrap();
    assert!(
        view.transport
            .vehicles
            .iter()
            .find(|v| v.unit_id == vehicle)
            .unwrap()
            .odometer_m
            > 0.
    );
}

#[test]
fn stale_input_timeout_brakes_the_vehicle_and_zero_move_is_an_explicit_stop() {
    let (mut host, mut client, _, vehicle, _) = controlled();
    for sequence in 1..=4 {
        client
            .send(ClientMessage::Drive {
                sequence,
                input: DriveInput {
                    throttle: 1.,
                    ..Default::default()
                },
            })
            .unwrap();
        pump(&mut host, &mut [&mut client], 10);
    }
    assert!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s
            > 1.
    );
    pump(&mut host, &mut [&mut client], 120);
    assert!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s
            .abs()
            < 0.01
    );
    let stopped = host.sandbox().living.units[&vehicle].position;
    pump(&mut host, &mut [&mut client], 16);
    assert!(stopped.surface_distance_m(host.sandbox().living.units[&vehicle].position) < 0.02);
    client
        .send(ClientMessage::Drive {
            sequence: 5,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 16);
    assert!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s
            > 0.5
    );
    client
        .send(ClientMessage::Move {
            sequence: 6,
            velocity_enu_m_s: [0.; 3],
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 40);
    assert!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s
            .abs()
            < 0.01
    );
}

#[test]
fn nonzero_move_cannot_bypass_pedals_and_drive_shares_replay_protection() {
    let (mut host, mut client, _, vehicle, _) = controlled();
    let initial = host.sandbox().living.units[&vehicle].position;
    client
        .send(ClientMessage::Move {
            sequence: 1,
            velocity_enu_m_s: [30., 0., 0.],
        })
        .unwrap();
    let messages = pump(&mut host, &mut [&mut client], 6);
    assert!(has_error(&messages[0], "drive_required"));
    assert!(initial.surface_distance_m(host.sandbox().living.units[&vehicle].position) < 0.001);
    client
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 12);
    let forward = host.sandbox().transport.vehicles[&vehicle]
        .dynamics
        .speed_m_s;
    assert!(forward > 0.1);
    client
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: -1.,
                steering: -1.,
                ..Default::default()
            },
        })
        .unwrap();
    let messages = pump(&mut host, &mut [&mut client], 4);
    assert!(has_error(&messages[0], "sequence"));
    assert!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s
            >= forward
    );
}

#[test]
fn drive_cannot_take_over_an_unpossessed_or_other_players_vehicle() {
    let mut host = server();
    let mut alice = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let mut bob = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 8);
    let (a, _) = identity(&messages[0]);
    let (b, _) = identity(&messages[1]);
    let vehicle = own_vehicle(&host, a);
    let other = own_vehicle(&host, b);
    let other_position = host.sandbox().living.units[&other].position;
    alice
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    transport(
        &mut bob,
        1,
        TransportCommand::EnterVehicle {
            vehicle_id: vehicle,
        },
    );
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 6);
    assert!(has_error(&messages[0], "possession"));
    assert!(
        messages[1]
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxRejected { sequence: 1, .. }))
    );
    transport(
        &mut alice,
        1,
        TransportCommand::EnterVehicle {
            vehicle_id: vehicle,
        },
    );
    pump(&mut host, &mut [&mut alice, &mut bob], 4);
    alice
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut alice, &mut bob], 12);
    assert!(host.sandbox().transport.vehicles[&vehicle].odometer_m > 0.);
    send_command(
        &mut alice,
        2,
        Command::Living {
            action: LivingCommand::Release,
        },
    );
    pump(&mut host, &mut [&mut alice, &mut bob], 4);
    let position = host.sandbox().living.units[&vehicle].position;
    alice
        .send(ClientMessage::Drive {
            sequence: 2,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 6);
    assert!(has_error(&messages[0], "possession"));
    assert_eq!(host.sandbox().living.units[&vehicle].position, position);
    assert_eq!(host.sandbox().living.units[&other].position, other_position);
    assert_eq!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s,
        0.
    );
    // Quick re-entry happens before the previous pedal command times out.
    // Taking control must clear that old intent as well as physical momentum.
    send_command(
        &mut alice,
        3,
        Command::Living {
            action: LivingCommand::Possess { unit_id: vehicle },
        },
    );
    pump(&mut host, &mut [&mut alice, &mut bob], 6);
    assert_eq!(
        host.sandbox().transport.vehicles[&vehicle]
            .dynamics
            .speed_m_s,
        0.
    );
    assert_eq!(host.sandbox().living.units[&vehicle].position, position);
}

#[test]
fn public_vehicle_and_hitch_deltas_preserve_other_driver_financial_privacy() {
    let mut host = server();
    let mut alice = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let mut bob = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 8);
    let (a, _) = identity(&messages[0]);
    let (b, _) = identity(&messages[1]);
    let vehicle = own_vehicle(&host, b);
    let garage = host
        .sandbox()
        .transport
        .facilities
        .values()
        .find(|f| f.kind == TransportFacilityKind::Garage)
        .unwrap()
        .id;
    transport(
        &mut bob,
        1,
        TransportCommand::SetCharacterProfile {
            profile: CharacterProfile {
                name: "River driver".into(),
                body_style: 2,
                skin: 6,
                hair: 4,
                outfit: 7,
            },
        },
    );
    transport(
        &mut bob,
        2,
        TransportCommand::Paint {
            vehicle_id: vehicle,
            facility_id: garage,
            paint: 5,
        },
    );
    transport(
        &mut bob,
        3,
        TransportCommand::BuyTrailer {
            facility_id: garage,
        },
    );
    pump(&mut host, &mut [&mut alice, &mut bob], 8);
    let trailer = host
        .sandbox()
        .transport
        .trailers
        .values()
        .find(|t| t.owner == b)
        .unwrap()
        .id;
    transport(
        &mut bob,
        4,
        TransportCommand::HitchTrailer {
            vehicle_id: vehicle,
            trailer_id: trailer,
        },
    );
    let job = host
        .sandbox()
        .transport
        .jobs
        .values()
        .find(|j| j.kind == TransportJobKind::Hitchhiker)
        .unwrap()
        .id;
    transport(
        &mut bob,
        5,
        TransportCommand::AcceptJob {
            job_id: job,
            vehicle_id: vehicle,
        },
    );
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 12);
    let alice_view = last_view(&messages[0]);
    alice_view
        .transport
        .validate_replica(a, &alice_view.living)
        .unwrap();
    assert!(alice_view.transport.vehicles.iter().all(|v| v.owner == a));
    assert!(alice_view.transport.jobs.iter().all(|j| j.id != job));
    assert_eq!(alice_view.transport.driver.as_ref().unwrap().player_id, a);
    assert_eq!(
        alice_view.transport.visible_profiles[&b].name,
        "River driver"
    );
    let appearance = alice_view
        .transport
        .visible_vehicles
        .iter()
        .find(|v| v.unit_id == vehicle)
        .unwrap();
    assert_eq!(appearance.paint, 5);
    assert_eq!(appearance.trailer, Some(trailer));
    assert_eq!(
        alice_view
            .transport
            .trailers
            .iter()
            .find(|t| t.id == trailer)
            .unwrap()
            .attached_to,
        Some(vehicle)
    );
    let replica = TransportWorld::from_view(alice_view.transport.clone(), a);
    assert_eq!(replica.treasury, 0);
    assert!(!replica.vehicles.contains_key(&vehicle));
    assert!(!replica.drivers.contains_key(&b));
    assert_eq!(replica.appearance(vehicle).unwrap().paint, 5);
    let bob_view = last_view(&messages[1]);
    assert!(
        bob_view
            .transport
            .jobs
            .iter()
            .any(|j| j.id == job && j.assignment.is_some())
    );
    assert!(
        bob_view
            .transport
            .vehicles
            .iter()
            .any(|v| v.unit_id == vehicle && v.fuel_l > 0.)
    );
    assert!(alice.replication_stats().deltas > 0);
    // Geography removes both the visible towing body and its dependent hitch.
    alice
        .send(ClientMessage::Focus {
            position: host.config().spawn,
            radius_m: 10.,
        })
        .unwrap();
    transport(
        &mut bob,
        6,
        TransportCommand::EnterVehicle {
            vehicle_id: vehicle,
        },
    );
    pump(&mut host, &mut [&mut alice, &mut bob], 6);
    let mut latest = Vec::new();
    for sequence in 1..=12 {
        bob.send(ClientMessage::Drive {
            sequence,
            input: DriveInput {
                throttle: 1.,
                ..Default::default()
            },
        })
        .unwrap();
        latest = pump(&mut host, &mut [&mut alice, &mut bob], 10);
    }
    let departed = last_view(&latest[0]);
    departed
        .transport
        .validate_replica(a, &departed.living)
        .unwrap();
    assert!(
        host.sandbox().living.units[&vehicle]
            .position
            .surface_distance_m(host.sandbox().players[&a].position)
            > 10.
    );
    assert!(
        !departed
            .transport
            .visible_vehicles
            .iter()
            .any(|v| v.unit_id == vehicle)
    );
    assert!(!departed.transport.trailers.iter().any(|t| t.id == trailer));
}

#[test]
fn regional_checkpoint_restart_and_reconnect_keep_driver_vehicle_and_funded_job() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("driving-world.sqlite");
    let mut host = server();
    host.open_region_store(&database).unwrap();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let messages = pump(&mut host, &mut [&mut client], 12);
    let (player, token) = identity(&messages[0]);
    let vehicle = own_vehicle(&host, player);
    transport(
        &mut client,
        1,
        TransportCommand::EnterVehicle {
            vehicle_id: vehicle,
        },
    );
    pump(&mut host, &mut [&mut client], 8);
    client
        .send(ClientMessage::Drive {
            sequence: 1,
            input: DriveInput {
                throttle: 0.8,
                ..Default::default()
            },
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 16);
    client
        .send(ClientMessage::Move {
            sequence: 2,
            velocity_enu_m_s: [0.; 3],
        })
        .unwrap();
    pump(&mut host, &mut [&mut client], 30);
    let job = host
        .sandbox()
        .transport
        .jobs
        .values()
        .find(|j| j.item.as_deref() == Some("wood"))
        .unwrap()
        .id;
    transport(
        &mut client,
        2,
        TransportCommand::AcceptJob {
            job_id: job,
            vehicle_id: vehicle,
        },
    );
    transport(
        &mut client,
        3,
        TransportCommand::SetCharacterProfile {
            profile: CharacterProfile {
                name: "Persistent driver".into(),
                outfit: 6,
                ..Default::default()
            },
        },
    );
    pump(&mut host, &mut [&mut client], 12);
    let checkpoint = host.checkpoint_regions().unwrap();
    assert!(checkpoint.generation > 0);
    let transport_state = host.sandbox().transport.clone();
    let wallet = host.sandbox().living.accounts[&player].money;
    assert!(transport_state.vehicles[&vehicle].odometer_m > 0.);
    assert!(transport_state.jobs[&job].assignment.is_some());
    drop(client);
    drop(host);
    let mut restored = server();
    assert_eq!(
        restored.open_region_store(&database).unwrap().generation,
        checkpoint.generation
    );
    assert_eq!(restored.sandbox().transport, transport_state);
    let mut client = NetClient::connect_with_token(
        restored.local_addr().unwrap(),
        restored.config().spawn,
        Some(token),
    )
    .unwrap();
    let messages = pump(&mut restored, &mut [&mut client], 12);
    assert_eq!(identity(&messages[0]).0, player);
    assert!(messages[0].iter().any(|message| matches!(
        message,
        ServerMessage::Welcome {
            next_sandbox_sequence: 4,
            ..
        }
    )));
    assert_eq!(restored.sandbox().living.accounts[&player].money, wallet);
    assert_eq!(
        restored.sandbox().transport.profiles[&player].name,
        "Persistent driver"
    );
    assert_eq!(
        restored.sandbox().transport.jobs[&job].assignment,
        transport_state.jobs[&job].assignment
    );
    assert_eq!(
        restored.sandbox().transport.vehicles[&vehicle].odometer_m,
        transport_state.vehicles[&vehicle].odometer_m
    );
    let original_escrow = restored.sandbox().transport.treasury;
    transport(
        &mut client,
        2,
        TransportCommand::AcceptJob {
            job_id: job,
            vehicle_id: vehicle,
        },
    );
    let rejected = pump(&mut restored, &mut [&mut client], 6);
    assert!(
        rejected[0]
            .iter()
            .any(|message| matches!(message, ServerMessage::SandboxRejected { sequence: 2, .. }))
    );
    assert_eq!(restored.sandbox().transport.treasury, original_escrow);
}

#[test]
fn tcp_company_ledgers_replicate_privately_and_replayed_transfers_do_not_pay_twice() {
    use terra_sandbox::CompanyCommand;
    let mut host = server();
    let mut owner = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let mut observer = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let joined = pump(&mut host, &mut [&mut owner, &mut observer], 8);
    let (player, _) = identity(&joined[0]);
    let (other, _) = identity(&joined[1]);
    let initial_wallet = host.sandbox().living.accounts[&player].money;
    let company = |action| TransportCommand::Company { action };
    transport(
        &mut owner,
        1,
        company(CompanyCommand::Create {
            name: "Private Owner Logistics".into(),
        }),
    );
    let created = pump(&mut host, &mut [&mut owner, &mut observer], 8);
    assert!(
        created[0]
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxResult { sequence: 1, .. })),
        "{:#?}",
        created[0]
    );
    transport(
        &mut owner,
        2,
        company(CompanyCommand::Deposit { amount: 5_000 }),
    );
    transport(
        &mut owner,
        3,
        company(CompanyCommand::HireDriver {
            name: "Private Driver".into(),
        }),
    );
    let updated = pump(&mut host, &mut [&mut owner, &mut observer], 8);
    let own = last_view(&updated[0]);
    let theirs = last_view(&updated[1]);
    let ledger = own.transport.company.as_ref().unwrap();
    assert_eq!(ledger.owner, player);
    assert_eq!(ledger.name, "Private Owner Logistics");
    assert_eq!(ledger.cash, 4_000);
    assert_eq!(ledger.drivers[0].name, "Private Driver");
    assert_eq!(
        own.living.account.as_ref().unwrap().money,
        initial_wallet - 7_000
    );
    assert!(theirs.transport.company.is_none());
    assert!(
        !serde_json::to_string(theirs)
            .unwrap()
            .contains("Private Owner Logistics")
    );
    assert!(
        !serde_json::to_string(theirs)
            .unwrap()
            .contains("Private Driver")
    );
    let before = host.sandbox().transport.companies[&player].clone();
    let wallet = host.sandbox().living.accounts[&player].money;
    transport(
        &mut owner,
        2,
        company(CompanyCommand::Deposit { amount: 5_000 }),
    );
    transport(
        &mut observer,
        1,
        company(CompanyCommand::Withdraw { amount: 1_000 }),
    );
    let errors = pump(&mut host, &mut [&mut owner, &mut observer], 8);
    assert!(
        errors[0]
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxRejected { sequence: 2, .. }))
    );
    assert!(
        errors[1]
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxRejected { sequence: 1, .. }))
    );
    assert_eq!(host.sandbox().transport.companies[&player], before);
    assert_eq!(host.sandbox().living.accounts[&player].money, wallet);
    assert!(!host.sandbox().transport.companies.contains_key(&other));
    // Real network delta reconstruction has retained the private typed header.
    assert_eq!(
        last_view(&errors[0]).transport.company.as_ref().unwrap(),
        &before
    );
}

#[test]
fn protocol_four_client_is_rejected_before_principal_or_company_allocation() {
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };
    let mut host = server();
    let mut socket = TcpStream::connect(host.local_addr().unwrap()).unwrap();
    socket.set_nonblocking(true).unwrap();
    socket
        .write_all(
            &terra_net::encode_frame(&ClientMessage::Hello {
                protocol: 4,
                interest: host.config().spawn,
                resume_token: None,
                expected_manifest: None,
            })
            .unwrap(),
        )
        .unwrap();
    let mut decoder = terra_net::FrameDecoder::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut rejected = false;
    while !rejected {
        host.update(0.025);
        let mut bytes = [0; 4096];
        match socket.read(&mut bytes) {
            Ok(0) => break,
            Ok(n) => decoder.push(&bytes[..n]).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("{error}"),
        }
        while let Some(message) = decoder.decode::<ServerMessage>().unwrap() {
            rejected |=
                matches!(message, ServerMessage::Error { ref code, .. } if code == "version");
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(rejected);
    assert!(host.sandbox().players.is_empty());
    assert!(host.sandbox().transport.companies.is_empty());
}
