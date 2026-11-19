//! Every resident keeps its own day (review npc-ai#11): the world's blocks
//! after its chronotype, diligence, trade (`economy.professions[].day`) and
//! age (`behavior.schedule.elder`), derived once and saved. The tree's
//! guards, meals, the fits-before-rest rule and the view read it; meals and
//! the shared leisure stay within `align_permille` of the world's.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, Villager,
    demography::{self, LifeStage},
    resident::{self, BlockKind, OwnSchedule, ScheduleBlock},
};
use life_support::*;
use npc_framework::ai::AiRegistry;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn inside(block: &ScheduleBlock, at: i32) -> bool {
    if block.from <= block.to {
        block.from <= at && at < block.to
    } else {
        at >= block.from || at < block.to
    }
}

fn covers(blocks: &[ScheduleBlock], kind: BlockKind, at: i32) -> bool {
    blocks
        .iter()
        .any(|block| block.kind == kind && inside(block, at))
}

fn own(world: &World, actor: Entity) -> Vec<ScheduleBlock> {
    world
        .get::<OwnSchedule>(actor)
        .expect("every resident keeps its own day")
        .blocks
        .clone()
}

fn by_id(app: &bevy_app::App) -> BTreeMap<SimId, Vec<ScheduleBlock>> {
    all_residents(app.world())
        .into_iter()
        .map(|(id, actor)| (id, own(app.world(), actor)))
        .collect()
}

fn role(world: &World, actor: Entity) -> Role {
    world.get::<Villager>(actor).expect("resident").role
}

fn elder(world: &World, actor: Entity) -> bool {
    demography::stage(world, actor) == LifeStage::Elder
}

fn shift(world: &World, actor: Entity) -> i32 {
    resident::traits(world, actor).chronotype
        * resident::spec(world)
            .expect("data")
            .schedule
            .chronotype_shift
        / 100
}

fn work_from(blocks: &[ScheduleBlock]) -> i32 {
    blocks
        .iter()
        .find(|block| block.kind == BlockKind::Work)
        .expect("work")
        .from
}

fn condition(world: &World, actor: Entity, name: &str) -> bool {
    let registry = world.resource::<AiRegistry>();
    let (_, test) = registry
        .conds
        .iter()
        .find(|(cond, _)| *cond == name)
        .expect("registered condition");
    test(world, actor)
}

/// Moves the clock to `permille` of the current day (a pure query follows).
fn at_permille(app: &mut bevy_app::App, permille: i32) {
    let day = app.world().resource::<SimConfig>().day_length;
    let start = app.world().resource::<SimClock>().tick / day * day;
    app.world_mut().resource_mut::<SimClock>().tick =
        start + permille.rem_euclid(1000) as u64 * day / 1000;
}

#[test]
fn schedules_differ_per_resident_and_are_the_same_for_the_same_seed() {
    let first = world_with(|_| {});
    let again = world_with(|_| {});
    let other = world_for(11, |_| {});
    let days = by_id(&first);
    assert_eq!(days, by_id(&again), "the same seed gives the same days");
    let distinct: BTreeSet<String> = days.values().map(|day| format!("{day:?}")).collect();
    println!(
        "OWN_DAYS residents={} distinct={}",
        days.len(),
        distinct.len()
    );
    assert!(
        distinct.len() * 2 >= days.len(),
        "only {} different days among {} residents",
        distinct.len(),
        days.len()
    );
    assert_ne!(days, by_id(&other), "another seed, other days");
    // Traits and trade alone decide the day: the stored blocks are the rule.
    let world = first.world();
    for (_, actor) in all_residents(world) {
        assert_eq!(Some(own(world, actor)), resident::day::derive(world, actor));
    }
}

#[test]
fn a_trade_shapes_the_day_from_its_data() {
    // The shipped data: bakers start work and go to bed 40 permille early
    // (edges only: their evening stays), fishers' whole day is 30 earlier,
    // the innkeeper's 60 later with its own afternoon off; other trades keep
    // the world's day moved by their hours.
    let app = world_with(|_| {});
    let world = app.world();
    let spec = resident::spec(world).expect("data");
    let mut seen = BTreeSet::new();
    for (_, actor) in all_residents(world) {
        let blocks = own(world, actor);
        let base = shift(world, actor) + if elder(world, actor) { -20 } else { 0 };
        // (whole-day shift, extra move of the work start, of the bedtime)
        let (trade, early, bed) = match role(world, actor) {
            Role::Baker => (0, -40, -40),
            Role::Fisher => (-30, 0, 0),
            Role::Innkeeper => (60, 0, 0),
            _ => (0, 0, 0),
        };
        seen.insert((trade, early));
        assert_eq!(
            work_from(&blocks),
            (base + trade + early).rem_euclid(1000),
            "{:?}",
            role(world, actor)
        );
        let sleep = blocks
            .iter()
            .find(|block| block.kind == BlockKind::Sleep)
            .expect("sleep");
        assert_eq!(sleep.from, (700 + base + trade + bed).rem_euclid(1000));
        let keeper = role(world, actor) == Role::Innkeeper;
        // The innkeeper's free time is its own afternoon, not the evening.
        assert_eq!(
            covers(&blocks, BlockKind::Leisure, 660),
            !keeper && {
                let delay = (resident::traits(world, actor).diligence - 50)
                    * spec.schedule.diligence_shift
                    / 50;
                let moved = (base + trade).clamp(-40, 40);
                660 >= 620 + delay + moved && 660 < 700 + moved
            }
        );
        if keeper {
            assert!(covers(
                &blocks,
                BlockKind::Leisure,
                (360 + base + trade).rem_euclid(1000)
            ));
            assert!(
                covers(&blocks, BlockKind::Work, 740),
                "works into the evening"
            );
        }
    }
    assert!(seen.len() >= 3, "the founders hold several shaped trades");

    // Any trade can be given a shape as data, without code naming it.
    let shaped = world_with(|world| {
        let professions = economy(world)["professions"]
            .as_array_mut()
            .expect("trades");
        let farmer = professions
            .iter_mut()
            .find(|entry| entry["role"] == "farmer")
            .expect("farmer");
        farmer["day"] = json!({"shift": 100, "edges": [{"kind": "work", "to": -100}]});
    });
    let world = shaped.world();
    let mut farmers = 0;
    for (_, actor) in all_residents(world) {
        if role(world, actor) != Role::Farmer || elder(world, actor) {
            continue;
        }
        farmers += 1;
        let blocks = own(world, actor);
        let work = blocks
            .iter()
            .find(|block| block.kind == BlockKind::Work)
            .expect("work");
        let from = (100 + shift(world, actor)).rem_euclid(1000);
        assert_eq!((work.from, work.to), (from, (from + 600).rem_euclid(1000)));
        // Meals stay within the alignment of the world's meals.
        let meal = blocks
            .iter()
            .find(|block| block.kind == BlockKind::Meal)
            .expect("meal");
        assert_eq!(meal.from, 250 + (100 + shift(world, actor)).clamp(-40, 40));
    }
    assert!(farmers > 0);
}

fn load(edit: impl FnOnce(&mut Value)) -> Result<(), String> {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    edit(&mut doc.world["construction"]["world"]["preset"]);
    island_content::scenario::load_scenario(&doc, 7)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[test]
fn a_day_shape_cannot_move_the_shared_meals() {
    let error = load(|world| {
        let professions = world["living_world"]["economy"]["professions"]
            .as_array_mut()
            .expect("trades");
        professions[0]["day"] = json!({"edges": [{"kind": "meal", "from": 100}]});
    })
    .expect_err("a meal edge is refused");
    assert!(error.contains("meals are shared"), "{error}");
    let error = load(|world| {
        world["living_world"]["behavior"] = json!({"schedule": {"elder": {"shift": 500}}});
    })
    .expect_err("a long shift is refused");
    assert!(error.contains("behavior.schedule.elder"), "{error}");
}

#[test]
fn guards_meals_and_the_work_rule_follow_the_residents_own_blocks() {
    let mut app = world_with(|_| {});
    let people = all_residents(app.world());
    let (_, keeper) = *people
        .iter()
        .find(|(_, actor)| role(app.world(), *actor) == Role::Innkeeper)
        .expect("an innkeeper");
    let (_, farmer) = *people
        .iter()
        .find(|(_, actor)| role(app.world(), *actor) == Role::Farmer && !elder(app.world(), *actor))
        .expect("a farmer");
    let leisure = own(app.world(), keeper)
        .into_iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("the keeper's afternoon");
    // In the keeper's afternoon off: the keeper's leisure guard holds, the
    // farmer's does not; the farmer is in its work window.
    at_permille(&mut app, leisure.from + 5);
    let world = app.world();
    assert!(condition(world, keeper, "life.in_leisure_window"));
    assert!(!condition(world, farmer, "life.in_leisure_window"));
    assert!(condition(world, farmer, "life.in_work_window"));
    // The farmer's evening: the keeper works on.
    let evening = own(world, farmer)
        .into_iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("the farmer's evening");
    at_permille(&mut app, evening.from + 2);
    let world = app.world();
    assert!(condition(world, farmer, "life.in_leisure_window"));
    assert!(!condition(world, keeper, "life.in_leisure_window"));
    assert!(condition(world, keeper, "life.in_work_window"));
    // Meals: each resident's own meal block, which stays near the world's.
    for actor in [keeper, farmer] {
        let meal = own(app.world(), actor)
            .into_iter()
            .find(|block| block.kind == BlockKind::Meal)
            .expect("meal");
        at_permille(&mut app, meal.from);
        assert!(resident::meal_block(app.world(), actor).is_some());
        assert!(condition(app.world(), actor, "life.in_meal_window"));
        at_permille(&mut app, meal.from - 1);
        assert!(resident::meal_block(app.world(), actor).is_none());
    }
    // Fits before rest: a work leaf of the keeper just before its afternoon
    // off starts no job that runs into it; the farmer, still at work then,
    // may start the same job.
    let day = app.world().resource::<SimConfig>().day_length;
    at_permille(&mut app, leisure.from - 10);
    let job = 20 * day / 1000;
    for (actor, fits) in [(keeper, false), (farmer, true)] {
        app.world_mut().resource_mut::<resident::WorkStart>().0 = Some(actor);
        assert_eq!(resident::fits_before_rest(app.world(), actor, job), fits);
    }
    app.world_mut().resource_mut::<resident::WorkStart>().0 = None;
    // The view's day strip reads the same blocks.
    let blocks: Vec<_> = village::view::schedule_blocks(app.world(), keeper)
        .into_iter()
        .map(|block| (block.kind, block.from_permille, block.to_permille))
        .collect();
    let stored: Vec<_> = own(app.world(), keeper)
        .into_iter()
        .map(|block| (block.kind, block.from, block.to))
        .collect();
    assert_eq!(blocks, stored);
}

#[test]
fn an_elder_gets_the_elders_day_when_it_grows_old() {
    let mut app = world_with(|_| {});
    let (_, actor) = *all_residents(app.world())
        .iter()
        .find(|(_, actor)| !elder(app.world(), *actor) && role(app.world(), *actor) == Role::Farmer)
        .expect("an adult farmer");
    let before = own(app.world(), actor);
    app.world_mut()
        .get_mut::<demography::Life>(actor)
        .expect("life")
        .stage = LifeStage::Elder;
    resident::assign_tree(app.world_mut(), actor);
    let after = own(app.world(), actor);
    let work = |blocks: &[ScheduleBlock]| {
        let block = blocks
            .iter()
            .find(|block| block.kind == BlockKind::Work)
            .expect("work");
        (block.to - block.from).rem_euclid(1000)
    };
    assert_eq!(
        work_from(&after),
        (work_from(&before) - 20).rem_euclid(1000)
    );
    assert_eq!(work(&after), work(&before) - 60, "elders stop work earlier");
}

#[test]
fn meals_and_free_time_stay_shared_and_every_trade_keeps_its_hours() {
    for seed in [7, 11, 23] {
        let app = world_for(seed, |_| {});
        let world = app.world();
        let spec = resident::spec(world).expect("data");
        let shared: Vec<ScheduleBlock> = spec.schedule.blocks.clone();
        let world_work = (0..1000)
            .filter(|at| covers(&shared, BlockKind::Work, *at))
            .count();
        let mut free_at_dusk = 0;
        let mut keeping_the_evening = 0;
        let mut least = i32::MAX;
        let mut coverage: BTreeMap<Role, BTreeSet<i32>> = BTreeMap::new();
        for (id, actor) in all_residents(world) {
            let blocks = own(world, actor);
            // Every meal overlaps one of the world's meals.
            for meal in blocks.iter().filter(|block| block.kind == BlockKind::Meal) {
                assert!(
                    (0..1000).any(|at| inside(meal, at) && covers(&shared, BlockKind::Meal, at)),
                    "seed {seed} resident {}: {meal:?}",
                    id.0
                );
            }
            let own_leisure = role(world, actor) == Role::Innkeeper;
            if !own_leisure {
                keeping_the_evening += 1;
                // Free time awake, at the same time as the world's free time.
                let overlap = (0..1000)
                    .filter(|at| {
                        covers(&blocks, BlockKind::Leisure, *at)
                            && !covers(&blocks, BlockKind::Sleep, *at)
                            && covers(&shared, BlockKind::Leisure, *at)
                    })
                    .count() as i32;
                least = least.min(overlap);
                assert!(
                    overlap > 0,
                    "seed {seed} resident {} has no shared evening",
                    id.0
                );
                free_at_dusk += usize::from(
                    covers(&blocks, BlockKind::Leisure, 660)
                        && !covers(&blocks, BlockKind::Sleep, 660),
                );
            }
            let hours = coverage.entry(role(world, actor)).or_default();
            hours.extend((0..1000).filter(|at| covers(&blocks, BlockKind::Work, *at)));
        }
        println!(
            "OVERLAP seed={seed} free_at_dusk={free_at_dusk}/{keeping_the_evening} \
             least_shared_evening_permille={least}"
        );
        assert!(
            free_at_dusk * 10 >= keeping_the_evening * 7,
            "seed {seed}: only {free_at_dusk} of {keeping_the_evening} are free at dusk"
        );
        for (trade, hours) in coverage {
            assert!(
                hours.len() + 60 >= world_work,
                "seed {seed}: {trade:?} covers {} of {world_work} working permille",
                hours.len()
            );
        }
    }
}

#[test]
fn a_saved_world_keeps_every_residents_day() {
    let mut app = world_with(|world| short_days(world, 2_400));
    run_ticks(app.world_mut(), 1_200);
    let days = by_id(&app);
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut fresh = world_with(|world| short_days(world, 2_400));
    snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert_eq!(by_id(&fresh), days);
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    run_ticks(app.world_mut(), 3_000);
    run_ticks(fresh.world_mut(), 3_000);
    assert_eq!(
        state_hash(fresh.world_mut()),
        state_hash(app.world_mut()),
        "the continued world runs on as the original"
    );
    assert_eq!(by_id(&fresh), by_id(&app));
}

#[test]
fn two_runs_of_a_seed_with_own_days_stay_equal() {
    let mut first = world_with(|world| short_days(world, 2_400));
    let mut second = world_with(|world| short_days(world, 2_400));
    for _ in 0..4 {
        run_ticks(first.world_mut(), 1_500);
        run_ticks(second.world_mut(), 1_500);
        assert_eq!(
            state_hash(first.world_mut()),
            state_hash(second.world_mut())
        );
    }
}
