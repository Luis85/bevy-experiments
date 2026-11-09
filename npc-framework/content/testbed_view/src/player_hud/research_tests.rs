use super::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn selected_site_exposes_work_before_any_other_plot_and_owned_garden_selects_useful_bed() {
    let app = testbed_content::new_sim_app(7, "island_colony");
    let observed = json!({"unit":42,"sections":{
    "inventory":{"items":[{"kind":"log","stack":4},{"kind":"plank","stack":8},{"kind":"village_berries","stack":2},{"kind":"village_water","stack":1}]},
    "homestead":{"cottage":{"inputs":[{"kind":"log","count":4},{"kind":"plank","count":8}]},"crops":[{"kind":"village_grain"},{"kind":"village_berries"}],"plots":[
        {"id":10,"name":"Other plot","use_kind":"housing","state_known":false},
        {"id":11,"name":"Your cottage","use_kind":"housing","own":true,"front_door":[100,200],"beds":[
            {"index":0,"stage":"empty","position":[100,300]},
            {"index":1,"stage":"growing","position":[100,400],"ready_at":9000},
            {"index":2,"stage":"needs_water","position":[100,500],"kind":"village_berries"}]},
        {"id":12,"name":"Another plot","use_kind":"housing","state_known":false}
    ]}}});
    assert_eq!(
        model::preferred_plot(&observed, Tab::Garden, Some(10)),
        Some(11)
    );
    assert_eq!(model::preferred_bed(&observed, Some(11)), Some(2));
    assert_eq!(
        model::preferred_crop(&observed, "village_grain"),
        "village_berries"
    );
    let state = PlayerState {
        tab: Some(Tab::Build),
        plot: Some(11),
        ..default()
    };
    let rows = model::rows(app.world(), &observed, &state, true);
    assert!(
        rows.iter()
            .all(|row| !matches!(row.action, Some(Action::Plot(_))))
    );
    assert!(rows.iter().position(|row| row.key == "build_home").unwrap() < 3);
    assert!(
        rows.iter()
            .any(|row| row.action == Some(Action::ChoosePlot))
    );
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::Garden),
            plot: Some(11),
            bed: 2,
            ..default()
        },
        true,
    );
    let water = rows.iter().position(|row| row.key == "water").unwrap();
    let choices = rows.iter().position(|row| row.key == "other_beds").unwrap();
    assert!(water < choices, "bed work must precede the bed chooser");
    let Some(Action::Work(request)) = &rows[water].action else {
        panic!("watering action")
    };
    assert_eq!(request.approach, Some(SimPos::new(100, 500)));
    assert_eq!(request.args["bed"], 2);
}

#[test]
fn only_known_residents_are_people_and_distant_selection_does_not_override_nearby_context() {
    let app = testbed_content::new_sim_app(7, "island_colony");
    let observed = json!({"unit":42,"sections":{
        "perception":{"sensed":[
            {"id":99,"kind":"actor","name":"Wild deer","pos":[500,0],"dist_mm":500},
            {"id":5,"kind":"actor","name":"Ada","pos":[10000,0],"dist_mm":10000},
            {"id":6,"kind":"actor","name":"Rowan","pos":[1500,0],"dist_mm":1500}]},
        "village":{"social":{"relationships":[
            {"id":5,"name":"Ada","role":"farmer","affinity":4000},
            {"id":6,"name":"Rowan","role":"miller","affinity":7000}]}}
    }});
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::People),
            ..default()
        },
        true,
    );
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row.action, Some(Action::Person(_))))
            .count(),
        2
    );
    assert!(!rows.iter().any(|row| row.title.contains("deer")));
    assert!(
        rows.iter()
            .any(|row| row.detail.contains("Farmer") && row.detail.contains("4%"))
    );
    let context = model::context(app.world(), &observed, Some(SimId(5)), true).unwrap();
    assert_eq!(context.action, Some(Action::Person(6)));
    let mut only_animal = observed.clone();
    only_animal["sections"]["perception"]["sensed"] = json!([
        {"id":99,"kind":"actor","name":"Wild deer","pos":[500,0],"dist_mm":500}]);
    assert!(model::context(app.world(), &only_animal, Some(SimId(99)), true).is_none());
}

#[test]
fn exploration_stays_reachable_after_a_source_is_discovered_and_uses_own_relative_position() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    let observed = json!({"unit":42,"sections":{"self":{"pos":[1000,2000]},"village":{"environment":{"resources":[{"id":99,"kind":"village_water","remaining":50}]}},"homestead":{"plots":[]}}});
    let before = state_hash(app.world_mut());
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::Craft),
            ..default()
        },
        true,
    );
    assert!(
        rows.iter()
            .any(|row| row.action == Some(Action::Open(Tab::Explore)))
    );
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::Explore),
            ..default()
        },
        true,
    );
    let destinations: Vec<_> = rows
        .iter()
        .filter_map(|row| {
            if row.key.starts_with("explore_")
                && let Some(Action::Walk(pos)) = row.action
            {
                Some(pos)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        destinations,
        vec![
            SimPos::new(1000, 8000),
            SimPos::new(7000, 2000),
            SimPos::new(1000, -4000),
            SimPos::new(-5000, 2000)
        ]
    );
    assert!(
        rows.iter()
            .filter(|row| row.key.starts_with("explore_") && row.action.is_some())
            .all(|row| row.compact)
    );
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn home_heating_projects_real_fuel_and_automatic_upkeep_without_an_invented_action() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    let observed = json!({"sections":{"village":{"home":[100,200],"home_heating":{"heated":false,"cold":true,"carried_fuel":0,"fuel_required":1}}}});
    let before = state_hash(app.world_mut());
    let rows = model::rows(
        app.world(),
        &observed,
        &PlayerState {
            tab: Some(Tab::Pouch),
            ..default()
        },
        true,
    );
    let heating = rows.iter().find(|row| row.key == "home_heating").unwrap();
    assert!(heating.detail.contains("automatically") && heating.detail.contains("0/1"));
    assert!(
        rows.iter()
            .any(|row| row.key == "home_fuel" && row.action == Some(Action::Open(Tab::Craft)))
    );
    assert!(rows.iter().all(
        |row| !matches!(&row.action, Some(Action::Work(work)) if work.verb.contains("hearth"))
    ));
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn watching_after_death_is_view_only_epoch_scoped_and_life_choices_remain_reachable() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    app.add_plugins(crate::TestbedViewPlugin);
    app.world_mut()
        .insert_resource(PlayerExperience { enabled: true });
    let played = app
        .world()
        .resource::<testbed_content::village::VillageState>()
        .played;
    testbed_content::village::colony::JoinIsland { unit: played.0 }
        .perform(app.world_mut())
        .unwrap();
    let entity = app.world().resource::<SimIndex>().get(played).unwrap();
    {
        let mut vitality = app
            .world_mut()
            .get_mut::<testbed_content::village::Vitality>(entity)
            .unwrap();
        vitality.health = 0;
        vitality.died_at = Some(1);
    }
    assert!(life::needs_prompt(app.world()));
    let before = state_hash(app.world_mut());
    watch_village(app.world_mut()).unwrap();
    assert!(!active(app.world()));
    assert!(life::watching(app.world()));
    assert_eq!(before, state_hash(app.world_mut()));
    let foreign_button = app
        .world_mut()
        .spawn((bevy::ui_widgets::Button, Node::default()))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(foreign_button, FocusCause::Navigated);
    app.world_mut().init_resource::<ButtonInput<KeyCode>>();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyL);
    keys(app.world_mut());
    assert!(
        app.world().resource::<PlayerState>().requests.is_empty(),
        "another panel's button owns L"
    );
    app.world_mut().resource_mut::<InputFocus>().clear();
    keys(app.world_mut());
    assert_eq!(
        app.world().resource::<PlayerState>().requests,
        vec![Action::LifeChoices]
    );
    actions::process(app.world_mut());
    assert!(active(app.world()));
    assert_eq!(before, state_hash(app.world_mut()));
    watch_village(app.world_mut()).unwrap();
    app.world_mut()
        .insert_resource(npc_framework::agent_io::api::AgentIoState::default());
    assert!(
        life::needs_prompt(app.world()),
        "another world epoch cannot inherit dismissal"
    );
    assert_eq!(before, state_hash(app.world_mut()));
}

#[test]
fn explore_shortcut_respects_foreign_text_and_button_focus() {
    let mut world = World::new();
    world.insert_resource(PlayerExperience { enabled: true });
    world.insert_resource(IslandMode::Character);
    world.init_resource::<PlayerState>();
    world.init_resource::<InputFocus>();
    world.init_resource::<ButtonInput<KeyCode>>();
    let editor = world.spawn(bevy::text::EditableText::new("Note")).id();
    world
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Navigated);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    keys(&mut world);
    assert!(world.resource::<PlayerState>().requests.is_empty());
    let foreign_button = world
        .spawn((bevy::ui_widgets::Button, Node::default()))
        .id();
    world
        .resource_mut::<InputFocus>()
        .set(foreign_button, FocusCause::Navigated);
    keys(&mut world);
    assert!(world.resource::<PlayerState>().requests.is_empty());
    let window = world.spawn(Window::default()).id();
    world
        .resource_mut::<InputFocus>()
        .set(window, FocusCause::Navigated);
    keys(&mut world);
    assert_eq!(
        world.resource::<PlayerState>().requests,
        vec![Action::Open(Tab::Explore)]
    );
    world.resource_mut::<PlayerState>().requests.clear();
    let player_button = world
        .spawn((PlayerControl, bevy::ui_widgets::Button, Node::default()))
        .id();
    world
        .resource_mut::<InputFocus>()
        .set(player_button, FocusCause::Navigated);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyM);
    keys(&mut world);
    assert_eq!(
        world.resource::<PlayerState>().requests,
        vec![Action::Open(Tab::Explore)]
    );
}
