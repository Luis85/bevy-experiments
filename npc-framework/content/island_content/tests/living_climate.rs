use island_content::{
    TestbedIds,
    village::{self, Role, VillageState},
};
use npc_framework::agent_io::api;
use npc_framework::crafting::Durability;
use npc_framework::interaction::items;
use npc_framework::needs::NeedRegistry;
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn living_world_adds_jobs_and_warmth_without_renumbering_incumbent_people() {
    let mut old = island_content::new_sim_app(7, "ai_village");
    let mut living = island_content::new_sim_app(7, "living_village");
    assert!(!village::needs_driven(old.world()));
    assert!(village::needs_driven(living.world()));
    for role in Role::RESIDENTS {
        assert_eq!(
            old.world().resource::<TestbedIds>().id(role.person()),
            living.world().resource::<TestbedIds>().id(role.person())
        );
    }
    assert!(
        old.world()
            .resource::<NeedRegistry>()
            .id("Warmth")
            .is_none()
    );
    assert!(
        living
            .world()
            .resource::<NeedRegistry>()
            .id("Warmth")
            .is_some()
    );
    let old_count = old
        .world_mut()
        .query::<&village::Villager>()
        .iter(old.world())
        .count();
    let count = living
        .world_mut()
        .query::<&village::Villager>()
        .iter(living.world())
        .count();
    assert_eq!(old_count, 10);
    assert_eq!(count, 13);
}

#[test]
fn a_carried_coat_does_not_protect_but_shared_equip_protects_and_wears_it() {
    let mut app = island_content::new_sim_app(7, "living_village");
    let actor = app.world().resource::<VillageState>().played;
    let body = app.world().resource::<SimIndex>().get(actor).unwrap();
    app.world_mut().resource_mut::<SimClock>().tick = 17_000;
    let (coat, item) = spawn_sim(
        app.world_mut(),
        (
            Item::new(village::catalog::WARM_CLOTHES),
            Durability::new(10_000),
        ),
    );
    items::put_in_inventory(app.world_mut(), body, item);
    run_ticks(app.world_mut(), 20);
    let cold = village::economy::need(app.world(), body, "Warmth");
    assert!(
        cold < 100_000,
        "carrying sale stock must not provide protection"
    );
    assert_eq!(app.world().get::<Durability>(item).unwrap().current, 10_000);
    let reply = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit":actor,"controller":"visitor","verb":"equip","target":coat})),
    )
    .unwrap();
    assert_eq!(reply["accepted"], true, "{reply}");
    run_ticks(app.world_mut(), 25);
    assert!(village::economy::need(app.world(), body, "Warmth") > cold);
    assert!(app.world().get::<Durability>(item).unwrap().current < 10_000);
}

#[test]
fn living_world_snapshot_keeps_climate_and_market_state() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(
        || island_content::new_sim_app(7, "living_village"),
        400,
        1600,
        40,
    );
}

#[test]
fn observer_calendar_matches_the_actual_late_autumn_start_and_winter_boundary() {
    let app = island_content::new_sim_app(7, "living_village");
    let config = app.world().resource::<SimConfig>();
    assert_eq!(config.day_length, 24_000);
    for (tick, expected) in [
        (0, (19, village::Season::Autumn, false)),
        (17_001, (19, village::Season::Autumn, true)),
        (48_000, (21, village::Season::Winter, false)),
    ] {
        assert_eq!(village::calendar_at(tick, config, true), expected);
    }
    assert_eq!(
        village::calendar_at(0, config, false),
        (0, village::Season::Spring, false),
        "the fixed island retains its original spring calendar"
    );
}
