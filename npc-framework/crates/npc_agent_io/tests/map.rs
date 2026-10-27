use npc_agent_io::map::{LandmarkChange, MapError, MapStore, NewLandmark, Place};
use npc_agent_io::notes::{NotesOwner, NotesStores, Stamp, Writer};
use serde_json::json;
use std::path::PathBuf;

fn folder(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("npc-map-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn at() -> Stamp {
    Stamp {
        tick: 1,
        date: "2026-10-27T12:00:00Z".into(),
    }
}
fn new(name: &str) -> NewLandmark {
    NewLandmark {
        name: name.into(),
        place: Place::Here,
        kind: Some("gate".into()),
        note: "Remember this place.".into(),
    }
}

#[test]
fn stale_landmark_stamp_never_repeats_after_reopen_and_clock_rollback() {
    for (tag, date) in [("aba-tick", "D"), ("aba-date", "C")] {
        let dir = folder(tag);
        let initial = Stamp {
            tick: 1,
            date: "D".into(),
        };
        let mut store = MapStore::open(&dir).unwrap();
        let a = store
            .add(new("Place"), [0, 0], None, initial.clone(), Writer::Agent)
            .unwrap();
        let b = store
            .update(
                a.id,
                &a.updated,
                LandmarkChange {
                    note: Some("B".into()),
                    ..Default::default()
                },
                Stamp {
                    tick: 0,
                    date: date.into(),
                },
                Writer::Player,
            )
            .unwrap();
        drop(store);
        let mut store = MapStore::open(&dir).unwrap();
        let c = store
            .update(
                a.id,
                &b.updated,
                LandmarkChange {
                    note: Some("C".into()),
                    ..Default::default()
                },
                initial.clone(),
                Writer::Agent,
            )
            .unwrap();
        drop(store);
        let mut store = MapStore::open(&dir).unwrap();
        assert!(matches!(
            store.update(
                a.id,
                &a.updated,
                LandmarkChange {
                    note: Some("stale overwrite".into()),
                    ..Default::default()
                },
                Stamp {
                    tick: 2,
                    date: "E".into()
                },
                Writer::Player
            ),
            Err(MapError::Stale { .. })
        ));
        assert_ne!(c.updated, a.updated);
        assert_eq!(c.created, a.created);
        assert_eq!(store.list().unwrap()[0].note, "C");
        assert!(
            std::fs::read_to_string(store.path_for(a.id).unwrap())
                .unwrap()
                .contains("revision: 3")
        );
    }
}
#[test]
fn add_list_and_file_round_trip() {
    let dir = folder("roundtrip");
    let mut store = MapStore::open_as(
        &dir,
        NotesOwner {
            agent: "Wren".into(),
            world: "W".into(),
        },
    )
    .unwrap();
    let mark = store
        .add(new("North gate"), [1000, -2000], None, at(), Writer::Agent)
        .unwrap();
    assert_eq!(mark.id, 1);
    assert_eq!(store.list().unwrap(), vec![mark.clone()]);
    assert_eq!(store.path_for(1).unwrap(), dir.join("map/North gate.md"));
    let text = std::fs::read_to_string(store.path_for(1).unwrap()).unwrap();
    assert!(text.contains("type: landmark"));
    assert!(text.contains("pos: [1000, -2000]"), "{text}");
    assert!(text.contains("author: agent"));
    assert!(text.contains("Wren"));
    assert_eq!(MapStore::open(&dir).unwrap().list().unwrap(), vec![mark]);
}
#[test]
fn update_renames_preserves_unknown_headers_and_rejects_stale_writes() {
    let dir = folder("rename");
    let mut store = MapStore::open(&dir).unwrap();
    let mark = store
        .add(new("Gate"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    let old = store.path_for(mark.id).unwrap();
    let text = std::fs::read_to_string(&old).unwrap().replace(
        "type: landmark",
        "type: landmark\ncustom: 'keep me' # exactly",
    );
    std::fs::write(&old, text).unwrap();
    let changed = store
        .update(
            mark.id,
            &mark.updated,
            LandmarkChange {
                name: Some("Well".into()),
                pos: Some([2000, 3000]),
                ..Default::default()
            },
            at(),
            Writer::Player,
        )
        .unwrap();
    assert_ne!(changed.updated, mark.updated);
    assert!(!old.exists());
    let text = std::fs::read_to_string(store.path_for(mark.id).unwrap()).unwrap();
    assert!(text.contains("custom: 'keep me' # exactly"));
    assert!(text.contains("Gate"));
    assert!(text.contains("author: player"));
    assert!(matches!(
        store.update(
            mark.id,
            &mark.updated,
            LandmarkChange {
                note: Some("stale".into()),
                ..Default::default()
            },
            at(),
            Writer::Player
        ),
        Err(MapError::Stale { .. })
    ));
}
#[test]
fn hand_edit_same_size_and_mtime_is_detected_and_versions_the_entry() {
    let dir = folder("digest");
    let mut store = MapStore::open(&dir).unwrap();
    let mark = store
        .add(new("Gate"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    let path = store.path_for(mark.id).unwrap();
    let time = std::fs::metadata(&path).unwrap().modified().unwrap();
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Remember this place.", "Remember THAT place.");
    std::fs::write(&path, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(time))
        .unwrap();
    let marks = store.list().unwrap();
    assert_eq!(marks[0].note, "Remember THAT place.");
    assert_ne!(marks[0].updated, mark.updated);
}
#[test]
fn patch_kind_distinguishes_omitted_null_and_text() {
    assert_eq!(
        serde_json::from_value::<LandmarkChange>(json!({}))
            .unwrap()
            .kind,
        None
    );
    assert_eq!(
        serde_json::from_value::<LandmarkChange>(json!({"kind":null}))
            .unwrap()
            .kind,
        Some(None)
    );
    assert_eq!(
        serde_json::from_value::<LandmarkChange>(json!({"kind":"well"}))
            .unwrap()
            .kind,
        Some(Some("well".into()))
    );
    assert_eq!(
        serde_json::to_value(LandmarkChange {
            kind: Some(None),
            ..Default::default()
        })
        .unwrap()["kind"],
        json!(null)
    );
}
#[test]
fn invalid_and_unreadable_ids_are_reserved_and_exhaustion_is_an_error() {
    let dir = folder("exhaustion");
    std::fs::create_dir_all(dir.join("map")).unwrap();
    let bad = dir.join("map/bad.md");
    let text = "---\nid: 4294967295\ntype: landmark\nname: broken\npos: nope\n---\n";
    std::fs::write(&bad, text).unwrap();
    let mut store = MapStore::open(&dir).unwrap();
    assert_eq!(store.unreadable().unwrap().len(), 1);
    assert!(matches!(
        store.add(new("New"), [0, 0], None, at(), Writer::Agent),
        Err(MapError::Exhausted(_))
    ));
    assert_eq!(std::fs::read_to_string(bad).unwrap(), text);
}
#[test]
fn map_limits_and_coordinates_are_checked_before_writing() {
    let dir = folder("limits");
    let mut store = MapStore::open(&dir).unwrap();
    let mut n = new(&"x".repeat(201));
    assert!(matches!(
        store.add(n.clone(), [0, 0], None, at(), Writer::Agent),
        Err(MapError::TooLong { .. })
    ));
    n.name = "Okay".into();
    n.note = "x".repeat(8193);
    assert!(store.add(n, [0, 0], None, at(), Writer::Agent).is_err());
    assert!(
        store
            .add(
                new("Extreme"),
                [i32::MAX, i32::MIN],
                None,
                at(),
                Writer::Agent
            )
            .is_err()
    );
    assert!(store.list().unwrap().is_empty());
}
#[test]
fn folder_mapping_keeps_one_map_store_and_owner_for_both_writers() {
    let dir = folder("mapping");
    let mut stores = NotesStores::default();
    stores.set_owner(
        &dir,
        NotesOwner {
            agent: "Wren".into(),
            world: "World".into(),
        },
    );
    let mark = stores
        .map_for(&dir)
        .unwrap()
        .add(new("Gate"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    let changed = stores
        .map_for(&dir)
        .unwrap()
        .update(
            mark.id,
            &mark.updated,
            LandmarkChange {
                note: Some("Player edit".into()),
                ..Default::default()
            },
            at(),
            Writer::Player,
        )
        .unwrap();
    assert_eq!(changed.note, "Player edit");
    assert_eq!(stores.map_for(&dir).unwrap().list().unwrap().len(), 1);
}

#[test]
fn kind_clears_thing_stays_fixed_and_revision_exhaustion_is_checked() {
    let dir = folder("revision");
    let mut store = MapStore::open(&dir).unwrap();
    let mark = store
        .add(
            new("Place"),
            [1000, 2000],
            Some(npc_kernel::SimId(42)),
            at(),
            Writer::Agent,
        )
        .unwrap();
    let changed = store
        .update(
            mark.id,
            &mark.updated,
            LandmarkChange {
                kind: Some(None),
                pos: Some([3000, 4000]),
                ..Default::default()
            },
            at(),
            Writer::Player,
        )
        .unwrap();
    assert_eq!(changed.kind, None);
    assert_eq!(changed.thing, Some(npc_kernel::SimId(42)));
    assert_eq!(changed.pos, [3000, 4000]);
    let path = store.path_for(mark.id).unwrap();
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("revision: 2", "revision: 4294967295");
    std::fs::write(&path, &text).unwrap();
    let mut store = MapStore::open(&dir).unwrap();
    let mark = store.list().unwrap().remove(0);
    for _ in 0..2 {
        assert!(matches!(
            store.update(
                mark.id,
                &mark.updated,
                LandmarkChange {
                    note: Some("cannot write".into()),
                    ..Default::default()
                },
                at(),
                Writer::Agent
            ),
            Err(MapError::Exhausted(_))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
}

#[test]
fn handmade_files_are_adopted_and_safe_names_do_not_collide() {
    let dir = folder("adopt");
    std::fs::create_dir_all(dir.join("map")).unwrap();
    std::fs::write(dir.join("map/Handmade.md"),"---\ntype: landmark\nname: Handmade\npos: [3, 4]\nkind: null\ncustom: {opaque: value}\n---\nhand note\n").unwrap();
    let mut store = MapStore::open(&dir).unwrap();
    let adopted = store.list().unwrap().remove(0);
    assert_eq!(adopted.id, 1);
    assert_eq!(adopted.kind, None);
    store
        .update(
            1,
            &adopted.updated,
            LandmarkChange {
                note: Some("new note".into()),
                ..Default::default()
            },
            at(),
            Writer::Player,
        )
        .unwrap();
    let raw = std::fs::read_to_string(dir.join("map/Handmade.md")).unwrap();
    assert!(raw.contains("kind: null"));
    assert!(raw.contains("custom: {opaque: value}"));
    let a = store
        .add(new("CON"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    let b = store
        .add(new("CON"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    assert_ne!(store.path_for(a.id).unwrap(), store.path_for(b.id).unwrap());
    assert!(
        store
            .path_for(a.id)
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("CON_")
    );
}

#[cfg(windows)]
#[test]
fn failed_rename_cleanup_preserves_a_later_human_edit() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = folder("rename-edit");
    let mut store = MapStore::open(&dir).unwrap();
    let mark = store
        .add(new("Old"), [0, 0], None, at(), Writer::Agent)
        .unwrap();
    let path = store.path_for(mark.id).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    store
        .update(
            mark.id,
            &mark.updated,
            LandmarkChange {
                name: Some("New".into()),
                ..Default::default()
            },
            at(),
            Writer::Player,
        )
        .unwrap();
    assert!(path.exists());
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Remember this place.", "Remember THAT place.");
    std::fs::write(&path, &edited).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    drop(held);
    store.list().unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), edited);
    assert!(dir.join("map/New.md").exists());
    assert_eq!(store.unreadable().unwrap().len(), 1);
}
