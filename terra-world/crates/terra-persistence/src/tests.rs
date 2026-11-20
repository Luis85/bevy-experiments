use super::*;
use serde_json::json;
use terra_core::GeoPosition;

fn near() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 100.).unwrap()
}
fn far() -> GeoPosition {
    GeoPosition::new(48.2, 11.7, 140.).unwrap()
}
fn fixture() -> (SandboxState, Catalog) {
    let catalog = Catalog::builtin();
    let mut state = SandboxState::new();
    state.add_player(1, near()).unwrap();
    state.seed_living_demo(near(), &[], &catalog).unwrap();
    state
        .seed_resource("near-deposit".into(), near(), "stone", 25, 0.)
        .unwrap();
    state
        .seed_resource("far-deposit".into(), far(), "stone", 37, 0.)
        .unwrap();
    state
        .resource_nodes
        .get_mut("far-deposit")
        .unwrap()
        .remaining = 3;
    state.validate_with_catalog(&catalog).unwrap();
    (state, catalog)
}
fn identity() -> Value {
    json!({"kind":"store-test","source":"fixture-1","epoch":1700000000})
}
fn open(path: &Path, catalog: &Catalog) -> RegionStore {
    RegionStore::open(path, identity(), catalog, 16).unwrap()
}
fn near_set() -> BTreeSet<RegionKey> {
    BTreeSet::from([RegionKey::at(near(), 16).unwrap()])
}

#[test]
fn uncertain_backup_publication_preserves_a_verifiable_complete_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let destination = directory.path().join("backup.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock": 42}), &near_set())
        .unwrap();
    let expected = serde_json::to_value(store.archive().unwrap()).unwrap();
    let error = store
        .backup_with_options_and_sync(&destination, RegionBackupOptions::default(), |_| {
            Err(std::io::Error::other("injected backup parent-sync failure"))
        })
        .unwrap_err();
    assert!(matches!(&error, StoreError::Publication(error) if error.published()));
    let backup = RegionStore::open_read_only(&destination).unwrap();
    assert_eq!(
        serde_json::to_value(backup.archive().unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::to_value(store.archive().unwrap()).unwrap(),
        expected
    );
    assert!(
        store.backup_to(&destination).is_err(),
        "an uncertain backup must never be silently replaced"
    );
}

#[test]
fn bounded_worker_keeps_visible_state_and_polling_responsive_while_sqlite_is_locked() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    let before = active.clone();
    let blocker = Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut worker = RegionWorker::new(store).unwrap();
    let start = std::time::Instant::now();
    let id = worker
        .submit_checkpoint(active.clone(), json!({"clock":2}), near_set())
        .unwrap();
    assert!(worker.pending());
    assert!(matches!(
        worker.submit_hydrate(active.clone()),
        Err(StoreError::Busy)
    ));
    assert!(matches!(worker.export(&active), Err(StoreError::Busy)));
    assert!(matches!(
        worker.with_store(|_| Ok(())),
        Err(StoreError::Busy)
    ));
    assert!(worker.try_complete().unwrap().is_none());
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "submission/poll must not wait through SQLite's write-lock timeout"
    );
    assert_eq!(
        active, before,
        "host keeps its renderable authoritative revision"
    );
    blocker.execute_batch("ROLLBACK").unwrap();
    let completed = worker.finish().unwrap().unwrap();
    assert_eq!(completed.id, id);
    assert_eq!(completed.kind, RegionWorkKind::Checkpoint);
    assert!(completed.error.is_none(), "{:?}", completed.error);
    assert_eq!(completed.status.generation, 2);
    assert!(!worker.pending());
    assert!(
        !completed
            .state
            .unwrap()
            .resource_nodes
            .contains_key("far-deposit")
    );
    assert_eq!(
        worker
            .with_store(|store| Ok(store.restore()?.unwrap().metadata))
            .unwrap(),
        json!({"clock":2})
    );
}

#[test]
fn worker_failure_preserves_state_and_generation_then_retry_commits() {
    let directory = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&directory.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":1}), &near_set())
        .unwrap();
    let mut worker = RegionWorker::new(store).unwrap();
    worker.with_store(|store| { store.connection.execute_batch("CREATE TEMP TRIGGER fail BEFORE UPDATE ON world BEGIN SELECT RAISE(ABORT,'test failure'); END;")?; Ok(()) }).unwrap();
    let before = state.clone();
    worker
        .submit_checkpoint(state.clone(), json!({"clock":2}), near_set())
        .unwrap();
    let failed = worker.finish().unwrap().unwrap();
    assert!(failed.error.is_some());
    assert!(failed.state.is_none());
    assert_eq!(failed.status.generation, 1);
    assert_eq!(state, before);
    worker
        .with_store(|store| {
            store.connection.execute_batch("DROP TRIGGER fail")?;
            Ok(())
        })
        .unwrap();
    worker
        .submit_checkpoint(state, json!({"clock":3}), near_set())
        .unwrap();
    let retry = worker.finish().unwrap().unwrap();
    assert!(retry.error.is_none());
    assert_eq!(retry.status.generation, 2);
}

#[test]
fn worker_drop_drains_submitted_checkpoint_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (state, catalog) = fixture();
    let mut worker = RegionWorker::new(open(&path, &catalog)).unwrap();
    worker
        .submit_checkpoint(state, json!({"final":true}), near_set())
        .unwrap();
    drop(worker);
    let restored = open(&path, &catalog).restore().unwrap().unwrap();
    assert_eq!(restored.generation, 1);
    assert_eq!(restored.metadata, json!({"final":true}));
}

#[test]
fn verified_partition_cache_skips_repeat_decode_but_not_external_corruption() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    store.export(&state).unwrap();
    store.export(&state).unwrap();
    let stats = store.cache_stats();
    assert_eq!(
        stats.decoded_checkpoints, 0,
        "new commits already supply a verified canonical partition"
    );
    assert_eq!(stats.hits, 2);
    let other = Connection::open(&path).unwrap();
    other
        .execute(
            "UPDATE regions SET payload=?1 WHERE rowid=(SELECT rowid FROM regions LIMIT 1)",
            params![b"{}".as_slice()],
        )
        .unwrap();
    assert!(
        store.export(&state).is_err(),
        "another connection's same-generation corruption must invalidate the cache"
    );
}

#[test]
fn owned_legacy_courier_retires_with_exact_cargo_and_resumes_without_offline_retries() {
    use terra_sandbox::{Delivery, DeliveryPhase, UnitOrder, UnitRole};
    for phase in [
        DeliveryPhase::ToPickup,
        DeliveryPhase::ToDropoff,
        DeliveryPhase::Returning,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("courier.db");
        let (mut active, catalog) = fixture();
        let courier = active
            .living
            .units
            .values()
            .find(|u| u.role == UnitRole::Hauler)
            .unwrap()
            .id;
        active.living.units.get_mut(&courier).unwrap().owner = Some(1);
        active.living.units.retain(|_, u| u.owner.is_some());
        for site in active.living.producers.values_mut() {
            site.enabled = false;
        }
        let source = active
            .living
            .producers
            .values()
            .find(|p| p.outputs.contains_key("water"))
            .unwrap()
            .id;
        let target = active
            .living
            .producers
            .values()
            .find(|p| p.id != source)
            .unwrap()
            .id;
        if phase != DeliveryPhase::ToPickup {
            *active
                .living
                .producers
                .get_mut(&source)
                .unwrap()
                .stock
                .get_mut("water")
                .unwrap() -= 4;
        }
        let destination = if phase == DeliveryPhase::ToDropoff {
            active.living.producers[&target].position
        } else {
            active.living.producers[&source].position
        };
        let body = active.living.units.get_mut(&courier).unwrap();
        body.position = far();
        body.home = far();
        body.order = UnitOrder::Move { destination };
        body.path = vec![destination];
        body.path_index = 0;
        body.supplies.clear();
        if phase != DeliveryPhase::ToPickup {
            body.supplies.insert("water".into(), 4);
        }
        let delivery = Delivery {
            id: active.living.next_id,
            unit_id: courier,
            from_site: source,
            to_site: target,
            item: "water".into(),
            amount: 4,
            phase,
            stalled_seconds: 7.,
            route_retries: 1,
            last_position: Some(far()),
        };
        active.living.next_id += 1;
        active.living.deliveries = vec![delivery.clone()];
        active.set_player_dormant(1, true).unwrap();
        active.validate_with_catalog(&catalog).unwrap();
        let body = active.living.units[&courier].clone();
        let producers = active.living.producers.clone();
        let accounts = active.living.accounts.clone();
        // The same job also freezes while its body is still physically loaded.
        active.advance_living(5., 0., &catalog).unwrap();
        assert_eq!(active.living.units[&courier], body);
        assert_eq!(active.living.deliveries, vec![delivery.clone()]);
        let mut store = open(&path, &catalog);
        store
            .checkpoint(&mut active, json!({}), &near_set())
            .unwrap();
        assert!(!active.living.units.contains_key(&courier));
        active.validate_with_catalog(&catalog).unwrap();
        for _ in 0..400 {
            active.advance_living(5., 0., &catalog).unwrap();
        }
        assert_eq!(active.living.deliveries, vec![delivery.clone()]);
        assert_eq!(active.living.producers, producers);
        assert_eq!(active.living.accounts, accounts);
        let complete = store.export(&active).unwrap();
        assert_eq!(complete.living.units[&courier], body);

        if phase != DeliveryPhase::ToPickup {
            // A forged receipt and matching cold projection cannot launder
            // cargo: complete merge still checks the real stored body.
            let mut raw = serde_json::to_value(&active).unwrap();
            raw["living"]["deliveries"][0]["amount"] = json!(99);
            raw["dormant"]["owned_units"][courier.to_string()]["courier_supplies"] =
                json!({"water":99});
            let mut forged: SandboxState = serde_json::from_value(raw).unwrap();
            forged.validate_with_catalog(&catalog).unwrap();
            let before = forged.clone();
            let status = store.status();
            assert!(
                store
                    .checkpoint(&mut forged, json!({}), &near_set())
                    .is_err()
            );
            assert_eq!(forged, before);
            assert_eq!(store.status(), status);
            assert_eq!(store.export(&active).unwrap().living.units[&courier], body);
        }
        store
            .checkpoint(&mut active, json!({}), &near_set())
            .unwrap();
        drop(store);
        let mut restored = open(&path, &catalog).restore().unwrap().unwrap().state;
        assert_eq!(restored, complete);
        restored.set_player_dormant(1, false).unwrap();
        restored.advance_living(0.1, 0., &catalog).unwrap();
        let distance = body
            .position
            .surface_distance_m(restored.living.units[&courier].position);
        assert!(
            distance > 0.005 && distance < 2.,
            "bounded current-step motion after {phase:?}: {distance}"
        );
        assert_eq!(restored.living.deliveries[0].phase, phase);
        assert!(restored.living.deliveries[0].stalled_seconds < 1.);
        assert_eq!(restored.living.units[&courier].supplies, body.supplies);
        restored.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn dormant_resident_owned_courier_is_not_assigned_new_public_work() {
    use terra_sandbox::{Inventory, UnitRole};
    let (mut state, catalog) = fixture();
    let courier = state
        .living
        .units
        .values()
        .find(|u| u.role == UnitRole::Hauler)
        .unwrap()
        .id;
    state.living.units.get_mut(&courier).unwrap().owner = Some(1);
    state.living.units.retain(|_, u| u.owner.is_some());
    let source = state
        .living
        .producers
        .values()
        .find(|p| p.outputs.contains_key("water"))
        .unwrap()
        .id;
    let target = state
        .living
        .producers
        .values()
        .find(|p| p.id != source)
        .unwrap()
        .id;
    for site in state.living.producers.values_mut() {
        site.enabled = site.id == target;
    }
    state.living.producers.get_mut(&source).unwrap().stock = Inventory::from([("water".into(), 4)]);
    let site = state.living.producers.get_mut(&target).unwrap();
    site.stock.clear();
    site.inputs = Inventory::from([("water".into(), 1)]);
    state.set_player_dormant(1, true).unwrap();
    let body = state.living.units[&courier].clone();
    state.advance_living(1., 0., &catalog).unwrap();
    assert!(state.living.deliveries.is_empty());
    assert_eq!(state.living.units[&courier], body);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn same_generation_edit_after_snapshot_composition_cannot_be_overwritten() {
    use std::sync::{Mutex, mpsc};

    // A separate writer holds BEGIN IMMEDIATE until checkpoint reaches its
    // own write barrier. The busy handler makes the interleaving deterministic
    // without guessing how long composition or serialization will take.
    static WRITE_BARRIER: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);
    fn waiting_for_writer(attempt: i32) -> bool {
        if let Some(signal) = WRITE_BARRIER.lock().unwrap().as_ref() {
            let _ = signal.try_send(());
        }
        std::thread::sleep(Duration::from_millis(1));
        attempt < 5_000
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    active
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining = 0;
    let before = active.clone();
    let status_before = store.status();
    let other = Connection::open(&path).unwrap();
    let global_before: Vec<u8> = other
        .query_row("SELECT global_state FROM world", [], |r| r.get(0))
        .unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let (signal, waiting) = mpsc::sync_channel(1);
    *WRITE_BARRIER.lock().unwrap() = Some(signal);
    store
        .connection
        .busy_handler(Some(waiting_for_writer))
        .unwrap();
    let task = std::thread::spawn(move || {
        let result = store.checkpoint(&mut active, json!({"clock":2}), &near_set());
        (store, active, result)
    });
    waiting
        .recv_timeout(Duration::from_secs(5))
        .expect("checkpoint must compose before waiting for the write lock");
    other
        .execute("UPDATE regions SET digest='external-edit'", [])
        .unwrap();
    other.execute_batch("COMMIT").unwrap();
    let (store, active, result) = task.join().unwrap();
    *WRITE_BARRIER.lock().unwrap() = None;
    assert!(matches!(
        result,
        Err(StoreError::Invalid(message))
            if message.contains("changed while preparing checkpoint")
    ));
    assert_eq!(active, before);
    assert_eq!(store.status(), status_before);
    assert_eq!(read_generation(&other).unwrap(), 1);
    assert_eq!(
        other
            .query_row::<Vec<u8>, _, _>("SELECT global_state FROM world", [], |r| r.get(0))
            .unwrap(),
        global_before
    );
    assert!(
        other
            .query_row::<bool, _, _>(
                "SELECT COUNT(*)>0 AND MIN(digest='external-edit') FROM regions",
                [],
                |r| r.get(0)
            )
            .unwrap()
    );
    assert!(
        store.export(&active).is_err(),
        "external corruption must remain visible"
    );
}

#[test]
fn external_descriptor_edits_cannot_relabel_an_open_authority_or_its_archive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    let before = state.clone();
    let other = Connection::open(&path).unwrap();
    other
        .execute(
            "UPDATE world SET identity=?1",
            params![serde_json::to_vec(&json!({"source":"another-world"})).unwrap()],
        )
        .unwrap();
    assert!(matches!(
        store.export(&state),
        Err(StoreError::IdentityMismatch)
    ));
    assert!(matches!(store.archive(), Err(StoreError::IdentityMismatch)));
    assert!(matches!(
        store.checkpoint(&mut state, json!({}), &near_set()),
        Err(StoreError::IdentityMismatch)
    ));
    assert_eq!(state, before);
    assert_eq!(store.status().generation, 1);
    assert_eq!(read_generation(&other).unwrap(), 1);
}

#[test]
fn portable_operations_preserve_complete_private_world_and_refuse_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("world.db");
    let archive_path = directory.path().join("world.json");
    let destination = directory.path().join("imported.db");
    let (mut state, catalog) = fixture();
    let complete = state.clone();
    let mut store = open(&source, &catalog);
    store
        .checkpoint(
            &mut state,
            json!({"private_session":"secret","clock":7}),
            &near_set(),
        )
        .unwrap();
    let before = store.status();
    let report = store.inspect().unwrap();
    assert_eq!(report.generation, 1);
    assert!(report.domain_validated);
    assert!(!serde_json::to_string(&report).unwrap().contains("secret"));
    let archive = store.archive().unwrap();
    assert_eq!(archive.state, complete);
    assert_eq!(
        store.status(),
        before,
        "inspection/export must not move the active cursor"
    );
    archive.write_new(&archive_path).unwrap();
    assert!(archive.write_new(&archive_path).is_err());
    let archive = RegionArchive::read(&archive_path).unwrap();
    archive.import_new(&destination).unwrap();
    assert!(archive.import_new(&destination).is_err());
    let mut imported = RegionStore::open_existing(&destination).unwrap();
    let restored = imported.restore().unwrap().unwrap();
    assert_eq!(restored.state, complete);
    assert_eq!(
        restored.metadata,
        json!({"private_session":"secret","clock":7})
    );
    assert!(matches!(
        RegionStore::open(&destination, json!({"other":"world"}), &catalog, 16),
        Err(StoreError::IdentityMismatch)
    ));
    let absent = directory.path().join("absent.db");
    assert!(RegionStore::open_existing(&absent).is_err());
    assert!(!absent.exists());
    let orphan = directory.path().join("orphan.db-wal");
    fs::write(&orphan, "incident evidence").unwrap();
    assert!(
        archive
            .import_new(directory.path().join("orphan.db"))
            .is_err()
    );
    assert_eq!(fs::read_to_string(orphan).unwrap(), "incident evidence");
    let bad_destination = directory.path().join("bad.db");
    let mut invalid = archive;
    invalid
        .state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("unknown".into(), 1);
    assert!(invalid.import_new(&bad_destination).is_err());
    assert!(!bad_destination.exists());
}

#[test]
fn checkpoint_pages_real_records_and_restart_preserves_depletion_and_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let original = state.clone();
    let mut store = open(&path, &catalog);
    assert!(store.restore().unwrap().is_none());
    let status = store
        .checkpoint(
            &mut state,
            json!({"clock":1234,"private_session":"capability"}),
            &near_set(),
        )
        .unwrap();
    assert_eq!(status.generation, 1);
    assert!(status.dormant_regions > 0);
    assert!(!state.resource_nodes.contains_key("far-deposit"));
    assert_eq!(state.players, original.players);
    assert_eq!(state.living.accounts, original.living.accounts);
    assert_eq!(store.export(&state).unwrap(), original);
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("capability")
    );
    state.living.accounts.get_mut(&1).unwrap().money -= 17;
    let expected_money = state.living.accounts[&1].money;
    store
        .checkpoint(&mut state, json!({"clock":1240}), &near_set())
        .unwrap();
    drop(store);
    let mut reopened = open(&path, &catalog);
    let restored = reopened.restore().unwrap().unwrap();
    assert_eq!(restored.generation, 2);
    assert_eq!(restored.metadata, json!({"clock":1240}));
    assert_eq!(restored.state.resource_nodes["far-deposit"].remaining, 3);
    assert_eq!(restored.state.living.accounts[&1].money, expected_money);
}

#[test]
fn hydrate_before_registration_does_not_reseed_depleted_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    state
        .resource_nodes
        .get_mut("far-deposit")
        .unwrap()
        .remaining = 0;
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    assert!(!state.resource_nodes.contains_key("far-deposit"));
    store.hydrate(&mut state).unwrap();
    assert_eq!(state.resource_nodes["far-deposit"].remaining, 0);
    // Providers use the same existing-ID check before materializing a feature.
    state
        .seed_resource("far-deposit".into(), far(), "stone", 37, 0.)
        .unwrap_err();
    store
        .checkpoint(
            &mut state,
            json!({}),
            &BTreeSet::from([RegionKey::at(far(), 16).unwrap()]),
        )
        .unwrap();
    assert_eq!(state.resource_nodes["far-deposit"].remaining, 0);
}

#[test]
fn transaction_failure_rolls_back_pages_metadata_generation_and_live_state() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":1}), &near_set())
        .unwrap();
    let committed = store.export(&state).unwrap();
    state
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining = 1;
    let before = state.clone();
    let status = store.status();
    store.connection.execute_batch("CREATE TEMP TRIGGER reject_commit BEFORE UPDATE ON world BEGIN SELECT RAISE(ABORT,'injected disk transaction failure'); END;").unwrap();
    assert!(
        store
            .checkpoint(&mut state, json!({"clock":2}), &BTreeSet::new())
            .is_err()
    );
    assert_eq!(state, before);
    assert_eq!(store.status(), status);
    store
        .connection
        .execute_batch("DROP TRIGGER reject_commit;")
        .unwrap();
    let restored = store.restore().unwrap().unwrap();
    assert_eq!(restored.state, committed);
    assert_eq!(restored.metadata, json!({"clock":1}));
}

#[test]
fn stale_writer_cannot_overwrite_newer_committed_money_or_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut first = open(&path, &catalog);
    first
        .checkpoint(&mut state, json!({"owner":1}), &near_set())
        .unwrap();
    let mut second = open(&path, &catalog);
    let mut stale = second.restore().unwrap().unwrap().state;
    state.living.accounts.get_mut(&1).unwrap().money -= 100;
    first
        .checkpoint(&mut state, json!({"owner":1,"newer":true}), &near_set())
        .unwrap();
    let stale_before = stale.clone();
    assert!(matches!(
        second.checkpoint(&mut stale, json!({"owner":2}), &near_set()),
        Err(StoreError::Conflict {
            expected: 1,
            actual: 2
        })
    ));
    assert_eq!(stale, stale_before);
    assert!(matches!(
        second.hydrate(&mut stale),
        Err(StoreError::Conflict { .. })
    ));
    assert_eq!(
        second.restore().unwrap().unwrap().state,
        first.export(&state).unwrap()
    );
}

#[test]
fn a_committed_deletion_and_cross_region_move_do_not_resurrect_records() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    state.resource_nodes.remove("near-deposit");
    let moving = state
        .living
        .units
        .values_mut()
        .find(|u| u.owner.is_none() && u.role != terra_sandbox::UnitRole::Hauler)
        .unwrap();
    let id = moving.id;
    moving.position = far();
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    let all = store.export(&state).unwrap();
    assert!(!all.resource_nodes.contains_key("near-deposit"));
    assert_eq!(all.resource_nodes["far-deposit"].remaining, 3);
    assert_eq!(all.living.units[&id].position, far());
    store.hydrate(&mut state).unwrap();
    assert_eq!(state, all);
}

#[test]
fn foreign_world_catalog_level_and_schema_are_rejected_without_resetting_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut state, json!({"original":true}), &near_set())
        .unwrap();
    assert!(matches!(
        RegionStore::open(&path, json!({"other":"world"}), &catalog, 16),
        Err(StoreError::IdentityMismatch)
    ));
    assert!(matches!(
        RegionStore::open(&path, identity(), &catalog, 15),
        Err(StoreError::IdentityMismatch)
    ));
    let mut altered = catalog.clone();
    altered.items.insert("custom_item".into());
    assert!(matches!(
        RegionStore::open(&path, identity(), &altered, 16),
        Err(StoreError::IdentityMismatch)
    ));
    assert_eq!(
        store.restore().unwrap().unwrap().metadata,
        json!({"original":true})
    );
    let foreign = dir.path().join("foreign.db");
    let connection = Connection::open(&foreign).unwrap();
    connection
        .execute_batch("CREATE TABLE unrelated (id INTEGER);")
        .unwrap();
    drop(connection);
    assert!(RegionStore::open(&foreign, identity(), &catalog, 16).is_err());
    let connection = Connection::open(&foreign).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='unrelated'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn corrupted_payload_is_not_installed_or_silently_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    let before = state.clone();
    let status = store.status();
    store
        .connection
        .execute(
            "UPDATE regions SET payload=?1 WHERE rowid=(SELECT MIN(rowid) FROM regions)",
            params![b"{}".to_vec()],
        )
        .unwrap();
    assert!(store.hydrate(&mut state).is_err());
    assert_eq!(state, before);
    assert!(
        store
            .checkpoint(&mut state, json!({}), &near_set())
            .is_err()
    );
    assert!(store.restore().is_err());
    assert_eq!(store.status(), status);
}

#[test]
fn rejected_host_metadata_preserves_the_old_residency_cursor_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":1}), &near_set())
        .unwrap();
    let status = store.status();
    assert!(
        store
            .restore_validated(|_| Err("host rejected the saved clock".into()))
            .is_err()
    );
    assert_eq!(store.status(), status);
    // If restore had switched to all-active before host validation, this next
    // commit would discard the cold region when it merged the unchanged view.
    store
        .checkpoint(&mut state, json!({"clock":2}), &near_set())
        .unwrap();
    assert_eq!(
        store.export(&state).unwrap().resource_nodes["far-deposit"].remaining,
        3
    );
}

#[test]
fn opening_an_existing_store_requires_restore_before_mutating() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut first = open(&path, &catalog);
    first
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    let mut other = open(&path, &catalog);
    assert!(
        other
            .checkpoint(&mut state, json!({}), &near_set())
            .is_err()
    );
    assert!(other.hydrate(&mut state).is_err());
    assert_eq!(first.status().generation, 1);
}

#[test]
fn unexpected_sql_triggers_cannot_turn_a_successful_checkpoint_into_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut state, json!({"original":true}), &near_set())
        .unwrap();
    let expected = store.export(&state).unwrap();
    let before = state.clone();
    store.connection.execute_batch("CREATE TRIGGER destroy_pages AFTER UPDATE OF global_state ON world BEGIN DELETE FROM regions; END;").unwrap();
    assert!(RegionStore::open(&path, identity(), &catalog, 16).is_err());
    assert!(
        store
            .checkpoint(&mut state, json!({"unsafe":true}), &near_set())
            .is_err()
    );
    assert_eq!(state, before);
    assert_eq!(store.status().generation, 1);
    store
        .connection
        .execute_batch("DROP TRIGGER destroy_pages;")
        .unwrap();
    assert_eq!(store.restore().unwrap().unwrap().state, expected);
}

#[test]
fn altered_regional_schema_is_rejected_by_readers_and_live_checkpoint_barriers() {
    for declaration in [
        // Same visible column names, but no region-identity guarantee.
        "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL",
        // The supported composite key has a fixed ordinal contract.
        "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(x,level,y)",
        "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB, digest TEXT NOT NULL, PRIMARY KEY(level,x,y)",
        "level INTEGER NOT NULL, x TEXT NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(level,x,y)",
        // table_info alone hides this additional authoritative expression.
        "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, payload_size INTEGER GENERATED ALWAYS AS (length(payload)) VIRTUAL, PRIMARY KEY(level,x,y)",
        // Foreign-key cascades are not part of the application-owned schema.
        "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(level,x,y), FOREIGN KEY(level) REFERENCES world(singleton) ON DELETE CASCADE",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let (mut active, catalog) = fixture();
        let mut store = open(&path, &catalog);
        store
            .checkpoint(&mut active, json!({"clock":1}), &near_set())
            .unwrap();
        let before = active.clone();
        let expected_generation = store.status().generation;
        let editor = Connection::open(&path).unwrap();
        // An external schema editor can disable its own FK enforcement. The
        // malformed-schema case must reach our admission check rather than
        // failing while copying existing level-16 rows into the hostile table.
        editor.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        editor
            .execute_batch(&format!(
                "BEGIN IMMEDIATE;
                 ALTER TABLE regions RENAME TO regions_original;
                 CREATE TABLE regions ({declaration});
                 INSERT INTO regions(level,x,y,payload,digest)
                     SELECT level,x,y,payload,digest FROM regions_original;
                 DROP TABLE regions_original;
                 COMMIT;"
            ))
            .unwrap();
        let stored_rows: i64 = editor
            .query_row("SELECT COUNT(*) FROM regions", [], |r| r.get(0))
            .unwrap();
        assert!(RegionStore::open(&path, identity(), &catalog, 16).is_err());
        assert!(RegionStore::open_read_only(&path).is_err());
        assert!(RegionStore::open_existing(&path).is_err());
        assert!(store.inspect().is_err());
        assert!(store.archive().is_err());
        assert!(store.restore().is_err());
        assert!(
            store
                .checkpoint(&mut active, json!({"clock":2}), &near_set())
                .is_err()
        );
        assert_eq!(active, before);
        assert_eq!(store.status().generation, expected_generation);
        assert_eq!(read_generation(&editor).unwrap(), expected_generation);
        assert_eq!(
            editor
                .query_row("SELECT COUNT(*) FROM regions", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            stored_rows
        );
    }
}

#[test]
fn duplicate_regional_rows_cannot_be_collapsed_into_an_accepted_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    let before = active.clone();
    let editor = Connection::open(&path).unwrap();
    editor
        .execute_batch(
            "BEGIN IMMEDIATE;
             ALTER TABLE regions RENAME TO regions_original;
             CREATE TABLE regions (
                 level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL,
                 payload BLOB NOT NULL, digest TEXT NOT NULL
             );
             INSERT INTO regions SELECT * FROM regions_original;
             INSERT INTO regions SELECT * FROM regions_original LIMIT 1;
             UPDATE world SET payload_bytes=payload_bytes+
                 (SELECT length(payload) FROM regions_original LIMIT 1);
             DROP TABLE regions_original;
             COMMIT;",
        )
        .unwrap();
    let duplicate_keys: i64 = editor
        .query_row(
            "SELECT COUNT(*) FROM (SELECT level,x,y FROM regions GROUP BY level,x,y HAVING COUNT(*)>1)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(duplicate_keys, 1);
    assert!(RegionStore::open_read_only(&path).is_err());
    assert!(RegionStore::open(&path, identity(), &catalog, 16).is_err());
    assert!(store.export(&active).is_err());
    assert!(
        store
            .checkpoint(&mut active, json!({"clock":2}), &near_set())
            .is_err()
    );
    assert_eq!(active, before);
    assert_eq!(store.status().generation, 1);
    assert_eq!(read_generation(&editor).unwrap(), 1);
}

#[test]
fn extra_unique_replace_indexes_cannot_delete_regions_during_the_first_checkpoint() {
    for (table, declaration) in [
        (
            "regions",
            "level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL, payload BLOB NOT NULL, digest TEXT NOT NULL, PRIMARY KEY(level,x,y), UNIQUE(level) ON CONFLICT REPLACE",
        ),
        (
            "world",
            "singleton INTEGER PRIMARY KEY CHECK(singleton=1), identity BLOB NOT NULL, catalog BLOB NOT NULL, region_level INTEGER NOT NULL, generation INTEGER NOT NULL CHECK(generation>=0), global_state BLOB, metadata BLOB, digest TEXT, payload_bytes INTEGER NOT NULL DEFAULT 0, UNIQUE(region_level) ON CONFLICT REPLACE",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let (mut active, catalog) = fixture();
        // These records share one level but occupy distinct regional keys.
        assert!(
            RegionPartition::from_state(&active, 16, &catalog)
                .unwrap()
                .pages
                .len()
                > 1
        );
        let mut store = open(&path, &catalog);
        let before = active.clone();
        let editor = Connection::open(&path).unwrap();
        editor
            .execute_batch(&format!(
                "BEGIN IMMEDIATE;
                 ALTER TABLE {table} RENAME TO original_table;
                 CREATE TABLE {table} ({declaration});
                 INSERT INTO {table} SELECT * FROM original_table;
                 DROP TABLE original_table;
                 COMMIT;"
            ))
            .unwrap();
        assert!(RegionStore::open(&path, identity(), &catalog, 16).is_err());
        assert!(RegionStore::open_read_only(&path).is_err());
        assert!(
            store
                .checkpoint(&mut active, json!({"clock":1}), &near_set())
                .is_err()
        );
        assert_eq!(active, before);
        assert_eq!(store.status().generation, 0);
        assert_eq!(read_generation(&editor).unwrap(), 0);
        assert_eq!(
            editor
                .query_row("SELECT COUNT(*) FROM regions", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn decoded_duplicate_page_rejection_preserves_the_first_authoritative_page() {
    let (state, catalog) = fixture();
    let partition = RegionPartition::from_state(&state, 16, &catalog).unwrap();
    let first = partition.pages.values().next().unwrap().clone();
    let mut pages = BTreeMap::new();
    insert_decoded_page(&mut pages, first.clone()).unwrap();
    let before = pages.clone();
    let mut conflicting = first;
    conflicting
        .feature_edits
        .insert("duplicate-row-change".into());
    assert!(matches!(
        insert_decoded_page(&mut pages, conflicting),
        Err(StoreError::Invalid(error)) if error.contains("duplicate regional identity")
    ));
    assert_eq!(pages, before);
    // An independently keyed page is still admitted after the rejected row.
    let other = partition
        .pages
        .values()
        .find(|page| !pages.contains_key(&page.key))
        .unwrap()
        .clone();
    insert_decoded_page(&mut pages, other).unwrap();
    assert_eq!(pages.len(), 2);
}

#[test]
fn supported_schema_one_remains_readable_and_writable_after_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let backup = directory.path().join("backup.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    let expected = store.archive().unwrap();
    store.backup_to(&backup).unwrap();
    let reader = RegionStore::open_read_only(&backup).unwrap();
    assert_eq!(reader.archive().unwrap().state, expected.state);
    drop(reader);
    let mut reopened = RegionStore::open_existing(&backup).unwrap();
    let restored = reopened.restore().unwrap().unwrap();
    assert_eq!(restored.state, expected.state);
    assert_eq!(restored.metadata, expected.metadata);
    let mut complete = restored.state;
    reopened
        .checkpoint(&mut complete, json!({"clock":2}), &near_set())
        .unwrap();
    assert_eq!(reopened.status().generation, 2);
    assert_eq!(store.status().generation, 1);
    assert_eq!(store.archive().unwrap().state, expected.state);
}

#[test]
fn a_zero_generation_with_payload_is_corruption_not_a_new_world() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    store
        .connection
        .execute_batch("UPDATE world SET generation=0;")
        .unwrap();
    assert!(store.restore().is_err());
    assert_eq!(store.status().generation, 1);
}

#[test]
fn discarded_restore_result_cannot_make_hydrate_accept_an_old_partial_view() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    assert!(state.is_partial());
    let before = state.clone();
    let _unused_restored_world = store.restore().unwrap().unwrap();
    assert!(store.hydrate(&mut state).is_err());
    assert_eq!(state, before);
}

#[test]
fn a_views_only_foreign_database_is_rejected_without_initializing_terra_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foreign.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE VIEW unrelated AS SELECT 1;")
        .unwrap();
    drop(connection);
    assert!(RegionStore::open(&path, identity(), &Catalog::builtin(), 16).is_err());
    let connection = Connection::open(path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn output_and_backup_destinations_cannot_alias_database_or_reserved_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let mut store = open(&path, &Catalog::builtin());
    let mut state = SandboxState::new();
    store
        .checkpoint(&mut state, json!({}), &BTreeSet::new())
        .unwrap();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let output = dir.path().join(format!("world.db{suffix}"));
        assert!(validate_output_path(&path, &output).is_err());
        assert!(store.check_output_path(&output).is_err());
        assert!(store.backup_to(&output).is_err());
    }
    assert!(validate_output_path(&path, &dir.path().join("./unused/../world.db")).is_err());
    assert!(validate_output_path(&path, &dir.path().join("WORLD.DB")).is_err());
    assert!(validate_output_path(&path, &dir.path().join("world-report.json")).is_ok());
    assert_eq!(store.restore().unwrap().unwrap().generation, 1);
}

#[cfg(unix)]
#[test]
fn aliases_through_symbolic_and_hard_links_are_rejected_before_output_writes() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let _store = open(&path, &Catalog::builtin());
    let symbolic = dir.path().join("report.json");
    symlink(&path, &symbolic).unwrap();
    let hard = dir.path().join("report2.json");
    fs::hard_link(&path, &hard).unwrap();
    assert!(validate_output_path(&path, &symbolic).is_err());
    assert!(validate_output_path(&path, &hard).is_err());
    let dirlink = dir.path().join("directory-alias");
    symlink(dir.path(), &dirlink).unwrap();
    assert!(validate_output_path(&path, &dirlink.join("world.db-wal")).is_err());
}

#[test]
fn hard_link_aliases_protect_database_sidecars_and_managed_metadata_on_each_platform() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("world.db");
    for (index, name) in [
        "world.db",
        "world.db-wal",
        "world.db-shm",
        "world.db-journal",
        "world.json",
        "world-started.json",
        "world-session.lock",
    ]
    .into_iter()
    .enumerate()
    {
        let protected = dir.path().join(name);
        let bytes = format!("retained protected bytes {index}").into_bytes();
        fs::write(&protected, &bytes).unwrap();
        let alias = dir.path().join(format!("diagnostic-{index}.json"));
        fs::hard_link(&protected, &alias).unwrap();
        let input = if name.starts_with("world.db") {
            &database
        } else {
            &protected
        };
        assert!(validate_output_path(input, &alias).is_err(), "{name}");
        assert_eq!(fs::read(&protected).unwrap(), bytes);
        assert_eq!(fs::read(&alias).unwrap(), bytes);
        // Identical bytes in a genuinely separate file are not an identity.
        let distinct = dir.path().join(format!("safe-{index}.json"));
        fs::write(&distinct, &bytes).unwrap();
        assert!(validate_output_path(input, &distinct).is_ok(), "{name}");
    }
}

#[cfg(any(unix, windows))]
#[test]
fn future_protected_outputs_under_directory_aliases_are_rejected_before_creation() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("worlds");
    fs::create_dir(&original).unwrap();
    let alias = dir.path().join("worlds-alias");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&original, &alias).unwrap();
    #[cfg(windows)]
    {
        // Junction creation does not require Developer Mode or symbolic-link
        // privileges. Paths travel through environment values, never shell code.
        let result = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:TERRA_TEST_JUNCTION -Target $env:TERRA_TEST_TARGET | Out-Null",
            ])
            .env("TERRA_TEST_JUNCTION", &alias)
            .env("TERRA_TEST_TARGET", &original)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "junction fixture failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for name in [
        "world.db",
        "world.json",
        "world-started.json",
        "world-session.lock",
    ] {
        let protected = original.join(name);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let output = alias.join(format!("{name}{suffix}"));
            assert!(validate_output_path(&protected, &output).is_err());
            assert!(!protected.exists());
            assert!(!output.exists());
        }
        assert!(validate_output_path(&protected, &alias.join("report.json")).is_ok());
    }
    assert_eq!(fs::read_dir(&original).unwrap().count(), 0);
}

#[test]
fn uninspectable_output_ancestor_is_an_error_without_changing_protected_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let protected = dir.path().join("world.json");
    fs::write(&protected, b"retained profile").unwrap();
    let ordinary_file = dir.path().join("not-a-directory");
    fs::write(&ordinary_file, b"retained file").unwrap();
    assert!(validate_output_path(&protected, &ordinary_file.join("capture.png")).is_err());
    assert_eq!(fs::read(&protected).unwrap(), b"retained profile");
    assert_eq!(fs::read(&ordinary_file).unwrap(), b"retained file");
}

#[cfg(windows)]
#[test]
fn windows_new_output_drive_relative_rooted_and_trimmed_aliases_are_rejected() {
    use std::path::{Component, Prefix};
    let current = std::env::current_dir().unwrap();
    let drive = match current.components().next().unwrap() {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive as char,
            other => panic!("this Windows fixture needs a local drive: {other:?}"),
        },
        other => panic!("Windows current directory needs a drive: {other:?}"),
    };
    let dir = tempfile::Builder::new()
        .prefix("terra-output-alias-")
        .tempdir_in(&current)
        .unwrap();
    let directory_name = dir.path().file_name().unwrap().to_str().unwrap();
    for name in [
        "world.db",
        "world.json",
        "world-started.json",
        "world-session.lock",
    ] {
        let protected = dir.path().join(name);
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let relative = PathBuf::from(format!("{drive}:{directory_name}\\{name}{suffix}"));
            let rooted: PathBuf = dir
                .path()
                .join(format!("{name}{suffix}"))
                .components()
                .skip(1)
                .collect();
            let trimmed = dir.path().join(format!("{name}{suffix}. "));
            for output in [relative, rooted, trimmed] {
                assert!(
                    validate_output_path(&protected, &output).is_err(),
                    "{output:?}"
                );
            }
        }
        assert!(!protected.exists());
    }
    assert!(
        validate_output_path(
            &dir.path().join("world.db"),
            &PathBuf::from(format!("{drive}:{directory_name}\\report.json")),
        )
        .is_ok()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn unresolved_unicode_name_comparison_rejects_uncertainty_and_retains_distinct_ascii_paths() {
    let parts = |values: &[&str]| {
        values
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    };
    for (left, right, ambiguous) in [
        (vec!["WÖRLD.db-wal"], vec!["wörld.db-wal"], true),
        (vec!["Ä", "save.db"], vec!["ä", "SAVE.DB"], true),
        (vec!["metadata", "Ü"], vec!["reports", "ü"], false),
        (vec!["Ä", "save.db"], vec!["ä", "report.json"], false),
        (vec!["world.db-wal"], vec!["WORLD.DB-WAL"], false),
        (vec!["Ä", "save.db"], vec!["ä"], false),
    ] {
        assert_eq!(
            unresolved_name_components_may_alias(&parts(&left), &parts(&right)),
            ambiguous
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_unresolved_unicode_aliases_fail_closed_without_rejecting_separate_output_folders() {
    let dir = tempfile::tempdir().unwrap();
    let protected = dir.path().join("wörld.db");
    fs::write(&protected, b"retained database bytes").unwrap();
    let output = dir.path().join("WÖRLD.db-wal");
    let error = validate_output_path(&protected, &output)
        .unwrap_err()
        .to_string();
    assert!(error.contains("distinct existing output directory"));
    assert!(!output.exists());
    assert_eq!(fs::read(&protected).unwrap(), b"retained database bytes");
    assert!(
        validate_output_path(&dir.path().join("Ä-new.db"), &dir.path().join("ä-NEW.db")).is_err()
    );
    let reports = dir.path().join("reports");
    fs::create_dir(&reports).unwrap();
    assert!(validate_output_path(&protected, &reports.join("récupération.json")).is_ok());
    assert!(
        validate_output_path(
            &protected,
            &dir.path().join("new-reports").join("récupération.json")
        )
        .is_ok()
    );
    assert_eq!(fs::read_dir(&reports).unwrap().count(), 0);
}

#[test]
fn bounds_reject_bad_region_keys_and_oversized_metadata_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({}), &near_set())
        .unwrap();
    let before = state.clone();
    let generation = store.status().generation;
    let bad = BTreeSet::from([RegionKey {
        level: 16,
        x: u32::MAX,
        y: 0,
    }]);
    assert!(store.checkpoint(&mut state, json!({}), &bad).is_err());
    assert!(
        store
            .checkpoint(
                &mut state,
                json!({}),
                &BTreeSet::from([RegionKey::at(near(), 15).unwrap()])
            )
            .is_err()
    );
    assert!(
        store
            .checkpoint(
                &mut state,
                Value::String("x".repeat(MAX_DOCUMENT_BYTES)),
                &near_set()
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert_eq!(store.status().generation, generation);
}

#[test]
fn online_backup_reads_committed_wal_and_refuses_to_replace_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":42}), &near_set())
        .unwrap();
    let committed = store.export(&state).unwrap();
    state.living.accounts.get_mut(&1).unwrap().money -= 42;
    let backup = dir.path().join("backup.db");
    store.backup_to(&backup).unwrap();
    assert!(store.backup_to(&backup).is_err());
    let mut restored = open(&backup, &catalog);
    let saved = restored.restore().unwrap().unwrap();
    assert_eq!(saved.state, committed);
    assert_eq!(saved.metadata, json!({"clock":42}));
}

#[cfg(unix)]
#[test]
fn new_database_is_private_and_symlinks_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (_, catalog) = fixture();
    let _store = open(&path, &catalog);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = dir.path().join("link.db");
    symlink(&path, &link).unwrap();
    assert!(RegionStore::open(&link, identity(), &catalog, 16).is_err());
    assert!(RegionStore::open(dir.path(), identity(), &catalog, 16).is_err());
}

#[test]
fn abrupt_process_exit_recovers_committed_and_uncommitted_transactions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut state, json!({"phase":"baseline"}), &near_set())
        .unwrap();
    drop(store);
    let child = |mode: &str| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::crash_writer_child", "--nocapture"])
            .env("TERRA_REGION_TEST_CHILD_PATH", &path)
            .env("TERRA_REGION_TEST_CHILD_MODE", mode)
            .output()
            .unwrap()
    };
    let abandoned = child("abandon");
    assert_eq!(abandoned.status.code(), Some(74));
    let mut recovered = open(&path, &catalog);
    let first = recovered.restore().unwrap().unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.metadata, json!({"phase":"baseline"}));
    assert_eq!(first.state.resource_nodes["far-deposit"].remaining, 3);
    drop(recovered);
    let committed = child("commit");
    assert_eq!(committed.status.code(), Some(73));
    let mut recovered = open(&path, &catalog);
    let second = recovered.restore().unwrap().unwrap();
    assert_eq!(second.generation, 2);
    assert_eq!(second.metadata, json!({"phase":"committed-child"}));
    assert_eq!(second.state.resource_nodes["far-deposit"].remaining, 1);
}

#[test]
fn crash_writer_child() {
    let Some(path) = std::env::var_os("TERRA_REGION_TEST_CHILD_PATH") else {
        return;
    };
    let catalog = Catalog::builtin();
    let mut store = open(Path::new(&path), &catalog);
    let mut state = store.restore().unwrap().unwrap().state;
    if std::env::var("TERRA_REGION_TEST_CHILD_MODE").unwrap() == "commit" {
        state
            .resource_nodes
            .get_mut("far-deposit")
            .unwrap()
            .remaining = 1;
        store
            .checkpoint(&mut state, json!({"phase":"committed-child"}), &near_set())
            .unwrap();
        std::process::exit(73); // No destructors, connection close or graceful checkpoint.
    }
    store
        .connection
        .execute_batch("BEGIN IMMEDIATE; DELETE FROM regions; UPDATE world SET generation=99;")
        .unwrap();
    std::process::exit(74); // Leave a partial uncommitted transaction to SQLite recovery.
}

#[test]
fn incremental_checkpoint_reuses_unchanged_payloads_and_writes_only_changed_pages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("incremental.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    let initial = store.status();
    active.living.accounts.get_mut(&1).unwrap().money -= 1;
    let expected = store.export(&active).unwrap();
    store
        .checkpoint(&mut active, json!({"clock":2}), &near_set())
        .unwrap();
    let metrics = store.checkpoint_metrics();
    assert_eq!(metrics.encoded_pages, 0);
    assert_eq!(
        metrics.reused_pages,
        initial.active_regions + initial.dormant_regions
    );
    assert!(metrics.validation.certified_environment_records > 0);
    assert!(metrics.reused_page_bytes > 0);
    assert_eq!(store.export(&active).unwrap(), expected);
    active
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining -= 1;
    let expected = store.export(&active).unwrap();
    store
        .checkpoint(&mut active, json!({"clock":3}), &near_set())
        .unwrap();
    assert_eq!(store.checkpoint_metrics().encoded_pages, 1);
    assert!(store.checkpoint_metrics().reused_pages > 0);
    drop(store);
    assert_eq!(
        open(&path, &catalog).restore().unwrap().unwrap().state,
        expected
    );
}

#[test]
fn cached_encoding_never_certifies_semantically_forged_external_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("forged.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({}), &near_set())
        .unwrap();
    let before = active.clone();
    let before_status = store.status();
    let other = Connection::open(&path).unwrap();
    let far_key = RegionKey::at(far(), 16).unwrap();
    let payload: Vec<u8> = other
        .query_row(
            "SELECT payload FROM regions WHERE level=?1 AND x=?2 AND y=?3",
            params![far_key.level, far_key.x, far_key.y],
            |row| row.get(0),
        )
        .unwrap();
    let mut page: RegionPage = serde_json::from_slice(&payload).unwrap();
    page.resource_nodes
        .get_mut("far-deposit")
        .unwrap()
        .regrow_seconds = -1.;
    let forged = serde_json::to_vec(&page).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    other
        .execute(
            "UPDATE regions SET payload=?1,digest=?2 WHERE level=?3 AND x=?4 AND y=?5",
            params![
                forged,
                blake3::hash(&forged).to_hex().to_string(),
                far_key.level,
                far_key.x,
                far_key.y
            ],
        )
        .unwrap();
    other
        .execute(
            "UPDATE world SET payload_bytes=payload_bytes+?1",
            params![forged.len() as i64 - payload.len() as i64],
        )
        .unwrap();
    other.execute_batch("COMMIT").unwrap();
    assert!(
        store
            .checkpoint(&mut active, json!({"clock":2}), &near_set())
            .is_err(),
        "matching checksums and inventory cannot replace domain validation"
    );
    assert_eq!(active, before);
    assert_eq!(store.status(), before_status);
    assert_eq!(read_generation(&other).unwrap(), before_status.generation);
}

#[test]
fn failed_incremental_commit_keeps_prior_payloads_metrics_and_residency() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("atomic.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    let committed = store.export(&active).unwrap();
    let prior_metrics = serde_json::to_value(store.checkpoint_metrics()).unwrap();
    let prior_status = store.status();
    active
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining -= 1;
    let changed = active.clone();
    store.connection.execute_batch("CREATE TEMP TRIGGER fail_incremental BEFORE UPDATE ON world BEGIN SELECT RAISE(ABORT,'fail after changed page write'); END;").unwrap();
    assert!(
        store
            .checkpoint(&mut active, json!({"clock":2}), &near_set())
            .is_err()
    );
    assert_eq!(active, changed);
    assert_eq!(store.status(), prior_status);
    assert_eq!(
        serde_json::to_value(store.checkpoint_metrics()).unwrap(),
        prior_metrics
    );
    let saved = open(&path, &catalog).restore().unwrap().unwrap();
    assert_eq!(saved.state, committed);
    assert_eq!(saved.metadata, json!({"clock":1}));
    store
        .connection
        .execute_batch("DROP TRIGGER fail_incremental")
        .unwrap();
    store
        .checkpoint(&mut active, json!({"clock":3}), &near_set())
        .unwrap();
    assert_eq!(store.status().generation, prior_status.generation + 1);
    assert_eq!(store.checkpoint_metrics().encoded_pages, 1);
}

#[test]
fn bounded_backup_retries_do_not_publish_a_partial_destination() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("world.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":42}), &near_set())
        .unwrap();
    store.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let output = dir.path().join("recovered.db");
    let error = store
        .backup_with_options(
            &output,
            RegionBackupOptions {
                max_copy_time: std::time::Duration::from_millis(20),
                pages_per_step: 1,
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("deadline"), "{error}");
    assert!(!output.exists());
    store.connection.execute_batch("ROLLBACK").unwrap();
    let receipt = store
        .backup_with_options(&output, RegionBackupOptions::default())
        .unwrap();
    assert_eq!(receipt.generation, 1);
    assert!(receipt.domain_validated);
    assert!(receipt.game_metadata_requires_host_validation);
    assert!(receipt.copied_pages > 0);
    assert!(receipt.copy_steps > 0);
    assert_eq!(
        RegionStore::open_existing(output)
            .unwrap()
            .inspect()
            .unwrap()
            .generation,
        1
    );
}

#[test]
fn backup_receipt_names_the_copied_generation_and_refuses_corrupt_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world.db");
    let (mut state, catalog) = fixture();
    let mut stale_reader = open(&path, &catalog);
    stale_reader
        .checkpoint(&mut state, json!({"clock":42}), &near_set())
        .unwrap();
    let mut current = open(&path, &catalog);
    let mut latest = current.restore().unwrap().unwrap().state;
    current
        .checkpoint(&mut latest, json!({"clock":43}), &near_set())
        .unwrap();
    assert_eq!(stale_reader.status().generation, 1);
    let output = dir.path().join("latest.db");
    let receipt = stale_reader
        .backup_with_options(&output, RegionBackupOptions::default())
        .unwrap();
    assert_eq!(receipt.generation, 2);
    let mut recovered = RegionStore::open_existing(&output).unwrap();
    assert_eq!(
        recovered.restore().unwrap().unwrap().metadata,
        json!({"clock":43})
    );
    let inspection = recovered.inspect().unwrap();
    assert_eq!(
        inspection.checkpoint_payload_bytes + inspection.checkpoint_remaining_bytes,
        inspection.checkpoint_limit_bytes
    );
    current
        .connection
        .execute("UPDATE regions SET digest='corrupt'", [])
        .unwrap();
    let corrupt_output = dir.path().join("must-not-publish.db");
    assert!(current.backup_to(&corrupt_output).is_err());
    assert!(!corrupt_output.exists());
    assert_eq!(
        RegionStore::open_existing(output)
            .unwrap()
            .inspect()
            .unwrap()
            .generation,
        2
    );
}

#[test]
fn maintenance_reader_and_recovery_preserve_the_verified_backup_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, catalog) = fixture();
    let mut store = open(&dir.path().join("active.db"), &catalog);
    store
        .checkpoint(&mut state, json!({"clock":42}), &near_set())
        .unwrap();
    let backup = dir.path().join("backup.db");
    store.backup_to(&backup).unwrap();
    let before = fs::read(&backup).unwrap();
    {
        let reader = RegionStore::open_read_only(&backup).unwrap();
        assert_eq!(reader.inspect().unwrap().generation, 1);
        assert_eq!(reader.archive().unwrap().metadata, json!({"clock":42}));
        reader.backup_to(dir.path().join("recovered.db")).unwrap();
        assert!(
            reader
                .connection
                .execute("UPDATE world SET generation=99", [])
                .is_err()
        );
    }
    assert_eq!(before, fs::read(backup).unwrap());
}

#[test]
fn fully_materialized_hydration_rejects_same_generation_corruption_without_installing_state() {
    for edit in [
        "UPDATE regions SET payload=x'00' WHERE rowid=(SELECT rowid FROM regions LIMIT 1)",
        "UPDATE world SET global_state=x'00' WHERE singleton=1",
        "UPDATE world SET metadata=x'00' WHERE singleton=1",
        "CREATE INDEX external_region_index ON regions(level)",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let (mut active, catalog) = fixture();
        let mut store = open(&path, &catalog);
        store
            .checkpoint(&mut active, json!({"clock":1}), &near_set())
            .unwrap();
        active = store.restore().unwrap().unwrap().state;
        assert!(store.fully_materialized);
        active
            .resource_nodes
            .get_mut("near-deposit")
            .unwrap()
            .remaining = 14;
        let before = serde_json::to_value(&active).unwrap();
        let status = store.status();
        let metrics = serde_json::to_value(store.checkpoint_metrics()).unwrap();
        let generation = store.generation;
        let editor = Connection::open(&path).unwrap();
        editor.execute_batch(edit).unwrap();
        assert_eq!(read_generation(&editor).unwrap(), generation);
        assert!(store.hydrate(&mut active).is_err(), "{edit}");
        assert_eq!(serde_json::to_value(&active).unwrap(), before, "{edit}");
        assert_eq!(store.status(), status, "{edit}");
        assert_eq!(store.generation, generation, "{edit}");
        assert_eq!(
            serde_json::to_value(store.checkpoint_metrics()).unwrap(),
            metrics
        );
        assert_eq!(read_generation(&editor).unwrap(), generation);
        active.validate_with_catalog(&catalog).unwrap();
    }
}

#[test]
fn initial_hydration_rejects_changed_empty_store_invariants() {
    for edit in [
        "CREATE INDEX external_region_index ON regions(level)",
        "UPDATE world SET metadata=x'00' WHERE singleton=1",
        "INSERT INTO regions(level,x,y,payload,digest) VALUES(16,0,0,x'00','invalid')",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let (mut active, catalog) = fixture();
        let mut store = open(&path, &catalog);
        assert!(store.fully_materialized);
        let before = serde_json::to_value(&active).unwrap();
        let status = store.status();
        let editor = Connection::open(&path).unwrap();
        editor.execute_batch(edit).unwrap();
        assert!(store.hydrate(&mut active).is_err(), "{edit}");
        assert_eq!(serde_json::to_value(&active).unwrap(), before, "{edit}");
        assert_eq!(store.status(), status);
        assert_eq!(read_generation(&editor).unwrap(), 0);
    }
}

#[test]
fn guarded_full_hydration_reuses_validated_pages_and_retains_uncommitted_edits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.db");
    let (mut active, catalog) = fixture();
    let mut store = open(&path, &catalog);
    // The empty-generation path remains a supported registration barrier.
    store.hydrate(&mut active).unwrap();
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    active = store.restore().unwrap().unwrap().state;
    active
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining = 14;
    let before = serde_json::to_value(&active).unwrap();
    let status = store.status();
    let cache = store.cache_stats();
    store.hydrate(&mut active).unwrap();
    store.hydrate(&mut active).unwrap();
    assert_eq!(serde_json::to_value(&active).unwrap(), before);
    assert_eq!(store.status(), status);
    assert_eq!(
        store.cache_stats().decoded_checkpoints,
        cache.decoded_checkpoints
    );
    assert_eq!(store.cache_stats().hits, cache.hits + 2);
    store
        .checkpoint(&mut active, json!({"clock":2}), &near_set())
        .unwrap();
    let saved = store.restore().unwrap().unwrap();
    assert_eq!(saved.generation, 2);
    assert_eq!(saved.state.resource_nodes["near-deposit"].remaining, 14);
}
