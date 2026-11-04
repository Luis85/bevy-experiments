//! Recorded public speech; fixtures arrange actors, never provider conversations.
use bevy_app::App;
use npc_framework::agent_io::{api, registration};
use npc_framework::kernel::journal::Journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{self, Role, VillageAct, Villager};

fn id(app: &App, name: &str) -> SimId {
    app.world()
        .resource::<testbed_content::TestbedIds>()
        .id(name)
}
fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn fixture() -> (App, SimId) {
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = id(&app, "human");
    let body = entity(&app, actor);
    app.world_mut().entity_mut(body).insert(SimPos::new(0, 0));
    // Move unrelated residents outside the audible fixture. Named actors are
    // looked up through the scene registry; no numerical identities are assumed.
    for (index, role) in Role::RESIDENTS.into_iter().enumerate() {
        let body = entity(&app, id(&app, role.person()));
        app.world_mut()
            .entity_mut(body)
            .insert(SimPos::new(40_000 + index as i32 * 1000, 40_000));
    }
    (app, actor)
}
fn visitor(app: &mut App, role: Role, name: &str, pos: SimPos) -> SimId {
    let id = id(app, role.person());
    let body = entity(app, id);
    app.world_mut().entity_mut(body).insert(pos);
    let mut person = app.world_mut().get_mut::<Villager>(body).unwrap();
    person.role = Role::Visitor;
    person.name = name.into();
    id
}
fn talk(app: &mut App, actor: SimId, target: Option<SimId>, text: &str) {
    let mut command = VillageAct::new(actor, "talk");
    command.target = target;
    command.text = text.into();
    command.perform(app.world_mut()).unwrap();
}
fn observed(app: &mut App, actor: SimId) -> Value {
    api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit":actor})),
    )
    .unwrap()
}
fn heard(app: &mut App, actor: SimId) -> Vec<Value> {
    observed(app, actor)["sections"]["village"]["heard_speech"]
        .as_array()
        .unwrap()
        .clone()
}
fn register(app: &mut App, actor: SimId, name: &str) -> String {
    app.insert_resource(registration::RegistrationRequired);
    let body = entity(app, actor);
    if app.world().get::<Brain>(body).is_none() {
        // Scene residents deliberately have no BT. This operator fixture turns
        // one into a second playable visitor, so give it the same player-style
        // brain with fresh runtime state before requesting ordinary Direct control.
        let player = entity(app, id(app, "human"));
        let brain = *app
            .world()
            .get::<Brain>(player)
            .expect("playable visitor brain");
        app.world_mut().entity_mut(body).insert((
            brain,
            BrainState::default(),
            Blackboard::default(),
        ));
    }
    let controller = if actor == id(app, "human") {
        "visitor"
    } else {
        name
    };
    registration::set_credential(
        app.world_mut(),
        name,
        registration::Credential {
            token_digest: registration::token_digest("speech-fixture-token"),
            assigned: actor,
            controller: controller.into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":name,"token":"speech-fixture-token"})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    api::call(
        app.world_mut(),
        "agent/enter_world",
        Some(json!({"session":session})),
    )
    .unwrap();
    api::call(
        app.world_mut(),
        "agent/take_control",
        Some(json!({"session":session})),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let controller_state = app
        .world()
        .get::<npc_framework::agent_io::ExternalController>(body)
        .unwrap();
    assert_eq!(
        controller_state.mode,
        npc_framework::agent_io::ControlMode::Direct
    );
    assert_eq!(controller_state.id, controller);
    session
}

#[test]
fn addressed_speech_reaches_only_its_living_target_and_visitors_can_reply() {
    let (mut app, actor) = fixture();
    let listener = visitor(
        &mut app,
        Role::Farmer,
        "Second visitor",
        SimPos::new(1000, 0),
    );
    let bystander = visitor(&mut app, Role::Miller, "Bystander", SimPos::new(500, 0));
    talk(&mut app, actor, Some(listener), "Hello, neighbor.");
    assert_eq!(heard(&mut app, listener)[0]["text"], "Hello, neighbor.");
    assert!(heard(&mut app, actor).is_empty());
    assert!(heard(&mut app, bystander).is_empty());
    talk(
        &mut app,
        listener,
        Some(actor),
        "Hello. Let us trade later.",
    );
    assert_eq!(heard(&mut app, actor)[0]["speaker"], json!(listener));
    let body = entity(&app, listener);
    app.world_mut()
        .get_mut::<village::Vitality>(body)
        .unwrap()
        .died_at = Some(0);
    let mut command = VillageAct::new(actor, "talk");
    command.text = "Are you there?".into();
    command.target = Some(listener);
    assert!(command.check(app.world()).is_err());
    command.target = Some(actor);
    assert!(command.check(app.world()).unwrap_err().contains("yourself"));
}

#[test]
fn aloud_speech_uses_integer_radius_sorted_recipients_and_excludes_dead_or_distant() {
    let (mut app, actor) = fixture();
    let first = visitor(&mut app, Role::Miller, "At boundary", SimPos::new(6000, 0));
    let second = visitor(&mut app, Role::Farmer, "Near", SimPos::new(1000, 0));
    let outside = visitor(&mut app, Role::Baker, "Outside", SimPos::new(6001, 0));
    let dead = visitor(&mut app, Role::WaterKeeper, "Dead", SimPos::new(500, 0));
    let body = entity(&app, dead);
    app.world_mut()
        .get_mut::<village::Vitality>(body)
        .unwrap()
        .died_at = Some(0);
    talk(&mut app, actor, None, "Hello, island.");
    for listener in [first, second] {
        assert_eq!(heard(&mut app, listener).len(), 1);
    }
    for listener in [actor, outside, dead] {
        assert!(heard(&mut app, listener).is_empty());
    }
    let recorded: Vec<_> = app.world().resource::<Journal>().entries.iter().filter_map(|entry| {
        if matches!(&entry.kind, JournalKind::Effect { effect, applied: true } if effect.get("type").and_then(Value::as_str)==Some("VillageSpeechHeard")) {
            entry.unit
        } else { None }
    }).collect();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(recorded, expected);
}

#[test]
fn speech_rejects_empty_multiline_control_and_oversized_text_before_recording() {
    let (mut app, actor) = fixture();
    let before = app.world().resource::<Journal>().entries.len();
    for text in [
        String::new(),
        "   ".into(),
        "hello\nworld".into(),
        "hello\rworld".into(),
        "hello\tworld".into(),
        "hello\0world".into(),
        "hello\u{2028}world".into(),
        "hello\u{2029}world".into(),
        "x".repeat(241),
        "🙂".repeat(257),
    ] {
        let mut command = VillageAct::new(actor, "talk");
        command.text = text;
        assert!(command.perform(app.world_mut()).is_err());
    }
    assert_eq!(app.world().resource::<Journal>().entries.len(), before);
    assert!(village::speech::validate_text(&"🙂".repeat(240)).is_ok());
}

#[test]
fn legacy_resident_talk_keeps_offers_and_adds_one_bounded_nonrecursive_reply() {
    let (mut app, actor) = fixture();
    let resident = id(&app, Role::Baker.person());
    let body = entity(&app, resident);
    app.world_mut()
        .entity_mut(body)
        .insert(SimPos::new(1000, 0));
    talk(&mut app, actor, Some(resident), "");
    assert!(app.world().resource::<Journal>().entries.iter().any(|entry| {
        entry.unit == Some(actor) && matches!(&entry.kind, JournalKind::Effect { effect, applied:true } if effect.get("type").and_then(Value::as_str)==Some("VillageConversation"))
    }));
    let replies = heard(&mut app, actor);
    assert_eq!(replies.len(), 1);
    assert!(replies[0]["text"].as_str().unwrap().contains("bread"));
    let speeches = app.world().resource::<Journal>().entries.iter().filter(|entry| matches!(&entry.kind, JournalKind::Effect { effect, applied:true } if effect.get("type").and_then(Value::as_str)==Some("VillageSpeech"))).count();
    assert_eq!(
        speeches, 2,
        "one question and one resident reply cannot recurse"
    );
    let serialized = serde_json::to_value(VillageAct::new(actor, "talk")).unwrap();
    assert!(
        serialized.get("text").is_none(),
        "legacy canonical commands omit empty optional speech"
    );
}

#[test]
fn heard_observation_is_bounded_expires_and_does_not_publish_other_recipients_words() {
    let (mut app, actor) = fixture();
    let listener = visitor(&mut app, Role::Farmer, "Listener", SimPos::new(1000, 0));
    let other = visitor(&mut app, Role::Miller, "Other", SimPos::new(2000, 0));
    for index in 0..12 {
        talk(&mut app, actor, Some(listener), &format!("Line {index}"));
    }
    talk(&mut app, listener, Some(other), "PRIVATE_OTHER_RECIPIENT");
    let lines = heard(&mut app, listener);
    assert_eq!(lines.len(), village::speech::MAX_HEARD_LINES);
    assert_eq!(lines[0]["text"], "Line 4");
    assert!(
        !serde_json::to_string(&lines)
            .unwrap()
            .contains("PRIVATE_OTHER_RECIPIENT")
    );
    assert!(heard(&mut app, actor).is_empty());
    app.world_mut().resource_mut::<SimClock>().tick = village::speech::HEARD_WINDOW_TICKS + 1;
    assert!(heard(&mut app, listener).is_empty());
}

#[test]
fn registered_actor_to_actor_talk_completes_and_action_notices_keep_only_public_verbs() {
    use npc_framework::agent_io::stats::{ActionNotice, ActionNotices};
    let (mut app, actor) = fixture();
    let listener = visitor(
        &mut app,
        Role::Farmer,
        "Second visitor",
        SimPos::new(1000, 0),
    );
    let session = register(&mut app, actor, "speaker");
    let recipient = register(&mut app, listener, "listener");
    // Operator arrangement: both visitors face one another and receive a full
    // perception scan. Mere physical proximity does not imply identity coverage.
    let speaker_body = entity(&app, actor);
    let listener_body = entity(&app, listener);
    app.world_mut()
        .entity_mut(speaker_body)
        .insert((Facing::new(1, 0), FixedFacing));
    app.world_mut()
        .entity_mut(listener_body)
        .insert((Facing::new(-1, 0), FixedFacing));
    run_ticks(app.world_mut(), 40);
    let observation = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap();
    assert!(
        observation["sections"]["perception"]["sensed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|percept| percept["id"] == json!(listener)),
        "operator fixture must establish the listener's perceived identity"
    );
    let offered = observation["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["verb"] == "talk" && a["target"] == json!(listener))
        .expect("nearby visiting actor offers talk");
    let target = offered["target"].clone();
    let accepted = api::call(app.world_mut(), "agent/act", Some(json!({"session":session,"verb":"talk","target":target,"args":{"text":"PUBLIC_ACTOR_GREETING"}}))).unwrap();
    assert_eq!(accepted["accepted"], true);
    run_ticks(app.world_mut(), 3);
    let received = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":recipient})),
    )
    .unwrap();
    assert!(
        received["sections"]["village"]["heard_speech"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line["text"] == "PUBLIC_ACTOR_GREETING")
    );
    let speaker = received["sections"]["village"]["heard_speech"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["text"] == "PUBLIC_ACTOR_GREETING")
        .unwrap()["speaker"]
        .clone();
    let reply = api::call(app.world_mut(), "agent/act", Some(json!({"session":recipient,"verb":"talk","target":speaker,"args":{"text":"PUBLIC_REGISTERED_VISITOR_REPLY"}}))).unwrap();
    assert_eq!(reply["accepted"], true);
    run_ticks(app.world_mut(), 3);
    let answered = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap();
    assert!(
        answered["sections"]["village"]["heard_speech"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line["speaker"] == json!(listener)
                && line["text"] == "PUBLIC_REGISTERED_VISITOR_REPLY")
    );
    assert!(
        app.world()
            .resource::<ActionNotices>()
            .entries
            .iter()
            .any(|notice| notice.unit == actor.0 && notice.verb == "talk")
    );
    let before = state_hash(app.world_mut());
    app.world_mut()
        .resource_mut::<ActionNotices>()
        .entries
        .push_back(ActionNotice {
            seq: 999,
            epoch: 1,
            tick: 0,
            unit: actor.0,
            verb: "talk".into(),
        });
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "observer notices never affect the sim hash"
    );
    let denied = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session,"unit":listener})),
    );
    assert!(denied.is_err());
}

#[test]
fn speech_commands_replay_to_the_same_journal_and_state_hash() {
    let (mut a, actor_a) = fixture();
    let (mut b, actor_b) = fixture();
    for (app, actor) in [(&mut a, actor_a), (&mut b, actor_b)] {
        let target = visitor(app, Role::Farmer, "Listener", SimPos::new(1000, 0));
        let mut command = VillageAct::new(actor, "talk");
        command.target = Some(target);
        command.text = "We will build something useful.".into();
        submit(app.world_mut(), None, SimCommand::content(&command)).unwrap();
        run_ticks(app.world_mut(), 2);
    }
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    assert_eq!(
        a.world().resource::<Journal>().entries,
        b.world().resource::<Journal>().entries
    );
}
