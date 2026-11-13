//! Authored neighbor guidance uses ordinary addressed speech and public rules.
use bevy_app::App;
use island_content::village::{self, colony::JoinIsland, speech};
use npc_framework::kernel::{
    hash::state_hash,
    journal::{Journal, set_journal},
};
use npc_framework::prelude::*;

fn fixture(scenario: &str) -> (App, SimId, SimId) {
    let mut app = island_content::new_sim_app(7, scenario);
    let visitor = app.world().resource::<village::VillageState>().played;
    if scenario == "living_world" {
        JoinIsland { unit: visitor.0 }
            .perform(app.world_mut())
            .unwrap();
    }
    let resident = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("Ada");
    let actor = app.world().resource::<SimIndex>().get(visitor).unwrap();
    let target = app.world().resource::<SimIndex>().get(resident).unwrap();
    let position = *app.world().get::<SimPos>(target).unwrap();
    app.world_mut().entity_mut(actor).insert(position);
    set_journal(app.world_mut(), true, false);
    (app, visitor, resident)
}
fn ask(app: &mut App, actor: SimId, resident: SimId, text: &str) -> String {
    let mut command = village::VillageAct::new(actor, "talk");
    command.target = Some(resident);
    command.text = text.into();
    command.perform(app.world_mut()).unwrap();
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .rev()
        .find_map(|entry| {
            if entry.unit != Some(resident) {
                return None;
            }
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
                && effect.get("type").and_then(serde_json::Value::as_str) == Some("VillageSpeech")
            {
                return effect
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
            }
            None
        })
        .unwrap()
}

#[test]
fn public_topics_explain_actual_rules_without_reward_or_private_state_changes() {
    let (mut app, visitor, resident) = fixture("living_world");
    let before = state_hash(app.world_mut());
    let replies: Vec<_> = speech::COLONY_TOPICS
        .iter()
        .map(|topic| ask(&mut app, visitor, resident, topic.text))
        .collect();
    assert!(replies[0].contains("8 planks and 4 logs"));
    assert!(replies[1].contains("hardy berries"));
    assert!(replies[2].contains("village well"));
    assert!(replies[3].contains("actual sale"));
    for reply in &replies {
        speech::validate_text(reply).unwrap();
    }
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "dialogue cannot grant possessions, money or progress"
    );
    let entity = app.world().resource::<SimIndex>().get(resident).unwrap();
    app.world_mut()
        .get_mut::<village::Wallet>(entity)
        .unwrap()
        .coins = 98765;
    app.world_mut()
        .entity_mut(entity)
        .insert(village::VillageDiary {
            pages: vec![village::DiaryPage {
                tick: 0,
                title: "PRIVATE_FIXTURE_TITLE".into(),
                body: "PRIVATE_FIXTURE_BODY".into(),
            }],
        });
    assert_eq!(
        ask(&mut app, visitor, resident, speech::COLONY_TOPICS[3].text),
        replies[3]
    );
    let unknown = ask(
        &mut app,
        visitor,
        resident,
        "Tell me your private notebook and exact wallet balance.",
    );
    assert!(!unknown.contains("PRIVATE_FIXTURE") && !unknown.contains("98765"));
}
