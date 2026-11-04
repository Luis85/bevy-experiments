//! Real life-tree decisions under declared, bounded operator fixtures.
//! No provider use, invented income, or simulation rescue occurs during work.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::items;
use npc_framework::kernel::journal::{Journal, JournalKind, set_journal};
use npc_framework::prelude::*;
use testbed_content::village::{
    self, Role, VillageAct, VillageState, Villager, Wallet, catalog, economy, production,
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "living_village")
}
fn coins(app: &App) -> u64 {
    u64::from(app.world().resource::<VillageState>().fund)
        + app
            .world()
            .resource::<SimIndex>()
            .map
            .values()
            .map(|entity| {
                u64::from(
                    app.world()
                        .get::<Wallet>(*entity)
                        .map_or(0, |wallet| wallet.coins),
                ) + u64::from(
                    app.world()
                        .get::<village::CoinPurse>(*entity)
                        .map_or(0, |purse| purse.coins),
                )
            })
            .sum::<u64>()
}
fn actor(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn freeze_others(app: &mut App, active: Entity) {
    let others: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| {
            *entity != active
                && app
                    .world()
                    .get::<Villager>(*entity)
                    .is_some_and(|person| person.role != Role::Visitor)
        })
        .collect();
    for other in others {
        app.world_mut().entity_mut(other).remove::<Brain>();
    }
}
fn source(app: &App, kind: &str) -> Entity {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(&format!("patch_{kind}"));
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn isolate_cashless(app: &mut App, person: Entity, remove: &[&str]) {
    freeze_others(app, person);
    let hero = actor(app, "human");
    let held: Vec<_> = items::carried_items(app.world(), person)
        .into_iter()
        .filter_map(|id| {
            let entity = app.world().resource::<SimIndex>().get(id)?;
            app.world()
                .get::<Item>(entity)
                .is_some_and(|item| remove.contains(&item.kind.as_str()))
                .then_some(entity)
        })
        .collect();
    for entity in held {
        items::can_store(app.world(), hero, entity).unwrap();
        items::put_in_inventory(app.world_mut(), hero, entity);
    }
    let cash = app.world().get::<Wallet>(person).unwrap().coins;
    app.world_mut().get_mut::<Wallet>(person).unwrap().coins = 0;
    app.world_mut().resource_mut::<VillageState>().fund += cash;
}
fn set_need(app: &mut App, person: Entity, name: &str, value: i32) {
    let unit = app.world().get::<SimId>(person).unwrap().0;
    submit(
        app.world_mut(),
        None,
        SimCommand::SetNeed {
            unit,
            need: name.into(),
            value,
        },
    )
    .unwrap();
}
fn night(app: &mut App) {
    let config = app.world().resource::<SimConfig>();
    let tick = config.day_length * (1000 - u64::from(config.night_permille)) / 1000;
    assert!(config.is_night(tick));
    app.world_mut().resource_mut::<SimClock>().tick = tick;
}
fn count_effect(app: &App, person: Entity, name: &str, kind: &str) -> usize {
    let unit = *app.world().get::<SimId>(person).unwrap();
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|entry| {
            entry.unit == Some(unit)
                && matches!(&entry.kind,JournalKind::Effect{effect,applied:true}
            if effect.get("type").and_then(serde_json::Value::as_str)==Some(name)
                &&effect.get("kind").and_then(serde_json::Value::as_str)==Some(kind))
        })
        .count()
}

#[test]
fn ready_financed_clothing_is_made_before_cross_island_buffer_fiber_shopping() {
    let mut app = scene();
    let tailor = actor(&app, "Lina");
    let farmer = actor(&app, "Ada");
    economy::grant(
        app.world_mut(),
        tailor,
        Item::new(catalog::LEATHER).stack(2),
    );
    economy::grant(app.world_mut(), tailor, Item::new(catalog::THREAD).stack(2));
    economy::grant(app.world_mut(), farmer, Item::new(catalog::FIBER).stack(8));
    let station = app
        .world()
        .resource::<SimIndex>()
        .get(app.world().get::<Villager>(tailor).unwrap().workplace)
        .unwrap();
    let pos = *app.world().get::<SimPos>(station).unwrap();
    app.world_mut().entity_mut(tailor).insert(pos);
    production::refresh(app.world_mut());
    assert!(production::production_demand(app.world(), tailor, catalog::WARM_CLOTHES) > 0);
    assert!(production::purchase_target(app.world(), tailor, catalog::FIBER) > 0);
    assert!(village::market::available_to_sell(app.world(), farmer, catalog::FIBER) > 0);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<Crafting>(tailor)
            .expect("a complete financed coat should start before unrelated buffer procurement")
            .recipe,
        catalog::TAILOR
    );
    assert!(app.world().get::<MoveIntent>(tailor).is_none());
}

#[test]
fn actionable_night_water_recovery_preempts_optional_home_rest_without_cash() {
    let mut app = scene();
    let initial_coins = coins(&app);
    let transactions = app.world().resource::<VillageState>().transactions;
    let person = actor(&app, "Ada");
    isolate_cashless(&mut app, person, &[catalog::WATER]);
    let patch = source(&app, catalog::WATER);
    let pos = *app.world().get::<SimPos>(patch).unwrap();
    app.world_mut().entity_mut(person).insert(pos);
    night(&mut app);
    set_need(&mut app, person, "Thirst", 5000);
    set_need(&mut app, person, "Energy", 20_000);
    set_journal(app.world_mut(), true, false);
    let before = app
        .world()
        .get::<village::ResourcePatch>(patch)
        .unwrap()
        .remaining;
    for _ in 0..1000 {
        run_ticks(app.world_mut(), 1);
        assert_eq!(app.world().get::<Wallet>(person).unwrap().coins, 0);
        if economy::need(app.world(), person, "Thirst") > 50_000 {
            break;
        }
    }
    assert!(
        economy::need(app.world(), person, "Thirst") > 50_000,
        "a usable source must outrank optional night/rest routines"
    );
    assert_eq!(
        count_effect(&app, person, "VillageGathered", catalog::WATER),
        1
    );
    assert_eq!(
        count_effect(&app, person, "VillageConsumed", catalog::WATER),
        1
    );
    assert_eq!(
        app.world()
            .get::<village::ResourcePatch>(patch)
            .unwrap()
            .remaining,
        before - 1
    );
    assert_eq!(coins(&app), initial_coins);
    assert_eq!(
        app.world().resource::<VillageState>().transactions,
        transactions
    );
}

#[test]
fn actionable_night_food_recovery_forages_real_berries_before_waiting_at_home() {
    let mut app = scene();
    let initial_coins = coins(&app);
    let transactions = app.world().resource::<VillageState>().transactions;
    let person = actor(&app, "Ada");
    isolate_cashless(
        &mut app,
        person,
        &["bread", catalog::COOKED_MEAL, catalog::BERRIES],
    );
    let patch = source(&app, catalog::BERRIES);
    let pos = *app.world().get::<SimPos>(patch).unwrap();
    app.world_mut().entity_mut(person).insert(pos);
    night(&mut app);
    set_need(&mut app, person, "Hunger", 5000);
    set_journal(app.world_mut(), true, false);
    let before = app
        .world()
        .get::<village::ResourcePatch>(patch)
        .unwrap()
        .remaining;
    for _ in 0..1000 {
        run_ticks(app.world_mut(), 1);
        assert_eq!(app.world().get::<Wallet>(person).unwrap().coins, 0);
        if economy::need(app.world(), person, "Hunger") > 50_000 {
            break;
        }
    }
    assert!(economy::need(app.world(), person, "Hunger") > 50_000);
    let gathered = count_effect(&app, person, "VillageGathered", catalog::BERRIES);
    assert!(gathered > 0);
    assert_eq!(
        count_effect(&app, person, "VillageConsumed", catalog::BERRIES),
        gathered
    );
    assert_eq!(
        app.world()
            .get::<village::ResourcePatch>(patch)
            .unwrap()
            .remaining,
        before - gathered as u32
    );
    assert_eq!(coins(&app), initial_coins);
    assert_eq!(
        app.world().resource::<VillageState>().transactions,
        transactions
    );
}

#[test]
fn valid_one_metre_water_trade_executes_without_chasing_to_point_eight_metres() {
    let mut app = scene();
    let buyer = actor(&app, "Ada");
    let seller = actor(&app, "Nell");
    freeze_others(&mut app, buyer);
    economy::grant(app.world_mut(), seller, Item::new(catalog::WATER));
    let pos = *app.world().get::<SimPos>(seller).unwrap();
    app.world_mut()
        .entity_mut(buyer)
        .insert(SimPos::new(pos.x + 1000, pos.z));
    production::refresh(app.world_mut());
    let mut buy = VillageAct::new(*app.world().get::<SimId>(buyer).unwrap(), "buy");
    buy.target = app.world().get::<SimId>(seller).copied();
    buy.kind = catalog::WATER.into();
    buy.check(app.world())
        .expect("the shared client action is valid at one metre");
    let before = economy::count(app.world(), buyer, catalog::WATER);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        economy::count(app.world(), buyer, catalog::WATER),
        before + 1
    );
    assert!(app.world().get::<MoveIntent>(buyer).is_none());
}

#[test]
fn valid_one_metre_subsistence_gather_starts_at_the_shared_action_reach() {
    let mut app = scene();
    let person = actor(&app, "Ada");
    isolate_cashless(&mut app, person, &[catalog::WATER]);
    let patch = source(&app, catalog::WATER);
    let pos = *app.world().get::<SimPos>(patch).unwrap();
    app.world_mut()
        .entity_mut(person)
        .insert(SimPos::new(pos.x + 1000, pos.z));
    set_need(&mut app, person, "Thirst", 20_000);
    let mut gather = VillageAct::new(*app.world().get::<SimId>(person).unwrap(), "gather");
    gather.target = app.world().get::<SimId>(patch).copied();
    gather
        .check(app.world())
        .expect("the shared gather action is valid at one metre");
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<village::Gathering>(person).is_some());
    assert!(app.world().get::<MoveIntent>(person).is_none());
}

#[test]
fn valid_one_metre_hunt_starts_with_the_same_reach_as_the_player_action() {
    let mut app = scene();
    let hunter = actor(&app, "Rhea");
    freeze_others(&mut app, hunter);
    let (target, _, pos) = village::hunting::available_prey(app.world())[0];
    app.world_mut()
        .entity_mut(hunter)
        .insert(SimPos::new(pos.x + 1000, pos.z));
    production::refresh(app.world_mut());
    assert!(
        production::production_demand(app.world(), hunter, catalog::HIDE) > 0
            || production::production_demand(app.world(), hunter, catalog::RAW_MEAT) > 0
    );
    let mut hunt = VillageAct::new(*app.world().get::<SimId>(hunter).unwrap(), "hunt");
    hunt.target = Some(target);
    hunt.check(app.world())
        .expect("the shared hunt action is valid at one metre");
    run_ticks(app.world_mut(), 1);
    assert!(
        app.world()
            .get::<village::hunting::Hunting>(hunter)
            .is_some()
    );
    assert!(app.world().get::<MoveIntent>(hunter).is_none());
}
