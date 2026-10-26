//! ADR 0013, game menu plan Task 2: a world saved at tick T and loaded into
//! a fresh app runs exactly like the world that was never saved (state
//! hashes equal at every checkpoint), for every embedded scenario and for
//! saves taken mid-walk, mid-craft, while waiting on a key, half-way through
//! a door interaction and while mounted, and at the two moments whose change
//! detection a load must restore (a door removed, a key gained after the
//! systems that react to them ran in the saved tick).

use std::path::PathBuf;

use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_ecs::system::ScheduleSystem;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::items::put_in_inventory;
use npc_framework::interaction::run::Interacting;
use npc_framework::interaction::{Door, Interactor};
use npc_framework::kernel::commands::{CommandLog, CommandQueue, submit};
use npc_framework::kernel::ensure_frozen;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::snapshot::{LoadMode, load, save};
use npc_framework::locomotion::MoveStatus;
use npc_framework::nav::{NavAgent, NavState};
use npc_framework::perception::{ExploredCells, RevealsFog, SeenStatics, VisibleCells};
use npc_framework::prelude::*;
use npc_framework::tools::snapshot_check::assert_resume_equivalent;
use npc_framework::vehicles::MountedIn;
use testbed_content::replay::{Replay, replay_app};
use testbed_content::{TestbedIds, new_sim_app};

#[test]
fn resume_equals_uninterrupted_testbed() {
    for at in [1, 37, 400, 1203, 2400] {
        assert_resume_equivalent(|| new_sim_app(1, "testbed"), at, 1200, 20);
    }
}

#[test]
fn resume_equals_uninterrupted_every_embedded_scenario() {
    for name in [
        "doors",
        "locked_store",
        "chop",
        "craft",
        "perception",
        "vehicles",
        "formation",
        "agent",
        "smithy",
        "explorer",
    ] {
        assert_resume_equivalent(|| new_sim_app(1, name), 150, 600, 10);
    }
}

fn corpus_replay(name: &str) -> Replay {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/replays")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// What a unit is doing at a tick, for [`resume_mid_action_states`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mid {
    Walking,
    Crafting,
    WaitingOnKey,
    /// Half-way through an interaction with a door (opening, unlocking).
    Interacting,
    Mounted,
}

fn states_now(world: &mut World) -> Vec<Mid> {
    let mut found = Vec::new();
    if world
        .query::<&MoveStatus>()
        .iter(world)
        .any(|s| *s == MoveStatus::Moving)
    {
        found.push(Mid::Walking);
    }
    if world.query::<&Crafting>().iter(world).next().is_some() {
        found.push(Mid::Crafting);
    }
    if world
        .query_filtered::<&NavState, (With<NavAgent>, With<Interactor>)>()
        .iter(world)
        .any(|s| matches!(s, NavState::Failed { .. }))
    {
        found.push(Mid::WaitingOnKey);
    }
    let targets: Vec<SimId> = world
        .query::<&Interacting>()
        .iter(world)
        .map(|i| i.target)
        .collect();
    let index = world.resource::<SimIndex>();
    if targets
        .iter()
        .filter_map(|&t| index.get(t))
        .any(|e| world.get::<Door>(e).is_some())
    {
        found.push(Mid::Interacting);
    }
    if world.query::<&MountedIn>().iter(world).next().is_some() {
        found.push(Mid::Mounted);
    }
    found
}

/// A testbed app with `orders` (`(tick, IssueOrder JSON)`) queued; `{unit}`
/// and `{target}` name testbed things.
fn with_orders(scenario: &str, orders: &[(u64, &str, serde_json::Value)]) -> App {
    let mut app = new_sim_app(1, scenario);
    let world = app.world_mut();
    ensure_frozen(world);
    for (tick, unit, order) in orders {
        let ids = world.resource::<TestbedIds>();
        let mut order = order.clone();
        if let Some(target) = order.get("target").and_then(|t| t.as_str()) {
            order["target"] = ids.id(target).0.into();
        }
        let cmd: SimCommand = serde_json::from_value(serde_json::json!({
            "type": "IssueOrder", "player": 1, "units": [ids.id(unit).0],
            "order": order, "queue": false
        }))
        .unwrap();
        submit(world, Some(*tick), cmd).unwrap();
    }
    app
}

/// The locked store: the keeper (no key) is ordered into the store (its
/// path fails at the locked door), then picks the key up next to it (an
/// `Interact` command, so the failed path is still there and is retried:
/// `key_replan_system`).
fn keeper_waits_on_key() -> App {
    let mut app = with_orders(
        "locked_store",
        &[(
            1,
            "keeper",
            serde_json::json!({"type": "Move", "pos": [6500, 6500]}),
        )],
    );
    let world = app.world_mut();
    let ids = world.resource::<TestbedIds>();
    let cmd: SimCommand = serde_json::from_value(serde_json::json!({
        "type": "Interact", "unit": ids.id("keeper").0, "verb": "pick_up",
        "target": ids.id("store_key").0
    }))
    .unwrap();
    npc_framework::kernel::commands::submit_deferred(world, Some(60), cmd).unwrap();
    app
}

/// A run to scan: its name, its app factory and how many ticks to scan.
type Source<'a> = (String, Box<dyn Fn() -> App + 'a>, u64);

#[test]
fn resume_mid_action_states() {
    let replays: Vec<Replay> = ["testbed_orders_craft", "testbed_vehicles"]
        .into_iter()
        .map(corpus_replay)
        .collect();
    let mut sources: Vec<Source> = replays
        .iter()
        .map(|r| {
            let make: Box<dyn Fn() -> App + '_> = Box::new(move || replay_app(r).unwrap());
            (r.scenario.clone(), make, r.ticks)
        })
        .collect();
    sources.push((
        "locked_store keeper".to_owned(),
        Box::new(keeper_waits_on_key),
        600,
    ));
    let mut todo = vec![
        Mid::Walking,
        Mid::Crafting,
        Mid::WaitingOnKey,
        Mid::Interacting,
        Mid::Mounted,
    ];
    for (name, make, ticks) in &sources {
        let mut scan = make();
        // Per state: the first tick it holds and the last tick of that first
        // stretch (the save just before it ends, e.g. before a key replan).
        let mut first: Vec<(Mid, u64, u64)> = Vec::new();
        let mut open: Vec<Mid> = Vec::new();
        for tick in 1..=*ticks {
            run_ticks(scan.world_mut(), 1);
            let now = states_now(scan.world_mut());
            for s in &now {
                if !todo.contains(s) {
                    continue;
                }
                match first.iter_mut().find(|(m, ..)| m == s) {
                    None => {
                        first.push((*s, tick, tick));
                        open.push(*s);
                    }
                    Some(row) if open.contains(s) => row.2 = tick,
                    Some(_) => {}
                }
            }
            open.retain(|s| now.contains(s));
            if open.is_empty() && todo.iter().all(|s| first.iter().any(|(m, ..)| m == s)) {
                break;
            }
        }
        for (state, from, to) in first {
            todo.retain(|s| *s != state);
            let mut ats = vec![from, to];
            ats.dedup();
            for at in ats {
                eprintln!("{name}: {state:?} at tick {at}");
                assert_resume_equivalent(make, at, 400, 5);
            }
        }
    }
    assert!(todo.is_empty(), "never found a unit {todo:?}");
}

/// The smithy with a move order for the smith queued at `tick`.
fn smithy_with_order(tick: Option<u64>) -> App {
    let mut app = new_sim_app(1, "smithy");
    if let Some(tick) = tick {
        let world = app.world_mut();
        ensure_frozen(world);
        let smith = world.resource::<TestbedIds>().id("smith");
        let cmd: SimCommand = serde_json::from_value(serde_json::json!({
            "type": "IssueOrder", "player": 1, "units": [smith.0],
            "order": {"type": "Move", "pos": [-2500, 7500]}, "queue": false
        }))
        .unwrap();
        submit(world, Some(tick), cmd).unwrap();
    }
    app
}

#[test]
fn resume_keeps_pending_commands() {
    let mut never = smithy_with_order(Some(500));
    run_ticks(never.world_mut(), 300);
    let mut saved = smithy_with_order(Some(500));
    run_ticks(saved.world_mut(), 300);
    let snap = save(saved.world_mut()).unwrap();
    // Built without the command: it can only come from the snapshot.
    let mut loaded = smithy_with_order(None);
    load(loaded.world_mut(), &snap, LoadMode::Strict).unwrap();
    let pending: Vec<(u64, u32)> = loaded
        .world()
        .resource::<CommandQueue>()
        .pending
        .keys()
        .copied()
        .collect();
    assert_eq!(pending, [(500, 0)]);
    let mut without = smithy_with_order(None);
    run_ticks(without.world_mut(), 300);
    for _ in 300..600 {
        for app in [&mut never, &mut saved, &mut loaded, &mut without] {
            run_ticks(app.world_mut(), 1);
        }
        let h = state_hash(never.world_mut());
        assert_eq!(state_hash(saved.world_mut()), h);
        assert_eq!(state_hash(loaded.world_mut()), h);
    }
    let applied: Vec<u64> = loaded
        .world()
        .resource::<CommandLog>()
        .entries
        .iter()
        .map(|e| e.0)
        .collect();
    assert_eq!(applied, [500]);
    assert_ne!(
        state_hash(without.world_mut()),
        state_hash(loaded.world_mut()),
        "the order changed the run"
    );
}

/// `system` runs in `NpcSet::Effects` of every tick (after the saved
/// tick's `door_sync_system` and `key_replan_system`).
fn with_late_system<M>(mut app: App, system: impl IntoScheduleConfigs<ScheduleSystem, M>) -> App {
    app.add_systems(NpcSimTick, system.in_set(NpcSet::Effects));
    app
}

/// The door scenario, the store door despawned in `NpcSet::Effects` of tick
/// 100 (after that tick's `door_sync_system`): the uninterrupted run clears
/// its cell in tick 101, so must the run loaded from a save at 101.
fn door_removed_late() -> App {
    let app = new_sim_app(1, "doors");
    let door = app.world().resource::<TestbedIds>().id("store_door");
    with_late_system(
        app,
        move |clock: Res<SimClock>, index: Res<SimIndex>, mut commands: Commands| {
            if clock.tick == 100
                && let Some(e) = index.get(door)
            {
                commands.entity(e).despawn();
            }
        },
    )
}

#[test]
fn resume_right_after_a_late_door_removal() {
    // The door's cell data is gone in the uninterrupted run (it is hashed:
    // the grid's static layer), so a missed clear would show.
    let mut app = door_removed_late();
    run_ticks(app.world_mut(), 102);
    let gone = app.world().resource::<TestbedIds>().id("store_door");
    assert!(app.world().resource::<SimIndex>().get(gone).is_none());
    for at in [100, 101, 102] {
        assert_resume_equivalent(door_removed_late, at, 200, 1);
    }
}

/// The locked store: the keeper is ordered into the store at tick 1 (its
/// path fails at the locked door), and at tick 40, in `NpcSet::Effects`
/// (after that tick's `key_replan_system`), the key is put straight into its
/// inventory. The uninterrupted run retries the path in tick 41.
fn key_gained_late() -> App {
    let app = with_orders(
        "locked_store",
        &[(
            1,
            "keeper",
            serde_json::json!({"type": "Move", "pos": [6500, 6500]}),
        )],
    );
    let ids = app.world().resource::<TestbedIds>();
    let (keeper, key) = (ids.id("keeper"), ids.id("store_key"));
    with_late_system(
        app,
        move |clock: Res<SimClock>, index: Res<SimIndex>, mut commands: Commands| {
            if clock.tick == 40
                && let (Some(k), Some(i)) = (index.get(keeper), index.get(key))
            {
                commands.queue(move |world: &mut World| put_in_inventory(world, k, i));
            }
        },
    )
}

#[test]
fn resume_right_after_a_late_key_pickup() {
    let mut app = key_gained_late();
    let keeper = app.world().resource::<TestbedIds>().id("keeper");
    let state = |app: &App| {
        let e = app.world().resource::<SimIndex>().get(keeper).unwrap();
        *app.world().get::<NavState>(e).unwrap()
    };
    run_ticks(app.world_mut(), 41);
    assert!(
        matches!(state(&app), NavState::Failed { .. }),
        "{:?}",
        state(&app)
    );
    run_ticks(app.world_mut(), 1);
    assert!(
        !matches!(state(&app), NavState::Failed { .. }),
        "the key replan retried the path in tick 41: {:?}",
        state(&app)
    );
    for at in [40, 41, 42] {
        assert_resume_equivalent(key_gained_late, at, 200, 1);
    }
}

/// Tiny deterministic generator for the order tests (no `rand`).
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

/// The testbed with random `IssueOrder` Move and `Interact` commands queued
/// at random ticks (all from `seed`).
fn random_orders(seed: u64) -> App {
    let mut app = new_sim_app(seed, "testbed");
    let world = app.world_mut();
    ensure_frozen(world);
    let units = ["carpenter", "smith", "woodcutter", "scout"];
    let targets = [
        ("store_key", "pick_up"),
        ("store_door", "open"),
        ("gate", "open"),
        ("workshop_door", "open"),
    ];
    let mut rng = seed;
    let mut accepted = 0;
    for _ in 0..24 {
        let tick = 1 + next(&mut rng) % 500;
        let unit = units[(next(&mut rng) % units.len() as u64) as usize];
        let ids = world.resource::<TestbedIds>();
        let cmd = if next(&mut rng).is_multiple_of(3) {
            let (target, verb) = targets[(next(&mut rng) % targets.len() as u64) as usize];
            serde_json::json!({
                "type": "Interact", "unit": ids.id(unit).0, "verb": verb,
                "target": ids.id(target).0
            })
        } else {
            let x = (next(&mut rng) % 16_001) as i64 - 8_000;
            let y = (next(&mut rng) % 16_001) as i64 - 8_000;
            serde_json::json!({
                "type": "IssueOrder", "player": 1, "units": [ids.id(unit).0],
                "order": {"type": "Move", "pos": [x, y]}, "queue": next(&mut rng).is_multiple_of(2)
            })
        };
        let cmd: SimCommand = serde_json::from_value(cmd).unwrap();
        if submit(world, Some(tick), cmd).is_ok() {
            accepted += 1;
        }
    }
    assert!(
        accepted >= 8,
        "seed {seed}: only {accepted} commands accepted"
    );
    app
}

#[test]
fn random_orders_resume_equivalent() {
    for seed in 1..=8u64 {
        let mut rng = seed ^ 0x5eed;
        let at = 1 + next(&mut rng) % 600;
        assert_resume_equivalent(|| random_orders(seed), at, 800, 20);
    }
}

/// The locked store with a tiny nav budget: the keeper's long path takes
/// several ticks, so the A* search is paused across tick boundaries.
fn slow_search() -> App {
    let mut app = with_orders(
        "locked_store",
        &[(
            1,
            "keeper",
            serde_json::json!({"type": "Move", "pos": [6500, 6500]}),
        )],
    );
    app.insert_resource(npc_framework::nav::NavBudget(3));
    app
}

#[test]
fn paused_astar_search_resume_equivalent() {
    let mut scan = slow_search();
    let mut found = None;
    for tick in 1..=300 {
        run_ticks(scan.world_mut(), 1);
        if scan
            .world()
            .resource::<npc_framework::nav::NavQueue>()
            .paused_nodes()
            > 0
        {
            found = Some(tick);
            break;
        }
    }
    let at = found.expect("no tick with a paused A* search");
    assert_resume_equivalent(slow_search, at, 400, 1);
}

/// Plan Task 9 (fog of war): the fog record is snapshot-only and only the
/// explorer host's actor reveals fog, so no shared scenario changes its
/// content or state hash. Constants recorded from the code before fog
/// existed (fe7a91e), tick 600.
#[test]
fn fog_does_not_change_existing_hashes() {
    let pinned: [(&str, u64, u64); 4] = [
        ("testbed", 0xd8a1bc52b1247a36, 0xb9f394ce322c6b4e),
        ("perception", 0xd8a1bc52b1247a36, 0xe347bceddfe4796e),
        ("smithy", 0x7d7a7460c2816513, 0xd826f1d690755af3),
        ("explorer", 0xd8a1bc52b1247a36, 0x197a2048050deccb),
    ];
    let mut got = Vec::new();
    for (name, _, _) in pinned {
        let mut app = new_sim_app(1, name);
        let content = npc_framework::kernel::registry::content_hash(app.world_mut());
        run_ticks(app.world_mut(), 600);
        got.push((name, content, state_hash(app.world_mut())));
    }
    assert_eq!(got, pinned.to_vec());
}

/// The `explorer` scenario's world and actor, the actor revealing fog (as
/// the explorer host makes it) and wandering up to 8 m from its start (no
/// agent controller), so it walks and explores on its own.
fn explorer_with_fog() -> App {
    let mut doc = testbed_content::scenario::scenario_doc("explorer").unwrap();
    doc.controllers.clear();
    let actor = &mut doc.actors[0];
    actor.extra.insert("behaviour".into(), "wander".into());
    actor.extra.insert("wander_radius_mm".into(), 8000.into());
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    let world = app.world_mut();
    let id = world.resource::<TestbedIds>().id("explorer");
    let e = world.resource::<SimIndex>().get(id).unwrap();
    world.entity_mut(e).insert(RevealsFog);
    app
}

type FogRecord = (ExploredCells, VisibleCells, SeenStatics);

fn fog_record(app: &App) -> FogRecord {
    let world = app.world();
    let id = world.resource::<TestbedIds>().id("explorer");
    let e = world.resource::<SimIndex>().get(id).unwrap();
    (
        world.get::<ExploredCells>(e).unwrap().clone(),
        world.get::<VisibleCells>(e).unwrap().clone(),
        world.get::<SeenStatics>(e).unwrap().clone(),
    )
}

/// Plan Task 9 (spec G8): the explored ground survives a save and load
/// mid-walk, and resuming stays equivalent (fog is not hashed, so the fog
/// record itself is compared too).
#[test]
fn explored_survives_resume() {
    assert_resume_equivalent(explorer_with_fog, 150, 300, 20);

    let mut never = explorer_with_fog();
    run_ticks(never.world_mut(), 150);
    let mut saved = explorer_with_fog();
    run_ticks(saved.world_mut(), 150);
    let snap = save(saved.world_mut()).unwrap();
    let text = serde_json::to_string(&snap).unwrap();
    let snap = serde_json::from_str(&text).unwrap();
    let mut loaded = explorer_with_fog();
    run_ticks(loaded.world_mut(), 0);
    load(loaded.world_mut(), &snap, LoadMode::Strict).unwrap();
    let at_save = fog_record(&never);
    assert!(at_save.0.count() > 0 && !at_save.2.0.is_empty());
    assert_eq!(fog_record(&loaded), at_save);

    run_ticks(never.world_mut(), 300);
    run_ticks(loaded.world_mut(), 300);
    let later = fog_record(&never);
    assert!(
        later.0.count() > at_save.0.count(),
        "walking explores more ({} -> {})",
        at_save.0.count(),
        later.0.count()
    );
    assert_eq!(fog_record(&loaded), later);
}
