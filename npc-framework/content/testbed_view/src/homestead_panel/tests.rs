use super::*;
use npc_framework::agent_io::control::{DetachController, ExternalController};
use npc_framework::prelude::*;
use testbed_content::village::{self, homestead};

#[test]
fn coherent_flow_hides_legacy_farm_controls_in_inspection_and_drops_hidden_requests() {
    let mut app = fixture("island_colony");
    app.insert_resource(crate::PlayerExperience { enabled: true });
    app.world_mut().resource_mut::<FarmState>().open = true;
    app.world_mut()
        .resource_mut::<FarmState>()
        .requests
        .push(Action::Toggle);
    process(app.world_mut());
    refresh(app.world_mut());
    assert!(!app.world().resource::<FarmState>().open);
    assert!(app.world().resource::<FarmState>().requests.is_empty());
    assert!(
        app.world_mut()
            .query_filtered::<&Node, Or<(With<FarmRoot>, With<FarmLauncher>)>>()
            .iter(app.world())
            .all(|node| node.display == Display::None)
    );
}

fn fixture(scene: &str) -> App {
    let mut app = testbed_content::try_new_sim_app_with(7, scene, |app| {
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::text::TextPlugin,
        ))
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<SimControl>()
        .init_resource::<crate::village_world::FollowHero>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<DragBox>()
        .configure_sets(
            Update,
            (
                ViewSystems::OverlayInput,
                ViewSystems::Input,
                ViewSystems::Present,
            )
                .chain(),
        )
        // Headless fixture has no UI layout/visibility propagation plugin.
        // Match the existing notes-panel keyboard fixture's visible-node stand-in.
        .add_systems(
            PostUpdate,
            |mut nodes: Query<(&Node, &mut InheritedVisibility)>| {
                for (node, mut visibility) in &mut nodes {
                    *visibility = if node.display == Display::None {
                        InheritedVisibility::HIDDEN
                    } else {
                        InheritedVisibility::VISIBLE
                    };
                }
            },
        )
        .add_plugins((plugin, crate::island_control::plugin));
    })
    .unwrap();
    app.update();
    app
}
fn press(app: &mut App, want: &Action) {
    let entity = app
        .world_mut()
        .query::<(Entity, &FarmAction)>()
        .iter(app.world())
        .find(|(_, action)| &action.0 == want)
        .map(|(entity, _)| entity)
        .expect("actual native control exists");
    app.world_mut().trigger(Activate { entity });
    app.world_mut().flush();
    app.update();
}
fn texts(app: &mut App) -> String {
    app.world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>()
        .join("\n")
}
fn advance(app: &mut App, ticks: u64) {
    for _ in 0..ticks.div_ceil(10) {
        run_ticks(app.world_mut(), 10);
        app.update();
    }
}
fn free_hero(app: &mut App) -> (SimId, Entity) {
    let id = app.world().resource::<VillageState>().played;
    let entity = app.world().resource::<SimIndex>().get(id).unwrap();
    if let Some(controller) = app.world().get::<ExternalController>(entity) {
        let controller = controller.id.clone();
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&DetachController {
                unit: id.0,
                controller,
            }),
        )
        .unwrap();
        run_ticks(app.world_mut(), 2);
    }
    app.update();
    (id, entity)
}

#[test]
fn expanded_living_world_has_a_discoverable_native_farming_launcher() {
    let mut app = fixture("living_village");
    assert!(
        texts(&mut app).contains("Home & farm"),
        "an expanded living island must offer native Home & farm controls"
    );
}

#[test]
fn colony_farm_and_ledger_share_pending_control_intent() {
    let mut app = fixture("island_colony");
    press(&mut app, &Action::Toggle);
    app.world_mut().resource_mut::<SimControl>().paused = true;
    press(&mut app, &Action::Take);
    assert_eq!(
        *app.world().resource::<crate::IslandMode>(),
        crate::IslandMode::Character
    );
    assert!(crate::island_control::control_pending(app.world()));
    crate::island_control::request_mode(app.world_mut(), crate::IslandMode::Inspector).unwrap();
    app.world_mut().resource_mut::<SimControl>().paused = false;
    run_ticks(app.world_mut(), 1);
    app.update();
    run_ticks(app.world_mut(), 1);
    app.update();
    assert_eq!(player_actions::control(app.world()), ControlState::Free);
    assert_eq!(
        *app.world().resource::<crate::IslandMode>(),
        crate::IslandMode::Inspector
    );
    press(&mut app, &Action::Take);
    run_ticks(app.world_mut(), 1);
    app.update();
    assert_eq!(player_actions::control(app.world()), ControlState::Native);
    assert_eq!(
        *app.world().resource::<crate::IslandMode>(),
        crate::IslandMode::Character
    );
    press(&mut app, &Action::Release);
    run_ticks(app.world_mut(), 1);
    app.update();
    assert_eq!(player_actions::control(app.world()), ControlState::Free);
    assert_eq!(
        *app.world().resource::<crate::IslandMode>(),
        crate::IslandMode::Inspector
    );
    let arrivals = app
        .world()
        .resource::<npc_framework::kernel::commands::CommandLog>()
        .entries
        .iter()
        .filter(|(_, _, command)| command.kind() == "JoinIsland")
        .count();
    assert_eq!(
        arrivals, 1,
        "F2 and ledger requests share the same mortal arrival"
    );
}

#[test]
fn f2_and_escape_work_but_a_foreign_field_keeps_its_keys() {
    let mut app = fixture("living_village");
    let foreign = app.world_mut().spawn(Node::default()).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(foreign, FocusCause::Navigated);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F2);
    app.update();
    assert!(!app.world().resource::<FarmState>().open);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut().resource_mut::<InputFocus>().clear();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F2);
    app.update();
    assert!(app.world().resource::<FarmState>().open);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Tab);
    app.update();
    assert!(
        app.world()
            .resource::<InputFocus>()
            .get()
            .is_some_and(|entity| app.world().get::<FarmControl>(entity).is_some())
    );
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    assert!(!app.world().resource::<FarmState>().open);
    assert!(app.world().resource::<InputFocus>().get().is_none());
}

#[test]
fn compact_world_has_no_inapplicable_farming_launcher() {
    let mut app = fixture("ai_village");
    assert!(!texts(&mut app).contains("Home & farm"));
}

#[test]
fn opening_and_browsing_are_read_only_and_keep_live_buttons_stable() {
    let mut app = fixture("living_village");
    let before = state_hash(app.world_mut());
    press(&mut app, &Action::Toggle);
    press(&mut app, &Action::Tab(Tab::Supplies));
    let controls: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<FarmControl>>()
        .iter(app.world())
        .collect();
    app.update();
    let later: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<FarmControl>>()
        .iter(app.world())
        .collect();
    assert_eq!(
        controls, later,
        "refresh must retain widgets and their input/focus identity"
    );
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn foreign_control_and_disabled_activation_cannot_silently_take_the_hero() {
    let mut app = fixture("living_village");
    let before = state_hash(app.world_mut());
    press(&mut app, &Action::Toggle);
    assert!(matches!(
        player_actions::control(app.world()),
        ControlState::Foreign { .. }
    ));
    press(&mut app, &Action::Take);
    assert_eq!(before, state_hash(app.world_mut()));
    assert!(texts(&mut app).contains("Another client controls the hero"));
    assert!(
        !world_commands(&app)
            .iter()
            .any(|(_, _, cmd)| matches!(cmd, SimCommand::Content { .. }))
    );
}

fn world_commands(app: &App) -> Vec<(u64, u32, SimCommand)> {
    npc_framework::agent_io::api::command_log(app.world())
}

#[test]
fn native_play_button_records_control_and_pause_has_an_explicit_resume_button() {
    let mut app = fixture("living_village");
    let (id, _) = free_hero(&mut app);
    app.world_mut().resource_mut::<SimControl>().paused = true;
    press(&mut app, &Action::Toggle);
    let before = state_hash(app.world_mut());
    press(&mut app, &Action::Take);
    assert_eq!(
        before,
        state_hash(app.world_mut()),
        "control is submitted, not directly written"
    );
    assert_eq!(player_actions::control(app.world()), ControlState::Free);
    press(&mut app, &Action::Resume);
    assert!(!app.world().resource::<SimControl>().paused);
    advance(&mut app, 20);
    assert_eq!(player_actions::control(app.world()), ControlState::Native);
    assert_eq!(app.world().resource::<Selected>().0, Some(id));
    assert!(
        app.world()
            .resource::<crate::village_world::FollowHero>()
            .enabled
    );
}

#[test]
fn actual_native_widgets_claim_build_water_and_harvest_the_selected_bed() {
    let mut app = fixture("living_village");
    let (id, actor) = free_hero(&mut app);
    let (plot_id, plot) = homestead::plots(app.world())[0];
    let door = homestead::front_door(*app.world().get::<SimPos>(plot).unwrap());
    // Presentation fixture supplies only these declared materials and an initial
    // location; every action below is dispatched by a real native widget.
    app.world_mut().entity_mut(actor).insert(door);
    for (kind, count) in [
        ("plank", 8),
        ("log", 4),
        (village::catalog::GRAIN, 1),
        (village::catalog::WATER, 1),
    ] {
        village::economy::grant(app.world_mut(), actor, Item::new(kind).stack(count));
    }
    advance(&mut app, 60);
    press(&mut app, &Action::Toggle);
    press(&mut app, &Action::Take);
    advance(&mut app, 20);
    press(&mut app, &Action::Plot(plot_id.0));
    let click_key = |app: &mut App, key: &str| {
        let entity = *app
            .world()
            .resource::<FarmState>()
            .rows
            .get(key)
            .expect("work widget rendered");
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
        app.update();
    };
    click_key(&mut app, "claim-plot");
    advance(&mut app, 120);
    assert_eq!(
        app.world()
            .get::<homestead::HomesteadPlot>(plot)
            .unwrap()
            .owner,
        Some(id)
    );
    click_key(&mut app, "build-cottage");
    advance(&mut app, 800);
    assert!(
        app.world()
            .get::<homestead::HomesteadPlot>(plot)
            .unwrap()
            .cottage_built
    );
    assert_eq!(village::economy::count(app.world(), actor, "plank"), 0);
    assert_eq!(village::economy::count(app.world(), actor, "log"), 0);
    press(&mut app, &Action::Bed(1));
    click_key(&mut app, "tend-bed");
    advance(&mut app, 160);
    let estate = app.world().get::<homestead::HomesteadPlot>(plot).unwrap();
    assert!(estate.beds[0].is_none());
    assert!(
        estate.beds[1].is_some(),
        "selected side-bed must be passed to the shared action"
    );
    click_key(&mut app, "tend-bed");
    advance(&mut app, 160);
    let crop = app
        .world()
        .get::<homestead::HomesteadPlot>(plot)
        .unwrap()
        .beds[1]
        .as_ref()
        .unwrap();
    assert!(crop.watered_at.is_some());
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::WATER),
        0
    );
    advance(&mut app, u64::from(homestead::GROW_TICKS) + 20);
    click_key(&mut app, "tend-bed");
    advance(&mut app, 160);
    assert!(
        app.world()
            .get::<homestead::HomesteadPlot>(plot)
            .unwrap()
            .beds[1]
            .is_none()
    );
    assert_eq!(
        village::economy::count(app.world(), actor, village::catalog::GRAIN),
        3
    );
}

#[test]
fn crop_model_keeps_unknown_foreign_state_unknown_and_does_not_guess_positions() {
    let mut app = fixture("living_village");
    let before = state_hash(app.world_mut());
    let observation = serde_json::json!({"sections":{"homestead":{"plots":[{"id":900,"name":"Remembered plot","state_known":false,"own":false,"beds":[]}]}}});
    let rows = model::entries(Context {
        world: app.world(),
        observation: &observation,
        tab: Tab::Land,
        plot: Some(900),
        bed: 0,
        quantity: 1,
        ready: false,
    });
    assert!(rows.iter().any(|row| row.text.contains("Visit to inspect")));
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row.action, Some(Action::Walk(_))))
    );
    assert_eq!(before, state_hash(app.world_mut()));
}
