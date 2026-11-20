use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use terra_bevy::{SandboxRuntime, WorldConfig, sandbox::LocalWorldSave};
use terra_core::WorldClock;
use terra_persistence::{RegionArchive, RegionStore};
use terra_sandbox::Catalog;

fn run(args: &[&Path], verb: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_terra-worldctl"))
        .arg(verb)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn actual_operations_cli_roundtrips_local_and_portable_worlds_without_overwriting() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("terra-worldctl-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let config = WorldConfig::default();
    let mut sandbox = SandboxRuntime::new(config.focus);
    sandbox
        .state
        .seed_resource("test:depleted".into(), config.focus, "wood", 10, 0.)
        .unwrap();
    sandbox
        .state
        .resource_nodes
        .get_mut("test:depleted")
        .unwrap()
        .remaining = 2;
    let source = directory.join("source.json");
    let settings = directory.join("settings.json");
    let database = directory.join("world.db");
    let backup = directory.join("backup.db");
    let archive = directory.join("archive.json");
    let recovered = directory.join("recovered.db");
    let imported = directory.join("imported.db");
    let exported = directory.join("exported.json");
    let save = LocalWorldSave {
        schema_version: 1,
        identity: (&config).into(),
        clock: WorldClock::default(),
        player_id: 1,
        state: sandbox.state,
        politics: None,
    };
    save.write(&source).unwrap();
    fs::write(&settings, serde_json::to_vec(&config).unwrap()).unwrap();
    let initial = fs::read(&source).unwrap();
    for (verb, args) in [
        (
            "import-local",
            vec![source.as_path(), database.as_path(), settings.as_path()],
        ),
        ("inspect", vec![database.as_path()]),
        ("verify", vec![database.as_path()]),
        ("backup", vec![database.as_path(), backup.as_path()]),
        ("recover", vec![backup.as_path(), recovered.as_path()]),
        ("export", vec![database.as_path(), archive.as_path()]),
        ("import", vec![archive.as_path(), imported.as_path()]),
        (
            "export-local",
            vec![imported.as_path(), exported.as_path(), settings.as_path()],
        ),
    ] {
        let output = run(&args, verb);
        assert!(
            output.status.success(),
            "{verb}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let restored = LocalWorldSave::read(&exported, &config, &Catalog::builtin()).unwrap();
    assert_eq!(restored.state, save.state);
    assert_eq!(restored.clock, save.clock);
    assert_eq!(RegionArchive::read(&archive).unwrap().state, save.state);
    assert_eq!(
        RegionStore::open_existing(&backup)
            .unwrap()
            .restore()
            .unwrap()
            .unwrap()
            .state,
        save.state
    );
    assert!(
        !run(&[&source, &database, &settings], "import-local")
            .status
            .success()
    );
    assert!(
        !run(&[&database, &exported, &settings], "export-local")
            .status
            .success()
    );
    assert!(!run(&[&database, &backup], "backup").status.success());
    assert!(!run(&[&backup, &recovered], "recover").status.success());
    assert_eq!(
        RegionStore::open_existing(&recovered)
            .unwrap()
            .inspect()
            .unwrap()
            .generation,
        1
    );
    assert!(!run(&[&database, &archive], "export").status.success());
    let mut wrong = config.clone();
    wrong.seed += 1;
    fs::write(&settings, serde_json::to_vec(&wrong).unwrap()).unwrap();
    let rejected = directory.join("rejected.db");
    assert!(
        !run(&[&source, &rejected, &settings], "import-local")
            .status
            .success()
    );
    assert!(!rejected.exists());
    let missing = directory.join("missing.db");
    assert!(!run(&[&missing], "inspect").status.success());
    assert!(!missing.exists());
    assert_eq!(fs::read(&source).unwrap(), initial);
    fs::remove_dir_all(directory).unwrap();
}
