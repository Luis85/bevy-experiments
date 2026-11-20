use super::*;
use crate::sandbox::WorldIdentity;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};
use terra_core::{GeoPosition, WorldClock};
use terra_persistence::RegionKey;
use terra_sandbox::{Command, SandboxState};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(
            new_private_directory(&std::env::temp_dir().join(format!(
                "terra-recovery-test-{}-{nonce}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )))
            .unwrap(),
        )
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(root: &Scratch, prepared: bool) -> (WorldConfig, PathBuf, RegionArchive) {
    let original = new_private_directory(&root.path("original")).unwrap();
    let mut config = WorldConfig {
        chunk_level: 16,
        live_weather: false,
        ..Default::default()
    };
    if prepared {
        let exported = export_source_package(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../terra-stream/data/heightmap-connected/manifest.json"),
            original.join("terrain"),
            SourceInspectionOptions {
                coverage_level: config.chunk_level,
                terrain_resolution: config.terrain_resolution,
                requested_bounds: None,
            },
        )
        .unwrap();
        config.base_data_revision = exported.inspection.source_identity;
        config.heightmap_manifest = Some(
            original
                .join("terrain")
                .join(exported.inspection.entrypoint),
        );
        let political_path = original.join(POLITICS);
        write_new_private_file(
            &political_path,
            &fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../terra-history/data/natural-earth-countries-2026-10-26.geojson"),
            )
            .unwrap(),
        )
        .unwrap();
        let dataset = PoliticsDataset::load(&political_path).unwrap();
        config.political_dataset_revision = Some(dataset.id);
        config.politics_file = Some(political_path);
    }
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    config.focus = origin;
    let catalog = Catalog::builtin();
    let mut world = SandboxState::new();
    world.add_player(1, origin).unwrap();
    world
        .add_player(2, GeoPosition::new(48.237, 11.675, 100.).unwrap())
        .unwrap();
    let faction = world
        .apply(
            1,
            Command::CreateFaction {
                name: "Recovery Commons".into(),
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    world
        .apply(
            1,
            Command::InviteToFaction {
                faction_id: faction,
                player_id: 2,
            },
            &catalog,
        )
        .unwrap();
    let plot = world
        .apply(
            1,
            Command::ClaimPlot {
                center: origin,
                radius_m: 30.,
            },
            &catalog,
        )
        .unwrap()
        .created_id
        .unwrap();
    world
        .apply(
            1,
            Command::TransferPlotToFaction {
                plot_id: plot,
                faction_id: faction,
            },
            &catalog,
        )
        .unwrap();
    let mut political_world = crate::PoliticalWorld::load(config.politics_file.as_deref()).unwrap();
    let clock = if let Some(dataset) = &political_world.dataset {
        WorldClock {
            unix_seconds: dataset.valid_from_unix as f64 + 43_200.,
            ..Default::default()
        }
    } else {
        WorldClock::default()
    };
    if prepared {
        political_world.draft = vec![
            origin,
            GeoPosition::new(origin.latitude + 0.001, origin.longitude, origin.altitude_m).unwrap(),
            GeoPosition::new(origin.latitude, origin.longitude + 0.001, origin.altitude_m).unwrap(),
        ];
        political_world
            .finish_draft(clock.unix_seconds as i64, Some(faction))
            .unwrap();
    }
    world.seed_living_demo(origin, &[], &catalog).unwrap();
    world.set_player_dormant(2, true).unwrap();
    for row in 0..4 {
        for col in 0..4 {
            let position = GeoPosition::new(
                origin.latitude + 0.05 + row as f64 * 0.01,
                origin.longitude + 0.05 + col as f64 * 0.02,
                100.,
            )
            .unwrap();
            let id = format!("recovery-{row}-{col}");
            world
                .seed_resource(id.clone(), position, "stone", 100, 0.)
                .unwrap();
            world.resource_nodes.get_mut(&id).unwrap().remaining = 7;
        }
    }
    let save = LocalWorldSave {
        schema_version: 1,
        identity: WorldIdentity::from(&config),
        clock,
        player_id: 1,
        state: world,
        politics: political_world.dataset,
    };
    let archive = save.regional_archive(&config, &catalog).unwrap();
    let database = original.join("active.db");
    config.region_store = Some(database.clone());
    let mut store = RegionStore::open(
        &database,
        archive.identity.clone(),
        &catalog,
        config.chunk_level,
    )
    .unwrap();
    let mut state = archive.state;
    let requested = BTreeSet::from([RegionKey::at(origin, config.chunk_level).unwrap()]);
    store
        .checkpoint(&mut state, archive.metadata.clone(), &requested)
        .unwrap();
    state.living.accounts.get_mut(&1).unwrap().money -= 7;
    store
        .checkpoint(&mut state, archive.metadata, &requested)
        .unwrap();
    let expected = store.archive().unwrap();
    assert!(expected.state.resource_nodes.len() > state.resource_nodes.len());
    json_new(&original.join("config.json"), &config).unwrap();
    (config, database, expected)
}

fn rewrite_index(directory: &Path, mut action: impl FnMut(&mut RecoveryIndex)) {
    let (mut index, _) = read_index(directory, None).unwrap();
    action(&mut index);
    fs::write(
        directory.join(RECOVERY_INDEX),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();
}

#[test]
fn uncertain_final_markers_keep_complete_backup_and_restored_tree_for_verification() {
    let scratch = Scratch::new();
    let (config, database, expected) = fixture(&scratch, true);
    let bundle = scratch.path("uncertain-bundle");
    let error = create_recovery_bundle_with_sync(&database, &config, &bundle, &|directory| {
        assert_eq!(directory, bundle);
        assert!(directory.join(RECOVERY_INDEX).is_file());
        Err(std::io::Error::other(
            "injected final bundle directory sync",
        ))
    })
    .unwrap_err();
    assert!(error.published());
    let verified = verify_recovery_bundle(&bundle, None).unwrap();
    assert_eq!(verified.source_generation, expected.source_generation);
    let destination = scratch.path("uncertain-restored");
    let error = restore_recovery_bundle_with_sync(
        &bundle,
        &destination,
        Some(&verified.bundle_identity),
        &|directory| {
            assert_eq!(directory, destination);
            assert!(directory.join("restore-receipt.json").is_file());
            Err(std::io::Error::other(
                "injected final restore directory sync",
            ))
        },
    )
    .unwrap_err();
    assert!(error.published());
    let restored = RegionStore::open_read_only(destination.join(DATABASE))
        .unwrap()
        .archive()
        .unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    let config = read_recovery_config(destination.join(CONFIG)).unwrap();
    LocalWorldSave::from_regional_archive(
        RegionStore::open_read_only(destination.join(DATABASE))
            .unwrap()
            .archive()
            .unwrap(),
        &config,
    )
    .unwrap();
    assert!(destination.join("restore-receipt.json").is_file());
    assert!(verify_recovery_bundle(&bundle, Some(&verified.bundle_identity)).is_ok());
}

#[test]
fn unpublished_recovery_failures_leave_no_completion_marker() {
    let scratch = Scratch::new();
    let (mut config, database, _) = fixture(&scratch, false);
    config.base_data_revision = "unsupported-source".into();
    let destination = scratch.path("refused-bundle");
    let error = create_recovery_bundle_with_sync(
        &database,
        &config,
        &destination,
        &terra_sandbox::publication::sync_directory,
    )
    .unwrap_err();
    assert!(!error.published());
    assert!(!destination.exists());
    assert!(
        RegionStore::open_read_only(database)
            .unwrap()
            .inspect()
            .unwrap()
            .domain_validated
    );
}

#[test]
fn prepared_recovery_survives_loss_of_original_tree_with_exact_cold_state_and_rebased_paths() {
    let scratch = Scratch::new();
    let (config, database, expected) = fixture(&scratch, true);
    let bundle = scratch.path("bundle");
    let created = create_recovery_bundle(&database, &config, &bundle).unwrap();
    assert!(created.prepared_source && created.local_host_metadata_validated);
    assert!(created.political_source);
    assert!(!created.inspection.game_metadata_requires_host_validation);
    assert_eq!(created.source_generation, 2);
    let original_bundle_database = fs::read(bundle.join(DATABASE)).unwrap();
    let original_political_bytes = fs::read(config.politics_file.as_ref().unwrap()).unwrap();
    fs::remove_dir_all(scratch.path("original")).unwrap();
    let destination = scratch.path("restored");
    let receipt =
        restore_recovery_bundle(&bundle, &destination, Some(&created.bundle_identity)).unwrap();
    assert!(receipt.domain_and_local_metadata_exact && receipt.runtime_paths_rebased);
    let restored_config = read_recovery_config(destination.join(CONFIG)).unwrap();
    assert_eq!(
        restored_config.region_store,
        Some(destination.join(DATABASE))
    );
    assert!(
        restored_config
            .heightmap_manifest
            .as_ref()
            .unwrap()
            .starts_with(&destination)
    );
    assert_eq!(restored_config.cache_dir, destination.join("cache"));
    assert_eq!(
        restored_config.politics_file,
        Some(destination.join(POLITICS))
    );
    assert_eq!(
        fs::read(destination.join(POLITICS)).unwrap(),
        original_political_bytes
    );
    let base_politics = PoliticsDataset::load(destination.join(POLITICS)).unwrap();
    assert_ne!(
        serde_json::to_value(&base_politics).unwrap(),
        expected.metadata["politics"],
        "authored saved territories are distinct from their immutable base input"
    );
    let restored = RegionStore::open_read_only(destination.join(DATABASE))
        .unwrap()
        .archive()
        .unwrap();
    assert_eq!(restored.state, expected.state);
    assert_eq!(restored.metadata, expected.metadata);
    assert_eq!(restored.identity, expected.identity);
    assert_eq!(restored.source_generation, 2);
    assert_eq!(
        archive_hash(&restored).unwrap(),
        archive_hash(&expected).unwrap()
    );
    assert_eq!(
        fs::read(bundle.join(DATABASE)).unwrap(),
        original_bundle_database
    );
    assert!(verify_recovery_bundle(&bundle, Some(&created.bundle_identity)).is_ok());
    let runtime = terra_stream::PreparedHeightmapProvider::open(
        restored_config.heightmap_manifest.as_ref().unwrap(),
        restored_config.terrain_resolution,
    )
    .unwrap();
    assert_eq!(runtime.source_revision(), config.base_data_revision);
}

#[test]
fn procedural_recovery_rejects_existing_destinations_and_wrong_receipts_without_mutation() {
    let scratch = Scratch::new();
    let (config, database, expected) = fixture(&scratch, false);
    let bundle = scratch.path("bundle");
    let created = create_recovery_bundle(&database, &config, &bundle).unwrap();
    assert!(!created.prepared_source);
    assert_eq!(created.file_count, 3);
    let index_before = fs::read(bundle.join(RECOVERY_INDEX)).unwrap();
    assert!(create_recovery_bundle(&database, &config, &bundle).is_err());
    assert_eq!(fs::read(bundle.join(RECOVERY_INDEX)).unwrap(), index_before);
    assert!(
        restore_recovery_bundle(
            &bundle,
            scratch.path("wrong-hash"),
            Some(&format!("sha256:{}", "0".repeat(64)))
        )
        .is_err()
    );
    assert!(!scratch.path("wrong-hash").exists());
    let existing = new_private_directory(&scratch.path("existing")).unwrap();
    fs::write(existing.join("keep"), b"incident remains").unwrap();
    assert!(restore_recovery_bundle(&bundle, &existing, Some(&created.bundle_identity)).is_err());
    assert_eq!(
        fs::read(existing.join("keep")).unwrap(),
        b"incident remains"
    );
    let source = RegionStore::open_read_only(&database)
        .unwrap()
        .archive()
        .unwrap();
    assert_eq!(
        archive_hash(&source).unwrap(),
        archive_hash(&expected).unwrap()
    );
}

#[test]
fn missing_changed_or_mismatched_political_dependencies_reject_without_publication() {
    let scratch = Scratch::new();
    let (config, database, _) = fixture(&scratch, true);
    let political_path = config.politics_file.as_ref().unwrap();
    let original_bytes = fs::read(political_path).unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(&original_bytes).unwrap();
    changed["terra"]["id"] = serde_json::json!("wrong-political-dataset");
    fs::write(political_path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(
        create_recovery_bundle(&database, &config, scratch.path("wrong-politics"))
            .unwrap_err()
            .contains("political input differs")
    );
    assert!(!scratch.path("wrong-politics").exists());
    fs::remove_file(political_path).unwrap();
    assert!(create_recovery_bundle(&database, &config, scratch.path("missing-politics")).is_err());
    assert!(!scratch.path("missing-politics").exists());
    write_new_private_file(political_path, &original_bytes).unwrap();
    let bundle = scratch.path("bundle");
    create_recovery_bundle(&database, &config, &bundle).unwrap();
    let mut altered = fs::read(bundle.join(POLITICS)).unwrap();
    altered.push(b'\n'); // Valid GeoJSON with unchanged ID must still fail its exact byte hash.
    fs::write(bundle.join(POLITICS), altered).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    assert!(restore_recovery_bundle(&bundle, scratch.path("changed-politics"), None).is_err());
    assert!(!scratch.path("changed-politics").exists());
}

#[test]
fn recovery_destinations_cannot_claim_sqlite_sidecars_or_mutate_retained_bundles() {
    let scratch = Scratch::new();
    let (config, database, expected) = fixture(&scratch, false);
    for suffix in ["-wal", "-shm", "-journal", "-WAL"] {
        let destination = PathBuf::from(format!("{}{suffix}", database.display()));
        let existed = destination.exists();
        let error = create_recovery_bundle(&database, &config, &destination).unwrap_err();
        assert!(error.contains("aliases the regional database"), "{error}");
        assert_eq!(destination.exists(), existed);
        assert!(!destination.is_dir());
    }
    let bundle = scratch.path("bundle");
    create_recovery_bundle(&database, &config, &bundle).unwrap();
    let before = fs::read(bundle.join(RECOVERY_INDEX)).unwrap();
    let error = restore_recovery_bundle(&bundle, bundle.join("nested"), None).unwrap_err();
    assert!(error.contains("outside the retained"), "{error}");
    assert!(!bundle.join("nested").exists());
    assert_eq!(fs::read(bundle.join(RECOVERY_INDEX)).unwrap(), before);
    assert!(verify_recovery_bundle(&bundle, None).is_ok());
    let unchanged = RegionStore::open_read_only(&database)
        .unwrap()
        .archive()
        .unwrap();
    assert_eq!(
        archive_hash(&unchanged).unwrap(),
        archive_hash(&expected).unwrap()
    );
}

#[test]
fn portable_recovery_inventory_and_configuration_use_forward_slashes() {
    let member = source_member(Path::new("tiles/west/height.png"));
    assert_eq!(member.to_str(), Some("source/tiles/west/height.png"));
    portable(&member).unwrap();
    let source = RecoverySource::Prepared {
        package_identity: "unused".into(),
        source_identity: "unused".into(),
        entrypoint: "manifest.json".into(),
    };
    let normalized = normalized_config(&WorldConfig::default(), &source, None);
    assert_eq!(
        normalized.heightmap_manifest.unwrap().to_str(),
        Some("source/manifest.json")
    );
    assert!(portable(Path::new("source\\manifest.json")).is_err());
}

#[test]
fn damaged_or_incomplete_bundles_never_publish_a_restore() {
    let scratch = Scratch::new();
    let (config, database, _) = fixture(&scratch, false);
    let bundle = scratch.path("bundle");
    create_recovery_bundle(database, &config, &bundle).unwrap();
    let mut bytes = fs::read(bundle.join(DATABASE)).unwrap();
    bytes[0] ^= 0x7f;
    fs::write(bundle.join(DATABASE), bytes).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    assert!(restore_recovery_bundle(&bundle, scratch.path("refused"), None).is_err());
    assert!(!scratch.path("refused").exists());
    fs::remove_file(bundle.join(RECOVERY_INDEX)).unwrap();
    assert!(restore_recovery_bundle(&bundle, scratch.path("incomplete"), None).is_err());
    assert!(!scratch.path("incomplete").exists());
}

#[test]
fn rewritten_indexes_cannot_hide_escaping_paths_unknown_members_or_unbounded_sizes() {
    let scratch = Scratch::new();
    let (config, database, _) = fixture(&scratch, false);
    let bundle = scratch.path("bundle");
    create_recovery_bundle(database, &config, &bundle).unwrap();
    let original_index = fs::read(bundle.join(RECOVERY_INDEX)).unwrap();
    for path in [
        "../outside",
        "/absolute",
        "source/../../outside",
        "source\\escape",
        "NUL",
        "source/file.",
    ] {
        fs::write(bundle.join(RECOVERY_INDEX), &original_index).unwrap();
        rewrite_index(&bundle, |index| index.files[0].path = path.into());
        assert!(verify_recovery_bundle(&bundle, None).is_err(), "{path}");
    }
    fs::write(bundle.join(RECOVERY_INDEX), &original_index).unwrap();
    rewrite_index(&bundle, |index| index.files[0].bytes = u64::MAX);
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    fs::write(bundle.join(RECOVERY_INDEX), &original_index).unwrap();
    json_new(&bundle.join("unknown.json"), &serde_json::json!({})).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
}

#[test]
fn local_host_metadata_and_resolved_configuration_are_validated_before_publication() {
    let scratch = Scratch::new();
    let (config, database, mut archive) = fixture(&scratch, false);
    let mut unsupported = config.clone();
    unsupported.live_data = true;
    assert!(create_recovery_bundle(&database, &unsupported, scratch.path("live")).is_err());
    assert!(!scratch.path("live").exists());
    unsupported = config.clone();
    unsupported.base_data_revision = "wrong-source".into();
    assert!(create_recovery_bundle(&database, &unsupported, scratch.path("wrong-source")).is_err());
    assert!(!scratch.path("wrong-source").exists());
    archive.metadata["player_id"] = serde_json::json!(999999);
    let invalid = scratch.path("invalid-host.db");
    archive.import_new(&invalid).unwrap(); // Generic storage deliberately does not own this metadata.
    assert!(create_recovery_bundle(&invalid, &config, scratch.path("invalid-host")).is_err());
    assert!(!scratch.path("invalid-host").exists());
    archive.metadata["player_id"] = serde_json::json!(1);
    archive.identity = serde_json::json!({"kind":"terra-server","schema_version":1});
    let server = scratch.path("server.db");
    archive.import_new(&server).unwrap();
    assert!(create_recovery_bundle(&server, &config, scratch.path("server-bundle")).is_err());
    assert!(!scratch.path("server-bundle").exists());
}

#[test]
fn rehashed_configuration_cannot_redirect_a_restore_to_an_original_path() {
    let scratch = Scratch::new();
    let (config, database, _) = fixture(&scratch, false);
    let bundle = scratch.path("bundle");
    create_recovery_bundle(&database, &config, &bundle).unwrap();
    let mut config = read_recovery_config(bundle.join(CONFIG)).unwrap();
    config.region_store = Some(database.clone());
    fs::write(
        bundle.join(CONFIG),
        serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    let new_hash = file_hash(&bundle, Path::new(CONFIG), MAX_CONFIG_BYTES).unwrap();
    rewrite_index(&bundle, |index| {
        *index
            .files
            .iter_mut()
            .find(|file| file.path == Path::new(CONFIG))
            .unwrap() = new_hash.clone();
        index.total_bytes = index.files.iter().map(|file| file.bytes).sum();
    });
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    assert!(restore_recovery_bundle(&bundle, scratch.path("redirected"), None).is_err());
    assert!(!scratch.path("redirected").exists());
    assert!(
        RegionStore::open_read_only(database)
            .unwrap()
            .inspect()
            .is_ok()
    );
}

#[cfg(unix)]
#[test]
fn recovery_members_are_private_and_symbolic_or_hard_links_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let scratch = Scratch::new();
    let (config, database, _) = fixture(&scratch, false);
    let bundle = scratch.path("bundle");
    create_recovery_bundle(database, &config, &bundle).unwrap();
    for name in [DATABASE, CONFIG, CATALOG, RECOVERY_INDEX] {
        assert_eq!(
            fs::metadata(bundle.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    assert_eq!(
        fs::metadata(&bundle).unwrap().permissions().mode() & 0o077,
        0
    );
    let catalog_bytes = fs::read(bundle.join(CATALOG)).unwrap();
    let external = scratch.path("outside-catalog.json");
    json_new(&external, &Catalog::builtin()).unwrap();
    fs::remove_file(bundle.join(CATALOG)).unwrap();
    symlink(&external, bundle.join(CATALOG)).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    fs::remove_file(bundle.join(CATALOG)).unwrap();
    fs::hard_link(&external, bundle.join(CATALOG)).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
    fs::remove_file(bundle.join(CATALOG)).unwrap();
    write_new_private_file(bundle.join(CATALOG), &catalog_bytes).unwrap();
    fs::set_permissions(bundle.join(CATALOG), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(verify_recovery_bundle(&bundle, None).is_err());
}
