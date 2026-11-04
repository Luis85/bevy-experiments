use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use testbed_content::{projection, village::VillageState};

fn person_bubbles(view: &serde_json::Value, actor: SimId) -> Vec<serde_json::Value> {
    view["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entity| entity["id"] == actor.0)
        .unwrap()["bubbles"]
        .as_array()
        .unwrap()
        .clone()
}

fn public_notice(app: &mut bevy_app::App, actor: SimId, seq: u64, tick: u64, verb: &str) {
    use npc_framework::agent_io::stats::{ActionNotice, ActionNotices};
    let epoch = npc_framework::agent_io::api::epoch(app.world());
    app.world_mut()
        .get_resource_or_init::<ActionNotices>()
        .entries
        .push_back(ActionNotice {
            seq,
            epoch,
            tick,
            unit: actor.0,
            verb: verb.into(),
        });
}

#[test]
fn island_projection_is_sorted_complete_and_does_not_change_the_simulation() {
    let mut observed = testbed_content::new_sim_app(7, "ai_village");
    let mut control = testbed_content::new_sim_app(7, "ai_village");
    for _ in 0..3 {
        let before = state_hash(observed.world_mut());
        let view = projection::world_state(observed.world_mut()).unwrap();
        assert_eq!(state_hash(observed.world_mut()), before);
        assert_eq!(view["schema_version"], 1);
        assert_eq!(view["world"]["scene"], "island_village");
        let entities = view["entities"].as_array().unwrap();
        assert!(
            entities
                .windows(2)
                .all(|p| p[0]["id"].as_u64() < p[1]["id"].as_u64())
        );
        assert_eq!(
            entities.iter().filter(|e| e["kind"] == "building").count(),
            9
        );
        assert_eq!(
            entities.iter().filter(|e| e["kind"] == "resource").count(),
            4
        );
        assert_eq!(
            entities.iter().filter(|e| e["kind"] == "person").count(),
            10
        );
        let player = observed.world().resource::<VillageState>().played.0;
        assert_eq!(
            entities.iter().filter(|e| e["is_player"] == true).count(),
            1
        );
        assert!(
            entities
                .iter()
                .any(|e| e["id"] == player && e["is_player"] == true)
        );
        assert_eq!(view["terrain"]["cells"].as_array().unwrap().len(), 4096);
        let json = serde_json::to_string(&view).unwrap();
        for private in ["token", "session_key", "diary", "agent_alias"] {
            assert!(!json.contains(private), "projection leaked {private}");
        }
        run_ticks(observed.world_mut(), 30);
        run_ticks(control.world_mut(), 30);
        assert_eq!(
            state_hash(observed.world_mut()),
            state_hash(control.world_mut())
        );
    }
}

#[test]
fn unsupported_scenes_are_explicit() {
    let mut app = testbed_content::new_sim_app(1, "testbed");
    assert!(
        projection::world_state(app.world_mut())
            .unwrap_err()
            .contains("island")
    );
}

#[test]
fn physical_diaries_are_visible_but_their_pages_are_not_exported() {
    use npc_framework::interaction::Item;
    use testbed_content::village::{DiaryPage, DroppedDiary};
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let author = app.world().resource::<VillageState>().played;
    let (diary, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(1000, 2000),
            Item::new("village_diary"),
            DroppedDiary {
                author,
                author_name: "private-author-sentinel".into(),
                life: 1,
                died_at: 0,
                pages: vec![DiaryPage {
                    tick: 0,
                    title: "private-title-sentinel".into(),
                    body: "private-body-sentinel".into(),
                }],
                sealed: true,
            },
        ),
    );
    let view = projection::world_state(app.world_mut()).unwrap();
    assert!(
        view["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == diary.0 && e["kind"] == "item")
    );
    assert!(!serde_json::to_string(&view).unwrap().contains("sentinel"));
}

#[test]
fn projection_bubbles_keep_four_public_actions_and_one_explicit_speech_without_private_payloads() {
    use npc_framework::kernel::journal::Journal;
    use serde_json::json;
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app.world().resource::<VillageState>().played;
    app.world_mut().resource_mut::<SimClock>().tick = 20;
    public_notice(&mut app, actor, 1, 20, "PRIVATE_UNKNOWN_VERB_CANARY");
    let unknown = projection::world_state(app.world_mut()).unwrap();
    assert_eq!(person_bubbles(&unknown, actor)[0]["text"], "Action: action");
    for (seq, verb) in ["gather", "sell", "buy", "talk", "stop_work"]
        .into_iter()
        .enumerate()
    {
        public_notice(&mut app, actor, seq as u64 + 2, 20, verb);
    }
    {
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.push(
            20,
            Some(actor),
            JournalKind::effect(
                "VillageSpeech",
                &json!({
                    "text":"Hello, island!","target":null,
                    "args":"PRIVATE_ARGUMENT_CANARY","token":"PRIVATE_TOKEN_CANARY"
                }),
            ),
        );
        journal.push(
            20,
            Some(actor),
            JournalKind::effect(
                "VillageSpeechHeard",
                &json!({"text":"PRIVATE_RECIPIENT_COPY_CANARY","speaker":actor}),
            ),
        );
        journal.push(
            20,
            Some(actor),
            JournalKind::effect(
                "VillageDiaryRead",
                &json!({"body":"PRIVATE_NOTEBOOK_CANARY"}),
            ),
        );
        journal.push(
            20,
            Some(actor),
            JournalKind::ActionRejected {
                controller: "PRIVATE_CONTROLLER_CANARY".into(),
                verb: "PRIVATE_REJECTED_INPUT_CANARY".into(),
                reason: "PRIVATE_REASON_CANARY".into(),
            },
        );
    }
    let before = state_hash(app.world_mut());
    let view = projection::world_state(app.world_mut()).unwrap();
    assert_eq!(state_hash(app.world_mut()), before);
    let bubbles = person_bubbles(&view, actor);
    assert_eq!(bubbles.len(), 2);
    assert_eq!(
        bubbles[0],
        json!({"kind":"thought","text":"Action: sell; buy; talk; stop_work","tick":20,"expires_tick":180})
    );
    assert_eq!(
        bubbles[1],
        json!({"kind":"speech","text":"Hello, island!","tick":20,"expires_tick":380})
    );
    let serialized = serde_json::to_string(&view).unwrap();
    assert!(
        !serialized.contains("PRIVATE_"),
        "projection copies no private payload fields or unknown labels"
    );
    for bubble in bubbles {
        assert!(bubble["text"].as_str().unwrap().chars().count() <= 512);
    }
}

#[test]
fn bubble_expiry_future_records_and_new_epoch_clear_stale_feedback() {
    use npc_framework::agent_io::api::AgentIoState;
    use npc_framework::kernel::journal::Journal;
    use serde_json::json;
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app.world().resource::<VillageState>().played;
    public_notice(&mut app, actor, 1, 0, "gather");
    app.world_mut().resource_mut::<Journal>().push(
        0,
        Some(actor),
        JournalKind::effect(
            "VillageSpeech",
            &json!({"text":"A public greeting.","target":null}),
        ),
    );
    for (tick, expected) in [(159, 2), (160, 1), (359, 1), (360, 0)] {
        app.world_mut().resource_mut::<SimClock>().tick = tick;
        let view = projection::world_state(app.world_mut()).unwrap();
        assert_eq!(person_bubbles(&view, actor).len(), expected);
    }
    let old_epoch = npc_framework::agent_io::api::epoch(app.world());
    app.world_mut().resource_mut::<AgentIoState>().epoch = old_epoch + 1;
    app.world_mut().resource_mut::<Journal>().entries.clear();
    app.world_mut().resource_mut::<SimClock>().tick = 0;
    let reset = projection::world_state(app.world_mut()).unwrap();
    assert!(
        person_bubbles(&reset, actor).is_empty(),
        "old epoch host notices are not replayed"
    );
    public_notice(&mut app, actor, 2, 100, "talk");
    app.world_mut().resource_mut::<Journal>().push(
        100,
        Some(actor),
        JournalKind::effect(
            "VillageSpeech",
            &json!({"text":"Future record.","target":null}),
        ),
    );
    let rewound = projection::world_state(app.world_mut()).unwrap();
    assert!(
        person_bubbles(&rewound, actor).is_empty(),
        "a lower-tick reset cannot display future records"
    );
}

#[test]
fn replay_actions_deduplicate_queued_started_handles_and_speech_is_validated() {
    use npc_framework::kernel::journal::Journal;
    use serde_json::json;
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app.world().resource::<VillageState>().played;
    app.world_mut().resource_mut::<SimClock>().tick = 10;
    {
        let mut journal = app.world_mut().resource_mut::<Journal>();
        for status in ["queued", "started"] {
            journal.push(
                10,
                Some(actor),
                JournalKind::ExternalAction {
                    handle: 55,
                    verb: "gather".into(),
                    status: status.into(),
                    latency_ticks: None,
                },
            );
        }
        journal.push(
            10,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 56,
                verb: "PRIVATE_RAW_UNKNOWN_CANARY".into(),
                status: "started".into(),
                latency_ticks: None,
            },
        );
        journal.push(
            10,
            Some(actor),
            JournalKind::effect("VillageSpeech", &json!({"text":"Valid public words."})),
        );
        journal.push(
            10,
            Some(actor),
            JournalKind::effect(
                "VillageSpeech",
                &json!({"text":"PRIVATE_MULTILINE\nCANARY"}),
            ),
        );
        journal.push(
            10,
            Some(actor),
            JournalKind::effect("VillageSpeech", &json!({"text":"x".repeat(241)})),
        );
        if let JournalKind::Effect { effect, .. } =
            JournalKind::effect("VillageSpeech", &json!({"text":"PRIVATE_UNAPPLIED_CANARY"}))
        {
            journal.push(
                10,
                Some(actor),
                JournalKind::Effect {
                    effect,
                    applied: false,
                },
            );
        }
    }
    let view = projection::world_state(app.world_mut()).unwrap();
    let bubbles = person_bubbles(&view, actor);
    assert_eq!(bubbles[0]["text"], "Action: gather; recorded action");
    assert_eq!(bubbles[1]["text"], "Valid public words.");
    assert!(!serde_json::to_string(&view).unwrap().contains("PRIVATE_"));
}

#[test]
fn public_distant_speech_is_operator_visible_and_feedback_does_not_change_future_simulation() {
    use npc_framework::kernel::journal::Journal;
    use serde_json::json;
    use testbed_content::village::Role;
    let mut observed = testbed_content::new_sim_app(7, "ai_village");
    let mut control = testbed_content::new_sim_app(7, "ai_village");
    let resident = observed
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(Role::Miner.person());
    observed.world_mut().resource_mut::<Journal>().push(
        0,
        Some(resident),
        JournalKind::effect(
            "VillageSpeech",
            &json!({"text":"I am heading to the mine."}),
        ),
    );
    for _ in 0..3 {
        let view = projection::world_state(observed.world_mut()).unwrap();
        assert_eq!(
            person_bubbles(&view, resident)[0]["text"],
            "I am heading to the mine."
        );
        run_ticks(observed.world_mut(), 30);
        run_ticks(control.world_mut(), 30);
        assert_eq!(
            state_hash(observed.world_mut()),
            state_hash(control.world_mut())
        );
    }
}

#[test]
fn retained_expired_notices_do_not_hide_delayed_action_execution() {
    use npc_framework::agent_io::stats::ActionNotices;
    use npc_framework::kernel::journal::Journal;
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app.world().resource::<VillageState>().played;
    public_notice(&mut app, actor, 1, 0, "rest");
    {
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.push(
            0,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 70,
                verb: "rest".into(),
                status: "queued".into(),
                latency_ticks: None,
            },
        );
        journal.push(
            500,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 70,
                verb: "rest".into(),
                status: "started".into(),
                latency_ticks: None,
            },
        );
    }
    assert_eq!(app.world().resource::<ActionNotices>().entries.len(), 1);
    for tick in [500, 659] {
        app.world_mut().resource_mut::<SimClock>().tick = tick;
        let view = projection::world_state(app.world_mut()).unwrap();
        let bubble = &person_bubbles(&view, actor)[0];
        assert_eq!(bubble["text"], "Action: rest");
        assert_eq!(bubble["tick"], 500);
        assert_eq!(bubble["expires_tick"], 660);
    }
    app.world_mut().resource_mut::<SimClock>().tick = 660;
    let expired = projection::world_state(app.world_mut()).unwrap();
    assert!(person_bubbles(&expired, actor).is_empty());
}

#[test]
fn adjacent_acceptance_and_start_deduplicate_and_handle_start_refreshes_time() {
    use npc_framework::kernel::journal::Journal;
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app.world().resource::<VillageState>().played;
    public_notice(&mut app, actor, 1, 10, "gather");
    {
        let mut journal = app.world_mut().resource_mut::<Journal>();
        journal.push(
            10,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 71,
                verb: "gather".into(),
                status: "queued".into(),
                latency_ticks: None,
            },
        );
        journal.push(
            12,
            Some(actor),
            JournalKind::ExternalAction {
                handle: 71,
                verb: "gather".into(),
                status: "started".into(),
                latency_ticks: None,
            },
        );
    }
    app.world_mut().resource_mut::<SimClock>().tick = 12;
    let initial = projection::world_state(app.world_mut()).unwrap();
    let bubble = &person_bubbles(&initial, actor)[0];
    assert_eq!(bubble["text"], "Action: gather");
    assert_eq!(bubble["tick"], 12);
    app.world_mut().resource_mut::<Journal>().push(
        100,
        Some(actor),
        JournalKind::ExternalAction {
            handle: 71,
            verb: "gather".into(),
            status: "started".into(),
            latency_ticks: None,
        },
    );
    app.world_mut().resource_mut::<SimClock>().tick = 100;
    let refreshed = projection::world_state(app.world_mut()).unwrap();
    let bubble = &person_bubbles(&refreshed, actor)[0];
    assert_eq!(bubble["text"], "Action: gather");
    assert_eq!(bubble["tick"], 100);
    assert_eq!(bubble["expires_tick"], 260);
}
