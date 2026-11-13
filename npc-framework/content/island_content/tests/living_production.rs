//! Funded material demand and physical resident work in the living island.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Role, Villager, Wallet, catalog, economy, production};
use npc_framework::crafting::{Durability, RecipeRegistry};
use npc_framework::interaction::items;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;

fn scene() -> App {
    island_content::new_sim_app(7, "living_village")
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
fn money(app: &mut App) -> u64 {
    let wallets: u64 = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|w| u64::from(w.coins))
        .sum();
    let purses: u64 = app
        .world_mut()
        .query::<&village::CoinPurse>()
        .iter(app.world())
        .map(|p| u64::from(p.coins))
        .sum();
    wallets + purses + u64::from(app.world().resource::<village::VillageState>().fund)
}
fn finance_diagnostic(app: &App) -> serde_json::Value {
    let world = app.world();
    let plan = world.resource::<production::ProductionPlan>();
    let people: Vec<_> = residents(app).into_iter().map(|e| {
        let person = world.get::<Villager>(e).unwrap();
        let id = world.get::<SimId>(e).unwrap();
        let recipes: Vec<_> = catalog::recipes_for(world,person.role).iter().map(|name| {
            let recipe = world.resource::<RecipeRegistry>().recipes.get(*name).unwrap();
            let actual_missing_cost: u64 = recipe.inputs.iter().map(|input| {
                u64::from(input.count.saturating_sub(economy::count(world,e,&input.key)))
                    * u64::from(village::market::unit_price(world,&input.key).unwrap())
            }).sum();
            serde_json::json!({"recipe":name,"actual_missing_cost":actual_missing_cost,
                "full_input_cost":village::market::input_cost(world,name)})
        }).collect();
        serde_json::json!({"name":person.name,"role":person.role,"cash":world.get::<Wallet>(e).unwrap().coins,
            "needs":{"hunger":economy::need(world,e,"Hunger"),"thirst":economy::need(world,e,"Thirst"),"warmth":economy::need(world,e,"Warmth")},
            "wanted":plan.wanted.get(id),"funded":plan.procurement.get(id),"self":plan.self_commissions.get(id),
            "produce":plan.production.get(id),"stock":economy::free_items(world,e).into_iter().map(|(_,item)| (item.kind,item.stack)).collect::<Vec<_>>(),
            "recipes":recipes,"job":world.get::<production::JobIntent>(e)})
    }).collect();
    let prices: std::collections::BTreeMap<_, _> = catalog::trade_kinds(world)
        .iter()
        .map(|kind| (*kind, village::market::unit_price(world, kind)))
        .collect();
    serde_json::json!({"tick":world.resource::<SimClock>().tick,"people":people,"prices":prices,
        "produced":world.resource::<village::VillageState>().produced})
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
fn satisfied_lives_do_not_generate_industrial_stock_quotas() {
    let mut app = terminal_stocked();
    production::refresh(app.world_mut());
    assert!(
        app.world()
            .resource::<production::ProductionPlan>()
            .production
            .values()
            .all(|goods| goods.values().all(|count| *count == 0))
    );
    run_ticks(app.world_mut(), 1);
    for e in residents(&app) {
        assert!(app.world().get::<MoveIntent>(e).is_none());
        assert!(app.world().get::<production::JobIntent>(e).is_none());
    }
}

#[test]
fn one_missing_garment_propagates_through_joined_materials_and_hunting_tools() {
    let mut app = terminal_stocked();
    let buyer = actor(&app, "Ada");
    remove_coat(&mut app, buyer);
    production::refresh(app.world_mut());
    for (name, kind, minimum) in [
        ("Lina", catalog::WARM_CLOTHES, 1),
        ("Sage", catalog::LEATHER, 2),
        ("Lina", catalog::THREAD, 2),
        ("Ada", catalog::FIBER, 4),
        ("Rhea", catalog::HIDE, 2),
        ("Ivo", catalog::SPEAR, 2),
        ("Jun", "plank", 1),
        ("Bram", "ore", 1),
    ] {
        assert!(
            production::production_demand(app.world(), actor(&app, name), kind) >= minimum,
            "missing upstream demand for {name}/{kind}: {:?}",
            app.world().resource::<production::ProductionPlan>()
        );
    }
}

#[test]
fn a_material_lot_cannot_cover_two_joining_branches_twice() {
    let mut needs_more = Vec::new();
    for planks in [2, 4] {
        let mut app = terminal_stocked();
        let buyer = actor(&app, "Ada");
        remove_coat(&mut app, buyer);
        let carpenter = actor(&app, "Jun");
        economy::grant(app.world_mut(), carpenter, Item::new("plank").stack(planks));
        production::refresh(app.world_mut());
        needs_more.push(production::production_demand(
            app.world(),
            carpenter,
            "plank",
        ));
    }
    assert!(
        needs_more[0] > 0,
        "two planks cannot satisfy both two spear shafts and a two-plank bowl"
    );
    assert_eq!(
        needs_more[1], 0,
        "four real planks cover those two branches once"
    );
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
fn a_cashless_tailor_can_meet_her_own_warmth_need_with_real_owned_inputs() {
    // Declared operator fixture: everyone else's needs are stocked; Lina alone
    // lacks a coat but owns the exact material bundle. Cash moves to the fund,
    // rather than disappearing. Lina's replacement coat is not supplied here.
    let mut app = terminal_stocked();
    let tailor = actor(&app, "Lina");
    remove_coat(&mut app, tailor);
    let cash = app.world().get::<Wallet>(tailor).unwrap().coins;
    app.world_mut().get_mut::<Wallet>(tailor).unwrap().coins = 0;
    app.world_mut().resource_mut::<village::VillageState>().fund += cash;
    economy::grant(
        app.world_mut(),
        tailor,
        Item::new(catalog::LEATHER).stack(2),
    );
    economy::grant(app.world_mut(), tailor, Item::new(catalog::THREAD).stack(2));
    let workplace = app.world().get::<Villager>(tailor).unwrap().workplace;
    let station = app.world().resource::<SimIndex>().get(workplace).unwrap();
    let pos = *app.world().get::<SimPos>(station).unwrap();
    app.world_mut().entity_mut(tailor).insert(pos);
    production::refresh(app.world_mut());
    assert_eq!(
        production::production_demand(app.world(), tailor, catalog::WARM_CLOTHES),
        1,
        "owned materials back self-production without buying the finished good"
    );
    assert_eq!(
        production::purchase_wanted(app.world(), tailor, catalog::WARM_CLOTHES),
        0,
        "material-backed work must not bid for the finished product"
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<npc_framework::crafting::Crafting>(tailor)
            .unwrap()
            .recipe,
        catalog::TAILOR
    );
    run_ticks(app.world_mut(), 340);
    assert!(village::living::has_coat(app.world(), tailor));
    assert_eq!(economy::count(app.world(), tailor, catalog::LEATHER), 0);
    assert_eq!(economy::count(app.world(), tailor, catalog::THREAD), 0);
    assert_eq!(app.world().get::<Wallet>(tailor).unwrap().coins, 0);
    assert_eq!(app.world().resource::<village::VillageState>().fund, cash);
}

#[test]
fn self_production_finances_only_its_missing_materials_not_the_finished_good_again() {
    let mut app = terminal_stocked();
    let tailor = actor(&app, "Lina");
    remove_coat(&mut app, tailor);
    let cash = app.world().get::<Wallet>(tailor).unwrap().coins;
    let leather_price = village::market::unit_price(app.world(), catalog::LEATHER).unwrap();
    app.world_mut().get_mut::<Wallet>(tailor).unwrap().coins = leather_price;
    app.world_mut().resource_mut::<village::VillageState>().fund += cash - leather_price;
    economy::grant(app.world_mut(), tailor, Item::new(catalog::LEATHER));
    economy::grant(app.world_mut(), tailor, Item::new(catalog::THREAD).stack(2));
    production::refresh(app.world_mut());
    assert_eq!(
        production::production_demand(app.world(), tailor, catalog::WARM_CLOTHES),
        1
    );
    assert_eq!(
        production::desired_stock(app.world(), tailor, catalog::LEATHER),
        2
    );
    assert_eq!(
        production::purchase_wanted(app.world(), tailor, catalog::WARM_CLOTHES),
        0
    );
    assert_eq!(
        production::desired_stock(app.world(), tailor, catalog::THREAD),
        2
    );
    assert_eq!(
        app.world().get::<Wallet>(tailor).unwrap().coins,
        leather_price,
        "planning must not perform a payment or manufacture missing materials"
    );
    assert_eq!(economy::count(app.world(), tailor, catalog::LEATHER), 1);
}

#[test]
fn latent_joined_material_bids_reserve_the_complete_bundle_within_the_real_purse() {
    let mut app = terminal_stocked();
    let tailor = actor(&app, "Lina");
    remove_coat(&mut app, tailor);
    let cash = app.world().get::<Wallet>(tailor).unwrap().coins;
    app.world_mut().get_mut::<Wallet>(tailor).unwrap().coins = 20;
    app.world_mut().resource_mut::<village::VillageState>().fund += cash - 20;
    production::refresh(app.world_mut());
    // Thread is an internal spin commission; only its real fiber joins the
    // externally purchased leather. No fictitious finished-thread cash bid.
    assert_eq!(
        production::purchase_target(app.world(), tailor, catalog::LEATHER),
        2
    );
    assert_eq!(
        production::purchase_target(app.world(), tailor, catalog::FIBER),
        4
    );
    assert_eq!(
        production::purchase_target(app.world(), tailor, catalog::THREAD),
        0
    );
    let leather = production::purchase_bid_limit(app.world(), tailor, catalog::LEATHER).unwrap();
    let fiber = production::purchase_bid_limit(app.world(), tailor, catalog::FIBER).unwrap();
    assert!(leather < village::market::unit_price(app.world(), catalog::LEATHER).unwrap());
    assert!(
        leather * 2 + fiber * 4 <= 20,
        "complementary reservations must share the same purse"
    );
    assert!(production::internal_demand(app.world(), tailor, catalog::THREAD) > 0);
    assert_eq!(app.world().get::<Wallet>(tailor).unwrap().coins, 20);
    let hero = actor(&app, "human");
    economy::grant(app.world_mut(), hero, Item::new(catalog::LEATHER));
    let position = *app.world().get::<SimPos>(tailor).unwrap();
    app.world_mut().entity_mut(hero).insert(position);
    let mut sell = village::VillageAct::new(*app.world().get::<SimId>(hero).unwrap(), "sell");
    sell.target = app.world().get::<SimId>(tailor).copied();
    sell.kind = catalog::LEATHER.into();
    assert!(
        sell.perform(app.world_mut()).is_err(),
        "a seller cannot force a10coin leather sale past the recipient's joint cap"
    );
    assert_eq!(app.world().get::<Wallet>(tailor).unwrap().coins, 20);
    assert_eq!(economy::count(app.world(), hero, catalog::LEATHER), 1);
}

#[test]
fn own_intermediate_work_executes_for_a_real_garment_even_with_negative_standalone_margin() {
    let mut app = terminal_stocked();
    let tailor = actor(&app, "Lina");
    remove_coat(&mut app, tailor);
    let cash = app.world().get::<Wallet>(tailor).unwrap().coins;
    app.world_mut().get_mut::<Wallet>(tailor).unwrap().coins = 0;
    app.world_mut().resource_mut::<village::VillageState>().fund += cash;
    economy::grant(
        app.world_mut(),
        tailor,
        Item::new(catalog::LEATHER).stack(2),
    );
    economy::grant(app.world_mut(), tailor, Item::new(catalog::FIBER).stack(4));
    production::refresh(app.world_mut());
    village::market::tick(app.world_mut());
    // Declared isolated quotation fixture: no cash or goods are changed by
    // these opportunity-cost beliefs. The separate shock test learns prices.
    let state = &mut *app
        .world_mut()
        .resource_mut::<village::market::MarketState>();
    state
        .commodities
        .get_mut(catalog::FIBER)
        .unwrap()
        .reference_milli = 9000;
    state
        .commodities
        .get_mut(catalog::THREAD)
        .unwrap()
        .reference_milli = 11000;
    app.world_mut()
        .resource_mut::<production::ProductionPlan>()
        .refreshed_at = None;
    production::refresh(app.world_mut());
    assert!(
        village::market::input_cost(app.world(), catalog::SPIN).unwrap()
            > village::market::unit_price(app.world(), catalog::THREAD).unwrap()
    );
    assert!(production::internal_demand(app.world(), tailor, catalog::THREAD) > 0);
    let station = app
        .world()
        .resource::<SimIndex>()
        .get(app.world().get::<Villager>(tailor).unwrap().workplace)
        .unwrap();
    let position = *app.world().get::<SimPos>(station).unwrap();
    app.world_mut().entity_mut(tailor).insert(position);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<npc_framework::crafting::Crafting>(tailor)
            .unwrap()
            .recipe,
        catalog::SPIN
    );
    run_ticks(app.world_mut(), 800);
    assert!(village::living::has_coat(app.world(), tailor));
    assert_eq!(economy::count(app.world(), tailor, catalog::FIBER), 0);
    assert_eq!(economy::count(app.world(), tailor, catalog::LEATHER), 0);
    assert_eq!(app.world().get::<Wallet>(tailor).unwrap().coins, 0);
}

#[test]
fn a_funded_ready_bowl_job_completes_with_owned_planks_despite_stale_book_cost() {
    let mut app = terminal_stocked();
    let buyer = actor(&app, "Ada");
    remove_coat(&mut app, buyer);
    let tanner = actor(&app, "Sage");
    let tailor = actor(&app, "Lina");
    let carpenter = actor(&app, "Jun");
    // Declared material fixture: the real garment need reaches tanning, whose
    // only missing item is a bowl. Jun owns exactly its two physical planks.
    economy::grant(app.world_mut(), tanner, Item::new(catalog::HIDE).stack(2));
    economy::grant(app.world_mut(), tailor, Item::new(catalog::THREAD).stack(2));
    economy::grant(app.world_mut(), carpenter, Item::new("plank").stack(2));
    let station = app
        .world()
        .resource::<SimIndex>()
        .get(app.world().get::<Villager>(carpenter).unwrap().workplace)
        .unwrap();
    let position = *app.world().get::<SimPos>(station).unwrap();
    app.world_mut().entity_mut(carpenter).insert(position);
    production::refresh(app.world_mut());
    village::market::tick(app.world_mut());
    // Captured quotation shape from the failed mature run: held planks are
    // marked far above the real bowl bid. No cash or item is changed here.
    let state = &mut *app
        .world_mut()
        .resource_mut::<village::market::MarketState>();
    state.commodities.get_mut("plank").unwrap().reference_milli = 106_000;
    state
        .commodities
        .get_mut(catalog::BOWL)
        .unwrap()
        .reference_milli = 2_000;
    app.world_mut()
        .resource_mut::<production::ProductionPlan>()
        .refreshed_at = None;
    production::refresh(app.world_mut());
    assert!(production::production_demand(app.world(), carpenter, catalog::BOWL) > 0);
    assert!(production::purchase_target(app.world(), tanner, catalog::BOWL) > 0);
    assert_eq!(
        production::internal_demand(app.world(), carpenter, catalog::BOWL),
        0,
        "this is another resident's funded need, not the carpenter's self-commission"
    );
    assert!(
        village::market::input_cost(app.world(), catalog::CARVE).unwrap()
            > village::market::unit_price(app.world(), catalog::BOWL).unwrap()
    );
    let initial = money(&mut app);
    let tanner_id = *app.world().get::<SimId>(tanner).unwrap();
    npc_framework::kernel::journal::set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<npc_framework::crafting::Crafting>(carpenter)
            .expect("a ready, funded bowl job must not be refused on sunk book value")
            .recipe,
        catalog::CARVE
    );
    run_ticks(app.world_mut(), 1000);
    assert_eq!(economy::count(app.world(), carpenter, "plank"), 0);
    assert!(
        app.world()
            .resource::<village::VillageState>()
            .produced
            .get(catalog::BOWL)
            .copied()
            .unwrap_or(0)
            > 0
    );
    let sold=app.world().resource::<npc_framework::kernel::journal::Journal>().entries.iter().any(|entry| {
        entry.unit==Some(tanner_id) && matches!(&entry.kind,npc_framework::kernel::journal::JournalKind::Effect{effect,applied:true}
            if effect.get("type").and_then(serde_json::Value::as_str)==Some("VillageTrade")
                && effect.get("kind").and_then(serde_json::Value::as_str)==Some(catalog::BOWL))
    });
    assert!(
        sold,
        "the actual tanning customer must physically purchase the completed bowl"
    );
    assert_eq!(money(&mut app), initial);
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
fn hunter_job_spends_the_same_physical_spear_and_yields_the_same_goods_as_a_player() {
    let mut app = scene();
    let hunter = actor(&app, "Rhea");
    clear(&mut app, hunter);
    economy::grant(app.world_mut(), hunter, Item::new(catalog::SPEAR));
    let spear = items::carried_matching(app.world(), hunter, catalog::SPEAR).unwrap();
    let (_, prey, pos) = village::hunting::available_prey(app.world())[0];
    app.world_mut().entity_mut(hunter).insert(pos);
    let id = *app.world().get::<SimId>(hunter).unwrap();
    let now = app.world().resource::<SimClock>().tick;
    app.insert_resource(production::ProductionPlan {
        refreshed_at: Some(now),
        procurement: Default::default(),
        production: [(id, [(catalog::HIDE.into(), 1)].into())].into(),
        ..Default::default()
    });
    run_ticks(app.world_mut(), 1);
    assert!(
        app.world()
            .get::<village::hunting::Hunting>(hunter)
            .is_some()
    );
    run_ticks(app.world_mut(), u64::from(village::hunting::HUNT_TICKS));
    assert!(app.world().resource::<SimIndex>().get(spear).is_none());
    assert!(
        app.world().get_entity(prey).is_err(),
        "the exact finite prey must be consumed"
    );
    assert_eq!(economy::count(app.world(), hunter, catalog::RAW_MEAT), 2);
    assert_eq!(economy::count(app.world(), hunter, catalog::HIDE), 1);
    assert_eq!(
        production::production_demand(app.world(), hunter, catalog::HIDE),
        0,
        "a fulfilled job must not repeat while waiting for the next planning round"
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

#[test]
fn autonomous_living_economy_operates_new_chains_through_winter_without_creating_coins() {
    let mut app = scene();
    let initial = money(&mut app);
    let mut trace = Vec::new();
    for _ in 0..72 {
        run_ticks(app.world_mut(), 1000);
        let baker = actor(&app, "Mira");
        trace.push(serde_json::json!({"tick":app.world().resource::<SimClock>().tick,
            "job":app.world().get::<production::JobIntent>(baker),
            "position":app.world().get::<SimPos>(baker),
            "coins":app.world().get::<Wallet>(baker).unwrap().coins,
            "needs":{"hunger":economy::need(app.world(),baker,"Hunger"),"thirst":economy::need(app.world(),baker,"Thirst")},
            "stock":([catalog::RAW_MEAT,catalog::FLOUR,"log",catalog::WATER,"bread",catalog::COOKED_MEAL].into_iter()
                .map(|kind| (kind,economy::count(app.world(),baker,kind))).collect::<std::collections::BTreeMap<_,_>>()),
            "make_meal":production::production_demand(app.world(),baker,catalog::COOKED_MEAL),
            "make_bread":production::production_demand(app.world(),baker,"bread"),
            "buy_raw":production::desired_stock(app.world(),baker,catalog::RAW_MEAT)}));
    }
    let dead: Vec<_> = residents(&app)
        .into_iter()
        .filter_map(|e| {
            let vitality = app.world().get::<village::Vitality>(e)?;
            vitality
                .died_at
                .map(|tick| (app.world().get::<Villager>(e).unwrap().name.clone(), tick))
        })
        .collect();
    assert!(
        dead.is_empty(),
        "residents failed the unaided winter fixture: {dead:?}"
    );
    let state = app.world().resource::<village::VillageState>();
    if state
        .produced
        .get(catalog::COOKED_MEAL)
        .copied()
        .unwrap_or(0)
        == 0
    {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/living-meal-before-fix.json"
            ),
            serde_json::to_vec(&trace).unwrap(),
        )
        .unwrap();
    }
    for kind in [
        catalog::RAW_MEAT,
        catalog::HIDE,
        catalog::SPEAR,
        catalog::COOKED_MEAL,
        catalog::LEATHER,
        catalog::FIBER,
        catalog::THREAD,
        catalog::WARM_CLOTHES,
    ] {
        assert!(
            state.produced.get(kind).copied().unwrap_or(0) > 0,
            "the living economy never completed {kind}: {:?}",
            state.produced
        );
    }
    assert!(
        state.transactions > 30,
        "production must participate in physical trade"
    );
    assert_eq!(money(&mut app), initial);
}

#[test]
fn a_priced_out_market_recovers_physical_clothing_production_and_sales_when_work_resumes() {
    let mut app = scene();
    let initial = money(&mut app);
    let brains: Vec<_> = residents(&app)
        .into_iter()
        .map(|e| (e, *app.world().get::<Brain>(e).unwrap()))
        .collect();
    for (e, _) in &brains {
        app.world_mut().entity_mut(*e).remove::<Brain>();
    }
    let inventory: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| app.world().get::<Item>(*e).map(|item| (*id, item.clone())))
        .collect();
    let wallets: Vec<_> = residents(&app)
        .into_iter()
        .map(|e| (e, app.world().get::<Wallet>(e).unwrap().coins))
        .collect();
    let mut constrained = false;
    // Declared operator prefix: hold all work/survival and advance only the
    // normal planning/market rounds. No goods, cash or prices are edited.
    for _ in 0..100 {
        production::refresh(app.world_mut());
        village::market::tick(app.world_mut());
        if let Some(sample) = app
            .world()
            .resource::<village::market::MarketState>()
            .commodities
            .get(catalog::WARM_CLOTHES)
            .and_then(|row| row.history.last())
        {
            constrained |= sample.supply == 0 && sample.below_reference_bid_units > 0;
        }
        let step = village::market::round_ticks(app.world());
        app.world_mut().resource_mut::<SimClock>().tick += step;
    }
    assert!(
        constrained,
        "the natural held-supply prefix must exercise a real affordability boundary"
    );
    assert_eq!(money(&mut app), initial);
    for (e, cash) in wallets {
        assert_eq!(app.world().get::<Wallet>(e).unwrap().coins, cash);
    }
    let after: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| app.world().get::<Item>(*e).map(|item| (*id, item.clone())))
        .collect();
    assert_eq!(
        after, inventory,
        "the prefix must not smuggle in a recovery supply"
    );
    let made = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .get(catalog::WARM_CLOTHES)
        .copied()
        .unwrap_or(0);
    let sold = app
        .world()
        .resource::<village::market::MarketState>()
        .commodities
        .get(catalog::WARM_CLOTHES)
        .map_or(0, |row| row.total_units);
    let before = finance_diagnostic(&app);
    for (e, brain) in brains {
        app.world_mut().entity_mut(e).insert(brain);
    }
    run_ticks(app.world_mut(), 72_000);
    let now_made = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .get(catalog::WARM_CLOTHES)
        .copied()
        .unwrap_or(0);
    let now_sold = app
        .world()
        .resource::<village::market::MarketState>()
        .commodities
        .get(catalog::WARM_CLOTHES)
        .map_or(0, |row| row.total_units);
    if now_made == made || now_sold == sold {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/living-recovery-finance-before-fix.json"
            ),
            serde_json::to_vec_pretty(
                &serde_json::json!({"before":before,"after":finance_diagnostic(&app)}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    assert!(
        now_made > made,
        "affordable beliefs must recover real production, not only plan counters"
    );
    assert!(
        now_sold > sold,
        "a customer must physically buy clothing after work resumes"
    );
    let dead: Vec<_> = residents(&app)
        .into_iter()
        .filter_map(|e| {
            app.world()
                .get::<village::Vitality>(e)
                .unwrap()
                .died_at
                .map(|tick| (app.world().get::<Villager>(e).unwrap().name.clone(), tick))
        })
        .collect();
    assert!(dead.is_empty(), "recovery starved residents: {dead:?}");
    assert_eq!(money(&mut app), initial);
}
