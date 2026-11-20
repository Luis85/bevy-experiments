//! Finite actual TCP/TLS sessions, living owners, regional writes and churn.
//! Private fixture saves/capabilities stay in a temporary directory; JSON output
//! contains workload, counters, timing and checks, never identity capabilities.
#[cfg(all(feature = "tls", feature = "reference-server"))]
#[path = "support/prepared_session_surface.rs"]
mod prepared_session_surface;

#[cfg(not(all(feature = "tls", feature = "reference-server")))]
fn main() {
    eprintln!(
        "regional_session_load requires tls and reference-server features (enabled by default)"
    );
    std::process::exit(2);
}
#[cfg(all(feature = "tls", feature = "reference-server"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    run::main()
}

#[cfg(all(feature = "tls", feature = "reference-server"))]
mod run {
    use crate::prepared_session_surface::PreparedSessionWorld;
    use serde_json::{Value, json};
    use std::{
        collections::{BTreeMap, BTreeSet},
        error::Error,
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    use terra_core::{GeoPosition, LocalFrame, TileKey};
    use terra_net::{
        ClientMessage, ClientTlsConfig, NetClient, ReplicationStats, Server, ServerConfig,
        ServerMessage, ServerReport, ServerTlsConfig,
    };
    use terra_sandbox::{Catalog, Command, SandboxState, UnitOrder};
    type Result<T> = std::result::Result<T, Box<dyn Error>>;
    const CA: &[u8] = include_bytes!("../tests/fixtures/tls/ca.pem");
    const CERT: &[u8] = include_bytes!("../tests/fixtures/tls/server.pem");
    const KEY: &[u8] = include_bytes!("../tests/fixtures/tls/fixture-key.pem");

    struct Bot {
        id: u64,
        token: Option<String>,
        client: Option<NetClient>,
        next_command: u64,
        command_ack: u64,
        next_move: u64,
        welcomes: usize,
        snapshots: usize,
        motion_states: usize,
        focus: GeoPosition,
        pending_pings: BTreeMap<u64, Instant>,
        pending_command: Option<(u64, Instant)>,
    }
    impl Bot {
        fn new(focus: GeoPosition) -> Self {
            Self {
                id: 0,
                token: None,
                client: None,
                next_command: 1,
                command_ack: 0,
                next_move: 1,
                welcomes: 0,
                snapshots: 0,
                motion_states: 0,
                focus,
                pending_pings: BTreeMap::new(),
                pending_command: None,
            }
        }
    }
    #[derive(Default)]
    struct Measurements {
        updates: Vec<f64>,
        advancing: Vec<f64>,
        storage_wait: Vec<f64>,
        paused_updates: usize,
        fixed_steps: usize,
        expected_storage_failures: usize,
        received: u64,
        sent: u64,
        replication: ReplicationStats,
        memory: Vec<Value>,
        phases: Vec<Value>,
        ping_roundtrip: Vec<f64>,
        durable_command_ack: Vec<f64>,
    }
    impl Measurements {
        fn disconnect(&mut self, bot: &mut Bot) {
            bot.pending_pings.clear();
            if let Some(client) = bot.client.take() {
                self.received += client.transport_stats().bytes_received;
                self.sent += client.transport_stats().bytes_sent;
            }
        }
        fn memory(&mut self, phase: &str) {
            let kib = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|text| {
                    text.lines()
                        .find(|line| line.starts_with("VmRSS:"))
                        .and_then(|line| line.split_whitespace().nth(1))
                        .and_then(|v| v.parse::<u64>().ok())
                });
            self.memory
                .push(json!({"phase":phase,"process_rss_kib":kib}));
        }
    }
    fn host(tls: bool, clients: usize, prepared: Option<&PreparedSessionWorld>) -> Result<Server> {
        let mut config = ServerConfig {
            bind_addr: "127.0.0.1:0".parse()?,
            max_clients: clients + 2,
            ..Default::default()
        };
        if let Some(world) = prepared {
            world.configure(&mut config);
        }
        let mut server = if tls {
            Server::bind_tls(config, ServerTlsConfig::from_pem(CERT, KEY)?)?
        } else {
            Server::bind(config)?
        };
        if let Some(world) = prepared {
            world.install(&mut server)?;
        }
        Ok(server)
    }
    fn connect(server: &Server, bot: &mut Bot, tls: bool) -> Result<()> {
        let client = if tls {
            NetClient::connect_tls(
                server.local_addr()?,
                bot.focus,
                bot.token.clone(),
                Some(server.config().manifest.clone()),
                ClientTlsConfig::from_ca_pem(CA, "localhost")?,
                Duration::from_secs(1),
            )?
        } else {
            NetClient::connect_with_manifest(
                server.local_addr()?,
                bot.focus,
                bot.token.clone(),
                server.config().manifest.clone(),
            )?
        };
        bot.client = Some(client);
        bot.next_move = 1;
        Ok(())
    }
    fn classify_update_failure(report: &ServerReport, expected_failure: bool) -> Result<bool> {
        // The host includes a failed storage completion in domain_errors. The
        // injected writer lock is the sole allowed error, at an idle barrier
        // before any input, simulation or snapshot work. An unrelated error,
        // a second error, or an advancing update must never be exempted.
        let expected_lock = expected_failure
            && report.persistence_error.as_deref() == Some("regional storage: database is locked")
            && report.domain_errors == 1
            && report.fixed_steps == 0
            && report.messages == 0
            && report.snapshots_sent == 0;
        if expected_lock {
            return Ok(true);
        }
        if report.persistence_error.is_some() || report.domain_errors != 0 {
            return Err(format!("unexpected authority error: {report:?}").into());
        }
        Ok(false)
    }

    #[test]
    fn expected_writer_lock_cannot_hide_another_error_or_authority_work() {
        let mut report = ServerReport {
            persistence_error: Some("regional storage: database is locked".into()),
            domain_errors: 1,
            ..Default::default()
        };
        assert!(classify_update_failure(&report, true).unwrap());
        assert!(classify_update_failure(&report, false).is_err());
        report.domain_errors = 2;
        assert!(classify_update_failure(&report, true).is_err());
        report.domain_errors = 1;
        report.fixed_steps = 1;
        assert!(classify_update_failure(&report, true).is_err());
        report.fixed_steps = 0;
        report.messages = 1;
        assert!(classify_update_failure(&report, true).is_err());
        report.messages = 0;
        report.snapshots_sent = 1;
        assert!(classify_update_failure(&report, true).is_err());
        report.snapshots_sent = 0;
        report.persistence_error = Some("regional storage: corrupt checkpoint".into());
        assert!(classify_update_failure(&report, true).is_err());
        assert!(!classify_update_failure(&ServerReport::default(), true).unwrap());
    }

    fn update(
        server: &mut Server,
        bots: &mut [Bot],
        metrics: &mut Measurements,
        delta: f64,
        expected_failure: bool,
    ) -> Result<bool> {
        let started = Instant::now();
        let report = server.update(delta);
        let ms = started.elapsed().as_secs_f64() * 1000.;
        metrics.updates.push(ms);
        metrics.fixed_steps += report.fixed_steps;
        metrics.replication.accumulate(&report.replication);
        if report.fixed_steps > 0 {
            metrics.advancing.push(ms);
        }
        if report.persistence_pending {
            metrics.paused_updates += 1;
            metrics.storage_wait.push(ms);
        }
        let failed = classify_update_failure(&report, expected_failure)?;
        if failed {
            metrics.expected_storage_failures += 1;
        }
        for bot in bots {
            if let Some(client) = &mut bot.client {
                for message in client.poll()? {
                    match message {
                        ServerMessage::Welcome {
                            actor_id,
                            resume_token,
                            next_sandbox_sequence,
                            ..
                        } => {
                            if bot.id != 0
                                && (bot.id != actor_id || bot.token.as_ref() != Some(&resume_token))
                            {
                                return Err(
                                    "resumed principal identity or capability changed".into()
                                );
                            }
                            if bot.welcomes > 0 && next_sandbox_sequence != bot.next_command {
                                return Err(
                                    "authenticated resume changed the durable command sequence"
                                        .into(),
                                );
                            }
                            bot.id = actor_id;
                            bot.token = Some(resume_token);
                            bot.next_command = next_sandbox_sequence;
                            bot.welcomes += 1;
                        }
                        ServerMessage::Snapshot { .. } => bot.snapshots += 1,
                        ServerMessage::MotionState { .. } => bot.motion_states += 1,
                        ServerMessage::SandboxResult { sequence, .. } => {
                            bot.command_ack = sequence;
                            if let Some((expected, started)) = bot.pending_command.take() {
                                if sequence != expected {
                                    return Err("unexpected durable command acknowledgement".into());
                                }
                                metrics
                                    .durable_command_ack
                                    .push(started.elapsed().as_secs_f64() * 1000.);
                            }
                        }
                        ServerMessage::Pong { nonce, .. } => {
                            if let Some(started) = bot.pending_pings.remove(&nonce) {
                                metrics
                                    .ping_roundtrip
                                    .push(started.elapsed().as_secs_f64() * 1000.);
                            }
                        }
                        ServerMessage::SandboxRejected { sequence, message } => {
                            return Err(format!("command {sequence} rejected: {message}").into());
                        }
                        ServerMessage::Error { code, message } => {
                            return Err(format!("network {code}: {message}").into());
                        }
                        _ => {}
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
        Ok(failed)
    }
    fn until(
        server: &mut Server,
        bots: &mut [Bot],
        metrics: &mut Measurements,
        predicate: impl Fn(&Server, &[Bot]) -> bool,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            update(server, bots, metrics, 0., false)?;
            if predicate(server, bots) {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("regional session barrier timed out".into());
            }
        }
    }
    fn snapshot(server: &Server, path: &Path) -> Result<SandboxState> {
        server.save_world(path)?;
        let envelope: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(serde_json::from_value(envelope["sandbox"].clone())?)
    }
    fn owned(world: &SandboxState, id: u64) -> Value {
        json!({"player":world.players[&id],"account":world.living.accounts[&id],
        "bodies":world.living.units.values().filter(|u|u.owner==Some(id)).collect::<Vec<_>>(),
        "vehicles":world.transport.vehicles.values().filter(|v|v.owner==id).collect::<Vec<_>>()})
    }
    fn ticks(
        server: &mut Server,
        bots: &mut [Bot],
        metrics: &mut Measurements,
        count: usize,
        phase: &str,
    ) -> Result<()> {
        let start_tick = server.tick();
        let target = start_tick + count as u64;
        let start = Instant::now();
        let first_update = metrics.updates.len();
        let first_bytes = metrics.replication.clone();
        let mut last_intent = None;
        let mut last_checkpoint = None;
        while server.tick() < target {
            let tick = server.tick();
            if last_intent.is_none_or(|old| tick >= old + 10) {
                for bot in bots.iter_mut() {
                    if let Some(client) = &mut bot.client {
                        client.send(ClientMessage::Move {
                            sequence: bot.next_move,
                            velocity_enu_m_s: [0.25, 0., 0.],
                        })?;
                        bot.next_move += 1;
                        if bot.pending_pings.len() >= 128 {
                            return Err("bounded ping acknowledgement window exhausted".into());
                        }
                        bot.pending_pings.insert(tick, Instant::now());
                        client.send(ClientMessage::Ping { nonce: tick })?;
                    }
                }
                last_intent = Some(tick);
            }
            if tick > start_tick
                && (tick - start_tick).is_multiple_of(40)
                && last_checkpoint != Some(tick)
            {
                server.request_region_checkpoint()?;
                last_checkpoint = Some(tick);
            }
            update(server, bots, metrics, 0.05, false)?;
            if start.elapsed() > Duration::from_secs(45) {
                return Err("living bot phase exceeded bounded wall budget".into());
            }
        }
        until(server, bots, metrics, |server, _| {
            !server.persistence_pending()
        })?;
        metrics.phases.push(json!({"name":phase,"fixed_ticks":server.tick()-start_tick,
        "wall_seconds":start.elapsed().as_secs_f64(),"host_updates":metrics.updates.len()-first_update,
        "host_update_ms":times(&metrics.updates[first_update..]),"starting_replication":first_bytes,"ending_replication":metrics.replication,
        "resident":server.region_store_status()}));
        metrics.memory(phase);
        Ok(())
    }
    fn times(values: &[f64]) -> Value {
        if values.is_empty() {
            return json!({"samples":0});
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let percentile = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
        json!({"samples":sorted.len(),"mean_ms":sorted.iter().sum::<f64>()/sorted.len() as f64,
        "p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),"max_ms":sorted.last()})
    }

    pub fn main() -> Result<()> {
        let mut clients = 8usize;
        let mut cycles = 3usize;
        let mut phase_ticks = 120usize;
        let mut tls = true;
        let mut output = None::<PathBuf>;
        let mut heightmap = None::<PathBuf>;
        let mut cold_regions = 0usize;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            if matches!(argument.as_str(), "--help" | "-h") {
                println!(
                    "regional_session_load [--clients 2..16] [--cycles 2..8] [--ticks 40..400] [--cold-regions 0..2048] [--transport tls|tcp] [--heightmap prepared-manifest.json] [--output report.json]"
                );
                return Ok(());
            }
            let value = args.next().ok_or("option requires a value")?;
            match argument.as_str() {
            "--clients"=>clients=value.parse()?,"--cycles"=>cycles=value.parse()?,"--ticks"=>phase_ticks=value.parse()?,
            "--cold-regions"=>cold_regions=value.parse()?,
            "--transport"=>tls=match value.as_str(){"tls"=>true,"tcp"=>false,_=>return Err("transport must be tls or tcp".into())},
            "--heightmap"=>heightmap=Some(value.into()),"--output"=>output=Some(value.into()),_=>return Err("usage: regional_session_load --clients 2..16 --cycles 2..8 --ticks 40..400 --transport tls|tcp --output report.json".into()),
        }
        }
        if !(2..=16).contains(&clients)
            || !(2..=8).contains(&cycles)
            || !(40..=400).contains(&phase_ticks)
            || cold_regions > 2048
        {
            return Err("finite workload bounds exceeded".into());
        }
        let prepared = heightmap
            .as_deref()
            .map(PreparedSessionWorld::open)
            .transpose()?;
        let directory = tempfile::tempdir()?;
        let fixture = directory.path().join("private-fixture.json");
        let inspection = directory.path().join("private-inspection.json");
        let database = directory.path().join("world.db");
        let mut server = host(tls, clients, prepared.as_ref())?;
        server.enable_living_demo(&[])?;
        let spawn = server.config().spawn;
        let mut bots: Vec<_> = (0..clients).map(|_| Bot::new(spawn)).collect();
        let mut bootstrap = Measurements::default();
        for bot in &mut bots {
            connect(&server, bot, tls)?;
        }
        until(&mut server, &mut bots, &mut bootstrap, |_, bots| {
            bots.iter().all(|b| b.welcomes == 1)
        })?;
        for bot in &mut bots {
            bootstrap.disconnect(bot);
        }
        until(&mut server, &mut bots, &mut bootstrap, |server, bots| {
            bots.iter()
                .all(|b| server.sandbox().is_player_dormant(b.id))
        })?;
        server.save_world(&fixture)?;
        let mut document: Value = serde_json::from_slice(&std::fs::read(&fixture)?)?;
        let mut world: SandboxState = serde_json::from_value(document["sandbox"].clone())?;
        for (index, bot) in bots.iter_mut().enumerate() {
            let avatar = world.living.accounts[&bot.id].avatar_unit;
            let original = world.living.units[&avatar].position;
            // Imported terrain keeps the actual safely admitted crew positions.
            // The flat spread workload preserves each crew's relative layout;
            // it no longer collapses all five bodies to the same centre.
            let position = if prepared.is_some() {
                original
            } else {
                LocalFrame::new(spawn).to_geo([
                    if index < clients / 2 {
                        index as f64 * 100.
                    } else {
                        10_000. + index as f64 * 3_000.
                    },
                    0.,
                    800.,
                ])?
            };
            bot.focus = position;
            if prepared.is_none() {
                let old_frame = LocalFrame::new(original);
                let new_frame = LocalFrame::new(position);
                for body in world
                    .living
                    .units
                    .values_mut()
                    .filter(|unit| unit.owner == Some(bot.id))
                {
                    body.position = new_frame.to_geo(old_frame.to_local(body.position))?;
                    body.home = new_frame.to_geo(old_frame.to_local(body.home))?;
                    body.order = UnitOrder::Idle;
                    body.path.clear();
                    body.path_index = 0;
                    body.patrol.clear();
                }
                // Use the exact transformed active-body coordinate: geographic
                // local-frame round trips can differ from the requested anchor.
                let active = world.living.accounts[&bot.id].active_unit;
                let actual = world.living.units[&active].position;
                world.players.get_mut(&bot.id).unwrap().position = actual;
                bot.focus = actual;
            }
            let vehicle = world
                .transport
                .vehicles
                .values_mut()
                .find(|vehicle| vehicle.owner == bot.id)
                .ok_or("starter vehicle absent")?;
            vehicle.cargo = BTreeMap::from([("stone".into(), 7)]);
            for node in 0..64 {
                world.seed_resource(format!("churn-{index}-{node}"), position, "stone", 30, 0.)?;
            }
        }
        // Distant mutable overlays exercise regional recovery alongside actual
        // local sessions. They are deliberately outside the prepared source:
        // these records establish dormant storage, never wider source coverage.
        let mut cold_keys = BTreeSet::new();
        let mut cold_nodes = BTreeMap::new();
        for region in 0..cold_regions {
            let position = GeoPosition::new(
                if spawn.latitude >= 0. { -30. } else { 30. } + (region / 64) as f64 * 0.03,
                90. + (region % 64) as f64 * 0.04,
                100.,
            )?;
            cold_keys.insert(TileKey::at(position, server.config().manifest.chunk_level)?);
            for node in 0..4 {
                let id = format!("cold-session-{region}-{node}");
                world.seed_resource(id.clone(), position, "stone", 30, 0.)?;
                world.resource_nodes.get_mut(&id).unwrap().remaining = 7;
                cold_nodes.insert(id.clone(), world.resource_nodes[&id].clone());
            }
        }
        if cold_keys.len() != cold_regions {
            return Err("requested cold overlay regions are not distinct".into());
        }
        for unit in world
            .living
            .units
            .values()
            .filter(|unit| unit.owner.is_some())
        {
            if world.spawn_obstruction(unit)?.is_some() {
                return Err(format!(
                    "session fixture starts with an overlapping owned body {}",
                    unit.id
                )
                .into());
            }
        }
        world.validate_with_catalog(&Catalog::builtin())?;
        document["sandbox"] = serde_json::to_value(&world)?;
        std::fs::write(&fixture, serde_json::to_vec(&document)?)?;
        drop(server);
        let mut server = host(tls, clients, prepared.as_ref())?;
        server.load_world(&fixture)?;
        server.open_region_store(&database)?;
        server.checkpoint_regions()?;
        let initial_generation = server.region_store_status().unwrap().generation;
        if cold_nodes
            .keys()
            .any(|id| server.sandbox().resource_nodes.contains_key(id))
            || server.region_store_status().unwrap().dormant_regions < cold_regions
        {
            return Err("cold overlay fixture was not retired before measured sessions".into());
        }
        let mut measurements = Measurements::default();
        measurements.memory("before measured admission");
        let mut exact_offline_checks = 0usize;
        let mut exact_resume_checks = 0usize;
        let mut exact_restart_checks = 0usize;
        let mut source_recovery_checks = 0usize;
        let measured_start = Instant::now();
        for cycle in 0..cycles {
            let welcomes: Vec<_> = bots.iter().map(|b| b.welcomes).collect();
            for bot in &mut bots {
                connect(&server, bot, tls)?;
            }
            until(&mut server, &mut bots, &mut measurements, |server, bots| {
                !server.persistence_pending()
                    && bots.iter().zip(&welcomes).all(|(b, w)| b.welcomes == w + 1)
            })?;
            for bot in &mut bots {
                let client = bot.client.as_mut().unwrap();
                client.send(ClientMessage::Focus {
                    position: if cycle % 2 == 0 { spawn } else { bot.focus },
                    radius_m: 900.,
                })?;
                bot.pending_command = Some((bot.next_command, Instant::now()));
                client.send(ClientMessage::Sandbox {
                    sequence: bot.next_command,
                    command: Command::Craft {
                        recipe: "planks".into(),
                        count: 1,
                    },
                })?;
            }
            until(&mut server, &mut bots, &mut measurements, |server, bots| {
                !server.persistence_pending()
                    && bots.iter().all(|b| b.command_ack == b.next_command)
            })?;
            for bot in &mut bots {
                bot.next_command += 1;
            }
            if cycle == 0 {
                if let Some(prepared) = &prepared {
                    let avatar = server.sandbox().living.accounts[&bots[0].id].avatar_unit;
                    let position = server.sandbox().living.units[&avatar].position;
                    let key = TileKey::at(position, server.config().manifest.chunk_level)?;
                    let chunk = prepared
                        .chunks
                        .iter()
                        .find(|chunk| chunk.key == key)
                        .ok_or("active owner is outside prepared source fixture")?;
                    server.try_evict_chunk(key)?;
                    ticks(
                        &mut server,
                        &mut bots,
                        &mut measurements,
                        20,
                        "source tile unavailable: movement must remain blocked",
                    )?;
                    if server.sandbox().living.units[&avatar].position != position {
                        return Err("missing source terrain moved the controlled avatar".into());
                    }
                    server.try_register_chunk(chunk)?;
                    until(&mut server, &mut bots, &mut measurements, |server, _| {
                        !server.persistence_pending()
                    })?;
                    ticks(
                        &mut server,
                        &mut bots,
                        &mut measurements,
                        20,
                        "source tile restored: physical movement resumes",
                    )?;
                    if server.sandbox().living.units[&avatar]
                        .position
                        .surface_distance_m(position)
                        < 0.02
                    {
                        return Err(
                            "restored source terrain did not restore physical movement".into()
                        );
                    }
                    source_recovery_checks += 1;
                }
            }
            ticks(
                &mut server,
                &mut bots,
                &mut measurements,
                phase_ticks,
                &format!(
                    "cycle {cycle}: all connected {} interest",
                    if cycle % 2 == 0 {
                        "overlapping"
                    } else {
                        "spread"
                    }
                ),
            )?;
            for (index, bot) in bots.iter_mut().enumerate() {
                if index % 2 == cycle % 2 {
                    measurements.disconnect(bot);
                }
            }
            until(&mut server, &mut bots, &mut measurements, |server, bots| {
                !server.persistence_pending()
                    && bots
                        .iter()
                        .filter(|b| b.client.is_none())
                        .all(|b| server.sandbox().is_player_dormant(b.id))
            })?;
            let before = snapshot(&server, &inspection)?;
            let sleeping: Vec<_> = bots
                .iter()
                .filter(|b| b.client.is_none())
                .map(|b| b.id)
                .collect();
            ticks(
                &mut server,
                &mut bots,
                &mut measurements,
                phase_ticks / 2,
                &format!("cycle {cycle}: half disconnected"),
            )?;
            let after = snapshot(&server, &inspection)?;
            for id in &sleeping {
                if owned(&before, *id) != owned(&after, *id) {
                    return Err(
                        "offline principal bodies, wallet, inventory or cargo advanced".into(),
                    );
                }
                exact_offline_checks += 1;
            }
            let welcomes: Vec<_> = bots.iter().map(|b| b.welcomes).collect();
            for bot in bots.iter_mut().filter(|b| b.client.is_none()) {
                connect(&server, bot, tls)?;
            }
            until(&mut server, &mut bots, &mut measurements, |server, bots| {
                !server.persistence_pending()
                    && bots
                        .iter()
                        .zip(&welcomes)
                        .all(|(b, w)| b.welcomes == *w + usize::from(sleeping.contains(&b.id)))
            })?;
            let resumed = snapshot(&server, &inspection)?;
            for id in &sleeping {
                if owned(&after, *id) != owned(&resumed, *id) {
                    return Err(
                        "authenticated resume changed private owned state before a tick".into(),
                    );
                }
                exact_resume_checks += 1;
            }
            if cycle == 0 {
                let original = server.sandbox().clone();
                let tick = server.tick();
                let generation = server.region_store_status().unwrap().generation;
                let lock = rusqlite::Connection::open(&database)?;
                lock.execute_batch("BEGIN IMMEDIATE")?;
                server.request_region_checkpoint()?;
                let deadline = Instant::now() + Duration::from_secs(5);
                while !update(&mut server, &mut bots, &mut measurements, 0.05, true)? {
                    if Instant::now() > deadline {
                        return Err("expected storage lock failure was not observed".into());
                    }
                }
                if server.tick() != tick
                    || *server.sandbox() != original
                    || server.region_store_status().unwrap().generation != generation
                {
                    return Err("failed checkpoint changed authority revision".into());
                }
                lock.execute_batch("ROLLBACK")?;
                server.checkpoint_regions()?;
                if *server.sandbox() != original {
                    return Err("checkpoint retry changed active logical state".into());
                }
            }
            for bot in &mut bots {
                measurements.disconnect(bot);
            }
            until(&mut server, &mut bots, &mut measurements, |server, bots| {
                !server.persistence_pending()
                    && bots
                        .iter()
                        .all(|b| server.sandbox().is_player_dormant(b.id))
            })?;
            server.checkpoint_regions()?;
            let before = snapshot(&server, &inspection)?;
            let tick = server.tick();
            let clock = server.clock().unix_seconds;
            drop(server);
            server = host(tls, clients, prepared.as_ref())?;
            server.open_region_store(&database)?;
            if server.tick() != tick || server.clock().unix_seconds != clock {
                return Err("restart clock/tick differs".into());
            }
            let after = snapshot(&server, &inspection)?;
            for bot in &bots {
                if owned(&before, bot.id) != owned(&after, bot.id) {
                    return Err("restart changed private owned state".into());
                }
                exact_restart_checks += 1;
            }
            server.checkpoint_regions()?;
            measurements.memory(&format!("cycle {cycle}: after disconnect and restart"));
        }
        server.checkpoint_regions()?;
        let before_recovery = snapshot(&server, &inspection)?;
        let private_before: Value = serde_json::from_slice(&std::fs::read(&inspection)?)?;
        let backup = directory.path().join("verified-backup.db");
        let recovered = directory.path().join("recovered-world.db");
        let backup_generation = server.region_store_status().unwrap().generation;
        server.backup_region_store(&backup)?;
        let backup_bytes = std::fs::read(&backup)?;
        let recovery_receipt = terra_persistence::RegionStore::open_read_only(&backup)?
            .backup_with_options(
                &recovered,
                terra_persistence::RegionBackupOptions::default(),
            )?;
        if recovery_receipt.generation != backup_generation
            || std::fs::read(&backup)? != backup_bytes
        {
            return Err("backup recovery changed source bytes or committed generation".into());
        }
        drop(server);
        server = host(tls, clients, prepared.as_ref())?;
        server.open_region_store(&recovered)?;
        let restored = snapshot(&server, &inspection)?;
        let private_after: Value = serde_json::from_slice(&std::fs::read(&inspection)?)?;
        if restored != before_recovery || private_before != private_after {
            return Err(
                "host-validated backup recovery changed full state or private metadata".into(),
            );
        }
        let welcomes: Vec<_> = bots.iter().map(|bot| bot.welcomes).collect();
        for bot in &mut bots {
            connect(&server, bot, tls)?;
        }
        until(&mut server, &mut bots, &mut measurements, |server, bots| {
            !server.persistence_pending()
                && bots
                    .iter()
                    .zip(&welcomes)
                    .all(|(bot, count)| bot.welcomes == count + 1)
        })?;
        let recovered_resumed = snapshot(&server, &inspection)?;
        for bot in &bots {
            if owned(&before_recovery, bot.id) != owned(&recovered_resumed, bot.id) {
                return Err(
                    "authenticated resume from recovered backup changed owned state".into(),
                );
            }
        }
        for bot in &mut bots {
            measurements.disconnect(bot);
        }
        until(&mut server, &mut bots, &mut measurements, |server, bots| {
            !server.persistence_pending()
                && bots
                    .iter()
                    .all(|bot| server.sandbox().is_player_dormant(bot.id))
        })?;
        server.checkpoint_regions()?;
        measurements.memory("after backup recovery and authenticated resume");
        let final_world = snapshot(&server, &inspection)?;
        if cold_nodes
            .iter()
            .any(|(id, node)| final_world.resource_nodes.get(id) != Some(node))
        {
            return Err("cold overlay records changed across session churn or recovery".into());
        }
        for bot in &bots {
            let held: u32 = final_world
                .transport
                .vehicles
                .values()
                .filter(|v| v.owner == bot.id)
                .map(|v| v.cargo.get("stone").copied().unwrap_or(0))
                .sum();
            if held != 7 {
                return Err("held cargo changed across churn".into());
            }
            if final_world.players[&bot.id]
                .inventory
                .get("planks")
                .copied()
                .unwrap_or(0)
                != 4 * cycles as u32
            {
                return Err("durable craft sequence or inventory differs".into());
            }
            if bot.snapshots == 0 {
                return Err("connected bot never received an actor snapshot".into());
            }
        }
        if measurements.durable_command_ack.len() != clients * cycles
            || measurements.ping_roundtrip.is_empty()
        {
            return Err("session workload did not collect every durable command acknowledgement and live pings".into());
        }
        let report = json!({"release":env!("CARGO_PKG_VERSION"),"wire_protocol":terra_net::PROTOCOL_VERSION,
        "transport":if tls{"trusted CA + localhost verification, TLS"}else{"plaintext loopback TCP"},
        "workload":{"real_socket_clients":clients,"churn_cycles":cycles,"active_ticks_per_cycle":phase_ticks,"offline_ticks_per_cycle":phase_ticks/2,
            "full_living_bodies":final_world.living.units.len(),"owned_bodies":final_world.living.units.values().filter(|u|u.owner.is_some()).count(),
            "resource_nodes":final_world.resource_nodes.len(),"cold_overlay_regions":cold_regions,"cold_overlay_resources":cold_nodes.len(),"flat_authored_surface":prepared.is_none(),"source_ingestion":prepared.is_some(),"prepared_source_revision":prepared.as_ref().map(|source|source.revision.as_str()),"resident_source_chunks":prepared.as_ref().map_or(0,|source|source.chunks.len()),"initial_owned_placement_checked":true,"renderer":false,"generic_actor_records":0},
        "environment":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"parallelism":std::thread::available_parallelism().map_or(1,usize::from),"debug_assertions":cfg!(debug_assertions),"sqlite":rusqlite::version()},
        "wall_seconds":measured_start.elapsed().as_secs_f64(),"host_update_ms":times(&measurements.updates),"advancing_update_ms":times(&measurements.advancing),
        "storage_wait_update_ms":times(&measurements.storage_wait),"ping_roundtrip_ms":times(&measurements.ping_roundtrip),"durable_command_ack_ms":times(&measurements.durable_command_ack),"authority_paused_updates":measurements.paused_updates,"fixed_steps":measurements.fixed_steps,
        "replication":measurements.replication,"actual_socket_bytes_received":measurements.received,"actual_socket_bytes_sent":measurements.sent,
        "per_client_actor_snapshots":bots.iter().map(|b|b.snapshots).collect::<Vec<_>>(),"per_client_motion_states":bots.iter().map(|b|b.motion_states).collect::<Vec<_>>(),
        "initial_generation":initial_generation,"backup_generation":backup_generation,"recovery_receipt":recovery_receipt,"final_store":server.region_store_status(),"memory_samples":measurements.memory,"phases":measurements.phases,
        "verification":{"exact_offline_owner_checks":exact_offline_checks,"exact_resume_owner_checks":exact_resume_checks,"exact_restart_owner_checks":exact_restart_checks,
            "expected_storage_failures":measurements.expected_storage_failures,"failure_atomic_and_retry_exact":true,"cargo_conserved":true,"craft_inventory_and_sequences_exact":true,"all_real_sessions_received_snapshots":true,"source_unavailable_and_restored_checks":source_recovery_checks,"backup_full_state_and_private_metadata_exact":true,"backup_generation_exact":true,"backup_source_bytes_unchanged":true,"authenticated_recovered_owner_checks":clients,"durable_resume_sequences_exact":true,"cold_overlay_records_exact":true},
        "limitations":["finite accelerated shared-container workload, not concurrent-player capacity","no Internet delay/loss, GPU workload, live provider or distributed authority; imported terrain is the explicitly authored local fixture when selected",
            "RSS belongs to this process including host and client fixtures; finite samples do not establish a long-duration plateau","one writer and aggregate domain caps remain; storage pauses authority",
            "distant cold overlay fixture records are outside local prepared terrain and do not extend source coverage or simulate an inhabited world in every region",
            "temporary synthetic saves and public test TLS key are fixtures; they are not production identity enrollment or credentials"]});
        let json = serde_json::to_string_pretty(&report)?;
        if let Some(path) = output {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &json)?;
        }
        println!("{json}");
        Ok(())
    }
}
