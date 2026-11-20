use super::*;
use std::collections::{BTreeMap, BTreeSet};
use terra_bevy::{
    SandboxRuntime,
    sandbox::{LocalWorldSave, WorldIdentity},
};
use terra_core::{GeoPosition, WorldClock};
use terra_persistence::{RegionArchive, RegionKey, RegionStore};

struct Scratch {
    path: PathBuf,
    keep: bool,
}
impl Scratch {
    fn new(review: bool) -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let explicit = review
            .then(|| std::env::var_os("TERRA_LAUNCHER_REVIEW_DIR"))
            .flatten();
        let keep = explicit.is_some();
        let path = explicit.map(PathBuf::from).unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "terra-launcher-recovery-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ))
        });
        assert!(!path.exists(), "Review/test directory must be new");
        fs::create_dir(&path).unwrap();
        model::private_permissions(&path, true).unwrap();
        Self { path, keep }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[test]
fn uncertain_nested_recovery_publication_retains_verifiable_outputs_without_success() {
    for failed_marker in ["backend", "wrapper"] {
        let scratch = Scratch::new(false);
        let fixture = fixture(&scratch, false);
        let world = RecentWorld {
            path: fixture.profile_path.clone(),
            profile: fixture.profile.clone(),
            has_checkpoint_file: true,
        };
        let error = create_backup_with_sync(
            select_world(&world).unwrap(),
            &fixture.user_data,
            &|directory| {
                if (failed_marker == "backend"
                    && directory
                        .join(terra_bevy::recovery::RECOVERY_INDEX)
                        .is_file())
                    || (failed_marker == "wrapper" && directory.join(RECORD).is_file())
                {
                    assert!(
                        model::begin_managed_maintenance(
                            &fixture.profile_path,
                            &fixture.profile.config,
                        )
                        .err()
                        .unwrap()
                        .contains("already open"),
                        "backup ownership must span the final publication boundary"
                    );
                    Err(std::io::Error::other("injected nested backup sync failure"))
                } else {
                    terra_bevy::rules::publication::sync_directory(directory)
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("retained") && error.contains("did not confirm"));
        let directory = fs::read_dir(fixture.user_data.join("backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let verified = verify_recovery_bundle(directory.join(BUNDLE), None).unwrap();
        assert_eq!(
            verified.source_generation,
            fixture.archive.source_generation
        );
        if failed_marker == "backend" {
            assert!(!directory.join(RECORD).exists());
            assert!(
                read_backup(&directory).is_err(),
                "an inner bundle is not a completed native backup wrapper"
            );
        } else {
            assert!(read_backup(&directory).is_ok());
        }
        // Complete a separate normal backup for the restore half of the same
        // fault matrix. The previously published output is not overwritten.
        let backup = create_backup(select_world(&world).unwrap(), &fixture.user_data).unwrap();
        let review = review_backup(backup.clone()).unwrap();
        let error = restore_reviewed_with_sync(review, &fixture.user_data, &|directory| {
            if (failed_marker == "backend" && directory.join("restore-receipt.json").is_file())
                || (failed_marker == "wrapper" && directory.join("world.json").is_file())
            {
                Err(std::io::Error::other(
                    "injected nested restore sync failure",
                ))
            } else {
                terra_bevy::rules::publication::sync_directory(directory)
            }
        })
        .unwrap_err();
        assert!(error.contains("retained") && error.contains("did not confirm"));
        let restored = fs::read_dir(fixture.user_data.join("worlds"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("recovered-")
            })
            .unwrap();
        let archive = RegionStore::open_read_only(restored.join("world/world.db"))
            .unwrap()
            .archive()
            .unwrap();
        assert_eq!(
            serde_json::to_value(archive).unwrap(),
            serde_json::to_value(&fixture.archive).unwrap()
        );
        if failed_marker == "backend" {
            assert!(!restored.join("world.json").exists());
            assert!(WorldProfile::read(&restored.join("world.json")).is_err());
        } else {
            assert!(WorldProfile::read(&restored.join("world.json")).is_ok());
        }
        assert!(read_backup(&backup.directory).is_ok());
        assert!(WorldProfile::read(&fixture.profile_path).is_ok());
    }
}

#[test]
fn new_managed_backup_and_world_parent_sync_failures_cannot_acknowledge_completion() {
    let scratch = Scratch::new(false);
    let fixture = fixture(&scratch, false);
    let world = RecentWorld {
        path: fixture.profile_path.clone(),
        profile: fixture.profile.clone(),
        has_checkpoint_file: true,
    };
    let original_profile = fs::read(&fixture.profile_path).unwrap();
    let observed = std::cell::Cell::new(false);
    let error = create_backup_with_sync(
        select_world(&world).unwrap(),
        &fixture.user_data,
        &|directory| {
            if directory == fixture.user_data {
                observed.set(true);
                Err(std::io::Error::other(
                    "injected new backups parent-entry sync",
                ))
            } else {
                terra_bevy::rules::publication::sync_directory(directory)
            }
        },
    )
    .unwrap_err();
    assert!(observed.get() && error.contains("parent-entry sync"));
    assert!(
        !fixture.user_data.join("backups").exists(),
        "failed new-parent admission must not leave a completed backup"
    );
    assert_eq!(fs::read(&fixture.profile_path).unwrap(), original_profile);
    let backup = create_backup(select_world(&world).unwrap(), &fixture.user_data).unwrap();
    let new_user_data = scratch.path.join("new-recovery-user-data");
    let observed = std::cell::Cell::new(false);
    let error = restore_reviewed_with_sync(
        review_backup(backup.clone()).unwrap(),
        &new_user_data,
        &|directory| {
            if directory == new_user_data {
                observed.set(true);
                Err(std::io::Error::other(
                    "injected new worlds parent-entry sync",
                ))
            } else {
                terra_bevy::rules::publication::sync_directory(directory)
            }
        },
    )
    .unwrap_err();
    assert!(observed.get() && error.contains("parent-entry sync"));
    assert!(
        !new_user_data.exists(),
        "an unconfirmed empty new-parent chain is removed before retry"
    );
    assert!(read_backup(&backup.directory).is_ok());
    let restored = restore_reviewed(review_backup(backup).unwrap(), &new_user_data).unwrap();
    assert!(WorldProfile::read(&restored.profile_path).is_ok());
}

struct Fixture {
    user_data: PathBuf,
    profile_path: PathBuf,
    source_root: PathBuf,
    profile: WorldProfile,
    archive: RegionArchive,
}

fn fixture(root: &Scratch, prepared: bool) -> Fixture {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let user_data = root.path.join("player-data");
    let source_root = root.path.join("original-inputs");
    model::private_directory(&source_root).unwrap();
    let mut draft = model::Draft {
        world_name: "Recovery valley".into(),
        ..Default::default()
    };
    draft.character.name = "Ari".into();
    draft.character.body_style = 1;
    draft.character.outfit = 5;
    draft.quality.quality = terra_bevy::render::RenderQuality::Performance;
    draft.quality.atmospheric_sky = false;
    draft.start_hour = 18;
    if prepared {
        draft.preset = model::Preset::Heightmap;
    }
    let profile_path = model::create_world(&draft, &project, &user_data).unwrap();
    let mut profile = WorldProfile::read(&profile_path).unwrap();
    profile.config.time_scale = 0.;
    profile.config.cache_dir = source_root.join("cache");
    if prepared {
        let exported = terra_stream::source_distribution::export_source_package(
            profile.config.heightmap_manifest.as_ref().unwrap(),
            source_root.join("terrain"),
            terra_stream::source_distribution::SourceInspectionOptions {
                coverage_level: profile.config.chunk_level,
                terrain_resolution: profile.config.terrain_resolution,
                requested_bounds: None,
            },
        )
        .unwrap();
        profile.config.heightmap_manifest = Some(
            source_root
                .join("terrain")
                .join(exported.inspection.entrypoint),
        );
        let politics = source_root.join("politics.geojson");
        fs::copy(profile.config.politics_file.as_ref().unwrap(), &politics).unwrap();
        profile.config.politics_file = Some(politics);
    } else {
        profile.config.politics_file = None;
        profile.config.political_dataset_revision = None;
    }
    let origin = profile.config.focus;
    let mut runtime = SandboxRuntime::new(origin);
    runtime
        .state
        .add_player(
            2,
            GeoPosition::new(origin.latitude + 0.06, origin.longitude, 0.).unwrap(),
        )
        .unwrap();
    runtime
        .state
        .seed_living_demo(origin, &[], &runtime.catalog)
        .unwrap();
    runtime.state.set_player_dormant(2, true).unwrap();
    for row in 0..4 {
        for col in 0..4 {
            let position = GeoPosition::new(
                origin.latitude + 0.05 + row as f64 * 0.01,
                origin.longitude + 0.05 + col as f64 * 0.02,
                0.,
            )
            .unwrap();
            let id = format!("launcher-recovery-{row}-{col}");
            runtime
                .state
                .seed_resource(id.clone(), position, "stone", 100, 0.)
                .unwrap();
            runtime.state.resource_nodes.get_mut(&id).unwrap().remaining = 7;
        }
    }
    let politics = terra_bevy::PoliticalWorld::load(profile.config.politics_file.as_deref())
        .unwrap()
        .dataset;
    let clock = WorldClock {
        unix_seconds: politics
            .as_ref()
            .map_or(WorldClock::default().unix_seconds, |dataset| {
                dataset.valid_from_unix as f64 + 43_200.
            }),
        time_scale: 0.,
        paused: false,
    };
    let save = LocalWorldSave {
        schema_version: 1,
        identity: WorldIdentity::from(&profile.config),
        clock,
        player_id: 1,
        state: runtime.state,
        politics,
    };
    let archive = save
        .regional_archive(&profile.config, &runtime.catalog)
        .unwrap();
    let mut store = RegionStore::open(
        profile.config.region_store.as_ref().unwrap(),
        archive.identity,
        &runtime.catalog,
        profile.config.chunk_level,
    )
    .unwrap();
    let requested = BTreeSet::from([RegionKey::at(origin, profile.config.chunk_level).unwrap()]);
    let mut state = archive.state;
    store
        .checkpoint(&mut state, archive.metadata.clone(), &requested)
        .unwrap();
    state.living.accounts.get_mut(&1).unwrap().money -= 7;
    store
        .checkpoint(&mut state, archive.metadata, &requested)
        .unwrap();
    let archive = store.archive().unwrap();
    drop(store);
    profile.checkpoint_present = true;
    model::atomic_json(&profile_path, &profile).unwrap();
    Fixture {
        user_data,
        profile_path,
        source_root,
        profile,
        archive,
    }
}

fn select(fixture: &Fixture) -> WorldSelection {
    let recent = model::recent_worlds(&fixture.user_data).0;
    select_world(
        recent
            .iter()
            .find(|world| world.path == fixture.profile_path)
            .unwrap(),
    )
    .unwrap()
}

fn tree_bytes(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), out);
            } else {
                out.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(path, path, &mut out);
    out
}

#[test]
fn managed_maintenance_coordinates_real_sessions_and_other_maintenance() {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const CHILD: &str = "TERRA_MAINTENANCE_TEST_PROFILE";
    const MODE: &str = "TERRA_MAINTENANCE_TEST_MODE";
    const READY: &str = "TERRA_MAINTENANCE_TEST_READY";
    if let Some(path) = std::env::var_os(CHILD) {
        let path = PathBuf::from(path);
        let profile = WorldProfile::read(&path).unwrap();
        let held = || {
            fs::write(std::env::var_os(READY).unwrap(), b"ownership acquired").unwrap();
            let mut command = [0];
            std::io::stdin().read_exact(&mut command).unwrap();
            assert_eq!(command, [b'q']);
        };
        if std::env::var(MODE).unwrap() == "session" {
            let _lease = model::begin_managed_session(&path, &profile.config).unwrap();
            held();
        } else {
            let _lease = model::begin_managed_maintenance(&path, &profile.config).unwrap();
            held();
        }
        return;
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    // Admit the legacy committed fixture once before retaining the start record.
    drop(model::begin_managed_session(&fixture.profile_path, &fixture.profile.config).unwrap());
    let witness = model::managed_start_witness_path(&fixture.profile_path);
    let database = fixture.profile.config.region_store.as_ref().unwrap();
    for mode in ["session", "maintenance"] {
        for abrupt in [false, true] {
            let selected = model::recent_worlds(&fixture.user_data).0.remove(0);
            let profile_before = fs::read(&fixture.profile_path).unwrap();
            let progress_before = fs::read(database).unwrap();
            let witness_before = fs::read(&witness).unwrap();
            let ready = root.path.join(format!("ready-{mode}-{abrupt}"));
            let mut child = Child(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "launcher::recovery::tests::managed_maintenance_coordinates_real_sessions_and_other_maintenance",
                        "--nocapture",
                    ])
                    .env(CHILD, &fixture.profile_path)
                    .env(MODE, mode)
                    .env(READY, &ready)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let started = Instant::now();
            while !ready.is_file() {
                assert!(child.0.try_wait().unwrap().is_none());
                assert!(started.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(10));
            }
            let started = Instant::now();
            assert!(
                rename_world(&selected, &fixture.user_data, "Must not overwrite")
                    .unwrap_err()
                    .contains("already open")
            );
            assert!(
                create_backup(select_world(&selected).unwrap(), &fixture.user_data)
                    .unwrap_err()
                    .contains("already open")
            );
            assert!(
                model::begin_managed_session(&fixture.profile_path, &selected.profile.config)
                    .err()
                    .unwrap()
                    .contains("already open")
            );
            assert!(started.elapsed() < Duration::from_secs(2));
            assert_eq!(fs::read(&fixture.profile_path).unwrap(), profile_before);
            assert_eq!(fs::read(database).unwrap(), progress_before);
            assert_eq!(fs::read(&witness).unwrap(), witness_before);
            assert!(
                !fixture.user_data.join("backups").exists(),
                "busy maintenance must not allocate a backup destination"
            );
            if abrupt {
                child.0.kill().unwrap();
                child.0.wait().unwrap();
            } else {
                child.0.stdin.as_mut().unwrap().write_all(b"q").unwrap();
                assert!(child.0.wait().unwrap().success());
            }
            drop(child);
            let title = format!("Released {mode} {abrupt}");
            assert_eq!(
                rename_world(&selected, &fixture.user_data, &title).unwrap(),
                title
            );
            assert_eq!(fs::read(database).unwrap(), progress_before);
            assert_eq!(fs::read(&witness).unwrap(), witness_before);
        }
    }
    // Successful maintenance after both kinds of release still performs the
    // existing independently verified regional backup/recovery path.
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let restored = restore_reviewed(review_backup(backup).unwrap(), &fixture.user_data).unwrap();
    let restored_profile = WorldProfile::read(&restored.profile_path).unwrap();
    let restored_archive =
        RegionStore::open_read_only(restored_profile.config.region_store.unwrap())
            .unwrap()
            .archive()
            .unwrap();
    assert_eq!(
        serde_json::to_value(restored_archive).unwrap(),
        serde_json::to_value(&fixture.archive).unwrap()
    );
}

#[test]
fn json_world_maintenance_does_not_admit_first_launch_or_create_progress() {
    let root = Scratch::new(false);
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let user_data = root.path.join("player-data");
    let draft = model::Draft {
        regional_storage: false,
        ..Default::default()
    };
    let path = model::create_world(&draft, &project, &user_data).unwrap();
    let selected = model::recent_worlds(&user_data).0.remove(0);
    let witness = model::managed_start_witness_path(&path);
    let original = fs::read(&path).unwrap();
    let lease = model::begin_managed_maintenance(&path, &selected.profile.config).unwrap();
    assert!(
        rename_world(&selected, &user_data, "Not yet")
            .unwrap_err()
            .contains("already open")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!witness.exists());
    assert!(!selected.profile.config.save_path.exists());
    drop(lease);
    rename_world(&selected, &user_data, "Ready for a first journey").unwrap();
    let renamed = WorldProfile::read(&path).unwrap();
    assert!(!renamed.checkpoint_present);
    assert_eq!(
        renamed.last_played_unix_seconds,
        selected.profile.last_played_unix_seconds
    );
    assert!(!witness.exists());
    assert!(!renamed.config.save_path.exists());
    // A real first launch still receives exactly its original fresh admission.
    let session = model::begin_managed_session(&path, &renamed.config).unwrap();
    assert!(!session.require_existing_progress);
    assert!(witness.is_file());
    assert!(!renamed.config.save_path.exists());
    drop(session);
}

#[test]
fn managed_recovery_journey_preserves_complete_world_after_original_loss() {
    let root = Scratch::new(true);
    let fixture = fixture(&root, true);
    let expected = serde_json::to_vec(&fixture.archive).unwrap();
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    assert_eq!(backup.record.source_generation, 2);
    let original_copy = tree_bytes(&backup.directory);
    let expected_politics =
        fs::read(fixture.profile.config.politics_file.as_ref().unwrap()).unwrap();
    fs::remove_dir_all(fixture.profile_path.parent().unwrap()).unwrap();
    fs::remove_dir_all(&fixture.source_root).unwrap();
    assert!(model::recent_worlds(&fixture.user_data).0.is_empty());
    let listed = recent_backups(&fixture.user_data);
    assert!(listed.1.is_empty());
    assert_eq!(listed.0.len(), 1);
    let review = review_backup(listed.0.into_iter().next().unwrap()).unwrap();
    let counts = review.verified.inspection.clone();
    let restored = restore_reviewed(review, &fixture.user_data).unwrap();
    let profile = WorldProfile::read(&restored.profile_path).unwrap();
    let store = RegionStore::open_read_only(profile.config.region_store.as_ref().unwrap()).unwrap();
    let archive = store.archive().unwrap();
    assert_eq!(serde_json::to_vec(&archive).unwrap(), expected);
    assert_eq!(
        serde_json::to_value(&profile.character).unwrap(),
        serde_json::to_value(&fixture.profile.character).unwrap()
    );
    assert_eq!(profile.quality, fixture.profile.quality);
    assert_eq!(profile.start_hour, fixture.profile.start_hour);
    assert_eq!(
        profile.config.base_data_revision,
        fixture.profile.config.base_data_revision
    );
    assert_eq!(
        fs::read(profile.config.politics_file.as_ref().unwrap()).unwrap(),
        expected_politics
    );
    assert_eq!(tree_bytes(&backup.directory), original_copy);
    assert!(profile.checkpoint_present);
    for path in [
        profile.config.region_store.as_ref().unwrap(),
        &profile.config.save_path,
        &profile.config.cache_dir,
        profile.config.heightmap_manifest.as_ref().unwrap(),
        profile.config.politics_file.as_ref().unwrap(),
    ] {
        assert!(path.is_absolute() && path.starts_with(restored.profile_path.parent().unwrap()));
    }
    let recent = model::recent_worlds(&fixture.user_data);
    assert!(recent.1.is_empty());
    assert_eq!(recent.0.len(), 1);
    assert_eq!(recent.0[0].path, restored.profile_path);
    assert!(recent.0[0].has_checkpoint_file);
    let repeated = create_backup(select_world(&recent.0[0]).unwrap(), &fixture.user_data).unwrap();
    assert_ne!(repeated.directory, backup.directory);
    assert_eq!(tree_bytes(&backup.directory), original_copy);
    let report = serde_json::json!({
        "schema_version": 1, "source_generation": archive.source_generation,
        "nonempty_regions": counts.nonempty_regions, "paged_records": counts.paged_records,
        "global_records": counts.global_records, "players": counts.players,
        "living_bodies": counts.living_bodies, "resources": counts.resources,
        "original_world_and_inputs_removed": true, "complete_archive_exact": true,
        "source_identity_retained": true, "political_input_exact": true,
        "character_and_presentation_retained": true, "restored_profile_discoverable": true,
        "checkpoint_present": true, "runtime_paths_absolute_and_rehomed": true,
        "profile_published_after_complete_restore": true, "original_backup_unchanged": true,
        "recovered_world_can_be_backed_up_again": true, "repeat_backup_uses_new_directory": true
    });
    println!(
        "LAUNCHER_RECOVERY_REPORT {}",
        serde_json::to_string(&report).unwrap()
    );
}

#[test]
fn stale_record_or_changed_checkpoint_cannot_retarget_a_review() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let review = review_backup(backup.clone()).unwrap();
    let before = model::recent_worlds(&fixture.user_data).0.len();
    let mut changed = backup.record.clone();
    changed.title = "Other valley".into();
    fs::write(
        backup.directory.join(RECORD),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        restore_reviewed(review, &fixture.user_data)
            .unwrap_err()
            .contains("changed")
    );
    assert_eq!(model::recent_worlds(&fixture.user_data).0.len(), before);
    changed.source_generation += 1;
    fs::write(
        backup.directory.join(RECORD),
        serde_json::to_vec_pretty(&changed).unwrap(),
    )
    .unwrap();
    assert!(
        review_backup(read_backup(&backup.directory).unwrap())
            .unwrap_err()
            .contains("checkpoint")
    );
    assert_eq!(model::recent_worlds(&fixture.user_data).0.len(), before);
}

#[test]
fn damaged_bundle_or_invalid_external_metadata_never_publishes_a_world() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let original_record = fs::read(backup.directory.join(RECORD)).unwrap();
    for invalid in [
        serde_json::json!({"title":""}),
        serde_json::json!({"character":{"name":"Ari","body_style":255,"skin":0,"hair":0,"outfit":0}}),
    ] {
        let mut value = serde_json::to_value(&backup.record).unwrap();
        for (key, value_to_set) in invalid.as_object().unwrap() {
            value[key] = value_to_set.clone();
        }
        fs::write(
            backup.directory.join(RECORD),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert!(read_backup(&backup.directory).is_err());
    }
    fs::write(backup.directory.join(RECORD), original_record).unwrap();
    let review = review_backup(read_backup(&backup.directory).unwrap()).unwrap();
    let output = root.path.join("recovered-player");
    fs::write(backup.directory.join(BUNDLE).join("world.db"), b"damaged").unwrap();
    assert!(restore_reviewed(review, &output).is_err());
    assert!(!output.exists());
    assert!(model::recent_worlds(&output).0.is_empty());
    assert!(WorldProfile::read(&fixture.profile_path).is_ok());
}

#[test]
fn backup_selection_is_exact_and_unsaved_or_unsupported_worlds_do_not_copy() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let selected = select(&fixture);
    let mut profile = fixture.profile.clone();
    profile.character.outfit = 2;
    model::atomic_json(&fixture.profile_path, &profile).unwrap();
    assert!(
        create_backup(selected, &fixture.user_data)
            .unwrap_err()
            .contains("changed")
    );
    assert!(!fixture.user_data.join("backups").exists());
    let mut recent = model::recent_worlds(&fixture.user_data).0.remove(0);
    recent.has_checkpoint_file = false;
    assert!(select_world(&recent).is_err());
    recent.has_checkpoint_file = true;
    recent.profile.config.region_store = None;
    assert!(select_world(&recent).is_err());
    recent.profile = fixture.profile.clone();
    recent.profile.config.live_data = true;
    assert!(select_world(&recent).is_err());
    assert!(!fixture.user_data.join("backups").exists());
}

#[test]
fn recovery_output_cannot_occupy_a_future_journal_or_live_inside_a_backup() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let database = fixture.profile.config.region_store.as_ref().unwrap();
    let journal = PathBuf::from(format!("{}-wal", database.display()));
    assert!(!journal.exists());
    assert!(create_backup(select(&fixture), &journal).is_err());
    assert!(!journal.exists());
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let before = tree_bytes(&backup.directory);
    let review = review_backup(backup.clone()).unwrap();
    assert!(restore_reviewed(review, &backup.directory.join("nested-player")).is_err());
    assert_eq!(tree_bytes(&backup.directory), before);
}

#[test]
fn incomplete_extra_or_linked_backup_content_is_rejected_without_cleanup_of_existing_data() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let extra = backup.directory.join("keep-me.txt");
    fs::write(&extra, b"retained").unwrap();
    assert!(read_backup(&backup.directory).is_err());
    assert_eq!(fs::read(&extra).unwrap(), b"retained");
    fs::remove_file(extra).unwrap();
    #[cfg(unix)]
    {
        let alias = root.path.join("alias");
        std::os::unix::fs::symlink(&backup.directory, &alias).unwrap();
        assert!(read_backup(&alias).is_err());
        let linked = root.path.join("metadata-link");
        fs::hard_link(backup.directory.join(RECORD), &linked).unwrap();
        assert!(read_backup(&backup.directory).is_err());
        fs::remove_file(linked).unwrap();
    }
    fs::remove_file(backup.directory.join(RECORD)).unwrap();
    assert!(read_backup(&backup.directory).is_err());
    let listed = recent_backups(&fixture.user_data);
    assert!(listed.0.is_empty() && !listed.1.is_empty());
    assert!(backup.directory.join(BUNDLE).exists());
}

#[test]
fn politics_free_recovery_remains_explicitly_free_of_a_political_source() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let restored = restore_reviewed(review_backup(backup).unwrap(), &fixture.user_data).unwrap();
    let profile = WorldProfile::read(&restored.profile_path).unwrap();
    assert!(profile.config.politics_file.is_none());
    assert!(profile.config.political_dataset_revision.is_none());
    assert_eq!(
        serde_json::to_vec(
            &RegionStore::open_read_only(profile.config.region_store.unwrap())
                .unwrap()
                .archive()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_vec(&fixture.archive).unwrap()
    );
}

fn menu_state(user_data: &Path) -> super::super::MenuState {
    use super::super::{MenuState, Screen};
    MenuState {
        project_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        user_data: user_data.to_owned(),
        screen: Screen::Worlds,
        draft: Default::default(),
        playtest_reports: false,
        report_summary: String::new(),
        recent: model::recent_worlds(user_data).0,
        backups: Vec::new(),
        completed_world: None,
        completed_backup: None,
        listing_revision: 7,
        recovery_context: 3,
        review_revision: 5,
        recovery_review: None,
        rename_selection: None,
        world_name_draft: String::new(),
        backup_path: String::new(),
        recovery_progress: None,
        notice: None,
        warning: None,
        error: None,
        dirty: false,
        focus: None,
        preview_angle: 0.,
        output: std::sync::Arc::new(std::sync::Mutex::new(None)),
        exit_frames: None,
        screenshot: None,
        frames: 0,
    }
}

#[test]
fn queued_world_selections_and_cancelled_restore_confirmation_do_not_retarget() {
    use super::super::{
        Action, Actions, MenuState, RecoveryOutcome, RecoveryWorker, Screen, process_actions,
    };
    use bevy::prelude::*;
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let review = review_backup(backup).unwrap();
    let mut state = menu_state(&fixture.user_data);
    state.recovery_review = Some(review);
    state.screen = Screen::RecoveryReview;
    let mut app = App::new();
    app.insert_resource(state)
        .init_resource::<Actions>()
        .init_resource::<RecoveryWorker>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<crate::player_input::InputSettings>()
        .init_resource::<crate::player_input::PlayerInput>()
        .init_resource::<crate::accessibility::ControllerKeyboard>()
        .add_message::<bevy::app::AppExit>()
        .add_systems(Update, process_actions);
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .extend([Action::Home, Action::RestoreBackup(5)]);
    app.update();
    assert_eq!(app.world().resource::<MenuState>().screen, Screen::Home);
    assert!(app.world().resource::<RecoveryWorker>().job.is_none());
    assert!(
        app.world()
            .resource::<MenuState>()
            .recovery_review
            .is_none()
    );
    assert!(
        app.world()
            .resource::<MenuState>()
            .error
            .as_ref()
            .unwrap()
            .contains("review changed")
    );
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .extend([Action::Continue(0, 6), Action::BackupWorld(0, 6)]);
    app.update();
    assert!(
        app.world()
            .resource::<MenuState>()
            .output
            .lock()
            .unwrap()
            .is_none()
    );
    assert!(app.world().resource::<RecoveryWorker>().job.is_none());
    assert!(
        app.world()
            .resource::<MenuState>()
            .error
            .as_ref()
            .unwrap()
            .contains("list changed")
    );
    assert_eq!(model::recent_worlds(&fixture.user_data).0.len(), 1);
    assert_eq!(recent_backups(&fixture.user_data).0.len(), 1);

    // The typed copy name must survive the native confirmation handoff, not
    // merely the lower-level restore helper. Progress names that destination.
    let mut review = review_backup(recent_backups(&fixture.user_data).0.remove(0)).unwrap();
    let source_title = review.backup.record.title.clone();
    review.restored_title = "  Valley for the next journey  ".into();
    let revision = {
        let mut menu = app.world_mut().resource_mut::<MenuState>();
        menu.screen = Screen::RecoveryReview;
        menu.recovery_review = Some(review);
        menu.review_revision += 1;
        menu.review_revision
    };
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::RestoreBackup(revision));
    app.update();
    let progress = app
        .world()
        .resource::<MenuState>()
        .recovery_progress
        .as_ref()
        .unwrap();
    assert!(progress.starts_with("Recovering Valley for the next journey · from "));
    assert!(progress.contains(&source_title));
    assert!(progress.contains("checkpoint"));
    let outcome = app
        .world_mut()
        .resource_mut::<RecoveryWorker>()
        .job
        .take()
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
    let RecoveryOutcome::Restored(restored) = outcome else {
        panic!("restored copy result");
    };
    assert_eq!(restored.title, "Valley for the next journey");
    assert_eq!(
        WorldProfile::read(&restored.profile_path).unwrap().title,
        restored.title
    );
    assert!(
        app.world()
            .resource::<MenuState>()
            .output
            .lock()
            .unwrap()
            .is_none()
    );
}

#[test]
fn one_background_operation_is_bounded_and_failure_restores_menu_actions() {
    use super::super::{MenuState, RecoveryWorker, poll_recovery, start_recovery};
    use bevy::prelude::*;
    let root = Scratch::new(false);
    let mut menu = menu_state(&root.path);
    let mut worker = RecoveryWorker::default();
    let (send, receive) = std::sync::mpsc::channel();
    start_recovery(
        &mut menu,
        &mut worker,
        "Checking backup…".into(),
        move || {
            receive
                .recv_timeout(std::time::Duration::from_secs(2))
                .map_err(|e| e.to_string())?;
            Err("Recorded verification failure".into())
        },
    )
    .unwrap();
    assert!(
        start_recovery(
            &mut menu,
            &mut worker,
            "Second operation".into(),
            || panic!("second job must not start")
        )
        .is_err()
    );
    assert!(menu.recovery_progress.is_some());
    let mut app = App::new();
    app.insert_resource(menu)
        .insert_resource(worker)
        .add_systems(Update, poll_recovery);
    app.update();
    assert!(app.world().resource::<RecoveryWorker>().job.is_some());
    send.send(()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.world().resource::<RecoveryWorker>().job.is_some()
        && std::time::Instant::now() < deadline
    {
        std::thread::yield_now();
        app.update();
    }
    assert!(app.world().resource::<RecoveryWorker>().job.is_none());
    assert!(
        app.world()
            .resource::<MenuState>()
            .recovery_progress
            .is_none()
    );
    assert_eq!(
        app.world().resource::<MenuState>().error.as_deref(),
        Some("Recorded verification failure")
    );
}

#[test]
fn completed_verification_does_not_reopen_a_review_after_navigation_away() {
    use super::super::{MenuState, RecoveryOutcome, RecoveryWorker, Screen, poll_recovery};
    use bevy::prelude::*;
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let review = review_backup(backup).unwrap();
    let mut menu = menu_state(&fixture.user_data);
    menu.screen = Screen::Home;
    menu.recovery_context = 4;
    menu.recovery_progress = Some("Verifying…".into());
    let worker = RecoveryWorker {
        job: Some(std::thread::spawn(move || {
            Ok(RecoveryOutcome::Reviewed(Box::new(review)))
        })),
        context: 3,
    };
    let mut app = App::new();
    app.insert_resource(menu)
        .insert_resource(worker)
        .add_systems(Update, poll_recovery);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.world().resource::<RecoveryWorker>().job.is_some()
        && std::time::Instant::now() < deadline
    {
        std::thread::yield_now();
        app.update();
    }
    let menu = app.world().resource::<MenuState>();
    assert_eq!(menu.screen, Screen::Home);
    assert!(menu.recovery_review.is_none() && menu.recovery_progress.is_none());
    assert!(menu.notice.as_ref().unwrap().contains("Select it again"));
}

#[test]
fn completed_backup_and_world_stay_reachable_after_bounded_list_refresh() {
    use super::super::{
        Action, Actions, MenuState, RecoveryOutcome, RecoveryWorker, poll_recovery, process_actions,
    };
    use bevy::prelude::*;
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let backup_path = backup.directory.clone();
    let original_copy = tree_bytes(&backup.directory);
    let future = now().unwrap() + 86_400;
    for index in 0..24 {
        let world_dir = fixture
            .user_data
            .join("worlds")
            .join(format!("future-{index}"));
        model::private_directory(&world_dir).unwrap();
        let mut profile = fixture.profile.clone();
        profile.title = format!("Future world {index}");
        profile.created_unix_seconds = future;
        profile.last_played_unix_seconds = future;
        model::write_new_json(&world_dir.join("world.json"), &profile).unwrap();
        let copy = fixture
            .user_data
            .join("backups")
            .join(format!("future-{index}"));
        model::private_directory(&copy).unwrap();
        for (relative, bytes) in &original_copy {
            let path = copy.join(relative);
            model::private_directory(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            model::private_permissions(&path, false).unwrap();
        }
        let mut record = backup.record.clone();
        record.created_unix_seconds = future;
        record.title = format!("Future backup {index}");
        fs::write(
            copy.join(RECORD),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(recent_backups(&fixture.user_data).0.len(), 24);
    assert!(
        !recent_backups(&fixture.user_data)
            .0
            .iter()
            .any(|item| item.directory == backup_path)
    );
    let restored =
        restore_reviewed(review_backup(backup.clone()).unwrap(), &fixture.user_data).unwrap();
    let restored_path = restored.profile_path.clone();
    assert!(
        !model::recent_worlds(&fixture.user_data)
            .0
            .iter()
            .any(|world| world.path == restored_path)
    );
    let menu = menu_state(&fixture.user_data);
    let worker = RecoveryWorker {
        context: menu.recovery_context,
        job: Some(std::thread::spawn(move || {
            Ok(RecoveryOutcome::Created(backup))
        })),
    };
    let mut app = App::new();
    app.insert_resource(menu)
        .insert_resource(worker)
        .init_resource::<Actions>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<crate::player_input::InputSettings>()
        .init_resource::<crate::player_input::PlayerInput>()
        .init_resource::<crate::accessibility::ControllerKeyboard>()
        .add_message::<bevy::app::AppExit>()
        .add_systems(Update, (poll_recovery, process_actions).chain());
    let wait = |app: &mut App| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.world().resource::<RecoveryWorker>().job.is_some()
            && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
            app.update();
        }
        assert!(app.world().resource::<RecoveryWorker>().job.is_none());
    };
    wait(&mut app);
    app.world_mut().resource_mut::<Actions>().0.extend([
        Action::Worlds,
        Action::Backups,
        Action::RefreshBackups,
    ]);
    app.update();
    assert_eq!(
        app.world().resource::<MenuState>().backups[0].directory,
        backup_path
    );
    assert_eq!(app.world().resource::<MenuState>().backups.len(), 24);
    let context = app.world().resource::<MenuState>().recovery_context;
    *app.world_mut().resource_mut::<RecoveryWorker>() = RecoveryWorker {
        context,
        job: Some(std::thread::spawn(move || {
            Ok(RecoveryOutcome::Restored(restored))
        })),
    };
    wait(&mut app);
    app.world_mut().resource_mut::<Actions>().0.extend([
        Action::Backups,
        Action::Worlds,
        Action::Refresh,
    ]);
    app.update();
    let menu = app.world().resource::<MenuState>();
    assert_eq!(menu.recent.len(), 24);
    assert_eq!(menu.recent[0].path, restored_path);
    assert!(menu.recent[0].has_checkpoint_file && menu.recent[0].profile.checkpoint_present);
    assert_eq!(tree_bytes(&backup_path), original_copy);
}

#[test]
fn named_recovery_copies_and_rename_preserve_progress_and_survive_relisting() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let backup = create_backup(select(&fixture), &fixture.user_data).unwrap();
    let original_backup = tree_bytes(&backup.directory);
    let mut first = review_backup(backup.clone()).unwrap();
    first.restored_title = "  Valley · before the journey  ".into();
    let first = restore_reviewed(first, &fixture.user_data).unwrap();
    let mut second = review_backup(backup.clone()).unwrap();
    second.restored_title = "Valley · new beginning".into();
    let second = restore_reviewed(second, &fixture.user_data).unwrap();
    assert_ne!(first.profile_path, second.profile_path);
    assert_eq!(first.title, "Valley · before the journey");
    // A clock with only second precision cannot serve as copy identity.
    let first_profile = WorldProfile::read(&first.profile_path).unwrap();
    let mut second_profile = WorldProfile::read(&second.profile_path).unwrap();
    second_profile.created_unix_seconds = first_profile.created_unix_seconds;
    model::atomic_json(&second.profile_path, &second_profile).unwrap();
    let selected = model::recent_worlds(&fixture.user_data)
        .0
        .into_iter()
        .find(|world| world.path == second.profile_path)
        .unwrap();
    let before = tree_bytes(&fixture.user_data);
    let title = "山と川の村 · Ari's working world";
    assert_eq!(
        rename_world(&selected, &fixture.user_data, title).unwrap(),
        title
    );
    let after = tree_bytes(&fixture.user_data);
    let changed: Vec<_> = before
        .iter()
        .filter(|(path, bytes)| after.get(*path) != Some(*bytes))
        .map(|(path, _)| path.clone())
        .collect();
    assert_eq!(
        changed,
        vec![
            second
                .profile_path
                .strip_prefix(&fixture.user_data)
                .unwrap()
                .to_path_buf()
        ]
    );
    let lease_path = model::managed_session_lease_path(&second.profile_path)
        .strip_prefix(&fixture.user_data)
        .unwrap()
        .to_owned();
    let added: Vec<_> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .cloned()
        .collect();
    assert_eq!(added, vec![lease_path]);
    assert_eq!(before.len() + 1, after.len());
    let mut expected = second_profile;
    expected.title = title.into();
    assert_eq!(
        serde_json::to_value(WorldProfile::read(&second.profile_path).unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    // The list is reconstructed as on a later launch, without a completed pin.
    let listed = model::recent_worlds(&fixture.user_data).0;
    assert!(
        listed
            .iter()
            .any(|world| world.path == first.profile_path && world.profile.title == first.title)
    );
    assert!(listed.iter().any(|world| world.path == second.profile_path
        && world.profile.title == title
        && world.has_checkpoint_file));
    assert_eq!(tree_bytes(&backup.directory), original_backup);
    let mut invalid = review_backup(backup).unwrap();
    invalid.restored_title = " ".into();
    let before = tree_bytes(&fixture.user_data);
    assert!(restore_reviewed(invalid, &fixture.user_data).is_err());
    assert_eq!(tree_bytes(&fixture.user_data), before);
    println!(
        "WORLD_NAMES_REPORT {}",
        serde_json::json!({
            "case": "named_recovery_copies_and_metadata_only_rename",
            "restored_copies": 2,
            "equal_creation_timestamps": true,
            "files_changed_by_rename": 1,
            "coordination_markers_created": 1,
            "scope": "Managed local checkpoint copies and profile naming; complete byte comparison of retained world and backup trees. No offsite or unsupported-source recovery claim.",
            "verification": {
                "separate_world_folders": true,
                "distinct_chosen_names_after_relisting": true,
                "only_selected_profile_changed": true,
                "only_title_field_changed": true,
                "progress_source_and_backup_bytes_exact": true,
                "invalid_recovery_name_rejected_before_output": true
            }
        })
    );
}

#[test]
fn rename_rejects_changed_profile_invalid_name_and_nonmanaged_target() {
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let selected = model::recent_worlds(&fixture.user_data).0.remove(0);
    let before = tree_bytes(&fixture.user_data);
    for name in ["", "  ", "bad\nname", &"界".repeat(49)] {
        assert!(rename_world(&selected, &fixture.user_data, name).is_err());
        assert_eq!(tree_bytes(&fixture.user_data), before);
    }
    let mut changed = selected.profile.clone();
    changed.start_hour = (changed.start_hour + 1) % 24;
    model::atomic_json(&selected.path, &changed).unwrap();
    let before = tree_bytes(&fixture.user_data);
    assert!(
        rename_world(&selected, &fixture.user_data, "Stale title")
            .unwrap_err()
            .contains("changed")
    );
    assert_eq!(tree_bytes(&fixture.user_data), before);
    let external = root.path.join("external");
    model::private_directory(&external).unwrap();
    let outside = external.join("world.json");
    model::write_new_json(&outside, &changed).unwrap();
    let outside_world = model::RecentWorld {
        path: outside.clone(),
        profile: changed,
        has_checkpoint_file: true,
    };
    let retained = fs::read(&outside).unwrap();
    assert!(
        rename_world(&outside_world, &fixture.user_data, "Wrong target")
            .unwrap_err()
            .contains("managed world")
    );
    assert_eq!(fs::read(outside).unwrap(), retained);
}

#[cfg(unix)]
#[test]
fn rename_rejects_symlink_and_hardlink_profile_aliases() {
    use std::os::unix::fs::symlink;
    let root = Scratch::new(false);
    let fixture = fixture(&root, false);
    let selected = model::recent_worlds(&fixture.user_data).0.remove(0);
    let retained = fs::read(&selected.path).unwrap();
    let alias = root.path.join("profile-alias.json");
    fs::hard_link(&selected.path, &alias).unwrap();
    assert!(rename_world(&selected, &fixture.user_data, "Alias").is_err());
    assert_eq!(fs::read(&selected.path).unwrap(), retained);
    fs::remove_file(&alias).unwrap();
    let link_dir = fixture.user_data.join("worlds/link");
    symlink(selected.path.parent().unwrap(), &link_dir).unwrap();
    let mut linked = selected.clone();
    linked.path = link_dir.join("world.json");
    assert!(rename_world(&linked, &fixture.user_data, "Symlink").is_err());
    assert_eq!(fs::read(&selected.path).unwrap(), retained);
}

#[test]
fn cancelled_or_stale_rename_cannot_mutate_and_new_world_stays_unplayed() {
    use super::super::{Action, Actions, MenuState, RecoveryWorker, Screen, process_actions};
    use bevy::prelude::*;
    let root = Scratch::new(false);
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let user_data = root.path.join("player-data");
    let profile_path = model::create_world(&model::Draft::default(), &project, &user_data).unwrap();
    let initial = fs::read(&profile_path).unwrap();
    let mut app = App::new();
    app.insert_resource(menu_state(&user_data))
        .init_resource::<Actions>()
        .init_resource::<RecoveryWorker>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<crate::player_input::InputSettings>()
        .init_resource::<crate::player_input::PlayerInput>()
        .init_resource::<crate::accessibility::ControllerKeyboard>()
        .add_message::<bevy::app::AppExit>()
        .add_systems(Update, process_actions);
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::RenameWorld(0, 7));
    app.update();
    let revision = app.world().resource::<MenuState>().review_revision;
    app.world_mut().resource_mut::<MenuState>().world_name_draft = "Cancelled".into();
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .extend([Action::Worlds, Action::SaveWorldName(revision)]);
    app.update();
    assert_eq!(fs::read(&profile_path).unwrap(), initial);
    assert_eq!(app.world().resource::<MenuState>().screen, Screen::Worlds);
    let listing = app.world().resource::<MenuState>().listing_revision;
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::RenameWorld(0, listing));
    app.update();
    app.world_mut().resource_mut::<MenuState>().world_name_draft = "My first outing".into();
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::SaveWorldName(revision));
    app.update();
    assert_eq!(fs::read(&profile_path).unwrap(), initial);
    let current = app.world().resource::<MenuState>().review_revision;
    app.world_mut()
        .resource_mut::<Actions>()
        .0
        .push_back(Action::SaveWorldName(current));
    app.update();
    let menu = app.world().resource::<MenuState>();
    assert_eq!(menu.screen, Screen::Worlds);
    assert_eq!(menu.recent[0].profile.title, "My first outing");
    assert!(!menu.recent[0].has_checkpoint_file);
    assert!(!menu.recent[0].profile.checkpoint_present);
    assert_eq!(menu.focus, Some(Action::Continue(0, menu.listing_revision)));
    assert!(menu.output.lock().unwrap().is_none());
    assert!(unavailable(&menu.recent[0]).is_some());
}
