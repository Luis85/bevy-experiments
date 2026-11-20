use super::*;
use crate::NetClient;
use std::{
    thread,
    time::{Duration, Instant},
};

/// Drive the actual nonblocking socket and authority loop while the peer's
/// application output is already near its cap. Reliable work must remain in the
/// bounded inbox until a maximum-size result can be admitted, then execute once.
#[test]
fn reliable_command_waits_for_response_headroom_and_executes_once_after_drain() {
    let mut server = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    })
    .unwrap();
    let mut client =
        NetClient::connect(server.local_addr().unwrap(), server.config().spawn).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut owner = None;
    while owner.is_none() {
        assert!(Instant::now() < deadline, "fixture handshake timed out");
        assert_eq!(server.update(0.).disconnected, 0);
        for message in client.poll().unwrap() {
            if let ServerMessage::Welcome { actor_id, .. } = message {
                owner = Some(actor_id);
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    let owner = owner.unwrap();
    let inventory_before = server.sandbox.players[&owner].inventory.clone();
    let large_control = ServerMessage::Error {
        code: "fixture_backlog".into(),
        message: "x".repeat(240 * 1024),
    };
    let peer = server.peers.values_mut().next().unwrap();
    peer.connection.queue(&large_control).unwrap();
    peer.connection.queue(&large_control).unwrap();
    assert!(!peer.connection.has_frame_headroom());
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: terra_sandbox::Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    let mut deferrals = 0;
    for _ in 0..2 {
        let report = server.update(0.);
        assert_eq!(report.disconnected, 0);
        deferrals += report.deferred_command_batches;
        assert_eq!(server.sandbox.players[&owner].inventory, inventory_before);
        assert!(!server.sandbox_sequences.contains_key(&owner));
        assert!(report.max_peer_pending_bytes <= 2 * crate::MAX_FRAME_BYTES);
    }
    assert!(deferrals > 0, "reliable admission was not backpressured");
    assert_eq!(server.peers.values().next().unwrap().inbox.len(), 1);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut successes = 0;
    while successes == 0 {
        assert!(Instant::now() < deadline, "reliable command did not drain");
        let report = server.update(0.);
        assert_eq!(report.disconnected, 0);
        assert_eq!(report.domain_errors, 0);
        for message in client.poll().unwrap() {
            match message {
                ServerMessage::SandboxResult { sequence: 1, .. } => successes += 1,
                ServerMessage::SandboxRejected { .. } => panic!("queued command was rejected"),
                ServerMessage::Error { code, .. } => assert_eq!(code, "fixture_backlog"),
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..4 {
        server.update(0.);
        successes += client
            .poll()
            .unwrap()
            .into_iter()
            .filter(|message| matches!(message, ServerMessage::SandboxResult { sequence: 1, .. }))
            .count();
    }
    assert_eq!(successes, 1);
    assert_eq!(server.sandbox_sequences.get(&owner), Some(&1));
    assert_eq!(
        server.sandbox.players[&owner].inventory.get("wood"),
        Some(&28)
    );
    assert_eq!(
        server.sandbox.players[&owner].inventory.get("planks"),
        Some(&4)
    );
    assert!(server.peers.values().next().unwrap().inbox.is_empty());
}
