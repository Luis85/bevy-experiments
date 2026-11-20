//! Building upkeep and fire (`village::civic::upkeep`): buildings wear by
//! their kind's daily rate, work worse when worn and not at all at 0, are
//! repaired through the ordinary civic site path, and may burn; a fire is
//! seeded, spreads to near doors and does less damage when residents fight
//! it. Saves continue mid-fire with the same hash.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, civic,
    civic::upkeep::{self, Upkeep, UpkeepState},
    frontier::settlements,
    integrity,
};
use life_support::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};

const DAY: u64 = 2_400;

/// Short clear days; fires only when a test starts one.
fn quiet(world: &mut Value) {
    short_days(world, DAY);
    only_weather(world, "clear");
    for building in economy(world)["buildings"]
        .as_array_mut()
        .expect("buildings")
    {
        if let Some(upkeep) = building.get_mut("upkeep").and_then(Value::as_object_mut) {
            upkeep.remove("fire");
        }
    }
}

fn app(edit: impl FnOnce(&mut Value)) -> App {
    let mut app = world_with(|world| {
        quiet(world);
        edit(world);
    });
    set_journal(app.world_mut(), true, false);
    // The first tick lays out the founding fields and the trading post and
    // runs the first daily pass.
    run_ticks(app.world_mut(), 2);
    app
}

fn home(world: &World, key: &str) -> SimId {
    let harbour = settlement(world, "harbour");
    id_of(
        world,
        settlements::home_entity(world, harbour, key).expect("a founding home"),
    )
}

fn workplace(world: &World, role: Role) -> SimId {
    settlements::workplace(world, settlement(world, "harbour"), role).expect("a workplace")
}

fn condition(world: &World, id: SimId) -> u32 {
    world
        .get::<Upkeep>(entity(world, id))
        .expect("upkeep")
        .condition
}

#[test]
fn every_kind_wears_by_its_own_rate_each_day_and_faster_in_a_storm() {
    let mut app = app(|_| {});
    let world = app.world();
    let cottage = home(world, "home_00");
    let bakery = workplace(world, Role::Baker);
    let well = workplace(world, Role::WaterKeeper);
    let founding: Vec<u32> = [cottage, bakery, well]
        .iter()
        .map(|id| condition(world, *id))
        .collect();
    // Founding buildings start at a seeded condition, 600 to 1000.
    assert!(
        founding.iter().all(|c| (600..=1000).contains(c)),
        "{founding:?}"
    );
    run_ticks(app.world_mut(), DAY);
    let world = app.world();
    let wear: Vec<u32> = [cottage, bakery, well]
        .iter()
        .zip(&founding)
        .map(|(id, before)| before - condition(world, *id))
        .collect();
    assert_eq!(wear, [15, 20, 10], "a cottage, a bakery and a well");

    let mut stormy = world_with(|world| {
        quiet(world);
        only_weather(world, "storm");
    });
    run_ticks(stormy.world_mut(), 2);
    let before = condition(stormy.world(), cottage);
    run_ticks(stormy.world_mut(), DAY);
    assert_eq!(
        before - condition(stormy.world(), cottage),
        22,
        "150% of 15"
    );
}

#[test]
fn a_worn_building_works_worse_and_a_broken_one_not_at_all() {
    let mut app = app(|_| {});
    let bakery = workplace(app.world(), Role::Baker);
    let cottage = home(app.world(), "home_01");
    assert_eq!(upkeep::work_percent(app.world(), bakery), 100);
    upkeep::set_condition_of(app.world_mut(), bakery, 250).expect("set");
    assert_eq!(upkeep::work_percent(app.world(), bakery), 125);
    assert!(upkeep::usable_id(app.world(), bakery));
    upkeep::set_condition_of(app.world_mut(), bakery, 0).expect("set");
    assert!(!upkeep::usable_id(app.world(), bakery));
    assert_eq!(effects(app.world(), "VillageBuildingUnusable").len(), 1);

    let home = entity(app.world(), cottage);
    assert_eq!(upkeep::shelter_percent(app.world(), home), 100);
    upkeep::set_condition_of(app.world_mut(), cottage, 100).expect("set");
    assert_eq!(upkeep::shelter_percent(app.world(), home), 60);
    upkeep::set_condition_of(app.world_mut(), cottage, 0).expect("set");
    assert_eq!(upkeep::shelter_percent(app.world(), home), 0);

    // A store takes new goods only up to 75% of its room when worn, none
    // when broken; what lies there stays.
    let (post, store) = civic::all_depots(app.world())
        .into_iter()
        .find(|(_, store)| app.world().get::<Upkeep>(*store).is_some())
        .expect("a store that wears: the trading post");
    let capacity = app
        .world()
        .get::<civic::Depot>(store)
        .expect("store")
        .capacity;
    upkeep::set_condition_of(app.world_mut(), post, 200).expect("set");
    let depot = app.world().get::<civic::Depot>(store).expect("store");
    assert_eq!(depot.withheld, capacity - capacity * 75 / 100);
    upkeep::set_condition_of(app.world_mut(), post, 0).expect("set");
    let depot = app.world().get::<civic::Depot>(store).expect("store");
    assert_eq!(depot.room(None, 0), 0);
    upkeep::set_condition_of(app.world_mut(), post, 900).expect("set");
    assert_eq!(
        app.world()
            .get::<civic::Depot>(store)
            .expect("store")
            .withheld,
        0
    );
}

#[test]
fn a_worn_home_is_repaired_through_a_paid_civic_site() {
    let mut app = app(|_| {});
    let cottage = home(app.world(), "home_02");
    upkeep::set_condition_of(app.world_mut(), cottage, 300).expect("set");
    fund(app.world_mut(), 300);
    let coins = money(app.world());
    // The repair site as last seen open: the journal keeps only its latest
    // entries, so the site is watched while it stands.
    let mut seen: Option<civic::SiteView> = None;
    let done = until(&mut app, 12 * DAY, 50, |app| {
        if let Some(site) = civic::sites(app.world())
            .into_iter()
            .find(|site| site.repairs == Some(cottage))
        {
            seen = Some(site);
        }
        upkeep::view(app.world(), cottage).is_some_and(|view| view.condition >= 990)
    });
    let site = seen.expect("a repair site was opened for the cottage");
    assert!(done, "the repair was never finished");
    assert!(site.civic, "the village's own site");
    assert_eq!(site.kind, "repair");
    // 40% of 8 planks and 4 logs, times the damage of 700 permille.
    let cost: Vec<(String, u32)> = site.cost.clone().into_iter().collect();
    assert_eq!(cost, [("log".to_owned(), 2), ("plank".to_owned(), 3)]);
    assert_eq!(
        site.delivered, site.cost,
        "the materials were carried to the door"
    );
    assert!(site.paid > 0, "the civic fund paid for them");
    let world = app.world();
    assert!(
        world.resource::<SimIndex>().get(site.id).is_none(),
        "the site is gone"
    );
    assert_eq!(world.resource::<UpkeepState>().repairs, 1);
    let repaired = effects(world, "VillageBuildingRepaired");
    assert_eq!(repaired.len(), 1);
    assert_eq!(repaired[0]["building"], json!(cottage.0));
    assert_eq!(repaired[0]["paid"], json!(site.paid));
    assert_eq!(money(world), coins, "coins are conserved");
}

fn burning(world: &World) -> Vec<SimId> {
    world.resource::<UpkeepState>().burning.clone()
}

fn fire_out(world: &World, building: SimId) -> Option<Value> {
    effects(world, "VillageFireOut")
        .into_iter()
        .find(|effect| effect["building"] == json!(building.0))
}

/// Starts a fire at a home and, when `crew` is given, puts that many awake
/// residents of its settlement at the door.
fn burn_a_home(edit: fn(&mut Value), crew: usize) -> (App, SimId) {
    let mut app = app(edit);
    let cottage = home(app.world(), "home_03");
    upkeep::set_condition_of(app.world_mut(), cottage, 900).expect("set");
    upkeep::start_fire(app.world_mut(), cottage).expect("a fire");
    let door = *app
        .world()
        .get::<SimPos>(entity(app.world(), cottage))
        .expect("door");
    let harbour = settlement(app.world(), "harbour");
    for (_, actor) in residents(app.world(), harbour).into_iter().take(crew) {
        teleport(app.world_mut(), actor, SimPos::new(door.x + 1_000, door.z));
    }
    (app, cottage)
}

fn nobody_comes(world: &mut Value) {
    economy(world)["upkeep"]["fire_reach_m"] = json!(0);
    economy(world)["upkeep"]["fire_spread_permille"] = json!(0);
}

fn everybody_comes(world: &mut Value) {
    economy(world)["upkeep"]["fire_spread_permille"] = json!(0);
}

#[test]
fn firefighters_put_a_fire_out_with_less_damage() {
    let (mut fought, cottage) = burn_a_home(everybody_comes, 4);
    let (mut alone, _) = burn_a_home(nobody_comes, 0);
    assert_eq!(burning(fought.world()), [cottage]);
    assert_eq!(effects(fought.world(), "VillageFire").len(), 1);
    assert!(until(&mut fought, 3_000, 50, |app| fire_out(
        app.world(),
        cottage
    )
    .is_some()));
    assert!(until(&mut alone, 3_000, 50, |app| fire_out(
        app.world(),
        cottage
    )
    .is_some()));
    let fought = fire_out(fought.world(), cottage).expect("out");
    let alone = fire_out(alone.world(), cottage).expect("out");
    assert_eq!(fought["put_out"], json!(true), "{fought}");
    assert!(fought["fighters"].as_u64().unwrap_or(0) >= 1, "{fought}");
    assert_eq!(alone["put_out"], json!(false), "{alone}");
    let (with, without) = (
        fought["damage"].as_u64().expect("damage"),
        alone["damage"].as_u64().expect("damage"),
    );
    assert!(
        with * 2 < without,
        "fought {with} permille against {without} unfought"
    );
    // Unfought: the flash and 35 a step for the 2000 ticks it burns.
    assert_eq!(without, 150 + 35 * 20);
}

#[test]
fn a_fire_spreads_to_a_near_door() {
    let (mut app, cottage) = burn_a_home(
        |world| {
            economy(world)["upkeep"]["fire_reach_m"] = json!(0);
            economy(world)["upkeep"]["fire_spread_m"] = json!(30);
            economy(world)["upkeep"]["fire_spread_permille"] = json!(1000);
        },
        0,
    );
    run_ticks(app.world_mut(), 120);
    let world = app.world();
    let state = world.resource::<UpkeepState>();
    assert!(state.spread > 0, "nothing caught");
    let spread = effects(world, "VillageFire");
    assert!(
        spread
            .iter()
            .any(|effect| effect["from"] == json!(cottage.0)),
        "{spread:?}"
    );
}

#[test]
fn fires_are_seeded_by_the_world_seed_and_the_day() {
    fn risky(world: &mut Value) {
        short_days(world, DAY);
        only_weather(world, "clear");
        for building in economy(world)["buildings"]
            .as_array_mut()
            .expect("buildings")
        {
            if building["id"] == json!("cottage") {
                building["upkeep"]["fire"] = json!({"daily_permille": 100});
            }
        }
    }
    let rolls = |seed: u64| {
        let mut app = world_for(seed, risky);
        run_ticks(app.world_mut(), 2 + 3 * DAY);
        let state = app.world().resource::<UpkeepState>().clone();
        (state.fires, state.pending, state_hash(app.world_mut()))
    };
    let (fires, pending, hash) = rolls(7);
    assert!(fires > 0, "42 homes at 10% a day burn within three days");
    assert_eq!(rolls(7), (fires, pending.clone(), hash), "the same seed");
    let (other, other_pending, _) = rolls(11);
    assert!(
        (other, other_pending) != (fires, pending),
        "another seed rolls other fires"
    );
}

#[test]
fn a_save_continues_mid_fire_with_the_same_hash() {
    let (mut app, cottage) = burn_a_home(everybody_comes, 2);
    run_ticks(app.world_mut(), 150);
    assert_eq!(burning(app.world()), [cottage], "still burning");
    let bakery = workplace(app.world(), Role::Baker);
    upkeep::set_condition_of(app.world_mut(), bakery, 0).expect("set");
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut fresh = world_with(quiet);
    let expected = snapshot::save(fresh.world_mut()).expect("baseline");
    integrity::validate_snapshot_structure(&expected, &saved).expect("the save continues");
    let report =
        snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    integrity::validate_colony(fresh.world()).expect("valid after the restore");
    civic::validate(fresh.world()).expect("civic state valid");
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    for _ in 0..6 {
        run_ticks(app.world_mut(), 400);
        run_ticks(fresh.world_mut(), 400);
        assert_eq!(
            state_hash(fresh.world_mut()),
            state_hash(app.world_mut()),
            "the continued world runs on as the original"
        );
    }
    assert!(burning(app.world()).is_empty(), "the fire went out");
}

#[test]
fn the_same_fires_and_repairs_make_the_same_world() {
    let run = || {
        let (mut app, _) = burn_a_home(everybody_comes, 3);
        let bakery = workplace(app.world(), Role::Baker);
        upkeep::set_condition_of(app.world_mut(), bakery, 100).expect("set");
        run_ticks(app.world_mut(), 4 * DAY);
        let report = upkeep::report(app.world());
        (state_hash(app.world_mut()), report)
    };
    let (a, report) = run();
    let (b, again) = run();
    assert_eq!(a, b);
    assert_eq!(report, again);
    assert!(report.fires >= 1);
    let _ = village::calendar;
}
