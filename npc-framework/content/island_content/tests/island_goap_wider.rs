//! The wider resident planner (economy review npc-ai#9): a missing gathering
//! tool bought when the time it saves on the funded units pays for the trip,
//! demand elsewhere on the island (the boat's order beyond the port's makers)
//! planned to the delivery, recipe chains bounded by depth, determinism and
//! a save that continues in the middle of a plan.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, Villager, Wallet, civic, economy, goap, merchant, production,
    resident::actions::ACTIONS,
};
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use std::collections::BTreeMap;

const AXE: &str = "axe";
const LOG: &str = "log";
const PLANK: &str = "plank";
const SAW: &str = "village_saw";

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}

/// Living residents of `role`, ascending by identity.
fn of_role(world: &World, role: Role) -> Vec<(SimId, Entity)> {
    let mut found: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role == role)
                && village::living(world, **entity)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();
    found.sort_unstable_by_key(|(id, _)| *id);
    found
}

fn clear(world: &mut World, actor: Entity, kind: &str) {
    let held = economy::count(world, actor, kind);
    if held > 0 {
        economy::consume(world, actor, kind, held).unwrap();
    }
}

fn give(world: &mut World, actor: Entity, kind: &str, count: u32) {
    for _ in 0..count {
        economy::grant(world, actor, Item::new(kind));
    }
}

/// A woodcutter without an axe, `logs` funded logs to cut, one funded axe
/// and a smith with an axe for sale standing 14 m off on both axes, on the
/// side away from the nearest wood: the detour is some 56 m (about 800
/// ticks), more than the 180 ticks one log cut with the axe saves, less
/// than what eight save.
fn woodcutter_without_axe(logs: u32) -> (App, Entity) {
    let mut app = scene();
    let (cutter_id, cutter) = of_role(app.world(), Role::Woodcutter)[0];
    let (_, smith) = of_role(app.world(), Role::Smith)[0];
    clear(app.world_mut(), cutter, AXE);
    clear(app.world_mut(), smith, AXE);
    // Awake: the first ticks are night.
    for person in [cutter, smith] {
        app.world_mut()
            .get_mut::<Villager>(person)
            .unwrap()
            .resting_until = 0;
    }
    give(app.world_mut(), smith, AXE, 1);
    let here = *app.world().get::<SimPos>(cutter).unwrap();
    let wood = app
        .world_mut()
        .query::<(&village::ResourcePatch, &SimPos)>()
        .iter(app.world())
        .filter(|(patch, _)| patch.kind == LOG && patch.remaining > 0)
        .map(|(_, pos)| *pos)
        .min_by_key(|pos| (here.dist2(*pos), pos.x, pos.z))
        .expect("a wood");
    let away = |from: i32, to: i32| if from >= to { 14_000 } else { -14_000 };
    app.world_mut()
        .entity_mut(smith)
        .remove::<MoveIntent>()
        .insert(SimPos {
            x: here.x + away(here.x, wood.x),
            z: here.z + away(here.z, wood.z),
        });
    app.world_mut().get_mut::<Wallet>(cutter).unwrap().coins = 200;
    let mut ledger = production::ProductionPlan::default();
    ledger
        .production
        .insert(cutter_id, BTreeMap::from([(LOG.into(), logs)]));
    ledger
        .procurement
        .insert(cutter_id, BTreeMap::from([(AXE.into(), 1)]));
    app.world_mut().insert_resource(ledger);
    (app, cutter)
}

#[test]
fn a_missing_tool_is_bought_first_only_when_the_funded_work_pays_for_it() {
    let (mut app, cutter) = woodcutter_without_axe(8);
    let before = state_hash(app.world_mut());
    let plan = goap::plan(app.world(), cutter).expect("a plan for eight logs");
    assert_eq!(plan.goal, LOG);
    assert!(
        matches!(plan.steps.first(), Some(goap::PlanStep::Buy { kind, .. }) if kind == AXE),
        "eight logs pay for the axe: {plan:?}"
    );
    assert!(
        plan.steps
            .iter()
            .skip(1)
            .all(|step| matches!(step, goap::PlanStep::Gather { kind, .. } if kind == LOG))
    );
    // The same world plans the same; planning writes nothing.
    assert_eq!(goap::plan(app.world(), cutter), Some(plan));
    assert_eq!(state_hash(app.world_mut()), before);

    // One funded log does not pay for the trip to the seller.
    let (app, cutter) = woodcutter_without_axe(1);
    let plan = goap::plan(app.world(), cutter).expect("a plan for one log");
    assert_eq!(plan.steps.len(), 1, "{plan:?}");
    assert!(matches!(&plan.steps[0], goap::PlanStep::Gather { kind, .. } if kind == LOG));
}

#[test]
fn a_tool_the_ledger_does_not_fund_is_never_planned() {
    let (mut app, cutter) = woodcutter_without_axe(8);
    let id = *app.world().get::<SimId>(cutter).unwrap();
    app.world_mut()
        .resource_mut::<production::ProductionPlan>()
        .procurement
        .remove(&id);
    let plan = goap::plan(app.world(), cutter).expect("bare-handed work");
    assert!(
        plan.steps
            .iter()
            .all(|step| matches!(step, goap::PlanStep::Gather { .. })),
        "{plan:?}"
    );
}

/// The boat's order for planks, a carpenter of a settlement other than the
/// port with `planks` spare planks, standing at the trading post, and no
/// maker of the port funded to make them.
fn carpenter_for_the_boat(planks: u32) -> (App, Entity, SimId) {
    let mut app = scene();
    // The trading post is founded once the clock has started.
    run_ticks(app.world_mut(), 2);
    let port = app
        .world()
        .resource::<merchant::Merchant>()
        .settlement
        .expect("a port");
    let (post, post_entity) = merchant::post(app.world()).expect("a trading post");
    let carpenter = of_role(app.world(), Role::Carpenter)
        .into_iter()
        .map(|(_, entity)| entity)
        .find(|entity| production::economic_settlement(app.world(), *entity) != Some(port))
        .expect("a carpenter outside the port");
    app.world_mut()
        .insert_resource(production::ProductionPlan::default());
    {
        let mut boat = app.world_mut().resource_mut::<merchant::Merchant>();
        boat.order = vec![merchant::BoatOrder {
            good: PLANK.into(),
            units: 10,
            unit_price: 12,
            delivered: 0,
        }];
    }
    clear(app.world_mut(), carpenter, PLANK);
    give(app.world_mut(), carpenter, PLANK, planks);
    let at = *app.world().get::<SimPos>(post_entity).unwrap();
    // Awake (the first ticks are night), at the post.
    app.world_mut()
        .get_mut::<Villager>(carpenter)
        .unwrap()
        .resting_until = 0;
    app.world_mut()
        .entity_mut(carpenter)
        .remove::<MoveIntent>()
        .insert(at);
    (app, carpenter, post)
}

/// Gives planks to a neighbour of the carpenter's own settlement who may
/// sell them (another trade open to carpentry) until that settlement is no
/// longer short of planks.
fn stock_home(app: &mut App, carpenter: Entity) {
    let home = production::economic_settlement(app.world(), carpenter).unwrap();
    let members = village::demography::members(app.world(), home);
    for (_, member) in members {
        if member == carpenter {
            continue;
        }
        for _ in 0..20 {
            if !goap::elsewhere::short(app.world(), home, PLANK) {
                return;
            }
            give(app.world_mut(), member, PLANK, 2);
            if village::market::available_to_sell(app.world(), member, PLANK) == 0 {
                clear(app.world_mut(), member, PLANK);
                break;
            }
        }
    }
    assert!(
        !goap::elsewhere::short(app.world(), home, PLANK),
        "nobody at home sells planks"
    );
}

#[test]
fn demand_elsewhere_is_planned_to_its_delivery_and_never_as_own_work() {
    let (mut app, carpenter, post) = carpenter_for_the_boat(0);
    give(app.world_mut(), carpenter, LOG, 1);
    // While its own settlement is short of planks, the boat gets none.
    let home = production::economic_settlement(app.world(), carpenter).unwrap();
    if goap::elsewhere::short(app.world(), home, PLANK) {
        assert!(
            !goap::elsewhere::goals(app.world(), carpenter)
                .iter()
                .any(|goal| goal.kind == PLANK),
            "a settlement short of planks keeps them"
        );
    }
    stock_home(&mut app, carpenter);
    let before = state_hash(app.world_mut());
    assert_eq!(
        goap::plan(app.world(), carpenter),
        None,
        "no funded work of its own"
    );
    let remote = goap::elsewhere::goals(app.world(), carpenter);
    assert!(
        remote.iter().any(|goal| goal.kind == PLANK
            && goal.place.map(|(id, _)| id) == Some(post)
            && goal.units > 0),
        "{remote:?}"
    );
    let plan =
        goap::plan_in(app.world(), carpenter, goap::Scope::Elsewhere).expect("planks for the boat");
    assert_eq!(plan.goal, PLANK);
    assert_eq!(plan.deliver_to, Some(post));
    assert!(
        matches!(&plan.steps[0], goap::PlanStep::Craft { recipe, .. } if recipe == SAW),
        "{plan:?}"
    );
    assert!(
        matches!(plan.steps.last(), Some(goap::PlanStep::Deliver { kind, target, .. })
            if kind == PLANK && *target == post),
        "{plan:?}"
    );
    assert_eq!(
        goap::plan_in(app.world(), carpenter, goap::Scope::Elsewhere),
        Some(plan)
    );
    // Planning writes nothing.
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn a_full_load_is_carried_to_the_post_and_laid_there_for_sale() {
    let (mut app, carpenter, post) = carpenter_for_the_boat(4);
    stock_home(&mut app, carpenter);
    let plan = goap::plan_in(app.world(), carpenter, goap::Scope::Elsewhere).expect("a load");
    assert_eq!(plan.steps.len(), 1, "{plan:?}");
    let shelf = app.world().resource::<SimIndex>().get(post).unwrap();
    let stocked = app
        .world()
        .get::<civic::Depot>(shelf)
        .map_or(0, |depot| depot.stock(PLANK));
    let pursue = ACTIONS
        .iter()
        .find(|info| info.name == "life.pursue_goal")
        .unwrap();
    // A resident looks for unplanned demand once every `LOOK_TICKS`: take
    // the clock to its next turn (nothing ticks in this test).
    while !goap::elsewhere::looks_now(app.world(), carpenter) {
        app.world_mut().resource_mut::<SimClock>().tick += 1;
    }
    let status = (pursue.start)(
        app.world_mut(),
        carpenter,
        &ParamValue::Name("elsewhere".into()),
    );
    assert_ne!(status, Status::Failure);
    let delivered = app
        .world()
        .get::<civic::Depot>(shelf)
        .map_or(0, |depot| depot.stock(PLANK))
        - stocked;
    let goap::PlanStep::Deliver { count, .. } = &plan.steps[0] else {
        panic!("{plan:?}")
    };
    assert_eq!(delivered, *count);
    assert_eq!(
        economy::count(app.world(), carpenter, PLANK),
        4 - delivered,
        "the planks left the pack"
    );
}

#[test]
fn the_port_makers_own_plan_covers_the_order_first() {
    let (mut app, carpenter, post) = carpenter_for_the_boat(0);
    stock_home(&mut app, carpenter);
    assert!(
        goap::elsewhere::goals(app.world(), carpenter)
            .iter()
            .any(|goal| goal.place.map(|(id, _)| id) == Some(post)),
        "the order is open before the port's makers are planned"
    );
    let port = app
        .world()
        .resource::<merchant::Merchant>()
        .settlement
        .unwrap();
    // A carpenter of the port funded for the whole order leaves nothing open.
    let Some((port_id, _)) = of_role(app.world(), Role::Carpenter)
        .into_iter()
        .find(|(_, entity)| production::economic_settlement(app.world(), *entity) == Some(port))
    else {
        return;
    };
    app.world_mut()
        .resource_mut::<production::ProductionPlan>()
        .production
        .insert(port_id, BTreeMap::from([(PLANK.into(), 10)]));
    assert!(
        !goap::elsewhere::goals(app.world(), carpenter)
            .iter()
            .any(|goal| goal.place.map(|(id, _)| id) == Some(post)),
        "the port's own makers are planned to fill the order"
    );
}

/// A woodcutter's funded axe that no smith is planned to make: the smith
/// of its settlement plans it (inputs in the pack) and carries it to a stall
/// there; an order its smith is planned to make is left to that plan.
#[test]
fn an_unplanned_purchase_of_the_settlement_is_made_and_laid_out() {
    let mut app = scene();
    run_ticks(app.world_mut(), 2);
    let smiths = of_role(app.world(), Role::Smith);
    let cutters = of_role(app.world(), Role::Woodcutter);
    let found = smiths.iter().find_map(|(smith_id, smith)| {
        let home = production::economic_settlement(app.world(), *smith)?;
        let (cutter_id, _) = cutters.iter().find(|(_, cutter)| {
            production::economic_settlement(app.world(), *cutter) == Some(home)
        })?;
        Some((*smith_id, *smith, *cutter_id))
    });
    let Some((smith_id, smith, cutter_id)) = found else {
        panic!("no smith shares a settlement with a woodcutter")
    };
    // Nobody on the island has an axe to sell; the woodcutter's is funded.
    let people: Vec<Entity> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .collect();
    for person in people {
        if app.world().get::<Villager>(person).is_some() {
            clear(app.world_mut(), person, AXE);
        }
    }
    app.world_mut()
        .get_mut::<Villager>(smith)
        .unwrap()
        .resting_until = 0;
    for kind in ["ingot", PLANK] {
        clear(app.world_mut(), smith, kind);
    }
    give(app.world_mut(), smith, "ingot", 1);
    give(app.world_mut(), smith, PLANK, 1);
    let mut ledger = production::ProductionPlan::default();
    ledger
        .procurement
        .insert(cutter_id, BTreeMap::from([(AXE.into(), 1)]));
    app.world_mut().insert_resource(ledger);
    let goals = goap::elsewhere::goals(app.world(), smith);
    let Some(axe) = goals.iter().find(|goal| goal.kind == AXE) else {
        panic!("the woodcutter's axe is open demand for its smith: {goals:?}")
    };
    assert_eq!(axe.units, 1);
    let plan = goap::plan_in(app.world(), smith, goap::Scope::Elsewhere).expect("an axe");
    assert_eq!(plan.goal, AXE);
    assert_eq!(
        plan.steps
            .iter()
            .map(goap::PlanStep::label)
            .collect::<Vec<_>>(),
        ["craft village_forge_axe", "deliver axe"]
    );
    // Once the smith is planned to make it, the plan covers it.
    app.world_mut()
        .resource_mut::<production::ProductionPlan>()
        .production
        .insert(smith_id, BTreeMap::from([(AXE.into(), 1)]));
    assert!(
        !goap::elsewhere::goals(app.world(), smith)
            .iter()
            .any(|goal| goal.kind == AXE)
    );
}

#[test]
fn chains_stop_at_the_depth_bound() {
    // A smith's hammer is two recipe levels above ore (ore -> ingot -> hammer).
    let mut app = scene();
    let (smith_id, smith) = of_role(app.world(), Role::Smith)[0];
    for kind in ["ingot", "hammer", "ore", PLANK] {
        clear(app.world_mut(), smith, kind);
    }
    give(app.world_mut(), smith, "ore", 4);
    give(app.world_mut(), smith, LOG, 2);
    give(app.world_mut(), smith, PLANK, 1);
    let mut ledger = production::ProductionPlan::default();
    ledger
        .production
        .insert(smith_id, BTreeMap::from([("hammer".into(), 1)]));
    app.world_mut().insert_resource(ledger);
    let plan = goap::plan(app.world(), smith).expect("ore -> ingot -> hammer");
    assert_eq!(
        plan.steps
            .iter()
            .map(goap::PlanStep::label)
            .collect::<Vec<_>>(),
        ["craft village_smelt", "craft village_tool"]
    );
    app.world_mut()
        .resource_mut::<village::policy::IslandAiPolicy>()
        .goap
        .max_depth = 1;
    assert_eq!(
        goap::plan(app.world(), smith),
        None,
        "one level cannot reach the ingots from ore"
    );
}

/// Same seed, same world: the plans residents follow are the same, and a
/// save taken while somebody follows a plan for demand elsewhere continues
/// to the same state.
#[test]
fn plans_are_deterministic_and_a_save_continues_mid_plan() {
    let elsewhere = |app: &App| {
        app.world()
            .resource::<SimIndex>()
            .map
            .values()
            .any(|entity| {
                app.world()
                    .get::<goap::PlanMemo>(*entity)
                    .is_some_and(|memo| memo.plan.elsewhere)
            })
    };
    let mut a = scene();
    let mut b = scene();
    let day = a.world().resource::<SimConfig>().day_length;
    let mut found = false;
    for _ in 0..(3 * day / 100) {
        run_ticks(a.world_mut(), 100);
        run_ticks(b.world_mut(), 100);
        assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
        if elsewhere(&a) {
            found = true;
            break;
        }
    }
    assert!(found, "nobody planned for demand elsewhere in three days");
    let saved = snapshot::save(a.world_mut()).expect("saved");
    let mut continued = scene();
    snapshot::load(continued.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert_eq!(state_hash(continued.world_mut()), state_hash(a.world_mut()));
    for _ in 0..6 {
        run_ticks(a.world_mut(), 100);
        run_ticks(continued.world_mut(), 100);
        assert_eq!(state_hash(continued.world_mut()), state_hash(a.world_mut()));
    }
}
