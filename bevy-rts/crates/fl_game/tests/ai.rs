//! `GameBuilder` hosts the scenario's `Controller::Ai` players.

use fl_client::ai::AiHost;
use fl_game::GameBuilder;
use fl_protocol::{PlayerId, TeamId};

#[test]
fn ai_scenario_builds_host() {
    let app = GameBuilder::new()
        .scenario("base.testbed_ai")
        .prepare_headless()
        .expect("the AI testbed prepares");
    let host = app.world().resource::<AiHost>();
    assert_eq!(host.players.len(), 1, "only player 1 is an AI");
    let (team, controller) = host.players.get(&PlayerId(1)).expect("player 1 is hosted");
    assert_eq!(*team, TeamId(1));
    assert_eq!(controller.id(), "null");
}

#[test]
fn human_scenario_has_no_ai() {
    let app = GameBuilder::new()
        .prepare_headless()
        .expect("the testbed prepares");
    assert!(app.world().resource::<AiHost>().players.is_empty());
}

#[test]
fn unknown_ai_is_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("mod.ron"),
        r#"Mod(id: "nope_ai", version: "0.1.0", depends: ["base"], load_after: ["base"])"#,
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("scenarios")).unwrap();
    let base = std::fs::read_to_string(
        fl_catalog::base_mod_dir().join("scenarios/testbed_ai.scenario.ron"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("scenarios/nope.scenario.ron"),
        base.replace("base.testbed_ai", "nope_ai.nope")
            .replace("Ai(\"null\")", "Ai(\"nope\")"),
    )
    .unwrap();
    let Err(error) = GameBuilder::new()
        .content_mod(fl_catalog::base_mod_dir())
        .content_mod(dir.path())
        .scenario("nope_ai.nope")
        .prepare_headless()
    else {
        panic!("an unknown AI controller must not prepare");
    };
    assert!(
        error.contains("unknown AI controller 'nope' for player 1"),
        "{error}"
    );
}

#[test]
fn loads_refresh_the_ai_host() {
    use bevy::prelude::*;
    use fl_client::LocalDriver;
    use fl_client::debug::{LoadRequest, load_match};
    use fl_client::link::{PendingReplies, RunnerLink, send_query, wait_for_reply};
    use fl_view::{SimQuery, SimReply, TickDriver};

    // Ruling R91: fl.load_scenario and fl.load replace the AI host with the new match's.
    let mut app = GameBuilder::new()
        .prepare_headless()
        .expect("the testbed prepares");
    let hosted = |app: &App| -> Vec<PlayerId> {
        app.world()
            .resource::<AiHost>()
            .players
            .keys()
            .copied()
            .collect()
    };
    assert!(hosted(&app).is_empty());
    load_match(
        app.world_mut(),
        LoadRequest::Scenario("base.testbed_ai".to_owned()),
    )
    .expect("the AI testbed loads");
    assert_eq!(hosted(&app), [PlayerId(1)]);

    let world = app.world_mut();
    let pending = world.resource::<LocalDriver>().pending();
    let id = world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
        send_query(
            world.resource::<RunnerLink>(),
            &mut replies,
            SimQuery::Save { pending },
        )
    });
    let Ok(SimReply::Save(save)) = wait_for_reply(world, id, std::time::Duration::from_secs(10))
    else {
        panic!("a save of the AI testbed");
    };

    load_match(
        app.world_mut(),
        LoadRequest::Scenario("base.testbed".to_owned()),
    )
    .expect("the testbed loads");
    assert!(hosted(&app).is_empty(), "the human testbed hosts no AI");
    load_match(app.world_mut(), LoadRequest::Save(save)).expect("the save loads");
    assert_eq!(
        hosted(&app),
        [PlayerId(1)],
        "the save's AI player is hosted again"
    );
}
