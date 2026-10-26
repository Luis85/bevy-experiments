//! Bounded client adapter. Connection setup runs outside Bevy frame work.
use crate::{SandboxIntent, SandboxQueue, SandboxRuntime, WorldConfig, WorldRuntime, WorldSet};
use bevy::prelude::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};
use terra_core::{ActorState, GeoPosition, synthetic_weather};
use terra_net::{
    BaseWorldManifest, ClientMessage, NetClient, PROTOCOL_VERSION, ServerMessage, SessionIdentity,
};
use terra_sandbox::{SandboxState, SandboxView};

pub struct TerraNetworkPlugin {
    pub address: SocketAddr,
    pub interest: GeoPosition,
    pub resume_token: Option<String>,
}
/// Insert before TerraNetworkPlugin to preserve the same player across restarts.
/// Use one path per server/player; its bearer capability is address-pinned.
#[derive(Resource)]
pub struct NetworkSession {
    pub path: PathBuf,
}
#[derive(Resource)]
struct ClientResource {
    client: Option<NetClient>,
    connecting: Mutex<mpsc::Receiver<Result<NetClient, String>>>,
    terminal: bool,
    connected_at: Option<Instant>,
    last_snapshot: Option<Instant>,
    last_input: Instant,
    last_ping: Instant,
    address: SocketAddr,
    session_path: Option<PathBuf>,
}
#[derive(Resource, Default)]
pub struct NetworkStatus {
    pub connected: bool,
    pub actor_id: Option<u64>,
    pub message: String,
    pub actors: BTreeMap<u64, ActorState>,
    pub tick: u64,
    pub move_sequence: u64,
    pub sandbox_sequence: u64,
    pub resume_token: Option<String>,
    pub desired_velocity_enu_m_s: [f64; 3],
    pub follow_player: bool,
    /// Latest completeness flags are tracked independently because their
    /// snapshots arrive at different frequencies.
    pub actor_snapshot_truncated: bool,
    pub sandbox_snapshot_truncated: bool,
    pub snapshot_truncated: bool,
}
impl Plugin for TerraNetworkPlugin {
    fn build(&self, app: &mut App) {
        // Lock authority before schedules run, including the first fixed tick.
        app.world_mut()
            .get_resource_mut::<WorldRuntime>()
            .expect("add TerraWorldPlugin before TerraNetworkPlugin")
            .network_authority = true;
        if let Some(mut sandbox) = app.world_mut().get_resource_mut::<SandboxRuntime>() {
            sandbox.state = SandboxState::default();
            sandbox.player_id = 0;
            sandbox.revision += 1;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let address = self.address;
        let interest = self.interest;
        let token = self.resume_token.clone();
        let session_path = app
            .world()
            .get_resource::<NetworkSession>()
            .map(|session| session.path.clone());
        let worker_session_path = session_path.clone();
        let expected_manifest =
            manifest_from_config(&app.world().resource::<WorldRuntime>().config);
        let live_data = app.world().resource::<WorldRuntime>().config.live_data;
        std::thread::spawn(move || {
            let result = (|| {
                if live_data { return Err(terra_net::NetError::Configuration("reference multiplayer requires matching procedural data or a versioned local pack".into())); }
                let token = match (token, worker_session_path) {
                    (Some(token), _) => Some(token),
                    (None, Some(path)) => {
                        SessionIdentity::load(path, address)?.map(|identity| identity.resume_token)
                    }
                    (None, None) => None,
                };
                NetClient::connect_with_manifest(address, interest, token, expected_manifest)
            })()
            .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
        app.insert_resource(ClientResource {
            client: None,
            connecting: Mutex::new(rx),
            terminal: false,
            connected_at: None,
            last_snapshot: None,
            last_input: Instant::now(),
            last_ping: Instant::now(),
            address,
            session_path,
        })
        .insert_resource(NetworkStatus {
            message: format!("Connecting to {address}"),
            follow_player: true,
            ..default()
        })
        .add_systems(
            Update,
            network_update
                .after(WorldSet::Stream)
                .before(WorldSet::Sandbox),
        );
    }
}

fn network_update(
    mut connection: ResMut<ClientResource>,
    mut status: ResMut<NetworkStatus>,
    mut runtime: ResMut<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
) {
    runtime.network_authority = true;
    if connection.client.is_none() && !connection.terminal {
        let result = match connection.connecting.lock() {
            Ok(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("connection worker ended".into()))
                }
            },
            Err(_) => Some(Err("connection worker channel failed".into())),
        };
        if let Some(result) = result {
            match result {
                Ok(client) => {
                    connection.client = Some(client);
                    connection.connected_at = Some(Instant::now());
                }
                Err(error) => {
                    disconnect(
                        &mut connection,
                        &mut status,
                        &mut sandbox,
                        &mut queue,
                        format!("Connection failed: {error}"),
                    );
                    return;
                }
            }
        }
    }
    if connection.client.is_none() {
        if !queue.0.is_empty() {
            sandbox.last_message = "Connect to a server before changing the world.".into();
        }
        queue.0.clear();
        return;
    }
    // Poll before checking the deadline: a long render frame can leave fresh
    // authoritative data waiting in the socket. Each nonblocking poll is capped
    // at 32 messages / 64 KiB of reads. Eight passes can finish a maximum-size
    // framed payload and catch up a modest stall without an unbounded drain.
    for _ in 0..8 {
        let messages = match connection.client.as_mut().expect("checked above").poll() {
            Ok(messages) => messages,
            Err(error) => {
                disconnect(
                    &mut connection,
                    &mut status,
                    &mut sandbox,
                    &mut queue,
                    format!("Disconnected: {error}"),
                );
                return;
            }
        };
        for message in messages {
            if let Err(error) = apply_message(
                message,
                &mut connection,
                &mut status,
                &mut runtime,
                &mut sandbox,
            ) {
                disconnect(
                    &mut connection,
                    &mut status,
                    &mut sandbox,
                    &mut queue,
                    error,
                );
                return;
            }
        }
    }
    let timed_out = if status.connected {
        connection
            .last_snapshot
            .is_some_and(|time| time.elapsed() > Duration::from_secs(10))
    } else {
        connection
            .connected_at
            .is_some_and(|time| time.elapsed() > Duration::from_secs(10))
    };
    if timed_out {
        disconnect(
            &mut connection,
            &mut status,
            &mut sandbox,
            &mut queue,
            "Connection timed out while waiting for authoritative world data.".into(),
        );
        return;
    }
    if !status.connected {
        if !queue.0.is_empty() {
            sandbox.last_message = "Waiting for the server handshake.".into();
        }
        queue.0.clear();
        return;
    }
    if connection.last_input.elapsed() >= Duration::from_millis(100) {
        connection.last_input = Instant::now();
        let Some(sequence) = status.move_sequence.checked_add(1) else {
            disconnect(
                &mut connection,
                &mut status,
                &mut sandbox,
                &mut queue,
                "Movement sequence exhausted.".into(),
            );
            return;
        };
        status.move_sequence = sequence;
        let sent = connection
            .client
            .as_mut()
            .expect("connected")
            .send(ClientMessage::Move {
                sequence,
                velocity_enu_m_s: status.desired_velocity_enu_m_s,
            })
            .and_then(|()| {
                connection
                    .client
                    .as_mut()
                    .expect("connected")
                    .send(ClientMessage::Focus {
                        position: runtime.focus,
                        radius_m: 5_000.0,
                    })
            });
        if let Err(error) = sent {
            disconnect(
                &mut connection,
                &mut status,
                &mut sandbox,
                &mut queue,
                format!("Send failed: {error}"),
            );
            return;
        }
    }
    if connection.last_ping.elapsed() >= Duration::from_secs(5) {
        connection.last_ping = Instant::now();
        if let Err(error) = connection
            .client
            .as_mut()
            .expect("connected")
            .send(ClientMessage::Ping { nonce: status.tick })
        {
            disconnect(
                &mut connection,
                &mut status,
                &mut sandbox,
                &mut queue,
                format!("Heartbeat failed: {error}"),
            );
            return;
        }
    }
    for _ in 0..8 {
        let Some(intent) = queue.0.pop_front() else {
            break;
        };
        match intent {
            SandboxIntent::Command(command) => {
                let sequence = status.sandbox_sequence;
                let Some(next) = sequence.checked_add(1) else {
                    disconnect(
                        &mut connection,
                        &mut status,
                        &mut sandbox,
                        &mut queue,
                        "Sandbox sequence exhausted.".into(),
                    );
                    return;
                };
                status.sandbox_sequence = next;
                if let Err(error) = connection
                    .client
                    .as_mut()
                    .expect("connected")
                    .send(ClientMessage::Sandbox { sequence, command })
                {
                    disconnect(
                        &mut connection,
                        &mut status,
                        &mut sandbox,
                        &mut queue,
                        format!("Action send failed: {error}"),
                    );
                    return;
                }
            }
            _ => {
                sandbox.last_message =
                    "Multiplayer movement belongs to the server; the host saves the world.".into()
            }
        }
    }
}

fn apply_message(
    message: ServerMessage,
    connection: &mut ClientResource,
    status: &mut NetworkStatus,
    runtime: &mut WorldRuntime,
    sandbox: &mut SandboxRuntime,
) -> Result<(), String> {
    match message {
        ServerMessage::Welcome {
            protocol,
            actor_id,
            spawn,
            manifest,
            unix_seconds,
            time_scale,
            paused,
            tick,
            resume_token,
            next_sandbox_sequence,
            ..
        } => {
            if status.connected
                || protocol != PROTOCOL_VERSION
                || actor_id == 0
                || !valid_position(spawn)
                || !valid_clock(unix_seconds)
                || !time_scale.is_finite()
                || !(0.0..=86_400.0).contains(&time_scale)
                || next_sandbox_sequence == u64::MAX
                || resume_token.len() != 64
                || !resume_token.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err("Server sent an invalid handshake.".into());
            }
            compatible_manifest(&manifest, &runtime.config)?;
            status.resume_token = Some(resume_token.clone());
            if let Some(path) = &connection.session_path {
                SessionIdentity { server: connection.address, resume_token }.save(path)
                    .map_err(|error| format!("Could not save your multiplayer identity: {error}. Choose a writable session path and reconnect."))?;
            }
            status.connected = true;
            status.actor_id = Some(actor_id);
            status.sandbox_sequence = next_sandbox_sequence;
            status.tick = tick;
            status.message = format!("Connected · player {actor_id}");
            sandbox.player_id = actor_id;
            runtime.focus = GeoPosition {
                altitude_m: 0.0,
                ..spawn
            };
            runtime.clock.unix_seconds = unix_seconds;
            runtime.clock.time_scale = time_scale;
            runtime.clock.paused = paused;
            connection.last_snapshot = Some(Instant::now());
        }
        ServerMessage::Snapshot {
            tick,
            unix_seconds,
            actors,
            truncated,
            ..
        } => {
            if !status.connected {
                return Err("Received world state before handshake.".into());
            }
            if tick < status.tick {
                return Ok(());
            }
            let mut ids = BTreeSet::new();
            if actors.len() > 512
                || !valid_clock(unix_seconds)
                || actors.iter().any(|actor| {
                    actor.id == 0
                        || !ids.insert(actor.id)
                        || actor.validate().is_err()
                        || !valid_position(actor.position)
                })
            {
                return Err("Server sent an invalid actor snapshot.".into());
            }
            status.tick = tick;
            status.actor_snapshot_truncated = truncated;
            status.snapshot_truncated = truncated || status.sandbox_snapshot_truncated;
            runtime.clock.unix_seconds = unix_seconds;
            status.actors = actors.into_iter().map(|actor| (actor.id, actor)).collect();
            // The host's primary pose arrives at 10 Hz; the richer living view
            // arrives at 5 Hz. Feed that pose into the SAME controlled body,
            // without predicting inventory, orders or ownership on the client.
            if let Some(actor) = status.actor_id.and_then(|id| status.actors.get(&id)) {
                let active = sandbox
                    .state
                    .living
                    .accounts
                    .get(&actor.id)
                    .filter(|account| account.possessed)
                    .map(|account| account.active_unit);
                if let Some(unit) = active.and_then(|id| sandbox.state.living.units.get_mut(&id))
                    && unit.kind == actor.kind
                {
                    unit.position = actor.position;
                    unit.velocity_enu_m_s = actor.velocity_enu_m_s;
                    unit.heading_deg = actor.heading_deg;
                    if let Some(player) = sandbox.state.players.get_mut(&actor.id) {
                        player.position = actor.position;
                    }
                }
            }
            runtime.weather =
                synthetic_weather(&runtime.focus, &runtime.clock, runtime.config.seed);
            connection.last_snapshot = Some(Instant::now());
        }
        ServerMessage::SandboxSnapshot { view, .. } => {
            if !status.connected || !valid_view(&view, status.actor_id) {
                return Err("Server sent an invalid sandbox snapshot.".into());
            }
            status.sandbox_snapshot_truncated = view.truncated || view.living.truncated;
            status.snapshot_truncated =
                status.actor_snapshot_truncated || status.sandbox_snapshot_truncated;
            sandbox.player_id = view.player.id;
            sandbox.state.version = view.version;
            sandbox.state.tick_seconds = view.tick_seconds;
            sandbox.state.players.clear();
            sandbox.state.players.insert(view.player.id, view.player);
            sandbox.state.plots = view.plots.into_iter().map(|plot| (plot.id, plot)).collect();
            sandbox.state.structures = view
                .structures
                .into_iter()
                .map(|structure| (structure.id, structure))
                .collect();
            sandbox.state.resource_nodes = view
                .resource_nodes
                .into_iter()
                .map(|node| (node.id.clone(), node))
                .collect();
            sandbox.state.farms = view.farms.into_iter().map(|farm| (farm.id, farm)).collect();
            sandbox.state.feature_edits = view.feature_edits.into_iter().collect();
            sandbox.state.living = terra_sandbox::LivingWorld::from_view(view.living);
            sandbox.state.factions = view
                .factions
                .into_iter()
                .map(|faction| (faction.id, faction))
                .collect();
            // A bounded replica is never treated as a complete writable save.
            sandbox.revision += 1;
        }
        ServerMessage::SandboxResult { outcome, .. } => {
            sandbox.last_message = bounded_message(&outcome.message);
            sandbox.revision += 1;
        }
        ServerMessage::SandboxRejected { message, .. } => {
            sandbox.last_message = format!("Server rejected action: {}", bounded_message(&message));
        }
        ServerMessage::Error { message, .. } => {
            let message = bounded_message(&message);
            if !status.connected {
                return Err(format!("Handshake rejected: {message}"));
            }
            status.message = message;
        }
        ServerMessage::Pong { .. } => {}
    }
    Ok(())
}

fn compatible_manifest(manifest: &BaseWorldManifest, config: &WorldConfig) -> Result<(), String> {
    if config.live_data
        || config.heightmap_manifest.is_some()
        || !manifest.matches_data(&manifest_from_config(config))
    {
        return Err("Server base-world provider, version, epoch or political overlay differs from this client configuration.".into());
    }
    Ok(())
}
fn manifest_from_config(config: &WorldConfig) -> BaseWorldManifest {
    BaseWorldManifest {
        seed: config.seed,
        base_data_revision: config.base_data_revision.clone(),
        terrain_provider: if config.historical_pack.is_some() {
            "historical-pack"
        } else {
            "procedural"
        }
        .into(),
        chunk_level: config.chunk_level,
        terrain_resolution: config.terrain_resolution,
        data_epoch_unix: config.data_epoch_unix,
        political_dataset_revision: config.political_dataset_revision.clone(),
        ..BaseWorldManifest::default()
    }
}
fn valid_clock(time: f64) -> bool {
    time.is_finite() && time.abs() <= 1e12
}
fn valid_position(position: GeoPosition) -> bool {
    position.validate().is_ok()
        && (-180.0..=180.0).contains(&position.longitude)
        && (-12_000.0..=100_000.0).contains(&position.altitude_m)
}
fn bounded_message(message: &str) -> String {
    message
        .chars()
        .take(512)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}
fn valid_view(view: &SandboxView, actor_id: Option<u64>) -> bool {
    view.version == terra_sandbox::SAVE_VERSION
        && view.living.validate_for(view.player.id).is_ok()
        && Some(view.player.id) == actor_id
        && valid_position(view.player.position)
        && view.tick_seconds.is_finite()
        && view.tick_seconds >= 0.0
        && view.player.inventory.len() <= 64
        && view.player.inventory.iter().all(|(item, count)| {
            !item.is_empty() && item.len() <= 128 && *count <= terra_sandbox::MAX_ITEM_COUNT
        })
        && [
            view.plots.len(),
            view.structures.len(),
            view.farms.len(),
            view.resource_nodes.len(),
            view.feature_edits.len(),
        ]
        .into_iter()
        .all(|count| count <= 64)
        && view.factions.len() <= 8
        && view.factions.iter().all(|faction| {
            faction.id > 0
                && !faction.name.trim().is_empty()
                && faction.name.chars().count() <= 64
                && faction.name.len() <= 128
                && faction.members.len() <= 128
                && faction.invites.len() <= 128
                && faction.members.get(&faction.leader) == Some(&terra_sandbox::FactionRole::Leader)
        })
        && view.plots.iter().all(|plot| valid_position(plot.center))
        && view
            .structures
            .iter()
            .all(|structure| valid_position(structure.position))
        && view.farms.iter().all(|farm| valid_position(farm.position))
        && view
            .resource_nodes
            .iter()
            .all(|node| valid_position(node.position))
}
fn disconnect(
    connection: &mut ClientResource,
    status: &mut NetworkStatus,
    sandbox: &mut SandboxRuntime,
    queue: &mut SandboxQueue,
    message: String,
) {
    let message = bounded_message(&message);
    eprintln!("Terra multiplayer: {message}");
    connection.client = None;
    connection.terminal = true;
    status.connected = false;
    status.actor_id = None;
    status.actors.clear();
    status.actor_snapshot_truncated = false;
    status.sandbox_snapshot_truncated = false;
    status.snapshot_truncated = false;
    status.desired_velocity_enu_m_s = [0.0; 3];
    status.message = message.clone();
    sandbox.last_message = message;
    queue.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct LateActionProbe {
        issued: bool,
        saw_nine_queued: bool,
        saw_one_remaining: bool,
    }

    fn issue_actions_after_network_send(
        status: Res<NetworkStatus>,
        sandbox: Res<SandboxRuntime>,
        mut queue: ResMut<SandboxQueue>,
        mut probe: ResMut<LateActionProbe>,
    ) {
        if probe.issued || !status.connected {
            return;
        }
        let Some(player) = sandbox.state.players.get(&sandbox.player_id) else {
            return;
        };
        queue
            .0
            .push_back(SandboxIntent::Command(terra_sandbox::Command::ClaimPlot {
                center: player.position,
                radius_m: 40.,
            }));
        // Exceed the bridge's eight-command pass, without exhausting the
        // server's legitimate command burst or starter inventory.
        for _ in 0..8 {
            queue
                .0
                .push_back(SandboxIntent::Command(terra_sandbox::Command::Craft {
                    recipe: "planks".into(),
                    count: 1,
                }));
        }
        probe.issued = true;
    }

    fn observe_queue_after_local_system(
        queue: Res<SandboxQueue>,
        mut probe: ResMut<LateActionProbe>,
    ) {
        if probe.issued {
            probe.saw_nine_queued |= queue.0.len() == 9;
            probe.saw_one_remaining |= queue.0.len() == 1;
        }
    }

    #[test]
    fn late_network_intents_and_bounded_backlog_reach_the_authoritative_server() {
        use crate::TerraWorldPlugin;
        use terra_net::{Server, ServerConfig};

        let config = WorldConfig {
            chunk_radius: 0,
            terrain_resolution: 9,
            max_resident_chunks: 4,
            max_queued: 4,
            max_in_flight: 1,
            live_weather: false,
            cache_dir: std::env::temp_dir()
                .join(format!("terra-bevy-network-test-{}", std::process::id())),
            ..Default::default()
        };
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            spawn: config.focus,
            manifest: manifest_from_config(&config),
            ..ServerConfig::default()
        })
        .unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(TerraWorldPlugin::new(config.clone()))
            .add_plugins(TerraNetworkPlugin {
                address: server.local_addr().unwrap(),
                interest: config.focus,
                resume_token: None,
            })
            .init_resource::<LateActionProbe>()
            .add_systems(
                Update,
                issue_actions_after_network_send
                    .after(network_update)
                    .before(crate::sandbox::apply_sandbox_commands),
            )
            .add_systems(
                Update,
                observe_queue_after_local_system.after(crate::sandbox::apply_sandbox_commands),
            );

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut previous = Instant::now();
        loop {
            let now = Instant::now();
            server.update((now - previous).as_secs_f64());
            previous = now;
            app.update();
            let replica = app.world().resource::<SandboxRuntime>();
            let complete = server.sandbox().plots.len() == 1
                && server
                    .sandbox()
                    .players
                    .values()
                    .any(|player| player.inventory.get("planks") == Some(&32))
                && replica.state.plots.len() == 1
                && replica
                    .state
                    .players
                    .values()
                    .any(|player| player.inventory.get("planks") == Some(&32));
            if complete {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "scheduled network commands did not complete: {} / {}",
                app.world().resource::<NetworkStatus>().message,
                replica.last_message
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let probe = app.world().resource::<LateActionProbe>();
        assert!(
            probe.saw_nine_queued,
            "late UI intents must survive the local command pass"
        );
        assert!(
            probe.saw_one_remaining,
            "bounded send backlog must survive until the next pass"
        );
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        assert_eq!(server.sandbox().players.len(), 1);
        server.sandbox().validate().unwrap();
    }

    #[test]
    fn queued_snapshots_recover_an_expired_frame_deadline_but_silence_still_times_out() {
        use crate::TerraWorldPlugin;
        use terra_net::{Server, ServerConfig};

        let config = WorldConfig {
            chunk_radius: 0,
            terrain_resolution: 9,
            max_resident_chunks: 4,
            max_queued: 4,
            max_in_flight: 1,
            live_weather: false,
            cache_dir: std::env::temp_dir()
                .join(format!("terra-bevy-deadline-test-{}", std::process::id())),
            ..Default::default()
        };
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            spawn: config.focus,
            manifest: manifest_from_config(&config),
            ..ServerConfig::default()
        })
        .unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(TerraWorldPlugin::new(config.clone()))
            .add_plugins(TerraNetworkPlugin {
                address: server.local_addr().unwrap(),
                interest: config.focus,
                resume_token: None,
            });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !app.world().resource::<NetworkStatus>().connected {
            server.update(0.025);
            app.update();
            assert!(Instant::now() < deadline, "handshake did not complete");
            std::thread::sleep(Duration::from_millis(1));
        }

        // Simulate a blocked render frame without sleeping for the timeout.
        // The real TCP host queues more snapshots than one 32-message poll can
        // consume, while still staying below its independent idle deadline.
        for _ in 0..100 {
            server.update(0.1);
        }
        // Default 20 Hz ticks publish 10 Hz snapshots on even ticks.
        if !server.tick().is_multiple_of(2) {
            server.update(0.05);
        }
        let latest_tick = server.tick();
        app.world_mut()
            .resource_mut::<ClientResource>()
            .last_snapshot = Some(Instant::now() - Duration::from_secs(11));
        app.update();
        let status = app.world().resource::<NetworkStatus>();
        assert!(
            status.connected,
            "queued data must refresh an expired frame timestamp: {}",
            status.message
        );
        assert_eq!(
            status.tick, latest_tick,
            "bounded catch-up must consume this queued snapshot burst"
        );

        // The same socket remains open, but the host sends nothing else. Only
        // actual authoritative progress refreshes the deadline; polling itself
        // and an empty socket must not turn a timeout into an eternal session.
        for _ in 0..4 {
            app.update();
            std::thread::sleep(Duration::from_millis(1));
        }
        app.world_mut()
            .resource_mut::<ClientResource>()
            .last_snapshot = Some(Instant::now() - Duration::from_secs(11));
        app.update();
        let status = app.world().resource::<NetworkStatus>();
        assert!(!status.connected);
        assert!(status.message.contains("timed out"));
        assert!(app.world().resource::<ClientResource>().terminal);
    }

    #[test]
    fn mixed_snapshot_rates_preserve_each_views_completeness() {
        let config = WorldConfig::default();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(crate::TerraWorldPlugin::new(config.clone()));
        let mut runtime = app.world_mut().remove_resource::<WorldRuntime>().unwrap();
        let mut sandbox = SandboxRuntime::new(config.focus);
        let (_tx, rx) = mpsc::sync_channel(1);
        let mut connection = ClientResource {
            client: None,
            connecting: Mutex::new(rx),
            terminal: false,
            connected_at: None,
            last_snapshot: None,
            last_input: Instant::now(),
            last_ping: Instant::now(),
            address: "127.0.0.1:7777".parse().unwrap(),
            session_path: None,
        };
        let mut status = NetworkStatus {
            connected: true,
            actor_id: Some(1),
            ..default()
        };
        let full = sandbox.state.snapshot_for(1, 5000.).unwrap();
        let mut partial = full.clone();
        partial.truncated = true;
        apply_message(
            ServerMessage::SandboxSnapshot {
                tick: 1,
                view: partial,
            },
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        let actor_view = |tick, truncated| ServerMessage::Snapshot {
            tick,
            unix_seconds: 1_782_000_000.,
            actors: vec![],
            truncated,
            total_in_interest: 0,
        };
        apply_message(
            actor_view(2, false),
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        assert!(
            status.snapshot_truncated,
            "a faster complete pose view must not erase a partial economy view"
        );
        apply_message(
            ServerMessage::SandboxSnapshot {
                tick: 3,
                view: full.clone(),
            },
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        assert!(!status.snapshot_truncated);
        apply_message(
            actor_view(4, true),
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        apply_message(
            ServerMessage::SandboxSnapshot {
                tick: 5,
                view: full,
            },
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        assert!(
            status.snapshot_truncated,
            "a complete economy view must not erase a partial pose view"
        );
        apply_message(
            actor_view(6, false),
            &mut connection,
            &mut status,
            &mut runtime,
            &mut sandbox,
        )
        .unwrap();
        assert!(!status.snapshot_truncated);
    }

    #[test]
    fn base_world_checks_pin_geometry_epoch_and_political_revision() {
        let mut config = WorldConfig::default();
        let mut manifest = BaseWorldManifest::default();
        assert!(compatible_manifest(&manifest, &config).is_ok());
        manifest.chunk_level += 1;
        assert!(compatible_manifest(&manifest, &config).is_err());
        manifest = BaseWorldManifest::default();
        manifest.terrain_resolution = 17;
        assert!(compatible_manifest(&manifest, &config).is_err());
        manifest = BaseWorldManifest::default();
        manifest.data_epoch_unix = Some(946684800);
        assert!(compatible_manifest(&manifest, &config).is_err());
        config.historical_pack = Some("era-b/manifest.json".into());
        config.data_epoch_unix = manifest.data_epoch_unix;
        config.base_data_revision = "era-b:sha256:123".into();
        manifest.base_data_revision = config.base_data_revision.clone();
        manifest.terrain_provider = "historical-pack".into();
        assert!(compatible_manifest(&manifest, &config).is_ok());
        manifest.political_dataset_revision = Some("politics-v1".into());
        assert!(compatible_manifest(&manifest, &config).is_err());
        config.political_dataset_revision = manifest.political_dataset_revision.clone();
        assert!(compatible_manifest(&manifest, &config).is_ok());
        config.live_data = true;
        assert!(compatible_manifest(&manifest, &config).is_err());
    }

    #[test]
    fn replica_view_is_owned_and_checks_faction_rosters() {
        let mut state = SandboxState::default();
        state
            .add_player(1, GeoPosition::new(48.137, 11.575, 0.0).unwrap())
            .unwrap();
        state
            .apply(
                1,
                terra_sandbox::Command::CreateFaction {
                    name: "Builders".into(),
                },
                &terra_sandbox::Catalog::builtin(),
            )
            .unwrap();
        let mut view = state.snapshot_for(1, 5000.0).unwrap();
        assert!(valid_view(&view, Some(1)));
        assert!(!valid_view(&view, Some(2)));
        view.factions[0].members.clear();
        assert!(!valid_view(&view, Some(1)));
    }
}
