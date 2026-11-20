//! Shared actual-socket fixture for tests and the inspectable CLI probe.
#[path = "conditioned_link.rs"]
mod conditioned_link;
use conditioned_link::{ConditionedLink, Conditions};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    error::Error,
    net::SocketAddr,
    thread,
    time::{Duration, Instant},
};
use terra_core::{ActorKind, ActorState, LocalFrame};
use terra_net::{ClientMessage, NetClient, Server, ServerConfig, ServerMessage};
use terra_sandbox::{Command, CompanyCommand, SandboxView, TransportCommand};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const NPCS: usize = 510;
#[cfg(feature = "tls")]
const CA: &[u8] = include_bytes!("../../tests/fixtures/tls/ca.pem");
#[cfg(feature = "tls")]
const CERT: &[u8] = include_bytes!("../../tests/fixtures/tls/server.pem");
#[cfg(feature = "tls")]
const KEY: &[u8] = include_bytes!("../../tests/fixtures/tls/fixture-key.pem");

fn check(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::other(message).into())
    }
}
fn host(encrypted: bool) -> Result<Server> {
    let config = ServerConfig {
        bind_addr: "127.0.0.1:0".parse()?,
        max_clients: 4,
        max_snapshot_actors: 512,
        snapshot_hz: 20,
        calendar_time_scale: 1.,
        ..Default::default()
    };
    #[cfg(feature = "tls")]
    if encrypted {
        return Ok(Server::bind_tls(
            config,
            terra_net::ServerTlsConfig::from_pem(CERT, KEY)?,
        )?);
    }
    check(!encrypted, "TLS support is not compiled")?;
    Ok(Server::bind(config)?)
}
fn connect(address: SocketAddr, encrypted: bool, token: Option<String>) -> Result<NetClient> {
    let spawn = ServerConfig::default().spawn;
    #[cfg(feature = "tls")]
    if encrypted {
        return Ok(NetClient::connect_tls(
            address,
            spawn,
            token,
            None,
            terra_net::ClientTlsConfig::from_ca_pem(CA, "localhost")?,
            Duration::from_secs(3),
        )?);
    }
    check(!encrypted, "TLS support is not compiled")?;
    Ok(NetClient::connect_with_token(address, spawn, token)?)
}

#[derive(Default)]
struct Observed {
    id: Option<u64>,
    token: Option<String>,
    next_sequence: Option<u64>,
    view: Option<SandboxView>,
    actor_tick: u64,
    actors: Vec<ActorState>,
    acknowledgements: BTreeSet<u64>,
    rejection_sequences: BTreeSet<u64>,
    pongs: usize,
}
impl Observed {
    fn poll(&mut self, client: &mut NetClient) -> Result<()> {
        // Four bounded polls also let the healthy reader drain the large actor
        // fixture while the other reader is deliberately stalled.
        for _ in 0..4 {
            for message in client.poll()? {
                match message {
                    ServerMessage::Welcome {
                        actor_id,
                        resume_token,
                        next_sandbox_sequence,
                        ..
                    } => {
                        check(self.id.is_none(), "duplicate welcome")?;
                        self.id = Some(actor_id);
                        self.token = Some(resume_token);
                        self.next_sequence = Some(next_sandbox_sequence);
                    }
                    ServerMessage::SandboxSnapshot { view, .. } => self.view = Some(view),
                    ServerMessage::Snapshot { tick, actors, .. } => {
                        self.actor_tick = tick;
                        self.actors = actors;
                    }
                    ServerMessage::SandboxResult { sequence, .. } => {
                        check(
                            self.acknowledgements.insert(sequence),
                            "duplicate command success",
                        )?;
                    }
                    ServerMessage::SandboxRejected { sequence, .. } => {
                        self.rejection_sequences.insert(sequence);
                    }
                    ServerMessage::Pong { .. } => self.pongs += 1,
                    ServerMessage::Error { code, .. } => {
                        return Err(std::io::Error::other(format!("host error: {code}")).into());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    fn ready(&self) -> bool {
        self.id.is_some() && self.view.is_some() && !self.actors.is_empty()
    }
}

fn view_matches(server: &Server, owner: u64, observed: Option<&SandboxView>) -> Result<bool> {
    let Some(observed) = observed else {
        return Ok(false);
    };
    let expected = server.sandbox().snapshot_for(owner, 5000.)?;
    Ok(serde_json::to_value(observed)? == serde_json::to_value(expected)?)
}

#[derive(Default)]
struct Measurements {
    updates: usize,
    snapshots_deferred: u64,
    command_batches_deferred: usize,
    peak_peer_queue_bytes: usize,
    steps: usize,
}
impl Measurements {
    fn update(&mut self, host: &mut Server, delta: f64) -> Result<terra_net::ServerReport> {
        let report = host.update(delta);
        check(report.domain_errors == 0, "host domain error")?;
        check(
            report.disconnected == 0,
            "a live peer was disconnected under bounded backpressure",
        )?;
        check(report.persistence_error.is_none(), "host persistence error")?;
        self.updates += 1;
        self.steps += report.fixed_steps;
        self.snapshots_deferred += report.replication.backpressure_deferrals;
        self.command_batches_deferred += report.deferred_command_batches;
        self.peak_peer_queue_bytes = self
            .peak_peer_queue_bytes
            .max(report.peak_peer_pending_bytes);
        check(
            report.max_peer_pending_bytes <= 2 * terra_net::MAX_FRAME_BYTES,
            "host output exceeded its queue cap",
        )?;
        Ok(report)
    }
}

/// One real slow connection, one healthy observer, private gameplay mutations,
/// exact coalesced convergence, abrupt disconnect, durable host restart/resume
/// and persisted command replay rejection. No fabricated packets or capacities.
pub fn run(encrypted: bool) -> Result<Value> {
    let started = Instant::now();
    let conditions = Conditions::default();
    let mut server = host(encrypted)?;
    server.enable_living_demo(&[])?;
    let frame = LocalFrame::new(server.config().spawn);
    for index in 0..NPCS {
        server.insert_actor(ActorState {
            id: 10_000 + index as u64,
            kind: ActorKind::Human,
            position: frame.to_geo([index as f64 * 0.5, 0., 60.])?,
            velocity_enu_m_s: [0.2, 0., 0.],
            heading_deg: 90.,
        })?;
    }
    let link = ConditionedLink::start(server.local_addr()?, conditions)?;
    let mut slow = connect(link.address, encrypted, None)?;
    let mut fast = connect(server.local_addr()?, encrypted, None)?;
    let (mut slow_state, mut fast_state) = (Observed::default(), Observed::default());
    let mut measurements = Measurements::default();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !slow_state.ready() || !fast_state.ready() {
        check(Instant::now() < deadline, "conditioned handshake timed out")?;
        measurements.update(&mut server, 0.01)?;
        slow_state.poll(&mut slow)?;
        fast_state.poll(&mut fast)?;
        thread::sleep(Duration::from_millis(2));
    }
    check(
        slow.is_encrypted() == encrypted && fast.is_encrypted() == encrypted,
        "transport identity differs",
    )?;
    let slow_id = slow_state.id.unwrap();
    let fast_id = fast_state.id.unwrap();
    for (sequence, command) in [
        (
            1,
            Command::Craft {
                recipe: "planks".into(),
                count: 1,
            },
        ),
        (
            2,
            Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::Create {
                        name: "Private fixture freight".into(),
                    },
                },
            },
        ),
        (
            3,
            Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::Deposit { amount: 5000 },
                },
            },
        ),
    ] {
        slow.send(ClientMessage::Sandbox { sequence, command })?;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while slow_state.acknowledgements.len() < 3 {
        check(Instant::now() < deadline, "conditioned commands timed out")?;
        measurements.update(&mut server, 0.01)?;
        slow_state.poll(&mut slow)?;
        fast_state.poll(&mut fast)?;
        thread::sleep(Duration::from_millis(2));
    }
    check(
        slow_state.rejection_sequences.is_empty(),
        "initial command rejected",
    )?;
    link.pause_downstream(true);
    let healthy_before = fast_state.pongs;
    let deferred_before = measurements.snapshots_deferred;
    let mut sent_under_pressure = false;
    let mut pressured_updates = 0;
    let mut paused_host_pending_bytes = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    for index in 0..600 {
        check(Instant::now() < deadline, "backpressure fixture timed out")?;
        if index % 10 == 0 {
            slow.send(ClientMessage::Ping { nonce: index })?;
            fast.send(ClientMessage::Ping { nonce: index })?;
        }
        let report = measurements.update(&mut server, 0.05)?;
        paused_host_pending_bytes = report.max_peer_pending_bytes;
        fast_state.poll(&mut fast)?;
        if measurements.snapshots_deferred > deferred_before {
            if !sent_under_pressure {
                slow.send(ClientMessage::Sandbox {
                    sequence: 4,
                    command: Command::Craft {
                        recipe: "planks".into(),
                        count: 1,
                    },
                })?;
                sent_under_pressure = true;
            }
            pressured_updates += 1;
            if pressured_updates >= 40 && report.max_peer_pending_bytes > 0 {
                break;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    check(
        sent_under_pressure && measurements.snapshots_deferred > deferred_before,
        "fixture never exercised snapshot deferral",
    )?;
    check(
        fast_state.pongs > healthy_before + 2,
        "healthy peer was starved by the slow reader",
    )?;
    check(
        server
            .principal_status(slow_id)
            .is_some_and(|status| status.connected),
        "backlogged owner was lost",
    )?;
    let paused_stats = link.stats();
    let healthy_pongs_during_stall = fast_state.pongs - healthy_before;
    link.pause_downstream(false);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !slow_state.acknowledgements.contains(&4)
        || !view_matches(&server, slow_id, slow_state.view.as_ref())?
    {
        check(
            Instant::now() < deadline,
            "coalesced view failed to converge after draining",
        )?;
        measurements.update(&mut server, 0.)?;
        slow_state.poll(&mut slow)?;
        fast_state.poll(&mut fast)?;
        thread::sleep(Duration::from_millis(2));
    }
    // One complete four-tick sandbox interval publishes current actor and
    // sandbox views after the old queue is drained. Then freeze the fixture.
    for _ in 0..4 {
        measurements.update(&mut server, 0.05)?;
        slow_state.poll(&mut slow)?;
        fast_state.poll(&mut fast)?;
    }
    let expected_tick = server.tick();
    let deadline = Instant::now() + Duration::from_secs(10);
    while slow_state.actor_tick < expected_tick || fast_state.actor_tick < expected_tick {
        check(
            Instant::now() < deadline,
            "latest actor views did not drain",
        )?;
        measurements.update(&mut server, 0.)?;
        slow_state.poll(&mut slow)?;
        fast_state.poll(&mut fast)?;
        thread::sleep(Duration::from_millis(2));
    }
    let drained_host_pending_bytes = measurements.update(&mut server, 0.)?.max_peer_pending_bytes;
    check(
        drained_host_pending_bytes == 0,
        "host application queue did not drain",
    )?;
    check(
        slow.replication_stats().resync_requests == 0
            && fast.replication_stats().resync_requests == 0,
        "backpressure broke an already-queued revision chain",
    )?;
    check(
        slow_state.actors.len() == NPCS + 2,
        "conditioned actor membership changed",
    )?;
    let actor_map = |actors: &[ActorState]| {
        actors
            .iter()
            .map(|actor| (actor.id, actor.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    check(
        actor_map(&slow_state.actors) == actor_map(&fast_state.actors),
        "healthy and conditioned actor states differ",
    )?;
    let owner = slow_state.view.as_ref().unwrap();
    let observer = fast_state.view.as_ref().unwrap();
    check(
        owner.player.id == slow_id && observer.player.id == fast_id,
        "recipient identity mixed",
    )?;
    check(
        owner
            .transport
            .company
            .as_ref()
            .is_some_and(|company| company.cash == 5000),
        "private company did not converge",
    )?;
    check(
        observer.transport.company.is_none(),
        "private company leaked to other owner",
    )?;
    check(
        owner.player.inventory.get("wood") == Some(&26)
            && owner.player.inventory.get("planks") == Some(&8),
        "commands failed inventory conservation",
    )?;
    let inventory = server.sandbox().players[&slow_id].inventory.clone();
    let account_money = server.sandbox().living.accounts[&slow_id].money;
    let company = server.sandbox().transport.companies[&slow_id].clone();
    let token = slow_state.token.clone().unwrap();
    let socket_stats = slow.transport_stats();
    let final_link_stats = link.stats();
    check(
        final_link_stats.peak_downstream_buffer_bytes <= 128 * 1024
            && final_link_stats.peak_upstream_buffer_bytes <= 128 * 1024,
        "condition relay exceeded buffer cap",
    )?;
    check(
        final_link_stats.downstream_write_fragments > 100 && final_link_stats.error.is_none(),
        "condition relay did not complete cleanly",
    )?;
    drop(slow);
    drop(link);
    drop(fast);
    let deadline = Instant::now() + Duration::from_secs(5);
    while server
        .principal_status(slow_id)
        .is_some_and(|status| status.connected)
        || server
            .principal_status(fast_id)
            .is_some_and(|status| status.connected)
    {
        check(
            Instant::now() < deadline,
            "disconnect did not freeze owners",
        )?;
        server.update(0.);
        thread::sleep(Duration::from_millis(2));
    }
    check(
        server.sandbox().players[&slow_id].inventory == inventory,
        "disconnect changed inventory",
    )?;
    check(
        server.sandbox().transport.companies[&slow_id] == company,
        "disconnect changed company",
    )?;
    let directory = tempfile::tempdir()?;
    let save = directory.path().join("private-host.json");
    server.save_world(&save)?;
    drop(server);
    let mut server = host(encrypted)?;
    server.load_world(&save)?;
    let resumed_link = ConditionedLink::start(server.local_addr()?, conditions)?;
    let mut resumed = connect(resumed_link.address, encrypted, Some(token))?;
    let mut resumed_state = Observed::default();
    let deadline = Instant::now() + Duration::from_secs(15);
    while resumed_state.id.is_none() || resumed_state.view.is_none() {
        check(Instant::now() < deadline, "conditioned resume timed out")?;
        measurements.update(&mut server, 0.)?;
        resumed_state.poll(&mut resumed)?;
        thread::sleep(Duration::from_millis(2));
    }
    check(
        resumed_state.id == Some(slow_id) && resumed_state.next_sequence == Some(5),
        "restart lost principal or accepted sequence",
    )?;
    check(
        server.sandbox().players[&slow_id].inventory == inventory
            && server.sandbox().living.accounts[&slow_id].money == account_money
            && server.sandbox().transport.companies[&slow_id] == company,
        "restart changed private ownership",
    )?;
    resumed.send(ClientMessage::Sandbox {
        sequence: 4,
        command: Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
    })?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !resumed_state.rejection_sequences.contains(&4) {
        check(
            Instant::now() < deadline,
            "persisted replay was not rejected",
        )?;
        measurements.update(&mut server, 0.)?;
        resumed_state.poll(&mut resumed)?;
        thread::sleep(Duration::from_millis(2));
    }
    check(
        server.sandbox().players[&slow_id].inventory == inventory,
        "replayed craft changed inventory",
    )?;
    Ok(json!({
        "package_version": env!("CARGO_PKG_VERSION"), "wire_protocol": terra_net::PROTOCOL_VERSION,
        "status": "passed", "transport": if encrypted { "tls" } else { "tcp" },
        "workload": {"actual_clients":2, "principal_actor_records":NPCS+2,
            "one_way_latency_ms":conditions.latency_ms, "ordered_jitter_ms":conditions.jitter_ms,
            "maximum_relay_write_fragment_bytes":conditions.fragment_bytes,
            "relay_direction_buffer_cap_bytes":128*1024, "host_peer_queue_cap_bytes":2*terra_net::MAX_FRAME_BYTES,
            "host_maximum_frame_body_bytes":terra_net::MAX_FRAME_BYTES,
            "host_optional_view_reliable_reserve_bytes":64*1024},
        "host": {"updates":measurements.updates,"fixed_steps":measurements.steps,
            "snapshot_backpressure_deferrals":measurements.snapshots_deferred,
            "reliable_command_batches_deferred":measurements.command_batches_deferred,
            "peak_peer_queue_bytes":measurements.peak_peer_queue_bytes,
            "paused_pending_bytes":paused_host_pending_bytes,
            "drained_pending_bytes":drained_host_pending_bytes,
            "healthy_pongs_during_stall":healthy_pongs_during_stall},
        "paused_link":paused_stats,"drained_link":final_link_stats,"conditioned_client_transport":socket_stats,
        "elapsed_wall_seconds":started.elapsed().as_secs_f64(),
        "checks":{"slow_peer_retained":true,"healthy_peer_served":true,
            "deferred_view_converged_exactly":true,"private_company_not_disclosed":true,
            "commands_acknowledged_once":true,"finite_inventory_preserved":true,
            "queued_baselines_preserved_without_resync":true,
            "resume_identity_and_next_sequence_exact":true,"restart_private_state_exact":true,
            "persisted_command_replay_rejected":true,"buffers_bounded":true},
        "scope":"Finite actual TCP/TLS byte relay with scheduled delay/jitter, fragmented writes, downstream pause, drain and restart. TCP packet loss/congestion behavior, Internet latency distribution, capacity and long-duration performance are not certified. No private capabilities or saves are emitted."
    }))
}
