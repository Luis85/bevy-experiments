//! Finite recovery rehearsal. Private fixture worlds are temporary; output is
//! counters, timings and verification flags, never world records or credentials.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};
use terra_bevy::{
    WorldConfig,
    recovery::{
        RECOVERY_INDEX, RecoveryIndex, create_recovery_bundle, read_recovery_config,
        restore_recovery_bundle, verify_recovery_bundle,
    },
    sandbox::{LocalWorldSave, WorldIdentity},
};
use terra_core::{GeoPosition, WorldClock};
use terra_history::PoliticsDataset;
use terra_persistence::{RegionKey, RegionStore, write_new_private_file};
use terra_sandbox::{Catalog, Command, SandboxState};
use terra_stream::{
    PreparedHeightmapProvider,
    source_distribution::{SourceInspectionOptions, export_source_package},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "terra-recovery-bundle-probe-{}-{nonce}",
            std::process::id()
        ));
        private_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
fn digest(path: &Path) -> std::io::Result<String> {
    Ok(format!("sha256:{:x}", Sha256::digest(fs::read(path)?)))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => None,
        ["--output", output] => Some(PathBuf::from(output)),
        _ => return Err("usage: recovery_bundle_probe [--output NEW_REPORT.json]".into()),
    };
    if output.as_ref().is_some_and(|path| path.exists()) {
        return Err("report output must be new".into());
    }
    let scratch = Scratch::new()?;
    let original = scratch.0.join("original");
    private_dir(&original)?;
    let origin = GeoPosition::new(48.137, 11.575, 100.)?;
    let mut config = WorldConfig {
        focus: origin,
        chunk_level: 16,
        live_weather: false,
        ..Default::default()
    };
    let source = export_source_package(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-stream/data/heightmap-connected/manifest.json"),
        original.join("terrain"),
        SourceInspectionOptions {
            coverage_level: config.chunk_level,
            terrain_resolution: config.terrain_resolution,
            requested_bounds: None,
        },
    )?;
    config.heightmap_manifest = Some(original.join("terrain").join(&source.inspection.entrypoint));
    config.base_data_revision = source.inspection.source_identity.clone();
    let political_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-history/data/natural-earth-countries-2026-10-26.geojson"),
    )?;
    let political_path = original.join("politics.geojson");
    write_new_private_file(&political_path, &political_bytes)?;
    let base_politics = PoliticsDataset::load(&political_path)?;
    config.political_dataset_revision = Some(base_politics.id.clone());
    config.politics_file = Some(political_path);
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    world.add_player(1, origin)?;
    world.add_player(2, GeoPosition::new(48.237, 11.675, 100.)?)?;
    let faction = world
        .apply(
            1,
            Command::CreateFaction {
                name: "Recovery Commons".into(),
            },
            &catalog,
        )?
        .created_id
        .ok_or("faction missing")?;
    world.apply(
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 2,
        },
        &catalog,
    )?;
    let plot = world
        .apply(
            1,
            Command::ClaimPlot {
                center: origin,
                radius_m: 30.,
            },
            &catalog,
        )?
        .created_id
        .ok_or("plot missing")?;
    world.apply(
        1,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
        &catalog,
    )?;
    world.seed_living_demo(origin, &[], &catalog)?;
    world.set_player_dormant(2, true)?;
    for row in 0..32 {
        for col in 0..32 {
            let position = GeoPosition::new(
                origin.latitude + 0.05 + row as f64 * 0.01,
                origin.longitude + 0.05 + col as f64 * 0.02,
                100.,
            )?;
            for node in 0..4 {
                let id = format!("bundle-{row}-{col}-{node}");
                world.seed_resource(id.clone(), position, "stone", 100, 0.)?;
                world
                    .resource_nodes
                    .get_mut(&id)
                    .ok_or("resource missing")?
                    .remaining = 7;
            }
        }
    }
    let clock = WorldClock {
        unix_seconds: base_politics.valid_from_unix as f64 + 43_200.,
        ..Default::default()
    };
    let mut political_world = terra_bevy::PoliticalWorld::load(config.politics_file.as_deref())?;
    political_world.draft = vec![
        origin,
        GeoPosition::new(origin.latitude + 0.001, origin.longitude, origin.altitude_m)?,
        GeoPosition::new(origin.latitude, origin.longitude + 0.001, origin.altitude_m)?,
    ];
    political_world.finish_draft(clock.unix_seconds as i64, Some(faction))?;
    let save = LocalWorldSave {
        schema_version: 1,
        identity: WorldIdentity::from(&config),
        clock,
        player_id: 1,
        state: world,
        politics: political_world.dataset,
    };
    let archive = save.regional_archive(&config, &catalog)?;
    let database = original.join("active.db");
    config.region_store = Some(database.clone());
    write_new_private_file(
        original.join("world-config.json"),
        &serde_json::to_vec_pretty(&config)?,
    )?;
    let mut store = RegionStore::open(
        &database,
        archive.identity.clone(),
        &catalog,
        config.chunk_level,
    )?;
    let mut active = archive.state;
    let requested = BTreeSet::from([RegionKey::at(origin, config.chunk_level)?]);
    store.checkpoint(&mut active, archive.metadata.clone(), &requested)?;
    active
        .living
        .accounts
        .get_mut(&1)
        .ok_or("owner account missing")?
        .money -= 7;
    store.checkpoint(&mut active, archive.metadata, &requested)?;
    let expected = store.archive()?;
    let active_resources = active.resource_nodes.len();
    drop(store);
    let bundle = scratch.0.join("bundle");
    let started = Instant::now();
    let created = create_recovery_bundle(&database, &config, &bundle)?;
    let create_ms = started.elapsed().as_secs_f64() * 1000.;
    let bundle_database_digest = digest(&bundle.join("world.db"))?;
    let index: RecoveryIndex = serde_json::from_slice(&fs::read(bundle.join(RECOVERY_INDEX))?)?;
    // Remove every original authority/configuration/source input, not merely
    // rebase a path while those files remain available as an accidental fallback.
    fs::remove_dir_all(&original)?;
    let restored = scratch.0.join("restored");
    let started = Instant::now();
    let receipt = restore_recovery_bundle(&bundle, &restored, Some(&created.bundle_identity))?;
    let restore_ms = started.elapsed().as_secs_f64() * 1000.;
    let restored_config = read_recovery_config(restored.join("world-config.json"))?;
    let actual = RegionStore::open_read_only(restored.join("world.db"))?.archive()?;
    let provider = PreparedHeightmapProvider::open(
        restored_config
            .heightmap_manifest
            .as_ref()
            .ok_or("restored terrain missing")?,
        restored_config.terrain_resolution,
    )?;
    let restored_political_path = restored_config
        .politics_file
        .as_ref()
        .ok_or("restored political input missing")?;
    let restored_base_politics = PoliticsDataset::load(restored_political_path)?;
    let private_modes = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(&restored)?.permissions().mode() & 0o077 == 0
                && [
                    "world.db",
                    "world-config.json",
                    "catalog.json",
                    "politics.geojson",
                    "restore-receipt.json",
                ]
                .iter()
                .all(|file| {
                    fs::metadata(restored.join(file))
                        .is_ok_and(|metadata| metadata.permissions().mode() & 0o077 == 0)
                })
        }
        #[cfg(not(unix))]
        {
            false
        }
    };
    let corrupt = scratch.0.join("damaged-bundle");
    private_dir(&corrupt)?;
    for file in index
        .files
        .iter()
        .map(|file| file.path.clone())
        .chain([PathBuf::from(RECOVERY_INDEX)])
    {
        if let Some(parent) = file.parent() {
            let mut cursor = corrupt.clone();
            for component in parent.components() {
                cursor.push(component);
                if !cursor.exists() {
                    private_dir(&cursor)?;
                }
            }
        }
        write_new_private_file(corrupt.join(&file), &fs::read(bundle.join(&file))?)?;
    }
    verify_recovery_bundle(&corrupt, Some(&created.bundle_identity))?;
    let mut damaged_bytes = fs::read(corrupt.join("world.db"))?;
    damaged_bytes[0] ^= 0x7f;
    fs::write(corrupt.join("world.db"), damaged_bytes)?;
    let refused = scratch.0.join("refused");
    let corrupt_rejected =
        restore_recovery_bundle(&corrupt, &refused, Some(&created.bundle_identity)).is_err()
            && !refused.exists();
    let final_verification = verify_recovery_bundle(&bundle, Some(&created.bundle_identity))?;
    let verification = json!({
        "original_database_configuration_and_source_removed": !original.exists(),
        "full_state_exact": actual.state == expected.state,
        "local_host_metadata_exact": actual.metadata == expected.metadata,
        "world_identity_exact": actual.identity == expected.identity,
        "catalog_exact": serde_json::to_value(&actual.catalog)? == serde_json::to_value(&expected.catalog)?,
        "generation_preserved": actual.source_generation == expected.source_generation && actual.source_generation == 2,
        "dormant_records_preserved": actual.state.resource_nodes.len() == expected.state.resource_nodes.len() && active_resources < expected.state.resource_nodes.len(),
        "source_identity_exact_without_original_tree": provider.source_revision() == config.base_data_revision,
        "political_source_bytes_exact": fs::read(restored_political_path)? == political_bytes,
        "political_source_identity_exact": Some(&restored_base_politics.id) == config.political_dataset_revision.as_ref(),
        "authored_political_state_exact": actual.metadata["politics"] == expected.metadata["politics"] && actual.metadata["politics"] != serde_json::to_value(&restored_base_politics)?,
        "runtime_paths_rebased": restored_config.region_store == Some(restored.join("world.db")) && restored_config.heightmap_manifest.as_ref().is_some_and(|path| path.starts_with(&restored)) && restored_political_path.starts_with(&restored),
        "bundle_database_bytes_unchanged": digest(&bundle.join("world.db"))? == bundle_database_digest,
        "bundle_receipt_unchanged": final_verification.bundle_identity == created.bundle_identity,
        "damaged_copy_rejected_without_output": corrupt_rejected,
    });
    if verification
        .as_object()
        .ok_or("invalid verification object")?
        .values()
        .any(|value| value != &json!(true))
        || (cfg!(unix) && !private_modes)
    {
        return Err(
            "recovery rehearsal did not satisfy exact continuation or rejection requirements"
                .into(),
        );
    }
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"), "fixture": "original local prepared-source recovery with 1024 separated resource groups",
        "environment": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "scope": {"resource_groups": 1024, "resources": expected.state.resource_nodes.len(), "active_resources": active_resources, "players": expected.state.players.len(), "living_bodies": expected.state.living.units.len(), "factions": expected.state.factions.len(), "plots": expected.state.plots.len(), "prepared_source_identity": source.inspection.source_identity, "political_dataset_identity": base_politics.id, "political_input_sha256": format!("{:x}", Sha256::digest(&political_bytes)), "server_capabilities": false, "renderer": false},
        "create_ms": create_ms, "restore_ms": restore_ms, "created": created, "restored": receipt,
        "unix_private_modes_verified": private_modes, "verification": verification,
        "limits": "Finite local host correctness rehearsal. Distant authored resources do not imply terrain coverage. No connected clients, server capability validation, power-cut, offsite service, Windows ACL or RPO/RTO qualification. Times include full validation on this machine."
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(output) = output {
        write_new_private_file(output, &bytes)?;
    }
    println!("{}", String::from_utf8(bytes)?);
    Ok(())
}
