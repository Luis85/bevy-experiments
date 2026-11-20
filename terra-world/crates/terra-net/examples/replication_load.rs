//! Finite, actual-loopback replication workload. It does not claim Internet or
//! MMO capacity: kinematic actor records and bounded living bodies differ.
use std::{
    error::Error,
    path::PathBuf,
    time::{Duration, Instant},
};
use terra_core::{ActorKind, ActorState, LocalFrame};
use terra_net::{ClientMessage, NetClient, ReplicationStats, Server, ServerConfig, ServerMessage};

fn main() -> Result<(), Box<dyn Error>> {
    let mut clients_count = 8usize;
    let mut actors_count = 4096usize;
    let mut ticks = 200usize;
    let mut output = None::<PathBuf>;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or("missing argument value");
        match arg.as_str() {
            "--clients" => clients_count = value()?.parse()?,
            "--actors" => actors_count = value()?.parse()?,
            "--ticks" => ticks = value()?.parse()?,
            "--output" => output = Some(value()?.into()),
            "--help" => {
                println!(
                    "replication_load [--clients 1..32] [--actors 32..20000] [--ticks 40..2000] [--output report.json]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument {arg}").into()),
        }
    }
    if !(1..=32).contains(&clients_count)
        || !(32..=20000).contains(&actors_count)
        || !(40..=2000).contains(&ticks)
    {
        return Err("workload outside finite harness budgets".into());
    }
    let mut host = Server::bind(ServerConfig {
        bind_addr: "127.0.0.1:0".parse()?,
        max_clients: clients_count,
        max_actors: actors_count + clients_count,
        max_snapshot_actors: 128,
        ..ServerConfig::default()
    })?;
    host.enable_living_demo(&[])?;
    let spawn = host.config().spawn;
    let frame = LocalFrame::new(spawn);
    let regions: Vec<_> = (0..4)
        .map(|n| frame.to_geo([n as f64 * 20_000., 0., 0.]).unwrap())
        .collect();
    for index in 0..actors_count {
        let region = index % 4;
        let local = LocalFrame::new(regions[region]);
        let grid = index / 4;
        host.insert_actor(ActorState {
            id: 10000 + index as u64,
            kind: ActorKind::Vehicle,
            position: local.to_geo([(grid % 32) as f64 * 8., 0., (grid / 32) as f64 * 8.])?,
            velocity_enu_m_s: if index % 4 == 0 {
                [0.5, 0., 0.]
            } else {
                [0.; 3]
            },
            heading_deg: 90.,
        })?;
    }
    let address = host.local_addr()?;
    let mut clients: Vec<_> = (0..clients_count)
        .map(|_| NetClient::connect(address, spawn))
        .collect::<Result<_, _>>()?;
    let mut ready = vec![false; clients_count];
    let mut counts = vec![0usize; clients_count];
    let mut totals = ReplicationStats::default();
    let mut candidates = 0usize;
    let mut disconnected = 0usize;
    let mut errors = 0usize;
    let mut timings = Vec::with_capacity(ticks);
    let mut phase_stats = [ReplicationStats::default(), ReplicationStats::default()];
    let mut phase_timings = [Vec::<f64>::new(), Vec::<f64>::new()];
    let warmup = 20usize;
    let started = Instant::now();
    for step in 0..warmup + ticks {
        if step == warmup + ticks / 2 {
            // The second phase leaves the living settlement running but stops
            // generic records, distinguishing dynamic from stable replication.
            let stationary: Vec<_> = host
                .actors()
                .values()
                .filter(|a| a.id >= 10000)
                .cloned()
                .map(|mut a| {
                    a.velocity_enu_m_s = [0.; 3];
                    a
                })
                .collect();
            for actor in stationary {
                host.insert_actor(actor)?;
            }
        }
        if step % 20 == 0 {
            for c in &mut clients {
                c.send(ClientMessage::Ping { nonce: step as u64 })?;
            }
        }
        let start = Instant::now();
        let report = host.update(0.05);
        let ms = start.elapsed().as_secs_f64() * 1000.;
        if step >= warmup {
            timings.push(ms);
            let phase = usize::from(step >= warmup + ticks / 2);
            phase_stats[phase].accumulate(&report.replication);
            phase_timings[phase].push(ms);
        }
        totals.accumulate(&report.replication);
        candidates += report.interest_candidates_tested;
        disconnected += report.disconnected;
        errors += report.domain_errors;
        for (i, c) in clients.iter_mut().enumerate() {
            for message in c.poll()? {
                match message {
                    ServerMessage::Welcome { .. } => {
                        ready[i] = true;
                        c.send(ClientMessage::Focus {
                            position: regions[i % 4],
                            radius_m: 600.,
                        })?;
                    }
                    ServerMessage::Snapshot { .. } => counts[i] += 1,
                    ServerMessage::Error { code, message } => {
                        return Err(format!("client {i}: {code}: {message}").into());
                    }
                    _ => {}
                }
            }
        }
        // A small OS scheduling allowance is excluded from host update timings.
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..12 {
        host.update(0.);
        for c in &mut clients {
            c.poll()?;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if ready.iter().any(|v| !*v) || counts.iter().any(|n| *n == 0) || disconnected > 0 || errors > 0
    {
        return Err("load run did not retain every client cleanly".into());
    }
    timings.sort_by(f64::total_cmp);
    let percentile =
        |fraction: f64| timings[((timings.len() - 1) as f64 * fraction).round() as usize];
    let bytes_received: u64 = clients
        .iter()
        .map(|c| c.transport_stats().bytes_received)
        .sum();
    let phase_reports: Vec<_> = phase_stats
        .iter()
        .zip(phase_timings.iter_mut())
        .enumerate()
        .map(|(i, (stats, times))| {
            times.sort_by(f64::total_cmp);
            serde_json::json!({
                "name": if i == 0 {
                    "25 percent generic actor records moving"
                } else {
                    "generic actor records stationary; living world continues"
                },
                "measured_ticks": times.len(),
                "replication": stats,
                "encoded_replication_reduction_fraction": stats.saved_fraction(),
                "host_update_mean_ms": times.iter().sum::<f64>() / times.len() as f64,
                "host_update_p95_ms": times[((times.len() - 1) as f64 * 0.95).round() as usize]
            })
        })
        .collect();
    let report = serde_json::json!({
        "release": "0.4.0",
        "wire_protocol": terra_net::PROTOCOL_VERSION,
        "workload": "single-process host plus actual local TCP clients; four actor regions; 25% moving actor records",
        "environment": {
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "available_parallelism": std::thread::available_parallelism().map_or(1, |n| n.get()),
            "debug_assertions": cfg!(debug_assertions)
        },
        "clients": clients_count,
        "nonplayer_actor_records": actors_count,
        "total_actor_records": host.actors().len(),
        "living_bodies": host.sandbox().living.units.len(),
        "warmup_ticks": warmup,
        "measured_ticks": ticks,
        "fixed_hz": 20,
        "simulated_seconds": (warmup + ticks) as f64 / 20.,
        "wall_seconds": started.elapsed().as_secs_f64(),
        "host_update_ms": {
            "mean": timings.iter().sum::<f64>() / timings.len() as f64,
            "p50": percentile(0.50),
            "p95": percentile(0.95),
            "p99": percentile(0.99),
            "max": *timings.last().unwrap()
        },
        "replication": totals,
        "encoded_replication_reduction_fraction": totals.saved_fraction(),
        "actual_client_socket_bytes_received": bytes_received,
        "phases": phase_reports,
        "per_client_replication": clients.iter().map(|c| c.replication_stats()).collect::<Vec<_>>(),
        "principal_aoi_distance_candidates": candidates,
        "per_client_actor_snapshots": counts,
        "disconnects": disconnected,
        "domain_errors": errors,
        "ecology_scheduler": host.ecology_schedule_stats(),
        "limitations": [
            "local TCP only",
            "accelerated fixed simulation time; not wall-clock concurrent capacity",
            "simple kinematic actor records are not full living agents",
            "existing bounded per-client view caps remain",
            "no render workload, TLS, Internet latency or distributed host"
        ]
    });
    let json = serde_json::to_string_pretty(&report)?;
    println!("{json}");
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, json + "\n")?;
    }
    Ok(())
}
