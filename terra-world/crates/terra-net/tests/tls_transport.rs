#![cfg(feature = "tls")]
use std::time::{Duration, Instant};
use terra_net::{
    ClientMessage, ClientTlsConfig, NetClient, Server, ServerConfig, ServerMessage, ServerTlsConfig,
};
use terra_sandbox::Command;

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const OTHER_CA: &[u8] = include_bytes!("fixtures/tls/untrusted-ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/fixture-key.pem");

fn host() -> Server {
    Server::bind_tls(
        ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        },
        ServerTlsConfig::from_pem(CERT, KEY).unwrap(),
    )
    .unwrap()
}

fn until(
    host: &mut Server,
    client: &mut NetClient,
    predicate: impl Fn(&ServerMessage) -> bool,
) -> Vec<ServerMessage> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut messages = Vec::new();
    loop {
        host.update(0.005);
        messages.extend(client.poll().unwrap());
        if messages.iter().any(&predicate) {
            return messages;
        }
        assert!(
            Instant::now() < deadline,
            "TLS/application handshake timed out"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn trusted_tls_handshake_supports_commands_and_resume_identity() {
    let mut host = host();
    let trust = ClientTlsConfig::from_ca_pem(CA, "localhost").unwrap();
    let mut client = NetClient::connect_tls(
        host.local_addr().unwrap(),
        host.config().spawn,
        None,
        Some(host.config().manifest.clone()),
        trust.clone(),
        Duration::from_secs(1),
    )
    .unwrap();
    let messages = until(&mut host, &mut client, |message| {
        matches!(message, ServerMessage::Welcome { .. })
    });
    let (player, token) = messages
        .iter()
        .find_map(|message| match message {
            ServerMessage::Welcome {
                actor_id,
                resume_token,
                ..
            } => Some((*actor_id, resume_token.clone())),
            _ => None,
        })
        .unwrap();
    assert!(client.is_encrypted());
    client
        .send(ClientMessage::Sandbox {
            sequence: 1,
            command: Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        })
        .unwrap();
    until(&mut host, &mut client, |message| {
        matches!(message, ServerMessage::SandboxResult { sequence: 1, .. })
    });
    assert_eq!(host.sandbox().players[&player].inventory["planks"], 4);
    assert!(client.transport_stats().bytes_sent > 0);
    assert!(client.transport_stats().bytes_received > 0);
    drop(client);
    for _ in 0..10 {
        host.update(0.005);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut resumed = NetClient::connect_tls(
        host.local_addr().unwrap(),
        host.config().spawn,
        Some(token),
        None,
        trust,
        Duration::from_secs(1),
    )
    .unwrap();
    let messages = until(&mut host, &mut resumed, |message| {
        matches!(message, ServerMessage::Welcome { .. })
    });
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::Welcome { actor_id, next_sandbox_sequence: 2, .. } if *actor_id == player)));
    assert_eq!(host.sandbox().players.len(), 1);
}

#[test]
fn untrusted_ca_and_wrong_hostname_fail_before_player_admission() {
    for (ca, name) in [(OTHER_CA, "localhost"), (CA, "wrong.example")] {
        let mut host = host();
        let mut client = NetClient::connect_tls(
            host.local_addr().unwrap(),
            host.config().spawn,
            Some("must-not-be-sent-before-verification".into()),
            None,
            ClientTlsConfig::from_ca_pem(ca, name).unwrap(),
            Duration::from_secs(1),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let error = loop {
            host.update(0.005);
            if let Err(error) = client.poll() {
                break error;
            }
            assert!(
                Instant::now() < deadline,
                "invalid server certificate was not rejected"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(error.to_string().contains("TLS"), "{error}");
        assert!(host.sandbox().players.is_empty());
        assert!(host.actors().is_empty());
    }
}

#[test]
fn plaintext_peer_cannot_be_admitted_to_tls_listener_and_other_peers_continue() {
    let mut host = host();
    let mut plaintext =
        NetClient::connect(host.local_addr().unwrap(), host.config().spawn).unwrap();
    for _ in 0..20 {
        let start = Instant::now();
        host.update(0.005);
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "TLS handshake must never block the tick thread"
        );
        if plaintext.poll().is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(host.sandbox().players.is_empty());
    let mut client = NetClient::connect_tls(
        host.local_addr().unwrap(),
        host.config().spawn,
        None,
        None,
        ClientTlsConfig::from_ca_pem(CA, "localhost").unwrap(),
        Duration::from_secs(1),
    )
    .unwrap();
    until(&mut host, &mut client, |message| {
        matches!(message, ServerMessage::Welcome { .. })
    });
    assert_eq!(host.sandbox().players.len(), 1);
}

#[test]
fn invalid_or_oversized_tls_configuration_is_rejected_before_binding() {
    assert!(ServerTlsConfig::from_pem(CERT, b"not a private key").is_err());
    assert!(ServerTlsConfig::from_pem(CA, KEY).is_err());
    assert!(ClientTlsConfig::from_ca_pem(b"not a CA", "localhost").is_err());
    assert!(ClientTlsConfig::from_ca_pem(CA, "bad host name").is_err());
    assert!(ClientTlsConfig::from_ca_pem(&vec![b'x'; 1024 * 1024 + 1], "localhost").is_err());
}

#[test]
fn obsolete_protocol_alpn_is_rejected_before_application_admission() {
    use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
    use std::{net::TcpStream, sync::Arc};
    let mut host = host();
    let mut roots = rustls::RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(CA) {
        roots.add(certificate.unwrap()).unwrap();
    }
    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.alpn_protocols =
        vec![format!("terra-world/{}", terra_net::PROTOCOL_VERSION - 1).into_bytes()];
    let mut tls =
        rustls::ClientConnection::new(Arc::new(config), ServerName::try_from("localhost").unwrap())
            .unwrap();
    let mut socket = TcpStream::connect(host.local_addr().unwrap()).unwrap();
    socket.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut rejected = false;
    while !rejected {
        while tls.wants_write() {
            match tls.write_tls(&mut socket) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    rejected = true;
                    break;
                }
            }
        }
        host.update(0.025);
        match tls.read_tls(&mut socket) {
            Ok(0) => rejected = true,
            Ok(_) => {
                if tls.process_new_packets().is_err() {
                    rejected = true;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => rejected = true,
        }
        assert!(Instant::now() < deadline, "obsolete ALPN was not rejected");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        tls.is_handshaking(),
        "must not negotiate an unsupported application protocol"
    );
    assert!(host.sandbox().players.is_empty());
}
