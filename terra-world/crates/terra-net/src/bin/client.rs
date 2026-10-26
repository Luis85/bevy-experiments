use std::{
    error::Error,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use terra_core::GeoPosition;
use terra_net::{
    BaseWorldManifest, ClientMessage, NetClient, PROTOCOL_VERSION, ServerMessage, SessionIdentity,
};
use terra_sandbox::Command;

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, Box<dyn Error>> {
    if !std::fs::metadata(path)?.is_file() {
        return Err("JSON input must be a regular file".into());
    }
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("JSON input exceeds its byte budget".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("terra-client: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut address: SocketAddr = "127.0.0.1:7777".parse()?;
    let mut seconds = 10.0_f64;
    let mut velocity = [0.0, 0.0, 0.0];
    let mut interest = GeoPosition::new(48.137, 11.575, 0.0)?;
    let mut session_path = None::<PathBuf>;
    let mut command_path = None::<PathBuf>;
    let mut expected_manifest_path = None::<PathBuf>;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("missing value for {argument}"))
        };
        match argument.as_str() {
            "--connect" => address = value()?.parse()?,
            "--seconds" => seconds = value()?.parse()?,
            "--east" => velocity[0] = value()?.parse()?,
            "--north" => velocity[1] = value()?.parse()?,
            "--lat" => interest.latitude = value()?.parse()?,
            "--lon" => interest.longitude = value()?.parse()?,
            "--session" => session_path = Some(value()?.into()),
            "--command" => command_path = Some(value()?.into()),
            "--expect-manifest" => expected_manifest_path = Some(value()?.into()),
            "--help" | "-h" => {
                println!(
                    "Terra reference client\n\n  --connect ADDRESS:PORT  default 127.0.0.1:7777\n  --seconds N             default 10\n  --east N --north N      movement intent in m/s\n  --lat N --lon N         spectator interest only\n  --session PATH          store/reuse this server's player resume token\n  --command PATH          submit one sandbox Command JSON after handshake\n  --expect-manifest PATH  reject any different base-world manifest\n\nDo not share the session file: it controls this player's saved claims and inventory."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    if !seconds.is_finite() || !(0.1..=86_400.0).contains(&seconds) {
        return Err("seconds must be finite and within [0.1, 86400]".into());
    }
    if !velocity.iter().all(|value: &f64| value.is_finite())
        || velocity[0].hypot(velocity[1]) > 12.0
    {
        return Err("reference human movement must be finite and at most 12 m/s".into());
    }
    let expected_manifest: Option<BaseWorldManifest> = if let Some(path) = expected_manifest_path {
        Some(read_json(&path, 8192)?)
    } else {
        None
    };
    let token = if let Some(path) = &session_path {
        SessionIdentity::load(path, address)?.map(|identity| identity.resume_token)
    } else {
        None
    };
    let mut command: Option<Command> = if let Some(path) = command_path {
        Some(read_json(&path, 16384)?)
    } else {
        None
    };
    let mut client = if let Some(expected) = &expected_manifest {
        NetClient::connect_with_manifest(address, interest, token, expected.clone())?
    } else {
        NetClient::connect_with_token(address, interest, token)?
    };
    let command_requested = command.is_some();
    let mut command_applied = false;
    let start = Instant::now();
    let mut last_move = Instant::now();
    let mut sequence = 0_u64;
    let mut ready = false;
    let mut snapshots = 0_u64;
    while start.elapsed().as_secs_f64() < seconds {
        for message in client.poll()? {
            match message {
                ServerMessage::Welcome {
                    client_id,
                    actor_id,
                    resume_token,
                    next_sandbox_sequence,
                    protocol,
                    manifest,
                    ..
                } => {
                    if ready
                        || protocol != PROTOCOL_VERSION
                        || actor_id == 0
                        || resume_token.len() != 64
                    {
                        return Err("invalid or repeated server handshake".into());
                    }
                    if expected_manifest
                        .as_ref()
                        .is_some_and(|expected| expected != &manifest)
                    {
                        return Err(
                            "server base-world manifest does not match --expect-manifest".into(),
                        );
                    }
                    ready = true;
                    println!(
                        "Connected: client={client_id} actor={actor_id} provider={} revision={} data_epoch={:?}",
                        manifest.terrain_provider,
                        manifest.base_data_revision,
                        manifest.data_epoch_unix
                    );
                    if let Some(path) = &session_path {
                        SessionIdentity {
                            server: address,
                            resume_token,
                        }
                        .save(path)?;
                    }
                    if let Some(command) = command.take() {
                        client.send(ClientMessage::Sandbox {
                            sequence: next_sandbox_sequence,
                            command,
                        })?;
                    }
                }
                ServerMessage::Snapshot {
                    tick,
                    actors,
                    truncated,
                    ..
                } => {
                    snapshots += 1;
                    if snapshots == 1 || snapshots.is_multiple_of(10) {
                        println!(
                            "Snapshot tick={tick} actors={} truncated={truncated}",
                            actors.len()
                        );
                    }
                }
                ServerMessage::SandboxResult { sequence, outcome } => {
                    command_applied = true;
                    println!("Command {sequence}: {}", outcome.message)
                }
                ServerMessage::SandboxRejected { sequence, message } => {
                    eprintln!("Command {sequence} rejected: {message}")
                }
                ServerMessage::Error { code, message } => {
                    if !ready {
                        return Err(format!("server rejected handshake ({code}): {message}").into());
                    }
                    eprintln!("Server {code}: {message}");
                }
                _ => {}
            }
        }
        if ready && last_move.elapsed() >= Duration::from_millis(100) {
            sequence += 1;
            client.send(ClientMessage::Move {
                sequence,
                velocity_enu_m_s: velocity,
            })?;
            last_move = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if ready {
        client.send(ClientMessage::Move {
            sequence: sequence + 1,
            velocity_enu_m_s: [0.0; 3],
        })?;
    }
    println!("Received {snapshots} authoritative snapshots");
    if !ready || snapshots == 0 {
        return Err("handshake or snapshot reception did not complete".into());
    }
    if command_requested && !command_applied {
        return Err("requested sandbox command was not accepted".into());
    }
    Ok(())
}
