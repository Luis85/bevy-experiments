//! Wave E1: `npc_interaction` (items, inventory, equipment, doors, tools,
//! verbs) on toy content: no rules, perception or agent control here (that
//! glue lives in the facade; see `npc_framework/tests/interaction.rs`).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_ai::{AiRegistry, BbKey, BbValue, Blackboard, Brain, ParamValue, Status, StopReason};
use npc_interaction::{
    CarriedBy, Door, Equipment, Interact, InteractionAppExt, InteractionPlugin, Interactor,
    Inventory, Item, ItemKindDef, ItemYield, MAIN_HAND, SkillCheck, Source, ToolTarget,
};
use npc_jobs::NpcJobsPlugin;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{DroppedItem, JournalEntry, JournalKind};
use npc_kernel::registry::{ContentHash, KernelAppExt};
use npc_kernel::spatial::{
    CellCoord, CellFlags, CellInfo, GridBounds, PathFailReason, SpatialGrid, SpatialPlugin,
};
use npc_kernel::{
    CommandError, Journal, JournalPlugin, NpcKernelPlugin, SimCommand, SimId, SimPos, run_ticks,
    spawn_sim, submit,
};
use npc_locomotion::{MoveIntent, MoveStatus, StraightLineMover, set_move_intent};
use npc_nav::{NavAgent, NavPlugin, NavState, Path};

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

fn hold_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn hold_stop(_: &mut World, _: Entity, _: StopReason) {}

fn base_app(configure: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        NpcJobsPlugin,
        StraightLineMover,
        NavPlugin,
        InteractionPlugin::default(),
        JournalPlugin::default(),
    ))
    .register_item_kind(
        ItemKindDef::new("axe")
            .weight_g(1500)
            .tag("tool:axe")
            .equip_slot(MAIN_HAND),
    )
    .register_item_kind(ItemKindDef::new("wood").weight_g(500).stackable(10))
    .register_item_kind(ItemKindDef::new("gate key").weight_g(50).tag("key:gate"))
    .register_item_kind(ItemKindDef::new("rock").weight_g(9000))
    .register_action("Hold", hold_start, hold_stop)
    .register_tree("grab", |b| {
        b.sequence("Root", |b| {
            b.leaf("Grab", "Interact", "pick_up");
            b.leaf("Hold", "Hold", ());
        });
    })
    .register_tree("hold", |b| {
        b.leaf("Hold", "Hold", ());
    });
    configure(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    base_app(|_| {})
}

fn actor(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Interactor,
            Inventory::new(3, 10_000),
            Equipment::hands(),
        ),
    )
}

fn item(app: &mut App, pos: SimPos, item: Item) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, item))
}

fn door(app: &mut App, cell: (i32, i32), door: Door) -> (SimId, Entity) {
    let pos = SimPos::new(cell.0 * 1000 + 500, cell.1 * 1000 + 500);
    spawn_sim(app.world_mut(), (pos, door))
}

fn interact(app: &mut App, unit: SimId, verb: &str, target: SimId) -> Result<(), CommandError> {
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Interact::new(unit, verb, target)),
    )
    .map(|_| ())
}

fn interact_with(
    app: &mut App,
    unit: SimId,
    verb: &str,
    target: SimId,
    with: SimId,
) -> Result<(), CommandError> {
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Interact::new(unit, verb, target).with_item(with)),
    )
    .map(|_| ())
}

fn run(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

/// Interaction entries as `(tick, unit, kind)`.
fn interaction_entries(app: &App) -> Vec<(u64, SimId, JournalKind)> {
    journal(app)
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                JournalKind::Interacted { .. }
                    | JournalKind::ItemMoved { .. }
                    | JournalKind::DoorChanged { .. }
                    | JournalKind::Reserve { .. }
                    | JournalKind::Release { .. }
            )
        })
        .map(|e| (e.tick, e.unit.unwrap(), e.kind))
        .collect()
}

/// `(verb, outcome)` of every `Interacted` entry of `unit`.
fn outcomes(app: &App, unit: SimId) -> Vec<(String, String)> {
    journal(app)
        .into_iter()
        .filter(|e| e.unit == Some(unit))
        .filter_map(|e| match e.kind {
            JournalKind::Interacted { verb, outcome, .. } => Some((verb, outcome)),
            _ => None,
        })
        .collect()
}

fn reason_of(err: CommandError) -> String {
    match err {
        CommandError::Unavailable { reason } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

fn inventory(app: &App, e: Entity) -> Vec<SimId> {
    app.world().get::<Inventory>(e).unwrap().items.clone()
}

fn grid(app: &App) -> &SpatialGrid {
    app.world().resource::<SpatialGrid>()
}

/// Line of sight as `npc_perception::line_of_sight` computes it.
fn los(grid: &SpatialGrid, a: SimPos, b: SimPos) -> bool {
    let (s, e) = (grid.cell_of(a), grid.cell_of(b));
    grid.trace_segment(a, b, |c| c == s || c == e || !grid.is_occluding(c))
}

/// A wall on column `x = 0` for `z` in `zs`, except where a door stands.
fn wall(app: &mut App, zs: std::ops::RangeInclusive<i32>, except: i32) {
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for z in zs {
        if z != except {
            assert!(grid.set_static(CellCoord::new(0, z), WALL));
        }
    }
}

// ---------------------------------------------------------------- items

#[test]
fn pick_up_drop_give_respect_slots_and_weight() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (a, ae) = actor(&mut app, here);
    let (b, be) = actor(&mut app, SimPos::new(1500, 500));
    let (axe, axe_e) = item(&mut app, here, Item::new("axe"));
    let (rock, _) = item(&mut app, here, Item::new("rock"));
    let (wood1, _) = item(&mut app, here, Item::new("wood").stack(4));
    let (wood2, _) = item(&mut app, here, Item::new("wood").stack(3));
    let (far, _) = item(&mut app, SimPos::new(9500, 500), Item::new("axe"));
    run(&mut app, 1);

    // Reach.
    let err = reason_of(interact(&mut app, a, "pick_up", far).unwrap_err());
    assert!(err.starts_with("too far: 9.0 m"), "{err}");

    // Pick up the axe: takes 10 ticks, then it is carried, not in the world.
    interact(&mut app, a, "pick_up", axe).unwrap();
    run(&mut app, 10);
    assert!(app.world().get::<SimPos>(axe_e).is_some(), "not yet");
    run(&mut app, 1);
    assert_eq!(inventory(&app, ae), vec![axe]);
    assert!(app.world().get::<SimPos>(axe_e).is_none());
    assert_eq!(
        app.world().get::<CarriedBy>(axe_e).unwrap().holder,
        a,
        "carried by a"
    );
    run(&mut app, 1);
    assert!(
        grid(&app).position_of(axe).is_none(),
        "left the grid index (updated in the next tick's inputs)"
    );

    // Weight: 1.5 kg + 9 kg > 10 kg.
    let err = reason_of(interact(&mut app, a, "pick_up", rock).unwrap_err());
    assert!(err.starts_with("too heavy"), "{err}");

    // Stacks merge into one slot.
    interact(&mut app, a, "pick_up", wood1).unwrap();
    run(&mut app, 11);
    interact(&mut app, a, "pick_up", wood2).unwrap();
    run(&mut app, 11);
    assert_eq!(inventory(&app, ae), vec![axe, wood1], "wood2 merged");
    let wood1_e = npc_interaction::items::entity_of(app.world(), wood1).unwrap();
    assert_eq!(app.world().get::<Item>(wood1_e).unwrap().stack, 7);
    assert!(
        npc_interaction::items::entity_of(app.world(), wood2).is_none(),
        "merged stack despawned"
    );

    // Slots: b has 3 slots; fill them and give fails.
    for i in 0..3 {
        let (x, _) = item(
            &mut app,
            SimPos::new(1500, 500),
            Item::new("wood").quality(i),
        );
        run(&mut app, 1);
        interact(&mut app, b, "pick_up", x).unwrap();
        run(&mut app, 11);
    }
    assert_eq!(inventory(&app, be).len(), 3);
    let err = reason_of(interact_with(&mut app, a, "give", b, axe).unwrap_err());
    assert!(err.contains("inventory is full"), "{err}");
    // give without an item is refused too
    let err = reason_of(interact(&mut app, a, "give", b).unwrap_err());
    assert!(err.contains("needs an item"), "{err}");

    // b drops one; then the give works.
    let drop_me = inventory(&app, be)[0];
    interact(&mut app, b, "drop", drop_me).unwrap();
    run(&mut app, 6);
    let dropped = npc_interaction::items::entity_of(app.world(), drop_me).unwrap();
    assert_eq!(
        app.world().get::<SimPos>(dropped).copied(),
        Some(SimPos::new(1500, 500)),
        "dropped at b's feet"
    );
    interact_with(&mut app, a, "give", b, axe).unwrap();
    run(&mut app, 11);
    assert!(inventory(&app, be).contains(&axe));
    assert!(!inventory(&app, ae).contains(&axe));
    let moved: Vec<(Option<SimId>, Option<SimId>)> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::ItemMoved { item, from, to, .. } if item == axe => Some((from, to)),
            _ => None,
        })
        .collect();
    assert_eq!(moved, vec![(None, Some(a)), (Some(a), Some(b))]);
}

#[test]
fn equip_tool_then_use_tool_on() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (a, ae) = actor(&mut app, here);
    let (axe, axe_e) = item(&mut app, here, Item::new("axe"));
    let (tree, tree_e) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(1500, 500),
            ToolTarget::new("tool:axe")
                .yields(ItemYield::new("wood", 2))
                .duration(40)
                .uses(1),
        ),
    );
    run(&mut app, 1);

    let err = reason_of(interact(&mut app, a, "use_tool_on", tree).unwrap_err());
    assert_eq!(err, "needs tool:axe");
    interact(&mut app, a, "pick_up", axe).unwrap();
    run(&mut app, 11);
    let err = reason_of(interact(&mut app, a, "use_tool_on", tree).unwrap_err());
    assert_eq!(err, "equip tool:axe first");

    interact(&mut app, a, "equip", axe).unwrap();
    run(&mut app, 11);
    let eq = app.world().get::<Equipment>(ae).unwrap();
    assert_eq!(eq.slots[MAIN_HAND], Some(axe));
    assert!(
        inventory(&app, ae).is_empty(),
        "equipped items leave the bag"
    );
    assert_eq!(
        app.world().get::<CarriedBy>(axe_e).unwrap().slot.as_deref(),
        Some(MAIN_HAND)
    );

    interact(&mut app, a, "use_tool_on", tree).unwrap();
    run(&mut app, 40);
    assert_eq!(
        app.world().get::<ToolTarget>(tree_e).unwrap().remaining,
        Some(1),
        "40 ticks of work: not done before the last tick"
    );
    run(&mut app, 1);
    assert_eq!(
        app.world().get::<ToolTarget>(tree_e).unwrap().remaining,
        Some(0)
    );
    let wood: Vec<Item> = app
        .world_mut()
        .query::<(&Item, &SimPos)>()
        .iter(app.world())
        .filter(|(i, _)| i.kind == "wood")
        .map(|(i, _)| i.clone())
        .collect();
    assert_eq!(wood, vec![Item::new("wood").stack(2)], "yield at a's feet");
    let err = reason_of(interact(&mut app, a, "use_tool_on", tree).unwrap_err());
    assert_eq!(err, "is used up");

    interact(&mut app, a, "unequip", axe).unwrap();
    run(&mut app, 11);
    assert_eq!(inventory(&app, ae), vec![axe]);
    assert_eq!(
        outcomes(&app, a)
            .iter()
            .filter(|(_, o)| o == "success")
            .map(|(v, _)| v.as_str())
            .collect::<Vec<_>>(),
        vec!["pick_up", "equip", "use_tool_on", "unequip"]
    );
}

// ---------------------------------------------------------------- doors

#[test]
fn open_and_close_toggle_the_grid_line_of_sight_and_nav() {
    let mut app = app();
    wall(&mut app, -5..=5, 0);
    let (d, de) = door(&mut app, (0, 0), Door::closed());
    let (a, _) = actor(&mut app, SimPos::new(-500, 500));
    run(&mut app, 1);
    let cell = CellCoord::new(0, 0);
    let (west, east) = (SimPos::new(-2500, 500), SimPos::new(2500, 500));
    assert!(grid(&app).is_blocking(cell) && grid(&app).is_occluding(cell));
    assert!(!los(grid(&app), west, east), "a closed door occludes");

    interact(&mut app, a, "open", d).unwrap();
    run(&mut app, 11);
    assert!(app.world().get::<Door>(de).unwrap().open);
    assert!(!grid(&app).is_blocking(cell) && !grid(&app).is_occluding(cell));
    assert!(los(grid(&app), west, east), "an open door does not");
    assert_eq!(
        grid(&app).cell(cell).flags.tags & 1,
        1,
        "the door tag bit stays"
    );

    let err = reason_of(interact(&mut app, a, "open", d).unwrap_err());
    assert_eq!(err, "door is already open");
    interact(&mut app, a, "close", d).unwrap();
    run(&mut app, 11);
    assert!(grid(&app).is_blocking(cell) && !los(grid(&app), west, east));
    let doors: Vec<bool> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::DoorChanged { door, open, .. } if door == d => Some(open),
            _ => None,
        })
        .collect();
    assert_eq!(doors, vec![true, false]);

    // Removing the door clears its cell.
    app.world_mut().despawn(de);
    run(&mut app, 1);
    assert_eq!(grid(&app).cell(cell), CellInfo::default());
}

fn path_of(app: &App, e: Entity) -> Path {
    app.world().get::<Path>(e).cloned().unwrap_or_default()
}

#[test]
fn closed_doors_cost_extra_with_a_marker_for_interactors_only() {
    let mut app = app();
    wall(&mut app, -5..=5, 0);
    door(&mut app, (0, 0), Door::closed());
    run(&mut app, 1);
    let from = SimPos::new(-2500, 500);
    let to = SimPos::new(2500, 500);
    let (_, opener) = spawn_sim(app.world_mut(), (from, NavAgent, Interactor));
    let (_, other) = spawn_sim(app.world_mut(), (from, NavAgent));
    for e in [opener, other] {
        set_move_intent(app.world_mut(), e, MoveIntent::to(to, 100));
    }
    run(&mut app, 1);
    let p = path_of(&app, opener);
    assert_eq!(p.markers.len(), 1, "through the door: {p:?}");
    assert_eq!(p.markers[0].cell, CellCoord::new(0, 0));
    let q = path_of(&app, other);
    assert!(q.markers.is_empty(), "around the wall: {q:?}");
    assert!(q.waypoints.len() > p.waypoints.len());
}

fn walk_through(app: &mut App, agent: Entity, goal: SimPos, max: u64) -> u64 {
    for t in 1..=max {
        run(app, 1);
        let pos = *app.world().get::<SimPos>(agent).unwrap();
        let g = grid(app);
        assert!(!g.is_blocking(g.cell_of(pos)), "inside a blocking cell");
        if pos == goal {
            return t;
        }
    }
    panic!(
        "did not arrive (at {:?}, {:?}, {:?})",
        app.world().get::<SimPos>(agent),
        app.world().get::<MoveStatus>(agent),
        app.world().get::<NavState>(agent),
    );
}

#[test]
fn nav_agent_opens_a_closed_door_on_its_path_and_walks_through() {
    let mut app = app();
    wall(&mut app, -16..=15, 0);
    let (d, de) = door(&mut app, (0, 0), Door::closed());
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (SimPos::new(-3500, 500), NavAgent, Interactor),
    );
    let goal = SimPos::new(3500, 500);
    run(&mut app, 1);
    set_move_intent(app.world_mut(), ae, MoveIntent::to(goal, 100));
    walk_through(&mut app, ae, goal, 400);
    assert!(app.world().get::<Door>(de).unwrap().open);
    assert_eq!(
        outcomes(&app, a),
        vec![
            ("open".to_owned(), "started".to_owned()),
            ("open".to_owned(), "success".to_owned())
        ]
    );
    assert!(journal(&app).iter().any(|e| matches!(
        e.kind,
        JournalKind::DoorChanged { door, open: true, .. } if door == d
    )));
}

#[test]
fn locked_door_blocks_until_the_key_is_carried() {
    let mut app = app();
    wall(&mut app, -16..=15, 0);
    let (d, de) = door(&mut app, (0, 0), Door::locked("key:gate"));
    let here = SimPos::new(-500, 500);
    let (a, ae) = actor(&mut app, here);
    let (key, _) = item(&mut app, SimPos::new(-1500, 500), Item::new("gate key"));
    run(&mut app, 1);

    assert_eq!(
        reason_of(interact(&mut app, a, "open", d).unwrap_err()),
        "door is locked"
    );
    assert_eq!(
        reason_of(interact(&mut app, a, "unlock", d).unwrap_err()),
        "door is locked: needs key:gate"
    );
    // A nav agent without the key finds no way.
    let (_, walker) = spawn_sim(
        app.world_mut(),
        (SimPos::new(-3500, 500), NavAgent, Interactor),
    );
    set_move_intent(
        app.world_mut(),
        walker,
        MoveIntent::to(SimPos::new(3500, 500), 100),
    );
    run(&mut app, 2);
    assert_eq!(
        app.world().get::<NavState>(walker).copied(),
        Some(NavState::Failed {
            goal: SimPos::new(3500, 500),
            reason: PathFailReason::Unreachable
        })
    );

    interact(&mut app, a, "pick_up", key).unwrap();
    run(&mut app, 11);
    interact(&mut app, a, "unlock", d).unwrap();
    run(&mut app, 21);
    assert!(!app.world().get::<Door>(de).unwrap().is_locked());
    assert!(
        !app.world().get::<Door>(de).unwrap().open,
        "unlocked, still closed"
    );
    interact(&mut app, a, "open", d).unwrap();
    run(&mut app, 11);
    assert!(app.world().get::<Door>(de).unwrap().open);
    // Lock it again: close, then lock.
    interact(&mut app, a, "close", d).unwrap();
    run(&mut app, 11);
    interact(&mut app, a, "lock", d).unwrap();
    run(&mut app, 21);
    assert!(app.world().get::<Door>(de).unwrap().is_locked());
    let _ = ae;
}

#[test]
fn nav_agent_with_the_key_unlocks_opens_and_walks_through() {
    let mut app = app();
    wall(&mut app, -16..=15, 0);
    let (_, de) = door(&mut app, (0, 0), Door::locked("key:gate"));
    let start = SimPos::new(-3500, 500);
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (start, NavAgent, Interactor, Inventory::new(2, 1000)),
    );
    let (key, _) = item(&mut app, start, Item::new("gate key"));
    run(&mut app, 1);
    interact(&mut app, a, "pick_up", key).unwrap();
    run(&mut app, 11);
    let goal = SimPos::new(3500, 500);
    set_move_intent(app.world_mut(), ae, MoveIntent::to(goal, 100));
    walk_through(&mut app, ae, goal, 500);
    let d = app.world().get::<Door>(de).unwrap();
    assert!(d.open && !d.is_locked());
    let done: Vec<String> = outcomes(&app, a)
        .into_iter()
        .filter(|(_, o)| o == "success")
        .map(|(v, _)| v)
        .collect();
    assert_eq!(done, vec!["pick_up", "unlock", "open"]);
}

/// Wave H2: a path that failed at a locked door is retried when the unit
/// later picks up the key (no `GridChanged` happens: the door blocks only
/// agents without the key), and only then.
#[test]
fn failed_path_replans_when_the_key_is_picked_up() {
    let mut app = app();
    wall(&mut app, -16..=15, 0);
    let (_, de) = door(&mut app, (0, 0), Door::locked("key:gate"));
    let start = SimPos::new(-3500, 500);
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (start, NavAgent, Interactor, Inventory::new(3, 10_000)),
    );
    let (key, _) = item(&mut app, start, Item::new("gate key"));
    let (wood, _) = item(&mut app, start, Item::new("wood"));
    run(&mut app, 1);
    let goal = SimPos::new(3500, 500);
    set_move_intent(app.world_mut(), ae, MoveIntent::to(goal, 100));
    run(&mut app, 2);
    let failed = Some(NavState::Failed {
        goal,
        reason: PathFailReason::Unreachable,
    });
    assert_eq!(app.world().get::<NavState>(ae).copied(), failed);
    let path_failed = |app: &App| {
        journal(app)
            .iter()
            .filter(|e| e.unit == Some(a) && matches!(e.kind, JournalKind::PathFailed { .. }))
            .count()
    };
    assert_eq!(path_failed(&app), 1);

    // Picking up something that is not a key changes nothing.
    interact(&mut app, a, "pick_up", wood).unwrap();
    run(&mut app, 12);
    assert_eq!(inventory(&app, ae), [wood]);
    assert_eq!(app.world().get::<NavState>(ae).copied(), failed);
    assert_eq!(path_failed(&app), 1, "no retry without the key");

    // The key: the path is retried at once and the agent walks through.
    interact(&mut app, a, "pick_up", key).unwrap();
    run(&mut app, 12);
    assert!(inventory(&app, ae).contains(&key));
    assert!(
        matches!(
            app.world().get::<NavState>(ae),
            Some(NavState::Following { .. } | NavState::Pending { .. })
        ),
        "{:?}",
        app.world().get::<NavState>(ae)
    );
    assert_eq!(path_failed(&app), 1);
    walk_through(&mut app, ae, goal, 500);
    let d = app.world().get::<Door>(de).unwrap();
    assert!(d.open && !d.is_locked());
}

// ---------------------------------------------------------------- BT, commands, reservations

fn brain(app: &App, tree: &str) -> Brain {
    Brain {
        tree: app.world().resource::<AiRegistry>().tree_id(tree).unwrap(),
    }
}

fn set_slot(app: &mut App, e: Entity, slot: u8, id: SimId) {
    app.world_mut()
        .get_mut::<Blackboard>(e)
        .unwrap()
        .set(BbKey(slot), BbValue::U32(id.0));
}

#[test]
fn two_agents_contend_for_one_item_and_the_lower_sim_id_wins() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let b1 = brain(&app, "grab");
    let (axe, axe_e) = item(&mut app, here, Item::new("axe"));
    let (first, first_e) = spawn_sim(
        app.world_mut(),
        (here, Interactor, Inventory::new(3, 10_000)),
    );
    let (second, second_e) = spawn_sim(
        app.world_mut(),
        (here, Interactor, Inventory::new(3, 10_000)),
    );
    // Brains inserted in the opposite order: thinking goes by SimId anyway.
    for e in [second_e, first_e] {
        app.world_mut().entity_mut(e).insert(b1);
        set_slot(&mut app, e, 7, axe);
    }
    run(&mut app, 12);
    assert!(first < second);
    assert_eq!(
        app.world().get::<CarriedBy>(axe_e).unwrap().holder,
        first,
        "the lower SimId reserved first"
    );
    let lost = outcomes(&app, second);
    assert!(!lost.is_empty());
    assert!(
        lost.iter().all(|(v, o)| v == "pick_up" && o == "rejected"),
        "the other one is refused: {lost:?}"
    );
    let reserves: Vec<(SimId, bool)> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::Reserve { target, ok } if target == axe => Some((e.unit.unwrap(), ok)),
            _ => None,
        })
        .collect();
    assert_eq!(reserves, vec![(first, true)], "the loser never claims it");
}

/// Runs the same pick-up through the BT leaf or the command; returns the
/// interaction journal (ticks relative to the first entry) and the final hash.
fn pick_up_via(via_tree: bool) -> (Vec<(u64, SimId, JournalKind)>, u64) {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (axe, _) = item(&mut app, here, Item::new("axe"));
    let tree = brain(&app, if via_tree { "grab" } else { "hold" });
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (here, Interactor, Inventory::new(3, 10_000), tree),
    );
    set_slot(&mut app, ae, 7, axe);
    if !via_tree {
        interact(&mut app, a, "pick_up", axe).unwrap();
    }
    run(&mut app, 20);
    let entries = interaction_entries(&app);
    let t0 = entries.first().map_or(0, |e| e.0);
    let entries = entries
        .into_iter()
        .map(|(t, u, k)| (t - t0, u, k))
        .collect();
    let hash = {
        // Strip what differs by construction: the tree's leaf state.
        let w = app.world_mut();
        w.entity_mut(ae)
            .remove::<(npc_ai::ActiveLeaf, npc_ai::BrainState, Brain)>();
        w.entity_mut(ae).remove::<npc_interaction::InteractLeaf>();
        state_hash(w)
    };
    (entries, hash)
}

#[test]
fn bt_leaf_and_command_take_the_same_path() {
    let (tree, h1) = pick_up_via(true);
    let (cmd, h2) = pick_up_via(false);
    assert!(!tree.is_empty());
    assert_eq!(tree, cmd);
    assert_eq!(h1, h2);
}

#[test]
fn skill_check_hook_failure_consumes_the_time_but_has_no_effect() {
    fn always_fail(_: &mut World, _: Entity, c: &SkillCheck, purpose: &str) -> Option<bool> {
        assert_eq!(c.skill, "Lockpicking");
        assert_eq!(purpose, "interaction.unlock");
        Some(false)
    }
    let mut app = base_app(|app| {
        app.set_skill_check_hook("test.fail", always_fail);
        app.override_interaction(
            npc_interaction::builtin::builtin_verbs()
                .into_iter()
                .find(|d| d.name == "unlock")
                .unwrap()
                .check("Lockpicking", 0),
        );
    });
    assert!(
        app.world()
            .resource::<ContentHash>()
            .manifest
            .contains(&("interaction_hook".into(), "skill_check=test.fail".into()))
    );
    let here = SimPos::new(-500, 500);
    let (d, de) = door(&mut app, (0, 0), Door::locked("key:gate"));
    let (a, _) = actor(&mut app, here);
    let (key, _) = item(&mut app, here, Item::new("gate key"));
    run(&mut app, 1);
    interact(&mut app, a, "pick_up", key).unwrap();
    run(&mut app, 11);
    interact(&mut app, a, "unlock", d).unwrap();
    let start = app.world().resource::<npc_kernel::SimClock>().tick;
    run(&mut app, 21);
    assert!(
        app.world().get::<Door>(de).unwrap().is_locked(),
        "no effect"
    );
    let failed: Vec<(u64, Option<String>)> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::Interacted {
                verb,
                outcome,
                reason,
                ..
            } if verb == "unlock" && outcome == "failed" => Some((e.tick, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        failed,
        vec![(start + 20, Some("Lockpicking check failed".to_owned()))],
        "after the full 20 ticks"
    );
}

#[test]
fn without_a_hook_checks_pass() {
    let mut app = base_app(|app| {
        app.override_interaction(
            npc_interaction::builtin::builtin_verbs()
                .into_iter()
                .find(|d| d.name == "unlock")
                .unwrap()
                .check("Lockpicking", -10),
        );
    });
    let here = SimPos::new(-500, 500);
    let (d, de) = door(&mut app, (0, 0), Door::locked("key:gate"));
    let (a, _) = actor(&mut app, here);
    let (key, _) = item(&mut app, here, Item::new("gate key"));
    run(&mut app, 1);
    interact(&mut app, a, "pick_up", key).unwrap();
    run(&mut app, 11);
    interact(&mut app, a, "unlock", d).unwrap();
    run(&mut app, 21);
    assert!(!app.world().get::<Door>(de).unwrap().is_locked());
}

#[test]
fn busy_units_reject_commands_and_aborts_cancel() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (a, ae) = actor(&mut app, here);
    let (axe, _) = item(&mut app, here, Item::new("axe"));
    let (wood, _) = item(&mut app, here, Item::new("wood"));
    run(&mut app, 1);
    interact(&mut app, a, "pick_up", axe).unwrap();
    run(&mut app, 1);
    let err = reason_of(interact(&mut app, a, "pick_up", wood).unwrap_err());
    assert_eq!(err, format!("unit {} is busy with pick_up #{}", a.0, axe.0));
    npc_interaction::cancel(app.world_mut(), ae, "test");
    assert_eq!(
        outcomes(&app, a).last().cloned(),
        Some(("pick_up".to_owned(), "cancelled".to_owned()))
    );
    let held = app.world().resource::<npc_jobs::Reservations>().held_by(a);
    assert!(held.is_empty(), "the claim is released");
}

// ---------------------------------------------------------------- determinism, opt-in

fn scenario_hash() -> u64 {
    let mut app = app();
    wall(&mut app, -16..=15, 0);
    door(&mut app, (0, 0), Door::closed());
    let mut agents = Vec::new();
    for i in 0..6 {
        let (_, e) = spawn_sim(
            app.world_mut(),
            (
                SimPos::new(-3500, -2500 + i * 1000),
                NavAgent,
                Interactor,
                Inventory::new(2, 5000),
            ),
        );
        agents.push(e);
    }
    let (axe, _) = item(&mut app, SimPos::new(-3500, -2500), Item::new("axe"));
    run(&mut app, 1);
    for (i, &e) in agents.iter().enumerate() {
        let goal = SimPos::new(3500, -2500 + 1000 * i as i32);
        set_move_intent(app.world_mut(), e, MoveIntent::to(goal, 80));
    }
    let first = *app.world().get::<SimId>(agents[0]).unwrap();
    interact(&mut app, first, "pick_up", axe).unwrap();
    run(&mut app, 300);
    state_hash(app.world_mut())
}

#[test]
fn two_runs_hash_the_same() {
    assert_eq!(scenario_hash(), scenario_hash());
}

#[test]
fn without_the_plugin_nothing_is_registered() {
    let mut bare = App::new();
    bare.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, NpcJobsPlugin));
    bare.finish();
    let manifest = &bare.world().resource::<ContentHash>().manifest;
    assert!(
        !manifest
            .iter()
            .any(|(k, n)| k.starts_with("interaction") || n == "item" || n == "door"),
        "{manifest:?}"
    );
    let with = app();
    let manifest = &with.world().resource::<ContentHash>().manifest;
    for verb in [
        "pick_up",
        "drop",
        "give",
        "open",
        "close",
        "lock",
        "unlock",
        "equip",
        "unequip",
        "use_tool_on",
    ] {
        assert!(
            manifest.contains(&("interaction".into(), verb.into())),
            "{verb}"
        );
    }
    assert!(
        !manifest.contains(&("interaction".into(), "mount".into())),
        "reserved only"
    );
}

// ------------------------------------------------- final review FX1 fixes

fn item_moved(app: &App) -> Vec<(SimId, Option<SimId>, Option<SimId>)> {
    journal(app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::ItemMoved { item, from, to, .. } => Some((item, from, to)),
            _ => None,
        })
        .collect()
}

/// Final review bug 2: a despawned holder left its items `CarriedBy` a dead
/// id and without a position (lost to the world). They drop at its last
/// position, in `SimId` order, and are back in the grid.
#[test]
fn despawned_holder_drops_its_items_where_it_stood() {
    let mut app = app();
    let at = SimPos::new(3000, 2000);
    let (holder, he) = actor(&mut app, at);
    let (other, oe) = actor(&mut app, SimPos::new(3500, 2000));
    let (wood, we) = item(&mut app, at, Item::new("wood").stack(3));
    let (axe, ae) = item(&mut app, at, Item::new("axe"));
    npc_interaction::items::put_in_inventory(app.world_mut(), he, we);
    npc_interaction::items::put_in_inventory(app.world_mut(), he, ae);
    npc_interaction::items::put_in_slot(app.world_mut(), he, ae, MAIN_HAND);
    run(&mut app, 1);
    submit(
        app.world_mut(),
        None,
        SimCommand::Despawn { unit: holder.0 },
    )
    .unwrap();
    run(&mut app, 2);
    let w = app.world();
    for (id, e) in [(wood, we), (axe, ae)] {
        assert!(w.get::<CarriedBy>(e).is_none(), "#{} still carried", id.0);
        assert_eq!(
            w.get::<SimPos>(e),
            Some(&at),
            "#{} dropped where it stood",
            id.0
        );
        assert!(
            grid(&app).ids_in_cell(grid(&app).cell_of(at)).contains(&id),
            "#{} back in the grid",
            id.0
        );
    }
    assert_eq!(
        item_moved(&app),
        vec![(wood, Some(holder), None), (axe, Some(holder), None)],
        "journaled in SimId order"
    );
    npc_interaction::run::can_interact(app.world(), oe, "pick_up", wood, None)
        .unwrap_or_else(|why| panic!("#{} cannot pick up the wood: {why}", other.0));
}

/// Final review bug 3: a carried item that leaves the sim left a ghost id
/// in its holder's inventory/equipment, blocking the slot forever.
#[test]
fn despawned_carried_items_leave_their_holder() {
    let mut app = app();
    let (_, ue) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 0),
            Interactor,
            Inventory::new(1, 10_000),
            Equipment::hands(),
        ),
    );
    let (axe, ae) = item(&mut app, SimPos::new(0, 0), Item::new("axe"));
    npc_interaction::items::put_in_inventory(app.world_mut(), ue, ae);
    npc_interaction::items::put_in_slot(app.world_mut(), ue, ae, MAIN_HAND);
    let (wood, we) = item(&mut app, SimPos::new(0, 0), Item::new("wood"));
    npc_interaction::items::put_in_inventory(app.world_mut(), ue, we);
    run(&mut app, 1);
    submit(app.world_mut(), None, SimCommand::Despawn { unit: axe.0 }).unwrap();
    submit(app.world_mut(), None, SimCommand::Despawn { unit: wood.0 }).unwrap();
    run(&mut app, 2);
    let w = app.world();
    assert_eq!(w.get::<Equipment>(ue).unwrap().slots[MAIN_HAND], None);
    assert!(w.get::<Inventory>(ue).unwrap().items.is_empty());
    // The freed slot takes a new axe.
    let (axe2, _) = item(&mut app, SimPos::new(200, 0), Item::new("axe"));
    npc_interaction::run::can_interact(app.world(), ue, "pick_up", axe2, None)
        .unwrap_or_else(|why| panic!("cannot pick up a new axe: {why}"));
}

/// Final review bug 4: `close` checked the doorway against the grid index
/// (positions at the start of the tick), so a unit that walked into the
/// door cell during the tick was shut in. The check uses live positions.
#[test]
fn close_is_invalid_when_a_unit_entered_the_doorway_this_tick() {
    let mut app = app();
    let (door_id, de) = door(&mut app, (3, 0), Door::opened());
    let (closer, _) = actor(&mut app, SimPos::new(2500, 1500));
    // A NavAgent that is not an Interactor (cannot open doors), 1000 mm per
    // tick along z = 500: in cell 2 after tick 9, in the door cell after
    // tick 10 (after the movers, before `close` completes).
    let (_, we) = spawn_sim(app.world_mut(), (SimPos::new(-7500, 500), NavAgent));
    let goal = SimPos::new(12_500, 500);
    set_move_intent(app.world_mut(), we, MoveIntent::to(goal, 1000));
    interact(&mut app, closer, "close", door_id).unwrap();
    run(&mut app, 11);
    let w = app.world();
    let wpos = *w.get::<SimPos>(we).unwrap();
    assert_eq!(
        grid(&app).cell_of(wpos),
        CellCoord::new(3, 0),
        "in the doorway"
    );
    assert!(
        w.get::<Door>(de).unwrap().open,
        "the door did not close on it"
    );
    assert_eq!(
        outcomes(&app, closer),
        vec![
            ("close".to_owned(), "started".to_owned()),
            ("close".to_owned(), "invalid".to_owned())
        ]
    );
    run(&mut app, 20);
    assert_eq!(
        *app.world().get::<SimPos>(we).unwrap(),
        goal,
        "walked through"
    );
}

// ------------------------------------------------- Task 26: item feedback

fn carry(app: &mut App, holder: Entity, item: Item) -> (SimId, Entity) {
    let (id, e) = spawn_sim(app.world_mut(), (SimPos::new(500, 500), item));
    npc_interaction::items::put_in_inventory(app.world_mut(), holder, e);
    (id, e)
}

/// The `(dropped, reason)` of `unit`'s last `Interacted` success.
fn last_success(app: &App, unit: SimId) -> (Vec<DroppedItem>, Option<String>, JournalKind) {
    journal(app)
        .into_iter()
        .rev()
        .filter(|e| e.unit == Some(unit))
        .find_map(|e| match &e.kind {
            JournalKind::Interacted {
                outcome,
                dropped,
                reason,
                ..
            } if outcome == "success" => Some((dropped.clone(), reason.clone(), e.kind.clone())),
            _ => None,
        })
        .expect("a success")
}

fn ground_items(app: &mut App, kind: &str) -> Vec<(SimId, SimPos, u32)> {
    let mut v: Vec<(SimId, SimPos, u32)> = app
        .world_mut()
        .query::<(&SimId, &Item, &SimPos)>()
        .iter(app.world())
        .filter(|(_, i, _)| i.kind == kind)
        .map(|(&id, i, &p)| (id, p, i.stack))
        .collect();
    v.sort_unstable_by_key(|x| x.0);
    v
}

/// Proposal 005: a tool yield is placed at the actor's feet. The success
/// entry lists every dropped item with its position and, when the pack could
/// not take it, why ("pack would carry 10500 of 10000 g").
#[test]
fn yield_overflow_says_where_it_dropped() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (a, ae) = actor(&mut app, here); // 3 slots, 10 kg
    let (_, axe_e) = carry(&mut app, ae, Item::new("axe")); // 1500 g
    npc_interaction::items::put_in_slot(app.world_mut(), ae, axe_e, MAIN_HAND);
    carry(&mut app, ae, Item::new("wood").stack(6)); // 3000 g, takes 2 more
    let (heavy, _) = carry(&mut app, ae, Item::new("wood").stack(10).quality(1)); // 5000 g
    let (tree, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(1500, 500),
            ToolTarget::new("tool:axe")
                .yields(ItemYield::new("wood", 2))
                .duration(5)
                .uses(3),
        ),
    );
    run(&mut app, 1);
    interact(&mut app, a, "use_tool_on", tree).unwrap();
    run(&mut app, 6);
    let ground = ground_items(&mut app, "wood");
    assert_eq!(ground.len(), 1, "{ground:?}");
    let (dropped, reason, kind) = last_success(&app, a);
    assert_eq!(
        dropped,
        vec![DroppedItem {
            item: ground[0].0,
            kind: "wood".into(),
            stack: 2,
            pos: [500, 500],
        }]
    );
    assert_eq!(
        reason.as_deref(),
        Some("pack would carry 10500 of 10000 g"),
        "9500 g carried + 1000 g"
    );
    let v = serde_json::to_value(&kind).unwrap();
    assert_eq!(
        v["dropped"],
        serde_json::json!([{"item": ground[0].0, "kind": "wood", "stack": 2, "pos": [500, 500]}])
    );

    // With room in the pack the yield still lands at the feet (no reason).
    interact(&mut app, a, "drop", heavy).unwrap();
    run(&mut app, 6);
    interact(&mut app, a, "use_tool_on", tree).unwrap();
    run(&mut app, 6);
    let (dropped, reason, _) = last_success(&app, a);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].pos, [500, 500]);
    assert_eq!(reason, None);
    // Entries without drops leave the field out of the JSON.
    let started = journal(&app)
        .into_iter()
        .find(
            |e| matches!(&e.kind, JournalKind::Interacted { outcome, .. } if outcome == "started"),
        )
        .unwrap();
    let v = serde_json::to_value(&started.kind).unwrap();
    assert!(v.get("dropped").is_none(), "{v}");
}

/// Proposal 008: a refusal for a full pack names the lightest items that
/// free a slot (weight, then `SimId`; at most three). The bare rule stays the
/// verb's own reason (what an order's state keeps); the command refusal and
/// the journal carry the hint.
#[test]
fn full_pack_names_what_to_drop() {
    let mut app = app();
    let here = SimPos::new(500, 500);
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (
            here,
            Interactor,
            Inventory::new(5, 100_000),
            Equipment::hands(),
        ),
    );
    carry(&mut app, ae, Item::new("rock")); // 9000 g
    let (w1, _) = carry(&mut app, ae, Item::new("wood").quality(1)); // 500 g
    let (axe, axe_e) = carry(&mut app, ae, Item::new("axe")); // 1500 g
    let (w2, _) = carry(&mut app, ae, Item::new("wood")); // 500 g
    let (k, _) = carry(&mut app, ae, Item::new("gate key")); // 50 g
    let (k2, _) = item(&mut app, here, Item::new("gate key"));
    run(&mut app, 1);
    let hinted = format!(
        "inventory is full (5 slots): drop 1 of [gate key #{}, wood #{}, wood #{}] to make room",
        k.0, w1.0, w2.0
    );
    let err = reason_of(interact(&mut app, a, "pick_up", k2).unwrap_err());
    assert_eq!(err, hinted);
    let bare = npc_interaction::begin(app.world_mut(), ae, "pick_up", k2, None, Source::Command)
        .unwrap_err();
    assert_eq!(bare, "inventory is full (5 slots)");
    let rejected: Vec<String> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::Interacted {
                outcome, reason, ..
            } if outcome == "rejected" => reason,
            _ => None,
        })
        .collect();
    assert_eq!(rejected, vec![hinted]);

    // Equip frees a slot (hand-held items take none); unequipping into a
    // full pack is refused with the same hint, and so is equipping another
    // hand tool (it says to unequip first, which needs a free slot).
    npc_interaction::items::put_in_slot(app.world_mut(), ae, axe_e, MAIN_HAND);
    let (axe2, _) = carry(&mut app, ae, Item::new("axe")); // 1500 g
    run(&mut app, 1);
    let list = format!(
        "drop 1 of [gate key #{}, wood #{}, wood #{}] to make room",
        k.0, w1.0, w2.0
    );
    let err = reason_of(interact(&mut app, a, "unequip", axe).unwrap_err());
    assert_eq!(err, format!("inventory is full (5 slots): {list}"));
    let err = reason_of(interact(&mut app, a, "equip", axe2).unwrap_err());
    assert_eq!(
        err,
        format!(
            "main_hand holds #{} (unequip it first; inventory is full (5 slots): {list})",
            axe.0
        )
    );
}
