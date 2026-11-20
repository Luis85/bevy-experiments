//! A reproducible 1,024-region recovery workload. This measures storage
//! correctness and maintenance cost, not connected-player or renderer capacity.
use serde_json::json;
use std::{collections::BTreeSet, error::Error, time::Instant};
use terra_core::GeoPosition;
use terra_persistence::{RegionBackupOptions, RegionKey, RegionStore};
use terra_sandbox::{Catalog, SandboxState};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = match args.next().as_deref() {
        None => None,
        Some("--output") => Some(args.next().ok_or("--output requires a path")?),
        _ => return Err("usage: recovery_probe [--output report.json]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let origin = GeoPosition::new(48.137, 11.575, 100.)?;
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    world.add_player(1, origin)?;
    world.seed_living_demo(origin, &[], &catalog)?;
    for row in 0..32 {
        for col in 0..32 {
            let position = GeoPosition::new(
                origin.latitude + row as f64 * 0.01,
                origin.longitude + col as f64 * 0.02,
                100.,
            )?;
            for node in 0..6 {
                let id = format!("recovery-{row}-{col}-{node}");
                world.seed_resource(id.clone(), position, "stone", 100, 0.)?;
                world.resource_nodes.get_mut(&id).unwrap().remaining = 7;
            }
        }
    }
    world.validate_with_catalog(&catalog)?;
    let complete_resources = world.resource_nodes.len();
    let complete_bodies = world.living.units.len();
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("active.db");
    let backup = directory.path().join("verified.db");
    let recovered = directory.path().join("recovered.db");
    let identity =
        json!({"kind":"regional-recovery-probe","source":"original-synthetic-fixture-v1"});
    let requested = BTreeSet::from([RegionKey::at(origin, 16)?]);
    let mut store = RegionStore::open(&source, identity.clone(), &catalog, 16)?;
    let start = Instant::now();
    store.checkpoint(
        &mut world,
        json!({"tick":1,"private_fixture":"preserve-exactly"}),
        &requested,
    )?;
    world
        .living
        .accounts
        .get_mut(&1)
        .ok_or("missing player account")?
        .money -= 7;
    store.checkpoint(
        &mut world,
        json!({"tick":2,"private_fixture":"preserve-exactly"}),
        &requested,
    )?;
    let checkpoint_ms = start.elapsed().as_secs_f64() * 1000.;
    let expected = store.export(&world)?;
    let active_resources = world.resource_nodes.len();
    let status = store.status();
    let backup_receipt = store.backup_with_options(&backup, RegionBackupOptions::default())?;
    if backup_receipt.generation != 2 || backup_receipt.nonempty_regions < 1024 {
        return Err("unexpected backup generation or workload size".into());
    }
    let backup_bytes = std::fs::read(&backup)?;
    // Simulate a bad incident database after a known-good backup. The recovery
    // operation does not open or modify that suspect file.
    let incident = rusqlite::Connection::open(&source)?;
    incident.execute(
        "UPDATE regions SET digest='corrupt-incident' WHERE rowid=(SELECT MIN(rowid) FROM regions)",
        [],
    )?;
    drop(incident);
    let refused = directory.path().join("refused.db");
    if store.backup_to(&refused).is_ok() || refused.exists() {
        return Err("corrupt source was published as a valid backup".into());
    }
    let reader = RegionStore::open_read_only(&backup)?;
    let recovery_receipt =
        reader.backup_with_options(&recovered, RegionBackupOptions::default())?;
    drop(reader);
    if std::fs::read(&backup)? != backup_bytes {
        return Err("recovery changed backup bytes".into());
    }
    let start = Instant::now();
    let mut resumed = RegionStore::open(&recovered, identity, &catalog, 16)?;
    let saved = resumed.restore()?.ok_or("missing recovered world")?;
    let restore_ms = start.elapsed().as_secs_f64() * 1000.;
    if saved.state != expected
        || saved.generation != 2
        || saved.metadata != json!({"tick":2,"private_fixture":"preserve-exactly"})
    {
        return Err("recovered world, private metadata or generation differ".into());
    }
    let inspection = resumed.inspect()?;
    let report = json!({
        "version":env!("CARGO_PKG_VERSION"),
        "fixture":"original synthetic 1024-region storage recovery",
        "environment":{"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"sqlite":rusqlite::version(),"profile":"optimized dev"},
        "scope":{"requested_regions":1,"complete_nonempty_regions":inspection.nonempty_regions,"complete_resource_nodes":complete_resources,"active_resource_nodes":active_resources,"complete_living_bodies":complete_bodies,"connected_clients":0,"terrain_import":false,"renderer":false},
        "two_checkpoints_ms":checkpoint_ms,"status":status,
        "backup":backup_receipt,"recovery":recovery_receipt,"restore_ms":restore_ms,
        "verification":{"full_state_exact":true,"private_metadata_exact":true,"generation_preserved":true,"corrupt_source_rejected_without_output":true,"backup_source_bytes_unchanged":true,"dormant_records_preserved":true},
        "limits":"Single bounded authority. Times include this fixture on this machine; they are not multiplayer throughput or a latency guarantee. Copy retry deadlines do not preempt filesystem I/O or full validation. Host-specific metadata still requires its application validator."
    });
    let bytes = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &bytes)?;
    }
    println!("{bytes}");
    Ok(())
}
