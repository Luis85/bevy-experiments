//! Task D: villager scenarios (actions, conditions, tree, scenario layout).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::ai::{AiRegistry, BrainState, BrainStats};
use npc_framework::prelude::{
    NeedLevel, NeedRegistry, Needs, NpcPlugins, Reservations, SimClock, SimConfig, SimId, SimIndex,
    run_ticks,
};
use village_content::needs::{ENERGY, HUNGER, VillagerNeeds, need_id};
use village_content::scenario::{VillageIds, spawn_village};
use village_content::villager::VillagerTree;
use village_content::{Activity, Stockpile, VillagePlugin};

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

/// Runs one tick at a time until `pred` holds (checked after each tick).
/// Returns the number of ticks run, or `None` when `max_ticks` ran out.
fn run_until(app: &mut App, max_ticks: u64, mut pred: impl FnMut(&mut App) -> bool) -> Option<u64> {
    for i in 1..=max_ticks {
        run_ticks(app.world_mut(), 1);
        if pred(app) {
            return Some(i);
        }
    }
    None
}

/// The villager's running tree path, as shown by the overlay.
fn path(app: &App) -> String {
    let ids = *app.world().resource::<VillageIds>();
    let state = app
        .world()
        .get::<BrainState>(ent(app, ids.villager))
        .expect("villager has BrainState");
    let tree = app.world().resource::<VillagerTree>().0;
    app.world()
        .resource::<AiRegistry>()
        .tree(tree)
        .path_string(&state.running_path)
}

fn activity(app: &App) -> Activity {
    let ids = *app.world().resource::<VillageIds>();
    *app.world().get::<Activity>(ent(app, ids.villager)).unwrap()
}

fn needs(app: &App) -> VillagerNeeds {
    let ids = *app.world().resource::<VillageIds>();
    let n = app.world().get::<Needs>(ent(app, ids.villager)).unwrap();
    VillagerNeeds::of(app.world(), n).unwrap()
}

fn set_need_value(app: &mut App, name: &str, value: i32) {
    let ids = *app.world().resource::<VillageIds>();
    let e = ent(app, ids.villager);
    let id = need_id(app.world(), name);
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(id)
        .value = value;
}

fn stats(app: &App) -> BrainStats {
    let ids = *app.world().resource::<VillageIds>();
    *app.world()
        .get::<BrainStats>(ent(app, ids.villager))
        .unwrap()
}

fn food(app: &App) -> u32 {
    let ids = *app.world().resource::<VillageIds>();
    app.world()
        .get::<Stockpile>(ent(app, ids.stockpile))
        .unwrap()
        .food
}

fn set_food(app: &mut App, food: u32) {
    let ids = *app.world().resource::<VillageIds>();
    let e = ent(app, ids.stockpile);
    app.world_mut().get_mut::<Stockpile>(e).unwrap().food = food;
}

fn set_hunger(app: &mut App, value: i32) {
    set_need_value(app, HUNGER, value);
}

fn set_tick(app: &mut App, tick: u64) {
    app.world_mut().insert_resource(SimClock { tick });
}

fn field_held_by_villager(app: &App) -> bool {
    let ids = *app.world().resource::<VillageIds>();
    app.world()
        .resource::<Reservations>()
        .holds(ids.field, ids.villager)
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

#[test]
fn spawn_village_layout() {
    let (app, ids) = village(1);
    assert_eq!(
        [ids.house, ids.bed, ids.stockpile, ids.field, ids.villager],
        [SimId(1), SimId(2), SimId(3), SimId(4), SimId(5)]
    );
    assert_eq!(food(&app), 4);
    assert_eq!(activity(&app), Activity::Idle);
    let full = app.world().resource::<NeedRegistry>().full();
    assert_eq!(needs(&app), VillagerNeeds::of(app.world(), &full).unwrap());
}

#[test]
fn works_then_eats_when_starving_and_empty() {
    let (mut app, _) = village(1);
    set_food(&mut app, 0);
    set_hunger(&mut app, 15_000);
    // 0 = waiting for Work, 1 = waiting for food, 2 = waiting for Eat, 3 = done.
    let mut phase = 0;
    let done = run_until(&mut app, 1200, |app| {
        match phase {
            0 if activity(app) == Activity::Work => phase = 1,
            1 if food(app) > 0 => phase = 2,
            2 if activity(app) == Activity::Eat => phase = 3,
            _ => {}
        }
        phase == 3
    });
    assert!(
        done.is_some(),
        "stuck in phase {phase}; path {:?}",
        path(&app)
    );
}

#[test]
fn urgent_hunger_interrupts_wander() {
    let (mut app, _) = village(1);
    set_food(&mut app, 10);
    set_tick(&mut app, 600);
    run_until(&mut app, 1000, |app| path(app).contains("Wander")).expect("wanders");
    let aborts = stats(&app).aborts;
    set_hunger(&mut app, 10_000);
    run_until(&mut app, 3, |app| {
        path(app).starts_with("Villager > UrgentEat")
    })
    .unwrap_or_else(|| panic!("no UrgentEat; path {:?}", path(&app)));
    assert!(stats(&app).aborts > aborts);
}

#[test]
fn urgent_hunger_interrupts_work() {
    let (mut app, _) = village(1);
    set_food(&mut app, 0);
    run_until(&mut app, 1000, |app| {
        path(app).contains("DayWork > WorkSeq > DoWork")
    })
    .expect("works");
    set_food(&mut app, 5);
    set_hunger(&mut app, 10_000);
    run_until(&mut app, 3, |app| path(app).contains("UrgentEat"))
        .unwrap_or_else(|| panic!("no UrgentEat; path {:?}", path(&app)));
}

#[test]
fn sleeps_at_night_and_wakes_in_morning() {
    let (mut app, _) = village(1);
    set_food(&mut app, 10);
    run_ticks(app.world_mut(), 1680 + 400);
    assert_eq!(activity(&app), Activity::Sleep, "path {:?}", path(&app));
    assert!(path(&app).contains("NightSleep"), "path {:?}", path(&app));
    run_ticks(app.world_mut(), 2400 + 300 - (1680 + 400));
    assert!(!path(&app).contains("Sleep"), "path {:?}", path(&app));
    assert_ne!(activity(&app), Activity::Sleep);
}

#[test]
fn full_stockpile_means_free_time() {
    let (mut app, _) = village(1);
    set_food(&mut app, 10);
    set_tick(&mut app, 600);
    let mut worked = false;
    let mut wandered = false;
    for _ in 0..600 {
        run_ticks(app.world_mut(), 1);
        worked |= activity(&app) == Activity::Work;
        wandered |= path(&app).contains("Wander");
    }
    assert!(!worked, "worked with a full stockpile");
    assert!(wandered, "never wandered");
}

#[test]
fn reservations_released_on_abort() {
    let (mut app, _) = village(1);
    set_food(&mut app, 0);
    set_tick(&mut app, 600);
    run_until(&mut app, 1000, |app| {
        path(app).contains("WorkSeq > MoveToDest") && field_held_by_villager(app)
    })
    .expect("walks to a reserved field");
    set_food(&mut app, 5);
    set_hunger(&mut app, 10_000);
    run_until(&mut app, 3, |app| !field_held_by_villager(app))
        .unwrap_or_else(|| panic!("field still reserved; path {:?}", path(&app)));
    // Only the food unit he is now going for may be held.
    let ids = *app.world().resource::<VillageIds>();
    let res = app.world().resource::<Reservations>();
    for (target, claimants) in &res.claims {
        if claimants.contains(&ids.villager) {
            assert_eq!(*target, ids.stockpile, "unexpected claim {res:?}");
        }
    }
}

/// Review focus 1, literally: food reserved, walking, then urgent sleep fires.
#[test]
fn food_reservation_released_when_urgent_sleep_aborts_walk() {
    let (mut app, ids) = village(1);
    set_food(&mut app, 10);
    set_tick(&mut app, 600);
    set_hunger(&mut app, 45_000);
    run_until(&mut app, 1000, |app| {
        path(app).contains("CasualEat > EatSeq > MoveToDest")
            && app
                .world()
                .resource::<Reservations>()
                .holds(ids.stockpile, ids.villager)
    })
    .expect("walks to the stockpile with food reserved");
    set_need_value(&mut app, ENERGY, 10_000);
    run_until(&mut app, 3, |app| {
        path(app).starts_with("Villager > UrgentSleep")
    })
    .unwrap_or_else(|| panic!("no UrgentSleep; path {:?}", path(&app)));
    let res = app.world().resource::<Reservations>();
    assert!(!res.holds(ids.stockpile, ids.villager), "{res:?}");
    assert!(res.holds(ids.bed, ids.villager), "{res:?}");
}

#[test]
fn forages_at_night_when_starving() {
    let (mut app, _) = village(1);
    set_tick(&mut app, 1700);
    assert!(app.world().resource::<SimConfig>().is_night(1700));
    set_food(&mut app, 0);
    set_hunger(&mut app, 15_000);
    let mut foraged = false;
    let mut worked = false;
    run_until(&mut app, 600, |app| {
        foraged |= path(app).contains("Forage");
        worked |= activity(app) == Activity::Work;
        foraged && worked
    })
    .unwrap_or_else(|| panic!("foraged {foraged} worked {worked}; path {:?}", path(&app)));
}

#[test]
fn never_starves_over_ten_days() {
    let (mut app, _) = village(1);
    let mut min_hunger = i32::MAX;
    let mut min_energy = i32::MAX;
    for _ in 0..24_000 {
        run_ticks(app.world_mut(), 1);
        let n = needs(&app);
        min_hunger = min_hunger.min(n.hunger.value);
        min_energy = min_energy.min(n.energy.value);
    }
    assert!(min_hunger > 0, "min hunger {min_hunger}");
    assert!(min_energy > 0, "min energy {min_energy}");
}

#[test]
fn no_flip_flop() {
    let (mut app, _) = village(1);
    run_ticks(app.world_mut(), 24_000);
    let s = stats(&app);
    assert!(s.aborts / 10 <= 6, "{s:?}");
    assert!(s.leaf_starts / 10 <= 250, "{s:?}");
}

// ---------------------------------------------------------------------------
// Diagnostic: `cargo test -p village_content --test villager -- --ignored --nocapture`
// ---------------------------------------------------------------------------

/// Prints every change of (top-level branch, activity) over the first 3 days,
/// plus a per-day summary for 10 days, so the rhythm can be checked by eye.
#[test]
#[ignore = "diagnostic output only"]
fn print_three_day_timeline() {
    let (mut app, _) = village(1);
    let config = *app.world().resource::<SimConfig>();
    let mut last: Option<(String, Activity)> = None;
    // Per day: (meals, work shifts, ticks asleep, min hunger, min energy,
    // drinks, min thirst).
    let mut days = [(0u32, 0u32, 0u32, i32::MAX, i32::MAX, 0u32, i32::MAX); 10];
    for _ in 0..config.day_length * 10 {
        let tick = app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), 1);
        let p = path(&app);
        let branch = p.split(" > ").nth(1).unwrap_or("-").to_owned();
        let act = activity(&app);
        let n = needs(&app);
        let day = &mut days[config.day(tick) as usize];
        if act == Activity::Sleep {
            day.2 += 1;
        }
        day.3 = day.3.min(n.hunger.value);
        day.4 = day.4.min(n.energy.value);
        day.6 = day.6.min(n.thirst.value);
        let key = (branch.clone(), act);
        if last.as_ref() != Some(&key) {
            if last.as_ref().map(|l| l.1) != Some(act) {
                match act {
                    Activity::Eat => day.0 += 1,
                    Activity::Work => day.1 += 1,
                    Activity::Drink => day.5 += 1,
                    _ => {}
                }
            }
            last = Some(key);
            if config.day(tick) >= 3 {
                continue;
            }
            let minutes = ((tick % config.day_length) * 1440 / config.day_length + 360) % 1440;
            let night = if config.is_night(tick) {
                "night"
            } else {
                "     "
            };
            let lvl = |l: NeedLevel| match l {
                NeedLevel::Ok => "ok",
                NeedLevel::Low => "LOW",
                NeedLevel::Urgent => "URG",
            };
            println!(
                "day {} {:02}:{:02} {night} t{tick:>5}  {:<11} {:<6} hunger {:>3}% {:<3} energy {:>3}% {:<3} thirst {:>3}% {:<3} food {:>2}  [{p}]",
                config.day(tick) + 1,
                minutes / 60,
                minutes % 60,
                branch,
                format!("{act:?}"),
                n.hunger.value / 1000,
                lvl(n.hunger.level),
                n.energy.value / 1000,
                lvl(n.energy.level),
                n.thirst.value / 1000,
                lvl(n.thirst.level),
                food(&app),
            );
        }
    }
    for (i, (meals, shifts, asleep, minh, mine, drinks, mint)) in days.iter().enumerate() {
        println!(
            "SUMMARY day {}: meals {meals}, drinks {drinks}, work shifts {shifts}, ticks asleep {asleep}, min hunger {}%, min energy {}%, min thirst {}%",
            i + 1,
            minh / 1000,
            mine / 1000,
            mint / 1000
        );
    }
    println!("stats after 10 days: {:?}", stats(&app));
}
