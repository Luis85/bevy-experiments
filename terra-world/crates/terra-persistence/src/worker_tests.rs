use super::*;
use serde_json::json;
use terra_core::GeoPosition;
use terra_sandbox::Catalog;

fn fixture() -> (tempfile::TempDir, RegionStore, SandboxState) {
    let directory = tempfile::tempdir().unwrap();
    let catalog = Catalog::builtin();
    let near = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let far = GeoPosition::new(48.2, 11.7, 140.).unwrap();
    let mut active = SandboxState::new();
    active.add_player(1, near).unwrap();
    active
        .seed_resource("near-deposit".into(), near, "stone", 25, 0.)
        .unwrap();
    active
        .seed_resource("far-deposit".into(), far, "stone", 37, 0.)
        .unwrap();
    active
        .resource_nodes
        .get_mut("far-deposit")
        .unwrap()
        .remaining = 3;
    let mut store = RegionStore::open(
        directory.path().join("world.db"),
        json!({"kind":"worker-loss-test","source":"fixture-1"}),
        &catalog,
        16,
    )
    .unwrap();
    store
        .checkpoint(&mut active, json!({"clock":1}), &near_set())
        .unwrap();
    assert!(active.is_partial());
    assert!(!active.resource_nodes.contains_key("far-deposit"));
    (directory, store, active)
}

fn near_set() -> BTreeSet<RegionKey> {
    BTreeSet::from([RegionKey::at(GeoPosition::new(48.137, 11.575, 100.).unwrap(), 16).unwrap()])
}

/// Lose the acknowledgement of real submitted work. The operation may already
/// have committed; closing its result receiver must not imply a rollback.
fn lose_completion(worker: &mut RegionWorker) {
    let (sender, disconnected) = mpsc::sync_channel(1);
    drop(sender);
    worker.receiver = Mutex::new(disconnected);
    worker.sender.take();
    worker.thread.take().unwrap().join().unwrap();
}

fn terminal(error: StoreError) -> String {
    assert!(matches!(&error, StoreError::Worker(_)), "{error}");
    let message = error.to_string();
    assert!(message.contains("Stop and reopen the world"), "{message}");
    message
}

fn rejects_continuation(worker: &mut RegionWorker, active: &SandboxState, path: &Path) {
    assert!(!worker.pending());
    assert!(worker.requires_reopen());
    assert_eq!(worker.pending_kind(), None);
    assert!(worker.try_complete().is_err());
    // A second finish must never reinterpret the cleared pending flag as a
    // successful save/exit barrier after the acknowledgement was lost.
    assert!(worker.finish().is_err());
    terminal(worker.submit_hydrate(active.clone()).unwrap_err());
    terminal(
        worker
            .submit_checkpoint(active.clone(), json!({"clock":99}), near_set())
            .unwrap_err(),
    );
    let mut entered = false;
    terminal(
        worker
            .with_store(|_| {
                entered = true;
                Ok(())
            })
            .unwrap_err(),
    );
    assert!(!entered, "uncertain state must not enter maintenance");
    terminal(worker.export(active).unwrap_err());
    terminal(worker.backup_to(path).unwrap_err());
    assert!(!path.exists());
}

#[test]
fn lost_checkpoint_completion_stops_waiting_and_reopen_recovers_the_exact_commit() {
    let (directory, store, mut active) = fixture();
    let mut expected = store.archive().unwrap();
    let acknowledged = store.status();
    active
        .resource_nodes
        .get_mut("near-deposit")
        .unwrap()
        .remaining = 17;
    expected.state = store.export(&active).unwrap();
    expected.metadata = json!({"clock":2});
    expected.source_generation = 2;
    let retained = active.clone();
    let mut worker = RegionWorker::new(store).unwrap();
    assert!(!worker.requires_reopen());
    worker
        .submit_checkpoint(active.clone(), expected.metadata.clone(), near_set())
        .unwrap();
    lose_completion(&mut worker);
    let error = match worker.try_complete() {
        Err(error) => terminal(error),
        Ok(_) => panic!("a lost checkpoint result must be an explicit failure"),
    };
    assert!(error.contains("result channel disconnected"));
    rejects_continuation(&mut worker, &active, &directory.path().join("unsafe.db"));
    assert_eq!(active, retained, "the host's visible state stays retained");
    assert_eq!(worker.status(), acknowledged);
    let path = directory.path().join("world.db");
    assert_eq!(
        serde_json::to_value(
            RegionStore::open_read_only(&path)
                .unwrap()
                .archive()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&expected).unwrap(),
        "a committed save remains exact even when its result was not delivered"
    );
    drop(worker);

    let mut reopened = RegionStore::open_existing(&path).unwrap();
    let restored = reopened.restore().unwrap().unwrap();
    assert_eq!(restored.generation, 2);
    assert_eq!(restored.state, expected.state);
    assert_eq!(restored.metadata, expected.metadata);
    let mut resumed = RegionWorker::new(reopened).unwrap();
    assert!(!resumed.requires_reopen());
    resumed
        .submit_checkpoint(restored.state, json!({"clock":3}), near_set())
        .unwrap();
    let completion = resumed.finish().unwrap().unwrap();
    assert!(completion.error.is_none(), "{:?}", completion.error);
    assert_eq!(completion.status.generation, 3);
    assert!(!resumed.pending());
}

#[test]
fn lost_hydration_completion_fails_finish_and_preserves_the_checkpoint() {
    let (directory, store, active) = fixture();
    let before = serde_json::to_value(store.archive().unwrap()).unwrap();
    let retained = active.clone();
    let mut worker = RegionWorker::new(store).unwrap();
    worker.submit_hydrate(active.clone()).unwrap();
    lose_completion(&mut worker);
    match worker.finish() {
        Err(error) => {
            terminal(error);
        }
        Ok(_) => panic!("a lost hydration result must fail the maintenance barrier"),
    }
    rejects_continuation(&mut worker, &active, &directory.path().join("unsafe.db"));
    assert_eq!(active, retained);
    let path = directory.path().join("world.db");
    assert_eq!(
        serde_json::to_value(
            RegionStore::open_read_only(&path)
                .unwrap()
                .archive()
                .unwrap()
        )
        .unwrap(),
        before
    );
    drop(worker);
    let mut reopened = RegionStore::open_existing(&path).unwrap();
    let restored = reopened.restore().unwrap().unwrap();
    assert_eq!(restored.generation, 1);
    assert_eq!(restored.state.resource_nodes["far-deposit"].remaining, 3);
    let mut resumed = RegionWorker::new(reopened).unwrap();
    resumed.submit_hydrate(restored.state.clone()).unwrap();
    let completion = resumed.finish().unwrap().unwrap();
    assert!(completion.error.is_none(), "{:?}", completion.error);
    assert_eq!(completion.state.unwrap(), restored.state);
}

#[test]
fn disconnected_idle_worker_rejects_submission_and_later_finish() {
    let (directory, store, active) = fixture();
    let before = serde_json::to_value(store.archive().unwrap()).unwrap();
    let mut worker = RegionWorker::new(store).unwrap();
    worker.sender.as_ref().unwrap().send(None).unwrap();
    worker.thread.take().unwrap().join().unwrap();
    let error = terminal(worker.submit_hydrate(active.clone()).unwrap_err());
    assert!(error.contains("request channel disconnected"));
    rejects_continuation(&mut worker, &active, &directory.path().join("unsafe.db"));
    assert_eq!(
        serde_json::to_value(
            RegionStore::open_read_only(directory.path().join("world.db"))
                .unwrap()
                .archive()
                .unwrap()
        )
        .unwrap(),
        before
    );
}

fn poison_store(worker: &RegionWorker) {
    let store = Arc::clone(&worker.store);
    assert!(
        std::thread::spawn(move || {
            let _guard = store.lock().unwrap();
            panic!("simulate storage panic");
        })
        .join()
        .is_err()
    );
}

#[test]
fn poisoned_background_store_is_terminal_and_preserves_acknowledged_status() {
    for checkpoint in [false, true] {
        let (directory, store, active) = fixture();
        let before = serde_json::to_value(store.archive().unwrap()).unwrap();
        let acknowledged = store.status();
        let mut worker = RegionWorker::new(store).unwrap();
        let shared = Arc::clone(&worker.store);
        let (ready, waiting) = mpsc::sync_channel(0);
        let (release, released) = mpsc::sync_channel(0);
        let poisoner = std::thread::spawn(move || {
            let _guard = shared.lock().unwrap();
            ready.send(()).unwrap();
            released.recv().unwrap();
            panic!("simulate storage panic with a submitted request");
        });
        waiting.recv().unwrap();
        if checkpoint {
            worker
                .submit_checkpoint(active.clone(), json!({"clock":2}), near_set())
                .unwrap();
        } else {
            worker.submit_hydrate(active.clone()).unwrap();
        }
        release.send(()).unwrap();
        assert!(poisoner.join().is_err());
        assert!(worker.requires_reopen());
        let error = if checkpoint {
            match worker.finish() {
                Err(error) => error,
                Ok(_) => panic!("poisoned storage must fail the barrier itself"),
            }
        } else {
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match worker.try_complete() {
                    Err(error) => break error,
                    Ok(None) => {
                        assert!(Instant::now() < deadline, "worker did not report poison");
                        std::thread::yield_now();
                    }
                    Ok(Some(_)) => panic!("poisoned storage must not return a normal completion"),
                }
            }
        };
        assert!(terminal(error).contains("store lock poisoned"));
        assert_eq!(worker.status(), acknowledged);
        rejects_continuation(&mut worker, &active, &directory.path().join("unsafe.db"));
        drop(worker);
        let mut reopened = RegionStore::open_existing(directory.path().join("world.db")).unwrap();
        assert_eq!(
            serde_json::to_value(reopened.archive().unwrap()).unwrap(),
            before
        );
        let restored = reopened.restore().unwrap().unwrap();
        let mut resumed = RegionWorker::new(reopened).unwrap();
        resumed
            .submit_checkpoint(restored.state, json!({"clock":2}), near_set())
            .unwrap();
        assert_eq!(resumed.finish().unwrap().unwrap().status.generation, 2);
    }
}

#[test]
fn poisoned_idle_store_blocks_read_only_and_maintenance_paths() {
    let (directory, store, active) = fixture();
    let acknowledged = store.status();
    let mut worker = RegionWorker::new(store).unwrap();
    poison_store(&worker);
    assert!(
        worker.requires_reopen(),
        "idle poison must expose the terminal recovery path"
    );
    terminal(worker.export(&active).unwrap_err());
    terminal(
        worker
            .backup_to(directory.path().join("unsafe.db"))
            .unwrap_err(),
    );
    assert_eq!(worker.status(), acknowledged);
    rejects_continuation(&mut worker, &active, &directory.path().join("unsafe.db"));
}
