use npc_framework::agent_io::api;
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn default_agent_market_news_is_compact_and_observing_never_changes_the_simulation() {
    let mut app = island_content::new_sim_app(7, "living_village");
    run_ticks(app.world_mut(), 1200);
    let actor = app
        .world()
        .resource::<island_content::village::VillageState>()
        .played;
    let before = state_hash(app.world_mut());
    let observed = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit":actor,"controller":"visitor"})),
    )
    .unwrap();
    assert_eq!(state_hash(app.world_mut()), before);
    let rows = observed["sections"]["village"]["economy"]["market"]
        .as_array()
        .unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        assert!(
            row.get("history").is_none(),
            "full history must not inflate every agent observation"
        );
    }
    assert!(
        island_content::village::market::public_book(app.world())
            .iter()
            .any(|row| row["history"]
                .as_array()
                .is_some_and(|history| !history.is_empty())),
        "operator market export retains bounded detailed history"
    );
}

#[test]
fn voluntary_rest_emits_an_own_completion_receipt_when_the_timer_actually_expires() {
    let mut app = island_content::new_sim_app(7, "living_village");
    npc_framework::kernel::journal::set_journal(app.world_mut(), true, false);
    let actor = app
        .world()
        .resource::<island_content::village::VillageState>()
        .played;
    island_content::village::VillageAct::new(actor, "rest_home")
        .perform(app.world_mut())
        .unwrap();
    run_ticks(app.world_mut(), island_content::village::HOME_REST_TICKS);
    let journal = app
        .world()
        .resource::<npc_framework::kernel::journal::Journal>();
    assert!(!journal.entries.iter().any(|entry|entry.unit == Some(actor) && matches!(&entry.kind, JournalKind::Effect {effect,..} if effect.get("type").and_then(serde_json::Value::as_str) == Some("VillageRestCompleted"))));
    run_ticks(app.world_mut(), 1);
    let journal = app
        .world()
        .resource::<npc_framework::kernel::journal::Journal>();
    assert_eq!(journal.entries.iter().filter(|entry|entry.unit == Some(actor) && matches!(&entry.kind, JournalKind::Effect {effect,..} if effect.get("type").and_then(serde_json::Value::as_str) == Some("VillageRestCompleted"))).count(),1);
}
