//! Hot reload, game side (ADR-0012): the watcher, the value and rebuild paths, the visual
//! refresh and the replay capture, on a headless testbed over a temp copy of the base mod.
//!
//! The real threaded runner answers, so the tests wait for it in bounded loops; the fake
//! runners of the last tests answer only when the test says so.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::LocalDriver;
use fl_client::debug::run_console_line;
use fl_client::link::{
    InputLog, PendingReplies, RunnerLink, RunnerStatus, SimJournalMsg, SnapshotBuffer, send_query,
    wait_for_reply,
};
use fl_client::map_view::MapView;
use fl_client::placeholder::PlaceholderAssets;
use fl_client::view::{ClientContent, UnitView, ViewIndex};
use fl_content::TerrainDef;
use fl_game::{
    ContentSource, ContentWatcher, GameBuilder, MatchTerrain, NetDriven, ReloadOutcome,
    ReplayCapture, reload_content,
};
use fl_protocol::{SimId, Tick};
use fl_sim::{Replay, ReplayEvent, VerifyOutcome, verify_replay};
use fl_view::{FromRunner, SimQuery, SimReply, TickDriver, ToRunner, runner_channel};

/// One sim tick per frame.
const FRAME: Duration = Duration::from_millis(50);
const MAX_UPDATES: usize = 4000;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

fn last_tick(app: &App) -> Option<Tick> {
    app.world().resource::<RunnerStatus>().last_tick
}

/// A headless testbed on a temp copy of the base mod.
struct Testbed {
    dir: tempfile::TempDir,
    app: App,
}

impl Testbed {
    fn new() -> Self {
        Self::with(|builder| builder, |_| {}, |_| {})
    }

    /// `builder` configures the game, `before` edits the mod copy before the match starts,
    /// and `setup` changes the app before its first frame.
    fn with(
        builder: impl FnOnce(GameBuilder) -> GameBuilder,
        before: impl FnOnce(&Path),
        setup: impl FnOnce(&mut App),
    ) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = dir.path().join("base");
        copy_dir(&fl_catalog::base_mod_dir(), &base);
        before(&base);
        let mut app = builder(GameBuilder::new().content_mod(&base))
            .prepare_headless()
            .expect("the testbed prepares");
        app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
        setup(&mut app);
        let mut bed = Testbed { dir, app };
        bed.run_until("the first snapshot", |app| last_tick(app).is_some());
        bed
    }

    fn base(&self) -> PathBuf {
        self.dir.path().join("base")
    }

    /// Replaces `from` (which must be there) by `to` in the mod file `file`.
    fn edit(&self, file: &str, from: &str, to: &str) {
        let path = self.base().join(file);
        let text = std::fs::read_to_string(&path).expect("read mod file");
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).expect("write mod file");
    }

    fn write(&self, file: &str, text: &str) {
        std::fs::write(self.base().join(file), text).expect("write mod file");
    }

    fn remove(&self, file: &str) {
        std::fs::remove_file(self.base().join(file)).expect("remove mod file");
    }

    fn world(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// Updates until `done`; panics after [`MAX_UPDATES`].
    fn run_until(&mut self, what: &str, mut done: impl FnMut(&mut App) -> bool) {
        for _ in 0..MAX_UPDATES {
            self.app.update();
            if done(&mut self.app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let failed = self.app.world().resource::<RunnerStatus>().failed.clone();
        panic!("{what}: not within {MAX_UPDATES} updates (runner: {failed:?})");
    }

    /// Runs until `n` more ticks than now were received.
    fn run_ticks(&mut self, n: u32) {
        let from = last_tick(&self.app).map_or(0, |t| t.0);
        self.run_until("ticks to advance", |app| {
            last_tick(app).is_some_and(|t| t.0 >= from + n)
        });
    }

    /// Pauses, and waits until every tick sent has been received.
    fn pause(&mut self) {
        self.world().resource_mut::<LocalDriver>().pause();
        self.run_until("the sent ticks to arrive", |app| {
            let next = app.world().resource::<LocalDriver>().next_tick();
            last_tick(app).is_some_and(|t| t.0 + 1 == next.0)
        });
    }

    fn next_tick(&self) -> Tick {
        self.app.world().resource::<LocalDriver>().next_tick()
    }

    fn query(&mut self, q: SimQuery) -> SimReply {
        let world = self.world();
        let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
            send_query(world.resource::<RunnerLink>(), &mut replies, q)
        });
        wait_for_reply(world, id, Duration::from_secs(10)).expect("the runner answers")
    }

    /// `(hp, max)` of unit `id`.
    fn health(&mut self, id: u32) -> (u64, u64) {
        let SimReply::Entity(Some(entity)) = self.query(SimQuery::Entity(SimId(id))) else {
            panic!("unit {id} exists");
        };
        let health = &entity["components"]["core.health"];
        (
            health["hp"].as_u64().expect("hp"),
            health["max"].as_u64().expect("max"),
        )
    }

    fn entities(&mut self) -> Vec<u64> {
        let SimReply::WorldSummary(summary) = self.query(SimQuery::WorldSummary) else {
            panic!("a world summary");
        };
        summary["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .map(|id| id.as_u64().expect("sim id"))
            .collect()
    }

    /// Runs console command `line` (a `debug.*` command), then until the sim has run the tick
    /// it executes on (the driver stamps it for the tick after the next one it sends).
    fn command(&mut self, line: &str) {
        let driver = self.app.world().resource::<LocalDriver>();
        let exec = driver.next_tick().0 + driver.input_delay();
        self.console(line);
        self.run_until("the command's tick", |app| {
            last_tick(app).is_some_and(|t| t.0 >= exec)
        });
    }

    fn console(&mut self, line: &str) -> String {
        let out = run_console_line(self.world(), line);
        assert!(!out.starts_with("error"), "{line}: {out}");
        out
    }

    fn reload(&mut self) -> ReloadOutcome {
        reload_content(self.world())
    }

    fn content(&self) -> Arc<fl_content::ContentDb> {
        self.app.world().resource::<ClientContent>().0.clone()
    }

    fn rules_hash(&self) -> u64 {
        self.app.world().resource::<RunnerLink>().header.rules_hash
    }
}

const PZ3: &str = "units/de/pz3.unit.ron";
const PZ4: &str = r#"Unit(
    id: "base.de.pz4",
    inherits: Some("base.abstract.medium_tank"),
    name: "Panzer IV",
    components: {
        "Hull": (hp: 130, footprint: (2.9, 5.9), hitbox_shrink: 0.5),
    },
)
"#;
const PZ3_HP: &str = "\"Hull\": (hp: 110,";

#[test]
fn value_edit_reloads_in_place() {
    let mut bed = Testbed::new();
    bed.command("sethp 2 55");
    assert_eq!(bed.health(2), (55, 110));
    let old_hash = bed.rules_hash();

    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let ReloadOutcome::Values { rules_hash, .. } = bed.reload() else {
        panic!("a value edit reloads in place");
    };
    assert_ne!(rules_hash, old_hash);
    assert_eq!(bed.rules_hash(), rules_hash, "the client's header follows");
    assert_eq!(bed.health(2), (110, 220), "max 220, hp at the same ratio");
    assert_eq!(bed.content().rules_hash(), rules_hash);
}

#[test]
fn adding_a_unit_rebuilds_with_same_sim_ids() {
    let mut bed = Testbed::new();
    bed.run_ticks(3);
    bed.pause();
    let before = bed.entities();
    let next = bed.next_tick();

    bed.write(
        "units/de/pz4.unit.ron",
        r#"Unit(
    id: "base.de.pz4",
    inherits: Some("base.abstract.medium_tank"),
    name: "Panzer IV",
    components: {
        "Hull": (hp: 130, footprint: (2.9, 5.9), hitbox_shrink: 0.5),
    },
)
"#,
    );
    bed.write(
        "visuals/de_pz4.visual.ron",
        r#"VisualDef(unit: "base.de.pz4", placeholder: Tank(hull: (2.9, 1.0, 5.9), turret: (1.7, 0.7, 1.9), barrel_len: 2.6), scale: 1.0, recolor: true)
"#,
    );
    let outcome = bed.reload();
    assert_eq!(
        outcome,
        ReloadOutcome::Rebuilt {
            tick: next,
            dropped: Vec::new()
        }
    );
    assert_eq!(bed.next_tick(), next, "the tick is unchanged");
    assert_eq!(bed.entities(), before, "every unit keeps its sim id");
    assert!(bed.content().unit_id("base.de.pz4").is_some());
    let rules_hash = bed.content().rules_hash();
    assert_eq!(
        bed.rules_hash(),
        rules_hash,
        "the rebuilt sim runs the new content"
    );
    assert_eq!(
        bed.app.world().resource::<ReplayCapture>().events(),
        [ReplayEvent::ContentReload {
            tick: next,
            rules_hash
        }],
        "the rebuild is a replay event too"
    );

    // The rebuilt match runs on.
    bed.world().resource_mut::<LocalDriver>().resume();
    bed.run_ticks(3);
}

#[test]
fn removing_a_unit_type_drops_its_units() {
    let mut bed = Testbed::new();
    bed.run_ticks(2);
    let before = bed.entities();
    assert!(before.contains(&3), "#3 is the testbed's tiger");

    bed.remove("units/de/tiger.unit.ron");
    bed.remove("visuals/de_tiger.visual.ron");
    bed.edit("factions/de.faction.ron", "\"base.de.tiger\", ", "");
    let tiger_line =
        "        (unit: \"base.de.tiger\", player: 0, pos: (410.0, 350.0), heading_deg: 90.0),\n";
    bed.edit("scenarios/testbed.scenario.ron", tiger_line, "");
    bed.edit("scenarios/testbed_ai.scenario.ron", tiger_line, "");

    let ReloadOutcome::Rebuilt { dropped, .. } = bed.reload() else {
        panic!("removing a unit type rebuilds");
    };
    assert_eq!(dropped, vec![(SimId(3), "base.de.tiger".to_owned())]);
    let after = bed.entities();
    let expected: Vec<u64> = before.into_iter().filter(|&id| id != 3).collect();
    assert_eq!(after, expected, "the survivors keep their sim ids");
    bed.run_ticks(3);
}

#[test]
fn malformed_ron_keeps_match_running() {
    let mut bed = Testbed::new();
    let content = bed.content();
    let path = bed.base().join(PZ3);
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, &text[..text.len() / 2]).expect("truncate");

    let ReloadOutcome::Failed(error) = bed.reload() else {
        panic!("a truncated file fails the reload");
    };
    assert!(error.contains("pz3.unit.ron"), "{error}");
    assert!(Arc::ptr_eq(&bed.content(), &content), "the content stays");
    bed.run_ticks(3);
    assert_eq!(bed.health(2), (110, 110));
}

#[test]
fn reload_refused_without_cheats() {
    let mut bed = Testbed::with(|builder| builder.cheats(false), |_| {}, |_| {});
    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let outcome = bed.reload();
    assert!(
        matches!(&outcome, ReloadOutcome::Refused(reason) if reason.contains("cheats")),
        "{outcome:?}"
    );
    assert_eq!(bed.health(2), (110, 110));
}

#[test]
fn reload_refused_in_lockstep_match() {
    // Carried from Task 12's review: cheats_allowed alone is not enough.
    let mut bed = Testbed::new();
    bed.world().insert_resource(NetDriven);
    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let outcome = bed.reload();
    assert!(
        matches!(&outcome, ReloadOutcome::Refused(reason) if reason.contains("lockstep")),
        "{outcome:?}"
    );
    assert_eq!(bed.health(2), (110, 110));
    let capture = bed.app.world().resource::<ReplayCapture>();
    assert!(capture.events().is_empty());
}

#[test]
fn reload_while_paused_applies() {
    let mut bed = Testbed::new();
    bed.run_ticks(2);
    bed.pause();
    let next = bed.next_tick();
    let last = last_tick(&bed.app);

    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let ReloadOutcome::Values { tick, .. } = bed.reload() else {
        panic!("a paused match reloads in place");
    };
    assert_eq!(tick, next);
    for _ in 0..5 {
        bed.app.update();
    }
    assert!(bed.app.world().resource::<LocalDriver>().is_paused());
    assert_eq!(bed.next_tick(), next, "no tick ran");
    assert_eq!(last_tick(&bed.app), last);
    assert_eq!(bed.health(2), (220, 220));
}

fn with_placeholders(app: &mut App) {
    app.add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>();
}

/// The scale of the first part of unit `id`'s view.
fn part_scale(app: &App, id: u32) -> Option<Vec3> {
    let view = *app.world().resource::<ViewIndex>().0.get(&SimId(id))?;
    let children = app.world().get::<Children>(view)?;
    Some(app.world().get::<Transform>(*children.first()?)?.scale)
}

#[test]
fn visual_only_change_swaps_client_content() {
    let mut bed = Testbed::with(|builder| builder, |_| {}, with_placeholders);
    bed.run_until("pz3 parts", |app| part_scale(app, 2).is_some());
    let pz3 = bed.content().unit_id("base.de.pz3").expect("pz3");
    let meshes = |bed: &Testbed| bed.app.world().resource::<PlaceholderAssets>().meshes(pz3);
    let before = meshes(&bed).expect("pz3 parts built");
    let (content, rules_hash) = (bed.content(), bed.rules_hash());

    bed.edit("visuals/de_pz3.visual.ron", "scale: 1.0", "scale: 1.5");
    assert_eq!(bed.reload(), ReloadOutcome::VisualsOnly);
    assert!(
        !Arc::ptr_eq(&bed.content(), &content),
        "the content is swapped"
    );
    assert_eq!(bed.rules_hash(), rules_hash);
    bed.run_until("the parts to rebuild", |app| {
        part_scale(app, 2) == Some(Vec3::splat(1.5))
    });
    let after = meshes(&bed).expect("pz3 parts rebuilt");
    assert_ne!(after, before, "PlaceholderAssets were rebuilt");
}

#[test]
fn reload_records_replay_event() {
    let mut bed = Testbed::new();
    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let ReloadOutcome::Values { tick, rules_hash } = bed.reload() else {
        panic!("a value edit reloads in place");
    };
    let path = bed.dir.path().join("reload.replay");
    bed.console(&format!("replay save {}", path.display()));
    let replay =
        Replay::from_bytes(&std::fs::read(&path).expect("replay written")).expect("decodes");
    assert_eq!(
        replay.events,
        vec![ReplayEvent::ContentReload { tick, rules_hash }]
    );
    assert!(!replay.reproducible());
    let verified = verify_replay(&replay, &fl_catalog::features(), bed.content());
    assert_eq!(verified.ok(), Some(VerifyOutcome::NonReproducible));
}

#[test]
fn replay_without_reload_verifies() {
    let mut bed = Testbed::new();
    bed.command("sethp 2 55");
    bed.run_ticks(45);
    bed.pause();
    let path = bed.dir.path().join("plain.replay");
    bed.console(&format!("replay save {}", path.display()));
    let replay =
        Replay::from_bytes(&std::fs::read(&path).expect("replay written")).expect("decodes");
    assert!(replay.reproducible());
    assert!(!replay.inputs.is_empty(), "the sethp command is recorded");
    let log = bed.app.world().resource::<InputLog>();
    assert!(
        log.inputs.iter().all(|input| !input.commands.is_empty()),
        "the log keeps only inputs with commands"
    );
    assert_eq!(replay.ticks, log.ticks);
    assert!(log.ticks >= 45);
    assert!(replay.hashes.len() >= 2, "{:?}", replay.hashes);
    let verified = verify_replay(&replay, &fl_catalog::features(), bed.content()).expect("plays");
    assert!(
        matches!(verified, VerifyOutcome::Verified { ticks, .. } if ticks == replay.ticks),
        "{verified:?}"
    );
}

#[test]
fn watcher_detects_change() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(dir.path().join("units")).expect("subdir");
    let file = dir.path().join("units/a.unit.ron");
    std::fs::write(&file, "x").expect("write");
    let mut watcher = ContentWatcher::new(vec![dir.path().to_owned()], Duration::from_millis(500));
    assert!(!watcher.scan());
    std::fs::write(&file, "xyz").expect("rewrite");
    assert!(watcher.scan(), "a new length is a change");
    assert!(!watcher.scan());
    std::fs::write(dir.path().join("b.ron"), "y").expect("add");
    assert!(watcher.scan(), "a new file is a change");
    std::fs::remove_file(&file).expect("remove");
    assert!(watcher.scan(), "a removed file is a change");
    assert!(!watcher.scan());
}

#[test]
fn failed_rebuild_keeps_match_running() {
    // The testbed's terrain PNG paints every base kind, so removing one of them is a content
    // error; a kind no map uses, painted only by debug.set_terrain, reaches the save alone.
    let mud = r#"TerrainDef(id: "base.test_mud", name: "Mud", pass: 0.5, blocks: [], color: (0.4, 0.3, 0.2))
"#;
    let mut bed = Testbed::with(
        |builder| builder,
        |base| std::fs::write(base.join("terrain/test_mud.terrain.ron"), mud).expect("mud"),
        |_| {},
    );
    bed.command("terrain 10 10 12 12 base.test_mud");
    let content = bed.content();
    bed.remove("terrain/test_mud.terrain.ron");

    let ReloadOutcome::Failed(error) = bed.reload() else {
        panic!("a rebuild whose save names a vanished terrain fails");
    };
    assert!(error.contains("core.terrain"), "{error}");
    assert!(Arc::ptr_eq(&bed.content(), &content), "the content stays");
    bed.run_ticks(3);
    assert_eq!(bed.health(2), (110, 110), "the match keeps its state");
}

#[test]
fn terrain_view_follows_set_terrain() {
    // Carried from Task 17: the drawn terrain follows debug.set_terrain.
    let mut bed = Testbed::new();
    let colour = |bed: &Testbed, kind: &str| {
        let t = bed
            .content()
            .kind::<TerrainDef>()
            .get(kind)
            .expect("kind")
            .color;
        [t.0, t.1, t.2]
    };
    let cell = |app: &App| {
        app.world()
            .resource::<MapView>()
            .terrain
            .as_ref()
            .expect("terrain")
            .colors[0]
    };
    let kind = if cell(&bed.app) == colour(&bed, "base.water") {
        "base.road"
    } else {
        "base.water"
    };
    let want = colour(&bed, kind);
    bed.console(&format!("terrain 0 0 1 1 {kind}"));
    bed.run_until("the cell to change colour", |app| cell(app) == want);
    let grid = bed
        .app
        .world()
        .resource::<MatchTerrain>()
        .grid
        .clone()
        .expect("grid");
    assert_eq!(grid.terrain_at(1, 1), Some(kind));
}

#[test]
fn terrain_view_follows_a_colour_reload() {
    // Carried from Task 17: the drawn terrain follows a content reload.
    let mut bed = Testbed::new();
    let grass_cell = {
        let grid = bed
            .app
            .world()
            .resource::<MatchTerrain>()
            .grid
            .clone()
            .expect("grid");
        let grass = grid
            .palette
            .iter()
            .position(|id| id == "base.grass")
            .expect("grass");
        grid.types
            .iter()
            .position(|&t| usize::from(t) == grass)
            .expect("a grass cell")
    };
    bed.edit(
        "terrain/grass.terrain.ron",
        "(0.33, 0.45, 0.20)",
        "(0.90, 0.10, 0.10)",
    );
    let outcome = bed.reload();
    assert!(
        matches!(outcome, ReloadOutcome::Values { .. }),
        "{outcome:?}"
    );
    let colors = &bed
        .app
        .world()
        .resource::<MapView>()
        .terrain
        .as_ref()
        .expect("terrain")
        .colors;
    assert_eq!(colors[grass_cell], [0.90, 0.10, 0.10]);
}

/// Swaps the testbed's runner for a fake one the test answers itself; waits for the runner
/// are `timeout`.
fn fake_runner(bed: &mut Testbed, timeout: Duration) -> fl_view::RunnerEnd {
    let (port, end) = runner_channel();
    let header = bed.app.world().resource::<RunnerLink>().header.clone();
    bed.world().insert_resource(RunnerLink { port, header });
    bed.world().resource_mut::<ContentSource>().reply_timeout = timeout;
    end
}

/// The id of the first control the fake runner received.
fn control_id(end: &fl_view::RunnerEnd) -> u64 {
    end.rx
        .try_iter()
        .find_map(|message| match message {
            ToRunner::Control { id, .. } => Some(id),
            _ => None,
        })
        .expect("a control was sent")
}

#[test]
fn replay_event_comes_from_the_runner_reply() {
    // Carried from Task 12's review: the event is the runner's answer, not a journal record
    // (which a match without a journal never writes).
    let mut bed = Testbed::new();
    let end = fake_runner(&mut bed, Duration::from_secs(10));
    let answer = std::thread::spawn(move || {
        for message in end.rx.iter() {
            if let ToRunner::Control { id, .. } = message {
                let reply = SimReply::Reloaded {
                    tick: Tick(4242),
                    rules_hash: 0xFEED,
                };
                end.tx.send(FromRunner::Reply { id, reply }).expect("send");
                return;
            }
        }
    });
    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    assert_eq!(
        bed.reload(),
        ReloadOutcome::Values {
            tick: Tick(4242),
            rules_hash: 0xFEED
        }
    );
    answer.join().expect("the fake runner answered");
    let capture = bed.app.world().resource::<ReplayCapture>();
    assert_eq!(
        capture.events(),
        [ReplayEvent::ContentReload {
            tick: Tick(4242),
            rules_hash: 0xFEED
        }]
    );
}

#[test]
fn reload_wait_is_bounded() {
    // Carried from Task 12's review: a runner that does not answer fails the reload instead of
    // hanging; the reply it sends a frame later is dropped.
    let mut bed = Testbed::new();
    let content = bed.content();
    let end = fake_runner(&mut bed, Duration::from_millis(100));
    let next = bed.next_tick();
    bed.edit(PZ3, PZ3_HP, "\"Hull\": (hp: 220,");
    let outcome = bed.reload();
    assert!(
        matches!(&outcome, ReloadOutcome::Failed(error) if error.contains("did not answer")),
        "{outcome:?}"
    );
    assert!(Arc::ptr_eq(&bed.content(), &content), "the content stays");

    // Review Important 3: the runner may still apply the content, so the replay records the
    // reload and never claims to reproduce.
    let edited = fl_sim::load_content(&fl_catalog::features(), &[bed.base()]).expect("loads");
    assert_eq!(
        bed.app.world().resource::<ReplayCapture>().events(),
        [ReplayEvent::ContentReload {
            tick: next,
            rules_hash: edited.rules_hash()
        }]
    );
    let path = bed.dir.path().join("timed_out.replay");
    bed.console(&format!("replay save {}", path.display()));
    let replay =
        Replay::from_bytes(&std::fs::read(&path).expect("replay written")).expect("decodes");
    assert!(!replay.reproducible());

    let id = control_id(&end);
    let reply = SimReply::Reloaded {
        tick: Tick(1),
        rules_hash: 1,
    };
    end.tx.send(FromRunner::Reply { id, reply }).expect("send");
    bed.app.update();
    assert_eq!(
        bed.world().resource_mut::<PendingReplies>().take(id),
        None,
        "the late reply was dropped"
    );
    assert!(Arc::ptr_eq(&bed.content(), &content));
}

#[test]
fn save_error_fails_the_rebuild() {
    // The brief: a SimReply::Error to the rebuild's save (spawn requests still waiting, say)
    // fails the reload, and the match keeps running.
    let mut bed = Testbed::new();
    let content = bed.content();
    let end = fake_runner(&mut bed, Duration::from_secs(10));
    let answer = std::thread::spawn(move || {
        for message in end.rx.iter() {
            if let ToRunner::Query {
                id,
                query: SimQuery::Save { .. },
            } = message
            {
                let reply = SimReply::Error("2 spawn requests are still waiting".to_owned());
                end.tx.send(FromRunner::Reply { id, reply }).expect("send");
                return;
            }
        }
    });
    bed.write("units/de/pz4.unit.ron", PZ4);
    let outcome = bed.reload();
    answer.join().expect("the fake runner answered");
    assert!(
        matches!(&outcome, ReloadOutcome::Failed(error) if error.contains("spawn requests")),
        "{outcome:?}"
    );
    assert!(Arc::ptr_eq(&bed.content(), &content), "the content stays");
    assert!(
        bed.app
            .world()
            .resource::<ReplayCapture>()
            .events()
            .is_empty()
    );
}

/// The unit type name of every unit view, under `content`, by sim id.
fn view_units(app: &mut App, content: &fl_content::ContentDb) -> Vec<(SimId, String)> {
    let world = app.world_mut();
    let mut views: Vec<(SimId, String)> = world
        .query::<&UnitView>()
        .iter(world)
        .map(|view| {
            let name = content
                .unit(view.unit)
                .map_or_else(|| "?".to_owned(), |u| u.id.clone());
            (view.sim_id, name)
        })
        .collect();
    views.sort();
    views
}

/// The unit type name of every record of the current snapshot, under `content`.
fn snapshot_units(app: &App, content: &fl_content::ContentDb) -> Vec<(SimId, String)> {
    let buffer = app.world().resource::<SnapshotBuffer>();
    let snapshot = buffer.curr.as_ref().expect("a snapshot");
    snapshot
        .entities
        .iter()
        .map(|r| {
            let name = content
                .unit(r.unit)
                .map_or_else(|| "?".to_owned(), |u| u.id.clone());
            (r.sim_id, name)
        })
        .collect()
}

#[test]
fn rebuild_never_shows_stale_unit_ids() {
    // Review Important 1: a unit def sorting before the others moves every dense unit id;
    // a paused rebuild gets no fresh snapshot, so the client must remap what it holds.
    let mut bed = Testbed::new();
    bed.run_until("every view", |app| {
        let world = app.world_mut();
        world.query::<&UnitView>().iter(world).count() == 10
    });
    bed.pause();
    let old = bed.content();
    let mut want = view_units(&mut bed.app, &old);
    assert_eq!(snapshot_units(&bed.app, &old), want);
    want.retain(|(id, _)| *id != SimId(3));

    bed.write(
        "units/de/a_scout.unit.ron",
        r#"Unit(
    id: "base.de.a_scout",
    inherits: Some("base.abstract.light_tank"),
    name: "Scout",
)
"#,
    );
    bed.remove("units/de/tiger.unit.ron");
    bed.remove("visuals/de_tiger.visual.ron");
    bed.edit("factions/de.faction.ron", "\"base.de.tiger\", ", "");
    let tiger_line =
        "        (unit: \"base.de.tiger\", player: 0, pos: (410.0, 350.0), heading_deg: 90.0),\n";
    bed.edit("scenarios/testbed.scenario.ron", tiger_line, "");
    bed.edit("scenarios/testbed_ai.scenario.ron", tiger_line, "");
    let ReloadOutcome::Rebuilt { dropped, .. } = bed.reload() else {
        panic!("new and removed unit types rebuild");
    };
    assert_eq!(dropped, vec![(SimId(3), "base.de.tiger".to_owned())]);
    let new = bed.content();
    assert_ne!(
        old.unit_id("base.de.pz3"),
        new.unit_id("base.de.pz3"),
        "the dense ids moved"
    );
    for _ in 0..3 {
        bed.app.update();
    }
    assert_eq!(
        view_units(&mut bed.app, &new),
        want,
        "every view shows its own unit"
    );
    assert_eq!(
        snapshot_units(&bed.app, &new),
        want,
        "and so does the snapshot"
    );
    assert!(
        !bed.app
            .world()
            .resource::<ViewIndex>()
            .0
            .contains_key(&SimId(3)),
        "the dropped unit's view is gone"
    );
}

#[test]
fn rebuild_keeps_the_known_map() {
    // Review Minor 4: a save names no map, but a rebuild continues a match whose map is known.
    let flat = r#"MapDef(id: "base.a_flat_1km", name: "Flat", size_m: (1000.0, 1000.0), cell_m: 4.0, ground_color: (0.9, 0.9, 0.9))
"#;
    let mut bed = Testbed::with(
        |builder| builder,
        |base| std::fs::write(base.join("maps/a_flat_1km.map.ron"), flat).expect("map"),
        |_| {},
    );
    let ground = bed.app.world().resource::<MapView>().ground_color;
    bed.write("units/de/pz4.unit.ron", PZ4);
    assert!(matches!(bed.reload(), ReloadOutcome::Rebuilt { .. }));
    let terrain = bed.app.world().resource::<MatchTerrain>();
    assert_eq!(terrain.map.as_deref(), Some("base.testbed_1km"));
    assert_eq!(bed.app.world().resource::<MapView>().ground_color, ground);
}

#[test]
fn stale_terrain_records_are_dropped_on_load() {
    // Review Minor 7: a terrain record of the old runner, still unread when a load swaps the
    // match, never reaches the new match's terrain.
    let mut bed = Testbed::new();
    let before = bed.app.world().resource::<MatchTerrain>().grid.clone();
    let record = fl_view::JournalEntry {
        tick: Tick(1),
        phase: 0,
        kind: "core.terrain_changed".to_owned(),
        sim_id: None,
        player: None,
        emitted: 0,
        data: serde_json::json!({"min": [0, 0], "max": [3, 3], "terrain": "base.road"}),
    };
    bed.world().write_message(SimJournalMsg(record));
    fl_client::debug::load_match(
        bed.world(),
        fl_client::debug::LoadRequest::Scenario("base.testbed".to_owned()),
    )
    .expect("the testbed loads");
    bed.app.update();
    assert_eq!(bed.app.world().resource::<MatchTerrain>().grid, before);
}

/// Rewrites `path` with its own bytes and moves its modification time.
fn touch(path: &Path) {
    let bytes = std::fs::read(path).expect("read");
    std::fs::write(path, bytes).expect("rewrite");
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open");
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000))
        .expect("set mtime");
}

#[test]
fn watcher_ignores_a_touch() {
    // Review Minor 3: a file rewritten with the same bytes is no change.
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("a.unit.ron");
    std::fs::write(&file, "x").expect("write");
    let mut watcher = ContentWatcher::new(vec![dir.path().to_owned()], Duration::from_millis(500));
    touch(&file);
    assert!(!watcher.scan(), "same bytes, new mtime: no change");
    std::fs::write(&file, "y").expect("rewrite");
    assert!(watcher.scan(), "same length, new bytes: a change");
}

#[test]
fn watcher_waits_for_the_files_to_settle() {
    // Review Minor 2: a change reloads only once a further scan finds the files unchanged.
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("a.unit.ron");
    std::fs::write(&file, "x").expect("write");
    let mut watcher = ContentWatcher::new(vec![dir.path().to_owned()], Duration::from_millis(500));
    assert!(!watcher.settled_change());
    std::fs::write(&file, "xy").expect("edit 1");
    assert!(!watcher.settled_change(), "still changing");
    std::fs::write(&file, "xyz").expect("edit 2");
    assert!(!watcher.settled_change(), "still changing");
    assert!(watcher.settled_change(), "stable for two scans");
    assert!(!watcher.settled_change(), "once");
}
