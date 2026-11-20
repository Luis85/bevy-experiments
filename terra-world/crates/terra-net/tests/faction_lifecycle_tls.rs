#![cfg(feature = "tls")]
//! Real authenticated peers exercise roster changes through the shared command
//! authority, including durable sequences and each principal's private view.
use std::time::{Duration, Instant};
use terra_core::LocalFrame;
use terra_net::{
    ClientMessage, ClientTlsConfig, NetClient, Server, ServerConfig, ServerMessage, ServerTlsConfig,
};
use terra_sandbox::{Command, FactionRole, Outcome, SandboxView};

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/fixture-key.pem");

fn host() -> Server {
    Server::bind_tls(
        ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            calendar_time_scale: 0.,
            ..Default::default()
        },
        ServerTlsConfig::from_pem(CERT, KEY).unwrap(),
    )
    .unwrap()
}

fn connect(host: &Server, token: Option<String>) -> NetClient {
    NetClient::connect_tls(
        host.local_addr().unwrap(),
        host.config().spawn,
        token,
        Some(host.config().manifest.clone()),
        ClientTlsConfig::from_ca_pem(CA, "localhost").unwrap(),
        Duration::from_secs(1),
    )
    .unwrap()
}

fn until(
    host: &mut Server,
    clients: &mut [&mut NetClient; 2],
    ready: impl Fn(&[Vec<ServerMessage>; 2]) -> bool,
) -> [Vec<ServerMessage>; 2] {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = [Vec::new(), Vec::new()];
    loop {
        host.update(0.025);
        for (index, client) in clients.iter_mut().enumerate() {
            received[index].extend(client.poll().unwrap());
        }
        if ready(&received) {
            return received;
        }
        assert!(
            Instant::now() < deadline,
            "TLS lifecycle exchange timed out"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn welcome(messages: &[ServerMessage]) -> Option<(u64, String, u64)> {
    messages.iter().find_map(|message| match message {
        ServerMessage::Welcome {
            actor_id,
            resume_token,
            next_sandbox_sequence,
            ..
        } => Some((*actor_id, resume_token.clone(), *next_sandbox_sequence)),
        _ => None,
    })
}

fn command(
    host: &mut Server,
    clients: &mut [&mut NetClient; 2],
    principal: usize,
    sequence: u64,
    command: Command,
) -> Result<Outcome, String> {
    eprintln!("TLS lifecycle principal {principal}, sequence {sequence}: {command:?}");
    clients[principal]
        .send(ClientMessage::Sandbox { sequence, command })
        .unwrap();
    let received = until(host, clients, |received| {
        received[principal].iter().any(|message| {
            matches!(message,
                ServerMessage::SandboxResult { sequence: actual, .. }
                    | ServerMessage::SandboxRejected { sequence: actual, .. }
                    if *actual == sequence)
        })
    });
    received[principal]
        .iter()
        .find_map(|message| match message {
            ServerMessage::SandboxResult {
                sequence: actual,
                outcome,
            } if *actual == sequence => Some(Ok(outcome.clone())),
            ServerMessage::SandboxRejected {
                sequence: actual,
                message,
            } if *actual == sequence => Some(Err(message.clone())),
            _ => None,
        })
        .unwrap()
}

fn snapshots(
    host: &mut Server,
    clients: &mut [&mut NetClient; 2],
    faction: u64,
    leader: u64,
    structure: u64,
) -> [SandboxView; 2] {
    for client in clients.iter_mut() {
        client.send(ClientMessage::Resync {}).unwrap();
    }
    let received = until(host, clients, |received| {
        received.iter().all(|messages| {
            messages.iter().any(|message| {
                matches!(message, ServerMessage::SandboxSnapshot { view, .. }
                    if view.factions.iter().any(|f| f.id == faction
                        && f.leader == leader && f.members.len() == 1)
                    && view.structures.iter().any(|s| s.id == structure
                        && s.faction_owner == Some(faction)))
            })
        })
    });
    received.map(|messages| {
        messages
            .into_iter()
            .rev()
            .find_map(|message| match message {
                ServerMessage::SandboxSnapshot { view, .. } => Some(view),
                _ => None,
            })
            .unwrap()
    })
}

#[test]
fn tls_handover_departure_and_private_snapshots_survive_host_restart() {
    let mut host = host();
    let spawn = host.config().spawn;
    let mut alice = connect(&host, None);
    let mut bob = connect(&host, None);
    let received = until(&mut host, &mut [&mut alice, &mut bob], |received| {
        received.iter().all(|messages| welcome(messages).is_some())
    });
    let (alice_id, alice_token, _) = welcome(&received[0]).unwrap();
    let (bob_id, bob_token, _) = welcome(&received[1]).unwrap();
    assert_ne!(alice_id, bob_id);
    assert_ne!(alice_token, bob_token);
    assert!(alice.is_encrypted() && bob.is_encrypted());
    let clients = &mut [&mut alice, &mut bob];

    let faction = command(
        &mut host,
        clients,
        0,
        1,
        Command::CreateFaction {
            name: "TLS Commons".into(),
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    assert!(
        command(
            &mut host,
            clients,
            1,
            1,
            Command::AcceptFactionInvite {
                faction_id: faction
            }
        )
        .is_err()
    );
    let plot = command(
        &mut host,
        clients,
        0,
        2,
        Command::ClaimPlot {
            center: spawn,
            radius_m: 10.,
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    command(
        &mut host,
        clients,
        0,
        3,
        Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
    )
    .unwrap();
    command(
        &mut host,
        clients,
        0,
        4,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
    )
    .unwrap();
    command(
        &mut host,
        clients,
        0,
        5,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: bob_id,
        },
    )
    .unwrap();
    command(
        &mut host,
        clients,
        1,
        2,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
    )
    .unwrap();

    let before = host.sandbox().factions.clone();
    assert!(
        command(
            &mut host,
            clients,
            1,
            3,
            Command::TransferFactionLeadership {
                faction_id: faction,
                player_id: bob_id
            }
        )
        .is_err()
    );
    assert_eq!(host.sandbox().factions, before);
    command(
        &mut host,
        clients,
        0,
        6,
        Command::TransferFactionLeadership {
            faction_id: faction,
            player_id: bob_id,
        },
    )
    .unwrap();
    assert_eq!(
        host.sandbox().factions[&faction].members[&alice_id],
        FactionRole::Builder
    );
    let before = host.sandbox().factions.clone();
    assert!(
        command(
            &mut host,
            clients,
            0,
            7,
            Command::RemoveFactionMember {
                faction_id: faction,
                player_id: bob_id
            }
        )
        .is_err()
    );
    assert_eq!(host.sandbox().factions, before);
    command(
        &mut host,
        clients,
        0,
        8,
        Command::LeaveFaction {
            faction_id: faction,
        },
    )
    .unwrap();
    let stale = command(
        &mut host,
        clients,
        0,
        8,
        Command::CreateFaction {
            name: "Stale shadow".into(),
        },
    )
    .unwrap_err();
    assert!(stale.contains("stale"));
    let before_players = host.sandbox().players.clone();
    assert!(
        command(
            &mut host,
            clients,
            0,
            9,
            Command::Build {
                blueprint: "fence".into(),
                position: spawn
            }
        )
        .is_err()
    );
    assert_eq!(host.sandbox().players, before_players);
    assert!(
        host.sandbox().structures.is_empty(),
        "former creator cannot bypass current faction authority"
    );

    command(
        &mut host,
        clients,
        1,
        4,
        Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
    )
    .unwrap();
    let fence_position = LocalFrame::new(spawn).to_geo([3., 0., 0.]).unwrap();
    let fence = command(
        &mut host,
        clients,
        1,
        5,
        Command::Build {
            blueprint: "fence".into(),
            position: fence_position,
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    let views = snapshots(&mut host, clients, faction, bob_id, fence);
    for (index, principal) in [alice_id, bob_id].into_iter().enumerate() {
        let view = &views[index];
        assert_eq!(view.player.id, principal);
        assert_eq!(
            view.player.inventory,
            host.sandbox().players[&principal].inventory
        );
        assert_eq!(
            view.plots.iter().find(|p| p.id == plot).unwrap().owner,
            alice_id
        );
        assert_eq!(
            view.plots
                .iter()
                .find(|p| p.id == plot)
                .unwrap()
                .faction_owner,
            Some(faction)
        );
        assert_eq!(
            view.structures
                .iter()
                .find(|s| s.id == fence)
                .unwrap()
                .owner,
            bob_id
        );
        assert_eq!(
            view.structures
                .iter()
                .find(|s| s.id == fence)
                .unwrap()
                .faction_owner,
            Some(faction)
        );
        let public = serde_json::to_string(view).unwrap();
        assert!(!public.contains(&alice_token) && !public.contains(&bob_token));
        assert!(!public.contains("resume_token"));
    }
    assert_eq!(views[0].player.inventory["planks"], 4);
    assert_eq!(views[1].player.inventory["planks"], 2);
    let factions = host.sandbox().factions.clone();
    let players = host.sandbox().players.clone();
    let plots = host.sandbox().plots.clone();
    let structures = host.sandbox().structures.clone();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("community-world.json");
    host.save_world(&path).unwrap();
    drop(alice);
    drop(bob);
    drop(host);

    let mut resumed = self::host();
    resumed.load_world(&path).unwrap();
    let mut alice = connect(&resumed, Some(alice_token.clone()));
    let mut bob = connect(&resumed, Some(bob_token.clone()));
    let received = until(&mut resumed, &mut [&mut alice, &mut bob], |received| {
        received.iter().all(|messages| welcome(messages).is_some())
    });
    assert_eq!(welcome(&received[0]), Some((alice_id, alice_token, 10)));
    assert_eq!(welcome(&received[1]), Some((bob_id, bob_token, 6)));
    assert!(alice.is_encrypted() && bob.is_encrypted());
    assert_eq!(resumed.sandbox().players, players);
    assert_eq!(resumed.sandbox().factions, factions);
    assert_eq!(resumed.sandbox().plots, plots);
    assert_eq!(resumed.sandbox().structures, structures);
    let clients = &mut [&mut alice, &mut bob];
    let stale = command(
        &mut resumed,
        clients,
        0,
        9,
        Command::CreateFaction {
            name: "Restart shadow".into(),
        },
    )
    .unwrap_err();
    assert!(stale.contains("stale"));
    assert!(
        command(
            &mut resumed,
            clients,
            0,
            10,
            Command::InviteToFaction {
                faction_id: faction,
                player_id: alice_id
            }
        )
        .is_err()
    );
    assert_eq!(resumed.sandbox().factions, factions);
    command(
        &mut resumed,
        clients,
        1,
        6,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: alice_id,
        },
    )
    .unwrap();
    command(
        &mut resumed,
        clients,
        0,
        11,
        Command::DeclineFactionInvite {
            faction_id: faction,
        },
    )
    .unwrap();
    assert_eq!(
        resumed.sandbox().factions,
        factions,
        "a fresh invitation does not silently restore membership"
    );
    assert_eq!(resumed.sandbox().players, players);
    resumed.sandbox().validate().unwrap();
}

#[test]
fn tls_unused_faction_dissolution_revokes_invites_and_keeps_replay_identity_on_resume() {
    let mut host = host();
    let mut alice = connect(&host, None);
    let mut bob = connect(&host, None);
    let received = until(&mut host, &mut [&mut alice, &mut bob], |received| {
        received.iter().all(|messages| welcome(messages).is_some())
    });
    let (alice_id, alice_token, _) = welcome(&received[0]).unwrap();
    let (bob_id, bob_token, _) = welcome(&received[1]).unwrap();
    assert!(alice.is_encrypted() && bob.is_encrypted());
    let clients = &mut [&mut alice, &mut bob];
    let faction = command(
        &mut host,
        clients,
        0,
        1,
        Command::CreateFaction {
            name: "Unused TLS idea".into(),
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    command(
        &mut host,
        clients,
        0,
        2,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: bob_id,
        },
    )
    .unwrap();
    let before = host.sandbox().clone();
    assert!(
        command(
            &mut host,
            clients,
            1,
            1,
            Command::DissolveFaction {
                faction_id: faction
            },
        )
        .is_err()
    );
    assert_eq!(
        host.sandbox(),
        &before,
        "an invited peer has no deletion authority"
    );
    command(
        &mut host,
        clients,
        0,
        3,
        Command::DissolveFaction {
            faction_id: faction,
        },
    )
    .unwrap();
    let mut expected = before;
    expected.factions.remove(&faction);
    assert_eq!(
        host.sandbox(),
        &expected,
        "dissolution removes only its roster"
    );
    assert!(
        command(
            &mut host,
            clients,
            1,
            2,
            Command::AcceptFactionInvite {
                faction_id: faction
            },
        )
        .is_err(),
        "the deleted invitation cannot be accepted from a stale client view"
    );
    for client in clients.iter_mut() {
        client.send(ClientMessage::Resync {}).unwrap();
    }
    let received = until(&mut host, clients, |received| {
        received.iter().all(|messages| {
            messages.iter().any(|message| {
                matches!(message, ServerMessage::SandboxSnapshot { view, .. }
                    if view.factions.iter().all(|f| f.id != faction))
            })
        })
    });
    for (index, principal) in [alice_id, bob_id].into_iter().enumerate() {
        let view = received[index]
            .iter()
            .rev()
            .find_map(|message| match message {
                ServerMessage::SandboxSnapshot { view, .. } => Some(view),
                _ => None,
            })
            .unwrap();
        assert_eq!(view.player.id, principal);
        assert!(view.factions.iter().all(|f| f.id != faction));
    }
    let replacement = command(
        &mut host,
        clients,
        0,
        4,
        Command::CreateFaction {
            name: "Reviewed replacement".into(),
        },
    )
    .unwrap()
    .created_id
    .unwrap();
    assert!(replacement > faction);
    let unchanged = host.sandbox().clone();
    let stale = command(
        &mut host,
        clients,
        0,
        3,
        Command::DissolveFaction {
            faction_id: replacement,
        },
    )
    .unwrap_err();
    assert!(stale.contains("stale"));
    assert!(
        command(
            &mut host,
            clients,
            0,
            5,
            Command::DissolveFaction {
                faction_id: faction
            },
        )
        .is_err()
    );
    assert_eq!(host.sandbox(), &unchanged);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("dissolved-community.json");
    host.save_world(&path).unwrap();
    drop(alice);
    drop(bob);
    drop(host);

    let mut resumed = self::host();
    resumed.load_world(&path).unwrap();
    let mut alice = connect(&resumed, Some(alice_token.clone()));
    let mut bob = connect(&resumed, Some(bob_token.clone()));
    let received = until(&mut resumed, &mut [&mut alice, &mut bob], |received| {
        received.iter().all(|messages| welcome(messages).is_some())
    });
    assert_eq!(welcome(&received[0]), Some((alice_id, alice_token, 6)));
    assert_eq!(welcome(&received[1]), Some((bob_id, bob_token, 3)));
    assert_eq!(resumed.sandbox(), &unchanged);
    assert!(!resumed.sandbox().factions.contains_key(&faction));
    let clients = &mut [&mut alice, &mut bob];
    assert!(
        command(
            &mut resumed,
            clients,
            0,
            3,
            Command::DissolveFaction {
                faction_id: replacement,
            },
        )
        .unwrap_err()
        .contains("stale")
    );
    assert_eq!(resumed.sandbox(), &unchanged);
    command(
        &mut resumed,
        clients,
        0,
        6,
        Command::InviteToFaction {
            faction_id: replacement,
            player_id: bob_id,
        },
    )
    .unwrap();
    command(
        &mut resumed,
        clients,
        1,
        3,
        Command::AcceptFactionInvite {
            faction_id: replacement,
        },
    )
    .unwrap();
    let with_member = resumed.sandbox().clone();
    assert!(
        command(
            &mut resumed,
            clients,
            0,
            7,
            Command::DissolveFaction {
                faction_id: replacement,
            },
        )
        .is_err()
    );
    assert_eq!(resumed.sandbox(), &with_member);
    command(
        &mut resumed,
        clients,
        1,
        4,
        Command::LeaveFaction {
            faction_id: replacement,
        },
    )
    .unwrap();
    command(
        &mut resumed,
        clients,
        0,
        8,
        Command::DissolveFaction {
            faction_id: replacement,
        },
    )
    .unwrap();
    assert!(resumed.sandbox().factions.is_empty());
    resumed.sandbox().validate().unwrap();
}
