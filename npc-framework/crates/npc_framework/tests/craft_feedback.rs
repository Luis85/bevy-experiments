//! Craft completion feedback crosses crafting, interaction and agent I/O.
#![cfg(all(feature = "agent-io", feature = "crafting"))]
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{
    api,
    control::{ControlMode, IdlePolicy, attach_now},
};
use npc_framework::crafting::{CraftSource, begin_craft};
use npc_framework::interaction::items::{put_in_inventory, put_in_slot};
use npc_framework::prelude::*;
use serde_json::json;

fn fixture(journal: bool) -> (App, SimId, Entity) {
    let mut app = App::new();
    app.add_plugins(
        NpcPlugins::new()
            .seed(1)
            .without_driver()
            .with_crafting()
            .with_agent_io(),
    )
    .register_item_kind(ItemKindDef::new("wood").weight_g(500).stackable(50))
    .register_item_kind(ItemKindDef::new("plank").weight_g(300).stackable(50))
    .register_item_kind(
        ItemKindDef::new("hammer")
            .weight_g(800)
            .equip_slot(npc_framework::interaction::MAIN_HAND),
    )
    .register_recipe(
        RecipeDef::new("splinter")
            .input("wood", 1)
            .output("plank", 1),
    );
    app.finish();
    app.cleanup();
    let (unit, actor) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            Interactor,
            Inventory::new(2, 10000),
            Equipment::hands(),
        ),
    );
    let (_, wood) = spawn_sim(app.world_mut(), Item::new("wood").stack(2));
    put_in_inventory(app.world_mut(), actor, wood);
    let (_, hammer) = spawn_sim(app.world_mut(), Item::new("hammer"));
    put_in_slot(
        app.world_mut(),
        actor,
        hammer,
        npc_framework::interaction::MAIN_HAND,
    );
    attach_now(
        app.world_mut(),
        actor,
        "craft-test",
        ControlMode::Advisor,
        IdlePolicy::Idle,
    );
    set_journal(app.world_mut(), journal, false);
    (app, unit, actor)
}

#[test]
fn crafted_fit_output_is_reported_as_inventory_gained() {
    let (mut app, unit, actor) = fixture(true);
    let before = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit":unit.0,"events":0})),
    )
    .unwrap();
    begin_craft(
        app.world_mut(),
        actor,
        "splinter",
        None,
        CraftSource::Command,
        None,
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let after = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit":unit.0,"since_seq":before["seq"],"events":100})),
    )
    .unwrap();
    let gained = after["changes"]["inventory_gained"].as_array().unwrap();
    assert_eq!(gained.len(), 1, "{after}");
    assert_eq!(gained[0]["kind"], "plank");
    assert_eq!(gained[0]["stack"], 1);
    assert_eq!(gained[0]["from"], json!(null));
    let output = npc_framework::kernel::SimId(gained[0]["item"].as_u64().unwrap() as u32);
    assert!(
        app.world()
            .get::<Inventory>(actor)
            .unwrap()
            .contains(output)
    );
    assert_eq!(app.world().get::<Inventory>(actor).unwrap().free_slots(), 0);
}

#[test]
fn crafted_feedback_does_not_change_state_when_journal_is_disabled() {
    let result = |journal| {
        let (mut app, _, actor) = fixture(journal);
        begin_craft(
            app.world_mut(),
            actor,
            "splinter",
            None,
            CraftSource::Command,
            None,
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        state_hash(app.world_mut())
    };
    assert_eq!(result(true), result(false));
}

#[cfg(feature = "command")]
#[test]
fn extreme_agent_move_is_rejected_without_submitting_inputs() {
    use npc_framework::kernel::commands::{CommandLog, CommandQueue};
    let mut app = App::new();
    app.add_plugins(
        NpcPlugins::new()
            .without_driver()
            .with_command()
            .with_agent_io(),
    );
    app.finish();
    app.cleanup();
    let (unit, actor) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), Commandable::new(70), Owner(PlayerId(1))),
    );
    attach_now(
        app.world_mut(),
        actor,
        "test",
        ControlMode::Advisor,
        IdlePolicy::Idle,
    );
    for value in [i32::MIN, i32::MAX] {
        let result=api::call(app.world_mut(),"agent/act",Some(json!({"unit":unit.0,"controller":"test","verb":"order","args":{"order":{"type":"Move","pos":[value,0]}}}))).unwrap();
        assert_eq!(result["accepted"], false, "{result}");
        assert_eq!(result["reason"], "unavailable");
        assert!(result["message"].as_str().unwrap().contains("outside"));
        assert!(app.world().resource::<CommandQueue>().pending.is_empty());
        assert!(app.world().resource::<CommandLog>().entries.is_empty());
    }
}
