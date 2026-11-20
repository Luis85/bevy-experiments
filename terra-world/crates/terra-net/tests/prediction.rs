use std::time::{Duration, Instant};
use terra_net::{ClientMessage, NetClient, Server, ServerConfig, ServerMessage};

fn receive(
    host: &mut Server,
    client: &mut NetClient,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> ServerMessage {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let report = host.update(0.05);
        assert_eq!(report.domain_errors, 0);
        for message in client.poll().unwrap() {
            if predicate(&message) {
                return message;
            }
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn owner_acknowledges_only_a_completed_step_and_never_another_players_motion() {
    let mut host = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..Default::default()
    })
    .unwrap();
    let mut a = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let ServerMessage::Welcome {
        actor_id: player_a, ..
    } = receive(&mut host, &mut a, |m| {
        matches!(m, ServerMessage::Welcome { .. })
    })
    else {
        unreachable!()
    };
    let mut b = NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    let ServerMessage::Welcome {
        actor_id: player_b, ..
    } = receive(&mut host, &mut b, |m| {
        matches!(m, ServerMessage::Welcome { .. })
    })
    else {
        unreachable!()
    };
    assert_ne!(player_a, player_b);
    // Drain pre-input snapshots before the receipt-only updates below.
    a.poll().unwrap();
    b.poll().unwrap();
    let before = host.actors()[&player_a].position;
    a.send(ClientMessage::Move {
        sequence: 7,
        velocity_enu_m_s: [3., 0., 0.],
    })
    .unwrap();
    for _ in 0..4 {
        host.update(0.);
        for message in a.poll().unwrap() {
            if let ServerMessage::MotionState { state } = message {
                assert_ne!(state.sequence, Some(7));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(host.actors()[&player_a].position, before);
    let ServerMessage::MotionState { state } = receive(
        &mut host,
        &mut a,
        |m| matches!(m, ServerMessage::MotionState { state } if state.sequence == Some(7)),
    ) else {
        unreachable!()
    };
    assert_eq!(state.player_id, player_a);
    assert_eq!(state.actor.id, player_a);
    assert!(state.actor.position.surface_distance_m(before) > 0.01);
    state.validate().unwrap();
    let mut saw_b = false;
    for message in b.poll().unwrap() {
        if let ServerMessage::MotionState { state } = message {
            assert_eq!(state.player_id, player_b);
            assert_eq!(state.actor.id, player_b);
            assert_eq!(state.sequence, None);
            saw_b = true;
        }
    }
    assert!(saw_b);
}
