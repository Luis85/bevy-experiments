use npc_framework::agent_io::api;
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{
    self, DiaryPage, DroppedDiary, PublishDiary, ResourcePatch, Role, VillageAct, VillageState,
    Villager, Vitality, Wallet,
};

fn scene() -> bevy_app::App {
    testbed_content::new_sim_app(7, "ai_village")
}

#[test]
fn polish_collecting_a_carried_purse_removes_its_inventory_slot() {
    let mut app = scene();
    let actor = id(&app, "human");
    let holder = entity(&app, actor);
    let (purse, item) = spawn_sim(
        app.world_mut(),
        (
            Item::new(village::catalog::COINS),
            village::CoinPurse { coins: 7 },
        ),
    );
    npc_framework::interaction::items::put_in_inventory(app.world_mut(), holder, item);
    let total = money(&mut app);
    let mut collect = VillageAct::new(actor, "collect_coins");
    collect.target = Some(purse);
    collect.perform(app.world_mut()).unwrap();
    assert_eq!(app.world().get::<Wallet>(holder).unwrap().coins, 7);
    assert!(
        !app.world()
            .get::<Inventory>(holder)
            .unwrap()
            .contains(purse),
        "redeeming carried coins must not leave a ghost inventory slot"
    );
    assert_eq!(money(&mut app), total);
}

fn polish_multi_preset_scene() -> bevy_app::App {
    let doc = testbed_content::scenario::scenario_doc("ai_village").unwrap();
    testbed_content::scenario::load_with(&doc, 7, |app| {
        let human = testbed_content::actors::human_preset();
        let mut other = human.clone();
        other.speed_mm_per_tick = 99;
        app.insert_resource(testbed_content::actors::ActorPresets(
            std::collections::BTreeMap::from([
                ("aaa_other".into(), other),
                ("human".into(), human),
            ]),
        ));
    })
    .unwrap()
    .app
}

#[test]
fn polish_scheduler_status_reflects_live_eligibility_without_dispatch_or_simulation_changes() {
    use npc_framework::agent_io::scheduler::{
        self, SchedulerAgentStatus, SchedulerExclusions, SchedulerPolicy, SchedulerStatus,
    };
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let actor = id(&app, "human");
    app.insert_resource(SimControl {
        paused: true,
        ..Default::default()
    });
    app.insert_resource(SchedulerStatus {
        enabled: true,
        events_enabled: true,
        error: None,
        agents: vec![SchedulerAgentStatus {
            controller: "visitor".into(),
            unit: actor.0,
            state: "ready".into(),
            idle_seconds: 123,
            nudges_last_hour: 0,
            last_result: None,
            pending_events: 0,
            event_overflow: 0,
            trigger: None,
            last_delivery_seq: None,
        }],
    });
    let before = state_hash(app.world_mut());
    let status = scheduler::read_status(app.world_mut());
    assert_eq!(
        status.agents[0].state, "game paused",
        "cached ready status must not hide a live pause"
    );
    app.world_mut().resource_mut::<SchedulerPolicy>().enabled = false;
    assert_eq!(
        scheduler::read_status(app.world_mut()).agents[0].state,
        "disabled"
    );
    app.world_mut().resource_mut::<SchedulerPolicy>().enabled = true;
    app.world_mut().resource_mut::<SimControl>().paused = false;
    app.insert_resource(SchedulerExclusions(std::collections::BTreeMap::from([(
        actor,
        "actor is permanently dead".into(),
    )])));
    assert_eq!(
        scheduler::read_status(app.world_mut()).agents[0].state,
        "actor is permanently dead"
    );
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn polish_successor_uses_the_scenario_preset_instead_of_the_first_registered_one() {
    let mut app = polish_multi_preset_scene();
    let old = id(&app, "human");
    let body = entity(&app, old);
    let speed = app.world().get::<Villager>(body).unwrap().base_speed;
    app.world_mut().get_mut::<Vitality>(body).unwrap().died_at = Some(0);
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&VillageAct::new(old, "new_life")),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let successor = app.world().resource::<VillageState>().played;
    assert_ne!(successor, old);
    assert_eq!(
        app.world()
            .get::<Villager>(entity(&app, successor))
            .unwrap()
            .base_speed,
        speed,
        "an unrelated earlier registry entry must not change a successor's body"
    );
}

#[test]
fn polish_future_successor_keeps_its_preset_across_snapshot_resume() {
    fn factory() -> bevy_app::App {
        let mut app = polish_multi_preset_scene();
        let actor = id(&app, "human");
        let body = entity(&app, actor);
        app.world_mut().get_mut::<Vitality>(body).unwrap().died_at = Some(0);
        submit(
            app.world_mut(),
            Some(500),
            SimCommand::content(&VillageAct::new(actor, "new_life")),
        )
        .unwrap();
        app
    }
    npc_framework::tools::snapshot_check::assert_resume_equivalent(factory, 400, 800, 40);
}

#[test]
fn village_preset_upgrade_preserves_broken_installations_and_user_owned_files() {
    let dir = std::env::temp_dir().join(format!("island-presets-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("invalid.json"), b"unfinished JSON").unwrap();
    testbed_content::worlds::install_builtins(&dir).unwrap();
    let island = dir.join("island_village.json");
    assert!(
        !island.exists(),
        "invalid-only folders must remain invalid instead of silently acquiring a valid world"
    );
    let plain = testbed_content::worlds::BUILTIN_PRESETS[0].1;
    std::fs::write(dir.join("plain_explorer.json"), plain).unwrap();
    testbed_content::worlds::install_builtins(&dir).unwrap();
    assert!(
        island.is_file(),
        "usable existing installations receive the new scene"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("plain_explorer.json")).unwrap(),
        plain
    );
    let mut custom = testbed_content::worlds::builtin_preset("island_village").unwrap();
    custom.name = "My island".into();
    let custom = serde_json::to_vec(&custom).unwrap();
    std::fs::write(&island, &custom).unwrap();
    testbed_content::worlds::install_builtins(&dir).unwrap();
    assert_eq!(std::fs::read(island).unwrap(), custom);
    let resolved = dir.canonicalize().unwrap();
    assert_eq!(
        resolved.parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    std::fs::remove_dir_all(resolved).unwrap();
}
fn id(app: &bevy_app::App, name: &str) -> SimId {
    app.world()
        .resource::<testbed_content::TestbedIds>()
        .id(name)
}
fn entity(app: &bevy_app::App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn money(app: &mut bevy_app::App) -> u64 {
    let wallets: u64 = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|w| u64::from(w.coins))
        .sum();
    let dropped: u64 = app
        .world_mut()
        .query::<&village::CoinPurse>()
        .iter(app.world())
        .map(|w| u64::from(w.coins))
        .sum();
    wallets + dropped + u64::from(app.world().resource::<VillageState>().fund)
}
fn act(app: &mut bevy_app::App, unit: SimId, mut args: Value) {
    args["unit"] = json!(unit.0);
    args["controller"] = json!("visitor");
    let reply = api::call(app.world_mut(), "agent/act", Some(args)).unwrap();
    assert_eq!(reply["accepted"], true, "{reply}");
}
fn walk(app: &mut bevy_app::App, unit: SimId, pos: SimPos) {
    act(
        app,
        unit,
        json!({"steps":[{"verb":"order","args":{"order":{"type":"Move","pos":[pos.x,pos.z]}}},{"verb":"execute_orders"}]}),
    );
    run_ticks(app.world_mut(), 900);
    let here = *app.world().get::<SimPos>(entity(app, unit)).unwrap();
    assert!(here.dist2(pos) <= 2_250_000, "{here:?} -> {pos:?}");
}

#[test]
fn island_scene_has_living_residents_and_no_free_recovery_stations() {
    let mut app =
        testbed_content::try_new_sim_app(7, "ai_village", 0).expect("new AI village scene");
    let actor = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("human");
    let entity = app.world().resource::<SimIndex>().get(actor).unwrap();
    let registry = app.world().resource::<NeedRegistry>();
    for name in ["Hunger", "Thirst", "Energy"] {
        let id = registry.id(name).expect("basic human need");
        assert_eq!(
            app.world().get::<Needs>(entity).unwrap().get(id).value,
            100_000
        );
    }
    assert_eq!(
        app.world_mut()
            .query::<&testbed_content::world_runtime::RecoverySource>()
            .iter(app.world())
            .count(),
        0
    );
    run_ticks(app.world_mut(), 100);
    assert!(
        app.world()
            .resource::<testbed_content::TestbedIds>()
            .units
            .len()
            >= 10
    );
}

#[test]
fn an_agent_earns_coins_and_buys_all_three_needs_through_the_action_api() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let u = id(&app, "human");
    let e = entity(&app, u);
    let total = money(&mut app);
    assert_eq!(app.world().get::<Wallet>(e).unwrap().coins, 0);
    let patch = id(&app, "patch_log");
    let target = *app.world().get::<SimPos>(entity(&app, patch)).unwrap();
    walk(&mut app, u, target);
    for _ in 0..4 {
        act(&mut app, u, json!({"verb":"gather","target":patch.0}));
        run_ticks(app.world_mut(), 260);
    }
    assert_eq!(village::economy::count(app.world(), e, "log"), 4);
    let inn = id(&app, "Tess");
    let pos = *app.world().get::<SimPos>(entity(&app, inn)).unwrap();
    walk(&mut app, u, pos);
    act(
        &mut app,
        u,
        json!({"verb":"sell","target":inn.0,"args":{"kind":"log","quantity":4}}),
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<Wallet>(e).unwrap().coins, 12);
    let registry = app.world().resource::<NeedRegistry>();
    let ids = [
        registry.id("Hunger").unwrap(),
        registry.id("Thirst").unwrap(),
        registry.id("Energy").unwrap(),
    ];
    for need in ids {
        app.world_mut()
            .get_mut::<Needs>(e)
            .unwrap()
            .get_mut(need)
            .value = 40_000;
    }
    for (name, kind, verb) in [
        ("Mira", "bread", "eat"),
        ("Nell", village::catalog::WATER, "drink"),
    ] {
        let trader = id(&app, name);
        let pos = *app.world().get::<SimPos>(entity(&app, trader)).unwrap();
        walk(&mut app, u, pos);
        act(
            &mut app,
            u,
            json!({"verb":"buy","target":trader.0,"args":{"kind":kind}}),
        );
        run_ticks(app.world_mut(), 1);
        act(&mut app, u, json!({"verb":verb}));
        run_ticks(app.world_mut(), 1);
    }
    let pos = *app.world().get::<SimPos>(entity(&app, inn)).unwrap();
    walk(&mut app, u, pos);
    act(&mut app, u, json!({"verb":"rest","target":inn.0}));
    run_ticks(app.world_mut(), 302);
    assert!(app.world().get::<Needs>(e).unwrap().get(ids[2]).value > 95_000);
    assert_eq!(app.world().get::<Wallet>(e).unwrap().coins, 2);
    assert_eq!(money(&mut app), total, "trade and lodging conserve money");
}

#[test]
fn failed_trade_is_atomic_and_cannot_sell_claimed_or_unowned_items() {
    let mut app = scene();
    let u = id(&app, "human");
    let buyer = entity(&app, u);
    let baker = entity(&app, id(&app, "Mira"));
    let pos = *app.world().get::<SimPos>(baker).unwrap();
    app.world_mut().entity_mut(buyer).insert(pos);
    let before = state_hash(app.world_mut());
    assert!(village::economy::trade(app.world_mut(), buyer, baker, "bread", 1).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    app.world_mut().get_mut::<Wallet>(baker).unwrap().coins -= 20;
    app.world_mut().get_mut::<Wallet>(buyer).unwrap().coins = 20;
    app.world_mut()
        .get_mut::<Inventory>(buyer)
        .unwrap()
        .max_weight_g = 0;
    let before = state_hash(app.world_mut());
    assert!(village::economy::trade(app.world_mut(), buyer, baker, "bread", 1).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(village::economy::consume(app.world_mut(), buyer, "log", 1).is_err());
    assert!(
        village::economy::merchant_check(app.world(), buyer, baker, "village_diary", 1, false)
            .is_err()
    );
}

#[test]
fn a_poor_merchants_refusal_does_not_reveal_their_private_wallet() {
    let mut app = scene();
    let actor = entity(&app, id(&app, "human"));
    let trader = entity(&app, id(&app, "Tess"));
    let pos = *app.world().get::<SimPos>(trader).unwrap();
    // Labeled refusal fixture: position and finite coins are arranged by the operator.
    app.world_mut().entity_mut(actor).insert(pos);
    app.world_mut().get_mut::<Wallet>(trader).unwrap().coins = 2;
    village::economy::grant(app.world_mut(), actor, Item::new("log"));
    let before = state_hash(app.world_mut());
    let reason =
        village::economy::merchant_check(app.world(), actor, trader, "log", 1, false).unwrap_err();
    assert!(
        reason.contains("3 coins"),
        "public cost should be actionable: {reason}"
    );
    assert!(
        !reason.contains("has 2"),
        "merchant wallet leaked: {reason}"
    );
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn distant_sell_refusals_do_not_reveal_a_merchants_current_stock() {
    let mut app = scene();
    let unit = id(&app, "human");
    let actor = entity(&app, unit);
    let merchant = id(&app, "Tess");
    let trader = entity(&app, merchant);
    assert!(!village::economy::near(app.world(), actor, trader));
    let mut sell = VillageAct::new(unit, "sell");
    sell.target = Some(merchant);
    sell.kind = "log".into();
    let before = sell.check(app.world()).unwrap_err();
    assert!(before.contains("within 1.5 m"), "{before}");
    // Operator-only privacy fixture: fill the distant shop without moving the
    // actor. An attempted sale must not reveal whether unseen stock changed.
    let existing = village::economy::count(app.world(), trader, "log");
    let capacity = village::catalog::stock_limit("log");
    village::economy::grant(
        app.world_mut(),
        trader,
        Item::new("log").stack(capacity - existing),
    );
    let hash = state_hash(app.world_mut());
    let after = sell.check(app.world()).unwrap_err();
    assert_eq!(
        after, before,
        "distant refusal disclosed current merchant stock"
    );
    assert_eq!(
        state_hash(app.world_mut()),
        hash,
        "validation must be read-only"
    );
}

#[test]
fn stopped_crafting_releases_claims_without_consuming_or_duplicating_inputs() {
    use npc_framework::crafting::{CraftSource, Crafting, begin_craft};
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let unit = id(&app, "human");
    let actor = entity(&app, unit);
    let station = id(&app, &format!("village_{}", Role::Carpenter.tag()));
    let pos = *app.world().get::<SimPos>(entity(&app, station)).unwrap();
    // Labeled interruption fixture: arrange one input and proximity, never claim
    // this privileged unit setup as a first-income or registered-client run.
    app.world_mut().entity_mut(actor).insert(pos);
    village::economy::grant(app.world_mut(), actor, Item::new("log"));
    let total = money(&mut app);
    for _ in 0..3 {
        begin_craft(
            app.world_mut(),
            actor,
            village::catalog::SAW,
            Some(station),
            CraftSource::Command,
            None,
        )
        .unwrap();
        let craft = app.world().get::<Crafting>(actor).unwrap().clone();
        assert!(!craft.claimed.is_empty());
        assert_eq!(
            village::economy::count(app.world(), actor, "log"),
            0,
            "claimed input is unavailable for a concurrent sale"
        );
        act(&mut app, unit, json!({"verb":"stop_work"}));
        run_ticks(app.world_mut(), 1);
        assert!(app.world().get::<Crafting>(actor).is_none());
        for claim in craft.claimed {
            assert_eq!(app.world().resource::<Reservations>().count(claim), 0);
        }
        assert_eq!(village::economy::count(app.world(), actor, "log"), 1);
    }
    run_ticks(app.world_mut(), 200);
    assert_eq!(village::economy::count(app.world(), actor, "log"), 1);
    assert_eq!(village::economy::count(app.world(), actor, "plank"), 0);
    assert_eq!(money(&mut app), total);
}

#[test]
fn water_is_blocked_even_when_an_agent_orders_a_path_into_it() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let u = id(&app, "human");
    act(
        &mut app,
        u,
        json!({"steps":[{"verb":"order","args":{"order":{"type":"Move","pos":[30000,0]}}},{"verb":"execute_orders"}]}),
    );
    run_ticks(app.world_mut(), 1500);
    let pos = *app.world().get::<SimPos>(entity(&app, u)).unwrap();
    let cell = app.world().resource::<SpatialGrid>().cell_of(pos);
    assert!(village::land_cell(cell.x, cell.z), "{pos:?}");
    assert!(pos.dist2(SimPos::new(30000, 0)) > 1_000_000);
}

#[test]
fn death_drops_one_diary_and_successor_keeps_the_village_with_a_new_identity() {
    let mut app = scene();
    let u = id(&app, "human");
    let e = entity(&app, u);
    let mut write = VillageAct::new(u, "write_diary");
    write.title = "My first day".into();
    write.body = "I earned my bread at the timber yard.".into();
    submit(app.world_mut(), None, SimCommand::content(&write)).unwrap();
    run_ticks(app.world_mut(), 1);
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(thirst)
        .value = 0;
    app.world_mut().get_mut::<Vitality>(e).unwrap().health = 1;
    run_ticks(app.world_mut(), 605);
    assert!(app.world().get::<Vitality>(e).unwrap().died_at.is_some());
    let books: Vec<_> = app
        .world_mut()
        .query::<(&SimId, &DroppedDiary)>()
        .iter(app.world())
        .map(|(id, d)| (*id, d.clone()))
        .collect();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].1.pages[0].title, "My first day");
    let total = money(&mut app);
    let clock = app.world().resource::<SimClock>().tick;
    village::host::frame(app.world_mut());
    run_ticks(app.world_mut(), 1);
    let birth = VillageAct::new(u, "new_life");
    submit(app.world_mut(), None, SimCommand::content(&birth)).unwrap();
    run_ticks(app.world_mut(), 1);
    let successor = app.world().resource::<VillageState>().played;
    assert_ne!(successor, u);
    assert_eq!(id(&app, "human"), successor);
    assert_eq!(app.world().resource::<VillageState>().life, 2);
    assert!(app.world().resource::<SimClock>().tick > clock);
    assert!(app.world().get::<Vitality>(e).unwrap().died_at.is_some());
    assert!(VillageAct::new(u, "eat").check(app.world()).is_err());
    let pick = npc_framework::interaction::Interact::new(successor, "pick_up", books[0].0);
    submit(app.world_mut(), None, SimCommand::content(&pick)).unwrap();
    run_ticks(app.world_mut(), 40);
    let be = entity(&app, books[0].0);
    assert_eq!(
        app.world()
            .get::<npc_framework::interaction::CarriedBy>(be)
            .unwrap()
            .holder,
        successor
    );
    let mut read = VillageAct::new(successor, "read_diary");
    read.target = Some(books[0].0);
    assert!(read.check(app.world()).is_ok());
    read.page = 0;
    assert!(read.check(app.world()).is_err());
    read.page = 2;
    assert!(read.check(app.world()).is_err());
    read.page = 1;
    submit(app.world_mut(), None, SimCommand::content(&read)).unwrap();
    run_ticks(app.world_mut(), 1);
    let observed = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit":successor.0})),
    )
    .unwrap();
    let observed = observed.to_string();
    assert!(
        !observed.contains("I earned my bread"),
        "observation must not repeatedly inline recovered diary text"
    );
    assert_eq!(money(&mut app), total);
    run_ticks(app.world_mut(), 500);
    assert_eq!(
        app.world_mut()
            .query::<&DroppedDiary>()
            .iter(app.world())
            .count(),
        1
    );
}

#[test]
fn economy_and_diary_resume_with_identical_hashes() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 400, 2400, 40);
}

#[test]
fn villagers_complete_every_production_stage_and_survive_a_short_year() {
    let mut preset = testbed_content::worlds::builtin_preset("island_village").unwrap();
    preset.day_night.cycle_ticks = 2400;
    preset.needs.hunger.rise_milli_per_tick = 20;
    preset.needs.thirst.rise_milli_per_tick = 30;
    preset.needs.energy.rise_milli_per_tick = 20;
    let doc = testbed_content::worlds::world_doc(&preset, &testbed_content::actors::human_preset())
        .unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 2, |_| {})
        .unwrap()
        .app;
    let total = money(&mut app);
    run_ticks(app.world_mut(), 2400 * 29);
    let living: Vec<_> = app
        .world_mut()
        .query::<(&Villager, &Vitality)>()
        .iter(app.world())
        .filter(|(v, _)| v.role != Role::Visitor)
        .map(|(v, h)| (v.role, h.died_at))
        .collect();
    assert_eq!(living.len(), 9);
    assert!(living.iter().all(|(_, d)| d.is_none()), "{living:?}");
    let state = app.world().resource::<VillageState>();
    for kind in [
        village::catalog::GRAIN,
        village::catalog::FLOUR,
        "bread",
        village::catalog::WATER,
        "log",
        "plank",
        village::catalog::BOWL,
        "ore",
        "ingot",
        "hammer",
    ] {
        assert!(
            state.produced.get(kind).copied().unwrap_or(0) > 0,
            "never produced {kind}: {:?}",
            state.produced
        );
    }
    assert!(state.transactions > 30);
    assert_eq!(money(&mut app), total);
    assert_eq!(village::calendar(app.world()).1, village::Season::Spring);
}

#[test]
fn traded_tools_keep_their_identity_quality_and_wear() {
    let mut app = scene();
    let buyer = entity(&app, id(&app, "human"));
    let seller = entity(&app, id(&app, "Ivo"));
    let pos = *app.world().get::<SimPos>(seller).unwrap();
    app.world_mut().entity_mut(buyer).insert(pos);
    app.world_mut().get_mut::<Wallet>(seller).unwrap().coins -= 20;
    app.world_mut().get_mut::<Wallet>(buyer).unwrap().coins = 20;
    let tool = village::economy::free_items(app.world(), seller)
        .into_iter()
        .find(|(_, i)| i.kind == "hammer")
        .unwrap()
        .0;
    let item = entity(&app, tool);
    app.world_mut()
        .entity_mut(item)
        .insert(npc_framework::crafting::Durability {
            current: 7,
            max: 50,
        });
    village::economy::trade(app.world_mut(), buyer, seller, "hammer", 1).unwrap();
    assert!(
        app.world().get::<Inventory>(buyer).unwrap().contains(tool),
        "trade must move the existing tool, not fabricate a replacement"
    );
    assert_eq!(
        app.world()
            .get::<npc_framework::crafting::Durability>(item)
            .unwrap()
            .current,
        7
    );
}

#[test]
fn hungry_baker_and_thirsty_well_keeper_restart_their_own_empty_supply() {
    let mut app = scene();
    for (name, good, need) in [
        ("Mira", "bread", "Hunger"),
        ("Nell", village::catalog::WATER, "Thirst"),
    ] {
        let e = entity(&app, id(&app, name));
        let n = village::economy::count(app.world(), e, good);
        village::economy::consume(app.world_mut(), e, good, n).unwrap();
        let need = app.world().resource::<NeedRegistry>().id(need).unwrap();
        app.world_mut()
            .get_mut::<Needs>(e)
            .unwrap()
            .get_mut(need)
            .value = 20_000;
        if name == "Mira" {
            for (kind, n) in [
                (village::catalog::FLOUR, 2),
                (village::catalog::WATER, 1),
                ("log", 1),
            ] {
                village::economy::grant(app.world_mut(), e, Item::new(kind).stack(n));
            }
        }
    }
    run_ticks(app.world_mut(), 600);
    for (name, need) in [("Mira", "Hunger"), ("Nell", "Thirst")] {
        let e = entity(&app, id(&app, name));
        assert!(
            village::economy::need(app.world(), e, need) > 50_000,
            "{name} waited for its own unavailable output"
        );
    }
}

#[test]
fn host_publishes_only_the_diary_and_never_private_books_or_credential_files() {
    use npc_framework::agent_io::notes::{Book, NewNote, NotesDirs, NotesStores, Stamp, Writer};
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "island-diary-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut app = scene();
    let u = id(&app, "human");
    let e = entity(&app, u);
    app.insert_resource(NotesDirs(BTreeMap::from([(u, dir.clone())])))
        .init_resource::<NotesStores>();
    for (book, title, body) in [
        (Book::Diary, "My journey", "The mill buys grain."),
        (
            Book::Memory,
            "Private knowledge",
            "not-for-the-physical-diary",
        ),
        (Book::Proposals, "A suggestion", "also-private"),
    ] {
        app.world_mut()
            .resource_mut::<NotesStores>()
            .store_for(&dir)
            .unwrap()
            .add(
                book,
                Writer::Player,
                NewNote {
                    title: title.into(),
                    body: body.into(),
                    prompted_by: None,
                },
                Stamp {
                    tick: 0,
                    date: "2026-10-28T12:00:00Z".into(),
                },
            )
            .unwrap();
    }
    std::fs::write(dir.join("AGENTS.md"), "PRIVATE-CREDENTIAL-FIXTURE").unwrap();
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(thirst)
        .value = 0;
    app.world_mut().get_mut::<Vitality>(e).unwrap().health = 1;
    run_ticks(app.world_mut(), 605);
    village::host::frame(app.world_mut());
    let queue = app
        .world()
        .resource::<npc_framework::kernel::commands::CommandQueue>();
    let command = queue
        .pending
        .values()
        .find(|c| c.kind() == "VillagePublishDiary")
        .expect("publication is recorded input");
    let text = serde_json::to_string(command).unwrap();
    assert!(text.contains("The mill buys grain."));
    assert!(!text.contains("not-for-the-physical-diary"));
    assert!(!text.contains("also-private"));
    assert!(!text.contains("PRIVATE-CREDENTIAL"));
    run_ticks(app.world_mut(), 1);
    let book = app
        .world_mut()
        .query::<&DroppedDiary>()
        .single(app.world())
        .unwrap()
        .clone();
    assert!(book.sealed);
    assert_eq!(book.pages.len(), 1);
    assert!(dir.join("AGENTS.md").is_file());
    assert_eq!(
        std::fs::read_to_string(dir.join("AGENTS.md")).unwrap(),
        "PRIVATE-CREDENTIAL-FIXTURE"
    );
    let path = dir.canonicalize().unwrap();
    assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    drop(app);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn publication_rejects_late_or_duplicate_pages_and_resource_stock_is_finite() {
    let mut app = scene();
    let u = id(&app, "human");
    let e = entity(&app, u);
    let patch = entity(&app, id(&app, "patch_log"));
    app.world_mut()
        .get_mut::<ResourcePatch>(patch)
        .unwrap()
        .remaining = 0;
    let mut gather = VillageAct::new(u, "gather");
    gather.target = app.world().get::<SimId>(patch).copied();
    assert!(gather.check(app.world()).is_err());
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(thirst)
        .value = 0;
    app.world_mut().get_mut::<Vitality>(e).unwrap().health = 1;
    run_ticks(app.world_mut(), 605);
    let (diary, death) = app
        .world_mut()
        .query::<(&SimId, &DroppedDiary)>()
        .single(app.world())
        .map(|(id, d)| (*id, d.died_at))
        .unwrap();
    let mut publish = PublishDiary {
        unit: u.0,
        diary,
        pages: vec![DiaryPage {
            tick: death + 1,
            title: "Too late".into(),
            body: "".into(),
        }],
    };
    assert!(publish.validate(app.world()).is_err());
    publish.pages[0].tick = death;
    publish.validate(app.world()).unwrap();
    publish.clone().apply(app.world_mut());
    assert!(publish.validate(app.world()).is_err());
}

fn host_scene(
    tag: &str,
) -> (
    bevy_app::App,
    testbed_content::explorer::host::HostArgs,
    testbed_content::explorer::home::AgentDir,
) {
    use testbed_content::explorer::host::{HostArgs, build_world};
    let home = std::env::temp_dir().join(format!("npc-island-host-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("Island".into()),
        resume: false,
        on_mismatch: None,
        preset: Some("human".into()),
        actors_dir: Some(home.join("actors")),
        world_preset: Some("island_village".into()),
        worlds_dir: Some(home.join("worlds")),
    };
    let (app, _, dir) = build_world(&args, 7, "2026-10-28T12:00:00Z", |_| {}).unwrap();
    (app, args, dir)
}
fn force_death(app: &mut bevy_app::App, u: SimId) {
    let e = entity(app, u);
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut()
        .get_mut::<Needs>(e)
        .unwrap()
        .get_mut(thirst)
        .value = 0;
    let mut v = app.world_mut().get_mut::<Vitality>(e).unwrap();
    v.health = 1;
    v.dehydrated_ticks = 601;
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<Vitality>(e).unwrap().died_at.is_some());
}
fn remove_owned_home(path: std::path::PathBuf) {
    let absolute = path.canonicalize().unwrap();
    assert!(absolute.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(absolute).unwrap();
}

#[test]
fn successor_revokes_the_old_registration_uses_current_credentials_and_starts_fresh_books() {
    use npc_framework::agent_io::notes::NotesDirs;
    use testbed_content::explorer::{
        home,
        host::{ExplorerSession, insert_notes},
    };
    let (mut app, args, dir) = host_scene("succession");
    let before = app.world().resource::<ExplorerSession>().meta.clone();
    let old = app.world().resource::<VillageState>().played;
    insert_notes(app.world_mut(), old, &dir, || "2026-10-28T12:00:00Z".into());
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":before.agent_id,"token":before.token})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    api::call(
        app.world_mut(),
        "agent/take_control",
        Some(json!({"session":session})),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    force_death(&mut app, old);
    let receipt = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"new_life"})),
    )
    .unwrap();
    assert_eq!(receipt["accepted"], true, "{receipt}");
    run_ticks(app.world_mut(), 1);
    let rotated = home::rotate_token(&dir).unwrap();
    village::host::frame(app.world_mut());
    let error = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap_err();
    assert_eq!(error.data.unwrap()["revoked"], true);
    let fresh = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":rotated.agent_id,"token":rotated.token})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    // Check a valid credential before the intentional bad-token attempt arms registration backoff.
    assert!(
        api::call(
            app.world_mut(),
            "agent/register",
            Some(json!({"agent_id":before.agent_id,"token":before.token}))
        )
        .is_err()
    );
    let next = app.world().resource::<VillageState>().played;
    assert_ne!(next, old);
    assert!(
        app.world()
            .get::<npc_framework::agent_io::ExternalController>(entity(&app, old))
            .is_none()
    );
    assert_eq!(
        app.world().resource::<NotesDirs>().0[&next],
        dir.dir.join("lives/2")
    );
    let entries = api::call(
        app.world_mut(),
        "agent/notes/list",
        Some(json!({"session":fresh,"book":"diary"})),
    )
    .unwrap();
    assert!(entries["notes"].as_array().unwrap().is_empty(), "{entries}");
    drop(app);
    remove_owned_home(args.home);
}

#[test]
fn a_saved_dead_actor_stays_dead_but_can_be_claimed_for_the_new_life_meta_action() {
    use testbed_content::explorer::host::{build_world, save_now};
    let (mut app, mut args, dir) = host_scene("dead-resume");
    let old = app.world().resource::<VillageState>().played;
    force_death(&mut app, old);
    save_now(app.world_mut(), &dir, "2026-10-28T12:00:00Z").unwrap();
    drop(app);
    args.resume = true;
    args.world = None;
    args.preset = None;
    args.world_preset = None;
    let (mut app, _, _) = build_world(&args, 8, "2026-10-28T12:00:01Z", |_| {}).unwrap();
    let e = entity(&app, old);
    assert!(app.world().get::<Vitality>(e).unwrap().died_at.is_some());
    assert!(
        app.world()
            .get::<npc_framework::perception::RevealsFog>(e)
            .is_none()
    );
    assert!(
        app.world()
            .get::<npc_framework::perception::ReachSense>(e)
            .is_none()
    );
    let meta = app
        .world()
        .resource::<testbed_content::explorer::host::ExplorerSession>()
        .meta
        .clone();
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":meta.agent_id,"token":meta.token})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    api::call(
        app.world_mut(),
        "agent/take_control",
        Some(json!({"session":session})),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let reply = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"new_life"})),
    )
    .unwrap();
    assert_eq!(reply["accepted"], true, "{reply}");
    run_ticks(app.world_mut(), 1);
    assert_ne!(app.world().resource::<VillageState>().played, old);
    drop(app);
    remove_owned_home(args.home);
}

#[test]
fn supervisor_and_its_status_endpoint_never_advance_or_mutate_authoritative_state() {
    let mut app = scene();
    run_ticks(app.world_mut(), 1);
    let before = state_hash(app.world_mut());
    let tick = app.world().resource::<SimClock>().tick;
    npc_framework::agent_io::scheduler::configure_host(
        app.world_mut(),
        std::env::temp_dir()
            .join(format!("npc-empty-scheduler-{}", std::process::id()))
            .join("missing.json"),
    );
    npc_framework::agent_io::scheduler::frame(app.world_mut());
    let _ = api::call(app.world_mut(), "scheduler/status", None).unwrap();
    assert_eq!(state_hash(app.world_mut()), before);
    assert_eq!(app.world().resource::<SimClock>().tick, tick);
}
