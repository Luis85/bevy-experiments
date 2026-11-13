//! Funded material demand and physical resident work in the living island.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Role, Villager, Wallet, catalog, economy, production};
use npc_framework::crafting::{Durability, RecipeRegistry};
use npc_framework::interaction::items;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}
fn actor(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn residents(app: &App) -> Vec<Entity> {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| {
            app.world()
                .get::<Villager>(*e)
                .is_some_and(|v| v.role != Role::Visitor)
        })
        .collect()
}
fn clear(app: &mut App, e: Entity) {
    for (_, item) in economy::free_items(app.world(), e) {
        economy::consume(app.world_mut(), e, &item.kind, item.stack).unwrap();
    }
}
fn coat(app: &mut App, e: Entity) {
    economy::grant(app.world_mut(), e, Item::new(catalog::WARM_CLOTHES));
    let id = items::carried_matching(app.world(), e, catalog::WARM_CLOTHES).unwrap();
    let item = app.world().resource::<SimIndex>().get(id).unwrap();
    app.world_mut()
        .entity_mut(item)
        .insert(Durability::new(catalog::CLOTHING_DURABILITY));
    items::put_in_slot(app.world_mut(), e, item, "body");
}
fn terminal_stocked() -> App {
    let mut app = scene();
    for e in residents(&app) {
        clear(&mut app, e);
        economy::grant(app.world_mut(), e, Item::new("bread").stack(16));
        economy::grant(app.world_mut(), e, Item::new(catalog::WATER).stack(16));
        coat(&mut app, e);
        app.world_mut().get_mut::<Wallet>(e).unwrap().coins = 10_000;
    }
    let inn = actor(&app, "Tess");
    for (kind, count) in [("log", 2), (catalog::BOWL, 1), ("hammer", 1)] {
        economy::grant(app.world_mut(), inn, Item::new(kind).stack(count));
    }
    app
}
fn remove_coat(app: &mut App, e: Entity) {
    let id = items::equipped_matching(app.world(), e, catalog::WARM_CLOTHES).unwrap();
    let item = app.world().resource::<SimIndex>().get(id).unwrap();
    items::take_from_holder(app.world_mut(), item);
    app.world_mut().despawn(item);
}

#[test]
fn unfunded_wants_do_not_become_orders_or_unsellable_production() {
    let mut app = scene();
    for e in residents(&app) {
        clear(&mut app, e);
        app.world_mut().get_mut::<Wallet>(e).unwrap().coins = 0;
    }
    production::refresh(app.world_mut());
    let plan = app.world().resource::<production::ProductionPlan>();
    assert!(
        plan.procurement
            .values()
            .all(|goods| goods.values().all(|count| *count == 0))
    );
    assert!(
        plan.production
            .values()
            .all(|goods| goods.values().all(|count| *count == 0))
    );
}

#[test]
fn finite_working_cash_funds_complete_joined_inputs_instead_of_one_stranded_stockpile() {
    let mut app = terminal_stocked();
    for e in residents(&app) {
        remove_coat(&mut app, e);
    }
    let tailor = actor(&app, "Lina");
    // After reserving her own garment, this purse supports one complete
    // leather+thread bundle, not the whole village's wardrobe at once.
    app.world_mut().get_mut::<Wallet>(tailor).unwrap().coins = 80;
    production::refresh(app.world_mut());
    let leather = production::desired_stock(app.world(), tailor, catalog::LEATHER);
    let thread = production::desired_stock(app.world(), tailor, catalog::THREAD);
    assert!(
        leather >= 2 && thread >= 2,
        "both joining inputs must remain financeable: leather={leather}, thread={thread}"
    );
    assert_eq!(
        production::purchase_target(app.world(), tailor, catalog::THREAD),
        0,
        "internal spinning must not silently become a finished-thread cash bid"
    );
    assert!(production::internal_demand(app.world(), tailor, catalog::THREAD) >= thread);
    let spin = &app.world().resource::<RecipeRegistry>().recipes[catalog::SPIN];
    let output = spin
        .outputs
        .iter()
        .find(|output| output.kind == catalog::THREAD)
        .unwrap()
        .count;
    let input = spin
        .inputs
        .iter()
        .find(|input| input.key == catalog::FIBER)
        .unwrap()
        .count;
    let fiber = production::purchase_target(app.world(), tailor, catalog::FIBER)
        .saturating_sub(economy::count(app.world(), tailor, catalog::FIBER));
    assert!(
        fiber >= thread.div_ceil(output) * input,
        "the actual spinning frontier must be funded"
    );
    let bids: Vec<_> = catalog::trade_kinds(app.world())
        .iter()
        .map(|kind| {
            let quantity = production::purchase_target(app.world(), tailor, kind)
                .saturating_sub(economy::count(app.world(), tailor, kind));
            let limit = production::purchase_bid_limit(app.world(), tailor, kind)
                .unwrap_or_else(|| village::market::unit_price(app.world(), kind).unwrap());
            (*kind, quantity, limit)
        })
        .collect();
    let cost: u64 = bids
        .iter()
        .map(|(_, quantity, limit)| u64::from(*quantity) * u64::from(*limit))
        .sum();
    assert!(
        cost <= 80,
        "all external input reservations must share the actual purse: {bids:?}"
    );
    assert_eq!(app.world().get::<Wallet>(tailor).unwrap().coins, 80);
}

#[test]
fn partial_bread_reserves_keep_a_funded_preferred_meal_request() {
    let mut app = terminal_stocked();
    let buyer = residents(&app)
        .into_iter()
        .find(|e| {
            app.world().get::<Villager>(*e).unwrap().role != Role::Baker
                && production::food_preference(app.world(), *e) == catalog::COOKED_MEAL
        })
        .expect("the village includes a resident who prefers meals");
    clear(&mut app, buyer);
    economy::grant(app.world_mut(), buyer, Item::new("bread"));
    economy::grant(app.world_mut(), buyer, Item::new(catalog::WATER).stack(16));
    production::refresh(app.world_mut());
    assert!(
        production::desired_stock(app.world(), buyer, catalog::COOKED_MEAL) > 0,
        "one bread must not cover a two-day forecast of {} ticks: {:?}",
        app.world().resource::<SimConfig>().day_length * 2,
        app.world().resource::<production::ProductionPlan>()
    );
    let baker = actor(&app, "Mira");
    assert!(production::production_demand(app.world(), baker, catalog::COOKED_MEAL) > 0);
}

#[test]
fn registered_material_cycles_fail_validation_before_play() {
    let mut app = scene();
    // A deliberately invalid content definition is inspected, never ticked.
    app.world_mut()
        .resource_mut::<RecipeRegistry>()
        .recipes
        .get_mut(catalog::TAILOR)
        .unwrap()
        .inputs
        .push(npc_framework::crafting::RecipeInput {
            key: catalog::WARM_CLOTHES.into(),
            count: 1,
        });
    assert!(
        production::validate(app.world())
            .unwrap_err()
            .contains("cyclic")
    );
}

#[test]
fn an_urgent_hungry_actor_eats_an_owned_meal_instead_of_ignoring_it_for_bread() {
    let mut app = scene();
    let e = actor(&app, "Ada");
    clear(&mut app, e);
    economy::grant(app.world_mut(), e, Item::new(catalog::COOKED_MEAL));
    let need = app.world().resource::<NeedRegistry>().id("Hunger").unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(need)
        .value = 10_000;
    run_ticks(app.world_mut(), 1);
    assert!(economy::need(app.world(), e, "Hunger") > 90_000);
    assert_eq!(economy::count(app.world(), e, catalog::COOKED_MEAL), 0);
}

#[test]
fn production_beliefs_and_running_jobs_resume_with_identical_hashes() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 77, 800, 40);
}
