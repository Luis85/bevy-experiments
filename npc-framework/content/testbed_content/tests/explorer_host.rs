//! The window-free explorer host (`testbed_content::explorer::host`, plan
//! Task 6): New, Resume, a save from another build, Fresh world,
//! `--check-save`, the session's agent-folder files and `run.json`. Most of
//! these moved here from actor_testbed's explorer tests; the callers there
//! pass a wall-clock seed and date, these pass fixed ones.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bevy_app::App;
use npc_framework::agent_io::notes::{NotesClock, NotesDirs};
use npc_framework::interaction::Door;
use npc_framework::kernel::spatial::{CellCoord, SpatialGrid};
use npc_framework::perception::{
    DEFAULT_FORGET_AFTER, ExploredCells, Facing, Perceivable, PerceptionConfig, PerceptionMemory,
    ReachSense, RevealsFog, SeenStatics, SenseKind, VisibleCells,
};
use npc_framework::prelude::{
    SimClock, SimControl, SimId, SimIndex, SimPos, run_ticks, state_hash,
};
use npc_framework::snapshot::{self, LoadReport};
use serde_json::Value;
use testbed_content::TestbedIds;
use testbed_content::explorer::home::{AgentDir, AgentMeta};
use testbed_content::explorer::host::{
    ACTOR, CheckJson, EXPLORER_FORGET_AFTER, EXPLORER_MEMORY_CAPACITY, EXPLORER_REACH_MM,
    ExplorerSession, HostArgs, Mismatch, RunFile, RunMode, Start, build_world, check_json,
    check_save, fog_reset_note, insert_notes, needs_choice, prompt_kind, rfc3339, save_now,
    write_session_files,
};
use testbed_content::explorer::prompt::PromptKind;
use testbed_content::explorer::workspace::GENERATED;
use testbed_content::world::kinds;

/// The New seed and the date every test passes (no wall clock in the host).
const SEED: u64 = 0x5eed;
const NOW: &str = "2026-10-25T12:00:00Z";

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-explorer-host-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// [`build_world`] with the tests' seed and date.
fn build_explorer_app(
    args: &HostArgs,
    plugins: impl FnOnce(&mut App),
) -> Result<(App, Start, AgentDir), String> {
    build_world(args, SEED, NOW, plugins)
}

fn new_args(home: &Path, agent: &str, world: &str) -> HostArgs {
    HostArgs {
        home: home.to_path_buf(),
        agent: agent.into(),
        world: Some(world.into()),
        resume: false,
        on_mismatch: None,
        preset: None,
        actors_dir: Some(home.join("actors")),
    }
}

fn resume_args(home: &Path, agent: &str, on_mismatch: Option<Mismatch>) -> HostArgs {
    HostArgs {
        world: None,
        resume: true,
        on_mismatch,
        ..new_args(home, agent, "")
    }
}

fn actor(app: &App, agent: &str) -> SimId {
    app.world()
        .resource::<TestbedIds>()
        .get(agent)
        .unwrap_or_else(|| panic!("no actor named {agent}"))
}

fn pos(app: &App, id: SimId) -> SimPos {
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    *app.world().get::<SimPos>(e).unwrap()
}

/// Rewrites the save at `path` through `edit` (as JSON).
fn edit_save(path: &Path, edit: impl FnOnce(&mut Value)) {
    let mut v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    edit(&mut v);
    std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn explorer_rfc3339_dates() {
    assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    let at = |s: u64| rfc3339(UNIX_EPOCH + std::time::Duration::from_secs(s));
    assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(at(1_700_000_000), "2023-11-14T22:13:20Z");
    assert_eq!(at(1_792_848_309), "2026-10-24T13:25:09Z");
}

#[test]
fn new_creates_folder_and_names_actor() {
    let home = tmp("new");
    let (app, start, dir) =
        build_explorer_app(&new_args(&home, "Wren Ashdown", "Hollowmere"), |_| {}).unwrap();
    assert!(matches!(start, Start::New));
    assert_eq!(dir.dir, home.join("wren-ashdown"));
    let ids = app.world().resource::<TestbedIds>();
    assert_eq!(ids.get("Wren Ashdown"), Some(ids.units[0]));
    assert_eq!(ids.get(ACTOR), None);
    let meta: AgentMeta =
        serde_json::from_str(&std::fs::read_to_string(dir.meta()).unwrap()).unwrap();
    assert_eq!(
        (meta.agent.as_str(), meta.world.as_str()),
        ("Wren Ashdown", "Hollowmere")
    );
    assert!(app.world().resource::<SimControl>().paused);
    // The tick-0 save exists and fits this build.
    let snap = snapshot::read(&dir.save("Hollowmere")).unwrap();
    assert_eq!(snap.tick, 0);
    assert_eq!(snap.header["agent"], "Wren Ashdown");
    assert_eq!(snap.header["world"], "Hollowmere");
    assert!(snap.header["seed"].is_u64() && snap.header["saved"].is_string());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn new_refuses_existing_agent() {
    let home = tmp("exists");
    let _first = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    let Err(e) = build_explorer_app(&new_args(&home, "wren", "Other"), |_| {}) else {
        panic!("a second agent with the same folder was created");
    };
    assert!(e.contains("exists"), "{e}");
    assert!(!needs_choice(&e));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn new_refuses_a_name_the_world_uses() {
    let home = tmp("taken");
    let Err(e) = build_explorer_app(&new_args(&home, "gate", "W"), |_| {}) else {
        panic!("an agent named like a world thing was created");
    };
    assert!(e.contains("gate"), "{e}");
    assert!(!home.join("gate").exists(), "no folder for a refused agent");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn resume_restores_tick_and_position() {
    let home = tmp("resume");
    let (mut app, _, dir) =
        build_explorer_app(&new_args(&home, "Wren", "Hollowmere"), |_| {}).unwrap();
    run_ticks(app.world_mut(), 300);
    let id = actor(&app, "Wren");
    let (at, hash) = (pos(&app, id), state_hash(app.world_mut()));
    let session = app.world().resource::<ExplorerSession>().clone();
    assert_eq!(session.save_path(), dir.save("Hollowmere"));
    save_now(app.world_mut(), &dir, NOW).unwrap();

    let (mut back, start, dir2) =
        build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    assert_eq!(dir2, dir);
    assert_eq!(back.world().resource::<SimClock>().tick, 300);
    assert_eq!(pos(&back, actor(&back, "Wren")), at);
    assert_eq!(state_hash(back.world_mut()), hash);
    assert!(back.world().resource::<SimControl>().paused);
    assert_eq!(
        back.world().resource::<ExplorerSession>().seed,
        session.seed
    );
    assert_eq!(check_save(&home, "Wren"), Ok(()));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn resume_with_other_build_needs_a_choice() {
    let home = tmp("choice");
    let (_, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    edit_save(&dir.save("W"), |v| v["content_hash"] = 12345.into());
    let Err(e) = build_explorer_app(&resume_args(&home, "Wren", None), |_| {}) else {
        panic!("a save from another build resumed silently");
    };
    assert!(needs_choice(&e), "{e}");
    assert!(
        e.contains("--load-anyway") && e.contains("--fresh-world"),
        "{e}"
    );
    let (app, start, _) = build_explorer_app(
        &resume_args(&home, "Wren", Some(Mismatch::LoadAnyway)),
        |_| {},
    )
    .unwrap();
    let Start::ResumedChanged(report) = start else {
        panic!("expected a lenient load, got {start:?}");
    };
    assert!(
        report.skipped.iter().any(|s| s.contains("content hash")),
        "{report:?}"
    );
    assert!(app.world().resource::<TestbedIds>().get("Wren").is_some());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn fresh_world_backs_up_old_save() {
    let home = tmp("fresh");
    let (mut app, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    run_ticks(app.world_mut(), 50);
    save_now(app.world_mut(), &dir, NOW).unwrap();
    let entry = dir.diary().join("2026-10-25 Day 1.md");
    std::fs::write(&entry, "---\nid: 1\ntitle: Day 1\n---\nI saw a tree.\n").unwrap();
    edit_save(&dir.save("W"), |v| v["version"] = 999.into());

    let (app, start, _) = build_explorer_app(
        &resume_args(&home, "Wren", Some(Mismatch::FreshWorld)),
        |_| {},
    )
    .unwrap();
    assert!(matches!(start, Start::Rebuilt), "{start:?}");
    assert_eq!(app.world().resource::<SimClock>().tick, 0);
    assert!(app.world().resource::<TestbedIds>().get("Wren").is_some());
    let backups: Vec<PathBuf> = std::fs::read_dir(&dir.dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            n.starts_with("w.world.") && n != "w.world.json"
        })
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert_eq!(snapshot::read(&backups[0]).unwrap().version, 999);
    assert!(read(&entry).contains("I saw a tree."));
    // The new world is saved and fits.
    assert_eq!(snapshot::read(&dir.save("W")).unwrap().tick, 0);
    assert_eq!(check_save(&home, "Wren"), Ok(()));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn check_save_reports_kind() {
    let home = tmp("check");
    let check = |agent: &str| check_save(&home, agent);
    let kind = |r: CheckJson| r.unwrap_err().0;
    assert_eq!(kind(check("Nobody")), "unreadable");
    let (_, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    assert_eq!(check("Wren"), Ok(()));
    assert_eq!(check_json(&check("Wren")), r#"{"ok":true}"#);
    let save = dir.save("W");
    let good = std::fs::read(&save).unwrap();
    edit_save(&save, |v| v["content_hash"] = 1.into());
    assert_eq!(kind(check("Wren")), "content_hash");
    std::fs::write(&save, &good).unwrap();
    edit_save(&save, |v| v["version"] = 999.into());
    assert_eq!(kind(check("Wren")), "version");
    std::fs::write(&save, "{ not a save").unwrap();
    let r = check("Wren");
    let line = check_json(&r);
    assert_eq!(kind(r), "unreadable");
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["kind"], "unreadable");
    assert!(
        v["error"].as_str().unwrap().contains("unreadable save"),
        "{line}"
    );
    assert!(line.starts_with(r#"{"ok":false,"error":"#), "{line}");
    std::fs::remove_file(&save).unwrap();
    assert_eq!(kind(check("Wren")), "unreadable");
    std::fs::remove_file(dir.meta()).unwrap();
    assert_eq!(kind(check("Wren")), "unreadable");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn prompt_kind_follows_start() {
    let r = LoadReport::default;
    assert_eq!(prompt_kind(&Start::New), PromptKind::Start);
    assert_eq!(prompt_kind(&Start::Resumed(r())), PromptKind::Resume);
    assert_eq!(
        prompt_kind(&Start::ResumedChanged(r())),
        PromptKind::ResumeChanged
    );
    assert_eq!(prompt_kind(&Start::Rebuilt), PromptKind::ResumeRebuilt);
}

/// Task 11 (replaces Ruling R3): a preset is checked before anything is
/// written (here a rover with a LIDAR of 0 beams), and a resume keeps the
/// agent's preset.
#[test]
fn presets_are_checked_before_the_world_starts() {
    let home = tmp("preset");
    let actors = home.join("actors");
    let _ = testbed_content::actors::load_presets(&actors);
    let mut broken: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(actors.join("rover.json")).unwrap()).unwrap();
    broken["sensors"][0]["beams"] = serde_json::json!(0);
    std::fs::write(actors.join("broken.json"), broken.to_string()).unwrap();
    let rover = HostArgs {
        preset: Some("broken".into()),
        ..new_args(&home, "Wren", "W")
    };
    let Err(e) = build_explorer_app(&rover, |_| {}) else {
        panic!("a rover without beams started");
    };
    assert!(e.starts_with("actor preset `broken` is invalid"), "{e}");
    assert!(e.contains("lidar beams 0"), "{e}");
    assert!(
        !home.join("wren").exists(),
        "no folder for a refused preset"
    );
    let human = HostArgs {
        preset: Some("human".into()),
        ..new_args(&home, "Wren", "W")
    };
    let (_, start, _) = build_explorer_app(&human, |_| {}).unwrap();
    assert!(matches!(start, Start::New));
    let resume_rover = HostArgs {
        preset: Some("rover".into()),
        ..resume_args(&home, "Wren", None)
    };
    let Err(e) = build_explorer_app(&resume_rover, |_| {}) else {
        panic!("a human resumed as a rover");
    };
    assert!(e.contains("plays the actor preset `human`"), "{e}");
    let resume_human = HostArgs {
        preset: Some("human".into()),
        ..resume_args(&home, "Wren", None)
    };
    let (_, start, _) = build_explorer_app(&resume_human, |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    let _ = std::fs::remove_dir_all(&home);
}

/// What a detached host needs besides the world: the agent folder's
/// generated files and the notes resources, without a window.
#[test]
fn session_files_and_notes_need_no_window() {
    let home = tmp("files");
    let (mut app, start, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    let npcctl = home.join("npcctl");
    let files = write_session_files(
        app.world(),
        &start,
        &dir,
        "http://127.0.0.1:15703",
        npcctl,
        true,
    )
    .unwrap();
    assert_eq!(files.kind, PromptKind::Start);
    assert_eq!(files.unit, actor(&app, "Wren"));
    assert_eq!(files.write_error, None);
    assert_eq!(files.path, dir.prompt(PromptKind::Start));
    // The file has a frontmatter; the text the modal copies has none.
    let file = read(&files.path);
    assert!(
        file.starts_with("---\ntype: prompt\nagent: Wren\nworld: W\ngenerated: true\n---\n"),
        "{file}"
    );
    assert!(file.ends_with(&files.text), "{file}");
    assert!(!files.text.contains("generated:") && !files.text.starts_with("---"));
    for g in GENERATED {
        assert!(dir.dir.join(g).exists(), "{g} missing");
    }
    assert!(read(dir.dir.join(".mcp.json")).contains("http://127.0.0.1:15703"));

    insert_notes(app.world_mut(), files.unit, &dir, || NOW.to_owned());
    let dirs = &app.world().resource::<NotesDirs>().0;
    assert_eq!(dirs.len(), 1, "{dirs:?}");
    assert_eq!(dirs.get(&files.unit), Some(&dir.dir));
    assert_eq!((app.world().resource::<NotesClock>().0)(), NOW);
    // Entry files carry the agent's and the world's names.
    let added = npc_framework::agent_io::call(
        app.world_mut(),
        "notes/add",
        Some(serde_json::json!({
            "unit": files.unit.0, "book": "memory", "title": "The gate", "body": "North."
        })),
    )
    .unwrap();
    assert_eq!(added["accepted"], true, "{added}");
    let memory = read(dir.memory().join("The gate.md"));
    assert!(
        memory.contains("\nagent: Wren\nworld: W\nauthor: player\ncreated: 2026-10-25T12:00:00Z\n"),
        "{memory}"
    );

    // A folder that cannot be written: the error, the text all the same.
    std::fs::remove_dir_all(dir.dir.join("prompts")).unwrap();
    std::fs::write(dir.dir.join("prompts"), "in the way").unwrap();
    let failed = write_session_files(
        app.world(),
        &start,
        &dir,
        "http://127.0.0.1:15703",
        home.join("npcctl"),
        true,
    )
    .unwrap();
    let e = failed.write_error.expect("the write failed");
    assert!(e.contains("cannot write the agent folder"), "{e}");
    assert_eq!(failed.text, files.text);
    let _ = std::fs::remove_dir_all(&home);
}

/// A pid that ran and exited (a child of this test, waited for).
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--list")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    child.wait().unwrap();
    drop(child);
    pid
}

fn run_file(pid: u32, mode: RunMode, port: u16) -> RunFile {
    RunFile {
        pid,
        port,
        mode,
        started: NOW.into(),
        agent: "Wren".into(),
        world: "W".into(),
    }
}

#[test]
fn run_file_round_trips() {
    let home = tmp("run");
    let (_, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    assert_eq!(RunFile::read_live(&dir), None, "no run.json yet");
    let path = RunFile::path(&dir);
    assert_eq!(path, dir.dir.join("run.json"));
    assert!(
        !GENERATED.iter().any(|g| path.ends_with(g)),
        "run.json is owned"
    );

    let detached = run_file(std::process::id(), RunMode::Detached, 15702);
    detached.write(&dir).unwrap();
    let v: Value = serde_json::from_str(&read(&path)).unwrap();
    assert_eq!(v["mode"], "detached", "{v}");
    assert_eq!(v["pid"], std::process::id());
    assert_eq!(v["port"], 15702);
    assert_eq!(v["started"], NOW);
    assert_eq!(
        (v["agent"].as_str(), v["world"].as_str()),
        (Some("Wren"), Some("W"))
    );
    assert_eq!(RunFile::read_live(&dir), Some(detached.clone()));

    // The window takes over: the file is replaced, no temp file is left.
    let window = run_file(std::process::id(), RunMode::Window, 15703);
    window.write(&dir).unwrap();
    assert_eq!(RunFile::read_live(&dir), Some(window.clone()));
    assert!(read(&path).contains("\"window\""));
    let temps: Vec<_> = std::fs::read_dir(&dir.dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(temps.is_empty(), "{temps:?}");

    window.remove(&dir).unwrap();
    assert!(!path.exists());
    assert_eq!(RunFile::read_live(&dir), None);
    // Removing twice is fine.
    window.remove(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn stale_run_file_is_ignored() {
    let home = tmp("stale");
    let (_, _, dir) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    let path = RunFile::path(&dir);
    let stale = run_file(dead_pid(), RunMode::Detached, 15702);
    stale.write(&dir).unwrap();
    assert_eq!(RunFile::read_live(&dir), None, "a dead pid is not running");
    assert!(path.is_file(), "reading leaves the file alone");

    // Another run never removes a run.json that is not its own.
    let mine = run_file(std::process::id(), RunMode::Window, 15703);
    mine.remove(&dir).unwrap();
    assert!(path.is_file(), "not ours: kept");
    // Writing over a stale file is how a new run takes the folder.
    mine.write(&dir).unwrap();
    assert_eq!(RunFile::read_live(&dir), Some(mine.clone()));

    // An unreadable run.json is ignored too.
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(RunFile::read_live(&dir), None);
    let _ = std::fs::remove_dir_all(&home);
}

/// Moves `agent` to `at` (a test teleport: the explorer's own controller is
/// in `direct` mode, so nothing else moves it).
fn teleport(app: &mut App, agent: SimId, at: SimPos) {
    let e = app.world().resource::<SimIndex>().get(agent).unwrap();
    *app.world_mut().get_mut::<SimPos>(e).unwrap() = at;
}

fn memory(app: &App, agent: SimId) -> PerceptionMemory {
    let e = app.world().resource::<SimIndex>().get(agent).unwrap();
    app.world().get::<PerceptionMemory>(e).unwrap().clone()
}

fn new_explorer(tag: &str) -> (App, SimId, PathBuf) {
    let home = tmp(tag);
    let (app, _, _) = build_explorer_app(&new_args(&home, "Wren", "W"), |_| {}).unwrap();
    let id = actor(&app, "Wren");
    (app, id, home)
}

#[test]
fn explorer_remembers_for_a_minute() {
    let (mut app, wren, home) = new_explorer("memory-minute");
    assert_eq!(
        *app.world().resource::<PerceptionConfig>(),
        PerceptionConfig {
            forget_after_ticks: EXPLORER_FORGET_AFTER,
            memory_capacity: EXPLORER_MEMORY_CAPACITY,
            ..*app.world().resource::<PerceptionConfig>()
        }
    );
    run_ticks(app.world_mut(), 8);
    let mem = memory(&app, wren);
    let (item, seen) = mem
        .percepts
        .iter()
        .filter(|(_, p)| p.sensed && p.via == npc_framework::perception::SenseKind::Vision)
        .map(|(&id, p)| (id, *p))
        .next()
        .expect("the actor sees something at the start");
    // Out of sight of everything it saw: far to the south-west.
    let far = SimPos {
        x: seen.pos.x.saturating_sub(60_000),
        z: seen.pos.z.saturating_sub(60_000),
    };
    teleport(&mut app, wren, far);
    // The sense that lost it stamps `last_seen_tick` on the next scan.
    run_ticks(app.world_mut(), 8);
    let last = memory(&app, wren)
        .get(item)
        .expect("still remembered")
        .last_seen_tick;
    let now = app.world().resource::<SimClock>().tick;
    let to = |app: &mut App, ticks_after_last: u64| {
        let now = app.world().resource::<SimClock>().tick;
        let want = last + ticks_after_last;
        assert!(want >= now, "already past {ticks_after_last}");
        run_ticks(app.world_mut(), want - now);
    };
    assert!(now > last);
    to(&mut app, 59 * 20);
    assert!(memory(&app, wren).get(item).is_some(), "59 s: remembered");
    to(&mut app, 61 * 20);
    assert!(memory(&app, wren).get(item).is_none(), "61 s: forgotten");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn explorer_memory_holds_more_than_32() {
    let (mut app, wren, home) = new_explorer("memory-128");
    let mut spots: Vec<SimPos> = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&SimPos, bevy_ecs::prelude::With<Perceivable>>();
        q.iter(world).copied().collect()
    };
    spots.sort_by_key(|p| (p.x, p.z));
    let mut most = 0;
    for p in spots {
        teleport(&mut app, wren, p);
        run_ticks(app.world_mut(), 4);
        most = most.max(memory(&app, wren).percepts.len());
    }
    assert!(most > 32, "remembered at most {most}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn resumed_old_save_gets_the_new_memory() {
    let (mut app, _, home) = new_explorer("memory-resume");
    let dir = app.world().resource::<ExplorerSession>().dir.clone();
    // A save written with the framework defaults, as before this change.
    let old = PerceptionConfig {
        forget_after_ticks: DEFAULT_FORGET_AFTER,
        memory_capacity: 32,
        ..*app.world().resource::<PerceptionConfig>()
    };
    app.insert_resource(old);
    save_now(app.world_mut(), &dir, NOW).unwrap();
    let (back, start, _) = build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    let c = back.world().resource::<PerceptionConfig>();
    assert_eq!(c.forget_after_ticks, EXPLORER_FORGET_AFTER);
    assert_eq!(c.memory_capacity, EXPLORER_MEMORY_CAPACITY);
    assert_eq!(c.stagger, old.stagger);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn other_scenarios_keep_the_default_memory() {
    let app = testbed_content::scenario::try_new_sim_app(1, "testbed", 0).unwrap();
    let c = app.world().resource::<PerceptionConfig>();
    assert_eq!(c.forget_after_ticks, DEFAULT_FORGET_AFTER);
    assert_eq!(
        c.memory_capacity,
        npc_framework::perception::DEFAULT_MEMORY_CAPACITY
    );
}

// ------------------------------------------- Task 27 (spec F3): reach sense

fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

fn named(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().get(name).unwrap()
}

/// Moves `agent` to `at` and turns it to `facing` (after the tick in which
/// the teleport would have turned it).
fn stand(app: &mut App, agent: SimId, at: SimPos, facing: (i32, i32)) {
    teleport(app, agent, at);
    run_ticks(app.world_mut(), 1);
    let e = entity(app, agent);
    *app.world_mut().get_mut::<Facing>(e).unwrap() = Facing::new(facing.0, facing.1);
}

fn observe_json(app: &mut App, unit: SimId) -> Value {
    npc_framework::agent_io::api::call(
        app.world_mut(),
        "agent/observe",
        Some(serde_json::json!({"unit": unit.0})),
    )
    .unwrap()
}

#[test]
fn explorer_actor_has_reach() {
    let (app, wren, home) = new_explorer("reach-on");
    assert_eq!(
        app.world().get::<ReachSense>(entity(&app, wren)),
        Some(&ReachSense {
            radius_mm: EXPLORER_REACH_MM
        })
    );
    assert_eq!(EXPLORER_REACH_MM, 1500);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn resumed_old_save_gets_reach() {
    let (mut app, wren, home) = new_explorer("reach-resume");
    let dir = app.world().resource::<ExplorerSession>().dir.clone();
    // A save written before the reach sense existed.
    let e = entity(&app, wren);
    app.world_mut().entity_mut(e).remove::<ReachSense>();
    save_now(app.world_mut(), &dir, NOW).unwrap();
    let (back, start, _) = build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    assert_eq!(
        back.world().get::<ReachSense>(entity(&back, wren)),
        Some(&ReachSense {
            radius_mm: EXPLORER_REACH_MM
        })
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Proposal 011: in the walled ruin, the chest half a metre behind the
/// actor is sensed (and remembered), facing away from it.
#[test]
fn chest_behind_in_reach_is_sensed() {
    let (mut app, wren, home) = new_explorer("reach-chest");
    let chest = named(&app, "ruin_chest");
    let at = pos(&app, chest);
    // East of the chest, facing east: the chest is behind.
    stand(&mut app, wren, SimPos::new(at.x + 600, at.z - 300), (1, 0));
    run_ticks(app.world_mut(), 8);
    let p = memory(&app, wren)
        .get(chest)
        .copied()
        .expect("the chest is sensed");
    assert!(p.sensed);
    assert_eq!(p.via, SenseKind::Reach);
    let obs = observe_json(&mut app, wren);
    let reach = &obs["sections"]["perception"]["within_reach"];
    assert!(
        reach
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(chest.0)),
        "{reach}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Proposal 007: standing 1.3 m from the forge and turned away, it stays
/// sensed past the memory span, so its crafts stay offered.
#[test]
fn station_in_reach_stays_usable_after_turning() {
    let (mut app, wren, home) = new_explorer("reach-forge");
    let forge = named(&app, "forge");
    let at = pos(&app, forge);
    // South of the forge, facing south: the forge is behind.
    stand(&mut app, wren, SimPos::new(at.x, at.z - 1300), (0, -1));
    run_ticks(app.world_mut(), EXPLORER_FORGET_AFTER + 40);
    let p = memory(&app, wren).get(forge).copied().expect("known");
    assert!(p.sensed, "{p:?}");
    assert_eq!(p.via, SenseKind::Reach);
    let obs = observe_json(&mut app, wren);
    let suffix = format!("@{}", forge.0);
    let offered = obs["affordances"].as_array().unwrap().iter().any(|a| {
        a["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("craft:") && id.ends_with(&suffix))
    });
    assert!(offered, "no craft at the forge: {}", obs["affordances"]);
    let _ = std::fs::remove_dir_all(&home);
}

/// Scenarios without the reach sense keep the content and state hashes
/// recorded on `main` before Task 27 (`explorer` here is the plain
/// scenario, not a host-built explorer world).
#[test]
fn scenarios_without_reach_keep_their_hash() {
    for (scenario, content, state) in [
        ("testbed", 0xd8a1bc52b1247a36_u64, 0xb9f394ce322c6b4e_u64),
        ("explorer", 0xd8a1bc52b1247a36, 0x197a2048050deccb),
        ("smithy", 0x7d7a7460c2816513, 0xd826f1d690755af3),
    ] {
        let mut app = testbed_content::scenario::try_new_sim_app(1, scenario, 0).unwrap();
        assert_eq!(
            npc_framework::kernel::registry::content_hash(app.world_mut()),
            content,
            "{scenario} content hash"
        );
        run_ticks(app.world_mut(), 600);
        assert_eq!(state_hash(app.world_mut()), state, "{scenario} state hash");
    }
}

// ------------------------------------------------------------ fog of war

fn fog(app: &App, id: SimId) -> (ExploredCells, VisibleCells, SeenStatics) {
    let w = app.world();
    let e = entity(app, id);
    (
        w.get::<ExploredCells>(e).cloned().expect("explored cells"),
        w.get::<VisibleCells>(e).cloned().expect("visible cells"),
        w.get::<SeenStatics>(e).cloned().expect("seen statics"),
    )
}

fn is_explored(app: &App, id: SimId, cell: (i32, i32)) -> bool {
    let grid = app.world().resource::<SpatialGrid>();
    fog(app, id)
        .0
        .is_explored(grid, CellCoord::new(cell.0, cell.1))
}

fn is_visible(app: &App, id: SimId, cell: (i32, i32)) -> bool {
    let grid = app.world().resource::<SpatialGrid>();
    fog(app, id)
        .1
        .is_visible(grid, CellCoord::new(cell.0, cell.1))
}

/// Plan Task 9 (spec G8): the explorer actor reveals fog; the walls and
/// statics it sees are remembered, the units are not.
#[test]
fn explorer_actor_reveals_fog() {
    let (mut app, wren, home) = new_explorer("fog-on");
    assert!(app.world().get::<RevealsFog>(entity(&app, wren)).is_some());
    run_ticks(app.world_mut(), 8);
    let (explored, visible, seen) = fog(&app, wren);
    assert!(visible.count() > 50, "{}", visible.count());
    assert!(explored.count() >= visible.count());
    assert!(seen.0.values().any(|s| s.kind == kinds::WALL), "walls");
    // Facing north from the yard: both inner doors, closed, and the
    // items lying in the yard.
    for door in ["workshop_door", "store_door"] {
        assert_eq!(seen.0[&named(&app, door)].open, Some(false), "{door}");
    }
    let key = seen.0[&named(&app, "store_key")];
    assert_eq!((key.kind, key.open, key.felled), (kinds::ITEM, None, None));
    assert!(
        seen.0
            .values()
            .all(|s| s.kind & (kinds::UNIT | kinds::VEHICLE) == 0),
        "units and vehicles are never last-seen statics"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// The workshop door, closed: the workshop behind it stays black; opened,
/// it is revealed. Seen open, closed unseen: remembered open until seen.
#[test]
fn closed_door_blocks_fog_and_is_remembered_as_seen() {
    let (mut app, wren, home) = new_explorer("fog-door");
    let door = named(&app, "workshop_door");
    let door_e = entity(&app, door);
    let set_open = |app: &mut App, open: bool| {
        app.world_mut().get_mut::<Door>(door_e).unwrap().open = open;
    };
    // Two cells south of the workshop door (-6, 3), facing it.
    stand(&mut app, wren, SimPos::new(-5500, 1500), (0, 1));
    run_ticks(app.world_mut(), 8);
    assert!(is_explored(&app, wren, (-6, 3)), "the door itself");
    assert!(
        !is_explored(&app, wren, (-6, 5)),
        "the workshop, door closed"
    );
    assert_eq!(fog(&app, wren).2.0[&door].open, Some(false));

    set_open(&mut app, true);
    run_ticks(app.world_mut(), 8);
    assert!(is_explored(&app, wren, (-6, 5)) && is_visible(&app, wren, (-6, 5)));
    assert_eq!(fog(&app, wren).2.0[&door].open, Some(true));

    // Turned away, the door closes unseen: still open in the record.
    stand(&mut app, wren, SimPos::new(-5500, 1500), (0, -1));
    run_ticks(app.world_mut(), 8);
    set_open(&mut app, false);
    run_ticks(app.world_mut(), 8);
    assert!(!is_visible(&app, wren, (-6, 3)));
    assert!(is_explored(&app, wren, (-6, 5)), "explored stays");
    assert_eq!(
        fog(&app, wren).2.0[&door].open,
        Some(true),
        "last seen open"
    );

    stand(&mut app, wren, SimPos::new(-5500, 1500), (0, 1));
    run_ticks(app.world_mut(), 8);
    assert_eq!(fog(&app, wren).2.0[&door].open, Some(false), "seen closed");
    let _ = std::fs::remove_dir_all(&home);
}

/// The explored ground is kept in the world save (spec G8): a resumed world
/// has the same explored cells, visible cells and last-seen statics.
#[test]
fn resume_keeps_the_explored_ground() {
    let (mut app, wren, home) = new_explorer("fog-resume");
    for (at, facing) in [
        (SimPos::new(0, -6500), (0, 1)),
        (SimPos::new(-8500, -2500), (1, 0)),
        (SimPos::new(8500, 6500), (-1, 0)),
    ] {
        stand(&mut app, wren, at, facing);
        run_ticks(app.world_mut(), 8);
    }
    let before = fog(&app, wren);
    assert!(
        before.0.count() > before.1.count(),
        "explored beyond what is visible"
    );
    let dir = app.world().resource::<ExplorerSession>().dir.clone();
    save_now(app.world_mut(), &dir, NOW).unwrap();
    let (back, start, _) = build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    assert!(
        back.world()
            .get::<RevealsFog>(entity(&back, wren))
            .is_some()
    );
    assert_eq!(fog(&back, wren), before);
    let _ = std::fs::remove_dir_all(&home);
}

/// Fog is snapshot-only: the host-built explorer world runs to the same
/// state hash with the actor revealing fog or not.
#[test]
fn fog_does_not_change_the_explorer_world_hash() {
    let (mut with, wren, home) = new_explorer("fog-hash-a");
    let (mut without, _, home_b) = new_explorer("fog-hash-b");
    let e = entity(&without, wren);
    without
        .world_mut()
        .entity_mut(e)
        .remove::<(RevealsFog, ExploredCells, VisibleCells, SeenStatics)>();
    for app in [&mut with, &mut without] {
        stand(app, wren, SimPos::new(-5500, 1500), (0, 1));
        run_ticks(app.world_mut(), 600);
    }
    assert!(fog(&with, wren).0.count() > 0);
    assert_eq!(
        state_hash(with.world_mut()),
        state_hash(without.world_mut())
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&home_b);
}

/// Review fix 3: the actor shut in the workshop (its door closed) explores
/// nothing outside the room, whichever way it faces, also through its
/// 1.5 m reach at the door; with the door open the yard shows.
#[test]
fn shut_in_a_room_explores_only_the_room() {
    let (mut app, wren, home) = new_explorer("fog-room");
    let e = entity(&app, wren);
    // Just inside the workshop door (-6, 3), before any scan.
    teleport(&mut app, wren, SimPos::new(-5500, 4500));
    *app.world_mut().get_mut::<ExploredCells>(e).unwrap() = ExploredCells::default();
    let door = named(&app, "workshop_door");
    assert!(!app.world().get::<Door>(entity(&app, door)).unwrap().open);
    // The workshop and its walls: x -12..=0, z 3..=10.
    let in_room = |c: CellCoord| (-12..=0).contains(&c.x) && (3..=10).contains(&c.z);
    let explored_cells = |app: &App| {
        let grid = app.world().resource::<SpatialGrid>();
        let b = grid.bounds();
        let x = fog(app, wren).0;
        (0..u32::try_from(b.len()).unwrap())
            .filter(|&i| x.get(i))
            .map(|i| b.coord(i))
            .collect::<Vec<_>>()
    };
    for facing in [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (-1, -1)] {
        stand(&mut app, wren, SimPos::new(-5500, 4500), facing);
        run_ticks(app.world_mut(), 8);
        let outside: Vec<_> = explored_cells(&app)
            .into_iter()
            .filter(|&c| !in_room(c))
            .collect();
        assert!(outside.is_empty(), "facing {facing:?}: {outside:?}");
    }
    assert!(is_explored(&app, wren, (-6, 3)), "the door's own cell");
    assert!(is_explored(&app, wren, (-11, 9)), "the room's far corner");
    // Door open: the yard behind it is seen.
    let door_e = entity(&app, door);
    app.world_mut().get_mut::<Door>(door_e).unwrap().open = true;
    stand(&mut app, wren, SimPos::new(-5500, 4500), (0, -1));
    run_ticks(app.world_mut(), 8);
    assert!(is_explored(&app, wren, (-6, 0)), "through the open door");
    let _ = std::fs::remove_dir_all(&home);
}

/// Review fix 2: a save whose explored record was made for another grid
/// resumes with the explored ground started over, and the load report says
/// so.
#[test]
fn resume_reports_an_explored_record_that_starts_over() {
    let (mut app, wren, home) = new_explorer("fog-reset");
    run_ticks(app.world_mut(), 8);
    let dir = app.world().resource::<ExplorerSession>().dir.clone();
    save_now(app.world_mut(), &dir, NOW).unwrap();
    let save = dir.save(&app.world().resource::<ExplorerSession>().meta.world);
    edit_save(&save, |v| {
        for entity in v["entities"].as_array_mut().unwrap() {
            if let Some(x) = entity["components"].get_mut("explored_cells") {
                x["min"] = serde_json::json!([0, 0]);
            }
        }
    });
    let (back, start, _) = build_explorer_app(&resume_args(&home, "Wren", None), |_| {}).unwrap();
    let Start::Resumed(report) = start else {
        panic!("{start:?}");
    };
    assert_eq!(report.skipped, vec![fog_reset_note(&[wren])], "{report:?}");
    let (explored, ..) = fog(&back, wren);
    assert_eq!(explored.count(), 0, "started over");
    let _ = std::fs::remove_dir_all(&home);
}

/// Proposal 004 (spec F4): the explorer's observation says how much nearby
/// ground it has searched, once it has sensed some.
#[test]
fn explorer_observation_shows_searched_ground() {
    let (mut app, wren, home) = new_explorer("searched");
    for (at, facing) in [
        (SimPos::new(0, -6500), (0, 1)),
        (SimPos::new(-8500, -2500), (1, 0)),
    ] {
        stand(&mut app, wren, at, facing);
        run_ticks(app.world_mut(), 8);
    }
    let obs = observe_json(&mut app, wren);
    let s = &obs["sections"]["perception"]["searched"];
    assert_eq!(s["cell_mm"], serde_json::json!(10_000), "{s}");
    let sectors = s["sectors"].as_array().unwrap();
    assert_eq!(sectors.len(), 8);
    let sum = |k: &str| sectors.iter().map(|x| x[k].as_u64().unwrap()).sum::<u64>();
    let (explored, total) = (sum("explored"), sum("total"));
    assert!(explored > 0 && explored < total, "{explored}/{total}");
    assert_eq!(
        s["within_radius_pct"].as_u64().unwrap(),
        explored * 100 / total
    );
    let map_pct = s["map_pct"].as_u64().unwrap();
    assert!((1..100).contains(&map_pct), "{map_pct}");
    let text = npc_framework::agent_io::api::call(
        app.world_mut(),
        "agent/observe",
        Some(serde_json::json!({"unit": wren.0, "format": "text"})),
    )
    .unwrap()["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let line = text.lines().find(|l| l.starts_with("Searched:")).unwrap();
    assert!(
        line.starts_with(&format!(
            "Searched: {}% within 60 m, {map_pct}% of the map",
            s["within_radius_pct"]
        )),
        "{line}"
    );
    let _ = std::fs::remove_dir_all(&home);
}
