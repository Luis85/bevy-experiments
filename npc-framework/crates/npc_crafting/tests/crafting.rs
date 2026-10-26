//! Wave F3: `npc_crafting` on test-only content (neutral item kinds, one
//! skill, one workstation kind; no game content).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_ai::{AiRegistry, BbKey, BbValue, Blackboard, Brain};
use npc_crafting::{
    Craft, CraftCompleted, CraftError, CraftResult, CraftSource, Crafting, CraftingAppExt,
    CraftingOrders, CraftingPlugin, Durability, PostCraftOrder, RecipeDef, RecipeRegistry,
    Workstation, begin_craft, can_craft, crafting_jobs, validate_recipe,
};
use npc_interaction::items::{put_in_inventory, put_in_slot};
use npc_interaction::{
    Equipment, InteractionAppExt, InteractionPlugin, InteractionRegistry, Interactor, Inventory,
    Item, ItemKindDef, MAIN_HAND,
};
use npc_jobs::{NpcJobsPlugin, Reservations};
use npc_kernel::hash::state_hash;
use npc_kernel::journal::JournalKind;
use npc_kernel::registry::KernelAppExt;
use npc_kernel::spatial::SpatialPlugin;
use npc_kernel::{
    CommandError, Journal, JournalPlugin, NpcKernelPlugin, SimCommand, SimId, SimIndex, SimPos,
    run_ticks, spawn_sim, submit,
};
use npc_rules::{
    CharacterSheet, Difficulty, RulesAppExt, RulesPlugin, RulesRegistry, SkillDef, Stat,
};

#[derive(Resource, Default)]
struct Seen(Vec<CraftCompleted>);

fn remember(world: &mut World, done: &CraftCompleted) {
    world.resource_mut::<Seen>().0.push(done.clone());
}

fn chair() -> RecipeDef {
    RecipeDef::new("chair")
        .input("material:wood", 3)
        .input("nails", 2)
        .tool("tool:hammer")
        .station("bench")
        .skill("Carpentry", 0)
        .duration(10)
        .output("chair", 1)
        .xp(25)
}

fn plank() -> RecipeDef {
    RecipeDef::new("plank")
        .input("wood", 1)
        .skill("Carpentry", 0)
        .output("plank", 2)
}

fn content(app: &mut App) {
    app.register_item_kind(
        ItemKindDef::new("wood")
            .weight_g(500)
            .stackable(50)
            .tag("material:wood"),
    )
    .register_item_kind(ItemKindDef::new("nails").weight_g(10).stackable(100))
    .register_item_kind(
        ItemKindDef::new("hammer")
            .weight_g(800)
            .tag("tool:hammer")
            .equip_slot(MAIN_HAND),
    )
    .register_item_kind(ItemKindDef::new("chair").weight_g(4000))
    .register_item_kind(ItemKindDef::new("plank").weight_g(300).stackable(50))
    .register_skill(
        SkillDef::new("Carpentry", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_recipe(chair())
    .register_recipe(plank())
    .register_recipe(
        RecipeDef::new("splinter")
            .input("wood", 1)
            .output("plank", 1),
    )
    .add_craft_hook("test.seen", remember)
    .register_action("Hold", |_, _, _| npc_ai::Status::Running, |_, _, _| {})
    .register_tree("work", |b| {
        b.leaf("Work", "Craft", ());
    });
}

fn app_with(seed: u64, journal: bool, extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        SpatialPlugin,
        NpcJobsPlugin,
        InteractionPlugin::default(),
        RulesPlugin,
        CraftingPlugin,
    ));
    if journal {
        app.add_plugins(JournalPlugin::default());
    }
    app.init_resource::<Seen>();
    content(&mut app);
    extra(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn app(seed: u64) -> App {
    app_with(seed, true, |_| {})
}

const HERE: SimPos = SimPos { x: 500, z: 500 };

fn crafter(app: &mut App, pos: SimPos, carpentry: i32) -> (SimId, Entity) {
    let mut sheet = CharacterSheet::default();
    if carpentry > 0 {
        sheet.skills.insert("Carpentry".into(), carpentry);
    }
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Interactor,
            Inventory::new(10, 1_000_000),
            Equipment::hands(),
            sheet,
        ),
    )
}

fn give(app: &mut App, holder: Entity, item: Item) -> (SimId, Entity) {
    let (id, e) = spawn_sim(app.world_mut(), (item, HERE));
    put_in_inventory(app.world_mut(), holder, e);
    (id, e)
}

fn equip(app: &mut App, holder: Entity, item: Item) -> (SimId, Entity) {
    let (id, e) = give(app, holder, item);
    put_in_slot(app.world_mut(), holder, e, MAIN_HAND);
    (id, e)
}

fn bench(app: &mut App, pos: SimPos) -> SimId {
    spawn_sim(app.world_mut(), (pos, Workstation::new(["bench"]))).0
}

/// A crafter at a bench with everything for one chair (hammer with 20
/// durability, quality 0).
fn chair_setup(app: &mut App, carpentry: i32) -> (SimId, Entity, SimId, SimId, SimId) {
    let ws = bench(app, HERE);
    let (u, ue) = crafter(app, HERE, carpentry);
    let (wood, _) = give(app, ue, Item::new("wood").stack(5));
    give(app, ue, Item::new("nails").stack(2));
    let (hammer, he) = equip(app, ue, Item::new("hammer"));
    app.world_mut().entity_mut(he).insert(Durability::new(20));
    (u, ue, ws, wood, hammer)
}

fn get<T: Component + Clone>(app: &App, id: SimId) -> Option<T> {
    let e = app.world().resource::<SimIndex>().get(id)?;
    app.world().get::<T>(e).cloned()
}

fn seen(app: &App) -> Vec<CraftCompleted> {
    app.world().resource::<Seen>().0.clone()
}

fn craft_kinds(app: &App, unit: SimId) -> Vec<JournalKind> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.unit == Some(unit))
        .filter(|e| {
            matches!(
                e.kind,
                JournalKind::CraftStarted { .. }
                    | JournalKind::CraftDone { .. }
                    | JournalKind::CraftFailed { .. }
            )
        })
        .map(|e| e.kind.clone())
        .collect()
}

fn inventory_kinds(app: &App, unit: SimId) -> Vec<(String, u32)> {
    get::<Inventory>(app, unit)
        .unwrap()
        .items
        .iter()
        .filter_map(|&id| get::<Item>(app, id))
        .map(|i| (i.kind, i.stack))
        .collect()
}

// ------------------------------------------------------------ validation

#[test]
fn recipes_are_validated_against_items_and_skills() {
    let app = app(1);
    let items = app.world().resource::<InteractionRegistry>().clone();
    let rules = app.world().resource::<RulesRegistry>().clone();
    assert_eq!(validate_recipe(&chair(), &items, &rules), Ok(()));
    let check = |def: RecipeDef| validate_recipe(&def, &items, &rules).unwrap_err();
    assert_eq!(check(RecipeDef::new("x")), "recipe `x` has no outputs");
    assert_eq!(
        check(RecipeDef::new("x").output("throne", 1)),
        "recipe `x`: output `throne` is not a registered item kind"
    );
    assert_eq!(
        check(RecipeDef::new("x").input("gold", 1).output("chair", 1)),
        "recipe `x`: input `gold` matches no registered item kind or tag"
    );
    assert_eq!(
        check(RecipeDef::new("x").input("wood", 0).output("chair", 1)),
        "recipe `x`: input `wood` needs a count > 0"
    );
    assert_eq!(
        check(RecipeDef::new("x").tool("tool:saw").output("chair", 1)),
        "recipe `x`: tool `tool:saw` matches no registered item kind or tag"
    );
    assert_eq!(
        check(RecipeDef::new("x").skill("Juggling", 0).output("chair", 1)),
        "recipe `x`: unknown skill `Juggling` (known: Carpentry)"
    );
    let names: Vec<_> = app
        .world()
        .resource::<RecipeRegistry>()
        .recipes
        .keys()
        .cloned()
        .collect();
    assert_eq!(names, ["chair", "plank", "splinter"]);
}

#[test]
#[should_panic(expected = "recipe `bad`: input `gold` matches no registered item kind or tag")]
fn an_invalid_recipe_fails_the_freeze() {
    let _ = app_with(1, false, |app| {
        app.register_recipe(RecipeDef::new("bad").input("gold", 1).output("chair", 1));
    });
}

#[test]
fn missing_inputs_tools_and_workstations_are_rejected_with_reasons() {
    let mut app = app(1);
    let ws = bench(&mut app, HERE);
    let far = bench(&mut app, SimPos::new(9500, 500));
    let shelf = spawn_sim(app.world_mut(), (HERE, Workstation::new(["shelf"]))).0;
    let rock = spawn_sim(app.world_mut(), HERE).0;
    let (u, ue) = crafter(&mut app, HERE, 4);
    let why = |app: &App, recipe: &str, at: Option<SimId>| {
        can_craft(app.world(), ue, recipe, at)
            .map(|_| ())
            .unwrap_err()
            .to_string()
    };
    assert_eq!(why(&app, "chair", None), "needs a workstation: bench");
    assert_eq!(
        why(&app, "chair", Some(rock)),
        format!("#{} is not a workstation", rock.0)
    );
    assert_eq!(
        why(&app, "chair", Some(shelf)),
        format!("#{} is not a bench", shelf.0)
    );
    assert_eq!(
        why(&app, "chair", Some(far)),
        "too far: 9.0 m (reach 1.5 m)"
    );
    // Every input and tool is named, the missing ones first (Task 26).
    assert_eq!(
        why(&app, "chair", Some(ws)),
        "needs 3 material:wood (has 0), 2 nails (has 0), tool hammer (has 0)"
    );
    give(&mut app, ue, Item::new("hammer")); // carried, not equipped: enough
    assert_eq!(
        why(&app, "chair", Some(ws)),
        "needs 3 material:wood (has 0), 2 nails (has 0), tool hammer (has 1)"
    );
    give(&mut app, ue, Item::new("wood").stack(2));
    assert_eq!(
        why(&app, "chair", Some(ws)),
        "needs 3 material:wood (has 2), 2 nails (has 0), tool hammer (has 1)"
    );
    give(&mut app, ue, Item::new("wood"));
    assert_eq!(
        why(&app, "chair", Some(ws)),
        "needs 2 nails (has 0), 3 material:wood (has 3), tool hammer (has 1)"
    );
    give(&mut app, ue, Item::new("nails").stack(2));
    assert!(can_craft(app.world(), ue, "chair", Some(ws)).is_ok());
    assert_eq!(why(&app, "plank", Some(ws)), "needs no workstation");
    assert_eq!(
        why(&app, "boat", None),
        "unknown recipe `boat` (known: chair, plank, splinter)"
    );
    // The command is refused with the same reason, and nothing starts.
    let err = submit(
        app.world_mut(),
        None,
        SimCommand::content(&Craft::new(u, "chair")),
    )
    .unwrap_err();
    assert_eq!(
        err,
        CommandError::Unavailable {
            reason: "needs a workstation: bench".into()
        }
    );
    assert!(get::<Crafting>(&app, u).is_none());
}

#[test]
fn a_sheet_without_the_skill_cannot_attempt_it() {
    let mut app = app_with(1, false, |app| {
        app.register_skill(SkillDef::new("Weaving", Stat::Dx, Difficulty::Hard))
            .register_recipe(
                RecipeDef::new("rug")
                    .input("wood", 1)
                    .skill("Weaving", 0)
                    .output("plank", 1),
            );
    });
    let (_, ue) = crafter(&mut app, HERE, 0);
    give(&mut app, ue, Item::new("wood"));
    assert_eq!(
        can_craft(app.world(), ue, "rug", None).unwrap_err(),
        CraftError::NoSkill {
            skill: "Weaving".into()
        }
    );
    // Without a character sheet at all: no roll, a plain success.
    let (u2, u2e) = spawn_sim(
        app.world_mut(),
        (HERE, Interactor, Inventory::new(4, 100_000)),
    );
    give(&mut app, u2e, Item::new("wood"));
    begin_craft(
        app.world_mut(),
        u2e,
        "rug",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let done = &seen(&app)[0];
    assert_eq!(
        (done.unit, done.result, done.margin),
        (u2, CraftResult::Success, 0)
    );
}

// ---------------------------------------------------------- running a craft

#[test]
fn a_craft_reserves_consumes_its_inputs_and_makes_the_output() {
    let mut app = app(3);
    let (u, ue, ws, wood, hammer) = chair_setup(&mut app, 40);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Craft::new(u, "chair").at(ws)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let c = get::<Crafting>(&app, u).expect("crafting");
    assert_eq!(c.claimed.len(), 3, "wood, nails and the hammer are claimed");
    {
        let r = app.world().resource::<Reservations>();
        assert!(r.holds(ws, u) && r.holds(wood, u) && r.holds(hammer, u));
    }
    run_ticks(app.world_mut(), 5);
    assert_eq!(
        inventory_kinds(&app, u),
        [("wood".to_owned(), 5), ("nails".to_owned(), 2)]
    );
    run_ticks(app.world_mut(), 10);
    assert!(get::<Crafting>(&app, u).is_none());
    let done = seen(&app);
    assert_eq!(done.len(), 1, "the hook fires once");
    let d = &done[0];
    assert!(d.result.succeeded(), "level 20 carpentry: {d:?}");
    assert_eq!(
        (d.recipe.as_str(), d.workstation, d.xp, d.duration_ticks),
        ("chair", Some(ws), 25, 10)
    );
    assert_eq!(d.skill.as_deref(), Some("Carpentry"));
    let mut inv = inventory_kinds(&app, u);
    inv.sort();
    assert_eq!(
        inv,
        [("chair".to_owned(), 1), ("wood".to_owned(), 2)],
        "nails used up"
    );
    let chair_item = get::<Item>(&app, d.outputs[0]).unwrap();
    assert_eq!(Some(chair_item.quality), d.quality);
    assert_eq!(
        get::<Durability>(&app, hammer).unwrap().current,
        19,
        "one use of wear"
    );
    assert!(app.world().resource::<Reservations>().held_by(u).is_empty());
    let kinds = craft_kinds(&app, u);
    assert!(
        matches!(&kinds[0], JournalKind::CraftStarted { recipe, ends, .. } if recipe == "chair" && *ends == 10)
    );
    assert!(matches!(&kinds[1], JournalKind::CraftDone { outputs, .. } if outputs == &d.outputs));
    let _ = ue;
}

#[test]
fn two_crafters_contend_for_one_workstation() {
    let mut app = app(5);
    let (a, ae, ws, _, _) = chair_setup(&mut app, 40);
    let (b, be) = crafter(&mut app, HERE, 40);
    give(&mut app, be, Item::new("wood").stack(3));
    give(&mut app, be, Item::new("nails").stack(2));
    equip(&mut app, be, Item::new("hammer"));
    begin_craft(
        app.world_mut(),
        ae,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    let err = begin_craft(
        app.world_mut(),
        be,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap_err();
    assert_eq!(err, CraftError::WorkstationInUse { workstation: ws.0 });
    assert!(matches!(
        craft_kinds(&app, b).last(),
        Some(JournalKind::CraftFailed { outcome, reason, .. }) if outcome == "rejected" && reason == &format!("#{} is in use", ws.0)
    ));
    // Busy with one craft, `a` cannot start another.
    assert_eq!(
        can_craft(app.world(), ae, "plank", None).map(|_| ()),
        Ok(()),
        "can_craft ignores busy"
    );
    assert!(matches!(
        begin_craft(
            app.world_mut(),
            ae,
            "plank",
            None,
            CraftSource::Command,
            None
        ),
        Err(CraftError::Busy { .. })
    ));
    run_ticks(app.world_mut(), 11);
    assert!(get::<Crafting>(&app, a).is_none());
    begin_craft(
        app.world_mut(),
        be,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .expect("free once the first craft is done");
}

#[test]
fn a_changed_world_invalidates_the_craft_and_releases_its_claims() {
    let mut app = app(2);
    let (u, ue, ws, _, _) = chair_setup(&mut app, 40);
    begin_craft(
        app.world_mut(),
        ue,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    // The bench is removed mid-craft.
    let wse = app.world().resource::<SimIndex>().get(ws).unwrap();
    app.world_mut().despawn(wse);
    run_ticks(app.world_mut(), 12);
    assert!(seen(&app).is_empty(), "invalid crafts are not completions");
    assert!(matches!(
        craft_kinds(&app, u).last(),
        Some(JournalKind::CraftFailed { outcome, consumed: false, .. }) if outcome == "invalid"
    ));
    assert_eq!(inventory_kinds(&app, u).len(), 2, "wood and nails kept");
    assert!(app.world().resource::<Reservations>().held_by(u).is_empty());
}

// ------------------------------------------------------- quality and crits

fn plank_quality_total(seed: u64) -> ((i32, u32), (i32, u32)) {
    let mut app = app_with(seed, false, |_| {});
    let (low, le) = crafter(&mut app, HERE, 1);
    let (high, he) = crafter(&mut app, HERE, 16);
    for e in [le, he] {
        give(&mut app, e, Item::new("wood"));
        begin_craft(
            app.world_mut(),
            e,
            "plank",
            None,
            CraftSource::Command,
            None,
        )
        .unwrap();
    }
    run_ticks(app.world_mut(), 1);
    let mut low_q = (0, 0);
    let mut high_q = (0, 0);
    for d in seen(&app) {
        let slot = if d.unit == low {
            &mut low_q
        } else {
            &mut high_q
        };
        assert!(d.unit == low || d.unit == high);
        slot.0 += d.quality.unwrap_or(0);
        slot.1 += u32::from(d.result.succeeded());
    }
    (low_q, high_q)
}

#[test]
fn higher_skill_gives_higher_quality_on_average() {
    let (mut low, mut high) = ((0, 0), (0, 0));
    for seed in 0..60 {
        let (l, h) = plank_quality_total(seed);
        low = (low.0 + l.0, low.1 + l.1);
        high = (high.0 + h.0, high.1 + h.1);
    }
    assert!(high.1 > low.1, "more successes: {high:?} vs {low:?}");
    assert!(high.0 > low.0, "more quality in total: {high:?} vs {low:?}");
    // Mean quality per success: high.0 / high.1 > low.0 / low.1.
    assert!(
        high.0 * low.1 as i32 > low.0 * high.1 as i32,
        "{high:?} vs {low:?}"
    );
    assert_eq!(
        plank_quality_total(7),
        plank_quality_total(7),
        "deterministic"
    );
}

/// The first seed whose chair attempt (at `carpentry`) ends with `want`.
fn chair_with(carpentry: i32, want: CraftResult, durability: bool) -> (App, SimId, SimId) {
    for seed in 0..400 {
        let mut app = app_with(seed, true, |_| {});
        let (u, ue, ws, _, hammer) = chair_setup(&mut app, carpentry);
        if !durability {
            let he = app.world().resource::<SimIndex>().get(hammer).unwrap();
            app.world_mut().entity_mut(he).remove::<Durability>();
            app.world_mut().get_mut::<Item>(he).unwrap().quality = 1;
        }
        begin_craft(
            app.world_mut(),
            ue,
            "chair",
            Some(ws),
            CraftSource::Command,
            None,
        )
        .unwrap();
        run_ticks(app.world_mut(), 11);
        if seen(&app).first().is_some_and(|d| d.result == want) {
            return (app, u, hammer);
        }
    }
    panic!("no seed in 0..400 gave {want:?}");
}

#[test]
fn critical_failure_consumes_inputs_and_damages_the_tool() {
    let (app, u, hammer) = chair_with(0, CraftResult::CriticalFailure, true);
    let d = &seen(&app)[0];
    assert!(d.outputs.is_empty() && d.quality.is_none());
    assert_eq!(
        inventory_kinds(&app, u),
        [("wood".to_owned(), 2)],
        "inputs used up"
    );
    assert_eq!(
        get::<Durability>(&app, hammer).unwrap().current,
        10,
        "20 - crit wear 10"
    );
    assert!(matches!(
        craft_kinds(&app, u).last(),
        Some(JournalKind::CraftFailed { outcome, consumed: true, .. }) if outcome == "critical"
    ));
    // Without durability the tool loses a quality level instead.
    let (app, _, hammer) = chair_with(0, CraftResult::CriticalFailure, false);
    assert_eq!(get::<Item>(&app, hammer).unwrap().quality, 0);
}

#[test]
fn ordinary_failure_spends_time_but_keeps_inputs() {
    let (app, u, hammer) = chair_with(0, CraftResult::Failure, true);
    let mut inv = inventory_kinds(&app, u);
    inv.sort();
    assert_eq!(inv, [("nails".to_owned(), 2), ("wood".to_owned(), 5)]);
    assert_eq!(get::<Durability>(&app, hammer).unwrap().current, 19);
    assert!(matches!(
        craft_kinds(&app, u).last(),
        Some(JournalKind::CraftFailed { outcome, consumed: false, reason, .. })
            if outcome == "failed" && reason.starts_with("Carpentry roll failed by")
    ));
}

#[test]
fn critical_success_adds_a_quality_bonus() {
    let (app, _, _) = chair_with(40, CraftResult::CriticalSuccess, true);
    let d = &seen(&app)[0];
    let base = (d.margin.max(0) / 2).min(5);
    assert_eq!(d.quality, Some(base + 2));
}

#[test]
fn a_tool_at_zero_durability_breaks() {
    let mut app = app(3);
    let (u, ue, ws, _, hammer) = chair_setup(&mut app, 40);
    let he = app.world().resource::<SimIndex>().get(hammer).unwrap();
    app.world_mut().entity_mut(he).insert(Durability::new(1));
    begin_craft(
        app.world_mut(),
        ue,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    run_ticks(app.world_mut(), 11);
    assert!(get::<Item>(&app, hammer).is_none(), "the hammer broke");
    assert!(get::<Equipment>(&app, u).unwrap().items().is_empty());
}

// ------------------------------------------------------- orders and the leaf

#[test]
fn workstation_orders_are_worked_by_the_craft_leaf() {
    let mut app = app(9);
    let (u, ue, ws, _, _) = chair_setup(&mut app, 40);
    give(&mut app, ue, Item::new("wood").stack(1));
    give(&mut app, ue, Item::new("nails").stack(2));
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&PostCraftOrder {
            workstation: ws.0,
            recipe: "chair".into(),
            count: 2,
        }),
    )
    .unwrap();
    let err = submit(
        app.world_mut(),
        None,
        SimCommand::content(&PostCraftOrder {
            workstation: ws.0,
            recipe: "plank".into(),
            count: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "needs no workstation");
    run_ticks(app.world_mut(), 1);
    let jobs = crafting_jobs(app.world_mut());
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        (
            jobs[0].workstation,
            jobs[0].recipe.as_str(),
            jobs[0].remaining
        ),
        (ws, "chair", 2)
    );
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("work")
        .unwrap();
    app.world_mut().entity_mut(ue).insert(Brain { tree });
    app.world_mut()
        .get_mut::<Blackboard>(ue)
        .unwrap()
        .set(BbKey(7), BbValue::U32(ws.0));
    run_ticks(app.world_mut(), 60);
    let done = seen(&app);
    assert_eq!(done.len(), 2, "both orders worked: {done:?}");
    assert!(done.iter().all(|d| d.unit == u && d.result.succeeded()));
    assert!(get::<CraftingOrders>(&app, ws).unwrap().orders.is_empty());
    assert!(crafting_jobs(app.world_mut()).is_empty());
}

// ------------------------------------------------------------ determinism

fn scripted(seed: u64, journal: bool) -> u64 {
    let mut app = app_with(seed, journal, |_| {});
    let (_, ue, ws, _, _) = chair_setup(&mut app, 8);
    let (_, pe) = crafter(&mut app, HERE, 2);
    give(&mut app, pe, Item::new("wood").stack(6));
    begin_craft(
        app.world_mut(),
        ue,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    for _ in 0..6 {
        let _ = begin_craft(
            app.world_mut(),
            pe,
            "plank",
            None,
            CraftSource::Command,
            None,
        );
        run_ticks(app.world_mut(), 2);
    }
    run_ticks(app.world_mut(), 10);
    state_hash(app.world_mut())
}

#[test]
fn crafting_is_deterministic_and_independent_of_the_journal() {
    let h = scripted(11, true);
    assert_eq!(scripted(11, true), h);
    assert_eq!(scripted(11, false), h);
}

#[test]
fn without_the_plugin_nothing_is_registered() {
    let mut app = App::new();
    app.add_plugins((
        NpcKernelPlugin::default(),
        InteractionPlugin::default(),
        RulesPlugin,
    ));
    app.finish();
    let names: Vec<_> = app
        .world()
        .resource::<npc_kernel::hash::SimHashRegistry>()
        .components
        .iter()
        .map(|c| c.name)
        .collect();
    assert!(!names.contains(&"workstation") && !names.contains(&"crafting"));
    assert!(app.world().get_resource::<RecipeRegistry>().is_none());
}

// ------------------------------------------------- final review FX1 fixes

/// Final review bug 6: a craft re-picked its inputs at completion instead
/// of using the ones it claimed, so it could consume an item it never
/// reserved (here: a lower-`SimId` stack another unit held at the start).
#[test]
fn a_craft_consumes_exactly_the_inputs_it_claimed() {
    let mut app = app(3);
    let ws = bench(&mut app, HERE);
    let (u, ue) = crafter(&mut app, HERE, 40);
    let (other, _) = crafter(&mut app, HERE, 0);
    let (wood_a, _) = give(&mut app, ue, Item::new("wood").stack(3));
    let (wood_b, _) = give(&mut app, ue, Item::new("wood").stack(3));
    let (nails, _) = give(&mut app, ue, Item::new("nails").stack(2));
    let (hammer, _) = equip(&mut app, ue, Item::new("hammer"));
    // Someone else holds the first stack when the craft starts.
    assert!(npc_jobs::reserve(app.world_mut(), other, wood_a, 1));
    begin_craft(
        app.world_mut(),
        ue,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    let c = get::<Crafting>(&app, u).expect("crafting");
    assert_eq!(c.tools, vec![hammer]);
    assert_eq!(c.inputs, vec![(wood_b, 3), (nails, 2)]);
    // ... and lets it go meanwhile.
    app.world_mut()
        .resource_mut::<Reservations>()
        .release(wood_a, other);
    run_ticks(app.world_mut(), 11);
    let done = seen(&app);
    assert_eq!(done.len(), 1);
    assert!(done[0].result.succeeded(), "{:?}", done[0]);
    assert_eq!(
        get::<Item>(&app, wood_a).map(|i| i.stack),
        Some(3),
        "the unclaimed stack is untouched"
    );
    assert!(
        get::<Item>(&app, wood_b).is_none(),
        "the claimed stack is used up"
    );
}

/// Bug 6: a claimed input that shrank below what the craft takes makes the
/// completion invalid (nothing consumed), even if other stacks would cover
/// it.
#[test]
fn a_craft_whose_claimed_input_shrank_is_invalid() {
    let mut app = app(3);
    let (u, ue, ws, wood, _) = chair_setup(&mut app, 40);
    begin_craft(
        app.world_mut(),
        ue,
        "chair",
        Some(ws),
        CraftSource::Command,
        None,
    )
    .unwrap();
    give(&mut app, ue, Item::new("wood").stack(5));
    let we = app.world().resource::<SimIndex>().get(wood).unwrap();
    app.world_mut().get_mut::<Item>(we).unwrap().stack = 2;
    run_ticks(app.world_mut(), 11);
    assert!(seen(&app).is_empty(), "no completion");
    assert!(matches!(
        craft_kinds(&app, u).last(),
        Some(JournalKind::CraftFailed { outcome, .. }) if outcome == "invalid"
    ));
    assert_eq!(get::<Item>(&app, wood).map(|i| i.stack), Some(2));
}

/// Final review cleanup 7: `PostCraftOrder` is capped per workstation.
#[test]
fn post_craft_order_is_capped_per_workstation() {
    let mut app = app(1);
    let ws = bench(&mut app, HERE);
    for _ in 0..npc_crafting::MAX_CRAFT_ORDERS {
        npc_crafting::post_order(app.world_mut(), ws, "chair", 1).unwrap();
    }
    let err = submit(
        app.world_mut(),
        None,
        SimCommand::content(&PostCraftOrder {
            workstation: ws.0,
            recipe: "chair".into(),
            count: 1,
        }),
    )
    .unwrap_err();
    assert!(
        matches!(&err, CommandError::Unavailable { reason } if reason.contains("orders")),
        "{err:?}"
    );
    assert!(npc_crafting::post_order(app.world_mut(), ws, "chair", 1).is_err());
}

// ------------------------------------------------- Task 26: craft feedback

/// Proposal 006: a craft that cannot start names every input and tool with
/// what the crafter has (the missing ones first), not only the first gap;
/// the error JSON lists them too. The plan of a craft that can start is
/// unchanged.
#[test]
fn craft_names_every_missing_input() {
    let mut app = app(1);
    let ws = bench(&mut app, HERE);
    let (_, ue) = crafter(&mut app, HERE, 4);
    give(&mut app, ue, Item::new("wood").stack(5));
    equip(&mut app, ue, Item::new("hammer"));
    let err = can_craft(app.world(), ue, "chair", Some(ws)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "needs 2 nails (has 0), 3 material:wood (has 5), tool hammer (has 1)"
    );
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(v["type"], "Missing");
    assert_eq!(
        v["needs"],
        serde_json::json!([
            {"key": "nails", "tool": false, "need": 2, "have": 0},
            {"key": "material:wood", "tool": false, "need": 3, "have": 5},
            {"key": "tool:hammer", "tool": true, "need": 1, "have": 1},
        ])
    );
    give(&mut app, ue, Item::new("nails").stack(2));
    let plan = can_craft(app.world(), ue, "chair", Some(ws)).unwrap();
    assert_eq!(plan.tools.len(), 1);
    assert_eq!(
        plan.inputs.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
        vec![3, 2],
        "takes exactly what the recipe needs"
    );
}
