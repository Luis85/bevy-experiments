//! Regression tests for code-review findings (ported from the reviewer's probes).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::ai::{AiRegistry, Brain, BrainState, BrainStats};
use npc_framework::kernel::ids::spawn_sim;
use npc_framework::prelude::{
    NeedLevel, NeedRegistry, Needs, NpcPlugins, PrevSimPos, Provides, Reservations, SimClock,
    SimConfig, SimId, SimIndex, SimPos, run_ticks,
};
use village_content::actions::{Eat, Sleep, Work};
use village_content::needs::{ENERGY, HUNGER, VillagerNeeds, need_id};
use village_content::scenario::{VillageIds, spawn_village};
use village_content::villager::VillagerTree;
use village_content::{Activity, Stockpile, VillagePlugin, Villager};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn village(seed: u64) -> (App, VillageIds) {
    let mut app = App::new();
    app.add_plugins((NpcPlugins::new().seed(seed).without_driver(), VillagePlugin));
    let ids = spawn_village(app.world_mut());
    (app, ids)
}

fn ent(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("sim entity exists")
}

fn path_of(app: &App, e: Entity) -> String {
    let st = app
        .world()
        .get::<BrainState>(e)
        .expect("agent has BrainState");
    let tree = app.world().resource::<VillagerTree>().0;
    app.world()
        .resource::<AiRegistry>()
        .tree(tree)
        .path_string(&st.running_path)
}

fn tick(app: &App) -> u64 {
    app.world().resource::<SimClock>().tick
}

fn set_tick(app: &mut App, tick: u64) {
    app.world_mut().insert_resource(SimClock { tick });
}

fn food(app: &App, ids: VillageIds) -> u32 {
    app.world()
        .get::<Stockpile>(ent(app, ids.stockpile))
        .unwrap()
        .food
}

fn set_food(app: &mut App, ids: VillageIds, food: u32) {
    let e = ent(app, ids.stockpile);
    app.world_mut().get_mut::<Stockpile>(e).unwrap().food = food;
}

fn set_need(app: &mut App, e: Entity, name: &str, value: i32, level: NeedLevel) {
    let id = need_id(app.world(), name);
    let mut n = app.world_mut().get_mut::<Needs>(e).unwrap();
    let n = n.get_mut(id);
    n.value = value;
    n.level = level;
}

fn set_hunger(app: &mut App, e: Entity, value: i32, level: NeedLevel) {
    set_need(app, e, HUNGER, value, level);
}

fn set_energy(app: &mut App, e: Entity, value: i32, level: NeedLevel) {
    set_need(app, e, ENERGY, value, level);
}

/// `e`'s hunger and energy.
fn vneeds(app: &App, e: Entity) -> VillagerNeeds {
    VillagerNeeds::of(app.world(), app.world().get::<Needs>(e).unwrap()).unwrap()
}

fn aborts(app: &App, e: Entity) -> u32 {
    app.world().get::<BrainStats>(e).unwrap().aborts
}

fn activity(app: &App, e: Entity) -> Activity {
    *app.world().get::<Activity>(e).unwrap()
}

/// Runs one tick at a time until `pred` holds (checked after each tick).
fn run_until(app: &mut App, max_ticks: u64, mut pred: impl FnMut(&App) -> bool) -> bool {
    for _ in 0..max_ticks {
        run_ticks(app.world_mut(), 1);
        if pred(app) {
            return true;
        }
    }
    false
}

/// A second villager of the same house, standing 1 m east of the first.
fn spawn_second_villager(app: &mut App, ids: VillageIds) -> (SimId, Entity) {
    let tree = app.world().resource::<VillagerTree>().0;
    let start = SimPos::new(1000, 0);
    let needs = app.world().resource::<NeedRegistry>().full();
    spawn_sim(
        app.world_mut(),
        (
            Villager {
                home: ids.house,
                speed: 60,
            },
            start,
            PrevSimPos(start),
            needs,
            Activity::Idle,
            Brain { tree },
        ),
    )
}

// ---------------------------------------------------------------------------
// #1: a sleeping agent re-thinks periodically
// ---------------------------------------------------------------------------

/// Two villagers, one field, empty stockpile, night, both starving. The one
/// who loses the field goes to bed; once the other's shift puts food in the
/// stockpile, the re-think timer must wake him to eat before he starves.
/// (Hunger 25 000 leaves him time to walk to the stockpile once food exists;
/// without a re-think he sleeps until morning and starves at tick 2315.)
#[test]
fn second_villager_eats_once_food_exists() {
    let (mut app, ids) = village(1);
    let (_, b) = spawn_second_villager(&mut app, ids);
    let a = ent(&app, ids.villager);
    set_food(&mut app, ids, 0);
    set_tick(&mut app, 1690); // night (>= 1680)
    for e in [a, b] {
        set_hunger(&mut app, e, 25_000, NeedLevel::Urgent);
    }
    let mut first_food_tick = None;
    let mut b_ate = false;
    for _ in 0..700 {
        run_ticks(app.world_mut(), 1);
        let food = food(&app, ids);
        if food > 0 && first_food_tick.is_none() {
            first_food_tick = Some(tick(&app));
        }
        b_ate |= activity(&app, b) == Activity::Eat;
        let hb = vneeds(&app, b).hunger.value;
        assert!(
            hb > 0,
            "B starved at tick {} (food {food}, first food at {first_food_tick:?}); B path {:?}",
            tick(&app),
            path_of(&app, b)
        );
    }
    assert!(first_food_tick.is_some(), "A never produced food");
    assert!(b_ate, "B never ate; B path {:?}", path_of(&app, b));
}

// ---------------------------------------------------------------------------
// #2: Forage must not abort a running DayWork shift
// ---------------------------------------------------------------------------

#[test]
fn forage_does_not_restart_day_work_shift() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 0);
    set_tick(&mut app, 600);
    assert!(
        run_until(&mut app, 3000, |app| app
            .world()
            .get::<Work>(v)
            .is_some_and(|w| w.remaining == 20)),
        "never worked down to 20 ticks"
    );
    assert!(path_of(&app, v).contains("DayWork"), "{}", path_of(&app, v));
    let aborts_before = aborts(&app, v);
    // Just above urgent_enter: crosses it next tick (a ThinkRequest).
    set_hunger(&mut app, v, 20_010, NeedLevel::Low);
    run_ticks(app.world_mut(), 2);
    assert_eq!(vneeds(&app, v).hunger.level, NeedLevel::Urgent);
    let w = app.world().get::<Work>(v).copied();
    assert_eq!(
        w.map(|w| w.remaining),
        Some(18),
        "shift restarted; path {:?}",
        path_of(&app, v)
    );
    assert_eq!(aborts(&app, v), aborts_before);
    assert!(path_of(&app, v).contains("DayWork"), "{}", path_of(&app, v));
}

// ---------------------------------------------------------------------------
// #3: despawning an agent releases its reservations
// ---------------------------------------------------------------------------

#[test]
fn despawn_releases_reservations() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 0);
    set_tick(&mut app, 600);
    assert!(
        run_until(&mut app, 1000, |app| app
            .world()
            .resource::<Reservations>()
            .holds(ids.field, ids.villager)),
        "never reserved the field"
    );
    app.world_mut().despawn(v);
    run_ticks(app.world_mut(), 5);
    let res = app.world().resource::<Reservations>();
    assert!(res.claims.is_empty(), "leaked: {res:?}");
}

/// A `SimId` duplicated onto another entity must not release the claims of
/// the entity the index maps it to when the duplicate leaves.
#[test]
fn despawning_duplicate_sim_id_keeps_reservations() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 0);
    set_tick(&mut app, 600);
    assert!(run_until(&mut app, 1000, |app| app
        .world()
        .resource::<Reservations>()
        .holds(ids.field, ids.villager)));
    // Spawn a stray duplicate, then put the index back on the real villager.
    let dup = app.world_mut().spawn(ids.villager).id();
    app.world_mut()
        .resource_mut::<SimIndex>()
        .map
        .insert(ids.villager, v);
    app.world_mut().despawn(dup);
    assert!(
        app.world()
            .resource::<Reservations>()
            .holds(ids.field, ids.villager)
    );
}

// ---------------------------------------------------------------------------
// #4: UrgentSleep only fires when a bed is free
// ---------------------------------------------------------------------------

/// Two villagers share one bed. A sleeps (urgent) for a long time; B is
/// energy-urgent too but the bed is taken, so B's work must not be aborted.
#[test]
fn shared_bed_urgent_sleep_does_not_abort_work() {
    let (mut app, ids) = village(1);
    let (_, b) = spawn_second_villager(&mut app, ids);
    let a = ent(&app, ids.villager);
    set_tick(&mut app, 300);
    set_energy(&mut app, a, 1_000, NeedLevel::Urgent);
    set_energy(&mut app, b, 14_000, NeedLevel::Urgent);
    set_food(&mut app, ids, 0);
    let b_aborts = aborts(&app, b);
    let mut b_worked = false;
    let mut restarts = 0;
    let mut last_rem = None;
    for _ in 0..400 {
        run_ticks(app.world_mut(), 1);
        assert!(
            app.world()
                .resource::<Reservations>()
                .holds(ids.bed, ids.villager),
            "A lost the bed; A path {:?}",
            path_of(&app, a)
        );
        b_worked |= activity(&app, b) == Activity::Work;
        let rem = app.world().get::<Work>(b).map(|w| w.remaining);
        if let (Some(prev), Some(now)) = (last_rem, rem)
            && now > prev
        {
            restarts += 1;
        }
        last_rem = rem;
    }
    assert!(b_worked, "B never worked; B path {:?}", path_of(&app, b));
    assert_eq!(restarts, 0, "B's shift restarted");
    assert_eq!(aborts(&app, b), b_aborts, "B path {:?}", path_of(&app, b));
}

// ---------------------------------------------------------------------------
// #5: an interrupted meal restores nothing
// ---------------------------------------------------------------------------

#[test]
fn aborted_meal_restores_no_hunger_and_takes_no_food() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 4);
    set_tick(&mut app, 600);
    set_hunger(&mut app, v, 30_000, NeedLevel::Low);
    let mut hunger_at_meal_start = None;
    assert!(
        run_until(&mut app, 2000, |app| {
            let eat = app.world().get::<Eat>(v);
            if eat.is_some() && hunger_at_meal_start.is_none() {
                hunger_at_meal_start = Some(vneeds(app, v).hunger.value);
            }
            eat.is_some_and(|e| e.remaining == 5)
        }),
        "never ate"
    );
    let start = hunger_at_meal_start.unwrap();
    // Energy crosses urgent next tick: UrgentSleep aborts the meal.
    set_energy(&mut app, v, 15_020, NeedLevel::Low);
    run_ticks(app.world_mut(), 3);
    assert!(app.world().get::<Eat>(v).is_none(), "{}", path_of(&app, v));
    assert!(
        path_of(&app, v).contains("UrgentSleep"),
        "{}",
        path_of(&app, v)
    );
    assert_eq!(food(&app, ids), 4, "aborted meal consumed food");
    let hunger = vneeds(&app, v).hunger.value;
    assert!(
        hunger < start,
        "aborted meal restored hunger: {start} -> {hunger}"
    );
}

#[test]
fn completed_meal_restores_hunger_and_takes_food() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 4);
    set_tick(&mut app, 600);
    set_hunger(&mut app, v, 30_000, NeedLevel::Low);
    assert!(run_until(&mut app, 2000, |app| app
        .world()
        .get::<Eat>(v)
        .is_some_and(|e| e.remaining == 1)));
    let before = vneeds(&app, v).hunger.value;
    // Tick 1 completes the meal, tick 2 consumes the result.
    run_ticks(app.world_mut(), 2);
    assert!(app.world().get::<Eat>(v).is_none(), "{}", path_of(&app, v));
    let after = vneeds(&app, v).hunger.value;
    // Two ticks of decay, plus the whole meal (1000/tick × 60 ticks) at once.
    assert_eq!(after, before - 2 * 40 + 1000 * 60);
    assert_eq!(food(&app, ids), 3);
}

// ---------------------------------------------------------------------------
// #7: no new day-work shift starts at night
// ---------------------------------------------------------------------------

/// Nightfall while walking to the field: DoWork must not start at night
/// (only foraging works at night).
#[test]
fn no_new_day_work_shift_at_night() {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 0);
    set_tick(&mut app, 1640);
    let config = *app.world().resource::<SimConfig>();
    let mut walked_to_work = false;
    for _ in 0..400 {
        run_ticks(app.world_mut(), 1);
        let t = tick(&app) - 1;
        let p = path_of(&app, v);
        walked_to_work |= p.contains("DayWork > WorkSeq > MoveToDest");
        if config.is_night(t) {
            assert!(
                !(activity(&app, v) == Activity::Work && !p.contains("Forage")),
                "DoWork started at night (tick {t}); path {p:?}"
            );
        }
    }
    assert!(walked_to_work, "never walked to work");
}

// ---------------------------------------------------------------------------
// #8: SleepInBed fails when the bed is gone or provides no energy
// ---------------------------------------------------------------------------

fn sleeping_villager() -> (App, VillageIds, Entity) {
    let (mut app, ids) = village(1);
    let v = ent(&app, ids.villager);
    set_food(&mut app, ids, 10);
    set_tick(&mut app, 300);
    set_energy(&mut app, v, 10_000, NeedLevel::Urgent);
    assert!(
        run_until(&mut app, 1000, |app| app.world().get::<Sleep>(v).is_some()),
        "never slept"
    );
    run_ticks(app.world_mut(), 5);
    assert!(app.world().get::<Sleep>(v).is_some());
    (app, ids, v)
}

#[test]
fn sleep_fails_when_bed_loses_provides() {
    let (mut app, ids, v) = sleeping_villager();
    let bed = ent(&app, ids.bed);
    app.world_mut().entity_mut(bed).remove::<Provides>();
    run_ticks(app.world_mut(), 3);
    assert!(
        app.world().get::<Sleep>(v).is_none(),
        "{}",
        path_of(&app, v)
    );
    assert_ne!(activity(&app, v), Activity::Sleep);
    assert!(
        !app.world()
            .resource::<Reservations>()
            .holds(ids.bed, ids.villager)
    );
}

#[test]
fn sleep_fails_when_bed_is_despawned() {
    let (mut app, ids, v) = sleeping_villager();
    let bed = ent(&app, ids.bed);
    app.world_mut().despawn(bed);
    run_ticks(app.world_mut(), 3);
    assert!(
        app.world().get::<Sleep>(v).is_none(),
        "{}",
        path_of(&app, v)
    );
    assert_ne!(activity(&app, v), Activity::Sleep);
}
