//! Actual socket tests: normalized views converge while transport sends deltas.
use std::{thread, time::Duration};
use terra_core::{ActorKind, ActorState, GeoPosition, LocalFrame};
use terra_net::{ClientMessage, NetClient, ReplicationStats, Server, ServerConfig, ServerMessage};

fn server() -> Server {
    Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..ServerConfig::default()
    })
    .unwrap()
}
fn pump(
    host: &mut Server,
    clients: &mut [&mut NetClient],
    ticks: usize,
) -> Vec<Vec<ServerMessage>> {
    let mut out: Vec<Vec<_>> = clients.iter().map(|_| vec![]).collect();
    for _ in 0..ticks {
        let report = host.update(0.05);
        assert_eq!(report.domain_errors, 0);
        for (i, c) in clients.iter_mut().enumerate() {
            out[i].extend(c.poll().unwrap());
        }
        thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..8 {
        host.update(0.);
        for (i, c) in clients.iter_mut().enumerate() {
            out[i].extend(c.poll().unwrap());
        }
        thread::sleep(Duration::from_millis(1));
    }
    out
}
fn last_view(messages: &[ServerMessage]) -> &terra_sandbox::SandboxView {
    messages
        .iter()
        .rev()
        .find_map(|m| {
            if let ServerMessage::SandboxSnapshot { view, .. } = m {
                Some(view)
            } else {
                None
            }
        })
        .unwrap()
}

#[test]
fn tcp_delta_views_converge_exactly_and_keep_each_players_wallet_private() {
    let mut host = server();
    host.enable_living_demo(&[]).unwrap();
    let spawn = host.config().spawn;
    let mut alice = NetClient::connect(host.local_addr().unwrap(), spawn).unwrap();
    let mut bob = NetClient::connect(host.local_addr().unwrap(), spawn).unwrap();
    let messages = pump(&mut host, &mut [&mut alice, &mut bob], 40);
    for (i, client) in [&alice, &bob].into_iter().enumerate() {
        let v = last_view(&messages[i]);
        let expected = host.sandbox().snapshot_for(v.player.id, 5000.).unwrap();
        assert_eq!(
            serde_json::to_value(v).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(v.living.account.as_ref().unwrap().player_id, v.player.id);
        assert!(client.replication_stats().deltas > 2);
        assert!(client.transport_stats().bytes_received > client.replication_stats().wire_bytes);
        assert!(messages[i].iter().all(|m| !matches!(
            m,
            ServerMessage::ActorUpdate { .. } | ServerMessage::SandboxUpdate { .. }
        )));
    }
    assert_ne!(
        last_view(&messages[0]).player.id,
        last_view(&messages[1]).player.id
    );
}

#[test]
fn tcp_interest_removal_reaches_client_without_stale_ghost_actors() {
    let mut host = server();
    let spawn = host.config().spawn;
    let frame = LocalFrame::new(spawn);
    for id in 100..140 {
        host.insert_actor(ActorState {
            id,
            kind: ActorKind::Vehicle,
            position: frame.to_geo([(id - 100) as f64, 0., 0.]).unwrap(),
            velocity_enu_m_s: [0.; 3],
            heading_deg: 0.,
        })
        .unwrap();
    }
    let mut client = NetClient::connect(host.local_addr().unwrap(), spawn).unwrap();
    pump(&mut host, &mut [&mut client], 8);
    host.remove_actor(111).unwrap();
    let received = pump(&mut host, &mut [&mut client], 8);
    let latest = received[0]
        .iter()
        .rev()
        .find_map(|m| {
            if let ServerMessage::Snapshot { actors, .. } = m {
                Some(actors)
            } else {
                None
            }
        })
        .unwrap();
    assert!(!latest.iter().any(|a| a.id == 111));
    assert!(latest.iter().any(|a| a.id == 112));
    assert!(client.replication_stats().entity_removals >= 1);
    client
        .send(ClientMessage::Focus {
            position: GeoPosition::new(0., 0., 0.).unwrap(),
            radius_m: 100.,
        })
        .unwrap();
    let received = pump(&mut host, &mut [&mut client], 8);
    let latest = received[0]
        .iter()
        .rev()
        .find_map(|m| {
            if let ServerMessage::Snapshot { actors, .. } = m {
                Some(actors)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].id, 1);
}

#[test]
fn tcp_explicit_resync_installs_both_keyframes_and_rate_limits_repeated_requests() {
    let mut host = server();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    pump(&mut host, &mut [&mut client], 24);
    let before = client.replication_stats().keyframes;
    client.send(ClientMessage::Resync {}).unwrap();
    assert!(client.resync_pending());
    pump(&mut host, &mut [&mut client], 4);
    assert!(!client.resync_pending());
    assert!(client.replication_stats().keyframes >= before + 2);
    client.send(ClientMessage::Resync {}).unwrap();
    let messages = pump(&mut host, &mut [&mut client], 2);
    assert!(
        messages[0]
            .iter()
            .any(|m| matches!(m,ServerMessage::Error{code,..} if code=="resync_rate"))
    );
    // The denied request produces no matching keyframe pair. The client keeps
    // a bounded retry obligation instead of permanently suppressing Resync.
    let before_retry = client.replication_stats().resync_requests;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while client.resync_pending() {
        assert!(
            std::time::Instant::now() < deadline,
            "resync rejection never recovered"
        );
        host.update(0.02);
        client.poll().unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(client.replication_stats().resync_requests, before_retry + 1);
    assert!(client.replication_stats().keyframes >= before + 4);
}

#[test]
fn server_reports_encoded_delta_bytes_against_full_snapshot_equivalents() {
    let mut host = server();
    host.enable_living_demo(&[]).unwrap();
    let mut client = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let mut sums = ReplicationStats::default();
    for _ in 0..40 {
        let r = host.update(0.05);
        sums.wire_bytes += r.replication.wire_bytes;
        sums.full_snapshot_bytes += r.replication.full_snapshot_bytes;
        sums.deltas += r.replication.deltas;
        client.poll().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert!(sums.deltas > 0);
    assert!(
        sums.wire_bytes < sums.full_snapshot_bytes,
        "{} vs {}",
        sums.wire_bytes,
        sums.full_snapshot_bytes
    );
}

#[test]
fn client_detects_an_actual_wire_gap_requests_one_resync_and_recovers() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use terra_net::replication::{ActorUpdate, CollectionDelta, SandboxUpdate};
    use terra_net::{BaseWorldManifest, FrameDecoder, PROTOCOL_VERSION, encode_frame};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
    let mut client = NetClient::connect(listener.local_addr().unwrap(), origin).unwrap();
    let (mut socket, _) = listener.accept().unwrap();
    socket.set_nodelay(true).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut decoder = FrameDecoder::default();
    let mut buffer = [0u8; 8192];
    loop {
        let n = socket.read(&mut buffer).unwrap();
        assert!(n > 0);
        decoder.push(&buffer[..n]).unwrap();
        if let Some(ClientMessage::Hello { protocol, .. }) = decoder.decode().unwrap() {
            assert_eq!(protocol, PROTOCOL_VERSION);
            break;
        }
    }
    socket
        .write_all(
            &encode_frame(&ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                client_id: 1,
                actor_id: 1,
                manifest: BaseWorldManifest::default(),
                tick: 0,
                unix_seconds: 100.,
                time_scale: 1.,
                paused: false,
                tick_hz: 20,
                spawn: origin,
                resume_token: "a".repeat(64),
                next_sandbox_sequence: 1,
            })
            .unwrap(),
        )
        .unwrap();
    let body = ActorState {
        id: 1,
        kind: ActorKind::Human,
        position: origin,
        velocity_enu_m_s: [0.; 3],
        heading_deg: 0.,
    };
    let full_actor = |revision, tick| ServerMessage::ActorUpdate {
        update: ActorUpdate {
            revision,
            base_revision: None,
            tick,
            unix_seconds: 100. + tick as f64,
            actors: CollectionDelta {
                upserts: vec![body.clone()],
                removed: vec![],
                order: Some(vec![1]),
            },
            truncated: false,
            total_in_interest: 1,
        },
    };
    let mut state = terra_sandbox::SandboxState::new();
    state.add_player(1, origin).unwrap();
    let full_sandbox = |revision, tick| ServerMessage::SandboxUpdate {
        update: SandboxUpdate {
            revision,
            base_revision: None,
            tick,
            full: Some(state.snapshot_for(1, 5000.).unwrap()),
            delta: None,
        },
    };
    socket
        .write_all(&encode_frame(&full_actor(1, 1)).unwrap())
        .unwrap();
    socket
        .write_all(&encode_frame(&full_sandbox(1, 1)).unwrap())
        .unwrap();
    for _ in 0..8 {
        client.poll().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    let gap = ServerMessage::ActorUpdate {
        update: ActorUpdate {
            revision: 3,
            base_revision: Some(2),
            tick: 3,
            unix_seconds: 103.,
            actors: CollectionDelta::default(),
            truncated: false,
            total_in_interest: 1,
        },
    };
    socket.write_all(&encode_frame(&gap).unwrap()).unwrap();
    for _ in 0..8 {
        client.poll().unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert!(client.resync_pending());
    loop {
        let n = socket.read(&mut buffer).unwrap();
        assert!(n > 0);
        decoder.push(&buffer[..n]).unwrap();
        if let Some(message) = decoder.decode::<ClientMessage>().unwrap() {
            assert!(matches!(message, ClientMessage::Resync {}));
            break;
        }
    }
    assert_eq!(client.replication_stats().resync_requests, 1);
    socket
        .write_all(&encode_frame(&full_actor(4, 4)).unwrap())
        .unwrap();
    socket
        .write_all(&encode_frame(&full_sandbox(2, 4)).unwrap())
        .unwrap();
    let mut received = vec![];
    for _ in 0..8 {
        received.extend(client.poll().unwrap());
        thread::sleep(Duration::from_millis(1));
    }
    assert!(!client.resync_pending());
    assert!(
        received
            .iter()
            .any(|m| matches!(m, ServerMessage::Snapshot { tick: 4, .. }))
    );
    assert!(
        received
            .iter()
            .any(|m| matches!(m, ServerMessage::SandboxSnapshot { tick: 4, .. }))
    );
}
