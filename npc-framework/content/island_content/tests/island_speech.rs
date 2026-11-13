//! Recorded public speech; fixtures arrange actors, never provider conversations.
use bevy_app::App;
use island_content::village::{self, Role, VillageAct};
use npc_framework::kernel::journal::Journal;
use npc_framework::prelude::*;

fn id(app: &App, name: &str) -> SimId {
    app.world()
        .resource::<island_content::TestbedIds>()
        .id(name)
}
fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn fixture() -> (App, SimId) {
    let mut app = island_content::new_sim_app(7, "living_world");
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
